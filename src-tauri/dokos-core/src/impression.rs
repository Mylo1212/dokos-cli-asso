//! Formats d'impression : lecture seule, et téléchargement de PDF.
//!
//! Modifier un format d'impression demande le rôle System Manager : l'outil
//! ne le fait jamais. Les modifications sont proposées dans un fichier, et
//! appliquées à la main dans Dokos.

use serde::Deserialize;

use crate::client::{Dokos, Filtre, lister_noms};
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::texte_ou_vide;

pub const DOCTYPE_FORMAT: &str = "Print Format";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FormatImpression {
    #[serde(rename = "name")]
    pub nom: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub doc_type: String,
    /// Gabarit Jinja (HTML), si le format est personnalisé.
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub html: String,
    #[serde(default)]
    pub custom_format: u8,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub print_format_type: String,
    #[serde(default)]
    pub disabled: u8,
}

impl FormatImpression {
    /// Seuls les formats HTML/Jinja personnalisés peuvent être analysés.
    /// Ceux du constructeur visuel n'ont pas de gabarit texte.
    pub fn est_jinja(&self) -> bool {
        self.custom_format == 1
            && matches!(self.print_format_type.as_str(), "" | "Jinja")
            && !self.html.trim().is_empty()
    }
}

/// Formats actifs pour un DocType.
pub fn lister_formats<T: Transport>(dokos: &Dokos<T>, doctype: &str) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_FORMAT, vec![Filtre::egal("doc_type", doctype), Filtre::egal("disabled", 0)], 100)
}

pub fn lire_format<T: Transport>(dokos: &Dokos<T>, nom: &str) -> Result<FormatImpression, DokosError> {
    dokos.lire(DOCTYPE_FORMAT, nom)
}

/// PDF d'un document avec un format d'impression donné.
pub fn telecharger_pdf<T: Transport>(
    dokos: &Dokos<T>,
    doctype: &str,
    nom: &str,
    format: &str,
) -> Result<Vec<u8>, DokosError> {
    let pdf = dokos.telecharger(
        "frappe.utils.print_format.download_pdf",
        &[("doctype", doctype), ("name", nom), ("format", format), ("no_letterhead", "0")],
    )?;
    if !pdf.starts_with(b"%PDF") {
        return Err(DokosError::ReponseInvalide("Dokos n'a pas renvoyé de PDF".to_owned()));
    }
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::faux::FauxTransport;

    #[test]
    fn formats_jinja() {
        let format = |custom_format, print_format_type: &str, html: &str| FormatImpression {
            nom: "F".into(),
            doc_type: "Employment Contract".into(),
            html: html.into(),
            custom_format,
            print_format_type: print_format_type.into(),
            disabled: 0,
        };
        assert!(format(1, "Jinja", "<p>{{ doc.name }}</p>").est_jinja());
        assert!(!format(0, "Jinja", "<p></p>").est_jinja());
        assert!(!format(1, "JS", "x").est_jinja());
        assert!(!format(1, "Jinja", "  ").est_jinja());
    }

    #[test]
    fn liste_et_lecture() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "Contrat CDD"}]}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "Contrat CDD", "doc_type": "Employment Contract", "html": "<p></p>",
                             "custom_format": 1, "print_format_type": "Jinja", "disabled": 0}}),
        );
        let dokos = Dokos::new(&faux);
        assert_eq!(lister_formats(&dokos, "Employment Contract").expect("liste"), ["Contrat CDD"]);
        assert!(lire_format(&dokos, "Contrat CDD").expect("lecture").est_jinja());
        assert_eq!(
            faux.requetes()[0].parametre("filters"),
            Some(r#"[["doc_type","=","Employment Contract"],["disabled","=",0]]"#)
        );
        assert_eq!(faux.requetes()[1].chemin, "/api/resource/Print%20Format/Contrat%20CDD");
    }

    #[test]
    fn pdf() {
        let faux = FauxTransport::new();
        faux.repondre_octets(200, b"%PDF-1.7 contenu".to_vec());
        faux.repondre(200, "<html>erreur</html>");
        let dokos = Dokos::new(&faux);
        let pdf = telecharger_pdf(&dokos, "Employment Contract", "C1", "Contrat CDD").expect("pdf");
        assert!(pdf.starts_with(b"%PDF"));
        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/method/frappe.utils.print_format.download_pdf");
        assert_eq!(requete.parametre("format"), Some("Contrat CDD"));
        assert!(matches!(
            telecharger_pdf(&dokos, "Employment Contract", "C1", "Contrat CDD"),
            Err(DokosError::ReponseInvalide(_))
        ));
    }
}
