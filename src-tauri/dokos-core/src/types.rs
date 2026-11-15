//! Documents Dokos lus par l'outil.
//!
//! DocTypes et champs standard d'ERPNext, repris tels quels par Dokos.
//! Les champs personnalisés (inscriptions, etc.) ne sont pas encore gérés.

use serde::{Deserialize, Deserializer};

pub const DOCTYPE_CLIENT: &str = "Customer";
pub const DOCTYPE_ARTICLE: &str = "Item";
pub const DOCTYPE_DEVIS: &str = "Quotation";
pub const DOCTYPE_COMMANDE: &str = "Sales Order";
pub const DOCTYPE_FACTURE: &str = "Sales Invoice";
/// Employés : lus pour l'export et pour les échéances de contrats.
pub const DOCTYPE_EMPLOYE: &str = "Employee";

/// Champs demandés à Dokos pour un [`Client`].
pub const CHAMPS_CLIENT: &[&str] = &["name", "customer_name"];
/// Champs demandés à Dokos pour un [`Article`].
pub const CHAMPS_ARTICLE: &[&str] = &["name", "item_name", "stock_uom", "item_group"];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Client {
    /// Identifiant Dokos (`name`), utilisé dans les liens.
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "customer_name", default, deserialize_with = "texte_ou_vide")]
    pub nom: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Article {
    /// Code article (`name` = `item_code`).
    #[serde(rename = "name")]
    pub code: String,
    #[serde(rename = "item_name", default, deserialize_with = "texte_ou_vide")]
    pub libelle: String,
    #[serde(rename = "stock_uom", default, deserialize_with = "texte_ou_vide")]
    pub unite: String,
    #[serde(rename = "item_group", default, deserialize_with = "texte_ou_vide")]
    pub groupe: String,
}

/// Retour de Dokos après création d'un devis, d'une commande ou d'une facture.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DocumentCree {
    #[serde(rename = "name")]
    pub id: String,
    pub docstatus: u8,
    /// Total calculé par Dokos (prix et taxes de Dokos).
    #[serde(rename = "grand_total", default)]
    pub total: f64,
    #[serde(rename = "currency", default, deserialize_with = "texte_ou_vide")]
    pub devise: String,
}

/// Toutes les valeurs simples d'un document, converties en texte (vide si
/// absente ou nulle, `0` numérique compris). Les tables et objets sont ignorés.
pub fn valeurs_texte(
    document: serde_json::Map<String, serde_json::Value>,
) -> std::collections::BTreeMap<String, String> {
    use serde_json::Value;
    document
        .into_iter()
        .filter_map(|(nom, valeur)| {
            let texte = match valeur {
                Value::Null => String::new(),
                Value::String(s) => s,
                Value::Number(n) if n.as_f64() == Some(0.0) => String::new(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Array(_) | Value::Object(_) => return None,
            };
            Some((nom, texte))
        })
        .collect()
}

/// Frappe renvoie `null` pour un champ texte non rempli.
pub(crate) fn texte_ou_vide<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn client_avec_nom_nul() {
        let client: Client = serde_json::from_value(json!({"name": "CUST-1", "customer_name": null})).expect("client");
        assert_eq!(client, Client { id: "CUST-1".into(), nom: String::new() });
    }

    #[test]
    fn article_champs_absents() {
        let article: Article = serde_json::from_value(json!({"name": "REPAS"})).expect("article");
        assert_eq!(
            article,
            Article { code: "REPAS".into(), libelle: String::new(), unite: String::new(), groupe: String::new() }
        );
    }

    #[test]
    fn document_cree() {
        let commande: DocumentCree = serde_json::from_value(json!({
            "name": "SAL-ORD-2026-00012",
            "docstatus": 0,
            "customer": "Mairie de Crest",
            "grand_total": 512.5,
            "currency": "EUR",
            "items": []
        }))
        .expect("commande");
        assert_eq!(commande.id, "SAL-ORD-2026-00012");
        assert_eq!(commande.docstatus, 0);
        assert!((commande.total - 512.5).abs() < f64::EPSILON);
    }
}

/// Adresse e-mail plausible : une partie locale, un domaine avec un point, rien d'autre.
pub(crate) fn email_plausible(email: &str) -> bool {
    match email.split_once('@') {
        Some((local, domaine)) => {
            !local.is_empty()
                && domaine.contains('.')
                && !domaine.starts_with('.')
                && !domaine.ends_with('.')
                && !email.chars().any(|c| c.is_whitespace() || c == ',' || c == ';')
                && !domaine.contains('@')
        }
        None => false,
    }
}

#[cfg(test)]
mod tests_email {
    use super::email_plausible;

    #[test]
    fn emails() {
        assert!(email_plausible("marie.dupont@exemple.fr"));
        assert!(!email_plausible("marie"));
        assert!(!email_plausible("@exemple.fr"));
        assert!(!email_plausible("marie@exemple"));
        assert!(!email_plausible("marie@.fr"));
        assert!(!email_plausible("marie dupont@exemple.fr"));
        assert!(!email_plausible("a@b.fr, c@d.fr"));
    }
}
