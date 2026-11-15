//! Abstraction du réseau : le client construit des [`Requete`], le transport les envoie.

use serde_json::Value;

use crate::erreur::DokosError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Methode {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Requete {
    pub methode: Methode,
    /// Chemin déjà encodé, commençant par `/api/`.
    pub chemin: String,
    /// Paramètres de requête, non encodés : le transport s'en charge.
    pub parametres: Vec<(String, String)>,
    pub corps: Option<Value>,
    /// Réponse longue attendue (PDF, transformation) : délai plus généreux.
    pub delai_long: bool,
}

impl Requete {
    pub fn get(chemin: impl Into<String>) -> Self {
        Self { methode: Methode::Get, chemin: chemin.into(), parametres: Vec::new(), corps: None, delai_long: false }
    }

    pub fn post(chemin: impl Into<String>, corps: Value) -> Self {
        Self {
            methode: Methode::Post,
            chemin: chemin.into(),
            parametres: Vec::new(),
            corps: Some(corps),
            delai_long: false,
        }
    }

    pub fn put(chemin: impl Into<String>, corps: Value) -> Self {
        Self {
            methode: Methode::Put,
            chemin: chemin.into(),
            parametres: Vec::new(),
            corps: Some(corps),
            delai_long: false,
        }
    }

    pub fn delete(chemin: impl Into<String>) -> Self {
        Self { methode: Methode::Delete, chemin: chemin.into(), parametres: Vec::new(), corps: None, delai_long: false }
    }

    pub fn avec_parametres(mut self, parametres: &[(&str, &str)]) -> Self {
        self.parametres = parametres.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect();
        self
    }

    /// Demande un délai de réponse long (téléchargement, transformation).
    pub fn longue(mut self) -> Self {
        self.delai_long = true;
        self
    }

    /// Valeur d'un paramètre, pour les tests.
    pub fn parametre(&self, nom: &str) -> Option<&str> {
        self.parametres.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reponse {
    pub statut: u16,
    /// Octets bruts : JSON en général, PDF pour les impressions.
    pub corps: Vec<u8>,
}

impl Reponse {
    pub fn texte(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.corps)
    }
}

/// Envoie une requête à Dokos. L'authentification est l'affaire du transport :
/// le client ne voit jamais la clé ni le secret.
///
/// `Sync` : un même transport peut servir plusieurs requêtes à la fois
/// (voir [`crate::Dokos::pour_chacun`]).
pub trait Transport: Sync {
    fn envoyer(&self, requete: &Requete) -> Result<Reponse, DokosError>;

    /// Vrai si plusieurs requêtes peuvent partir en même temps. Faux par
    /// défaut : le faux transport des tests rejoue ses réponses dans l'ordre.
    fn parallele(&self) -> bool {
        false
    }

    /// Envoie plusieurs lectures indépendantes : les réponses dans l'ordre des
    /// requêtes, ou la première erreur. Par défaut, l'une après l'autre en
    /// s'arrêtant à la première erreur ; un transport réel peut les mener de
    /// front ([`crate::HttpTransport`]).
    fn envoyer_lot(&self, requetes: &[Requete]) -> Result<Vec<Reponse>, DokosError> {
        requetes.iter().map(|requete| self.envoyer(requete)).collect()
    }
}

impl<T: Transport + ?Sized> Transport for &T {
    fn envoyer(&self, requete: &Requete) -> Result<Reponse, DokosError> {
        (**self).envoyer(requete)
    }

    fn parallele(&self) -> bool {
        (**self).parallele()
    }

    fn envoyer_lot(&self, requetes: &[Requete]) -> Result<Vec<Reponse>, DokosError> {
        (**self).envoyer_lot(requetes)
    }
}

/// Faux transport pour les tests : rejoue des réponses prévues et garde les requêtes reçues.
#[cfg(any(test, feature = "faux"))]
pub mod faux {
    use std::collections::VecDeque;
    use std::sync::{Mutex, MutexGuard};

    use serde_json::Value;

    use super::{Reponse, Requete, Transport};
    use crate::erreur::DokosError;

    #[derive(Debug, Default)]
    pub struct FauxTransport {
        reponses: Mutex<VecDeque<Result<Reponse, DokosError>>>,
        requetes: Mutex<Vec<Requete>>,
    }

    /// Verrou qui survit à un test qui a paniqué en le tenant.
    fn verrou<V>(m: &Mutex<V>) -> MutexGuard<'_, V> {
        m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    impl FauxTransport {
        pub fn new() -> Self {
            Self::default()
        }

        /// Ajoute une réponse à rejouer, dans l'ordre d'arrivée des requêtes.
        pub fn repondre(&self, statut: u16, corps: impl Into<String>) -> &Self {
            self.repondre_octets(statut, corps.into().into_bytes())
        }

        pub fn repondre_octets(&self, statut: u16, corps: Vec<u8>) -> &Self {
            verrou(&self.reponses).push_back(Ok(Reponse { statut, corps }));
            self
        }

        pub fn repondre_json(&self, statut: u16, corps: &Value) -> &Self {
            self.repondre(statut, corps.to_string())
        }

        pub fn echouer(&self, erreur: DokosError) -> &Self {
            verrou(&self.reponses).push_back(Err(erreur));
            self
        }

        /// Requêtes reçues, dans l'ordre.
        pub fn requetes(&self) -> Vec<Requete> {
            verrou(&self.requetes).clone()
        }

        /// Nombre de réponses prévues qui n'ont pas été consommées.
        pub fn reponses_restantes(&self) -> usize {
            verrou(&self.reponses).len()
        }
    }

    impl Transport for FauxTransport {
        fn envoyer(&self, requete: &Requete) -> Result<Reponse, DokosError> {
            verrou(&self.requetes).push(requete.clone());
            verrou(&self.reponses)
                .pop_front()
                .unwrap_or_else(|| Err(DokosError::Reseau("FauxTransport : aucune réponse prévue".to_owned())))
        }
    }
}
