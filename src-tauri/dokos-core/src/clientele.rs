//! Création de clients (DocType `Customer`). Les contacts et adresses
//! restent gérés dans Dokos.

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Dokos, Filtre, Liste, lister_noms};
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::{Client, DOCTYPE_CLIENT};

pub const DOCTYPE_GROUPE_CLIENT: &str = "Customer Group";
pub const DOCTYPE_TERRITOIRE: &str = "Territory";
pub const NOM_MAX: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeClient {
    /// Entreprise, association, collectivité.
    Organisation,
    Particulier,
}

impl TypeClient {
    fn valeur_dokos(self) -> &'static str {
        match self {
            Self::Organisation => "Company",
            Self::Particulier => "Individual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurClient {
    NomVide,
    NomTropLong(usize),
    EmailInvalide(String),
}

impl fmt::Display for ErreurClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NomVide => write!(f, "nom du client manquant"),
            Self::NomTropLong(n) => write!(f, "nom du client trop long : {n} caractères (maximum {NOM_MAX})"),
            Self::EmailInvalide(e) => write!(f, "adresse e-mail invalide : « {e} »"),
        }
    }
}

impl std::error::Error for ErreurClient {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NouveauClient {
    nom: String,
    type_client: TypeClient,
    groupe: Option<String>,
    territoire: Option<String>,
    email: Option<String>,
    telephone: Option<String>,
}

impl NouveauClient {
    pub fn new(nom: &str, type_client: TypeClient) -> Result<Self, ErreurClient> {
        let nom = nom.split_whitespace().collect::<Vec<_>>().join(" ");
        if nom.is_empty() {
            return Err(ErreurClient::NomVide);
        }
        let longueur = nom.chars().count();
        if longueur > NOM_MAX {
            return Err(ErreurClient::NomTropLong(longueur));
        }
        Ok(Self { nom, type_client, groupe: None, territoire: None, email: None, telephone: None })
    }

    /// Groupe de clients et territoire : identifiants Dokos, ou les valeurs par
    /// défaut de Dokos s'ils sont vides.
    pub fn avec_classement(mut self, groupe: Option<&str>, territoire: Option<&str>) -> Self {
        self.groupe = non_vide(groupe);
        self.territoire = non_vide(territoire);
        self
    }

    pub fn avec_contact(mut self, email: Option<&str>, telephone: Option<&str>) -> Result<Self, ErreurClient> {
        self.email = match non_vide(email) {
            None => None,
            Some(e) if crate::employe::email_plausible(&e) => Some(e),
            Some(e) => return Err(ErreurClient::EmailInvalide(e)),
        };
        self.telephone = non_vide(telephone);
        Ok(self)
    }

    pub fn nom(&self) -> &str {
        &self.nom
    }

    pub fn corps(&self) -> Value {
        let mut corps = json!({"customer_name": self.nom, "customer_type": self.type_client.valeur_dokos()});
        if let Some(groupe) = &self.groupe {
            corps["customer_group"] = json!(groupe);
        }
        if let Some(territoire) = &self.territoire {
            corps["territory"] = json!(territoire);
        }
        if let Some(email) = &self.email {
            corps["email_id"] = json!(email);
        }
        if let Some(telephone) = &self.telephone {
            corps["mobile_no"] = json!(telephone);
        }
        corps
    }
}

fn non_vide(texte: Option<&str>) -> Option<String> {
    texte.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationClient {
    Cree(Client),
    /// Rien n'a été écrit : un client porte déjà ce nom.
    DejaExistant(Client),
}

#[derive(Debug, Deserialize)]
struct ClientCree {
    name: String,
    #[serde(default, deserialize_with = "crate::types::texte_ou_vide")]
    customer_name: String,
}

/// Client actif ou non de même nom (casse ignorée par la base de Dokos).
pub fn chercher_client_identique<T: Transport>(
    dokos: &Dokos<T>,
    client: &NouveauClient,
) -> Result<Option<Client>, DokosError> {
    let liste =
        Liste::new(&["name", "customer_name"]).filtre(Filtre::egal("customer_name", client.nom.as_str())).limite(1);
    Ok(dokos.lister::<Client>(DOCTYPE_CLIENT, &liste)?.into_iter().next())
}

pub fn creer_client<T: Transport>(
    dokos: &Dokos<T>,
    client: &NouveauClient,
    si_doublon: SiDoublon,
) -> Result<CreationClient, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = chercher_client_identique(dokos, client)?
    {
        return Ok(CreationClient::DejaExistant(existant));
    }
    let cree: ClientCree = dokos.creer(DOCTYPE_CLIENT, client.corps())?;
    Ok(CreationClient::Cree(Client { id: cree.name, nom: cree.customer_name }))
}

pub fn lister_groupes_clients<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_GROUPE_CLIENT, vec![Filtre::egal("is_group", 0)], 200)
}

pub fn lister_territoires<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_TERRITOIRE, vec![Filtre::egal("is_group", 0)], 200)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    #[test]
    fn corps_du_client() {
        let client = NouveauClient::new("  Mairie de   Crest ", TypeClient::Organisation)
            .expect("client")
            .avec_classement(Some("Collectivités"), Some(" "))
            .avec_contact(Some("mairie@crest.fr"), Some("04 75 00 00 00"))
            .expect("contact");
        assert_eq!(
            client.corps(),
            json!({
                "customer_name": "Mairie de Crest",
                "customer_type": "Company",
                "customer_group": "Collectivités",
                "email_id": "mairie@crest.fr",
                "mobile_no": "04 75 00 00 00",
            })
        );
        assert_eq!(NouveauClient::new(" ", TypeClient::Particulier), Err(ErreurClient::NomVide));
        assert!(matches!(
            NouveauClient::new(&"a".repeat(141), TypeClient::Particulier),
            Err(ErreurClient::NomTropLong(141))
        ));
        assert!(
            NouveauClient::new("X", TypeClient::Particulier)
                .expect("client")
                .avec_contact(Some("pas un mail"), None)
                .is_err()
        );
    }

    #[test]
    fn creation_et_doublon() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "CUST-9", "customer_name": "Mairie de Crest"}}));
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-9", "customer_name": "Mairie de Crest"}]}));
        let dokos = Dokos::new(&faux);
        let client = NouveauClient::new("Mairie de Crest", TypeClient::Organisation).expect("client");
        let cree = creer_client(&dokos, &client, SiDoublon::Reutiliser).expect("création");
        assert!(matches!(cree, CreationClient::Cree(c) if c.id == "CUST-9"));
        let doublon = creer_client(&dokos, &client, SiDoublon::Reutiliser).expect("doublon");
        assert!(matches!(doublon, CreationClient::DejaExistant(c) if c.id == "CUST-9"));
        let requetes = faux.requetes();
        assert_eq!(requetes[1].methode, Methode::Post);
        assert_eq!(requetes.len(), 3);
    }
}
