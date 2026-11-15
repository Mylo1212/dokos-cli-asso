//! Création d'employés (DocType `Employee`).

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Dokos, Filtre, Liste, lister_noms};
use crate::date::Date;
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::recherche::{Candidat, Correspondance, motif_recherche, resoudre};
use crate::transport::Transport;
use crate::types::texte_ou_vide;

pub const DOCTYPE_EMPLOYE: &str = "Employee";
pub const DOCTYPE_GENRE: &str = "Gender";
pub const DOCTYPE_POSTE: &str = "Designation";
/// Âge minimal absolu à l'embauche en France (travaux légers pendant les vacances scolaires).
pub const AGE_MINIMUM: i32 = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurEmploye {
    PrenomVide,
    NomVide,
    GenreVide,
    SocieteVide,
    NaissanceApresEntree,
    TropJeune(i32),
    EmailInvalide(String),
}

impl fmt::Display for ErreurEmploye {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrenomVide => write!(f, "prénom manquant"),
            Self::NomVide => write!(f, "nom manquant"),
            Self::GenreVide => write!(f, "genre manquant (obligatoire dans Dokos)"),
            Self::SocieteVide => write!(f, "société manquante (obligatoire dans Dokos)"),
            Self::NaissanceApresEntree => write!(f, "la date de naissance est après la date d'entrée"),
            Self::TropJeune(age) => write!(f, "{age} ans à l'embauche : moins de {AGE_MINIMUM} ans est impossible"),
            Self::EmailInvalide(e) => write!(f, "adresse e-mail invalide : « {e} »"),
        }
    }
}

impl std::error::Error for ErreurEmploye {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NouvelEmploye {
    prenom: String,
    nom: String,
    genre: String,
    naissance: Date,
    entree: Date,
    societe: String,
    email: Option<String>,
    poste: Option<String>,
}

impl NouvelEmploye {
    /// `genre` est un nom du DocType `Gender` de Dokos (voir [`lister_genres`]).
    pub fn new(
        prenom: &str,
        nom: &str,
        genre: &str,
        naissance: Date,
        entree: Date,
        societe: &str,
    ) -> Result<Self, ErreurEmploye> {
        let net = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
        let (prenom, nom, genre, societe) = (net(prenom), net(nom), net(genre), net(societe));
        if prenom.is_empty() {
            return Err(ErreurEmploye::PrenomVide);
        }
        if nom.is_empty() {
            return Err(ErreurEmploye::NomVide);
        }
        if genre.is_empty() {
            return Err(ErreurEmploye::GenreVide);
        }
        if societe.is_empty() {
            return Err(ErreurEmploye::SocieteVide);
        }
        if naissance >= entree {
            return Err(ErreurEmploye::NaissanceApresEntree);
        }
        let age = naissance.annees_pleines_jusqua(entree);
        if age < AGE_MINIMUM {
            return Err(ErreurEmploye::TropJeune(age));
        }
        Ok(Self { prenom, nom, genre, naissance, entree, societe, email: None, poste: None })
    }

    pub fn avec_email(mut self, email: Option<&str>) -> Result<Self, ErreurEmploye> {
        self.email = match email.map(str::trim).filter(|e| !e.is_empty()) {
            None => None,
            Some(e) if email_plausible(e) => Some(e.to_owned()),
            Some(e) => return Err(ErreurEmploye::EmailInvalide(e.to_owned())),
        };
        Ok(self)
    }

    /// Poste : un nom du DocType `Designation`.
    pub fn avec_poste(mut self, poste: Option<&str>) -> Self {
        self.poste = poste.map(str::trim).filter(|p| !p.is_empty()).map(str::to_owned);
        self
    }

    pub fn nom_complet(&self) -> String {
        format!("{} {}", self.prenom, self.nom)
    }

    pub fn entree(&self) -> Date {
        self.entree
    }

    pub fn corps(&self) -> Value {
        let mut corps = json!({
            "first_name": self.prenom,
            "last_name": self.nom,
            "gender": self.genre,
            "date_of_birth": self.naissance.to_string(),
            "date_of_joining": self.entree.to_string(),
            "company": self.societe,
            "status": "Active",
        });
        if let Some(email) = &self.email {
            corps["personal_email"] = json!(email);
        }
        if let Some(poste) = &self.poste {
            corps["designation"] = json!(poste);
        }
        corps
    }
}

pub(crate) fn email_plausible(email: &str) -> bool {
    match email.split_once('@') {
        Some((local, domaine)) => {
            !local.is_empty()
                && domaine.contains('.')
                && !domaine.starts_with('.')
                && !domaine.ends_with('.')
                && !email.chars().any(|c| c.is_whitespace() || c == ',' || c == ';')
                && !domaine.contains('@')
        }
        None => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EmployeExistant {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "employee_name", default, deserialize_with = "texte_ou_vide")]
    pub nom_complet: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub status: String,
    #[serde(rename = "date_of_joining", default, deserialize_with = "texte_ou_vide")]
    pub entree: String,
    #[serde(rename = "company", default, deserialize_with = "texte_ou_vide")]
    pub societe: String,
}

impl Candidat for EmployeExistant {
    fn identifiant(&self) -> &str {
        &self.id
    }

    fn nom(&self) -> &str {
        &self.nom_complet
    }
}

const CHAMPS_EMPLOYE: &[&str] = &["name", "employee_name", "status", "date_of_joining", "company"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationEmploye {
    Cree(EmployeExistant),
    /// Rien n'a été écrit : même prénom, même nom, même date de naissance.
    DejaExistant(EmployeExistant),
}

pub fn chercher_employe_identique<T: Transport>(
    dokos: &Dokos<T>,
    employe: &NouvelEmploye,
) -> Result<Option<EmployeExistant>, DokosError> {
    let liste = Liste::new(CHAMPS_EMPLOYE)
        .filtre(Filtre::egal("first_name", employe.prenom.as_str()))
        .filtre(Filtre::egal("last_name", employe.nom.as_str()))
        .filtre(Filtre::egal("date_of_birth", employe.naissance.to_string()))
        .limite(1);
    Ok(dokos.lister::<EmployeExistant>(DOCTYPE_EMPLOYE, &liste)?.into_iter().next())
}

pub fn creer_employe<T: Transport>(
    dokos: &Dokos<T>,
    employe: &NouvelEmploye,
    si_doublon: SiDoublon,
) -> Result<CreationEmploye, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = chercher_employe_identique(dokos, employe)?
    {
        return Ok(CreationEmploye::DejaExistant(existant));
    }
    Ok(CreationEmploye::Cree(dokos.creer(DOCTYPE_EMPLOYE, employe.corps())?))
}

/// Employés actifs dont l'identifiant ou le nom contient `texte`.
pub fn chercher_employes<T: Transport>(dokos: &Dokos<T>, texte: &str) -> Result<Vec<EmployeExistant>, DokosError> {
    let motif = motif_recherche(texte)?;
    let liste = Liste::new(CHAMPS_EMPLOYE)
        .filtre(Filtre::egal("status", "Active"))
        .ou(Filtre::contient("name", &motif))
        .ou(Filtre::contient("employee_name", &motif))
        .ordre("employee_name asc")
        .limite(10);
    dokos.lister(DOCTYPE_EMPLOYE, &liste)
}

/// Employé actif désigné par son nom : unique, ambigu ou introuvable.
pub fn trouver_employe<T: Transport>(
    dokos: &Dokos<T>,
    texte: &str,
) -> Result<Correspondance<EmployeExistant>, DokosError> {
    Ok(resoudre(texte, chercher_employes(dokos, texte)?))
}

pub fn lister_genres<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_GENRE, Vec::new(), 50)
}

pub fn lister_postes<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_POSTE, Vec::new(), 500)
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

    fn marie() -> NouvelEmploye {
        NouvelEmploye::new(" Marie ", "Dupont", "Femme", date("1995-04-02"), date("2026-12-01"), "Les Amanins")
            .expect("employé")
    }

    #[test]
    fn employes_invalides() {
        let (n, e) = (date("1995-04-02"), date("2026-12-01"));
        let nouveau = |p, nom, g, n, e, s| NouvelEmploye::new(p, nom, g, n, e, s);
        assert_eq!(nouveau(" ", "D", "F", n, e, "S"), Err(ErreurEmploye::PrenomVide));
        assert_eq!(nouveau("M", "", "F", n, e, "S"), Err(ErreurEmploye::NomVide));
        assert_eq!(nouveau("M", "D", "", n, e, "S"), Err(ErreurEmploye::GenreVide));
        assert_eq!(nouveau("M", "D", "F", n, e, " "), Err(ErreurEmploye::SocieteVide));
        assert_eq!(nouveau("M", "D", "F", e, n, "S"), Err(ErreurEmploye::NaissanceApresEntree));
        assert_eq!(nouveau("M", "D", "F", date("2013-01-01"), e, "S"), Err(ErreurEmploye::TropJeune(13)));
        assert!(nouveau("M", "D", "F", date("2012-12-01"), e, "S").is_ok());
    }

    #[test]
    fn emails() {
        assert!(marie().avec_email(Some("marie@exemple.fr")).is_ok());
        assert!(marie().avec_email(Some("  ")).is_ok());
        for invalide in ["marie", "marie@", "@exemple.fr", "marie@exemple", "ma rie@exemple.fr", "a@b@c.fr", "m@.fr"] {
            assert!(marie().avec_email(Some(invalide)).is_err(), "{invalide}");
        }
    }

    #[test]
    fn corps() {
        let employe = marie().avec_email(Some("marie@exemple.fr")).expect("email").avec_poste(Some("Animatrice"));
        assert_eq!(
            employe.corps(),
            json!({
                "first_name": "Marie", "last_name": "Dupont", "gender": "Femme",
                "date_of_birth": "1995-04-02", "date_of_joining": "2026-12-01",
                "company": "Les Amanins", "status": "Active",
                "personal_email": "marie@exemple.fr", "designation": "Animatrice",
            })
        );
    }

    #[test]
    fn doublon_puis_creation() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "HR-EMP-00012", "employee_name": "Marie Dupont"}]}));
        let creation = creer_employe(&Dokos::new(&faux), &marie(), SiDoublon::Reutiliser).expect("doublon");
        assert!(matches!(creation, CreationEmploye::DejaExistant(e) if e.id == "HR-EMP-00012"));
        assert_eq!(
            faux.requetes()[0].parametre("filters"),
            Some(r#"[["first_name","=","Marie"],["last_name","=","Dupont"],["date_of_birth","=","1995-04-02"]]"#)
        );

        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "HR-EMP-00013", "employee_name": "Marie Dupont"}}));
        let creation = creer_employe(&Dokos::new(&faux), &marie(), SiDoublon::Reutiliser).expect("création");
        assert!(matches!(creation, CreationEmploye::Cree(e) if e.id == "HR-EMP-00013"));
        assert_eq!(faux.requetes()[1].methode, Methode::Post);
        assert_eq!(faux.requetes()[1].chemin, "/api/resource/Employee");
    }

    #[test]
    fn listes_de_reference() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "Femme"}, {"name": "Homme"}]}));
        assert_eq!(lister_genres(&Dokos::new(&faux)).expect("genres"), ["Femme", "Homme"]);
        assert_eq!(faux.requetes()[0].chemin, "/api/resource/Gender");
    }
}
