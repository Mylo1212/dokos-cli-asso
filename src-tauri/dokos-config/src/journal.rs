//! Journal technique (fichier tournant, sans secret ni donnée personnelle),
//! diagnostic copiable, et journal des actions (ce qui a été créé, quand).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use dokos_core::date::Date;

use crate::{ErreurConfig, Profil, dossier, dossier_documents, lire};

/// Jours de conservation des fichiers de journal.
const CONSERVATION_JOURS: u64 = 30;
/// Lignes du journal reprises dans le diagnostic.
const LIGNES_DIAGNOSTIC: usize = 60;
pub const FICHIER_ACTIONS: &str = "journal-actions.csv";

/// Dossier des journaux : `%APPDATA%\dokos-cli\logs`.
pub fn dossier_journaux() -> Result<PathBuf, ErreurConfig> {
    let dossier = dossier()?.join("logs");
    fs::create_dir_all(&dossier).map_err(|e| ErreurConfig::Fichier(format!("{} : {e}", dossier.display())))?;
    Ok(dossier)
}

/// Installe le journal : un fichier par jour, `dokos-<appli>.<date>`, niveau
/// `debug` pour les crates de l'outil. Sans effet si un journal est déjà
/// installé. Les fichiers de plus de trente jours sont supprimés.
pub fn initialiser_journal(appli: &str) -> Result<PathBuf, ErreurConfig> {
    let dossier = dossier_journaux()?;
    purger_anciens(&dossier);
    let fichier = tracing_appender::rolling::daily(&dossier, format!("dokos-{appli}.log"));
    let filtre = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::Level::WARN)
        .with_target("dokos_core", tracing::Level::DEBUG)
        .with_target("dokos_config", tracing::Level::DEBUG)
        .with_target("dokos_import", tracing::Level::DEBUG)
        .with_target("assistant_core", tracing::Level::DEBUG)
        .with_target("conformite", tracing::Level::DEBUG)
        .with_target("dokos_assistant", tracing::Level::DEBUG)
        .with_target("dokos_cli", tracing::Level::DEBUG);
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let _ = tracing_subscriber::registry()
        .with(filtre)
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_target(true).with_writer(fichier))
        .try_init();
    tracing::info!(appli, version = env!("CARGO_PKG_VERSION"), "journal ouvert");
    Ok(dossier)
}

fn purger_anciens(dossier: &std::path::Path) {
    let Ok(entrees) = fs::read_dir(dossier) else { return };
    let limite = std::time::Duration::from_secs(CONSERVATION_JOURS * 86_400);
    for entree in entrees.flatten() {
        let ancien = entree
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age > limite);
        if ancien {
            let _ = fs::remove_file(entree.path());
        }
    }
}

/// Dernières lignes du journal le plus récent.
fn dernieres_lignes() -> String {
    let Ok(dossier) = dossier_journaux() else { return String::new() };
    let Ok(entrees) = fs::read_dir(&dossier) else { return String::new() };
    let mut fichiers: Vec<PathBuf> = entrees.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    fichiers.sort();
    let Some(dernier) = fichiers.pop() else { return String::new() };
    let Ok(texte) = fs::read_to_string(&dernier) else { return String::new() };
    let lignes: Vec<&str> = texte.lines().collect();
    let debut = lignes.len().saturating_sub(LIGNES_DIAGNOSTIC);
    lignes[debut..].join("\n")
}

/// Texte de diagnostic à copier dans une demande d'aide : versions, réglages
/// sans secret, dernières lignes du journal.
pub fn diagnostic(appli: &str) -> String {
    let mut texte = format!(
        "Dokos Assistant — diagnostic\nAppli : {appli} {}\nSystème : {} {}\nDate (UTC) : {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        Date::aujourdhui()
    );
    match crate::chemin() {
        Ok(chemin) => texte.push_str(&format!("Configuration : {}\n", chemin.display())),
        Err(e) => texte.push_str(&format!("Configuration : {e}\n")),
    }
    match lire() {
        Ok(Some(config)) => {
            texte.push_str(&format!("Dokos : {} (http autorisé : {})\n", config.url, config.autoriser_http));
            texte.push_str(&format!("Société : {}\n", config.societe.as_deref().unwrap_or("(non renseignée)")));
            for profil in Profil::TOUS {
                let etat = match config.cles.get(profil) {
                    Some(cle) => {
                        format!("clé {}… ({})", cle.chars().take(4).collect::<String>(), "secret dans le trousseau")
                    }
                    None => "non configuré".to_owned(),
                };
                texte.push_str(&format!("Profil {} : {etat}\n", profil.nom()));
            }
            texte.push_str(&format!(
                "IA : {} — {} — modèle {} — clé {}\n",
                if config.ia.active { "activée" } else { "désactivée" },
                config.ia.url,
                config.ia.modele,
                if crate::cle_ia_enregistree() { "enregistrée" } else { "absente" }
            ));
        }
        Ok(None) => texte.push_str("Aucune configuration enregistrée.\n"),
        Err(e) => texte.push_str(&format!("Configuration illisible : {e}\n")),
    }
    match crate::charger_regles() {
        Ok((regles, chemin)) => texte.push_str(&format!(
            "Mentions : {} ({}) — {}\n",
            regles.version,
            chemin.map_or("liste livrée".to_owned(), |c| c.display().to_string()),
            if regles.est_validee() { "validée" } else { "NON validée" }
        )),
        Err(e) => texte.push_str(&format!("Mentions : {e}\n")),
    }
    texte.push_str("\n--- Dernières lignes du journal ---\n");
    texte.push_str(&dernieres_lignes());
    texte.push('\n');
    texte
}

/// Une action enregistrée dans le journal des actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action<'a> {
    pub profil: &'a str,
    /// `creation`, `publication`, `activation`, `import`…
    pub action: &'a str,
    pub doctype: &'a str,
    pub document: &'a str,
    /// `cree`, `existant`, `erreur`…
    pub resultat: &'a str,
    pub detail: &'a str,
}

/// Horodatage UTC `AAAA-MM-JJ HH:MM:SS`.
pub fn horodatage() -> String {
    let secondes = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let date = Date::aujourdhui();
    let reste = secondes % 86_400;
    format!("{date} {:02}:{:02}:{:02}", reste / 3600, (reste % 3600) / 60, reste % 60)
}

fn champ_csv(texte: &str) -> String {
    let texte = texte.replace(['\n', '\r'], " ");
    if texte.contains([';', '"']) { format!("\"{}\"", texte.replace('"', "\"\"")) } else { texte }
}

/// Ligne CSV (séparateur `;`, lisible par Excel en français) d'une action.
pub fn ligne_action(horodatage: &str, action: &Action<'_>) -> String {
    [horodatage, action.profil, action.action, action.doctype, action.document, action.resultat, action.detail]
        .iter()
        .map(|c| champ_csv(c))
        .collect::<Vec<_>>()
        .join(";")
}

/// Ajoute l'action au fichier `Documents\Dokos\journal-actions.csv`, créé avec
/// son en-tête s'il n'existe pas. Une erreur d'écriture est journalisée, jamais
/// remontée : le journal ne doit pas bloquer une création réussie.
pub fn journaliser_action(action: &Action<'_>) {
    let Ok(dossier) = dossier_documents() else { return };
    let chemin = dossier.join(FICHIER_ACTIONS);
    let nouveau = !chemin.exists();
    let resultat = fs::OpenOptions::new().create(true).append(true).open(&chemin).and_then(|mut fichier| {
        if nouveau {
            writeln!(fichier, "horodatage_utc;profil;action;doctype;document;resultat;detail")?;
        }
        writeln!(fichier, "{}", ligne_action(&horodatage(), action))
    });
    if let Err(e) = resultat {
        tracing::warn!("journal des actions : {} : {e}", chemin.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ligne_csv_echappee() {
        let action = Action {
            profil: "ventes",
            action: "creation",
            doctype: "Sales Invoice",
            document: "F-1",
            resultat: "cree",
            detail: "total 12,50 ; client \"Mairie\"\nsuite",
        };
        assert_eq!(
            ligne_action("2026-11-03 10:00:00", &action),
            "2026-11-03 10:00:00;ventes;creation;Sales Invoice;F-1;cree;\"total 12,50 ; client \"\"Mairie\"\" suite\""
        );
    }

    #[test]
    fn horodatage_lisible() {
        let h = horodatage();
        assert_eq!(h.len(), 19, "{h}");
        assert_eq!(&h[10..11], " ");
    }

    #[test]
    fn diagnostic_sans_secret() {
        let texte = diagnostic("test");
        assert!(texte.starts_with("Dokos Assistant — diagnostic"));
        assert!(texte.contains("Dernières lignes du journal"));
    }
}
