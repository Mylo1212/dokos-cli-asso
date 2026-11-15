//! Erreur structurée envoyée à l'interface : un genre, pour proposer la bonne
//! action (réessayer, ouvrir Réglages…), et un message lisible.

use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErreurVue {
    /// `reseau`, `authentification`, `permission`, `introuvable`, `doublon`,
    /// `validation`, `configuration`, `ia`, `fichier` ou `autre`.
    pub genre: String,
    pub message: String,
}

impl ErreurVue {
    pub fn new(genre: &str, message: impl Into<String>) -> Self {
        Self { genre: genre.to_owned(), message: message.into() }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new("validation", message)
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new("configuration", message)
    }

    pub fn fichier(message: impl Into<String>) -> Self {
        Self::new("fichier", message)
    }
}

impl fmt::Display for ErreurVue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ErreurVue {}

/// Convertit n'importe quelle erreur connue en erreur de vue.
pub fn erreur<E: Into<ErreurVue>>(e: E) -> ErreurVue {
    e.into()
}

impl From<dokos_core::DokosError> for ErreurVue {
    fn from(e: dokos_core::DokosError) -> Self {
        use dokos_core::DokosError as D;
        let genre = match &e {
            D::Configuration(_) => "configuration",
            D::Reseau(_) => "reseau",
            D::Authentification => "authentification",
            D::Permission(_) => "permission",
            D::Introuvable(_) => "introuvable",
            D::Doublon(_) => "doublon",
            D::Validation(_) => "validation",
            D::Serveur { .. } | D::ReponseInvalide(_) => "autre",
        };
        Self::new(genre, e.to_string())
    }
}

impl From<dokos_config::ErreurConfig> for ErreurVue {
    fn from(e: dokos_config::ErreurConfig) -> Self {
        use dokos_config::ErreurConfig as C;
        match e {
            C::Dokos(d) => d.into(),
            C::Fichier(_) | C::Trousseau(_) | C::ProfilNonConfigure(_) => Self::configuration(e.to_string()),
            C::IaDesactivee => Self::new("ia", e.to_string()),
        }
    }
}

impl From<assistant_core::ErreurIa> for ErreurVue {
    fn from(e: assistant_core::ErreurIa) -> Self {
        use assistant_core::ErreurIa as I;
        match e {
            I::Dokos(d) => d.into(),
            I::Injoignable(_) => Self::new("reseau", e.to_string()),
            I::CleRefusee => Self::configuration(e.to_string()),
            I::Service { .. } | I::ReponseIllisible(_) => Self::new("ia", e.to_string()),
            I::Demande(_) => Self::validation(e.to_string()),
        }
    }
}

impl From<dokos_import::ErreurImport> for ErreurVue {
    fn from(e: dokos_import::ErreurImport) -> Self {
        use dokos_import::ErreurImport as I;
        match e {
            I::Dokos(d) => d.into(),
            I::Fichier(_) | I::Modele(_) => Self::fichier(e.to_string()),
            I::Colonnes(_) | I::LignesBloquantes(_) => Self::validation(e.to_string()),
        }
    }
}

impl From<dokos_core::export::ErreurExport> for ErreurVue {
    fn from(e: dokos_core::export::ErreurExport) -> Self {
        use dokos_core::export::ErreurExport as E;
        match e {
            E::Dokos(d) => d.into(),
            E::Filtre(m) => Self::validation(m),
        }
    }
}

impl From<dokos_core::modele_vente::ErreurModele> for ErreurVue {
    fn from(e: dokos_core::modele_vente::ErreurModele) -> Self {
        use dokos_core::modele_vente::ErreurModele as M;
        match e {
            M::Dokos(d) => d.into(),
            M::ChampAbsent { .. } => Self::configuration(e.to_string()),
            autre => Self::validation(autre.to_string()),
        }
    }
}

impl From<std::io::Error> for ErreurVue {
    fn from(e: std::io::Error) -> Self {
        Self::fichier(e.to_string())
    }
}

impl From<String> for ErreurVue {
    fn from(message: String) -> Self {
        Self::validation(message)
    }
}

impl From<&str> for ErreurVue {
    fn from(message: &str) -> Self {
        Self::validation(message)
    }
}

macro_rules! erreurs_de_validation {
    ($($type:ty),* $(,)?) => {$(
        impl From<$type> for ErreurVue {
            fn from(e: $type) -> Self {
                Self::validation(e.to_string())
            }
        }
    )*};
}

erreurs_de_validation!(
    dokos_core::date::ErreurDate,
    dokos_core::vente::ErreurVente,
    dokos_core::evenement::ErreurEvenement,
    dokos_core::formulaire::ErreurFormulaire,
    dokos_core::article::ErreurArticle,
    dokos_core::clientele::ErreurClient,
    dokos_core::inscription::ErreurInscription,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genres() {
        assert_eq!(erreur(dokos_core::DokosError::Authentification).genre, "authentification");
        assert_eq!(erreur(dokos_core::DokosError::Reseau("x".into())).genre, "reseau");
        assert_eq!(erreur(dokos_config::ErreurConfig::IaDesactivee).genre, "ia");
        assert_eq!(erreur(dokos_config::ErreurConfig::Fichier("x".into())).genre, "configuration");
        assert_eq!(erreur(dokos_import::ErreurImport::LignesBloquantes(2)).genre, "validation");
        assert_eq!(erreur("texte").genre, "validation");
        assert_eq!(erreur(dokos_core::vente::ErreurVente::ClientVide).message, "client manquant");
        let json = serde_json::to_value(erreur(dokos_core::DokosError::Permission("non".into()))).expect("json");
        assert_eq!(json["genre"], "permission");
    }
}
