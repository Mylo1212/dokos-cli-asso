//! Transformation d'un document de vente en le suivant : devis → commande,
//! commande → facture.
//!
//! Dokos prépare le document suivant par ses méthodes de correspondance
//! (`make_sales_order`, `make_sales_invoice`) sans l'enregistrer ; l'outil
//! nettoie ce qu'il renvoie, vérifie qu'un document suivant n'existe pas
//! déjà, puis le crée en brouillon. Les lignes, prix et taxes viennent de
//! Dokos, jamais de l'outil.

use serde_json::{Map, Value, json};

use crate::client::{Dokos, Filtre, Liste};
use crate::doublon::{DocumentExistant, SiDoublon};
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::{DOCTYPE_COMMANDE, DOCTYPE_DEVIS, DOCTYPE_FACTURE, DocumentCree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transformation {
    DevisEnCommande,
    CommandeEnFacture,
}

impl Transformation {
    pub fn source(self) -> &'static str {
        match self {
            Self::DevisEnCommande => DOCTYPE_DEVIS,
            Self::CommandeEnFacture => DOCTYPE_COMMANDE,
        }
    }

    pub fn cible(self) -> &'static str {
        match self {
            Self::DevisEnCommande => DOCTYPE_COMMANDE,
            Self::CommandeEnFacture => DOCTYPE_FACTURE,
        }
    }

    fn methode(self) -> &'static str {
        match self {
            Self::DevisEnCommande => "erpnext.selling.doctype.quotation.quotation.make_sales_order",
            Self::CommandeEnFacture => "erpnext.selling.doctype.sales_order.sales_order.make_sales_invoice",
        }
    }

    /// Table enfant et champ de la cible qui référencent la source.
    fn lien_vers_source(self) -> (&'static str, &'static str) {
        match self {
            Self::DevisEnCommande => ("Sales Order Item", "prevdoc_docname"),
            Self::CommandeEnFacture => ("Sales Invoice Item", "sales_order"),
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::DevisEnCommande => "commande à partir du devis",
            Self::CommandeEnFacture => "facture à partir de la commande",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResultatTransformation {
    Cree(DocumentCree),
    /// Rien n'a été écrit : un document suivant non annulé existe déjà.
    DejaExistant(DocumentExistant),
}

/// Documents cibles non annulés déjà liés à la source.
pub fn documents_suivants<T: Transport>(
    dokos: &Dokos<T>,
    transformation: Transformation,
    source: &str,
) -> Result<Vec<DocumentExistant>, DokosError> {
    #[derive(serde::Deserialize)]
    struct Nom {
        name: String,
    }
    let (table, champ) = transformation.lien_vers_source();
    let liste = Liste::new(&["name"])
        .filtre(Filtre::enfant(table, champ, source))
        .filtre(Filtre::different("docstatus", 2))
        .ordre("creation desc")
        .limite(10);
    let noms: Vec<Nom> = dokos.lister(transformation.cible(), &liste)?;
    let mut uniques: Vec<String> = Vec::new();
    for nom in noms {
        if !uniques.contains(&nom.name) {
            uniques.push(nom.name);
        }
    }
    dokos.pour_chacun(&uniques, |nom| dokos.lire(transformation.cible(), nom))
}

/// Retire ce qui ne doit pas être renvoyé à Dokos : identifiants, marqueurs
/// internes (`__islocal`…), état. Les lignes restent liées à la source.
pub fn nettoyer_document_prepare(mut document: Map<String, Value>) -> Value {
    fn nettoyer(objet: &mut Map<String, Value>) {
        objet.retain(|cle, _| {
            !cle.starts_with("__")
                && !matches!(cle.as_str(), "name" | "owner" | "creation" | "modified" | "modified_by")
        });
        objet.insert("docstatus".to_owned(), json!(0));
        for valeur in objet.values_mut() {
            if let Value::Array(lignes) = valeur {
                for ligne in lignes {
                    if let Value::Object(ligne) = ligne {
                        nettoyer(ligne);
                    }
                }
            }
        }
    }
    nettoyer(&mut document);
    Value::Object(document)
}

/// Crée le document suivant en brouillon. `source` est l'identifiant du devis
/// ou de la commande, qui doit être validé dans Dokos.
pub fn transformer<T: Transport>(
    dokos: &Dokos<T>,
    transformation: Transformation,
    source: &str,
    si_doublon: SiDoublon,
) -> Result<ResultatTransformation, DokosError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(DokosError::Configuration("identifiant du document source vide".to_owned()));
    }
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = documents_suivants(dokos, transformation, source)?.into_iter().next()
    {
        return Ok(ResultatTransformation::DejaExistant(existant));
    }
    let prepare: Map<String, Value> = dokos.methode_post(transformation.methode(), json!({"source_name": source}))?;
    if prepare.get("doctype").and_then(Value::as_str) != Some(transformation.cible()) {
        return Err(DokosError::ReponseInvalide(format!(
            "Dokos n'a pas préparé de {} pour {source}",
            transformation.cible()
        )));
    }
    let cree: DocumentCree = dokos.creer(transformation.cible(), nettoyer_document_prepare(prepare))?;
    if cree.docstatus != 0 {
        return Err(DokosError::ReponseInvalide(format!("le document {} n'est pas en brouillon", cree.id)));
    }
    Ok(ResultatTransformation::Cree(cree))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    #[test]
    fn nettoyage() {
        let prepare = json!({
            "doctype": "Sales Order", "name": "new-sales-order-1", "__islocal": 1, "__unsaved": 1,
            "customer": "CUST-1", "docstatus": 0, "owner": "x",
            "items": [{"__islocal": 1, "name": "new-1", "item_code": "REPAS", "qty": 25, "prevdoc_docname": "Q-1"}],
        });
        let Value::Object(objet) = prepare else { panic!() };
        let propre = nettoyer_document_prepare(objet);
        assert_eq!(
            propre,
            json!({"doctype": "Sales Order", "customer": "CUST-1", "docstatus": 0,
                   "items": [{"item_code": "REPAS", "qty": 25, "prevdoc_docname": "Q-1", "docstatus": 0}]})
        );
    }

    #[test]
    fn devis_en_commande() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"message": {"doctype": "Sales Order", "__islocal": 1, "customer": "CUST-1",
                                 "items": [{"item_code": "REPAS", "qty": 25, "prevdoc_docname": "Q-1"}]}}),
        );
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SO-7", "docstatus": 0, "grand_total": 300.0, "currency": "EUR"}}),
        );
        let resultat =
            transformer(&Dokos::new(&faux), Transformation::DevisEnCommande, "Q-1", SiDoublon::Reutiliser).expect("ok");
        assert!(matches!(resultat, ResultatTransformation::Cree(d) if d.id == "SO-7"));
        let requetes = faux.requetes();
        assert_eq!(requetes[0].chemin, "/api/resource/Sales%20Order");
        assert!(
            requetes[0]
                .parametre("filters")
                .is_some_and(|f| f.contains(r#"["Sales Order Item","prevdoc_docname","=","Q-1"]"#))
        );
        assert_eq!(requetes[1].methode, Methode::Post);
        assert_eq!(requetes[1].chemin, "/api/method/erpnext.selling.doctype.quotation.quotation.make_sales_order");
        assert!(requetes[1].delai_long);
        assert_eq!(requetes[2].corps.as_ref().map(|c| c["customer"].clone()), Some(json!("CUST-1")));
    }

    #[test]
    fn commande_deja_facturee() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "F-1"}, {"name": "F-1"}]}));
        faux.repondre_json(200, &json!({"data": {"name": "F-1", "docstatus": 1, "posting_date": "2026-11-03"}}));
        let resultat =
            transformer(&Dokos::new(&faux), Transformation::CommandeEnFacture, "SO-1", SiDoublon::Reutiliser)
                .expect("ok");
        assert!(matches!(resultat, ResultatTransformation::DejaExistant(d) if d.id == "F-1"));
        assert_eq!(faux.requetes().len(), 2);
    }

    #[test]
    fn mauvais_doctype_prepare() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"message": {"doctype": "Quotation"}}));
        let resultat = transformer(&Dokos::new(&faux), Transformation::DevisEnCommande, "Q-1", SiDoublon::Reutiliser);
        assert!(matches!(resultat, Err(DokosError::ReponseInvalide(_))));
        assert!(transformer(&Dokos::new(&faux), Transformation::DevisEnCommande, " ", SiDoublon::Reutiliser).is_err());
    }
}
