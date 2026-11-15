//! Lecture de la structure d'un DocType (champs standard et personnalisés).

use serde::Deserialize;

use crate::client::Dokos;
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::texte_ou_vide;

/// Types de champ sans valeur (mise en page).
const TYPES_SANS_VALEUR: &[&str] =
    &["Section Break", "Column Break", "Tab Break", "HTML", "Button", "Heading", "Image", "Fold"];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Champ {
    pub fieldname: String,
    pub fieldtype: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub label: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub options: String,
    #[serde(default)]
    pub reqd: u8,
    #[serde(default)]
    pub hidden: u8,
    #[serde(default)]
    pub read_only: u8,
    #[serde(default)]
    pub permlevel: u8,
}

impl Champ {
    pub fn a_une_valeur(&self) -> bool {
        !TYPES_SANS_VALEUR.contains(&self.fieldtype.as_str())
    }

    /// Libellé, ou nom technique s'il n'y en a pas.
    pub fn libelle(&self) -> &str {
        if self.label.trim().is_empty() { &self.fieldname } else { &self.label }
    }

    /// Champ saisissable : visible, modifiable, sans niveau de permission.
    pub fn saisissable(&self) -> bool {
        self.a_une_valeur() && self.hidden == 0 && self.read_only == 0 && self.permlevel == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub doctype: String,
    pub champs: Vec<Champ>,
}

impl Meta {
    pub fn champ(&self, nom: &str) -> Option<&Champ> {
        self.champs.iter().find(|c| c.fieldname == nom)
    }

    pub fn a_champ(&self, nom: &str) -> bool {
        self.champ(nom).is_some_and(Champ::a_une_valeur)
    }
}

/// Champs d'un DocType, personnalisés compris, via `frappe.desk.form.load.getdoctype`.
///
/// La réponse de Dokos est volumineuse et ne change que si quelqu'un
/// personnalise le DocType : elle est gardée deux minutes par client
/// ([`Dokos::oublier_structures`] force une relecture). Vérifier puis créer un
/// contrat, ou enchaîner des exports, ne la redemande donc pas à chaque fois.
pub fn lire_meta<T: Transport>(dokos: &Dokos<T>, doctype: &str) -> Result<Meta, DokosError> {
    match dokos.structure_en_memoire(doctype) {
        Some(meta) => Ok(meta),
        None => relire_meta(dokos, doctype),
    }
}

/// Comme [`lire_meta`], mais si l'un des `champs` attendus manque dans la
/// structure gardée en mémoire, Dokos est relu : un champ que l'utilisateur
/// vient de créer est vu tout de suite, sans attendre l'expiration.
pub fn lire_meta_avec<T: Transport>(dokos: &Dokos<T>, doctype: &str, champs: &[&str]) -> Result<Meta, DokosError> {
    match dokos.structure_en_memoire(doctype) {
        Some(meta) if champs.iter().all(|c| meta.a_champ(c)) => Ok(meta),
        _ => relire_meta(dokos, doctype),
    }
}

/// Lit la structure dans Dokos, sans regarder la mémoire, et la retient.
pub fn relire_meta<T: Transport>(dokos: &Dokos<T>, doctype: &str) -> Result<Meta, DokosError> {
    #[derive(Deserialize)]
    struct Reponse {
        docs: Vec<Doc>,
    }
    #[derive(Deserialize)]
    struct Doc {
        name: String,
        #[serde(default)]
        fields: Vec<Champ>,
    }
    let reponse: Reponse = dokos.methode("frappe.desk.form.load.getdoctype", &[("doctype", doctype)])?;
    let doc = reponse
        .docs
        .into_iter()
        .find(|d| d.name == doctype)
        .ok_or_else(|| DokosError::ReponseInvalide(format!("structure du DocType {doctype} absente")))?;
    let meta = Meta { doctype: doc.name, champs: doc.fields };
    dokos.retenir_structure(&meta);
    Ok(meta)
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::transport::faux::FauxTransport;

    /// Réponse `getdoctype` pour un DocType donné, avec des tables enfants en plus.
    pub(crate) fn reponse_meta(doctype: &str, champs: Value) -> Value {
        json!({"docs": [
            {"name": doctype, "fields": champs},
            {"name": "Table enfant", "fields": [{"fieldname": "x", "fieldtype": "Data"}]},
        ], "user_settings": "{}"})
    }

    #[test]
    fn lecture() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_meta(
                "Event Registration",
                json!([
                    {"fieldname": "first_name", "fieldtype": "Data", "label": "First Name", "reqd": 1},
                    {"fieldname": "sb", "fieldtype": "Section Break"},
                    {"fieldname": "regime", "fieldtype": "Select", "label": "Régime", "options": "\nVégétarien"},
                    {"fieldname": "attended", "fieldtype": "Check", "read_only": 1},
                ]),
            ),
        );
        let meta = lire_meta(&Dokos::new(&faux), "Event Registration").expect("meta");
        assert_eq!(meta.champs.len(), 4);
        assert!(meta.a_champ("regime"));
        assert!(!meta.a_champ("sb"));
        assert!(!meta.a_champ("inconnu"));
        assert!(!meta.champ("attended").is_some_and(Champ::saisissable));
        assert_eq!(meta.champ("sb").map(Champ::libelle), Some("sb"));

        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/method/frappe.desk.form.load.getdoctype");
        assert_eq!(requete.parametre("doctype"), Some("Event Registration"));
    }

    #[test]
    fn structure_gardee_en_memoire() {
        let faux = FauxTransport::new();
        let champs =
            |noms: &[&str]| Value::Array(noms.iter().map(|n| json!({"fieldname": n, "fieldtype": "Data"})).collect());
        faux.repondre_json(200, &reponse_meta("Quotation", champs(&["a"])));
        faux.repondre_json(200, &reponse_meta("Quotation", champs(&["a", "b"])));
        faux.repondre_json(200, &reponse_meta("Quotation", champs(&["a", "b", "c"])));
        let dokos = Dokos::new(&faux);

        // Deuxième lecture : la mémoire, sans requête.
        assert!(lire_meta(&dokos, "Quotation").expect("première").a_champ("a"));
        assert!(!lire_meta(&dokos, "Quotation").expect("mémoire").a_champ("b"));
        assert_eq!(faux.requetes().len(), 1);

        // Champ attendu absent de la mémoire : Dokos est relu. Présent : non.
        assert!(lire_meta_avec(&dokos, "Quotation", &["b"]).expect("relue").a_champ("b"));
        assert!(lire_meta_avec(&dokos, "Quotation", &["a", "b"]).expect("mémoire").a_champ("b"));
        assert_eq!(faux.requetes().len(), 2);

        // Oubli demandé : nouvelle requête.
        dokos.oublier_structures();
        assert!(lire_meta(&dokos, "Quotation").expect("après oubli").a_champ("c"));
        assert_eq!(faux.requetes().len(), 3);
    }

    #[test]
    fn doctype_absent_de_la_reponse() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"docs": []}));
        assert!(matches!(lire_meta(&Dokos::new(&faux), "X"), Err(DokosError::ReponseInvalide(_))));
    }
}
