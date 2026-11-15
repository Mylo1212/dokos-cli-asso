//! Suivi en lecture seule : factures en retard de paiement. Les relances
//! restent dans Dokos.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::client::{Dokos, Filtre, Liste};
use crate::date::Date;
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::{DOCTYPE_FACTURE, texte_ou_vide};

pub const IMPAYEES_MAX: u32 = 200;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FactureImpayee {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub customer: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub customer_name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub posting_date: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub due_date: String,
    #[serde(default)]
    pub grand_total: f64,
    #[serde(default)]
    pub outstanding_amount: f64,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub currency: String,
}

impl FactureImpayee {
    /// Jours de retard à la date donnée (0 si l'échéance est illisible).
    pub fn retard(&self, aujourdhui: Date) -> i64 {
        self.due_date.parse::<Date>().map(|d| d.jours_jusqua(aujourdhui)).unwrap_or(0)
    }
}

/// Factures validées dont l'échéance est dépassée et le solde non nul,
/// de la plus ancienne à la plus récente.
pub fn factures_en_retard<T: Transport>(dokos: &Dokos<T>, aujourdhui: Date) -> Result<Vec<FactureImpayee>, DokosError> {
    let liste = Liste::new(&[
        "name",
        "customer",
        "customer_name",
        "posting_date",
        "due_date",
        "grand_total",
        "outstanding_amount",
        "currency",
    ])
    .filtre(Filtre::egal("docstatus", 1))
    .filtre(Filtre::superieur("outstanding_amount", 0))
    .filtre(Filtre::inferieur("due_date", aujourdhui.to_string()))
    .ordre("due_date asc")
    .limite(IMPAYEES_MAX);
    dokos.lister(DOCTYPE_FACTURE, &liste)
}

/// Solde dû par client, du plus élevé au plus faible.
pub fn soldes_par_client(factures: &[FactureImpayee]) -> Vec<(String, f64, usize)> {
    let mut soldes: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    for facture in factures {
        let nom = if facture.customer_name.is_empty() { &facture.customer } else { &facture.customer_name };
        let entree = soldes.entry(nom).or_insert((0.0, 0));
        entree.0 += facture.outstanding_amount;
        entree.1 += 1;
    }
    let mut liste: Vec<(String, f64, usize)> = soldes.into_iter().map(|(n, (s, c))| (n.to_owned(), s, c)).collect();
    liste.sort_by(|a, b| b.1.total_cmp(&a.1));
    liste
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::faux::FauxTransport;

    fn jour() -> Date {
        "2026-11-03".parse().expect("date")
    }

    #[test]
    fn factures_en_retard_et_soldes() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [
                {"name": "F-1", "customer": "C1", "customer_name": "Mairie", "due_date": "2026-10-04", "outstanding_amount": 100.0, "currency": "EUR"},
                {"name": "F-2", "customer": "C2", "customer_name": "CCAS", "due_date": "2026-10-24", "outstanding_amount": 250.0},
                {"name": "F-3", "customer": "C1", "customer_name": "Mairie", "due_date": "n'importe quoi", "outstanding_amount": 50.0},
            ]}),
        );
        let factures = factures_en_retard(&Dokos::new(&faux), jour()).expect("factures");
        assert_eq!(factures[0].retard(jour()), 30);
        assert_eq!(factures[2].retard(jour()), 0);
        let soldes = soldes_par_client(&factures);
        assert_eq!(soldes[0], ("CCAS".to_owned(), 250.0, 1));
        assert_eq!(soldes[1], ("Mairie".to_owned(), 150.0, 2));
        let filtres = faux.requetes()[0].parametre("filters").unwrap_or_default().to_owned();
        assert!(
            filtres.contains(r#"["outstanding_amount",">",0]"#) && filtres.contains(r#"["due_date","<","2026-11-03"]"#)
        );
    }
}
