//! Proposition de modification d'un gabarit : ajout des mentions manquantes.
//!
//! Les blocs ajoutés viennent du fichier des mentions (relu par un humain),
//! jamais d'une source non vérifiée. Le gabarit proposé est réanalysé avant
//! d'être montré : il ne doit contenir aucune construction interdite.

use std::collections::BTreeSet;

use crate::gabarit::{Analyse, analyser};
use crate::rapport::{EtatMention, RapportGabarit, existe_dans_dokos};
use crate::regles::Regles;

pub const DEBUT_AJOUT: &str = "<!-- dokos : mentions ajoutées, à relire et compléter avant usage -->";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposition {
    pub gabarit: String,
    /// Libellés des mentions ajoutées.
    pub ajouts: Vec<String>,
    /// Champs affichés par le gabarit proposé qui n'existent pas dans Dokos.
    pub champs_a_creer: Vec<String>,
    /// Analyse du gabarit proposé.
    pub analyse: Analyse,
}

/// `None` si aucune mention n'est à ajouter.
pub fn proposer(
    gabarit: &str,
    rapport: &RapportGabarit,
    regles: &Regles,
    champs_dokos: &BTreeSet<String>,
) -> Option<Proposition> {
    let manquantes: Vec<_> = rapport
        .mentions
        .iter()
        .filter(|r| r.etat == EtatMention::Manquante)
        .filter_map(|r| regles.mentions.iter().find(|m| m.id == r.id))
        .collect();
    if manquantes.is_empty() {
        return None;
    }

    let blocs: Vec<&str> = manquantes.iter().map(|m| m.proposition.as_str()).collect();
    let nouveau = inserer_blocs(gabarit, &blocs);
    let analyse = analyser(&nouveau);
    let champs_a_creer = analyse.champs.iter().filter(|c| !existe_dans_dokos(c, champs_dokos)).cloned().collect();
    Some(Proposition {
        gabarit: nouveau,
        ajouts: manquantes.iter().map(|m| m.libelle.clone()).collect(),
        champs_a_creer,
        analyse,
    })
}

/// Ajoute les blocs, encadrés et commentés, avant `</body>` ou à la fin.
pub fn inserer_blocs(gabarit: &str, blocs: &[&str]) -> String {
    let mut bloc = format!("\n{DEBUT_AJOUT}\n<div class=\"mentions-ajoutees\">\n");
    for contenu in blocs {
        bloc.push_str(contenu.trim());
        bloc.push('\n');
    }
    bloc.push_str("</div>\n");
    match position_fin_body(gabarit) {
        Some(position) => format!("{}{bloc}{}", &gabarit[..position], &gabarit[position..]),
        None => format!("{}{bloc}", gabarit.trim_end_matches('\n')),
    }
}

/// Position du dernier `</body>`, sans tenir compte de la casse.
fn position_fin_body(gabarit: &str) -> Option<usize> {
    gabarit.to_ascii_lowercase().rfind("</body>")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LigneDiff {
    Commune(String),
    Ajoutee(String),
    Retiree(String),
}

/// Différence ligne à ligne (plus longue sous-suite commune).
pub fn difference(avant: &str, apres: &str) -> Vec<LigneDiff> {
    let a: Vec<&str> = avant.lines().collect();
    let b: Vec<&str> = apres.lines().collect();
    let (n, m) = (a.len(), b.len());
    // longueurs[i][j] = longueur de la plus longue sous-suite commune de a[i..] et b[j..].
    let mut longueurs = vec![0_u32; (n + 1) * (m + 1)];
    let indice = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            longueurs[indice(i, j)] = if a[i] == b[j] {
                longueurs[indice(i + 1, j + 1)] + 1
            } else {
                longueurs[indice(i + 1, j)].max(longueurs[indice(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut lignes = Vec::with_capacity(n.max(m));
    while i < n && j < m {
        if a[i] == b[j] {
            lignes.push(LigneDiff::Commune(a[i].to_owned()));
            i += 1;
            j += 1;
        } else if longueurs[indice(i + 1, j)] >= longueurs[indice(i, j + 1)] {
            lignes.push(LigneDiff::Retiree(a[i].to_owned()));
            i += 1;
        } else {
            lignes.push(LigneDiff::Ajoutee(b[j].to_owned()));
            j += 1;
        }
    }
    lignes.extend(a[i..].iter().map(|l| LigneDiff::Retiree((*l).to_owned())));
    lignes.extend(b[j..].iter().map(|l| LigneDiff::Ajoutee((*l).to_owned())));
    lignes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rapport::tests::{CDI_COMPLET, champs_dokos};
    use crate::rapport::verifier_gabarit;
    use crate::regles::Contexte;

    #[test]
    fn ajout_des_mentions_cdd() {
        let regles = Regles::par_defaut();
        let gabarit = format!("<html><body>{CDI_COMPLET}</BODY></html>");
        let contexte = Contexte { cdi: false, cdd: true, temps_partiel: false };
        let champs = champs_dokos();
        let rapport = verifier_gabarit(&regles, &analyser(&gabarit), &contexte, &champs);
        let proposition = proposer(&gabarit, &rapport, &regles, &champs).expect("proposition");

        assert_eq!(proposition.ajouts, ["Motif précis du recours au CDD", "Date du terme du contrat"]);
        assert!(proposition.gabarit.ends_with("</div>\n</BODY></html>"), "{}", proposition.gabarit);
        assert_eq!(proposition.champs_a_creer, ["motif_cdd"]);
        assert!(proposition.analyse.interdits.is_empty());

        // Une fois le champ créé dans Dokos, le gabarit proposé est conforme.
        let mut champs_apres = champs.clone();
        champs_apres.insert("motif_cdd".into());
        let apres = verifier_gabarit(&regles, &proposition.analyse, &contexte, &champs_apres);
        assert!(apres.conforme(), "{:?}", apres.non_presentes().collect::<Vec<_>>());
    }

    #[test]
    fn rien_a_proposer() {
        let regles = Regles::par_defaut();
        let contexte = Contexte { cdi: true, cdd: false, temps_partiel: false };
        let rapport = verifier_gabarit(&regles, &analyser(CDI_COMPLET), &contexte, &champs_dokos());
        assert!(proposer(CDI_COMPLET, &rapport, &regles, &champs_dokos()).is_none());
    }

    #[test]
    fn mentions_temps_partiel_et_texte_a_completer() {
        let regles = Regles::par_defaut();
        let contexte = Contexte { cdi: true, cdd: false, temps_partiel: true };
        let rapport = verifier_gabarit(&regles, &analyser(CDI_COMPLET), &contexte, &champs_dokos());
        let proposition = proposer(CDI_COMPLET, &rapport, &regles, &champs_dokos()).expect("proposition");
        assert_eq!(proposition.ajouts.len(), 4);
        // Les propositions contiennent des « [à compléter] » : le résultat n'est pas encore conforme.
        let apres = verifier_gabarit(&regles, &proposition.analyse, &contexte, &champs_dokos());
        assert!(apres.non_presentes().next().is_none());
        assert!(apres.a_completer >= 3);
        assert!(!apres.conforme());
    }

    #[test]
    fn chaque_proposition_satisfait_sa_mention() {
        let regles = Regles::par_defaut();
        let tout = Contexte { cdi: true, cdd: true, temps_partiel: true };
        let champs = champs_dokos();
        for mention in &regles.mentions {
            let mut seule = regles.clone();
            seule.mentions.retain(|m| m.id == mention.id);
            let rapport = verifier_gabarit(&seule, &analyser(&mention.proposition), &tout, &champs);
            let etat = &rapport.mentions[0].etat;
            assert!(
                *etat == EtatMention::Presente || matches!(etat, EtatMention::ChampsAbsentsDeDokos(_)),
                "la proposition de « {} » ne satisfait pas la mention : {etat:?}",
                mention.id
            );
            assert!(rapport.interdits.is_empty(), "{}", mention.id);
        }
    }

    #[test]
    fn differences() {
        let diff = difference("a\nb\nc", "a\nx\nc\nd");
        assert_eq!(
            diff,
            [
                LigneDiff::Commune("a".into()),
                LigneDiff::Retiree("b".into()),
                LigneDiff::Ajoutee("x".into()),
                LigneDiff::Commune("c".into()),
                LigneDiff::Ajoutee("d".into()),
            ]
        );
        assert!(difference("", "").is_empty());
    }
}
