//! Devis, commandes et factures client, toujours créés en brouillon.
//!
//! Un [`NouveauDocumentVente`] ne peut exister que valide. Il ne porte ni prix
//! ni taxes : Dokos les calcule à partir de ses listes de prix.

use std::fmt;

use serde_json::{Value, json};

use crate::client::Dokos;
use crate::date::{Date, ErreurDate};
pub use crate::doublon::SiDoublon;
use crate::doublon::{DocumentExistant, Doublons, chercher_doublons};
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::{DOCTYPE_COMMANDE, DOCTYPE_DEVIS, DOCTYPE_FACTURE, DocumentCree};

/// Nombre maximal de lignes par document.
pub const LIGNES_MAX: usize = 200;
/// Quantité maximale par ligne, pour attraper les fautes de frappe.
pub const QUANTITE_MAX: f64 = 100_000.0;
/// Prix unitaire maximal saisi sur une ligne : garde-fou contre une erreur de saisie.
pub const PRIX_MAX: f64 = 10_000_000.0;
/// Longueur maximale d'une référence externe (champ `po_no`).
pub const REFERENCE_MAX: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeVente {
    Devis,
    Commande,
    Facture,
}

impl TypeVente {
    pub fn doctype(self) -> &'static str {
        match self {
            Self::Devis => DOCTYPE_DEVIS,
            Self::Commande => DOCTYPE_COMMANDE,
            Self::Facture => DOCTYPE_FACTURE,
        }
    }

    /// Champ qui porte le client.
    pub(crate) fn champ_client(self) -> &'static str {
        match self {
            Self::Devis => "party_name",
            Self::Commande | Self::Facture => "customer",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Devis => "devis",
            Self::Commande => "commande",
            Self::Facture => "facture",
        }
    }

    /// Ce que représente la date principale du document.
    pub fn libelle_date(self) -> &'static str {
        match self {
            Self::Devis => "date du devis",
            Self::Commande => "date de livraison",
            Self::Facture => "date de facture",
        }
    }

    /// Ce que représente la date d'échéance, si le type en a une.
    pub fn libelle_echeance(self) -> Option<&'static str> {
        match self {
            Self::Devis => Some("valable jusqu'au"),
            Self::Commande => None,
            Self::Facture => Some("échéance"),
        }
    }
}

impl fmt::Display for TypeVente {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.libelle())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurVente {
    Date(ErreurDate),
    ClientVide,
    ArticleVide,
    QuantiteInvalide(f64),
    PrixInvalide(f64),
    /// Tarif du client à mettre à jour, mais aucun prix saisi sur la ligne.
    TarifClientSansPrix,
    AucuneLigne,
    TropDeLignes(usize),
    /// Échéance antérieure à la date du document.
    EcheanceAvantDate,
    /// Échéance donnée pour un type qui n'en a pas (commande).
    EcheanceNonPrevue(TypeVente),
    /// Date de livraison par ligne donnée hors commande.
    LivraisonParLigneNonPrevue(TypeVente),
    /// Référence externe donnée sur un devis, qui n'a pas de champ pour elle.
    ReferenceNonPrevue(TypeVente),
    ReferenceTropLongue(usize),
}

impl fmt::Display for ErreurVente {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Date(e) => e.fmt(f),
            Self::ClientVide => write!(f, "client manquant"),
            Self::ArticleVide => write!(f, "article manquant sur une ligne"),
            Self::QuantiteInvalide(q) => {
                write!(f, "quantité invalide : {q} (attendu : entre 0 exclu et {QUANTITE_MAX})")
            }
            Self::PrixInvalide(p) => write!(f, "prix invalide : {p} (attendu : entre 0 et {PRIX_MAX})"),
            Self::TarifClientSansPrix => {
                write!(f, "tarif du client à mettre à jour, mais aucun prix unitaire n'est saisi sur la ligne")
            }
            Self::AucuneLigne => write!(f, "le document n'a aucune ligne"),
            Self::TropDeLignes(n) => write!(f, "trop de lignes : {n} (maximum {LIGNES_MAX})"),
            Self::EcheanceAvantDate => write!(f, "l'échéance est antérieure à la date du document"),
            Self::EcheanceNonPrevue(t) => write!(f, "une {t} n'a pas d'échéance"),
            Self::LivraisonParLigneNonPrevue(t) => {
                write!(f, "une date de livraison par ligne n'est possible que sur une commande, pas sur un {t}")
            }
            Self::ReferenceNonPrevue(t) => write!(f, "un {t} n'a pas de champ de référence externe"),
            Self::ReferenceTropLongue(n) => {
                write!(f, "référence trop longue : {n} caractères (maximum {REFERENCE_MAX})")
            }
        }
    }
}

impl std::error::Error for ErreurVente {}

impl From<ErreurDate> for ErreurVente {
    fn from(e: ErreurDate) -> Self {
        Self::Date(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LigneVente {
    article: String,
    quantite: f64,
    livraison: Option<Date>,
    /// Prix unitaire hors taxes saisi ; `None` : prix de Dokos.
    prix: Option<f64>,
    /// Le prix saisi devient aussi le prix de l'article pour ce client.
    tarif_client: bool,
}

impl LigneVente {
    /// `article` est le code article Dokos (`item_code`), déjà résolu.
    /// `livraison` n'est permise que sur une commande ; sans elle, la ligne
    /// prend la date du document.
    pub fn new(article: &str, quantite: f64, livraison: Option<Date>) -> Result<Self, ErreurVente> {
        let article = article.trim();
        if article.is_empty() {
            return Err(ErreurVente::ArticleVide);
        }
        if !quantite.is_finite() || quantite <= 0.0 || quantite > QUANTITE_MAX {
            return Err(ErreurVente::QuantiteInvalide(quantite));
        }
        Ok(Self { article: article.to_owned(), quantite, livraison, prix: None, tarif_client: false })
    }

    /// Prix unitaire hors taxes imposé à la ligne, à la place de celui des
    /// listes de prix de Dokos. `None` : Dokos calcule le prix.
    pub fn avec_prix(mut self, prix: Option<f64>) -> Result<Self, ErreurVente> {
        if let Some(p) = prix
            && !(p.is_finite() && (0.0..=PRIX_MAX).contains(&p))
        {
            return Err(ErreurVente::PrixInvalide(p));
        }
        self.prix = prix;
        Ok(self)
    }

    pub fn prix(&self) -> Option<f64> {
        self.prix
    }

    /// Demande que le prix saisi soit aussi enregistré comme prix de l'article
    /// pour ce client (voir [`crate::article::enregistrer_prix_client`]).
    /// Refusé sans prix saisi.
    pub fn avec_tarif_client(mut self, tarif_client: bool) -> Result<Self, ErreurVente> {
        if tarif_client && self.prix.is_none() {
            return Err(ErreurVente::TarifClientSansPrix);
        }
        self.tarif_client = tarif_client;
        Ok(self)
    }

    pub fn tarif_client(&self) -> bool {
        self.tarif_client
    }

    pub fn article(&self) -> &str {
        &self.article
    }

    pub fn quantite(&self) -> f64 {
        self.quantite
    }

    pub fn livraison(&self) -> Option<Date> {
        self.livraison
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NouveauDocumentVente {
    type_vente: TypeVente,
    client: String,
    date: Date,
    echeance: Option<Date>,
    lignes: Vec<LigneVente>,
    societe: Option<String>,
    reference: Option<String>,
}

impl NouveauDocumentVente {
    /// `client` est l'identifiant Dokos du client (`name`), déjà résolu.
    /// `date` est la date du devis, la date de livraison de la commande ou la
    /// date de la facture (voir [`TypeVente::libelle_date`]).
    pub fn new(
        type_vente: TypeVente,
        client: &str,
        date: Date,
        echeance: Option<Date>,
        lignes: Vec<LigneVente>,
    ) -> Result<Self, ErreurVente> {
        let client = client.trim();
        if client.is_empty() {
            return Err(ErreurVente::ClientVide);
        }
        if lignes.is_empty() {
            return Err(ErreurVente::AucuneLigne);
        }
        if lignes.len() > LIGNES_MAX {
            return Err(ErreurVente::TropDeLignes(lignes.len()));
        }
        if type_vente != TypeVente::Commande && lignes.iter().any(|l| l.livraison.is_some()) {
            return Err(ErreurVente::LivraisonParLigneNonPrevue(type_vente));
        }
        match echeance {
            Some(_) if type_vente == TypeVente::Commande => return Err(ErreurVente::EcheanceNonPrevue(type_vente)),
            Some(e) if e < date => return Err(ErreurVente::EcheanceAvantDate),
            _ => {}
        }
        Ok(Self { type_vente, client: client.to_owned(), date, echeance, lignes, societe: None, reference: None })
    }

    /// Société Dokos, utile seulement s'il y en a plusieurs. Une valeur vide est ignorée.
    pub fn avec_societe(mut self, societe: Option<&str>) -> Self {
        self.societe = societe.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        self
    }

    /// Référence externe (numéro de bon de commande du client, lot d'import),
    /// reportée dans le champ standard « N° de commande client » (`po_no`)
    /// d'une commande ou d'une facture. Une valeur vide est ignorée.
    pub fn avec_reference(mut self, reference: Option<&str>) -> Result<Self, ErreurVente> {
        let reference = reference.map(str::trim).filter(|r| !r.is_empty());
        if let Some(r) = reference {
            if self.type_vente == TypeVente::Devis {
                return Err(ErreurVente::ReferenceNonPrevue(self.type_vente));
            }
            let longueur = r.chars().count();
            if longueur > REFERENCE_MAX {
                return Err(ErreurVente::ReferenceTropLongue(longueur));
            }
        }
        self.reference = reference.map(str::to_owned);
        Ok(self)
    }

    pub fn reference(&self) -> Option<&str> {
        self.reference.as_deref()
    }

    pub fn type_vente(&self) -> TypeVente {
        self.type_vente
    }

    pub fn client(&self) -> &str {
        &self.client
    }

    pub fn date(&self) -> Date {
        self.date
    }

    pub fn echeance(&self) -> Option<Date> {
        self.echeance
    }

    pub fn lignes(&self) -> &[LigneVente] {
        &self.lignes
    }

    pub fn societe(&self) -> Option<&str> {
        self.societe.as_deref()
    }

    /// Corps JSON envoyé à Dokos. `docstatus` vaut toujours 0 (brouillon).
    pub fn corps(&self) -> Value {
        let lignes: Vec<Value> = self
            .lignes
            .iter()
            .map(|ligne| {
                let mut valeur = match self.type_vente {
                    TypeVente::Commande => json!({
                        "item_code": ligne.article,
                        "qty": ligne.quantite,
                        "delivery_date": ligne.livraison.unwrap_or(self.date).to_string(),
                    }),
                    TypeVente::Devis | TypeVente::Facture => json!({"item_code": ligne.article, "qty": ligne.quantite}),
                };
                if let Some(prix) = ligne.prix {
                    valeur["rate"] = json!(prix);
                }
                valeur
            })
            .collect();
        let date = self.date.to_string();
        let mut corps = match self.type_vente {
            TypeVente::Devis => json!({
                "quotation_to": "Customer",
                "party_name": self.client,
                "transaction_date": date,
                "order_type": "Sales",
            }),
            TypeVente::Commande => json!({"customer": self.client, "delivery_date": date}),
            // Sans `set_posting_time`, Dokos remplace la date par celle du jour.
            TypeVente::Facture => json!({"customer": self.client, "posting_date": date, "set_posting_time": 1}),
        };
        corps["docstatus"] = json!(0);
        corps["items"] = Value::Array(lignes);
        // Un prix saisi doit rester tel quel : sans cela, une règle de prix de
        // Dokos (remise) le recalculerait. Les lignes sans prix gardent le
        // tarif de la liste de prix, sans règle de prix non plus.
        if self.lignes.iter().any(|l| l.prix.is_some()) {
            corps["ignore_pricing_rule"] = json!(1);
        }
        if let Some(echeance) = self.echeance {
            let champ = if self.type_vente == TypeVente::Devis { "valid_till" } else { "due_date" };
            corps[champ] = json!(echeance.to_string());
        }
        if let Some(societe) = &self.societe {
            corps["company"] = json!(societe);
        }
        if let Some(reference) = &self.reference {
            corps["po_no"] = json!(reference);
        }
        corps
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Creation {
    Cree {
        document: DocumentCree,
        /// Documents existants du même client aux mêmes dates, à signaler.
        autres: Vec<DocumentExistant>,
    },
    /// Rien n'a été écrit : ce document identique existait déjà.
    DejaExistant(DocumentExistant),
}

/// Crée le document dans Dokos, en brouillon, après le contrôle de doublon.
pub fn creer_brouillon<T: Transport>(
    dokos: &Dokos<T>,
    document: &NouveauDocumentVente,
    si_doublon: SiDoublon,
) -> Result<Creation, DokosError> {
    creer_avec_corps(dokos, document, document.corps(), si_doublon)
}

/// Contrôle de doublon sur `empreinte`, puis création de `corps` en brouillon.
pub(crate) fn creer_avec_corps<T: Transport>(
    dokos: &Dokos<T>,
    empreinte: &NouveauDocumentVente,
    corps: Value,
    si_doublon: SiDoublon,
) -> Result<Creation, DokosError> {
    let document = empreinte;
    let Doublons { identiques, similaires } = chercher_doublons(dokos, document)?;
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = identiques.first()
    {
        return Ok(Creation::DejaExistant(existant.clone()));
    }
    // Avec `CreerQuandMeme`, les identiques sont signalés avec les similaires.
    let autres: Vec<DocumentExistant> = identiques.into_iter().chain(similaires).collect();

    let cree: DocumentCree = dokos.creer(document.type_vente.doctype(), corps)?;
    if cree.docstatus != 0 {
        return Err(DokosError::ReponseInvalide(format!(
            "le document {} n'est pas en brouillon (docstatus {})",
            cree.id, cree.docstatus
        )));
    }
    Ok(Creation::Cree { document: cree, autres })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn date(texte: &str) -> Date {
        texte.parse().expect("date de test")
    }

    fn ligne(article: &str, quantite: f64) -> LigneVente {
        LigneVente::new(article, quantite, None).expect("ligne")
    }

    fn commande_exemple() -> NouveauDocumentVente {
        let lignes =
            vec![ligne("REPAS", 25.0), LigneVente::new("GRANDE-SALLE", 1.0, Some(date("2026-11-13"))).expect("ligne")];
        NouveauDocumentVente::new(TypeVente::Commande, "Mairie de Crest", date("2026-11-12"), None, lignes)
            .expect("commande")
    }

    #[test]
    fn lignes_invalides() {
        assert_eq!(LigneVente::new("  ", 1.0, None), Err(ErreurVente::ArticleVide));
        for quantite in [0.0, -1.0, f64::NAN, f64::INFINITY, QUANTITE_MAX + 1.0] {
            assert!(LigneVente::new("REPAS", quantite, None).is_err(), "{quantite}");
        }
        assert!(LigneVente::new("REPAS", 0.5, None).is_ok());
    }

    #[test]
    fn documents_invalides() {
        let jour = date("2026-11-12");
        let nouveau = |t, client: &str, echeance, lignes| NouveauDocumentVente::new(t, client, jour, echeance, lignes);
        assert_eq!(nouveau(TypeVente::Devis, " ", None, vec![ligne("A", 1.0)]), Err(ErreurVente::ClientVide));
        assert_eq!(nouveau(TypeVente::Facture, "C", None, vec![]), Err(ErreurVente::AucuneLigne));
        let trop = vec![ligne("A", 1.0); LIGNES_MAX + 1];
        assert_eq!(nouveau(TypeVente::Commande, "C", None, trop), Err(ErreurVente::TropDeLignes(LIGNES_MAX + 1)));
        assert_eq!(
            nouveau(TypeVente::Facture, "C", Some(date("2026-11-11")), vec![ligne("A", 1.0)]),
            Err(ErreurVente::EcheanceAvantDate)
        );
        assert_eq!(
            nouveau(TypeVente::Commande, "C", Some(date("2026-12-01")), vec![ligne("A", 1.0)]),
            Err(ErreurVente::EcheanceNonPrevue(TypeVente::Commande))
        );
        let avec_livraison = vec![LigneVente::new("A", 1.0, Some(jour)).expect("ligne")];
        assert_eq!(
            nouveau(TypeVente::Devis, "C", None, avec_livraison),
            Err(ErreurVente::LivraisonParLigneNonPrevue(TypeVente::Devis))
        );
    }

    #[test]
    fn corps_commande() {
        assert_eq!(
            commande_exemple().corps(),
            json!({
                "docstatus": 0,
                "customer": "Mairie de Crest",
                "delivery_date": "2026-11-12",
                "items": [
                    {"item_code": "REPAS", "qty": 25.0, "delivery_date": "2026-11-12"},
                    {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": "2026-11-13"},
                ],
            })
        );
    }

    #[test]
    fn corps_devis() {
        let devis = NouveauDocumentVente::new(
            TypeVente::Devis,
            "CUST-1",
            date("2026-11-03"),
            Some(date("2026-12-03")),
            vec![ligne("REPAS", 25.0)],
        )
        .expect("devis");
        assert_eq!(
            devis.corps(),
            json!({
                "docstatus": 0,
                "quotation_to": "Customer",
                "party_name": "CUST-1",
                "transaction_date": "2026-11-03",
                "valid_till": "2026-12-03",
                "order_type": "Sales",
                "items": [{"item_code": "REPAS", "qty": 25.0}],
            })
        );
    }

    #[test]
    fn corps_facture() {
        let facture = NouveauDocumentVente::new(
            TypeVente::Facture,
            "CUST-1",
            date("2026-11-03"),
            Some(date("2026-12-03")),
            vec![ligne("REPAS", 25.0)],
        )
        .expect("facture")
        .avec_societe(Some("Les Amanins"));
        assert_eq!(
            facture.corps(),
            json!({
                "docstatus": 0,
                "customer": "CUST-1",
                "posting_date": "2026-11-03",
                "set_posting_time": 1,
                "due_date": "2026-12-03",
                "company": "Les Amanins",
                "items": [{"item_code": "REPAS", "qty": 25.0}],
            })
        );
    }

    #[test]
    fn aucun_prix_envoye() {
        for type_vente in [TypeVente::Devis, TypeVente::Commande, TypeVente::Facture] {
            let document = NouveauDocumentVente::new(type_vente, "C", date("2026-11-03"), None, vec![ligne("A", 2.0)])
                .expect("document");
            let texte = document.corps().to_string();
            for interdit in ["rate", "price", "amount", "tax", "discount"] {
                assert!(!texte.contains(interdit), "{interdit} dans {texte}");
            }
        }
    }

    #[test]
    fn reference_externe() {
        let commande = commande_exemple().avec_reference(Some(" LOT-2026-10 ")).expect("référence");
        assert_eq!(commande.corps()["po_no"], "LOT-2026-10");
        assert!(commande_exemple().avec_reference(Some(" ")).expect("vide").corps().get("po_no").is_none());
        let devis = NouveauDocumentVente::new(TypeVente::Devis, "C", date("2026-11-03"), None, vec![ligne("A", 1.0)])
            .expect("devis");
        assert_eq!(devis.avec_reference(Some("X")).err(), Some(ErreurVente::ReferenceNonPrevue(TypeVente::Devis)));
        assert!(commande_exemple().avec_reference(Some(&"r".repeat(141))).is_err());
    }

    #[test]
    fn prix_saisi_sur_une_ligne() {
        let jour = date("2026-11-12");
        let lignes = vec![ligne("REPAS", 25.0).avec_prix(Some(12.5)).expect("prix"), ligne("SALLE", 1.0)];
        let corps = NouveauDocumentVente::new(TypeVente::Facture, "CUST-1", jour, None, lignes).expect("doc").corps();
        assert_eq!(corps["items"][0]["rate"], 12.5);
        assert!(corps["items"][1].get("rate").is_none());
        assert_eq!(corps["ignore_pricing_rule"], 1);
        // Sans prix saisi : rien ne change, Dokos calcule tout.
        let corps = NouveauDocumentVente::new(TypeVente::Facture, "CUST-1", jour, None, vec![ligne("REPAS", 1.0)])
            .expect("doc")
            .corps();
        assert!(corps.get("ignore_pricing_rule").is_none());
        assert!(corps["items"][0].get("rate").is_none());
        // Tarif du client : seulement avec un prix saisi.
        assert_eq!(ligne("REPAS", 1.0).avec_tarif_client(true), Err(ErreurVente::TarifClientSansPrix));
        assert!(ligne("REPAS", 1.0).avec_prix(Some(9.0)).and_then(|l| l.avec_tarif_client(true)).is_ok());
        // Gratuit : permis. Négatif, infini ou démesuré : refusé.
        assert!(ligne("REPAS", 1.0).avec_prix(Some(0.0)).is_ok());
        for prix in [-1.0, f64::NAN, f64::INFINITY, PRIX_MAX * 2.0] {
            assert!(ligne("REPAS", 1.0).avec_prix(Some(prix)).is_err(), "{prix}");
        }
    }

    #[test]
    fn societe_optionnelle() {
        assert_eq!(commande_exemple().avec_societe(Some("Les Amanins")).corps()["company"], "Les Amanins");
        assert!(commande_exemple().avec_societe(Some("  ")).corps().get("company").is_none());
    }

    const AUCUN_DOCUMENT: &str = r#"{"data": []}"#;

    fn reponse_creee(docstatus: u8) -> Value {
        json!({"data": {"name": "SAL-ORD-2026-00012", "docstatus": docstatus, "customer": "Mairie de Crest",
                        "grand_total": 512.5, "currency": "EUR"}})
    }

    /// Commande déjà dans Dokos avec les lignes de `commande_exemple`, ou une quantité différente.
    fn reponse_existante(repas: f64) -> Value {
        json!({"data": {"name": "SO-1", "docstatus": 0, "customer": "Mairie de Crest",
        "delivery_date": "2026-11-13", "items": [
            {"item_code": "REPAS", "qty": repas, "delivery_date": "2026-11-12"},
            {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": "2026-11-13"},
        ]}})
    }

    #[test]
    fn creation_en_brouillon() {
        let faux = FauxTransport::new();
        faux.repondre(200, AUCUN_DOCUMENT);
        faux.repondre_json(200, &reponse_creee(0));
        let commande = commande_exemple();
        let creation = creer_brouillon(&Dokos::new(&faux), &commande, SiDoublon::Reutiliser).expect("création");
        let Creation::Cree { document, autres } = creation else { panic!("{creation:?}") };
        assert_eq!(document.id, "SAL-ORD-2026-00012");
        assert!(autres.is_empty());

        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 2);
        assert_eq!(requetes[0].methode, Methode::Get);
        assert_eq!(requetes[1].methode, Methode::Post);
        assert_eq!(requetes[1].chemin, "/api/resource/Sales%20Order");
        assert_eq!(requetes[1].corps.as_ref(), Some(&commande.corps()));
    }

    #[test]
    fn creation_de_devis_et_facture() {
        for (type_vente, chemin) in
            [(TypeVente::Devis, "/api/resource/Quotation"), (TypeVente::Facture, "/api/resource/Sales%20Invoice")]
        {
            let faux = FauxTransport::new();
            faux.repondre(200, AUCUN_DOCUMENT);
            faux.repondre_json(200, &json!({"data": {"name": "X-1", "docstatus": 0}}));
            let document = NouveauDocumentVente::new(type_vente, "C", date("2026-11-03"), None, vec![ligne("A", 1.0)])
                .expect("document");
            creer_brouillon(&Dokos::new(&faux), &document, SiDoublon::Reutiliser).expect("création");
            assert_eq!(faux.requetes()[1].chemin, chemin);
        }
    }

    #[test]
    fn doublon_reutilise_sans_ecriture() {
        let faux = FauxTransport::new();
        faux.repondre(200, r#"{"data": [{"name": "SO-1"}]}"#);
        faux.repondre_json(200, &reponse_existante(25.0));
        let creation =
            creer_brouillon(&Dokos::new(&faux), &commande_exemple(), SiDoublon::Reutiliser).expect("doublon");
        assert!(matches!(&creation, Creation::DejaExistant(c) if c.id == "SO-1"), "{creation:?}");
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));
        assert_eq!(faux.reponses_restantes(), 0);
    }

    #[test]
    fn doublon_force() {
        let faux = FauxTransport::new();
        faux.repondre(200, r#"{"data": [{"name": "SO-1"}]}"#);
        faux.repondre_json(200, &reponse_existante(25.0));
        faux.repondre_json(200, &reponse_creee(0));
        let creation =
            creer_brouillon(&Dokos::new(&faux), &commande_exemple(), SiDoublon::CreerQuandMeme).expect("création");
        let Creation::Cree { document, autres } = creation else { panic!("{creation:?}") };
        assert_eq!(document.id, "SAL-ORD-2026-00012");
        assert_eq!(autres.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["SO-1"]);
        assert_eq!(faux.requetes().last().map(|r| r.methode), Some(Methode::Post));
    }

    #[test]
    fn document_similaire_signale_mais_cree() {
        let faux = FauxTransport::new();
        faux.repondre(200, r#"{"data": [{"name": "SO-1"}]}"#);
        faux.repondre_json(200, &reponse_existante(10.0));
        faux.repondre_json(200, &reponse_creee(0));
        let creation =
            creer_brouillon(&Dokos::new(&faux), &commande_exemple(), SiDoublon::Reutiliser).expect("création");
        let Creation::Cree { autres, .. } = creation else { panic!("{creation:?}") };
        assert_eq!(autres.len(), 1);
    }

    #[test]
    fn controle_impossible_donc_rien_cree() {
        let faux = FauxTransport::new();
        faux.repondre(403, "{}");
        let resultat = creer_brouillon(&Dokos::new(&faux), &commande_exemple(), SiDoublon::Reutiliser);
        assert!(matches!(resultat, Err(DokosError::Permission(_))));
        assert_eq!(faux.requetes().len(), 1);
    }

    #[test]
    fn document_soumis_signale() {
        let faux = FauxTransport::new();
        faux.repondre(200, AUCUN_DOCUMENT);
        faux.repondre_json(200, &reponse_creee(1));
        let resultat = creer_brouillon(&Dokos::new(&faux), &commande_exemple(), SiDoublon::Reutiliser);
        assert!(matches!(resultat, Err(DokosError::ReponseInvalide(m)) if m.contains("SAL-ORD-2026-00012")));
    }

    #[test]
    fn refus_de_dokos() {
        let faux = FauxTransport::new();
        faux.repondre(200, AUCUN_DOCUMENT);
        faux.repondre(
            417,
            r#"{"exc_type": "LinkValidationError", "_server_messages": "[\"{\\\"message\\\": \\\"Article REPAS introuvable\\\"}\"]"}"#,
        );
        let resultat = creer_brouillon(&Dokos::new(&faux), &commande_exemple(), SiDoublon::Reutiliser);
        assert_eq!(resultat, Err(DokosError::Validation("Article REPAS introuvable".into())));
    }
}
