//! Transport HTTPS réel, basé sur `reqwest` (bloquant) et rustls.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use reqwest::redirect::Policy;

use crate::erreur::DokosError;
use crate::transport::{Methode, Reponse, Requete, Transport};

const DELAI_CONNEXION: Duration = Duration::from_secs(10);
const DELAI_TOTAL: Duration = Duration::from_secs(30);
/// Délai des requêtes marquées longues (PDF, transformation de document).
const DELAI_LONG: Duration = Duration::from_secs(120);
/// Connexion inutilisée gardée ouverte, pour éviter une nouvelle négociation TLS.
const DELAI_INACTIVITE: Duration = Duration::from_secs(300);
const DELAI_TCP_KEEPALIVE: Duration = Duration::from_secs(60);
/// Lectures menées de front au plus dans un lot : assez pour diviser l'attente,
/// assez peu pour ne pas occuper tous les processus d'un petit serveur Dokos.
const LECTURES_DE_FRONT: usize = 4;

/// Secret API. N'apparaît jamais dans `Debug`.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    fn exposer(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

pub struct HttpTransport {
    base: String,
    autorisation: HeaderValue,
    client: Client,
}

impl fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpTransport").field("base", &self.base).finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// `url` est la racine du site Dokos, par exemple `https://dokos.exemple.fr`.
    /// Le HTTP non chiffré est refusé sauf si `autoriser_http` est vrai.
    pub fn new(url: &str, cle: &str, secret: &Secret, autoriser_http: bool) -> Result<Self, DokosError> {
        let base = valider_url_base(url, autoriser_http)?;
        let cle = cle.trim();
        if cle.is_empty() || secret.exposer().trim().is_empty() {
            return Err(DokosError::Configuration("clé ou secret API vide".to_owned()));
        }
        Self::construire(base, &format!("token {cle}:{}", secret.exposer().trim()), autoriser_http, DELAI_TOTAL)
    }

    /// Transport vers un autre service à jeton `Bearer` (Open WebUI), avec
    /// un délai adapté (une réponse d'IA peut prendre une minute).
    pub fn bearer(url: &str, jeton: &Secret, autoriser_http: bool, delai_total: Duration) -> Result<Self, DokosError> {
        let base = valider_url_base(url, autoriser_http)?;
        if jeton.exposer().trim().is_empty() {
            return Err(DokosError::Configuration("clé API vide".to_owned()));
        }
        Self::construire(base, &format!("Bearer {}", jeton.exposer().trim()), autoriser_http, delai_total)
    }

    fn construire(
        base: String,
        autorisation: &str,
        autoriser_http: bool,
        delai_total: Duration,
    ) -> Result<Self, DokosError> {
        let mut autorisation = HeaderValue::from_str(autorisation).map_err(|_| {
            DokosError::Configuration("la clé ou le secret contient des caractères invalides".to_owned())
        })?;
        autorisation.set_sensitive(true);

        installer_fournisseur_tls();
        let client = Client::builder()
            // Pas de redirection : le jeton ne doit partir que vers l'URL configurée.
            .redirect(Policy::none())
            .https_only(!autoriser_http)
            .connect_timeout(DELAI_CONNEXION)
            .timeout(delai_total)
            .user_agent(concat!("dokos-cli/", env!("CARGO_PKG_VERSION")))
            // Réponses JSON compressées (`Accept-Encoding: gzip`) : les listes de
            // 500 clients ou articles pèsent plusieurs fois moins sur le réseau.
            .gzip(true)
            // Garde la connexion TLS ouverte entre deux actions de l'utilisateur.
            .tcp_keepalive(DELAI_TCP_KEEPALIVE)
            .pool_idle_timeout(DELAI_INACTIVITE)
            .build()
            .map_err(|e| DokosError::Configuration(format!("client HTTP : {e}")))?;

        Ok(Self { base, autorisation, client })
    }

    pub fn url(&self) -> &str {
        &self.base
    }
}

impl Transport for HttpTransport {
    fn envoyer(&self, requete: &Requete) -> Result<Reponse, DokosError> {
        let url = format!("{}{}", self.base, requete.chemin);
        let mut builder = match requete.methode {
            Methode::Get => self.client.get(url),
            Methode::Post => self.client.post(url),
            Methode::Put => self.client.put(url),
            Methode::Delete => self.client.delete(url),
        }
        .header(AUTHORIZATION, self.autorisation.clone())
        .header(ACCEPT, "application/json");
        if !requete.parametres.is_empty() {
            builder = builder.query(&requete.parametres);
        }
        if let Some(corps) = &requete.corps {
            builder = builder.header(CONTENT_TYPE, "application/json").body(corps.to_string());
        }
        if requete.delai_long {
            builder = builder.timeout(DELAI_LONG);
        }

        let depart = Instant::now();
        let reponse = builder.send().map_err(|e| {
            let message = e.without_url().to_string();
            tracing::warn!(methode = ?requete.methode, chemin = %requete.chemin, "réseau : {message}");
            DokosError::Reseau(message)
        })?;
        let statut = reponse.status().as_u16();
        if reponse.status().is_redirection() {
            return Err(DokosError::Configuration(format!(
                "Dokos répond par une redirection (HTTP {statut}) : vérifiez l'URL (https, nom de domaine exact)"
            )));
        }
        let corps = reponse.bytes().map_err(|e| DokosError::Reseau(e.without_url().to_string()))?;
        // Journal sans en-tête ni corps : ni jeton, ni donnée personnelle. La durée
        // et la taille disent où part le temps quand l'appli paraît lente.
        tracing::debug!(
            methode = ?requete.methode,
            chemin = %requete.chemin,
            statut,
            ms = depart.elapsed().as_millis(),
            octets = corps.len(),
            "réponse"
        );
        Ok(Reponse { statut, corps: corps.to_vec() })
    }

    fn parallele(&self) -> bool {
        true
    }

    /// Lectures menées de front, sur les connexions gardées ouvertes par le
    /// client. Un lot qui contient autre chose qu'un `GET` reste séquentiel :
    /// une écriture n'est jamais envoyée en parallèle d'une autre requête.
    fn envoyer_lot(&self, requetes: &[Requete]) -> Result<Vec<Reponse>, DokosError> {
        if requetes.len() < 2 || requetes.iter().any(|r| r.methode != Methode::Get) {
            return requetes.iter().map(|requete| self.envoyer(requete)).collect();
        }
        let suivante = AtomicUsize::new(0);
        let arret = AtomicBool::new(false);
        let resultats: Vec<Mutex<Option<Result<Reponse, DokosError>>>> =
            requetes.iter().map(|_| Mutex::new(None)).collect();
        std::thread::scope(|portee| {
            for _ in 0..requetes.len().min(LECTURES_DE_FRONT) {
                portee.spawn(|| {
                    while !arret.load(Ordering::Relaxed) {
                        let indice = suivante.fetch_add(1, Ordering::Relaxed);
                        let Some(requete) = requetes.get(indice) else { break };
                        let resultat = self.envoyer(requete);
                        if resultat.is_err() {
                            arret.store(true, Ordering::Relaxed);
                        }
                        if let Ok(mut case) = resultats[indice].lock() {
                            *case = Some(resultat);
                        }
                    }
                });
            }
        });
        // Toutes les réponses, dans l'ordre des requêtes, ou la première erreur.
        // Après une erreur, les requêtes pas encore parties ne partent plus.
        let mut reponses = Vec::with_capacity(requetes.len());
        let mut erreur = None;
        for case in resultats {
            match case.into_inner().ok().flatten() {
                Some(Ok(reponse)) => reponses.push(reponse),
                Some(Err(e)) => {
                    erreur.get_or_insert(e);
                }
                None => {}
            }
        }
        match erreur {
            Some(e) => Err(e),
            None if reponses.len() == requetes.len() => Ok(reponses),
            None => Err(DokosError::Reseau("lot de lectures interrompu".to_owned())),
        }
    }
}

/// Vérifie l'URL racine de Dokos et la renvoie sans `/` final.
pub fn valider_url_base(url: &str, autoriser_http: bool) -> Result<String, DokosError> {
    let erreur = |m: &str| Err(DokosError::Configuration(m.to_owned()));
    let Ok(analysee) = Url::parse(url.trim()) else {
        return erreur("URL Dokos invalide (exemple attendu : https://dokos.exemple.fr)");
    };
    match analysee.scheme() {
        "https" => {}
        "http" if autoriser_http => {}
        "http" => return erreur("HTTP non chiffré refusé : utilisez https:// ou activez autoriser_http"),
        _ => return erreur("l'URL Dokos doit commencer par https://"),
    }
    if analysee.host_str().is_none_or(str::is_empty) {
        return erreur("l'URL Dokos n'a pas de nom d'hôte");
    }
    if !analysee.username().is_empty() || analysee.password().is_some() {
        return erreur("l'URL Dokos ne doit pas contenir d'identifiants");
    }
    if analysee.query().is_some() || analysee.fragment().is_some() || analysee.path() != "/" {
        return erreur("l'URL Dokos doit être la racine du site, sans chemin (exemple : https://dokos.exemple.fr)");
    }
    Ok(analysee.as_str().trim_end_matches('/').to_owned())
}

/// reqwest est compilé sans fournisseur cryptographique : on installe ring.
/// Sans effet si un fournisseur est déjà installé.
fn installer_fournisseur_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_https_normalisee() {
        assert_eq!(valider_url_base("https://dokos.exemple.fr", false).as_deref(), Ok("https://dokos.exemple.fr"));
        assert_eq!(valider_url_base(" https://dokos.exemple.fr/ ", false).as_deref(), Ok("https://dokos.exemple.fr"));
        assert_eq!(
            valider_url_base("https://Dokos.Exemple.fr:8443", false).as_deref(),
            Ok("https://dokos.exemple.fr:8443")
        );
    }

    #[test]
    fn http_refuse_par_defaut() {
        assert!(valider_url_base("http://192.168.1.10", false).is_err());
        assert_eq!(valider_url_base("http://192.168.1.10:8000", true).as_deref(), Ok("http://192.168.1.10:8000"));
    }

    #[test]
    fn urls_refusees() {
        for url in [
            "",
            "dokos.exemple.fr",
            "ftp://dokos.exemple.fr",
            "https://user:mdp@dokos.exemple.fr",
            "https://dokos.exemple.fr/app",
            "https://dokos.exemple.fr/?a=1",
            "https://dokos.exemple.fr/#x",
        ] {
            assert!(valider_url_base(url, true).is_err(), "{url}");
        }
    }

    #[test]
    fn secret_masque() {
        let secret = Secret::new("tres-secret");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        let transport = HttpTransport::new("https://dokos.exemple.fr", "cle", &secret, false).expect("transport");
        let debug = format!("{transport:?}");
        assert!(!debug.contains("tres-secret"), "{debug}");
    }

    /// Petit serveur HTTP local : répond `{"chemin": …}` après `attente`, et
    /// note combien de requêtes il a traitées en même temps au plus.
    fn serveur_local(attente: Duration) -> (String, std::sync::Arc<AtomicUsize>) {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        use std::sync::Arc;

        let ecoute = TcpListener::bind("127.0.0.1:0").expect("port local");
        let url = format!("http://{}", ecoute.local_addr().expect("adresse"));
        let maximum = Arc::new(AtomicUsize::new(0));
        let en_cours = Arc::new(AtomicUsize::new(0));
        let (max, cours) = (Arc::clone(&maximum), en_cours);
        std::thread::spawn(move || {
            for flux in ecoute.incoming() {
                let Ok(mut flux) = flux else { break };
                let (max, cours) = (Arc::clone(&max), Arc::clone(&cours));
                std::thread::spawn(move || {
                    let Ok(copie) = flux.try_clone() else { return };
                    let mut lecteur = BufReader::new(copie);
                    loop {
                        let mut premiere = String::new();
                        if lecteur.read_line(&mut premiere).unwrap_or(0) == 0 {
                            return;
                        }
                        // En-têtes, puis le corps annoncé (POST), pour rester calé sur la requête suivante.
                        let mut longueur = 0;
                        let mut ligne = String::new();
                        while lecteur.read_line(&mut ligne).unwrap_or(0) > 2 {
                            if let Some(valeur) = ligne.to_ascii_lowercase().strip_prefix("content-length:") {
                                longueur = valeur.trim().parse().unwrap_or(0);
                            }
                            ligne.clear();
                        }
                        if lecteur.read_exact(&mut vec![0; longueur]).is_err() {
                            return;
                        }
                        let chemin = premiere.split(' ').nth(1).unwrap_or_default().to_owned();
                        max.fetch_max(cours.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                        std::thread::sleep(attente);
                        cours.fetch_sub(1, Ordering::SeqCst);
                        let corps = format!("{{\"chemin\":\"{chemin}\"}}");
                        let reponse = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{corps}",
                            corps.len()
                        );
                        if flux.write_all(reponse.as_bytes()).is_err() {
                            return;
                        }
                    }
                });
            }
        });
        (url, maximum)
    }

    #[test]
    fn lot_de_lectures_de_front_et_dans_l_ordre() {
        let (url, maximum) = serveur_local(Duration::from_millis(120));
        let transport = HttpTransport::new(&url, "cle", &Secret::new("secret"), true).expect("transport");
        let requetes: Vec<Requete> = (0..8).map(|i| Requete::get(format!("/api/resource/Doc/{i}"))).collect();

        let reponses = transport.envoyer_lot(&requetes).expect("lot");
        let chemins: Vec<String> = reponses.iter().map(|r| r.texte().into_owned()).collect();
        let attendus: Vec<String> = (0..8).map(|i| format!("{{\"chemin\":\"/api/resource/Doc/{i}\"}}")).collect();
        assert_eq!(chemins, attendus, "réponses dans l'ordre des requêtes");
        let de_front = maximum.load(Ordering::SeqCst);
        assert!((2..=LECTURES_DE_FRONT).contains(&de_front), "{de_front} lectures de front");
    }

    #[test]
    fn lot_avec_ecriture_reste_sequentiel() {
        let (url, maximum) = serveur_local(Duration::from_millis(30));
        let transport = HttpTransport::new(&url, "cle", &Secret::new("secret"), true).expect("transport");
        let requetes = [
            Requete::get("/api/resource/Doc/a"),
            Requete::post("/api/resource/Doc", serde_json::json!({"x": 1})),
            Requete::get("/api/resource/Doc/b"),
        ];
        assert_eq!(transport.envoyer_lot(&requetes).expect("lot").len(), 3);
        assert_eq!(maximum.load(Ordering::SeqCst), 1, "jamais deux requêtes à la fois avec une écriture");
    }

    #[test]
    fn lot_qui_echoue_renvoie_l_erreur() {
        // Port fermé : chaque lecture échoue, le lot renvoie une erreur réseau.
        let ecoute = std::net::TcpListener::bind("127.0.0.1:0").expect("port local");
        let url = format!("http://{}", ecoute.local_addr().expect("adresse"));
        drop(ecoute);
        let transport = HttpTransport::new(&url, "cle", &Secret::new("secret"), true).expect("transport");
        let requetes = [Requete::get("/api/resource/Doc/a"), Requete::get("/api/resource/Doc/b")];
        assert!(matches!(transport.envoyer_lot(&requetes), Err(DokosError::Reseau(_))));
    }

    #[test]
    fn identifiants_vides_refuses() {
        assert!(HttpTransport::new("https://d.fr", "", &Secret::new("s"), false).is_err());
        assert!(HttpTransport::new("https://d.fr", "c", &Secret::new("  "), false).is_err());
        assert!(HttpTransport::new("https://d.fr", "c", &Secret::new("s\nX-Autre: 1"), false).is_err());
        let delai = Duration::from_secs(5);
        assert!(HttpTransport::bearer("https://d.fr", &Secret::new(" "), false, delai).is_err());
        assert!(HttpTransport::bearer("http://mac-studio:8080", &Secret::new("sk-1"), false, delai).is_err());
        assert!(HttpTransport::bearer("http://mac-studio:8080", &Secret::new("sk-1"), true, delai).is_ok());
    }
}
