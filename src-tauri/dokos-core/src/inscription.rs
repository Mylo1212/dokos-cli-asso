//! Inscriptions à un événement (DocType `Event Registration` de l'appli
//! bookings) : saisie au nom d'un participant (téléphone, papier, groupe) et
//! liste des inscrits.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::client::{Dokos, Filtre, Liste};
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::evenement::{DOCTYPE_EVENEMENT, EvenementExistant};
use crate::formulaire::{DOCTYPE_INSCRIPTION, questions_possibles};
use crate::meta::Meta;
use crate::transport::Transport;
use crate::types::{texte_ou_vide, valeurs_texte};

pub const INSCRITS_MAX: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurInscription {
    EvenementVide,
    PrenomVide,
    NomVide,
    EmailInvalide(String),
    /// Réponse à une question qui n'existe pas (ou n'est pas permise) dans Dokos.
    QuestionInconnue(String),
}

impl fmt::Display for ErreurInscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EvenementVide => write!(f, "événement manquant"),
            Self::PrenomVide => write!(f, "prénom manquant"),
            Self::NomVide => write!(f, "nom manquant"),
            Self::EmailInvalide(e) => write!(f, "adresse e-mail invalide : « {e} »"),
            Self::QuestionInconnue(q) => {
                write!(f, "« {q} » n'est pas une question du formulaire d'inscription dans Dokos")
            }
        }
    }
}

impl std::error::Error for ErreurInscription {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NouvelleInscription {
    evenement: String,
    prenom: String,
    nom: String,
    email: String,
    telephone: Option<String>,
    reponses: BTreeMap<String, String>,
}

impl NouvelleInscription {
    /// `meta` est la structure d'`Event Registration` lue dans Dokos ; les
    /// réponses ne peuvent porter que sur ses questions possibles.
    pub fn new(
        meta: &Meta,
        evenement: &str,
        prenom: &str,
        nom: &str,
        email: &str,
        telephone: Option<&str>,
        reponses: BTreeMap<String, String>,
    ) -> Result<Self, ErreurInscription> {
        let net = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
        let (evenement, prenom, nom, email) =
            (evenement.trim().to_owned(), net(prenom), net(nom), email.trim().to_owned());
        if evenement.is_empty() {
            return Err(ErreurInscription::EvenementVide);
        }
        if prenom.is_empty() {
            return Err(ErreurInscription::PrenomVide);
        }
        if nom.is_empty() {
            return Err(ErreurInscription::NomVide);
        }
        if !crate::employe::email_plausible(&email) {
            return Err(ErreurInscription::EmailInvalide(email));
        }
        let possibles = questions_possibles(meta);
        let mut propres = BTreeMap::new();
        for (champ, valeur) in reponses {
            let champ = champ.trim().to_owned();
            if !possibles.iter().any(|c| c.fieldname == champ) {
                return Err(ErreurInscription::QuestionInconnue(champ));
            }
            let valeur = valeur.trim();
            if !valeur.is_empty() {
                propres.insert(champ, valeur.to_owned());
            }
        }
        let telephone = telephone.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned);
        Ok(Self { evenement, prenom, nom, email, telephone, reponses: propres })
    }

    pub fn evenement(&self) -> &str {
        &self.evenement
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn corps(&self) -> Value {
        let mut corps = json!({
            "event": self.evenement,
            "first_name": self.prenom,
            "last_name": self.nom,
            "email": self.email,
        });
        if let Some(telephone) = &self.telephone {
            corps["mobile_number"] = json!(telephone);
        }
        for (champ, valeur) in &self.reponses {
            corps[champ.as_str()] = json!(valeur);
        }
        corps
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct InscriptionExistante {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub event: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub first_name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub last_name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub email: String,
    #[serde(default)]
    pub docstatus: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationInscription {
    Cree(InscriptionExistante),
    /// Rien n'a été écrit : même événement, même e-mail.
    DejaExistant(InscriptionExistante),
}

const CHAMPS_INSCRIPTION: &[&str] = &["name", "event", "first_name", "last_name", "email", "docstatus"];

pub fn chercher_inscription_identique<T: Transport>(
    dokos: &Dokos<T>,
    inscription: &NouvelleInscription,
) -> Result<Option<InscriptionExistante>, DokosError> {
    let liste = Liste::new(CHAMPS_INSCRIPTION)
        .filtre(Filtre::egal("event", inscription.evenement.as_str()))
        .filtre(Filtre::egal("email", inscription.email.as_str()))
        .filtre(Filtre::different("docstatus", 2))
        .limite(1);
    Ok(dokos.lister::<InscriptionExistante>(DOCTYPE_INSCRIPTION, &liste)?.into_iter().next())
}

pub fn creer_inscription<T: Transport>(
    dokos: &Dokos<T>,
    inscription: &NouvelleInscription,
    si_doublon: SiDoublon,
) -> Result<CreationInscription, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existante) = chercher_inscription_identique(dokos, inscription)?
    {
        return Ok(CreationInscription::DejaExistant(existante));
    }
    Ok(CreationInscription::Cree(dokos.creer(DOCTYPE_INSCRIPTION, inscription.corps())?))
}

/// Événements à venir ou récents, pour choisir celui d'une inscription.
pub fn lister_evenements<T: Transport>(
    dokos: &Dokos<T>,
    depuis: &str,
    limite: u32,
) -> Result<Vec<EvenementExistant>, DokosError> {
    let liste = Liste::new(&["name", "subject", "starts_on", "published", "route", "registration_form"])
        .filtre(Filtre::different("status", "Cancelled"))
        .filtre(Filtre::superieur("starts_on", depuis))
        .ordre("starts_on asc")
        .limite(limite);
    dokos.lister(DOCTYPE_EVENEMENT, &liste)
}

/// Inscrits d'un événement, non annulés, toutes valeurs en texte (prénom, nom,
/// e-mail, téléphone et réponses aux questions).
pub fn lister_inscrits<T: Transport>(
    dokos: &Dokos<T>,
    evenement: &str,
) -> Result<Vec<BTreeMap<String, String>>, DokosError> {
    let evenement = evenement.trim();
    if evenement.is_empty() {
        return Err(DokosError::Configuration("événement vide".to_owned()));
    }
    let liste = Liste::new(&["*"])
        .filtre(Filtre::egal("event", evenement))
        .filtre(Filtre::different("docstatus", 2))
        .ordre("last_name asc")
        .limite(INSCRITS_MAX);
    let documents: Vec<Map<String, Value>> = dokos.lister(DOCTYPE_INSCRIPTION, &liste)?;
    Ok(documents.into_iter().map(valeurs_texte).collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::meta::Champ;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn meta() -> Meta {
        let champ = |nom: &str, fieldtype: &str| Champ {
            fieldname: nom.into(),
            fieldtype: fieldtype.into(),
            label: String::new(),
            options: String::new(),
            reqd: 0,
            hidden: 0,
            read_only: 0,
            permlevel: 0,
        };
        Meta {
            doctype: DOCTYPE_INSCRIPTION.into(),
            champs: vec![
                champ("event", "Link"),
                champ("first_name", "Data"),
                champ("last_name", "Data"),
                champ("email", "Data"),
                champ("mobile_number", "Data"),
                champ("age", "Int"),
                champ("allergies", "Small Text"),
                champ("amount", "Currency"),
            ],
        }
    }

    fn inscription(reponses: &[(&str, &str)]) -> Result<NouvelleInscription, ErreurInscription> {
        let reponses = reponses.iter().map(|(c, v)| ((*c).to_owned(), (*v).to_owned())).collect();
        NouvelleInscription::new(&meta(), "EV-1", " Marie ", "Dupont", "marie@exemple.fr", Some("06"), reponses)
    }

    #[test]
    fn corps_et_validation() {
        let i = inscription(&[("age", "12"), ("allergies", " ")]).expect("inscription");
        assert_eq!(
            i.corps(),
            json!({"event": "EV-1", "first_name": "Marie", "last_name": "Dupont", "email": "marie@exemple.fr",
                   "mobile_number": "06", "age": "12"})
        );
        assert_eq!(inscription(&[("amount", "10")]), Err(ErreurInscription::QuestionInconnue("amount".into())));
        assert_eq!(inscription(&[("inconnu", "x")]), Err(ErreurInscription::QuestionInconnue("inconnu".into())));
        let sans_mail = NouvelleInscription::new(&meta(), "EV-1", "A", "B", "rien", None, BTreeMap::new());
        assert_eq!(sans_mail, Err(ErreurInscription::EmailInvalide("rien".into())));
        assert!(NouvelleInscription::new(&meta(), "", "A", "B", "a@b.fr", None, BTreeMap::new()).is_err());
    }

    #[test]
    fn creation_et_doublon() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "REG-1", "event": "EV-1", "email": "marie@exemple.fr"}}));
        faux.repondre_json(200, &json!({"data": [{"name": "REG-1", "event": "EV-1"}]}));
        let dokos = Dokos::new(&faux);
        let i = inscription(&[]).expect("inscription");
        assert!(matches!(
            creer_inscription(&dokos, &i, SiDoublon::Reutiliser).expect("ok"),
            CreationInscription::Cree(_)
        ));
        assert!(matches!(
            creer_inscription(&dokos, &i, SiDoublon::Reutiliser).expect("ok"),
            CreationInscription::DejaExistant(_)
        ));
        let requetes = faux.requetes();
        assert_eq!(requetes[1].methode, Methode::Post);
        assert_eq!(requetes[1].chemin, "/api/resource/Event%20Registration");
    }

    #[test]
    fn inscrits_en_texte() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "REG-1", "first_name": "Marie", "age": 12, "attended": 0, "allergies": null}]}),
        );
        let inscrits = lister_inscrits(&Dokos::new(&faux), "EV-1").expect("inscrits");
        assert_eq!(inscrits[0]["age"], "12");
        assert_eq!(inscrits[0]["attended"], "");
        assert_eq!(inscrits[0]["allergies"], "");
        assert_eq!(faux.requetes()[0].parametre("fields"), Some(r#"["*"]"#));
        assert!(lister_inscrits(&Dokos::new(&faux), " ").is_err());
    }

    #[test]
    fn evenements_a_venir() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "EV-1", "subject": "Atelier"}]}));
        let evenements = lister_evenements(&Dokos::new(&faux), "2026-11-03", 50).expect("événements");
        assert_eq!(evenements[0].titre, "Atelier");
        assert!(
            faux.requetes()[0].parametre("filters").is_some_and(|f| f.contains(r#"["starts_on",">","2026-11-03"]"#))
        );
    }
}
