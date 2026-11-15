//! Erreurs du client Dokos et lecture des réponses d'erreur de Frappe.

use std::fmt;

use serde_json::Value;

/// Longueur maximale, en caractères, d'un message d'erreur serveur conservé.
const MESSAGE_MAX: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DokosError {
    /// Configuration locale invalide : URL, clé, secret.
    Configuration(String),
    /// Serveur injoignable : DNS, TLS, délai dépassé…
    Reseau(String),
    /// Clé ou secret API refusés (HTTP 401).
    Authentification,
    /// L'utilisateur API n'a pas le droit de faire cette opération (HTTP 403).
    Permission(String),
    /// Document ou DocType introuvable (HTTP 404).
    Introuvable(String),
    /// Le document existe déjà (HTTP 409).
    Doublon(String),
    /// Dokos a refusé le document : champ obligatoire, lien invalide… (HTTP 417).
    Validation(String),
    /// Toute autre réponse en erreur.
    Serveur { statut: u16, message: String },
    /// Réponse reçue mais illisible ou inattendue.
    ReponseInvalide(String),
}

impl DokosError {
    /// Construit l'erreur correspondant à une réponse HTTP hors 2xx.
    pub(crate) fn depuis_reponse(statut: u16, corps: &str) -> Self {
        let message = message_frappe(corps);
        match statut {
            401 => Self::Authentification,
            403 => Self::Permission(message),
            404 => Self::Introuvable(message),
            409 => Self::Doublon(message),
            400 | 417 | 422 => Self::Validation(message),
            _ => Self::Serveur { statut, message },
        }
    }
}

impl fmt::Display for DokosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(m) => write!(f, "configuration invalide : {m}"),
            Self::Reseau(m) => write!(f, "Dokos injoignable : {m}"),
            Self::Authentification => write!(f, "clé ou secret API refusés par Dokos"),
            Self::Permission(m) => write!(f, "permission refusée par Dokos : {m}"),
            Self::Introuvable(m) => write!(f, "introuvable dans Dokos : {m}"),
            Self::Doublon(m) => write!(f, "le document existe déjà : {m}"),
            Self::Validation(m) => write!(f, "refusé par Dokos : {m}"),
            Self::Serveur { statut, message } => write!(f, "erreur Dokos (HTTP {statut}) : {message}"),
            Self::ReponseInvalide(m) => write!(f, "réponse Dokos inattendue : {m}"),
        }
    }
}

impl std::error::Error for DokosError {}

/// Extrait un message lisible d'un corps d'erreur Frappe.
///
/// Ordre de préférence : `_server_messages` (messages affichés à l'utilisateur),
/// puis `exception`, puis `message`, puis `exc_type`. La trace Python (`exc`)
/// n'est jamais reprise.
fn message_frappe(corps: &str) -> String {
    let Ok(json) = serde_json::from_str::<Value>(corps) else {
        // Page HTML d'un proxy (502, 504…) ou texte brut.
        return finaliser(&nettoyer(corps));
    };

    let message = messages_serveur(&json)
        .or_else(|| json.get("exception").and_then(Value::as_str).map(sans_prefixe_python))
        .or_else(|| match json.get("message") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        })
        .or_else(|| json.get("exc_type").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    finaliser(&nettoyer(&message))
}

/// `_server_messages` est une chaîne JSON contenant une liste de chaînes JSON,
/// chacune étant un objet avec un champ `message`.
fn messages_serveur(json: &Value) -> Option<String> {
    let brut = json.get("_server_messages")?.as_str()?;
    let liste: Vec<String> = serde_json::from_str(brut).ok()?;
    let messages: Vec<String> = liste
        .iter()
        .filter_map(|element| {
            let objet: Value = serde_json::from_str(element).ok()?;
            objet.get("message")?.as_str().map(str::to_owned)
        })
        .filter(|m| !m.trim().is_empty())
        .collect();
    (!messages.is_empty()).then(|| messages.join(" ; "))
}

/// `frappe.exceptions.ValidationError: Message` → `Message`.
fn sans_prefixe_python(exception: &str) -> String {
    match exception.split_once(": ") {
        Some((prefixe, reste)) if !prefixe.contains(' ') && prefixe.contains('.') => reste.to_owned(),
        _ => exception.to_owned(),
    }
}

/// Retire les balises HTML, décode les entités courantes et compacte les espaces.
fn nettoyer(texte: &str) -> String {
    let mut sans_balises = String::with_capacity(texte.len());
    let mut dans_balise = false;
    for c in texte.chars() {
        match c {
            '<' => dans_balise = true,
            '>' if dans_balise => {
                dans_balise = false;
                sans_balises.push(' ');
            }
            _ if !dans_balise => sans_balises.push(c),
            _ => {}
        }
    }
    let decode = sans_balises
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decode.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn finaliser(message: &str) -> String {
    if message.is_empty() {
        return "(aucun détail)".to_owned();
    }
    if message.chars().count() <= MESSAGE_MAX {
        return message.to_owned();
    }
    let mut tronque: String = message.chars().take(MESSAGE_MAX).collect();
    tronque.push('…');
    tronque
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuts_http() {
        assert_eq!(DokosError::depuis_reponse(401, "{}"), DokosError::Authentification);
        assert!(matches!(DokosError::depuis_reponse(403, "{}"), DokosError::Permission(_)));
        assert!(matches!(DokosError::depuis_reponse(404, "{}"), DokosError::Introuvable(_)));
        assert!(matches!(DokosError::depuis_reponse(409, "{}"), DokosError::Doublon(_)));
        assert!(matches!(DokosError::depuis_reponse(417, "{}"), DokosError::Validation(_)));
        assert!(matches!(DokosError::depuis_reponse(500, "{}"), DokosError::Serveur { statut: 500, .. }));
    }

    #[test]
    fn server_messages_prioritaires() {
        let corps = r#"{
            "exc_type": "MandatoryError",
            "exception": "frappe.exceptions.MandatoryError: [Sales Order, new]: customer",
            "exc": "[\"Traceback (most recent call last): secret interne\"]",
            "_server_messages": "[\"{\\\"message\\\": \\\"Champs obligatoires : <b>Client</b>\\\", \\\"indicator\\\": \\\"red\\\"}\"]"
        }"#;
        let erreur = DokosError::depuis_reponse(417, corps);
        assert_eq!(erreur, DokosError::Validation("Champs obligatoires : Client".to_owned()));
    }

    #[test]
    fn plusieurs_server_messages() {
        let corps =
            r#"{"_server_messages": "[\"{\\\"message\\\": \\\"Un\\\"}\", \"{\\\"message\\\": \\\"Deux\\\"}\"]"}"#;
        assert_eq!(message_frappe(corps), "Un ; Deux");
    }

    #[test]
    fn exception_sans_prefixe() {
        let corps = r#"{"exception": "frappe.exceptions.DoesNotExistError: Customer XYZ not found"}"#;
        assert_eq!(message_frappe(corps), "Customer XYZ not found");
    }

    #[test]
    fn trace_python_jamais_reprise() {
        let corps = r#"{"exc_type": "PermissionError", "exc": "Traceback… /home/frappe/secret.py"}"#;
        let message = message_frappe(corps);
        assert_eq!(message, "PermissionError");
    }

    #[test]
    fn page_html_de_proxy() {
        let corps = "<html><head><title>502 Bad Gateway</title></head><body>nginx</body></html>";
        assert_eq!(message_frappe(corps), "502 Bad Gateway nginx");
    }

    #[test]
    fn corps_vide() {
        assert_eq!(message_frappe(""), "(aucun détail)");
        assert_eq!(message_frappe("{}"), "(aucun détail)");
    }

    #[test]
    fn entites_et_espaces() {
        assert_eq!(nettoyer("a&nbsp;&amp;&nbsp;b\n\n  c"), "a & b c");
        assert_eq!(nettoyer("x &lt;b&gt; y"), "x <b> y");
    }

    #[test]
    fn troncature_unicode() {
        let long = "é".repeat(MESSAGE_MAX + 10);
        let message = finaliser(&long);
        assert_eq!(message.chars().count(), MESSAGE_MAX + 1);
        assert!(message.ends_with('…'));
    }
}
