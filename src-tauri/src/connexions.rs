//! Connexions à Dokos et à Open WebUI gardées en mémoire par profil : la
//! configuration, le trousseau et le client HTTPS ne sont relus qu'après un
//! changement de réglages, pas à chaque frappe dans un champ de recherche.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use assistant_core::OpenWebUi;
use dokos_config::{Config, ConfigIa, Profil};
use dokos_core::{Dokos, HttpTransport};

use crate::erreurs::{ErreurVue, erreur};

/// Réglages et client Open WebUI, ensemble.
pub type ClientIa = (ConfigIa, OpenWebUi<HttpTransport>);

#[derive(Default)]
struct Cache {
    config: Mutex<Option<Arc<Config>>>,
    dokos: Mutex<BTreeMap<Profil, Arc<Dokos<HttpTransport>>>>,
    ia: Mutex<Option<Arc<ClientIa>>>,
}

static CACHE: LazyLock<Cache> = LazyLock::new(Cache::default);

/// Oublie tout : à appeler après chaque enregistrement de réglages.
pub fn invalider() {
    if let Ok(mut c) = CACHE.config.lock() {
        *c = None;
    }
    if let Ok(mut d) = CACHE.dokos.lock() {
        d.clear();
    }
    if let Ok(mut i) = CACHE.ia.lock() {
        *i = None;
    }
    tracing::debug!("connexions oubliées");
}

/// Configuration enregistrée (erreur s'il n'y en a pas).
pub fn config() -> Result<Arc<Config>, ErreurVue> {
    if let Ok(cache) = CACHE.config.lock()
        && let Some(config) = cache.as_ref()
    {
        return Ok(Arc::clone(config));
    }
    let config = Arc::new(dokos_config::charger().map_err(erreur)?);
    if let Ok(mut cache) = CACHE.config.lock() {
        *cache = Some(Arc::clone(&config));
    }
    Ok(config)
}

/// Client Dokos du profil, créé une fois puis réutilisé.
pub fn connecter(profil: Profil) -> Result<(Arc<Config>, Arc<Dokos<HttpTransport>>), ErreurVue> {
    let config = config()?;
    if let Ok(cache) = CACHE.dokos.lock()
        && let Some(dokos) = cache.get(&profil)
    {
        return Ok((config, Arc::clone(dokos)));
    }
    let (_, dokos) = dokos_config::connecter(profil).map_err(erreur)?;
    let dokos = Arc::new(dokos);
    if let Ok(mut cache) = CACHE.dokos.lock() {
        cache.insert(profil, Arc::clone(&dokos));
    }
    tracing::debug!(profil = %profil, "connexion Dokos ouverte");
    Ok((config, dokos))
}

/// Client Open WebUI, créé une fois puis réutilisé.
pub fn connecter_ia() -> Result<Arc<ClientIa>, ErreurVue> {
    if let Ok(cache) = CACHE.ia.lock()
        && let Some(ia) = cache.as_ref()
    {
        return Ok(Arc::clone(ia));
    }
    let ia = Arc::new(dokos_config::connecter_ia().map_err(erreur)?);
    if let Ok(mut cache) = CACHE.ia.lock() {
        *cache = Some(Arc::clone(&ia));
    }
    Ok(ia)
}
