//! Adresse du serveur Open WebUI.
//!
//! Le texte des demandes (noms de clients, de salariés…) part vers ce serveur.
//! `http://` n'est donc accepté que vers une machine du réseau Tailscale (trafic
//! chiffré par WireGuard) ou vers la machine locale ; ailleurs, `https://` est
//! obligatoire.

use std::net::IpAddr;

use dokos_core::DokosError;
use dokos_core::http::valider_url_base;

/// Adresse par défaut : nom MagicDNS du Mac Studio sur le réseau Tailscale.
pub const URL_PAR_DEFAUT: &str = "http://mac-studio:8080";

/// URL normalisée et vrai si elle est en `http://` (autorisé pour ce réseau).
pub fn valider_url_ia(url: &str) -> Result<(String, bool), DokosError> {
    let normalisee = valider_url_base(url, true)?;
    let Some(reste) = normalisee.strip_prefix("http://") else {
        return Ok((normalisee, false));
    };
    let hote = hote(reste);
    if reseau_prive_chiffre(&hote) {
        Ok((normalisee, true))
    } else {
        Err(DokosError::Configuration(format!(
            "http:// refusé vers « {hote} » : utilisez https://, ou l'adresse Tailscale du serveur (nom court, *.ts.net ou 100.x.y.z)"
        )))
    }
}

fn hote(autorite: &str) -> String {
    if let Some(ipv6) = autorite.strip_prefix('[') {
        return ipv6.split(']').next().unwrap_or_default().to_owned();
    }
    autorite.split(':').next().unwrap_or_default().to_owned()
}

/// Nom MagicDNS (court ou `*.ts.net`), adresse Tailscale (100.64.0.0/10,
/// fd7a:115c:a1e0::/48) ou boucle locale.
fn reseau_prive_chiffre(hote: &str) -> bool {
    if let Ok(ip) = hote.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                let [a, b, ..] = v4.octets();
                v4.is_loopback() || (a == 100 && (64..=127).contains(&b))
            }
            IpAddr::V6(v6) => {
                let segments = v6.segments();
                v6.is_loopback() || (segments[0] == 0xfd7a && segments[1] == 0x115c && segments[2] == 0xa1e0)
            }
        };
    }
    let hote = hote.to_ascii_lowercase();
    hote == "localhost" || hote.ends_with(".ts.net") || !hote.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresses_acceptees() {
        for (url, attendu, http) in [
            ("http://mac-studio:8080", "http://mac-studio:8080", true),
            ("http://mac-studio.tail1234.ts.net:3000/", "http://mac-studio.tail1234.ts.net:3000", true),
            ("http://100.101.102.103:8080", "http://100.101.102.103:8080", true),
            ("http://[fd7a:115c:a1e0::1]:8080", "http://[fd7a:115c:a1e0::1]:8080", true),
            ("http://localhost:3000", "http://localhost:3000", true),
            ("http://127.0.0.1:3000", "http://127.0.0.1:3000", true),
            ("https://ia.exemple.fr", "https://ia.exemple.fr", false),
        ] {
            assert_eq!(valider_url_ia(url).ok(), Some((attendu.to_owned(), http)), "{url}");
        }
    }

    #[test]
    fn http_hors_tailscale_refuse() {
        for url in [
            "http://ia.exemple.fr",
            "http://192.168.1.20:8080",
            "http://100.128.0.1",
            "http://8.8.8.8",
            "http://[2001:db8::1]",
            "ftp://mac-studio",
            "http://mac-studio/api",
        ] {
            assert!(valider_url_ia(url).is_err(), "{url}");
        }
    }
}
