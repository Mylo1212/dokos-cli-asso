//! « Mes brouillons » : documents créés par l'outil qui attendent une
//! validation dans Dokos. Lecture seule.

use serde::Deserialize;

use crate::client::{Dokos, Filtre, Liste};
use crate::erreur::DokosError;
use crate::evenement::DOCTYPE_EVENEMENT;
use crate::transport::Transport;
use crate::types::texte_ou_vide;
use crate::vente::TypeVente;

pub const BROUILLONS_MAX: u32 = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct Brouillon {
    pub doctype: &'static str,
    pub id: String,
    /// Client ou titre de l'événement.
    pub tiers: String,
    pub date: String,
    pub total: f64,
    pub devise: String,
    pub modifie: String,
}

#[derive(Debug, Deserialize)]
struct VenteBrouillon {
    name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    customer_name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    party_name: String,
    #[serde(rename = "transaction_date", alias = "posting_date", default, deserialize_with = "texte_ou_vide")]
    date: String,
    #[serde(default)]
    grand_total: f64,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    currency: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    modified: String,
}

#[derive(Debug, Deserialize)]
struct EvenementBrouillon {
    name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    subject: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    starts_on: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    modified: String,
}

/// Devis, commandes et factures en brouillon créés par `utilisateur`
/// (l'utilisateur API du profil), du plus récent au plus ancien.
pub fn brouillons_de_vente<T: Transport>(dokos: &Dokos<T>, utilisateur: &str) -> Result<Vec<Brouillon>, DokosError> {
    const TYPES: [TypeVente; 3] = [TypeVente::Devis, TypeVente::Commande, TypeVente::Facture];
    let listes: Vec<Liste> = TYPES
        .iter()
        .map(|type_vente| {
            let champ_date = match type_vente {
                TypeVente::Facture => "posting_date",
                TypeVente::Devis | TypeVente::Commande => "transaction_date",
            };
            let champ_tiers = match type_vente {
                TypeVente::Devis => "party_name",
                TypeVente::Commande | TypeVente::Facture => "customer_name",
            };
            Liste::new(&["name", champ_tiers, champ_date, "grand_total", "currency", "modified"])
                .filtre(Filtre::egal("docstatus", 0))
                .filtre(Filtre::egal("owner", utilisateur))
                .filtre(crate::modele_vente::filtre_sans_modeles())
                .ordre("modified desc")
                .limite(BROUILLONS_MAX)
        })
        .collect();
    // Les trois listes en un lot : de front sur le réseau réel.
    let demandes: Vec<(&str, &Liste)> = TYPES.iter().map(|t| t.doctype()).zip(&listes).collect();
    let documents: Vec<Vec<VenteBrouillon>> = dokos.lister_lot(&demandes)?;
    let mut brouillons: Vec<Brouillon> = TYPES
        .iter()
        .zip(documents)
        .flat_map(|(type_vente, documents)| {
            documents.into_iter().map(|d| Brouillon {
                doctype: type_vente.doctype(),
                id: d.name,
                tiers: if d.customer_name.is_empty() { d.party_name } else { d.customer_name },
                date: d.date,
                total: d.grand_total,
                devise: d.currency,
                modifie: d.modified,
            })
        })
        .collect();
    brouillons.sort_by(|a, b| b.modifie.cmp(&a.modifie));
    Ok(brouillons)
}

/// Événements non publiés créés par `utilisateur`.
pub fn evenements_non_publies<T: Transport>(dokos: &Dokos<T>, utilisateur: &str) -> Result<Vec<Brouillon>, DokosError> {
    let liste = Liste::new(&["name", "subject", "starts_on", "modified"])
        .filtre(Filtre::egal("published", 0))
        .filtre(Filtre::different("status", "Cancelled"))
        .filtre(Filtre::egal("owner", utilisateur))
        .ordre("modified desc")
        .limite(BROUILLONS_MAX);
    let evenements: Vec<EvenementBrouillon> = dokos.lister(DOCTYPE_EVENEMENT, &liste)?;
    Ok(evenements
        .into_iter()
        .map(|e| Brouillon {
            doctype: DOCTYPE_EVENEMENT,
            id: e.name,
            tiers: e.subject,
            date: e.starts_on,
            total: 0.0,
            devise: String::new(),
            modifie: e.modified,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::faux::FauxTransport;

    #[test]
    fn brouillons_tries_par_date_de_modification() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "Q-1", "party_name": "Mairie", "transaction_date": "2026-11-02",
                               "grand_total": 10.0, "currency": "EUR", "modified": "2026-11-02 10:00:00"}]}),
        );
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "F-1", "customer_name": "CCAS", "posting_date": "2026-11-03",
                               "grand_total": 20.0, "currency": "EUR", "modified": "2026-11-03 09:00:00"}]}),
        );
        let brouillons = brouillons_de_vente(&Dokos::new(&faux), "api@exemple.fr").expect("brouillons");
        assert_eq!(brouillons.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["F-1", "Q-1"]);
        assert_eq!(brouillons[1].tiers, "Mairie");
        assert_eq!(brouillons[0].doctype, "Sales Invoice");
        let requete = &faux.requetes()[0];
        assert!(requete.parametre("filters").is_some_and(|f| f.contains(r#"["owner","=","api@exemple.fr"]"#)));
    }

    #[test]
    fn evenements_non_publies_listes() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "EV-1", "subject": "Atelier", "starts_on": "2026-12-14 14:00:00"}]}),
        );
        let brouillons = evenements_non_publies(&Dokos::new(&faux), "api@exemple.fr").expect("événements");
        assert_eq!(brouillons[0].tiers, "Atelier");
        assert_eq!(brouillons[0].doctype, "Event");
    }
}
