//! Liste des mentions obligatoires, lue depuis un fichier TOML relu par un humain.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

/// Fichier livré avec l'outil : brouillon, non validé.
pub const REGLES_PAR_DEFAUT: &str = include_str!("../mentions-contrats.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Portee {
    Tous,
    Cdi,
    Cdd,
    TempsPartiel,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Categories {
    #[serde(default)]
    pub cdi: Vec<String>,
    #[serde(default)]
    pub cdd: Vec<String>,
    #[serde(default = "duree_temps_plein")]
    pub duree_temps_plein: f64,
}

fn duree_temps_plein() -> f64 {
    35.0
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mention {
    pub id: String,
    pub libelle: String,
    pub reference: String,
    pub pour: Vec<Portee>,
    #[serde(default)]
    pub champs: Vec<String>,
    #[serde(default)]
    pub textes: Vec<String>,
    pub proposition: String,
    #[serde(default)]
    pub donnee_obligatoire: bool,
}

impl Mention {
    pub fn applicable(&self, contexte: &Contexte) -> bool {
        self.pour.iter().any(|portee| match portee {
            Portee::Tous => true,
            Portee::Cdi => contexte.cdi,
            Portee::Cdd => contexte.cdd,
            Portee::TempsPartiel => contexte.temps_partiel,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regles {
    pub version: String,
    /// Personne qui a relu la liste. Vide : liste non validée.
    #[serde(default)]
    pub valide_par: String,
    #[serde(default)]
    pub valide_le: String,
    pub categories: Categories,
    #[serde(rename = "mention", default)]
    pub mentions: Vec<Mention>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurRegles(pub String);

impl fmt::Display for ErreurRegles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fichier des mentions invalide : {}", self.0)
    }
}

impl std::error::Error for ErreurRegles {}

/// Catégorie d'un type de contrat Dokos, d'après `[categories]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Categorie {
    Cdi,
    Cdd,
    Autre,
}

/// Situation d'un contrat ou d'un gabarit : quelles mentions s'appliquent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Contexte {
    pub cdi: bool,
    pub cdd: bool,
    pub temps_partiel: bool,
}

impl Regles {
    pub fn par_defaut() -> Self {
        #[allow(clippy::expect_used)] // Vérifié par le test `fichier_par_defaut_valide`.
        Self::depuis_toml(REGLES_PAR_DEFAUT).expect("fichier des mentions livré invalide")
    }

    pub fn depuis_toml(texte: &str) -> Result<Self, ErreurRegles> {
        let regles: Self = toml::from_str(texte).map_err(|e| ErreurRegles(e.to_string()))?;
        let mut ids = BTreeSet::new();
        for mention in &regles.mentions {
            if !ids.insert(mention.id.as_str()) {
                return Err(ErreurRegles(format!("mention « {} » en double", mention.id)));
            }
            if mention.champs.is_empty() && mention.textes.is_empty() {
                return Err(ErreurRegles(format!("mention « {} » sans champ ni texte", mention.id)));
            }
            if mention.pour.is_empty() {
                return Err(ErreurRegles(format!("mention « {} » sans portée (`pour`)", mention.id)));
            }
            if mention.proposition.trim().is_empty() {
                return Err(ErreurRegles(format!("mention « {} » sans proposition", mention.id)));
            }
            if mention.donnee_obligatoire && mention.champs.is_empty() {
                return Err(ErreurRegles(format!("mention « {} » : donnée obligatoire sans champ", mention.id)));
            }
        }
        let duree = regles.categories.duree_temps_plein;
        if !duree.is_finite() || duree <= 0.0 {
            return Err(ErreurRegles(format!("durée de temps plein invalide : {duree}")));
        }
        Ok(regles)
    }

    pub fn est_validee(&self) -> bool {
        !self.valide_par.trim().is_empty()
    }

    pub fn categorie(&self, type_contrat: &str) -> Categorie {
        let egal = |noms: &[String]| noms.iter().any(|n| n.trim().eq_ignore_ascii_case(type_contrat.trim()));
        if egal(&self.categories.cdd) {
            Categorie::Cdd
        } else if egal(&self.categories.cdi) {
            Categorie::Cdi
        } else {
            Categorie::Autre
        }
    }

    pub fn contexte_contrat(&self, type_contrat: &str, heures_hebdo: Option<f64>) -> Contexte {
        let categorie = self.categorie(type_contrat);
        Contexte {
            cdi: categorie == Categorie::Cdi,
            cdd: categorie == Categorie::Cdd,
            temps_partiel: heures_hebdo.is_some_and(|h| h > 0.0 && h < self.categories.duree_temps_plein),
        }
    }

    pub fn applicables<'a>(&'a self, contexte: &'a Contexte) -> impl Iterator<Item = &'a Mention> + 'a {
        self.mentions.iter().filter(move |m| m.applicable(contexte))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichier_par_defaut_valide() {
        let regles = Regles::depuis_toml(REGLES_PAR_DEFAUT).expect("fichier livré");
        assert!(!regles.est_validee(), "le fichier livré doit rester un brouillon non validé");
        assert!(regles.mentions.len() >= 15);
        assert_eq!(regles.categorie("cdd"), Categorie::Cdd);
        assert_eq!(regles.categorie("CDI"), Categorie::Cdi);
        assert_eq!(regles.categorie("Contrat d'engagement éducatif"), Categorie::Autre);
    }

    #[test]
    fn contextes() {
        let regles = Regles::par_defaut();
        let compter = |contexte: Contexte| regles.applicables(&contexte).count();
        let cdi = regles.contexte_contrat("CDI", Some(35.0));
        let cdd_partiel = regles.contexte_contrat("CDD", Some(24.0));
        assert!(cdi.cdi && !cdi.temps_partiel);
        assert!(cdd_partiel.cdd && cdd_partiel.temps_partiel);
        assert!(compter(cdd_partiel) > compter(cdi));
        assert!(!regles.contexte_contrat("CDI", None).temps_partiel);
    }

    #[test]
    fn fichiers_invalides() {
        let base = "version = \"1\"\n[categories]\ncdi = [\"CDI\"]\n";
        let mention =
            |corps: &str| format!("{base}[[mention]]\nid = \"a\"\nlibelle = \"A\"\nreference = \"r\"\n{corps}");
        for (corps, attendu) in [
            ("pour = [\"tous\"]\nproposition = \"p\"\n", "sans champ ni texte"),
            ("pour = []\ntextes = [\"x\"]\nproposition = \"p\"\n", "sans portée"),
            ("pour = [\"tous\"]\ntextes = [\"x\"]\nproposition = \" \"\n", "sans proposition"),
            ("pour = [\"tous\"]\ntextes = [\"x\"]\nproposition = \"p\"\ndonnee_obligatoire = true\n", "sans champ"),
            ("pour = [\"tous\"]\ntextes = [\"x\"]\nproposition = \"p\"\ninconnu = 1\n", "unknown field"),
            ("pour = [\"stagiaire\"]\ntextes = [\"x\"]\nproposition = \"p\"\n", "unknown variant"),
        ] {
            let erreur = Regles::depuis_toml(&mention(corps)).expect_err(attendu);
            assert!(erreur.0.contains(attendu), "{erreur} / {attendu}");
        }
        let double = format!(
            "{}{}",
            mention("pour = [\"tous\"]\ntextes = [\"x\"]\nproposition = \"p\"\n"),
            "[[mention]]\nid = \"a\"\nlibelle = \"B\"\nreference = \"r\"\npour = [\"tous\"]\ntextes = [\"y\"]\nproposition = \"p\"\n"
        );
        assert!(Regles::depuis_toml(&double).expect_err("double").0.contains("en double"));
    }
}
