//! Écritures de paiement (DocType `Payment Entry`) en règlement d'une facture
//! de vente validée. **Toujours créées en brouillon** : l'outil ne valide
//! jamais, la facture n'est soldée qu'à la validation dans Dokos.
//!
//! Dokos prépare l'écriture lui-même (`get_payment_entry` : client, comptes,
//! devises, lien vers la facture), sans l'enregistrer. L'outil y pose la date,
//! le mode de paiement et la référence, vérifie que le montant préparé est bien
//! celui demandé, puis la crée en brouillon. Le journal comptable n'est pas
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
    FactureVide,
    ModeVide,
    MontantInvalide,
    ReferenceTropLongue(usize),
    /// Compte bancaire : Dokos refuse un paiement sans référence.
    ReferenceObligatoire,
}

impl fmt::Display for ErreurPaiement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FactureVide => write!(f, "facture manquante"),
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

/// Paiement d'une facture, validé avant tout envoi à Dokos.
#[derive(Debug, Clone, PartialEq)]
pub struct NouveauPaiement {
    facture: String,
    date: Date,
    montant: f64,
    mode: String,
    compte: CompteEncaissement,
    reference: Option<String>,
}

impl NouveauPaiement {
    /// `facture` : identifiant d'une facture validée ; `montant` : arrondi au
    /// centime ; `compte` : celui du mode de paiement ([`compte_du_mode`]).
    pub fn new(
        facture: &str,
        date: Date,
        montant: f64,
        mode: &str,
        compte: CompteEncaissement,
        reference: Option<&str>,
    ) -> Result<Self, ErreurPaiement> {
        let facture = facture.trim();
        if facture.is_empty() {
            return Err(ErreurPaiement::FactureVide);
        }
        let mode = mode.trim();
        if mode.is_empty() {
            return Err(ErreurPaiement::ModeVide);
        }
        let montant = arrondi(montant);
        if !montant.is_finite() || montant <= 0.0 || montant > MONTANT_MAX {
            return Err(ErreurPaiement::MontantInvalide);
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
        Ok(Self { facture: facture.to_owned(), date, montant, mode: mode.to_owned(), compte, reference })
    }

    pub fn facture(&self) -> &str {
        &self.facture
    }

    pub fn date(&self) -> Date {
        self.date
    }

    pub fn montant(&self) -> f64 {
        self.montant
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

/// Paiement identique non annulé : même facture, même date, même montant.
pub fn chercher_paiement_identique<T: Transport>(
    dokos: &Dokos<T>,
    paiement: &NouveauPaiement,
) -> Result<Option<PaiementExistant>, DokosError> {
    let liste = Liste::new(&["name", "docstatus", "posting_date", "paid_amount"])
        .filtre(Filtre::different("docstatus", 2))
        .filtre(Filtre::egal("payment_type", "Receive"))
        .filtre(Filtre::egal("posting_date", paiement.date.to_string()))
        .filtre(Filtre::egal("paid_amount", paiement.montant))
        .filtre(Filtre::enfant(DOCTYPE_REFERENCE_PAIEMENT, "reference_name", paiement.facture.as_str()))
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

/// Écriture préparée par Dokos, complétée de ce que l'outil décide : date, mode
/// de paiement, référence. Refuse ce que Dokos n'a pas préparé comme demandé.
fn completer_ecriture(mut ecriture: Map<String, Value>, paiement: &NouveauPaiement) -> Result<Value, DokosError> {
    let inattendu = |detail: String| {
        DokosError::ReponseInvalide(format!("Dokos n'a pas préparé le paiement de {} : {detail}", paiement.facture))
    };
    if ecriture.get("doctype").and_then(Value::as_str) != Some(DOCTYPE_PAIEMENT) {
        return Err(inattendu("ce n'est pas une écriture de paiement".to_owned()));
    }
    if ecriture.get("payment_type").and_then(Value::as_str) != Some("Receive") {
        return Err(inattendu("ce n'est pas un encaissement".to_owned()));
    }
    let references = ecriture.get("references").and_then(Value::as_array).cloned().unwrap_or_default();
    if references.is_empty()
        || references.iter().any(|r| r.get("reference_name").and_then(Value::as_str) != Some(paiement.facture.as_str()))
    {
        return Err(inattendu("la facture n'est pas la seule référence".to_owned()));
    }
    let alloue: f64 = references.iter().map(|r| nombre(r.get("allocated_amount"))).sum();
    let paye = nombre(ecriture.get("paid_amount"));
    if !proche(alloue, paiement.montant) || !proche(paye, paiement.montant) {
        return Err(inattendu(format!(
            "montant payé {paye:.2}, réparti {alloue:.2}, demandé {:.2} (facture déjà réglée ou en devise étrangère ?)",
            paiement.montant
        )));
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
    let mut demande = json!({
        "dt": DOCTYPE_FACTURE,
        "dn": paiement.facture,
        "party_amount": paiement.montant,
        "bank_account": paiement.compte.compte,
    });
    if paiement.reference.is_some() {
        demande["reference_date"] = json!(paiement.date.to_string());
    }
    let prepare: Map<String, Value> = dokos.methode_post(METHODE_PREPARER, demande)?;
    let corps = completer_ecriture(prepare, paiement)?;
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
        NouveauPaiement::new("ACC-SINV-1", jour(), montant, "Virement", compte("Bank"), reference).expect("paiement")
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
            NouveauPaiement::new(" ", jour(), 10.0, "Espèces", compte("Cash"), None),
            Err(ErreurPaiement::FactureVide)
        );
        assert_eq!(NouveauPaiement::new("F-1", jour(), 10.0, " ", compte("Cash"), None), Err(ErreurPaiement::ModeVide));
        for montant in [0.0, -5.0, f64::NAN, f64::INFINITY, MONTANT_MAX * 2.0, 0.004] {
            assert_eq!(
                NouveauPaiement::new("F-1", jour(), montant, "Espèces", compte("Cash"), None),
                Err(ErreurPaiement::MontantInvalide),
                "{montant}"
            );
        }
        let p = NouveauPaiement::new("F-1", jour(), 12.346, "Espèces", compte("Cash"), Some("  ")).expect("espèces");
        assert_eq!((p.montant(), p.reference()), (12.35, None), "arrondi au centime, référence vide ignorée");
        // Compte bancaire : référence obligatoire ; espèces : facultative.
        assert_eq!(
            NouveauPaiement::new("F-1", jour(), 10.0, "Virement", compte("Bank"), None),
            Err(ErreurPaiement::ReferenceObligatoire)
        );
        assert!(NouveauPaiement::new("F-1", jour(), 10.0, "Virement", compte("Bank"), Some("VIR-9")).is_ok());
        assert!(matches!(
            NouveauPaiement::new("F-1", jour(), 10.0, "Chèque", compte("Cash"), Some(&"x".repeat(141))),
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
