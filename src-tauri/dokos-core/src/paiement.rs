//! Écritures de paiement (DocType `Payment Entry`) en règlement d'une ou de
//! plusieurs factures de vente validées d'un même client, avec le montant
//! affecté à chacune. **Toujours créées en brouillon** : l'outil ne valide
//! jamais, les factures ne sont soldées qu'à la validation dans Dokos.
//!
//! Dokos prépare l'écriture lui-même (`get_payment_entry` : client, comptes,
//! devises, lien vers la facture), sans l'enregistrer, une fois par facture.
//! L'outil vérifie que chaque préparation porte bien le montant demandé, les
//! réunit en une seule écriture (même client, même compte, devise de la
//! société), y pose la date, le mode de paiement et la référence, puis la crée
//! en brouillon. Le journal comptable n'est pas
//! envoyé : Dokos l'attribue à l'enregistrement, d'après les règles de ses
//! journaux.
//!
//! Lecture seule hors création : factures à régler, modes de paiement et
//! comptes d'encaissement sont lus dans Dokos.

use std::fmt;

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

use crate::client::{Dokos, Filtre, Liste, lister_noms};
use crate::date::Date;
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::transformation::nettoyer_document_prepare;
use crate::transport::Transport;
use crate::types::{DOCTYPE_FACTURE, texte_ou_vide};

pub const DOCTYPE_PAIEMENT: &str = "Payment Entry";
pub const DOCTYPE_MODE_PAIEMENT: &str = "Mode of Payment";
pub const DOCTYPE_COMPTE: &str = "Account";
/// Table enfant d'une écriture de paiement qui désigne les factures réglées.
const DOCTYPE_REFERENCE_PAIEMENT: &str = "Payment Entry Reference";

/// Méthodes de Dokos (ERPNext) appelées par l'API.
const METHODE_PREPARER: &str = "erpnext.accounts.doctype.payment_entry.payment_entry.get_payment_entry";
const METHODE_COMPTE_DU_MODE: &str = "erpnext.accounts.doctype.sales_invoice.sales_invoice.get_bank_cash_account";

/// Factures à régler lues au plus.
pub const FACTURES_MAX: usize = 5_000;
/// Modes de paiement lus au plus.
const MODES_MAX: u32 = 200;
/// Montant maximal d'un paiement.
pub const MONTANT_MAX: f64 = 1_000_000_000.0;
/// Longueur maximale de la référence (champ « Data » de Dokos).
pub const REFERENCE_MAX: usize = 140;
/// Type de compte qui rend la référence obligatoire dans Dokos.
const TYPE_COMPTE_BANQUE: &str = "Bank";
/// Écart toléré entre deux montants (Frappe stocke des flottants).
const TOLERANCE: f64 = 0.005;

fn nombre_ou_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(0.0))
}

fn arrondi(montant: f64) -> f64 {
    (montant * 100.0).round() / 100.0
}

// --- Lectures -----------------------------------------------------------------

/// Facture de vente validée dont le solde n'est pas nul.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FactureARegler {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "customer", default, deserialize_with = "texte_ou_vide")]
    pub client: String,
    #[serde(rename = "customer_name", default, deserialize_with = "texte_ou_vide")]
    pub nom_client: String,
    #[serde(rename = "posting_date", default, deserialize_with = "texte_ou_vide")]
    pub date: String,
    #[serde(rename = "outstanding_amount", default, deserialize_with = "nombre_ou_zero")]
    pub reste_du: f64,
    #[serde(rename = "currency", default, deserialize_with = "texte_ou_vide")]
    pub devise: String,
    /// Société de la facture, pour trouver le compte d'encaissement du mode de paiement.
    #[serde(rename = "company", default, deserialize_with = "texte_ou_vide")]
    pub societe: String,
}

impl FactureARegler {
    /// « Mairie de Crest — 31/10/2026 — reste dû 120.00 EUR », pour les listes et les messages.
    pub fn detail(&self) -> String {
        let client = if self.nom_client.is_empty() { &self.client } else { &self.nom_client };
        format!("{client} — {} — reste dû {:.2} {}", self.date, self.reste_du, self.devise).trim_end().to_owned()
    }
}

/// Factures validées avec un solde, de la plus ancienne à la plus récente.
pub fn lister_factures_a_regler<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<FactureARegler>, DokosError> {
    let liste =
        Liste::new(&["name", "customer", "customer_name", "posting_date", "outstanding_amount", "currency", "company"])
            .filtre(Filtre::egal("docstatus", 1))
            .filtre(Filtre::superieur("outstanding_amount", 0))
            .ordre("posting_date asc, name asc");
    dokos.lister_tout(DOCTYPE_FACTURE, &liste, FACTURES_MAX)
}

/// Modes de paiement actifs, par ordre alphabétique.
pub fn lister_modes_de_paiement<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_MODE_PAIEMENT, vec![Filtre::egal("enabled", 1)], MODES_MAX)
}

/// Compte sur lequel un mode de paiement encaisse, dans une société.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompteEncaissement {
    /// Identifiant Dokos du compte (« 512 - Banque - SLA »).
    pub compte: String,
    /// `Bank`, `Cash`… (vide si Dokos ne l'indique pas).
    pub type_compte: String,
}

impl CompteEncaissement {
    /// Dokos exige une référence (numéro de chèque, de virement…) sur un compte bancaire.
    pub fn reference_obligatoire(&self) -> bool {
        self.type_compte == TYPE_COMPTE_BANQUE
    }
}

/// Compte d'encaissement d'un mode de paiement dans la société, tel que Dokos
/// le configure. Erreur de Dokos si le mode n'a aucun compte pour cette société.
pub fn compte_du_mode<T: Transport>(
    dokos: &Dokos<T>,
    mode: &str,
    societe: &str,
) -> Result<CompteEncaissement, DokosError> {
    #[derive(Deserialize)]
    struct Compte {
        account: Option<String>,
    }
    #[derive(Deserialize)]
    struct TypeDeCompte {
        #[serde(default, deserialize_with = "texte_ou_vide")]
        account_type: String,
    }
    let compte: Compte =
        dokos.methode_post(METHODE_COMPTE_DU_MODE, json!({"mode_of_payment": mode, "company": societe}))?;
    let compte = compte.account.filter(|c| !c.trim().is_empty()).ok_or_else(|| {
        DokosError::Validation(format!("le mode de paiement « {mode} » n'a pas de compte pour {societe}"))
    })?;
    let details: TypeDeCompte = dokos.lire(DOCTYPE_COMPTE, &compte)?;
    Ok(CompteEncaissement { compte, type_compte: details.account_type })
}

// --- Demande validée ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurPaiement {
    /// Aucune facture à régler.
    AucuneFacture,
    FactureVide,
    /// La même facture deux fois dans un paiement.
    FactureEnDouble(String),
    ModeVide,
    MontantInvalide,
    ReferenceTropLongue(usize),
    /// Compte bancaire : Dokos refuse un paiement sans référence.
    ReferenceObligatoire,
}

impl fmt::Display for ErreurPaiement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AucuneFacture => write!(f, "aucune facture à régler"),
            Self::FactureVide => write!(f, "facture manquante"),
            Self::FactureEnDouble(facture) => {
                write!(f, "facture {facture} présente deux fois dans le même paiement : regroupez ses montants")
            }
            Self::ModeVide => write!(f, "mode de paiement manquant"),
            Self::MontantInvalide => write!(f, "montant invalide : un nombre supérieur à zéro est attendu"),
            Self::ReferenceTropLongue(n) => {
                write!(f, "référence trop longue : {n} caractères (maximum {REFERENCE_MAX})")
            }
            Self::ReferenceObligatoire => {
                write!(f, "référence obligatoire pour un paiement encaissé sur un compte bancaire")
            }
        }
    }
}

impl std::error::Error for ErreurPaiement {}

/// Part d'un paiement affectée à une facture.
#[derive(Debug, Clone, PartialEq)]
pub struct Affectation {
    pub facture: String,
    /// Arrondi au centime, supérieur à zéro.
    pub montant: f64,
}

/// Paiement d'une ou de plusieurs factures, validé avant tout envoi à Dokos.
#[derive(Debug, Clone, PartialEq)]
pub struct NouveauPaiement {
    affectations: Vec<Affectation>,
    date: Date,
    mode: String,
    compte: CompteEncaissement,
    reference: Option<String>,
}

impl NouveauPaiement {
    /// `affectations` : (identifiant d'une facture validée, montant affecté),
    /// dans l'ordre ; montants arrondis au centime. `compte` : celui du mode de
    /// paiement ([`compte_du_mode`]).
    pub fn new(
        affectations: &[(&str, f64)],
        date: Date,
        mode: &str,
        compte: CompteEncaissement,
        reference: Option<&str>,
    ) -> Result<Self, ErreurPaiement> {
        if affectations.is_empty() {
            return Err(ErreurPaiement::AucuneFacture);
        }
        let mut validees: Vec<Affectation> = Vec::with_capacity(affectations.len());
        for (facture, montant) in affectations {
            let facture = facture.trim();
            if facture.is_empty() {
                return Err(ErreurPaiement::FactureVide);
            }
            if validees.iter().any(|a| a.facture == facture) {
                return Err(ErreurPaiement::FactureEnDouble(facture.to_owned()));
            }
            let montant = arrondi(*montant);
            if !montant.is_finite() || montant <= 0.0 || montant > MONTANT_MAX {
                return Err(ErreurPaiement::MontantInvalide);
            }
            validees.push(Affectation { facture: facture.to_owned(), montant });
        }
        if validees.iter().map(|a| a.montant).sum::<f64>() > MONTANT_MAX {
            return Err(ErreurPaiement::MontantInvalide);
        }
        let mode = mode.trim();
        if mode.is_empty() {
            return Err(ErreurPaiement::ModeVide);
        }
        let reference = reference.map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned);
        if let Some(r) = &reference {
            let longueur = r.chars().count();
            if longueur > REFERENCE_MAX {
                return Err(ErreurPaiement::ReferenceTropLongue(longueur));
            }
        }
        if reference.is_none() && compte.reference_obligatoire() {
            return Err(ErreurPaiement::ReferenceObligatoire);
        }
        Ok(Self { affectations: validees, date, mode: mode.to_owned(), compte, reference })
    }

    pub fn affectations(&self) -> &[Affectation] {
        &self.affectations
    }

    /// Factures réglées, dans l'ordre.
    pub fn factures(&self) -> Vec<&str> {
        self.affectations.iter().map(|a| a.facture.as_str()).collect()
    }

    pub fn date(&self) -> Date {
        self.date
    }

    /// Montant total du paiement.
    pub fn montant(&self) -> f64 {
        arrondi(self.affectations.iter().map(|a| a.montant).sum())
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn compte(&self) -> &str {
        &self.compte.compte
    }

    pub fn reference(&self) -> Option<&str> {
        self.reference.as_deref()
    }
}

// --- Doublon et création -----------------------------------------------------

/// Écriture de paiement déjà présente dans Dokos.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PaiementExistant {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(default)]
    pub docstatus: u8,
    #[serde(rename = "posting_date", default, deserialize_with = "texte_ou_vide")]
    pub date: String,
    #[serde(rename = "paid_amount", default, deserialize_with = "nombre_ou_zero")]
    pub montant: f64,
}

impl PaiementExistant {
    pub fn statut(&self) -> &'static str {
        match self.docstatus {
            0 => "brouillon",
            1 => "validé",
            _ => "annulé",
        }
    }
}

/// Paiement identique non annulé : même première facture, même date, même montant total.
pub fn chercher_paiement_identique<T: Transport>(
    dokos: &Dokos<T>,
    paiement: &NouveauPaiement,
) -> Result<Option<PaiementExistant>, DokosError> {
    let liste = Liste::new(&["name", "docstatus", "posting_date", "paid_amount"])
        .filtre(Filtre::different("docstatus", 2))
        .filtre(Filtre::egal("payment_type", "Receive"))
        .filtre(Filtre::egal("posting_date", paiement.date.to_string()))
        .filtre(Filtre::egal("paid_amount", paiement.montant()))
        .filtre(Filtre::enfant(DOCTYPE_REFERENCE_PAIEMENT, "reference_name", paiement.affectations[0].facture.as_str()))
        .limite(1);
    Ok(dokos.lister::<PaiementExistant>(DOCTYPE_PAIEMENT, &liste)?.into_iter().next())
}

/// Retour de Dokos après création de l'écriture.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PaiementCree {
    #[serde(rename = "name")]
    pub id: String,
    pub docstatus: u8,
    #[serde(rename = "paid_amount", default, deserialize_with = "nombre_ou_zero")]
    pub montant: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreationPaiement {
    Cree(PaiementCree),
    /// Rien n'a été écrit : un paiement identique existait déjà.
    DejaExistant(PaiementExistant),
}

fn proche(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE
}

fn nombre(valeur: Option<&Value>) -> f64 {
    valeur.and_then(Value::as_f64).unwrap_or(0.0)
}

/// Vérifie l'écriture préparée par Dokos pour une facture : un encaissement
/// qui ne règle que cette facture, du montant demandé.
fn verifier_preparation(ecriture: &Map<String, Value>, affectation: &Affectation) -> Result<(), DokosError> {
    let inattendu = |detail: String| {
        DokosError::ReponseInvalide(format!("Dokos n'a pas préparé le paiement de {} : {detail}", affectation.facture))
    };
    if ecriture.get("doctype").and_then(Value::as_str) != Some(DOCTYPE_PAIEMENT) {
        return Err(inattendu("ce n'est pas une écriture de paiement".to_owned()));
    }
    if ecriture.get("payment_type").and_then(Value::as_str) != Some("Receive") {
        return Err(inattendu("ce n'est pas un encaissement".to_owned()));
    }
    let references = ecriture.get("references").and_then(Value::as_array).cloned().unwrap_or_default();
    if references.is_empty()
        || references
            .iter()
            .any(|r| r.get("reference_name").and_then(Value::as_str) != Some(affectation.facture.as_str()))
    {
        return Err(inattendu("la facture n'est pas la seule référence".to_owned()));
    }
    let alloue: f64 = references.iter().map(|r| nombre(r.get("allocated_amount"))).sum();
    let paye = nombre(ecriture.get("paid_amount"));
    if !proche(alloue, affectation.montant) || !proche(paye, affectation.montant) {
        return Err(inattendu(format!(
            "montant payé {paye:.2}, réparti {alloue:.2}, demandé {:.2} (facture déjà réglée ou en devise étrangère ?)",
            affectation.montant
        )));
    }
    Ok(())
}

/// Champs qui doivent être les mêmes dans les écritures préparées pour chaque
/// facture d'un même paiement.
const CHAMPS_COMMUNS: [&str; 5] = ["party_type", "party", "company", "paid_from", "paid_to"];
/// Montants de l'en-tête, recalculés pour le total d'un paiement à plusieurs factures.
const MONTANTS_TOTAUX: [&str; 6] = [
    "paid_amount",
    "received_amount",
    "base_paid_amount",
    "base_received_amount",
    "total_allocated_amount",
    "base_total_allocated_amount",
];

/// Une seule écriture à partir des préparations de Dokos (une par facture, dans
/// l'ordre des affectations), complétée de ce que l'outil décide : date, mode de
/// paiement, référence.
fn assembler_ecriture(preparees: Vec<Map<String, Value>>, paiement: &NouveauPaiement) -> Result<Value, DokosError> {
    for (ecriture, affectation) in preparees.iter().zip(&paiement.affectations) {
        verifier_preparation(ecriture, affectation)?;
    }
    let mut preparees = preparees.into_iter();
    let mut ecriture =
        preparees.next().ok_or_else(|| DokosError::ReponseInvalide("aucune écriture préparée".to_owned()))?;
    let autres: Vec<Map<String, Value>> = preparees.collect();
    if !autres.is_empty() {
        let factures = paiement.factures().join(", ");
        let refuse = |detail: &str| {
            DokosError::ReponseInvalide(format!("paiement de {factures} impossible en une seule écriture : {detail}"))
        };
        for autre in &autres {
            if let Some(champ) = CHAMPS_COMMUNS.iter().find(|c| autre.get(**c) != ecriture.get(**c)) {
                return Err(refuse(&format!(
                    "« {champ} » diffère d'une facture à l'autre (client, société ou compte)"
                )));
            }
        }
        // Un taux de change autre que 1 : montants de l'en-tête à convertir, ce que l'outil ne fait pas.
        let taux_unitaire =
            |e: &Map<String, Value>, champ: &str| e.get(champ).and_then(Value::as_f64).is_none_or(|t| proche(t, 1.0));
        if [&ecriture]
            .into_iter()
            .chain(&autres)
            .any(|e| !taux_unitaire(e, "source_exchange_rate") || !taux_unitaire(e, "target_exchange_rate"))
        {
            return Err(refuse("facture en devise étrangère : un paiement par facture"));
        }
        let mut references = ecriture.get("references").and_then(Value::as_array).cloned().unwrap_or_default();
        for autre in &autres {
            references.extend(autre.get("references").and_then(Value::as_array).cloned().unwrap_or_default());
        }
        ecriture.insert("references".to_owned(), Value::Array(references));
        let total = paiement.montant();
        for champ in MONTANTS_TOTAUX {
            ecriture.insert(champ.to_owned(), json!(total));
        }
        ecriture.insert("unallocated_amount".to_owned(), json!(0.0));
        ecriture.insert("difference_amount".to_owned(), json!(0.0));
    }
    ecriture.insert("posting_date".to_owned(), json!(paiement.date.to_string()));
    ecriture.insert("mode_of_payment".to_owned(), json!(paiement.mode));
    if let Some(reference) = &paiement.reference {
        ecriture.insert("reference_no".to_owned(), json!(reference));
        ecriture.insert("reference_date".to_owned(), json!(paiement.date.to_string()));
    }
    Ok(nettoyer_document_prepare(ecriture))
}

/// Crée l'écriture de paiement **en brouillon**, après le contrôle de doublon.
pub fn creer_paiement<T: Transport>(
    dokos: &Dokos<T>,
    paiement: &NouveauPaiement,
    si_doublon: SiDoublon,
) -> Result<CreationPaiement, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = chercher_paiement_identique(dokos, paiement)?
    {
        return Ok(CreationPaiement::DejaExistant(existant));
    }
    // Dokos prépare l'écriture de chaque facture, avec le montant qui lui est affecté.
    let mut preparees = Vec::with_capacity(paiement.affectations.len());
    for affectation in &paiement.affectations {
        let mut demande = json!({
            "dt": DOCTYPE_FACTURE,
            "dn": affectation.facture,
            "party_amount": affectation.montant,
            "bank_account": paiement.compte.compte,
        });
        if paiement.reference.is_some() {
            demande["reference_date"] = json!(paiement.date.to_string());
        }
        preparees.push(dokos.methode_post::<Map<String, Value>>(METHODE_PREPARER, demande)?);
    }
    let corps = assembler_ecriture(preparees, paiement)?;
    let cree: PaiementCree = dokos.creer(DOCTYPE_PAIEMENT, corps)?;
    if cree.docstatus != 0 {
        return Err(DokosError::ReponseInvalide(format!(
            "le paiement {} n'est pas en brouillon (docstatus {})",
            cree.id, cree.docstatus
        )));
    }
    Ok(CreationPaiement::Cree(cree))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn jour() -> Date {
        "2026-11-05".parse().expect("date")
    }

    fn compte(type_compte: &str) -> CompteEncaissement {
        CompteEncaissement { compte: "512 - Banque - SLA".into(), type_compte: type_compte.into() }
    }

    fn paiement(montant: f64, reference: Option<&str>) -> NouveauPaiement {
        NouveauPaiement::new(&[("ACC-SINV-1", montant)], jour(), "Virement", compte("Bank"), reference)
            .expect("paiement")
    }

    /// Ce que `get_payment_entry` renvoie : une écriture non enregistrée.
    fn ecriture_preparee(montant: f64) -> Value {
        json!({"message": {
            "doctype": "Payment Entry", "name": "new-payment-entry-1", "__islocal": 1, "docstatus": 0,
            "payment_type": "Receive", "party_type": "Customer", "party": "CUST-2", "company": "Les Amanins",
            "paid_to": "512 - Banque - SLA", "paid_amount": montant, "received_amount": montant,
            "posting_date": "2026-11-01",
            "references": [{"__islocal": 1, "name": "new-ref-1", "reference_doctype": "Sales Invoice",
                            "reference_name": "ACC-SINV-1", "allocated_amount": montant, "outstanding_amount": 200.0}],
        }})
    }

    #[test]
    fn validation_de_la_demande() {
        assert_eq!(
            NouveauPaiement::new(&[(" ", 10.0)], jour(), "Espèces", compte("Cash"), None),
            Err(ErreurPaiement::FactureVide)
        );
        assert_eq!(
            NouveauPaiement::new(&[("F-1", 10.0)], jour(), " ", compte("Cash"), None),
            Err(ErreurPaiement::ModeVide)
        );
        for montant in [0.0, -5.0, f64::NAN, f64::INFINITY, MONTANT_MAX * 2.0, 0.004] {
            assert_eq!(
                NouveauPaiement::new(&[("F-1", montant)], jour(), "Espèces", compte("Cash"), None),
                Err(ErreurPaiement::MontantInvalide),
                "{montant}"
            );
        }
        let p =
            NouveauPaiement::new(&[("F-1", 12.346)], jour(), "Espèces", compte("Cash"), Some("  ")).expect("espèces");
        assert_eq!((p.montant(), p.reference()), (12.35, None), "arrondi au centime, référence vide ignorée");
        // Compte bancaire : référence obligatoire ; espèces : facultative.
        assert_eq!(
            NouveauPaiement::new(&[("F-1", 10.0)], jour(), "Virement", compte("Bank"), None),
            Err(ErreurPaiement::ReferenceObligatoire)
        );
        assert!(NouveauPaiement::new(&[("F-1", 10.0)], jour(), "Virement", compte("Bank"), Some("VIR-9")).is_ok());
        assert!(matches!(
            NouveauPaiement::new(&[("F-1", 10.0)], jour(), "Chèque", compte("Cash"), Some(&"x".repeat(141))),
            Err(ErreurPaiement::ReferenceTropLongue(141))
        ));
    }

    #[test]
    fn creation_en_brouillon() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []})); // doublons
        faux.repondre_json(200, &ecriture_preparee(120.0));
        faux.repondre_json(200, &json!({"data": {"name": "ACC-PAY-0001", "docstatus": 0, "paid_amount": 120.0}}));
        let dokos = Dokos::new(&faux);
        let resultat =
            creer_paiement(&dokos, &paiement(120.0, Some("VIR-2026-9")), SiDoublon::Reutiliser).expect("création");
        assert!(matches!(resultat, CreationPaiement::Cree(c) if c.id == "ACC-PAY-0001" && c.docstatus == 0));

        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 3);
        let filtres = requetes[0].parametre("filters").unwrap_or_default().to_owned();
        for attendu in [
            r#"["docstatus","!=",2]"#,
            r#"["posting_date","=","2026-11-05"]"#,
            r#"["paid_amount","=",120.0]"#,
            r#"["Payment Entry Reference","reference_name","=","ACC-SINV-1"]"#,
        ] {
            assert!(filtres.contains(attendu), "{attendu} absent de {filtres}");
        }
        // Dokos prépare l'écriture : facture, montant, compte d'encaissement.
        assert_eq!(requetes[1].methode, Methode::Post);
        assert!(requetes[1].chemin.ends_with("payment_entry.get_payment_entry"), "{}", requetes[1].chemin);
        let demande = requetes[1].corps.clone().expect("corps");
        assert_eq!(demande["dt"], "Sales Invoice");
        assert_eq!(demande["dn"], "ACC-SINV-1");
        assert_eq!(demande["party_amount"], 120.0);
        assert_eq!(demande["bank_account"], "512 - Banque - SLA");
        // L'écriture envoyée : nettoyée, complétée, en brouillon.
        assert_eq!(requetes[2].methode, Methode::Post);
        assert_eq!(requetes[2].chemin, "/api/resource/Payment%20Entry");
        let corps = requetes[2].corps.clone().expect("corps");
        assert_eq!(corps["posting_date"], "2026-11-05");
        assert_eq!(corps["mode_of_payment"], "Virement");
        assert_eq!(
            (corps["reference_no"].as_str(), corps["reference_date"].as_str()),
            (Some("VIR-2026-9"), Some("2026-11-05"))
        );
        assert_eq!(corps["docstatus"], 0);
        assert!(corps.get("name").is_none() && corps.get("__islocal").is_none());
        assert!(corps["references"][0].get("name").is_none());
        assert_eq!(corps["references"][0]["reference_name"], "ACC-SINV-1");
        assert_eq!(faux.reponses_restantes(), 0);
    }

    #[test]
    fn plusieurs_factures_dans_une_demande() {
        let p = NouveauPaiement::new(&[("F-1", 100.0), ("F-2", 20.004)], jour(), "Chèque", compte("Cash"), None)
            .expect("deux factures");
        assert_eq!(p.factures(), ["F-1", "F-2"]);
        assert_eq!(p.montant(), 120.0);
        assert_eq!(
            NouveauPaiement::new(&[("F-1", 10.0), (" F-1 ", 5.0)], jour(), "Chèque", compte("Cash"), None),
            Err(ErreurPaiement::FactureEnDouble("F-1".into()))
        );
        assert_eq!(
            NouveauPaiement::new(&[], jour(), "Chèque", compte("Cash"), None),
            Err(ErreurPaiement::AucuneFacture)
        );
        assert_eq!(
            NouveauPaiement::new(&[("F-1", 10.0), ("F-2", 0.0)], jour(), "Chèque", compte("Cash"), None),
            Err(ErreurPaiement::MontantInvalide)
        );
    }

    /// Préparation de Dokos pour une facture d'un même client.
    fn preparee(facture: &str, montant: f64, client: &str) -> Value {
        json!({"message": {
            "doctype": "Payment Entry", "docstatus": 0, "payment_type": "Receive", "party_type": "Customer",
            "party": client, "company": "Les Amanins", "paid_from": "411 - Clients - SLA",
            "paid_to": "512 - Banque - SLA", "source_exchange_rate": 1.0, "target_exchange_rate": 1.0,
            "paid_amount": montant, "received_amount": montant, "base_paid_amount": montant,
            "total_allocated_amount": montant, "unallocated_amount": 0.0,
            "references": [{"reference_doctype": "Sales Invoice", "reference_name": facture, "allocated_amount": montant}],
        }})
    }

    #[test]
    fn un_paiement_pour_plusieurs_factures() {
        let p = NouveauPaiement::new(
            &[("ACC-SINV-1", 100.0), ("ACC-SINV-2", 20.0)],
            jour(),
            "Virement",
            compte("Bank"),
            Some("VIR-7"),
        )
        .expect("paiement");
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []})); // doublons
        faux.repondre_json(200, &preparee("ACC-SINV-1", 100.0, "CUST-2"));
        faux.repondre_json(200, &preparee("ACC-SINV-2", 20.0, "CUST-2"));
        faux.repondre_json(200, &json!({"data": {"name": "ACC-PAY-0010", "docstatus": 0, "paid_amount": 120.0}}));
        let resultat = creer_paiement(&Dokos::new(&faux), &p, SiDoublon::Reutiliser).expect("création");
        assert!(matches!(resultat, CreationPaiement::Cree(c) if c.id == "ACC-PAY-0010"));

        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 4);
        // Doublon cherché sur le total et la première facture.
        let filtres = requetes[0].parametre("filters").unwrap_or_default().to_owned();
        assert!(
            filtres.contains(r#"["paid_amount","=",120.0]"#)
                && filtres.contains(r#""reference_name","=","ACC-SINV-1""#)
        );
        // Une préparation par facture, avec le montant affecté.
        let demandes: Vec<Value> = requetes[1..3].iter().filter_map(|r| r.corps.clone()).collect();
        assert_eq!(
            (demandes[0]["dn"].as_str(), demandes[0]["party_amount"].as_f64()),
            (Some("ACC-SINV-1"), Some(100.0))
        );
        assert_eq!(
            (demandes[1]["dn"].as_str(), demandes[1]["party_amount"].as_f64()),
            (Some("ACC-SINV-2"), Some(20.0))
        );
        // Une seule écriture : deux références, montants de l'en-tête au total.
        let corps = requetes[3].corps.clone().expect("corps");
        let references: Vec<(&str, f64)> = corps["references"]
            .as_array()
            .expect("références")
            .iter()
            .map(|r| {
                (r["reference_name"].as_str().unwrap_or_default(), r["allocated_amount"].as_f64().unwrap_or_default())
            })
            .collect();
        assert_eq!(references, [("ACC-SINV-1", 100.0), ("ACC-SINV-2", 20.0)]);
        for champ in ["paid_amount", "received_amount", "base_paid_amount", "total_allocated_amount"] {
            assert_eq!(corps[champ], 120.0, "{champ}");
        }
        assert_eq!(corps["unallocated_amount"], 0.0);
        assert_eq!(
            (corps["mode_of_payment"].as_str(), corps["reference_no"].as_str()),
            (Some("Virement"), Some("VIR-7"))
        );
        assert_eq!(corps["docstatus"], 0);
    }

    #[test]
    fn factures_de_clients_differents_refusees() {
        let p = NouveauPaiement::new(
            &[("ACC-SINV-1", 100.0), ("ACC-SINV-2", 20.0)],
            jour(),
            "Espèces",
            compte("Cash"),
            None,
        )
        .expect("paiement");
        let faux = FauxTransport::new();
        faux.repondre_json(200, &preparee("ACC-SINV-1", 100.0, "CUST-2"));
        faux.repondre_json(200, &preparee("ACC-SINV-2", 20.0, "CUST-9"));
        let erreur = creer_paiement(&Dokos::new(&faux), &p, SiDoublon::CreerQuandMeme).expect_err("refusé");
        assert!(matches!(&erreur, DokosError::ReponseInvalide(m) if m.contains("party")), "{erreur}");
        assert!(faux.requetes().iter().all(|r| !r.chemin.starts_with("/api/resource/")), "rien n'est créé");

        // Devise étrangère : refusé aussi, les montants de l'en-tête ne sont pas convertis.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &preparee("ACC-SINV-1", 100.0, "CUST-2"));
        let mut devise = preparee("ACC-SINV-2", 20.0, "CUST-2");
        devise["message"]["source_exchange_rate"] = json!(1.08);
        faux.repondre_json(200, &devise);
        let erreur = creer_paiement(&Dokos::new(&faux), &p, SiDoublon::CreerQuandMeme).expect_err("devise");
        assert!(matches!(&erreur, DokosError::ReponseInvalide(m) if m.contains("devise")), "{erreur}");
    }

    #[test]
    fn paiement_identique_deja_present() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "ACC-PAY-0007", "docstatus": 0, "posting_date": "2026-11-05", "paid_amount": 120.0}]}),
        );
        let resultat =
            creer_paiement(&Dokos::new(&faux), &paiement(120.0, Some("V")), SiDoublon::Reutiliser).expect("doublon");
        assert!(
            matches!(&resultat, CreationPaiement::DejaExistant(e) if e.id == "ACC-PAY-0007" && e.statut() == "brouillon")
        );
        assert_eq!(faux.requetes().len(), 1, "rien n'est préparé ni créé");

        // Créer quand même : pas de recherche, création directe.
        faux.repondre_json(200, &ecriture_preparee(120.0));
        faux.repondre_json(200, &json!({"data": {"name": "ACC-PAY-0008", "docstatus": 0, "paid_amount": 120.0}}));
        let resultat = creer_paiement(&Dokos::new(&faux), &paiement(120.0, Some("V")), SiDoublon::CreerQuandMeme)
            .expect("création");
        assert!(matches!(resultat, CreationPaiement::Cree(c) if c.id == "ACC-PAY-0008"));
    }

    #[test]
    fn ecriture_non_conforme_refusee_avant_creation() {
        for (preparee, attendu) in [
            (ecriture_preparee(80.0), "montant payé 80.00"),
            (json!({"message": {"doctype": "Journal Entry"}}), "pas une écriture de paiement"),
            (json!({"message": {"doctype": "Payment Entry", "payment_type": "Pay"}}), "pas un encaissement"),
            (
                json!({"message": {"doctype": "Payment Entry", "payment_type": "Receive", "references": []}}),
                "seule référence",
            ),
        ] {
            let faux = FauxTransport::new();
            faux.repondre_json(200, &preparee);
            let erreur = creer_paiement(&Dokos::new(&faux), &paiement(120.0, Some("V")), SiDoublon::CreerQuandMeme)
                .expect_err("refusée");
            assert!(matches!(&erreur, DokosError::ReponseInvalide(m) if m.contains(attendu)), "{erreur}");
            assert_eq!(faux.requetes().len(), 1, "aucune création");
        }
    }

    #[test]
    fn brouillon_exige() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &ecriture_preparee(120.0));
        faux.repondre_json(200, &json!({"data": {"name": "ACC-PAY-0009", "docstatus": 1, "paid_amount": 120.0}}));
        let erreur = creer_paiement(&Dokos::new(&faux), &paiement(120.0, Some("V")), SiDoublon::CreerQuandMeme)
            .expect_err("validé");
        assert!(matches!(erreur, DokosError::ReponseInvalide(m) if m.contains("pas en brouillon")));
    }

    #[test]
    fn factures_modes_et_compte() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "ACC-SINV-1", "customer": "CUST-2", "customer_name": "Mairie de Crest",
                              "posting_date": "2026-10-31", "outstanding_amount": 120.5, "currency": "EUR",
                              "company": "Les Amanins"}]}),
        );
        let factures = lister_factures_a_regler(&Dokos::new(&faux)).expect("factures");
        assert_eq!(factures[0].societe, "Les Amanins");
        assert_eq!(factures[0].detail(), "Mairie de Crest — 2026-10-31 — reste dû 120.50 EUR");
        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/resource/Sales%20Invoice");
        let filtres = requete.parametre("filters").unwrap_or_default().to_owned();
        assert!(filtres.contains(r#"["docstatus","=",1]"#) && filtres.contains(r#"["outstanding_amount",">",0]"#));

        faux.repondre_json(200, &json!({"data": [{"name": "Espèces"}, {"name": "Virement"}]}));
        assert_eq!(lister_modes_de_paiement(&Dokos::new(&faux)).expect("modes"), ["Espèces", "Virement"]);
        assert_eq!(faux.requetes()[1].parametre("filters"), Some(r#"[["enabled","=",1]]"#));

        faux.repondre_json(200, &json!({"message": {"account": "512 - Banque - SLA"}}));
        faux.repondre_json(200, &json!({"data": {"name": "512 - Banque - SLA", "account_type": "Bank"}}));
        let compte = compte_du_mode(&Dokos::new(&faux), "Virement", "Les Amanins").expect("compte");
        assert_eq!(compte, CompteEncaissement { compte: "512 - Banque - SLA".into(), type_compte: "Bank".into() });
        assert!(compte.reference_obligatoire());
        let requetes = faux.requetes();
        assert!(requetes[2].chemin.ends_with("sales_invoice.get_bank_cash_account"));
        assert_eq!(
            requetes[2].corps.clone().expect("corps"),
            json!({"mode_of_payment": "Virement", "company": "Les Amanins"})
        );
        assert_eq!(requetes[3].chemin, "/api/resource/Account/512%20-%20Banque%20-%20SLA");

        // Mode sans compte pour la société : message clair.
        faux.repondre_json(200, &json!({"message": {"account": null}}));
        let erreur = compte_du_mode(&Dokos::new(&faux), "Chèque", "Les Amanins").expect_err("sans compte");
        assert!(matches!(erreur, DokosError::Validation(m) if m.contains("Chèque")));
    }
}
