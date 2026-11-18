use std::fmt;

use dokos_config::ErreurConfig;
use dokos_core::DokosError;
use dokos_core::article::ErreurArticle;
use dokos_core::date::ErreurDate;
use dokos_core::vente::ErreurVente;
use dokos_import::ErreurImport;

#[derive(Debug)]
pub enum Erreur {
    Config(ErreurConfig),
    Dokos(DokosError),
    Saisie(String),
    /// Client ou article introuvable ou ambigu : l'utilisateur doit préciser.
    Resolution(String),
    /// La vérification a trouvé des erreurs : rien n'a été créé.
    Bloque(String),
    Fichier(String),
    Import(ErreurImport),
}

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(e) => e.fmt(f),
            Self::Dokos(e) => e.fmt(f),
            Self::Import(e) => e.fmt(f),
            Self::Saisie(m) | Self::Resolution(m) | Self::Bloque(m) | Self::Fichier(m) => f.write_str(m),
        }
    }
}

impl From<ErreurConfig> for Erreur {
    fn from(e: ErreurConfig) -> Self {
        Self::Config(e)
    }
}

impl From<DokosError> for Erreur {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

impl From<ErreurImport> for Erreur {
    fn from(e: ErreurImport) -> Self {
        match e {
            ErreurImport::Dokos(e) => Self::Dokos(e),
            autre => Self::Import(autre),
        }
    }
}

macro_rules! erreur_de_saisie {
    ($($type:ty),*) => {$(
        impl From<$type> for Erreur {
            fn from(e: $type) -> Self {
                Self::Saisie(e.to_string())
            }
        }
    )*};
}

erreur_de_saisie!(ErreurVente, ErreurDate, ErreurArticle);
