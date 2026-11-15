//! Contrats de travail (DocType `Employment Contract` de l'appli RH de Dokos).
//!
//! Ce DocType n'a pas d'état brouillon : un contrat créé est tout de suite
//! actif dans Dokos. Le contrôle de doublon et la vérification de conformité
//! (crate `conformite`) passent donc avant la création.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::client::{Dokos, Filtre, Liste, lister_noms};
use crate::date::Date;
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::meta::{Champ, Meta};
use crate::transport::Transport;
use crate::types::{texte_ou_vide, valeurs_texte};

pub const DOCTYPE_CONTRAT: &str = "Employment Contract";
pub const DOCTYPE_TYPE_CONTRAT: &str = "Employment Contract Type";
/// Champs de répartition des heures, du lundi au dimanche.
pub const JOURS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
pub const NOMS_JOURS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
/// Champs gérés par [`NouveauContrat`] ou par Dokos : pas de saisie libre.
const CHAMPS_GERES: &[&str] = &[
    "employee",
    "employee_name",
    "contract_type",
    "designation",
    "date_of_joining",
    "relieving_date",
    "weekly_working_hours",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "company",
    "naming_series",
    "leave_types",
];
const HEURES_HEBDO_MAX: f64 = 168.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurContrat {
    EmployeVide,
    TypeVide,
    PosteVide,
    FinAvantEntree,
    HeuresInvalides(f64),
    HeuresJourInvalides { jour: &'static str, heures: f64 },
    ChampInconnu(String),
    ChampGere(String),
}

impl fmt::Display for ErreurContrat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmployeVide => write!(f, "employé manquant"),
            Self::TypeVide => write!(f, "type de contrat manquant"),
            Self::PosteVide => write!(f, "poste manquant"),
            Self::FinAvantEntree => write!(f, "la date de fin est avant la date d'entrée"),
            Self::HeuresInvalides(h) => write!(f, "heures hebdomadaires invalides : {h}"),
            Self::HeuresJourInvalides { jour, heures } => write!(f, "heures invalides le {jour} : {heures}"),
            Self::ChampInconnu(c) => {
                write!(f, "« {c} » n'existe pas dans {DOCTYPE_CONTRAT} : créez d'abord le champ dans Dokos")
            }
            Self::ChampGere(c) => write!(f, "« {c} » se renseigne avec les champs dédiés, pas en champ libre"),
        }
    }
}

impl std::error::Error for ErreurContrat {}

#[derive(Debug, Clone, PartialEq)]
pub struct NouveauContrat {
    employe: String,
    type_contrat: String,
    poste: String,
    entree: Date,
    fin: Option<Date>,
    heures_hebdo: Option<f64>,
    repartition: Option<[f64; 7]>,
    societe: Option<String>,
    supplementaires: BTreeMap<String, String>,
}

impl NouveauContrat {
    /// `employe`, `type_contrat` et `poste` sont des identifiants Dokos.
    pub fn new(
        employe: &str,
        type_contrat: &str,
        poste: &str,
        entree: Date,
        fin: Option<Date>,
    ) -> Result<Self, ErreurContrat> {
        let (employe, type_contrat, poste) = (employe.trim(), type_contrat.trim(), poste.trim());
        if employe.is_empty() {
            return Err(ErreurContrat::EmployeVide);
        }
        if type_contrat.is_empty() {
            return Err(ErreurContrat::TypeVide);
        }
        if poste.is_empty() {
            return Err(ErreurContrat::PosteVide);
        }
        if fin.is_some_and(|fin| fin < entree) {
            return Err(ErreurContrat::FinAvantEntree);
        }
        Ok(Self {
            employe: employe.to_owned(),
            type_contrat: type_contrat.to_owned(),
            poste: poste.to_owned(),
            entree,
            fin,
            heures_hebdo: None,
            repartition: None,
            societe: None,
            supplementaires: BTreeMap::new(),
        })
    }

    /// Durée hebdomadaire et répartition du lundi au dimanche. Les règles
    /// légales (48 h, temps partiel…) sont vérifiées par `conformite`.
    pub fn avec_heures(mut self, hebdo: Option<f64>, repartition: Option<[f64; 7]>) -> Result<Self, ErreurContrat> {
        if let Some(h) = hebdo
            && (!h.is_finite() || h <= 0.0 || h > HEURES_HEBDO_MAX)
        {
            return Err(ErreurContrat::HeuresInvalides(h));
        }
        if let Some(jours) = repartition {
            for (jour, heures) in NOMS_JOURS.iter().zip(jours) {
                if !heures.is_finite() || !(0.0..=24.0).contains(&heures) {
                    return Err(ErreurContrat::HeuresJourInvalides { jour, heures });
                }
            }
        }
        self.heures_hebdo = hebdo;
        self.repartition = repartition.filter(|jours| jours.iter().any(|h| *h > 0.0));
        Ok(self)
    }

    pub fn avec_societe(mut self, societe: Option<&str>) -> Self {
        self.societe = societe.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        self
    }

    /// Champs personnalisés (rémunération, période d'essai, motif…), vérifiés
    /// contre la structure d'`Employment Contract` lue dans Dokos. Les valeurs
    /// vides sont ignorées.
    pub fn avec_champs(mut self, meta: &Meta, champs: BTreeMap<String, String>) -> Result<Self, ErreurContrat> {
        for (nom, valeur) in champs {
            let nom = nom.trim().to_owned();
            if CHAMPS_GERES.contains(&nom.as_str()) {
                return Err(ErreurContrat::ChampGere(nom));
            }
            if !meta.champ(&nom).is_some_and(|c| c.saisissable()) {
                return Err(ErreurContrat::ChampInconnu(nom));
            }
            let valeur = valeur.trim();
            if !valeur.is_empty() {
                self.supplementaires.insert(nom, valeur.to_owned());
            }
        }
        Ok(self)
    }

    pub fn type_contrat(&self) -> &str {
        &self.type_contrat
    }

    pub fn employe(&self) -> &str {
        &self.employe
    }

    /// Toutes les valeurs du contrat, par nom de champ Dokos, pour la vérification.
    pub fn valeurs(&self) -> BTreeMap<String, String> {
        let mut valeurs = BTreeMap::new();
        valeurs.insert("employee".to_owned(), self.employe.clone());
        valeurs.insert("contract_type".to_owned(), self.type_contrat.clone());
        valeurs.insert("designation".to_owned(), self.poste.clone());
        valeurs.insert("date_of_joining".to_owned(), self.entree.to_string());
        valeurs.insert("relieving_date".to_owned(), self.fin.map(|d| d.to_string()).unwrap_or_default());
        valeurs.insert("weekly_working_hours".to_owned(), self.heures_hebdo.map(|h| h.to_string()).unwrap_or_default());
        for (i, jour) in JOURS.iter().enumerate() {
            let heures = self.repartition.map(|r| r[i]).filter(|h| *h > 0.0);
            valeurs.insert((*jour).to_owned(), heures.map(|h| h.to_string()).unwrap_or_default());
        }
        valeurs.insert("company".to_owned(), self.societe.clone().unwrap_or_default());
        valeurs.extend(self.supplementaires.clone());
        valeurs
    }

    pub fn corps(&self) -> Value {
        let mut corps = json!({
            "employee": self.employe,
            "contract_type": self.type_contrat,
            "designation": self.poste,
            "date_of_joining": self.entree.to_string(),
        });
        if let Some(fin) = self.fin {
            corps["relieving_date"] = json!(fin.to_string());
        }
        if let Some(h) = self.heures_hebdo {
            corps["weekly_working_hours"] = json!(h);
        }
        if let Some(jours) = self.repartition {
            for (jour, heures) in JOURS.iter().zip(jours) {
                corps[*jour] = json!(heures);
            }
        }
        if let Some(societe) = &self.societe {
            corps["company"] = json!(societe);
        }
        for (nom, valeur) in &self.supplementaires {
            corps[nom.as_str()] = json!(valeur);
        }
        corps
    }
}

/// Champs d'`Employment Contract` à saisir en plus des champs gérés :
/// en pratique, les champs personnalisés (rémunération, motif…).
pub fn champs_libres(meta: &Meta) -> Vec<Champ> {
    meta.champs.iter().filter(|c| c.saisissable() && !CHAMPS_GERES.contains(&c.fieldname.as_str())).cloned().collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ContratExistant {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "employee", default, deserialize_with = "texte_ou_vide")]
    pub employe: String,
    #[serde(rename = "employee_name", default, deserialize_with = "texte_ou_vide")]
    pub nom_employe: String,
    #[serde(rename = "contract_type", default, deserialize_with = "texte_ou_vide")]
    pub type_contrat: String,
    #[serde(rename = "date_of_joining", default, deserialize_with = "texte_ou_vide")]
    pub entree: String,
}

const CHAMPS_CONTRAT: &[&str] = &["name", "employee", "employee_name", "contract_type", "date_of_joining"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationContrat {
    Cree(ContratExistant),
    /// Rien n'a été écrit : même employé, même type, même date d'entrée.
    DejaExistant(ContratExistant),
}

pub fn chercher_contrat_identique<T: Transport>(
    dokos: &Dokos<T>,
    contrat: &NouveauContrat,
) -> Result<Option<ContratExistant>, DokosError> {
    let liste = Liste::new(CHAMPS_CONTRAT)
        .filtre(Filtre::egal("employee", contrat.employe.as_str()))
        .filtre(Filtre::egal("contract_type", contrat.type_contrat.as_str()))
        .filtre(Filtre::egal("date_of_joining", contrat.entree.to_string()))
        .limite(1);
    Ok(dokos.lister::<ContratExistant>(DOCTYPE_CONTRAT, &liste)?.into_iter().next())
}

pub fn creer_contrat<T: Transport>(
    dokos: &Dokos<T>,
    contrat: &NouveauContrat,
    si_doublon: SiDoublon,
) -> Result<CreationContrat, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = chercher_contrat_identique(dokos, contrat)?
    {
        return Ok(CreationContrat::DejaExistant(existant));
    }
    Ok(CreationContrat::Cree(dokos.creer(DOCTYPE_CONTRAT, contrat.corps())?))
}

/// Contrat existant, toutes valeurs converties en texte (vide si absente).
pub fn lire_valeurs_contrat<T: Transport>(dokos: &Dokos<T>, id: &str) -> Result<BTreeMap<String, String>, DokosError> {
    let document: Map<String, Value> = dokos.lire(DOCTYPE_CONTRAT, id)?;
    Ok(valeurs_texte(document))
}

pub fn lister_types_contrat<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_TYPE_CONTRAT, Vec::new(), 100)
}

/// Contrats d'un employé, du plus récent au plus ancien.
pub fn contrats_employe<T: Transport>(dokos: &Dokos<T>, employe: &str) -> Result<Vec<ContratExistant>, DokosError> {
    let liste =
        Liste::new(CHAMPS_CONTRAT).filtre(Filtre::egal("employee", employe)).ordre("date_of_joining desc").limite(20);
    dokos.lister(DOCTYPE_CONTRAT, &liste)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn date(texte: &str) -> Date {
        texte.parse().expect("date de test")
    }

    fn meta_contrat() -> Meta {
        let champ = |fieldname: &str, fieldtype: &str, read_only: u8| Champ {
            fieldname: fieldname.into(),
            fieldtype: fieldtype.into(),
            label: fieldname.into(),
            options: String::new(),
            reqd: 0,
            hidden: 0,
            read_only,
            permlevel: 0,
        };
        Meta {
            doctype: DOCTYPE_CONTRAT.into(),
            champs: vec![
                champ("employee", "Link", 0),
                champ("remuneration_brute", "Currency", 0),
                champ("motif_cdd", "Small Text", 0),
                champ("calcule", "Data", 1),
            ],
        }
    }

    fn cdd() -> NouveauContrat {
        NouveauContrat::new("HR-EMP-00012", "CDD", "Animatrice", date("2026-12-01"), Some(date("2027-03-31")))
            .expect("contrat")
    }

    #[test]
    fn contrats_invalides() {
        let (e, f) = (date("2026-12-01"), date("2026-11-30"));
        assert_eq!(NouveauContrat::new(" ", "CDD", "P", e, None), Err(ErreurContrat::EmployeVide));
        assert_eq!(NouveauContrat::new("E", "", "P", e, None), Err(ErreurContrat::TypeVide));
        assert_eq!(NouveauContrat::new("E", "CDD", "", e, None), Err(ErreurContrat::PosteVide));
        assert_eq!(NouveauContrat::new("E", "CDD", "P", e, Some(f)), Err(ErreurContrat::FinAvantEntree));
        for h in [0.0, -1.0, 200.0, f64::NAN] {
            assert!(cdd().avec_heures(Some(h), None).is_err(), "{h}");
        }
        let mut jours = [0.0; 7];
        jours[2] = 25.0;
        assert_eq!(
            cdd().avec_heures(Some(24.0), Some(jours)),
            Err(ErreurContrat::HeuresJourInvalides { jour: "mercredi", heures: 25.0 })
        );
    }

    #[test]
    fn champs_supplementaires() {
        let champs = |paires: &[(&str, &str)]| -> BTreeMap<String, String> {
            paires.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect()
        };
        let meta = meta_contrat();
        let contrat =
            cdd().avec_champs(&meta, champs(&[("remuneration_brute", " 2100 "), ("motif_cdd", "")])).expect("champs");
        assert_eq!(contrat.corps()["remuneration_brute"], "2100");
        assert!(contrat.corps().get("motif_cdd").is_none());

        assert_eq!(
            cdd().avec_champs(&meta, champs(&[("salaire", "1")])),
            Err(ErreurContrat::ChampInconnu("salaire".into()))
        );
        assert_eq!(
            cdd().avec_champs(&meta, champs(&[("calcule", "1")])),
            Err(ErreurContrat::ChampInconnu("calcule".into()))
        );
        assert_eq!(
            cdd().avec_champs(&meta, champs(&[("employee", "X")])),
            Err(ErreurContrat::ChampGere("employee".into()))
        );
    }

    #[test]
    fn corps_et_valeurs() {
        let contrat = cdd()
            .avec_heures(Some(24.0), Some([6.0, 6.0, 6.0, 6.0, 0.0, 0.0, 0.0]))
            .expect("heures")
            .avec_societe(Some("Les Amanins"));
        assert_eq!(
            contrat.corps(),
            json!({
                "employee": "HR-EMP-00012", "contract_type": "CDD", "designation": "Animatrice",
                "date_of_joining": "2026-12-01", "relieving_date": "2027-03-31",
                "weekly_working_hours": 24.0, "company": "Les Amanins",
                "monday": 6.0, "tuesday": 6.0, "wednesday": 6.0, "thursday": 6.0,
                "friday": 0.0, "saturday": 0.0, "sunday": 0.0,
            })
        );
        let valeurs = contrat.valeurs();
        assert_eq!(valeurs["relieving_date"], "2027-03-31");
        assert_eq!(valeurs["monday"], "6");
        assert_eq!(valeurs["friday"], "");
    }

    #[test]
    fn repartition_vide_ignoree() {
        let contrat = cdd().avec_heures(Some(35.0), Some([0.0; 7])).expect("heures");
        assert!(contrat.corps().get("monday").is_none());
    }

    #[test]
    fn doublon_puis_creation() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "HR-EMP-CONT-00003", "employee": "HR-EMP-00012"}]}));
        let creation = creer_contrat(&Dokos::new(&faux), &cdd(), SiDoublon::Reutiliser).expect("doublon");
        assert!(matches!(creation, CreationContrat::DejaExistant(c) if c.id == "HR-EMP-CONT-00003"));

        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "HR-EMP-CONT-00004"}}));
        let creation = creer_contrat(&Dokos::new(&faux), &cdd(), SiDoublon::Reutiliser).expect("création");
        assert!(matches!(creation, CreationContrat::Cree(c) if c.id == "HR-EMP-CONT-00004"));
        assert_eq!(faux.requetes()[1].methode, Methode::Post);
        assert_eq!(faux.requetes()[1].chemin, "/api/resource/Employment%20Contract");
    }

    #[test]
    fn lecture_des_valeurs() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": {"name": "C1", "relieving_date": null, "weekly_working_hours": 24.0, "monday": 0,
                             "motif_cdd": "Surcroît d'activité", "leave_types": [{"x": 1}]}}),
        );
        let valeurs = lire_valeurs_contrat(&Dokos::new(&faux), "C1").expect("lecture");
        assert_eq!(valeurs["relieving_date"], "");
        assert_eq!(valeurs["weekly_working_hours"], "24.0");
        assert_eq!(valeurs["monday"], "");
        assert_eq!(valeurs["motif_cdd"], "Surcroît d'activité");
        assert!(!valeurs.contains_key("leave_types"));
    }
}
