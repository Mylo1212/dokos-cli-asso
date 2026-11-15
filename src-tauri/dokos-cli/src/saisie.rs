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
    /// Prix unitaire hors taxes imposé à la ligne ; `None` : prix de Dokos.
    pub prix: Option<f64>,
}

/// `ARTICLE=QUANTITÉ`, avec en plus `*PRIX` (prix unitaire HT) et/ou `@DATE` (livraison d'une
/// commande), dans cet ordre : `repas=25`, `repas=25*12,5`, `repas=25*12,5@12/11/2026`.
/// La quantité et le prix acceptent la virgule décimale.
pub fn ligne(texte: &str) -> Result<LigneSaisie, String> {
    let format = "format attendu : ARTICLE=QUANTITÉ, ARTICLE=QUANTITÉ*PRIX ou ARTICLE=QUANTITÉ*PRIX@DATE";
    let (article, reste) = texte.rsplit_once('=').ok_or_else(|| format!("« {texte} » : {format}"))?;
    let article = article.trim();
    if article.is_empty() {
        return Err(format!("« {texte} » : article manquant"));
    }
    let (reste, livraison) = match reste.split_once('@') {
        Some((r, d)) => (r, Some(date(d)?)),
        None => (reste, None),
    };
    let (quantite, prix) = match reste.split_once('*') {
        Some((q, p)) => {
            let prix = nombre(p)
                .filter(|p| (0.0..=dokos_core::vente::PRIX_MAX).contains(p))
                .ok_or_else(|| format!("« {texte} » : prix « {} » illisible", p.trim()))?;
            (q, Some(prix))
        }
        None => (reste, None),
    };
    let quantite =
        nombre(quantite).ok_or_else(|| format!("« {texte} » : quantité « {} » illisible", quantite.trim()))?;
    Ok(LigneSaisie { article: article.to_owned(), quantite, livraison, prix })
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
        assert_eq!(
            ligne("repas=25"),
            Ok(LigneSaisie { article: "repas".into(), quantite: 25.0, livraison: None, prix: None })
        );
        assert_eq!(
            ligne(" grande salle = 1 @ 13/11/2026"),
            Ok(LigneSaisie {
                article: "grande salle".into(),
                quantite: 1.0,
                livraison: Some(date("2026-11-13").expect("date")),
                prix: None
            })
        );
        let avec_prix = ligne("repas=25*12,5@13/11/2026").expect("prix");
        assert_eq!((avec_prix.quantite, avec_prix.prix), (25.0, Some(12.5)));
        assert!(avec_prix.livraison.is_some());
        assert_eq!(ligne("repas=2*0").map(|l| l.prix), Ok(Some(0.0)));
        assert_eq!(ligne("café=2,5").map(|l| l.quantite), Ok(2.5));
        // Le dernier « = » sépare la quantité : l'article peut en contenir un.
        assert_eq!(ligne("formule A=B=3").map(|l| l.article), Ok("formule A=B".into()));
    }

    #[test]
    fn lignes_invalides() {
        for texte in [
            "repas",
            "=3",
            "repas=",
            "repas=beaucoup",
            "repas=3@demain",
            "repas=3@2026-02-30",
            "repas=inf",
            "repas=3*cher",
            "repas=3*-1",
            "repas=3*",
        ] {
            assert!(ligne(texte).is_err(), "{texte}");
        }
    }

    #[test]
    fn questions() {
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
