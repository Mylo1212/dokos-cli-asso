//! Vérification des données d'un contrat, sans IA ni réseau.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::rapport::existe_dans_dokos;
use crate::regles::{Categorie, Regles};

pub const JOURS: [(&str, &str); 7] = [
    ("monday", "lundi"),
    ("tuesday", "mardi"),
    ("wednesday", "mercredi"),
    ("thursday", "jeudi"),
    ("friday", "vendredi"),
    ("saturday", "samedi"),
    ("sunday", "dimanche"),
];
/// Durée hebdomadaire maximale absolue.
pub const HEURES_HEBDO_MAX: f64 = 48.0;
/// Durée quotidienne maximale, sauf dérogation.
pub const HEURES_JOUR_MAX: f64 = 10.0;
const TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gravite {
    /// Bloque la création du contrat.
    Erreur,
    Avertissement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probleme {
    pub gravite: Gravite,
    pub message: String,
    pub reference: Option<String>,
}

impl fmt::Display for Probleme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let gravite = match self.gravite {
            Gravite::Erreur => "Erreur",
            Gravite::Avertissement => "Attention",
        };
        write!(f, "{gravite} : {}", self.message)?;
        if let Some(reference) = &self.reference {
            write!(f, " ({reference})")?;
        }
        Ok(())
    }
}

fn probleme(gravite: Gravite, message: impl Into<String>, reference: Option<&str>) -> Probleme {
    Probleme { gravite, message: message.into(), reference: reference.map(str::to_owned) }
}

/// Vérifie un contrat.
///
/// - `valeurs` : champ Dokos → valeur texte (vide si non renseignée), dates `AAAA-MM-JJ`.
/// - `champs_dokos` : champs d'`Employment Contract` dans Dokos.
/// - `entree_employe` : date d'entrée sur la fiche employé, si connue.
/// - `champs_gabarit` : champs affichés par le format d'impression choisi, si connu.
pub fn verifier_donnees(
    regles: &Regles,
    valeurs: &BTreeMap<String, String>,
    champs_dokos: &BTreeSet<String>,
    entree_employe: Option<&str>,
    champs_gabarit: Option<&BTreeSet<String>>,
) -> Vec<Probleme> {
    use Gravite::{Avertissement, Erreur};

    let valeur = |champ: &str| valeurs.get(champ).map(|v| v.trim()).unwrap_or_default();
    let nombre = |champ: &str| valeur(champ).replace(',', ".").parse::<f64>().ok();
    let mut problemes = Vec::new();

    let type_contrat = valeur("contract_type");
    let entree = valeur("date_of_joining");
    let fin = valeur("relieving_date");
    let heures = nombre("weekly_working_hours").filter(|h| *h > 0.0);
    let categorie = regles.categorie(type_contrat);
    let contexte = regles.contexte_contrat(type_contrat, heures);

    if categorie == Categorie::Autre {
        problemes.push(probleme(
            Avertissement,
            format!(
                "type « {type_contrat} » absent des catégories CDI/CDD du fichier des mentions : seules les mentions communes sont vérifiées"
            ),
            None,
        ));
    }
    if !fin.is_empty() && !entree.is_empty() && fin < entree {
        problemes.push(probleme(Erreur, "la date de fin est avant la date d'entrée", None));
    }
    match categorie {
        Categorie::Cdd if fin.is_empty() => {
            problemes.push(probleme(Erreur, "un CDD doit avoir une date de fin", Some("C. trav. art. L1242-12")));
        }
        Categorie::Cdd if !fin.is_empty() && fin == entree => {
            problemes.push(probleme(Avertissement, "CDD d'un seul jour : vérifiez les dates", None));
        }
        Categorie::Cdi if !fin.is_empty() => {
            problemes.push(probleme(Avertissement, "un CDI n'a normalement pas de date de fin", None));
        }
        _ => {}
    }

    if let Some(h) = heures
        && h > HEURES_HEBDO_MAX
    {
        problemes.push(probleme(
            Erreur,
            format!("{h} h par semaine : au-delà de la durée maximale de {HEURES_HEBDO_MAX} h"),
            Some("C. trav. art. L3121-20"),
        ));
    }
    let repartition: Vec<(&str, f64)> =
        JOURS.iter().filter_map(|(champ, jour)| nombre(champ).filter(|h| *h > 0.0).map(|h| (*jour, h))).collect();
    for (jour, h) in &repartition {
        if *h > HEURES_JOUR_MAX {
            problemes.push(probleme(
                Avertissement,
                format!("{h} h le {jour} : au-delà de {HEURES_JOUR_MAX} h par jour, une dérogation est nécessaire"),
                Some("C. trav. art. L3121-18"),
            ));
        }
    }
    if contexte.temps_partiel {
        if repartition.is_empty() {
            problemes.push(probleme(
                Erreur,
                "temps partiel : la répartition des heures entre les jours de la semaine est obligatoire",
                Some("C. trav. art. L3123-6"),
            ));
        } else if let Some(h) = heures {
            let total: f64 = repartition.iter().map(|(_, h)| h).sum();
            if (total - h).abs() > TOLERANCE {
                problemes.push(probleme(
                    Erreur,
                    format!("la répartition totalise {total} h pour {h} h hebdomadaires"),
                    Some("C. trav. art. L3123-6"),
                ));
            }
        }
    }

    if let Some(entree_employe) = entree_employe.map(str::trim).filter(|d| !d.is_empty())
        && !entree.is_empty()
        && entree_employe != entree
    {
        problemes.push(probleme(
            Avertissement,
            format!("date d'entrée du contrat ({entree}) différente de celle de la fiche employé ({entree_employe})"),
            None,
        ));
    }

    let mut deja_signales = BTreeSet::new();
    for mention in regles.applicables(&contexte).filter(|m| m.donnee_obligatoire) {
        if mention.champs.iter().any(|c| !valeur(c).is_empty()) {
            continue;
        }
        let absents: Vec<&String> = mention.champs.iter().filter(|c| !existe_dans_dokos(c, champs_dokos)).collect();
        let message = if absents.len() == mention.champs.len() {
            let noms: Vec<&str> = absents.iter().map(|c| c.as_str()).collect();
            format!("{} : champ « {} » à créer dans Dokos puis à remplir", mention.libelle, noms.join(" » ou « "))
        } else {
            format!("{} : non renseigné(e)", mention.libelle)
        };
        problemes.push(probleme(Erreur, message, Some(&mention.reference)));
        deja_signales.extend(mention.champs.iter().cloned());
    }

    if let Some(champs_gabarit) = champs_gabarit {
        let sans_objet = |champ: &str| {
            deja_signales.contains(champ)
                || JOURS.iter().any(|(j, _)| *j == champ)
                || (champ == "relieving_date" && categorie != Categorie::Cdd)
        };
        for champ in champs_gabarit {
            if valeurs.contains_key(champ) && valeur(champ).is_empty() && !sans_objet(champ) {
                problemes.push(probleme(
                    Avertissement,
                    format!("le format d'impression affiche « {champ} », vide pour ce contrat"),
                    None,
                ));
            }
        }
    }

    problemes.sort_by_key(|p| p.gravite);
    problemes
}

pub fn a_des_erreurs(problemes: &[Probleme]) -> bool {
    problemes.iter().any(|p| p.gravite == Gravite::Erreur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rapport::tests::champs_dokos;

    fn valeurs(paires: &[(&str, &str)]) -> BTreeMap<String, String> {
        paires.iter().map(|(c, v)| ((*c).to_owned(), (*v).to_owned())).collect()
    }

    /// CDD à temps partiel (24 h du lundi au jeudi), complet.
    fn cdd_complet() -> BTreeMap<String, String> {
        valeurs(&[
            ("contract_type", "CDD"),
            ("designation", "Animatrice"),
            ("date_of_joining", "2026-12-01"),
            ("relieving_date", "2027-03-31"),
            ("weekly_working_hours", "24"),
            ("monday", "6"),
            ("tuesday", "6"),
            ("wednesday", "6"),
            ("thursday", "6"),
            ("friday", ""),
            ("remuneration_brute", "1500"),
            ("lieu_travail", "Les Amanins, Crest"),
            ("motif_cdd", "Accroissement temporaire d'activité"),
        ])
    }

    fn verifier(valeurs: &BTreeMap<String, String>) -> Vec<Probleme> {
        let mut champs = champs_dokos();
        champs.insert("motif_cdd".into());
        verifier_donnees(&Regles::par_defaut(), valeurs, &champs, Some("2026-12-01"), None)
    }

    fn messages(problemes: &[Probleme]) -> Vec<String> {
        problemes.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn contrat_complet() {
        let problemes = verifier(&cdd_complet());
        assert!(problemes.is_empty(), "{:?}", messages(&problemes));
    }

    #[test]
    fn cdd_sans_fin_ni_motif() {
        let mut v = cdd_complet();
        v.insert("relieving_date".into(), String::new());
        v.insert("motif_cdd".into(), String::new());
        let problemes = verifier(&v);
        assert!(a_des_erreurs(&problemes));
        let texte = messages(&problemes).join("\n");
        assert!(texte.contains("un CDD doit avoir une date de fin"), "{texte}");
        assert!(texte.contains("Motif précis du recours au CDD : non renseigné(e)"), "{texte}");
    }

    #[test]
    fn temps_partiel() {
        let mut v = cdd_complet();
        v.insert("monday".into(), "4".into());
        let texte = messages(&verifier(&v)).join("\n");
        assert!(texte.contains("la répartition totalise 22 h pour 24 h hebdomadaires"), "{texte}");

        for jour in ["monday", "tuesday", "wednesday", "thursday"] {
            v.insert(jour.into(), String::new());
        }
        let texte = messages(&verifier(&v)).join("\n");
        assert!(texte.contains("la répartition des heures entre les jours de la semaine est obligatoire"), "{texte}");
    }

    #[test]
    fn durees_maximales() {
        let mut v = cdd_complet();
        v.insert("weekly_working_hours".into(), "50".into());
        v.insert("monday".into(), "11".into());
        let problemes = verifier(&v);
        let texte = messages(&problemes).join("\n");
        assert!(texte.contains("au-delà de la durée maximale de 48 h"), "{texte}");
        assert!(texte.contains("11 h le lundi"), "{texte}");
        // Les erreurs passent avant les avertissements.
        assert_eq!(problemes[0].gravite, Gravite::Erreur);
    }

    #[test]
    fn champ_a_creer_dans_dokos() {
        let mut v = cdd_complet();
        v.remove("motif_cdd");
        let problemes = verifier_donnees(&Regles::par_defaut(), &v, &champs_dokos(), None, None);
        let texte = messages(&problemes).join("\n");
        assert!(texte.contains("champ « motif_cdd » à créer dans Dokos"), "{texte}");
    }

    #[test]
    fn incoherences_signalees() {
        let mut v = cdd_complet();
        v.insert("contract_type".into(), "CDI".into());
        v.insert("date_of_joining".into(), "2026-12-02".into());
        let texte = messages(&verifier(&v)).join("\n");
        assert!(texte.contains("un CDI n'a normalement pas de date de fin"), "{texte}");
        assert!(texte.contains("différente de celle de la fiche employé"), "{texte}");

        v.insert("relieving_date".into(), "2026-11-01".into());
        assert!(messages(&verifier(&v)).join("\n").contains("la date de fin est avant la date d'entrée"));

        v.insert("contract_type".into(), "Contrat d'engagement éducatif".into());
        assert!(messages(&verifier(&v)).join("\n").contains("seules les mentions communes"));
    }

    #[test]
    fn champs_du_gabarit_vides() {
        let mut v = cdd_complet();
        v.insert("periode_essai".into(), String::new());
        let gabarit: BTreeSet<String> =
            ["periode_essai", "monday", "motif_cdd"].into_iter().map(str::to_owned).collect();
        let mut champs = champs_dokos();
        champs.insert("motif_cdd".into());
        let problemes = verifier_donnees(&Regles::par_defaut(), &v, &champs, None, Some(&gabarit));
        assert_eq!(
            messages(&problemes),
            ["Attention : le format d'impression affiche « periode_essai », vide pour ce contrat"]
        );
    }
}
