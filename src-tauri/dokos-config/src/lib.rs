//! Configuration partagée par la ligne de commande et l'appli.
//!
//! - `config.toml` : URL, société, et une clé API **par profil** (ventes,
//!   événements, RH, comptabilité), chacun lié à un utilisateur Dokos au rôle minimal.
//! - Secrets : trousseau du système (Gestionnaire d'identification sous
//!   Windows), service [`SERVICE_TROUSSEAU`], jamais dans le fichier.
//! - `[ia]` : serveur Open WebUI (adresse Tailscale), modèle et bases de
//!   connaissance par domaine ; sa clé API est aussi dans le trousseau.

pub mod journal;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fmt, fs};

use assistant_core::OpenWebUi;
use assistant_core::adresse::{URL_PAR_DEFAUT, valider_url_ia};
use assistant_core::owui::MODELE_PAR_DEFAUT;

use dokos_core::{Dokos, DokosError, HttpTransport, Secret};
use serde::{Deserialize, Serialize};

pub const SERVICE_TROUSSEAU: &str = "dokos-cli-asso";
/// Chemin du fichier de configuration, si on veut l'imposer.
const VARIABLE_CHEMIN: &str = "DOKOS_CONFIG";
/// Entrée du trousseau pour la clé Open WebUI.
const UTILISATEUR_IA: &str = "openwebui";
/// Une réponse du modèle local peut prendre plus d'une minute.
const DELAI_IA: Duration = Duration::from_secs(180);

/// Domaine d'usage, chacun avec son utilisateur API dans Dokos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profil {
    /// Clients, articles, devis, commandes, factures.
    Ventes,
    /// Événements et formulaires web (rôle Website Manager).
    Evenements,
    /// Export des employés, en lecture seule (rôle HR User ou lecture d'« Employee »).
    Rh,
    /// Grand livre : lecture des écritures comptables et des journaux.
    Compta,
}

impl Profil {
    pub const TOUS: [Profil; 4] = [Profil::Ventes, Profil::Evenements, Profil::Rh, Profil::Compta];

    pub fn nom(self) -> &'static str {
        match self {
            Self::Ventes => "ventes",
            Self::Evenements => "evenements",
            Self::Rh => "rh",
            Self::Compta => "compta",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Ventes => "Ventes",
            Self::Evenements => "Événements",
            Self::Rh => "RH",
            Self::Compta => "Comptabilité",
        }
    }

    /// Rôles conseillés pour l'utilisateur API de ce profil.
    pub fn roles_conseilles(self) -> &'static str {
        match self {
            Self::Ventes => {
                "lecture clients et articles, lecture et création de devis, commandes et factures, sans validation"
            }
            Self::Evenements => "Website Manager et création d'événements",
            Self::Rh => "HR User",
            Self::Compta => {
                "lecture seule de GL Entry, Accounting Journal et Account (Accounts User sans « Soumettre »)"
            }
        }
    }
}

impl fmt::Display for Profil {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

impl std::str::FromStr for Profil {
    type Err = String;

    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        match texte.trim().to_lowercase().as_str() {
            "ventes" | "vente" => Ok(Self::Ventes),
            "evenements" | "événements" | "evenement" | "événement" => Ok(Self::Evenements),
            "rh" => Ok(Self::Rh),
            "compta" | "comptabilite" | "comptabilité" => Ok(Self::Compta),
            autre => Err(format!("profil inconnu : « {autre} » (ventes, evenements, rh ou compta)")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ventes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evenements: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rh: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compta: Option<String>,
}

impl Cles {
    pub fn get(&self, profil: Profil) -> Option<&str> {
        match profil {
            Profil::Ventes => self.ventes.as_deref(),
            Profil::Evenements => self.evenements.as_deref(),
            Profil::Rh => self.rh.as_deref(),
            Profil::Compta => self.compta.as_deref(),
        }
    }

    pub fn set(&mut self, profil: Profil, cle: Option<String>) {
        let cle = cle.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty());
        match profil {
            Profil::Ventes => self.ventes = cle,
            Profil::Evenements => self.evenements = cle,
            Profil::Rh => self.rh = cle,
            Profil::Compta => self.compta = cle,
        }
    }
}

/// Noms de champ qui ressemblent à un secret : refusés partout dans le fichier.
const NOMS_SECRETS: &[&str] = &["secret", "password", "mot_de_passe", "motdepasse", "token", "jeton", "api_secret"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub url: String,
    #[serde(default)]
    pub autoriser_http: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub societe: Option<String>,
    #[serde(default)]
    pub cles: Cles,
    #[serde(default)]
    pub ia: ConfigIa,
    /// Champs écrits par une version plus récente de l'outil : conservés
    /// tels quels, jamais interprétés. Un nom de secret y est refusé.
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub autres: BTreeMap<String, toml::Value>,
}

impl Config {
    /// Configuration minimale : adresse de Dokos, rien d'autre.
    pub fn nouvelle(url: &str, autoriser_http: bool) -> Self {
        Self {
            url: url.to_owned(),
            autoriser_http,
            societe: None,
            cles: Cles::default(),
            ia: ConfigIa::default(),
            autres: BTreeMap::new(),
        }
    }
}

/// Identifiants des bases de connaissance Open WebUI, par domaine. Les
/// valeurs par défaut sont des noms d'attente : remplacez-les par les
/// identifiants réels (visibles dans l'adresse de la base dans Open WebUI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasesIa {
    #[serde(default)]
    pub ventes: Vec<String>,
    #[serde(default)]
    pub formulaires: Vec<String>,
}

impl Default for BasesIa {
    fn default() -> Self {
        Self { ventes: vec!["prestations".to_owned()], formulaires: vec!["evenements".to_owned()] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigIa {
    /// Faux : aucune zone Assistant, aucun appel à Open WebUI.
    #[serde(default = "vrai")]
    pub active: bool,
    #[serde(default = "url_ia_par_defaut")]
    pub url: String,
    #[serde(default = "modele_par_defaut")]
    pub modele: String,
    #[serde(default)]
    pub bases: BasesIa,
}

fn vrai() -> bool {
    true
}

fn url_ia_par_defaut() -> String {
    URL_PAR_DEFAUT.to_owned()
}

fn modele_par_defaut() -> String {
    MODELE_PAR_DEFAUT.to_owned()
}

impl Default for ConfigIa {
    fn default() -> Self {
        Self { active: true, url: url_ia_par_defaut(), modele: modele_par_defaut(), bases: BasesIa::default() }
    }
}

/// Liste d'identifiants séparés par des virgules ou des espaces.
pub fn liste_bases(texte: &str) -> Vec<String> {
    texte.split([',', ' ', ';', '\n']).map(str::trim).filter(|b| !b.is_empty()).map(str::to_owned).collect()
}

#[derive(Debug)]
pub enum ErreurConfig {
    Fichier(String),
    Trousseau(String),
    Dokos(DokosError),
    ProfilNonConfigure(Profil),
    /// L'assistant IA est désactivé dans les réglages.
    IaDesactivee,
}

impl fmt::Display for ErreurConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fichier(m) => write!(f, "configuration : {m}"),
            Self::Trousseau(m) => write!(f, "Gestionnaire d'identification : {m}"),
            Self::Dokos(e) => e.fmt(f),
            Self::ProfilNonConfigure(p) => write!(
                f,
                "aucune clé API pour le profil « {p} » : lancez « dokos connexion enregistrer --profil {p} » ou passez par Réglages"
            ),
            Self::IaDesactivee => write!(f, "assistant IA désactivé dans Réglages"),
        }
    }
}

impl std::error::Error for ErreurConfig {}

impl From<DokosError> for ErreurConfig {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

impl From<keyring::Error> for ErreurConfig {
    fn from(e: keyring::Error) -> Self {
        Self::Trousseau(e.to_string())
    }
}

impl Config {
    pub fn depuis_toml(texte: &str) -> Result<Self, ErreurConfig> {
        let config: Self = toml::from_str(texte).map_err(|e| ErreurConfig::Fichier(e.to_string()))?;
        if let Some(nom) = nom_de_secret(&config.autres) {
            return Err(ErreurConfig::Fichier(format!(
                "le champ « {nom} » ressemble à un secret : les secrets vont dans le trousseau, pas dans le fichier"
            )));
        }
        Ok(config)
    }

    pub fn en_toml(&self) -> Result<String, ErreurConfig> {
        toml::to_string(self).map_err(|e| ErreurConfig::Fichier(e.to_string()))
    }

    pub fn cle(&self, profil: Profil) -> Result<&str, ErreurConfig> {
        self.cles.get(profil).ok_or(ErreurConfig::ProfilNonConfigure(profil))
    }

    pub fn transport(&self, profil: Profil, secret: &Secret) -> Result<HttpTransport, ErreurConfig> {
        Ok(HttpTransport::new(&self.url, self.cle(profil)?, secret, self.autoriser_http)?)
    }

    /// Racine du site sans `/` final, pour construire des liens.
    pub fn url_base(&self) -> String {
        self.url.trim().trim_end_matches('/').to_owned()
    }
}

/// Premier nom de champ (à tout niveau) qui ressemble à un secret.
fn nom_de_secret(valeurs: &BTreeMap<String, toml::Value>) -> Option<String> {
    for (nom, valeur) in valeurs {
        let minuscule = nom.to_lowercase();
        if NOMS_SECRETS.iter().any(|s| minuscule.contains(s)) {
            return Some(nom.clone());
        }
        if let toml::Value::Table(table) = valeur {
            let imbrique: BTreeMap<String, toml::Value> = table.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            if let Some(trouve) = nom_de_secret(&imbrique) {
                return Some(format!("{nom}.{trouve}"));
            }
        }
    }
    None
}

/// Dossier de configuration : `%APPDATA%\dokos-cli-asso` sous Windows.
pub fn dossier() -> Result<PathBuf, ErreurConfig> {
    if let Some(chemin) = env::var_os(VARIABLE_CHEMIN) {
        let chemin = PathBuf::from(chemin);
        return Ok(chemin.parent().map(PathBuf::from).unwrap_or_default());
    }
    let dossier =
        dirs::config_dir().ok_or_else(|| ErreurConfig::Fichier("dossier de configuration introuvable".into()))?;
    Ok(dossier.join("dokos-cli-asso"))
}

pub fn chemin() -> Result<PathBuf, ErreurConfig> {
    if let Some(chemin) = env::var_os(VARIABLE_CHEMIN) {
        return Ok(PathBuf::from(chemin));
    }
    Ok(dossier()?.join("config.toml"))
}

fn erreur_fichier(chemin: &std::path::Path, e: std::io::Error) -> ErreurConfig {
    ErreurConfig::Fichier(format!("{} : {e}", chemin.display()))
}

/// Configuration existante, ou `None` si le fichier n'existe pas.
pub fn lire() -> Result<Option<Config>, ErreurConfig> {
    let chemin = chemin()?;
    match fs::read_to_string(&chemin) {
        Ok(texte) => Config::depuis_toml(&texte).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(erreur_fichier(&chemin, e)),
    }
}

pub fn charger() -> Result<Config, ErreurConfig> {
    lire()?.ok_or_else(|| {
        ErreurConfig::Fichier("aucune connexion enregistrée : lancez « dokos connexion enregistrer »".into())
    })
}

pub fn ecrire(config: &Config) -> Result<PathBuf, ErreurConfig> {
    let chemin = chemin()?;
    if let Some(dossier) = chemin.parent() {
        fs::create_dir_all(dossier).map_err(|e| erreur_fichier(dossier, e))?;
    }
    fs::write(&chemin, config.en_toml()?).map_err(|e| erreur_fichier(&chemin, e))?;
    Ok(chemin)
}

pub fn supprimer() -> Result<(), ErreurConfig> {
    let chemin = chemin()?;
    match fs::remove_file(&chemin) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(erreur_fichier(&chemin, e)),
    }
}

fn entree(profil: Profil, cle: &str) -> Result<keyring::Entry, ErreurConfig> {
    Ok(keyring::Entry::new(SERVICE_TROUSSEAU, &format!("{profil}:{cle}"))?)
}

pub fn lire_secret(profil: Profil, cle: &str) -> Result<Secret, ErreurConfig> {
    match entree(profil, cle)?.get_password() {
        Ok(secret) => Ok(Secret::new(secret)),
        Err(keyring::Error::NoEntry) => Err(ErreurConfig::Trousseau(format!(
            "secret API absent pour le profil « {profil} » : enregistrez à nouveau la connexion"
        ))),
        Err(e) => Err(e.into()),
    }
}

pub fn enregistrer_secret(profil: Profil, cle: &str, secret: &str) -> Result<(), ErreurConfig> {
    Ok(entree(profil, cle)?.set_password(secret)?)
}

pub fn oublier_secret(profil: Profil, cle: &str) -> Result<(), ErreurConfig> {
    match entree(profil, cle)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn lire_cle_ia() -> Result<Secret, ErreurConfig> {
    match keyring::Entry::new(SERVICE_TROUSSEAU, UTILISATEUR_IA)?.get_password() {
        Ok(cle) => Ok(Secret::new(cle)),
        Err(keyring::Error::NoEntry) => {
            Err(ErreurConfig::Trousseau("clé API Open WebUI absente : renseignez-la dans Réglages".into()))
        }
        Err(e) => Err(e.into()),
    }
}

pub fn cle_ia_enregistree() -> bool {
    lire_cle_ia().is_ok()
}

pub fn enregistrer_cle_ia(cle: &str) -> Result<(), ErreurConfig> {
    Ok(keyring::Entry::new(SERVICE_TROUSSEAU, UTILISATEUR_IA)?.set_password(cle.trim())?)
}

/// Client Open WebUI, à partir de la configuration et du trousseau.
pub fn connecter_ia() -> Result<(ConfigIa, OpenWebUi<HttpTransport>), ErreurConfig> {
    let config = lire()?.map(|c| c.ia).unwrap_or_default();
    connecter_ia_avec(config)
}

/// Client Open WebUI pour des réglages donnés (avant de les enregistrer, par exemple).
pub fn connecter_ia_avec(config: ConfigIa) -> Result<(ConfigIa, OpenWebUi<HttpTransport>), ErreurConfig> {
    if !config.active {
        return Err(ErreurConfig::IaDesactivee);
    }
    let (url, http) = valider_url_ia(&config.url)?;
    let transport = HttpTransport::bearer(&url, &lire_cle_ia()?, http, DELAI_IA)?;
    let modele = config.modele.clone();
    Ok((config, OpenWebUi::new(transport, &modele)))
}

/// Client Dokos du profil, à partir de la configuration et du trousseau.
pub fn connecter(profil: Profil) -> Result<(Config, Dokos<HttpTransport>), ErreurConfig> {
    let config = charger()?;
    let secret = lire_secret(profil, config.cle(profil)?)?;
    let transport = config.transport(profil, &secret)?;
    Ok((config, Dokos::new(transport).avec_memoire()))
}

/// Dossier `Documents\Dokos` : exports, modèles et rapports d'import, journal des actions.
pub fn dossier_documents() -> Result<PathBuf, ErreurConfig> {
    let base = dirs::document_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| ErreurConfig::Fichier("dossier Documents introuvable".into()))?;
    let dossier = base.join("Dokos");
    fs::create_dir_all(&dossier).map_err(|e| erreur_fichier(&dossier, e))?;
    Ok(dossier)
}

/// Nom de fichier sûr à partir d'un texte libre (nom de format, identifiant).
pub fn nom_de_fichier(texte: &str) -> String {
    let nom: String = texte
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|m| !m.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if nom.is_empty() { "document".to_owned() } else { nom.chars().take(80).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_toml() {
        let mut cles = Cles::default();
        cles.set(Profil::Ventes, Some(" abc ".into()));
        cles.set(Profil::Rh, Some("rh123".into()));
        cles.set(Profil::Evenements, Some("  ".into()));
        let config =
            Config { societe: Some("Les Amanins".into()), cles, ..Config::nouvelle("https://dokos.exemple.fr", false) };
        let texte = config.en_toml().expect("toml");
        assert!(!texte.contains("evenements = "), "{texte}");
        assert_eq!(Config::depuis_toml(&texte).expect("relecture"), config);
        assert_eq!(config.cle(Profil::Ventes).ok(), Some("abc"));
        assert!(matches!(config.cle(Profil::Evenements), Err(ErreurConfig::ProfilNonConfigure(Profil::Evenements))));
    }

    #[test]
    fn ia_par_defaut() {
        let config = Config::depuis_toml("url = \"https://d.fr\"\n").expect("config");
        assert_eq!(config.ia.url, "http://mac-studio:8080");
        assert_eq!(config.ia.modele, "qwen3.6-35b-a3b");
        assert_eq!(config.ia.bases.ventes, ["prestations"]);
        let partielle = Config::depuis_toml("url = \"https://d.fr\"\n[ia]\nmodele = \"autre\"\n").expect("config");
        assert_eq!((partielle.ia.url.as_str(), partielle.ia.modele.as_str()), ("http://mac-studio:8080", "autre"));
        assert_eq!(liste_bases(" kb-1, kb-2 ;kb-3 "), ["kb-1", "kb-2", "kb-3"]);
    }

    #[test]
    fn champ_inconnu_refuse() {
        // Un secret collé dans le fichier par erreur doit être refusé, pas ignoré.
        for texte in [
            "url = \"https://d.fr\"\nsecret = \"s\"\n",
            "url = \"https://d.fr\"\n[cles]\nventes = \"k\"\nsecret = \"s\"\n",
            "url = \"https://d.fr\"\n[futur]\napi_secret = \"s\"\n",
            "url = \"https://d.fr\"\n[ia]\nmot_de_passe = \"s\"\n",
        ] {
            assert!(Config::depuis_toml(texte).is_err(), "{texte}");
        }
    }

    #[test]
    fn champ_inconnu_conserve() {
        // Un champ d'une version plus récente est gardé tel quel à la réécriture.
        let texte = "url = \"https://d.fr\"\ncouleur = \"verte\"\n\n[futur]\noption = 3\n";
        let config = Config::depuis_toml(texte).expect("config");
        assert_eq!(config.autres.get("couleur").and_then(|v| v.as_str()), Some("verte"));
        let reecrit = config.en_toml().expect("toml");
        let relu = Config::depuis_toml(&reecrit).expect("relecture");
        assert_eq!(relu, config);
        assert!(reecrit.contains("option = 3"), "{reecrit}");
    }

    #[test]
    fn ia_desactivable() {
        let config = Config::depuis_toml("url = \"https://d.fr\"\n[ia]\nactive = false\n").expect("config");
        assert!(!config.ia.active);
        assert!(matches!(connecter_ia_avec(config.ia), Err(ErreurConfig::IaDesactivee)));
        assert!(Config::nouvelle("https://d.fr", false).ia.active);
    }

    #[test]
    fn profils() {
        assert_eq!("Événements".parse::<Profil>(), Ok(Profil::Evenements));
        assert_eq!("RH".parse::<Profil>(), Ok(Profil::Rh));
        assert_eq!("Comptabilité".parse::<Profil>(), Ok(Profil::Compta));
        assert_eq!(Profil::TOUS.len(), 4);
        assert!("paie".parse::<Profil>().is_err());
    }

    #[test]
    fn noms_de_fichier() {
        assert_eq!(nom_de_fichier("Factures / octobre"), "Factures-octobre");
        assert_eq!(nom_de_fichier("../../etc"), "etc");
        assert_eq!(nom_de_fichier("***"), "document");
    }
}
