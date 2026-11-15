//! Suivi en lecture seule : factures en retard de paiement, échéances de
//! contrats de travail. Les relances et les avenants restent dans Dokos.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::client::{Dokos, Filtre, Liste};
use crate::date::Date;
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::{DOCTYPE_FACTURE, texte_ou_vide};

/// Contrats de travail (application RH de Dokos), lus seulement pour les échéances.
pub const DOCTYPE_CONTRAT: &str = "Employment Contract";
/// Types de contrat à durée déterminée, dont la fin doit être renseignée.
pub const TYPES_CDD: &[&str] = &["CDD"];

pub const IMPAYEES_MAX: u32 = 200;
pub const CONTRATS_MAX: u32 = 200;

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EcheanceContrat {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub employee: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub employee_name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub contract_type: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub date_of_joining: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub relieving_date: String,
}

const CHAMPS_ECHEANCE: &[&str] =
    &["name", "employee", "employee_name", "contract_type", "date_of_joining", "relieving_date"];

/// Contrats dont la fin tombe entre aujourd'hui et `jours` plus tard.
pub fn contrats_qui_se_terminent<T: Transport>(
    dokos: &Dokos<T>,
    aujourdhui: Date,
    jours: i64,
) -> Result<Vec<EcheanceContrat>, DokosError> {
    dokos.lister(DOCTYPE_CONTRAT, &liste_fins(aujourdhui, jours)?)
}

fn liste_fins(aujourdhui: Date, jours: i64) -> Result<Liste, DokosError> {
    let fin = aujourdhui.plus_jours(jours).map_err(|e| DokosError::Configuration(e.to_string()))?;
    Ok(Liste::new(CHAMPS_ECHEANCE)
        .filtre(Filtre::entre("relieving_date", aujourdhui.to_string(), fin.to_string()))
        .ordre("relieving_date asc")
        .limite(CONTRATS_MAX))
}

/// Contrats d'un des types donnés (voir [`TYPES_CDD`]) sans date de fin.
pub fn contrats_sans_fin<T: Transport>(dokos: &Dokos<T>, types: &[&str]) -> Result<Vec<EcheanceContrat>, DokosError> {
    match liste_sans_fin(types) {
        Some(liste) => dokos.lister(DOCTYPE_CONTRAT, &liste),
        None => Ok(Vec::new()),
    }
}

fn liste_sans_fin(types: &[&str]) -> Option<Liste> {
    if types.is_empty() {
        return None;
    }
    Some(
        Liste::new(CHAMPS_ECHEANCE)
            .filtre(Filtre::parmi("contract_type", types))
            .filtre(Filtre::non_renseigne("relieving_date"))
            .ordre("date_of_joining desc")
            .limite(CONTRATS_MAX),
    )
}

/// Contrats dont l'entrée est dans moins de `jours` jours depuis aujourd'hui :
/// la période d'essai court encore ou commence (à confirmer dans le contrat).
pub fn contrats_recents<T: Transport>(
    dokos: &Dokos<T>,
    aujourdhui: Date,
    jours: i64,
) -> Result<Vec<EcheanceContrat>, DokosError> {
    dokos.lister(DOCTYPE_CONTRAT, &liste_recents(aujourdhui, jours)?)
}

fn liste_recents(aujourdhui: Date, jours: i64) -> Result<Liste, DokosError> {
    let debut = aujourdhui.plus_jours(-jours).map_err(|e| DokosError::Configuration(e.to_string()))?;
    Ok(Liste::new(CHAMPS_ECHEANCE)
        .filtre(Filtre::entre("date_of_joining", debut.to_string(), aujourdhui.to_string()))
        .ordre("date_of_joining desc")
        .limite(CONTRATS_MAX))
}

/// Les trois listes d'échéances de contrats, lues ensemble.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Echeances {
    /// Fins de contrat dans l'horizon demandé.
    pub fins: Vec<EcheanceContrat>,
    /// CDD sans date de fin.
    pub sans_fin: Vec<EcheanceContrat>,
    /// Contrats récents (période d'essai probable).
    pub recents: Vec<EcheanceContrat>,
}

/// [`contrats_qui_se_terminent`], [`contrats_sans_fin`] et [`contrats_recents`]
/// en un lot : de front sur le réseau réel, au lieu de trois allers-retours.
pub fn echeances_de_contrats<T: Transport>(
    dokos: &Dokos<T>,
    aujourdhui: Date,
    horizon: i64,
    types_cdd: &[&str],
    recent: i64,
) -> Result<Echeances, DokosError> {
    let fins = liste_fins(aujourdhui, horizon)?;
    let sans_fin = liste_sans_fin(types_cdd);
    let recents = liste_recents(aujourdhui, recent)?;
    let mut demandes = vec![(DOCTYPE_CONTRAT, &fins)];
    demandes.extend(sans_fin.as_ref().map(|liste| (DOCTYPE_CONTRAT, liste)));
    demandes.push((DOCTYPE_CONTRAT, &recents));
    let mut resultats = dokos.lister_lot::<EcheanceContrat>(&demandes)?.into_iter();
    let mut suivante = || resultats.next().unwrap_or_default();
    Ok(Echeances {
        fins: suivante(),
        sans_fin: if sans_fin.is_some() { suivante() } else { Vec::new() },
        recents: suivante(),
    })
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

    #[test]
    fn echeances_de_contrats() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "C-1", "employee_name": "Marie", "relieving_date": "2026-11-20"}]}),
        );
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": [{"name": "C-2", "date_of_joining": "2026-10-15"}]}));
        let dokos = Dokos::new(&faux);
        let fins = contrats_qui_se_terminent(&dokos, jour(), 30).expect("fins");
        assert_eq!(fins[0].id, "C-1");
        assert!(contrats_sans_fin(&dokos, TYPES_CDD).expect("sans fin").is_empty());
        assert!(contrats_sans_fin(&dokos, &[]).expect("aucun type").is_empty());
        assert_eq!(contrats_recents(&dokos, jour(), 60).expect("récents")[0].id, "C-2");
        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 3);
        assert!(
            requetes[0]
                .parametre("filters")
                .is_some_and(|f| f.contains(r#"["relieving_date","between",["2026-11-03","2026-12-03"]]"#))
        );
        assert!(requetes[1].parametre("filters").is_some_and(|f| f.contains(r#"["relieving_date","is","not set"]"#)));
    }
}
