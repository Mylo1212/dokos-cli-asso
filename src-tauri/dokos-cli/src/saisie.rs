//! Analyse des arguments saisis dans le terminal.

use dokos_core::date::{Date, DateHeure};
use dokos_core::formulaire::Question;

/// Ligne demandée par l'utilisateur, avant résolution de l'article dans Dokos.
#[derive(Debug, Clone, PartialEq)]
pub struct LigneSaisie {
    /// Texte recherché : code ou libellé de l'article.
    pub article: String,
    pub quantite: f64,
    pub livraison: Option<Date>,
}

/// `ARTICLE=QUANTITÉ` ou `ARTICLE=QUANTITÉ@DATE`, par exemple `repas=25@12/11/2026`.
/// La quantité accepte la virgule décimale.
pub fn ligne(texte: &str) -> Result<LigneSaisie, String> {
    let format = "format attendu : ARTICLE=QUANTITÉ ou ARTICLE=QUANTITÉ@DATE";
    let (article, reste) = texte.rsplit_once('=').ok_or_else(|| format!("« {texte} » : {format}"))?;
    let article = article.trim();
    if article.is_empty() {
        return Err(format!("« {texte} » : article manquant"));
    }
    let (quantite, livraison) = match reste.split_once('@') {
        Some((q, d)) => (q, Some(date(d)?)),
        None => (reste, None),
    };
    let quantite =
        nombre(quantite).ok_or_else(|| format!("« {texte} » : quantité « {} » illisible", quantite.trim()))?;
    Ok(LigneSaisie { article: article.to_owned(), quantite, livraison })
}

pub fn date(texte: &str) -> Result<Date, String> {
    texte.parse().map_err(|e: dokos_core::date::ErreurDate| e.to_string())
}

pub fn date_heure(texte: &str) -> Result<DateHeure, String> {
    texte.parse().map_err(|e: dokos_core::date::ErreurDate| e.to_string())
}

/// Nombre avec point ou virgule décimale.
pub fn nombre(texte: &str) -> Option<f64> {
    texte.trim().replace(',', ".").parse().ok().filter(|n: &f64| n.is_finite())
}

pub fn heures(texte: &str) -> Result<f64, String> {
    nombre(texte).ok_or_else(|| format!("« {texte} » : nombre d'heures illisible"))
}

/// `6/6/6/6/0/0/0` : heures du lundi au dimanche.
pub fn repartition(texte: &str) -> Result<[f64; 7], String> {
    let valeurs: Vec<f64> = texte
        .split('/')
        .map(|v| nombre(v).ok_or_else(|| format!("« {v} » : nombre d'heures illisible")))
        .collect::<Result<_, _>>()?;
    valeurs.try_into().map_err(|v: Vec<f64>| {
        format!("{} valeurs au lieu de 7 (format : lundi/mardi/…/dimanche, par exemple 6/6/6/6/0/0/0)", v.len())
    })
}

/// `champ=valeur`.
pub fn champ(texte: &str) -> Result<(String, String), String> {
    let (nom, valeur) = texte.split_once('=').ok_or_else(|| format!("« {texte} » : format attendu champ=valeur"))?;
    let nom = nom.trim();
    if nom.is_empty() {
        return Err(format!("« {texte} » : nom de champ manquant"));
    }
    Ok((nom.to_owned(), valeur.trim().to_owned()))
}

/// `age!` (obligatoire) ou `allergies` (facultative).
pub fn question(texte: &str) -> Result<Question, String> {
    let texte = texte.trim();
    let (champ, obligatoire) = match texte.strip_suffix('!') {
        Some(champ) => (champ.trim(), true),
        None => (texte, false),
    };
    if champ.is_empty() {
        return Err("question vide".to_owned());
    }
    Ok(Question { champ: champ.to_owned(), obligatoire })
}

/// `25` plutôt que `25.0`, `2.5` reste `2.5`.
pub fn quantite(q: f64) -> String {
    if q.fract() == 0.0 && q.abs() < 1e15 { format!("{q:.0}") } else { q.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lignes_valides() {
        assert_eq!(ligne("repas=25"), Ok(LigneSaisie { article: "repas".into(), quantite: 25.0, livraison: None }));
        assert_eq!(
            ligne(" grande salle = 1 @ 13/11/2026"),
            Ok(LigneSaisie {
                article: "grande salle".into(),
                quantite: 1.0,
                livraison: Some(date("2026-11-13").expect("date"))
            })
        );
        assert_eq!(ligne("café=2,5").map(|l| l.quantite), Ok(2.5));
        // Le dernier « = » sépare la quantité : l'article peut en contenir un.
        assert_eq!(ligne("formule A=B=3").map(|l| l.article), Ok("formule A=B".into()));
    }

    #[test]
    fn lignes_invalides() {
        for texte in ["repas", "=3", "repas=", "repas=beaucoup", "repas=3@demain", "repas=3@2026-02-30", "repas=inf"] {
            assert!(ligne(texte).is_err(), "{texte}");
        }
    }

    #[test]
    fn repartitions() {
        assert_eq!(repartition("6/6/6/6/0/0/0"), Ok([6.0, 6.0, 6.0, 6.0, 0.0, 0.0, 0.0]));
        assert_eq!(repartition("3,5/0/0/0/0/0/0").map(|r| r[0]), Ok(3.5));
        assert!(repartition("6/6/6").is_err());
        assert!(repartition("6/x/6/6/0/0/0").is_err());
    }

    #[test]
    fn champs_et_questions() {
        assert_eq!(champ("motif_cdd = Surcroît d'activité"), Ok(("motif_cdd".into(), "Surcroît d'activité".into())));
        assert!(champ("=x").is_err());
        assert_eq!(question("age!"), Ok(Question { champ: "age".into(), obligatoire: true }));
        assert_eq!(question("allergies"), Ok(Question { champ: "allergies".into(), obligatoire: false }));
        assert!(question("!").is_err());
    }

    #[test]
    fn affichage_quantite() {
        assert_eq!(quantite(25.0), "25");
        assert_eq!(quantite(2.5), "2.5");
    }
}
