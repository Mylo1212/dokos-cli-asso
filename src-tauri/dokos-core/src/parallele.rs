//! Requêtes simultanées, en nombre limité, pour les lectures indépendantes
//! (doublons de chaque ligne d'un import, documents candidats…).
//!
//! Des fils système suffisent : le transport est bloquant et le nombre de
//! requêtes en vol reste faible pour ne pas surcharger Dokos. Un appel fait
//! depuis un de ces fils reste séquentiel, pour ne jamais multiplier les
//! requêtes simultanées en imbriquant les appels.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

/// Requêtes simultanées au plus.
pub(crate) const FILS_MAX: usize = 6;

thread_local! {
    static DANS_UN_FIL: Cell<bool> = const { Cell::new(false) };
}

fn dans_un_fil() -> bool {
    DANS_UN_FIL.with(Cell::get)
}

fn marquer_fil() {
    DANS_UN_FIL.with(|d| d.set(true));
}

/// Applique `f` à chaque élément, sur au plus [`FILS_MAX`] fils si `parallele`
/// est vrai. Les résultats gardent l'ordre des éléments ; en cas d'erreur, on
/// renvoie la première dans cet ordre, comme le ferait une boucle, et les
/// éléments pas encore commencés ne le sont plus.
pub(crate) fn pour_chacun<E, R, Er, F>(parallele: bool, elements: &[E], f: F) -> Result<Vec<R>, Er>
where
    E: Sync,
    R: Send,
    Er: Send,
    F: Fn(&E) -> Result<R, Er> + Sync,
{
    if !parallele || elements.len() < 2 || dans_un_fil() {
        return elements.iter().map(f).collect();
    }
    let suivant = AtomicUsize::new(0);
    let arret = AtomicBool::new(false);
    let resultats: Mutex<Vec<Option<Result<R, Er>>>> = Mutex::new((0..elements.len()).map(|_| None).collect());
    thread::scope(|s| {
        for _ in 0..FILS_MAX.min(elements.len()) {
            s.spawn(|| {
                marquer_fil();
                while !arret.load(Ordering::Relaxed) {
                    let i = suivant.fetch_add(1, Ordering::Relaxed);
                    let Some(element) = elements.get(i) else { break };
                    let resultat = f(element);
                    if resultat.is_err() {
                        arret.store(true, Ordering::Relaxed);
                    }
                    if let Ok(mut r) = resultats.lock()
                        && let Some(case) = r.get_mut(i)
                    {
                        *case = Some(resultat);
                    }
                }
            });
        }
    });
    // Les indices sont pris dans l'ordre : tous ceux qui précèdent la première
    // erreur ont été traités, ceux qui manquent la suivent forcément.
    let resultats = resultats.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut sortie = Vec::with_capacity(elements.len());
    for resultat in resultats {
        match resultat {
            Some(Ok(r)) => sortie.push(r),
            Some(Err(e)) => return Err(e),
            None => break,
        }
    }
    Ok(sortie)
}

/// Calcule `a` et `b` en même temps si `parallele` est vrai.
pub(crate) fn joindre<A, B>(parallele: bool, a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B)
where
    A: Send,
    B: Send,
{
    if !parallele || dans_un_fil() {
        return (a(), b());
    }
    thread::scope(|s| {
        let fil = s.spawn(|| {
            marquer_fil();
            a()
        });
        let b = b();
        match fil.join() {
            Ok(a) => (a, b),
            Err(panique) => std::panic::resume_unwind(panique),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use super::*;

    #[test]
    fn ordre_garde() {
        let elements: Vec<u32> = (0..50).collect();
        let carres = pour_chacun(true, &elements, |&n| {
            thread::sleep(Duration::from_millis(u64::from(n % 3)));
            Ok::<_, ()>(n * n)
        })
        .expect("carrés");
        assert_eq!(carres, elements.iter().map(|n| n * n).collect::<Vec<_>>());
    }

    #[test]
    fn premiere_erreur_dans_l_ordre() {
        let elements: Vec<u32> = (0..40).collect();
        let resultat = pour_chacun(true, &elements, |&n| if n == 7 || n == 30 { Err(n) } else { Ok(n) });
        assert_eq!(resultat, Err(7));
    }

    #[test]
    fn simultaneite_bornee_et_pas_d_imbrication() {
        let en_cours = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let elements: Vec<u32> = (0..30).collect();
        pour_chacun(true, &elements, |_| {
            // Appel imbriqué : séquentiel, il n'ajoute aucune requête simultanée.
            pour_chacun(true, &[1, 2, 3], |_| {
                let n = en_cours.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(n, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(2));
                en_cours.fetch_sub(1, Ordering::SeqCst);
                Ok::<_, ()>(())
            })
        })
        .expect("appels");
        assert!(maximum.load(Ordering::SeqCst) <= FILS_MAX, "{maximum:?}");
    }

    #[test]
    fn sequentiel_si_demande() {
        let ordre = Mutex::new(Vec::new());
        pour_chacun(false, &[1, 2, 3], |&n| {
            ordre.lock().expect("verrou").push(n);
            Ok::<_, ()>(())
        })
        .expect("appels");
        assert_eq!(*ordre.lock().expect("verrou"), [1, 2, 3]);
        assert_eq!(joindre(true, || 1, || 2), (1, 2));
    }
}
