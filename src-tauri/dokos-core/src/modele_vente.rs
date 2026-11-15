//! Devis, commandes et factures créés à partir d'un modèle.
//!
//! Un modèle est un document du même type qui porte l'étiquette Dokos
//! « Template » (exactement). Le nouveau document en est une copie, avec :
//! - le client choisi ;
//! - la date du jour (date de livraison d'une commande : la date de départ) ;
//! - les dates d'arrivée et de départ (`custom_date_arrivee`, `custom_date_depart`) ;
//! - seulement les lignes dont la quantité est saisie et non nulle, aux
//!   quantités saisies.
//!
//! Tout le reste est repris tel quel : prix et remises des lignes (règles de
//! prix ignorées), taxes, conditions de paiement, conditions générales, liste
//! de prix, champs personnalisés. Les champs propres à un document
//! (identifiant, client, adresses, dates, totaux, liens vers d'autres
//! documents, étiquettes) ne sont pas copiés. Le modèle n'est jamais modifié.

use std::fmt;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::client::{Dokos, Filtre, Liste};
use crate::date::Date;
use crate::erreur::DokosError;
use crate::meta::lire_meta_avec;
use crate::transport::Transport;
use crate::types::texte_ou_vide;
use crate::vente::{Creation, LigneVente, NouveauDocumentVente, QUANTITE_MAX, SiDoublon, TypeVente, creer_avec_corps};

/// Étiquette Dokos qui désigne un modèle.
pub const ETIQUETTE_MODELE: &str = "Template";
/// Champs personnalisés des dates d'arrivée et de départ.
pub const CHAMP_ARRIVEE: &str = "custom_date_arrivee";
pub const CHAMP_DEPART: &str = "custom_date_depart";
/// Modèles listés au plus, par type.
const MODELES_MAX: u32 = 200;

/// Vrai si la liste d'étiquettes Frappe (`_user_tags`, « ,A,B ») contient exactement « Template ».
pub fn porte_etiquette_modele(etiquettes: &str) -> bool {
    etiquettes.split(',').any(|e| e.trim() == ETIQUETTE_MODELE)
}

/// Condition qui écarte les modèles d'une liste de Dokos.
pub fn filtre_sans_modeles() -> Filtre {
    Filtre::ne_contient_pas("_user_tags", ETIQUETTE_MODELE)
}

/// Champs d'en-tête jamais copiés : propres au document, recalculés par Dokos, ou fixés ici.
const EXCLUS_ENTETE: &[&str] = &[
    "name",
    "owner",
    "creation",
    "modified",
    "modified_by",
    "docstatus",
    "idx",
    "doctype",
    "amended_from",
    "title",
    "status",
    // Client et ce qui en découle.
    "customer",
    "customer_name",
    "party_name",
    "quotation_to",
    "customer_address",
    "address_display",
    "contact_person",
    "contact_display",
    "contact_mobile",
    "contact_email",
    "contact_phone",
    "shipping_address_name",
    "shipping_address",
    "dispatch_address_name",
    "dispatch_address",
    "customer_group",
    "territory",
    "tax_id",
    "po_no",
    "po_date",
    // Dates.
    "transaction_date",
    "posting_date",
    "posting_time",
    "set_posting_time",
    "due_date",
    "valid_till",
    "delivery_date",
    CHAMP_ARRIVEE,
    CHAMP_DEPART,
    // Tables propres au document.
    "payment_schedule",
    "advances",
    "payments",
    "packed_items",
    "pricing_rules",
    "timesheets",
    // Suivi, totaux et montants recalculés.
    "outstanding_amount",
    "paid_amount",
    "write_off_amount",
    "change_amount",
    "per_billed",
    "per_delivered",
    "per_picked",
    "billing_status",
    "delivery_status",
    "advance_paid",
    "total_advance",
    "total",
    "net_total",
    "total_qty",
    "total_net_weight",
    "total_taxes_and_charges",
    "grand_total",
    "rounded_total",
    "rounding_adjustment",
    "in_words",
    "amount_eligible_for_commission",
    // Liens vers d'autres documents.
    "is_return",
    "return_against",
    "opportunity",
    "auto_repeat",
    "inter_company_order_reference",
    "inter_company_invoice_reference",
    "represents_company",
    "lost_reasons",
    "competitors",
    "order_lost_reason",
];

/// Champs des lignes d'articles jamais copiés (en plus des champs système).
const EXCLUS_LIGNE: &[&str] = &[
    "qty",
    "stock_qty",
    "amount",
    "net_amount",
    "delivery_date",
    "transaction_date",
    "prevdoc_docname",
    "prevdoc_doctype",
    "against_blanket_order",
    "blanket_order",
    "blanket_order_rate",
    "quotation_item",
    "sales_order",
    "so_detail",
    "delivery_note",
    "dn_detail",
    "delivered_qty",
    "returned_qty",
    "billed_amt",
    "ordered_qty",
    "actual_qty",
    "projected_qty",
    "planned_qty",
    "produced_qty",
    "work_order_qty",
    "picked_qty",
    "valuation_rate",
    "gross_profit",
    "pricing_rules",
];

/// Champs système de toute ligne de table enfant.
const SYSTEME_LIGNE: &[&str] = &[
    "name",
    "owner",
    "creation",
    "modified",
    "modified_by",
    "docstatus",
    "idx",
    "parent",
    "parentfield",
    "parenttype",
    "doctype",
];

/// Champ copiable : ni système, ni étiquette (`_user_tags`…), ni montant en devise de la société (`base_…`).
fn copiable(champ: &str, exclus: &[&str]) -> bool {
    !champ.starts_with('_') && !champ.starts_with("base_") && !exclus.contains(&champ)
}

fn nettoyer_ligne(ligne: &Map<String, Value>, exclus: &[&str]) -> Map<String, Value> {
    ligne
        .iter()
        .filter(|(champ, _)| copiable(champ, SYSTEME_LIGNE) && !exclus.contains(&champ.as_str()))
        .map(|(champ, valeur)| (champ.clone(), valeur.clone()))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurModele {
    Dokos(DokosError),
    /// Le document n'est pas (ou plus) un modèle utilisable.
    PasUnModele(String),
    ChampAbsent {
        champ: &'static str,
        doctype: &'static str,
    },
    ClientVide,
    DatesInversees {
        arrivee: Date,
        depart: Date,
    },
    /// Commande : Dokos refuse une livraison antérieure à la date de création.
    DepartPasse {
        depart: Date,
        aujourdhui: Date,
    },
    QuantiteInvalide {
        ligne: String,
        quantite: f64,
    },
    LigneInconnue(usize),
    AucuneQuantite,
    /// Le modèle a changé depuis qu'il a été affiché ou que le fichier a été généré.
    ModeleModifie {
        attendu: String,
        actuel: String,
    },
}

impl fmt::Display for ErreurModele {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dokos(e) => e.fmt(f),
            Self::PasUnModele(m) => f.write_str(m),
            Self::ChampAbsent { champ, doctype } => {
                write!(f, "le champ « {champ} » n'existe pas sur {doctype} dans votre Dokos : créez-le, ou demandez-le")
            }
            Self::ClientVide => f.write_str("client manquant"),
            Self::DatesInversees { arrivee, depart } => {
                write!(f, "la date de départ ({depart}) est avant la date d'arrivée ({arrivee})")
            }
            Self::DepartPasse { depart, aujourdhui } => write!(
                f,
                "la date de départ ({depart}) est la date de livraison de la commande : elle ne peut pas être avant aujourd'hui ({aujourdhui})"
            ),
            Self::QuantiteInvalide { ligne, quantite } => {
                write!(f, "quantité invalide pour « {ligne} » : {quantite} (attendu : entre 0 et {QUANTITE_MAX})")
            }
            Self::LigneInconnue(i) => write!(f, "la ligne {} n'existe pas dans le modèle", i + 1),
            Self::AucuneQuantite => f.write_str("aucune quantité saisie : au moins un article est nécessaire"),
            Self::ModeleModifie { attendu, actuel } => write!(
                f,
                "le modèle a été modifié dans Dokos ({actuel}, au lieu de {attendu}) : rechargez-le, ou régénérez le fichier d'import"
            ),
        }
    }
}

impl std::error::Error for ErreurModele {}

impl From<DokosError> for ErreurModele {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

/// Modèle dans une liste de choix.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ResumeModele {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "title", default, deserialize_with = "texte_ou_vide")]
    pub titre: String,
    #[serde(rename = "customer_name", default, deserialize_with = "texte_ou_vide")]
    pub client: String,
    #[serde(rename = "grand_total", default)]
    pub total: f64,
    #[serde(rename = "currency", default, deserialize_with = "texte_ou_vide")]
    pub devise: String,
    #[serde(rename = "modified", default, deserialize_with = "texte_ou_vide")]
    pub modifie: String,
    #[serde(rename = "_user_tags", default, deserialize_with = "texte_ou_vide")]
    etiquettes: String,
}

/// Documents du type portant exactement l'étiquette « Template » (annulés exclus).
pub fn lister_modeles<T: Transport>(dokos: &Dokos<T>, type_vente: TypeVente) -> Result<Vec<ResumeModele>, DokosError> {
    let liste = Liste::new(&["name", "title", "customer_name", "grand_total", "currency", "modified", "_user_tags"])
        .filtre(Filtre::contient("_user_tags", ETIQUETTE_MODELE))
        .filtre(Filtre::different("docstatus", 2))
        .ordre("title asc")
        .limite(MODELES_MAX);
    let mut modeles: Vec<ResumeModele> = dokos.lister(type_vente.doctype(), &liste)?;
    modeles.retain(|m| porte_etiquette_modele(&m.etiquettes));
    Ok(modeles)
}

/// Ligne d'article du modèle.
#[derive(Debug, Clone, PartialEq)]
pub struct LigneModele {
    /// Position dans le modèle (0 = première ligne).
    pub index: usize,
    pub code: String,
    pub libelle: String,
    pub quantite: f64,
    pub prix: f64,
    pub unite: String,
}

impl LigneModele {
    /// « Repas complet (REPAS) ».
    pub fn designation(&self) -> String {
        if self.libelle.is_empty() || self.libelle == self.code {
            self.code.clone()
        } else {
            format!("{} ({})", self.libelle, self.code)
        }
    }
}

/// Modèle lu dans Dokos, prêt à être copié.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeleVente {
    pub type_vente: TypeVente,
    pub id: String,
    pub titre: String,
    /// Date de dernière modification (`modified`), pour détecter un modèle changé.
    pub modifie: String,
    pub devise: String,
    pub lignes: Vec<LigneModele>,
    document: Map<String, Value>,
}

fn texte(document: &Map<String, Value>, champ: &str) -> String {
    match document.get(champ) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Null) | None => String::new(),
        Some(autre) => autre.to_string(),
    }
}

fn nombre(document: &Map<String, Value>, champ: &str) -> f64 {
    document.get(champ).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Lit le modèle : vérifie l'étiquette et la présence des champs de dates sur le type.
pub fn lire_modele<T: Transport>(
    dokos: &Dokos<T>,
    type_vente: TypeVente,
    id: &str,
) -> Result<ModeleVente, ErreurModele> {
    let doctype = type_vente.doctype();
    #[derive(Deserialize)]
    struct Etiquettes {
        #[serde(rename = "_user_tags", default, deserialize_with = "texte_ou_vide")]
        etiquettes: String,
    }
    let confirmes: Vec<Etiquettes> =
        dokos.lister(doctype, &Liste::new(&["name", "_user_tags"]).filtre(Filtre::egal("name", id)).limite(1))?;
    if !confirmes.iter().any(|c| porte_etiquette_modele(&c.etiquettes)) {
        return Err(ErreurModele::PasUnModele(format!(
            "{doctype} « {id} » ne porte pas l'étiquette « {ETIQUETTE_MODELE} » (ou n'existe pas)"
        )));
    }
    let document: Map<String, Value> = dokos.lire(doctype, id)?;
    if document.get("docstatus").and_then(Value::as_i64) == Some(2) {
        return Err(ErreurModele::PasUnModele(format!("le modèle {id} est annulé")));
    }
    // Un champ qui vient d'être créé dans Dokos doit être vu tout de suite.
    let meta = lire_meta_avec(dokos, doctype, &[CHAMP_ARRIVEE, CHAMP_DEPART])?;
    for champ in [CHAMP_ARRIVEE, CHAMP_DEPART] {
        if !meta.a_champ(champ) {
            return Err(ErreurModele::ChampAbsent { champ, doctype });
        }
    }
    let lignes = document
        .get("items")
        .and_then(Value::as_array)
        .map(|lignes| {
            lignes
                .iter()
                .filter_map(Value::as_object)
                .enumerate()
                .map(|(index, l)| LigneModele {
                    index,
                    code: texte(l, "item_code"),
                    libelle: texte(l, "item_name"),
                    quantite: nombre(l, "qty"),
                    prix: nombre(l, "rate"),
                    unite: texte(l, "uom"),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if lignes.is_empty() {
        return Err(ErreurModele::PasUnModele(format!("le modèle {id} n'a aucune ligne d'article")));
    }
    let titre = Some(texte(&document, "title")).filter(|t| !t.is_empty()).unwrap_or_else(|| id.to_owned());
    Ok(ModeleVente {
        type_vente,
        id: id.to_owned(),
        titre,
        modifie: texte(&document, "modified"),
        devise: texte(&document, "currency"),
        lignes,
        document,
    })
}

/// Document à créer à partir d'un modèle. N'existe que valide.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentDepuisModele {
    modele: ModeleVente,
    client: String,
    arrivee: Date,
    depart: Date,
    aujourdhui: Date,
    /// (index de la ligne du modèle, quantité), quantités nulles retirées.
    quantites: Vec<(usize, f64)>,
}

impl DocumentDepuisModele {
    /// `quantites` : (index de ligne, quantité) ; une quantité absente ou nulle retire la ligne.
    pub fn new(
        modele: &ModeleVente,
        client: &str,
        arrivee: Date,
        depart: Date,
        quantites: &[(usize, Option<f64>)],
        aujourdhui: Date,
    ) -> Result<Self, ErreurModele> {
        let client = client.trim();
        if client.is_empty() {
            return Err(ErreurModele::ClientVide);
        }
        if depart < arrivee {
            return Err(ErreurModele::DatesInversees { arrivee, depart });
        }
        if modele.type_vente == TypeVente::Commande && depart < aujourdhui {
            return Err(ErreurModele::DepartPasse { depart, aujourdhui });
        }
        let mut retenues = Vec::new();
        for &(index, quantite) in quantites {
            let ligne = modele.lignes.get(index).ok_or(ErreurModele::LigneInconnue(index))?;
            match quantite {
                None => {}
                Some(0.0) => {}
                Some(q) if q.is_finite() && q > 0.0 && q <= QUANTITE_MAX => retenues.push((index, q)),
                Some(q) => return Err(ErreurModele::QuantiteInvalide { ligne: ligne.designation(), quantite: q }),
            }
        }
        if retenues.is_empty() {
            return Err(ErreurModele::AucuneQuantite);
        }
        retenues.sort_by_key(|(i, _)| *i);
        retenues.dedup_by_key(|(i, _)| *i);
        Ok(Self { modele: modele.clone(), client: client.to_owned(), arrivee, depart, aujourdhui, quantites: retenues })
    }

    pub fn modele(&self) -> &ModeleVente {
        &self.modele
    }

    pub fn client(&self) -> &str {
        &self.client
    }

    /// Date du document : celle du jour, ou le départ pour la livraison d'une commande.
    pub fn date_document(&self) -> Date {
        match self.modele.type_vente {
            TypeVente::Commande => self.depart,
            TypeVente::Devis | TypeVente::Facture => self.aujourdhui,
        }
    }

    /// Lignes retenues (désignation, quantité).
    pub fn lignes(&self) -> Vec<(String, f64)> {
        self.quantites.iter().filter_map(|&(i, q)| self.modele.lignes.get(i).map(|l| (l.designation(), q))).collect()
    }

    /// Équivalent simplifié (client, date, articles, quantités) pour le contrôle de doublon.
    pub fn empreinte(&self) -> Result<NouveauDocumentVente, DokosError> {
        let lignes = self
            .quantites
            .iter()
            .filter_map(|&(i, q)| self.modele.lignes.get(i).map(|l| (l.code.clone(), q)))
            .map(|(code, q)| LigneVente::new(&code, q, None))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DokosError::Validation(e.to_string()))?;
        NouveauDocumentVente::new(self.modele.type_vente, &self.client, self.date_document(), None, lignes)
            .map_err(|e| DokosError::Validation(e.to_string()))
    }

    /// Corps JSON : copie du modèle, client, dates et lignes retenues ; toujours en brouillon.
    pub fn corps(&self) -> Value {
        let type_vente = self.modele.type_vente;
        let mut corps: Map<String, Value> = self
            .modele
            .document
            .iter()
            .filter(|(champ, _)| copiable(champ, EXCLUS_ENTETE) && champ.as_str() != "items")
            .map(|(champ, valeur)| {
                // Autres tables (taxes, commerciaux…) : copiées sans leurs champs système.
                let valeur = match valeur {
                    Value::Array(lignes) => Value::Array(
                        lignes
                            .iter()
                            .map(|l| match l {
                                Value::Object(o) => Value::Object(nettoyer_ligne(o, &[])),
                                autre => autre.clone(),
                            })
                            .collect(),
                    ),
                    autre => autre.clone(),
                };
                (champ.clone(), valeur)
            })
            .collect();
        let aujourdhui = self.aujourdhui.to_string();
        let depart = self.depart.to_string();
        match type_vente {
            TypeVente::Devis => {
                corps.insert("quotation_to".into(), json!("Customer"));
                corps.insert("party_name".into(), json!(self.client));
                corps.insert("transaction_date".into(), json!(aujourdhui));
            }
            TypeVente::Commande => {
                corps.insert("customer".into(), json!(self.client));
                corps.insert("transaction_date".into(), json!(aujourdhui));
                corps.insert("delivery_date".into(), json!(depart));
            }
            TypeVente::Facture => {
                corps.insert("customer".into(), json!(self.client));
                corps.insert("posting_date".into(), json!(aujourdhui));
                corps.insert("set_posting_time".into(), json!(1));
            }
        }
        corps.insert(CHAMP_ARRIVEE.into(), json!(self.arrivee.to_string()));
        corps.insert(CHAMP_DEPART.into(), json!(depart));
        // Les prix sont ceux du modèle : aucune règle de prix ne les remplace.
        corps.insert("ignore_pricing_rule".into(), json!(1));
        corps.insert("docstatus".into(), json!(0));

        let lignes_modele: Vec<&Map<String, Value>> = self
            .modele
            .document
            .get("items")
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();
        let lignes: Vec<Value> = self
            .quantites
            .iter()
            .filter_map(|&(i, q)| {
                let mut ligne = nettoyer_ligne(lignes_modele.get(i)?, EXCLUS_LIGNE);
                ligne.insert("qty".into(), json!(q));
                if type_vente == TypeVente::Commande {
                    ligne.insert("delivery_date".into(), json!(depart));
                }
                Some(Value::Object(ligne))
            })
            .collect();
        corps.insert("items".into(), Value::Array(lignes));
        Value::Object(corps)
    }
}

/// Crée le document en brouillon, après le contrôle de doublon (modèles exclus).
pub fn creer_depuis_modele<T: Transport>(
    dokos: &Dokos<T>,
    document: &DocumentDepuisModele,
    si_doublon: SiDoublon,
) -> Result<Creation, DokosError> {
    creer_avec_corps(dokos, &document.empreinte()?, document.corps(), si_doublon)
}

/// Vérifie que le modèle n'a pas changé depuis `modifie`.
pub fn verifier_version(modele: &ModeleVente, modifie: &str) -> Result<(), ErreurModele> {
    if modifie.trim() != modele.modifie {
        return Err(ErreurModele::ModeleModifie { attendu: modifie.trim().to_owned(), actuel: modele.modifie.clone() });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;

    use super::*;
    use crate::meta::tests::reponse_meta;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn date(texte: &str) -> Date {
        texte.parse().expect("date")
    }

    /// Devis modèle : deux lignes à prix négociés, taxes, conditions, champ personnalisé.
    pub(crate) fn document_modele() -> Value {
        serde_json::from_str(
            r#"{
            "name": "SAL-QTN-0007", "doctype": "Quotation", "docstatus": 0, "owner": "ana@exemple.fr",
            "title": "Séminaire 2 jours", "quotation_to": "Customer", "party_name": "CLIENT-MODELE",
            "customer_name": "Client modèle", "transaction_date": "2025-01-10", "valid_till": "2025-02-10",
            "company": "Les Amanins", "currency": "EUR", "selling_price_list": "Tarif 2026",
            "taxes_and_charges": "TVA 10 % - LA", "payment_terms_template": "30 jours",
            "tc_name": "CGV séminaires", "terms": "<p>Acompte de 30 %</p>", "naming_series": "SAL-QTN-.YYYY.-",
            "custom_formule": "Pension complète", "custom_date_arrivee": "2025-03-01",
            "custom_date_depart": "2025-03-02", "grand_total": 1234.5, "base_grand_total": 1234.5,
            "status": "Draft", "modified": "2026-10-01 09:00:00.000000", "_user_tags": ",Template",
            "_comments": "[]",
            "items": [
                {"name": "a1b2", "parent": "SAL-QTN-0007", "idx": 1, "doctype": "Quotation Item",
                 "item_code": "REPAS", "item_name": "Repas complet", "qty": 20, "uom": "Unité",
                 "rate": 18.5, "price_list_rate": 22, "discount_percentage": 15.9, "amount": 370,
                 "base_amount": 370, "prevdoc_docname": "OPP-1", "custom_regime": "Standard"},
                {"name": "c3d4", "parent": "SAL-QTN-0007", "idx": 2, "doctype": "Quotation Item",
                 "item_code": "SALLE-G", "item_name": "Grande salle", "qty": 2, "uom": "Jour", "rate": 300},
                {"name": "e5f6", "parent": "SAL-QTN-0007", "idx": 3, "doctype": "Quotation Item",
                 "item_code": "NUITEE", "item_name": "Nuitée", "qty": 20, "uom": "Unité", "rate": 45}
            ],
            "taxes": [{"name": "t1", "parent": "SAL-QTN-0007", "idx": 1, "charge_type": "On Net Total",
                       "account_head": "TVA collectée 10 %", "rate": 10, "description": "TVA 10 %"}],
            "payment_schedule": [{"name": "p1", "due_date": "2025-02-09", "payment_amount": 1234.5}]
        }"#,
        )
        .expect("modèle JSON")
    }

    pub(crate) fn repondre_modele(faux: &FauxTransport, type_vente: TypeVente) {
        faux.repondre_json(200, &json!({"data": [{"name": "SAL-QTN-0007", "_user_tags": ",Template"}]}));
        faux.repondre_json(200, &json!({ "data": document_modele() }));
        faux.repondre_json(
            200,
            &reponse_meta(
                type_vente.doctype(),
                json!([
                    {"fieldname": CHAMP_ARRIVEE, "fieldtype": "Date"},
                    {"fieldname": CHAMP_DEPART, "fieldtype": "Date"},
                ]),
            ),
        );
    }

    fn modele(type_vente: TypeVente) -> ModeleVente {
        let faux = FauxTransport::new();
        repondre_modele(&faux, type_vente);
        lire_modele(&Dokos::new(&faux), type_vente, "SAL-QTN-0007").expect("modèle")
    }

    #[test]
    fn etiquettes() {
        assert!(porte_etiquette_modele(",Template"));
        assert!(porte_etiquette_modele("Urgent, Template ,Autre"));
        assert!(!porte_etiquette_modele(",Templates"));
        assert!(!porte_etiquette_modele(",template"));
        assert!(!porte_etiquette_modele(""));
    }

    #[test]
    fn liste_des_modeles() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [
                {"name": "SAL-QTN-0007", "title": "Séminaire", "grand_total": 1234.5, "currency": "EUR",
                 "modified": "2026-10-01", "_user_tags": ",Template"},
                {"name": "SAL-QTN-0008", "title": "Ancien", "_user_tags": ",Templates-archive"},
            ]}),
        );
        let modeles = lister_modeles(&Dokos::new(&faux), TypeVente::Devis).expect("modèles");
        assert_eq!(modeles.len(), 1);
        assert_eq!(modeles[0].titre, "Séminaire");
        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/resource/Quotation");
        let filtres: Value = serde_json::from_str(requete.parametre("filters").unwrap_or("[]")).expect("filtres");
        assert_eq!(filtres[0], json!(["_user_tags", "like", "%Template%"]));
    }

    #[test]
    fn lecture_du_modele() {
        let m = modele(TypeVente::Devis);
        assert_eq!(m.titre, "Séminaire 2 jours");
        assert_eq!(m.modifie, "2026-10-01 09:00:00.000000");
        assert_eq!(m.lignes.len(), 3);
        assert_eq!(m.lignes[0].designation(), "Repas complet (REPAS)");
        assert_eq!(m.lignes[0].prix, 18.5);
        assert!(verifier_version(&m, "2026-10-01 09:00:00.000000").is_ok());
        assert!(matches!(verifier_version(&m, "2026-09-01"), Err(ErreurModele::ModeleModifie { .. })));
    }

    #[test]
    fn modele_refuse() {
        // Sans l'étiquette : rien d'autre n'est lu.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "SAL-QTN-0001", "_user_tags": ",Urgent"}]}));
        let erreur = lire_modele(&Dokos::new(&faux), TypeVente::Devis, "SAL-QTN-0001").expect_err("pas un modèle");
        assert!(matches!(erreur, ErreurModele::PasUnModele(_)));
        assert_eq!(faux.requetes().len(), 1);

        // Champs de dates absents du type de document.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "SAL-QTN-0007", "_user_tags": ",Template"}]}));
        faux.repondre_json(200, &json!({ "data": document_modele() }));
        faux.repondre_json(200, &reponse_meta("Quotation", json!([{"fieldname": CHAMP_ARRIVEE, "fieldtype": "Date"}])));
        let erreur = lire_modele(&Dokos::new(&faux), TypeVente::Devis, "SAL-QTN-0007").expect_err("champ absent");
        assert_eq!(erreur, ErreurModele::ChampAbsent { champ: CHAMP_DEPART, doctype: "Quotation" });
    }

    #[test]
    fn demandes_invalides() {
        let m = modele(TypeVente::Devis);
        let (a, d, j) = (date("2026-12-01"), date("2026-12-03"), date("2026-11-05"));
        let nouveau = |client: &str, arrivee, depart, q: &[(usize, Option<f64>)]| {
            DocumentDepuisModele::new(&m, client, arrivee, depart, q, j)
        };
        assert_eq!(nouveau(" ", a, d, &[(0, Some(1.0))]), Err(ErreurModele::ClientVide));
        assert!(matches!(nouveau("C", d, a, &[(0, Some(1.0))]), Err(ErreurModele::DatesInversees { .. })));
        assert_eq!(nouveau("C", a, d, &[(0, None), (1, Some(0.0))]), Err(ErreurModele::AucuneQuantite));
        assert!(matches!(nouveau("C", a, d, &[(0, Some(-2.0))]), Err(ErreurModele::QuantiteInvalide { .. })));
        assert_eq!(nouveau("C", a, d, &[(9, Some(1.0))]), Err(ErreurModele::LigneInconnue(9)));

        let commande = modele(TypeVente::Commande);
        let passe =
            DocumentDepuisModele::new(&commande, "C", date("2026-11-01"), date("2026-11-02"), &[(0, Some(1.0))], j);
        assert!(matches!(passe, Err(ErreurModele::DepartPasse { .. })));
    }

    #[test]
    fn corps_copie_du_modele() {
        let m = modele(TypeVente::Devis);
        let document = DocumentDepuisModele::new(
            &m,
            "CUST-2",
            date("2026-12-01"),
            date("2026-12-03"),
            &[(0, Some(30.0)), (1, Some(0.0)), (2, None)],
            date("2026-11-05"),
        )
        .expect("document");
        let corps = document.corps();
        // Repris tel quel.
        for (champ, valeur) in [
            ("company", json!("Les Amanins")),
            ("selling_price_list", json!("Tarif 2026")),
            ("taxes_and_charges", json!("TVA 10 % - LA")),
            ("payment_terms_template", json!("30 jours")),
            ("tc_name", json!("CGV séminaires")),
            ("terms", json!("<p>Acompte de 30 %</p>")),
            ("naming_series", json!("SAL-QTN-.YYYY.-")),
            ("custom_formule", json!("Pension complète")),
        ] {
            assert_eq!(corps[champ], valeur, "{champ}");
        }
        // Fixé ici.
        assert_eq!(corps["party_name"], "CUST-2");
        assert_eq!(corps["quotation_to"], "Customer");
        assert_eq!(corps["transaction_date"], "2026-11-05");
        assert_eq!(corps[CHAMP_ARRIVEE], "2026-12-01");
        assert_eq!(corps[CHAMP_DEPART], "2026-12-03");
        assert_eq!(corps["ignore_pricing_rule"], 1);
        assert_eq!(corps["docstatus"], 0);
        // Jamais copié.
        for champ in [
            "name",
            "owner",
            "title",
            "customer_name",
            "valid_till",
            "grand_total",
            "base_grand_total",
            "status",
            "modified",
            "_user_tags",
            "_comments",
            "payment_schedule",
            "doctype",
        ] {
            assert!(corps.get(champ).is_none(), "{champ} ne doit pas être copié");
        }
        // Lignes : seule la première, à la quantité saisie, au prix du modèle.
        let lignes = corps["items"].as_array().expect("lignes");
        assert_eq!(lignes.len(), 1);
        let l = &lignes[0];
        assert_eq!(l["item_code"], "REPAS");
        assert_eq!(l["qty"], 30.0);
        assert_eq!(l["rate"], 18.5);
        assert_eq!(l["price_list_rate"], 22);
        assert_eq!(l["discount_percentage"], 15.9);
        assert_eq!(l["custom_regime"], "Standard");
        for champ in ["name", "parent", "idx", "doctype", "amount", "base_amount", "prevdoc_docname"] {
            assert!(l.get(champ).is_none(), "ligne : {champ} ne doit pas être copié");
        }
        // Autres tables : sans champs système.
        let taxe = &corps["taxes"][0];
        assert_eq!(taxe["account_head"], "TVA collectée 10 %");
        assert!(taxe.get("name").is_none() && taxe.get("parent").is_none());
        assert_eq!(document.lignes(), [("Repas complet (REPAS)".to_owned(), 30.0)]);
    }

    #[test]
    fn corps_commande_et_facture() {
        let jour = date("2026-11-05");
        let commande = modele(TypeVente::Commande);
        let doc = DocumentDepuisModele::new(
            &commande,
            "CUST-2",
            date("2026-12-01"),
            date("2026-12-03"),
            &[(2, Some(10.0))],
            jour,
        )
        .expect("commande");
        let corps = doc.corps();
        assert_eq!(corps["customer"], "CUST-2");
        assert_eq!(corps["transaction_date"], "2026-11-05");
        assert_eq!(corps["delivery_date"], "2026-12-03");
        assert_eq!(corps["items"][0]["delivery_date"], "2026-12-03");
        assert!(corps.get("party_name").is_none());
        assert_eq!(doc.date_document(), date("2026-12-03"));

        let facture = modele(TypeVente::Facture);
        let doc = DocumentDepuisModele::new(
            &facture,
            "CUST-2",
            date("2026-10-01"),
            date("2026-10-03"),
            &[(0, Some(5.0))],
            jour,
        )
        .expect("facture");
        let corps = doc.corps();
        assert_eq!(corps["posting_date"], "2026-11-05");
        assert_eq!(corps["set_posting_time"], 1);
        assert!(corps.get("due_date").is_none(), "échéance recalculée par Dokos");
    }

    #[test]
    fn creation_avec_controle_de_doublon() {
        let m = modele(TypeVente::Devis);
        let doc = DocumentDepuisModele::new(
            &m,
            "CUST-2",
            date("2026-12-01"),
            date("2026-12-03"),
            &[(0, Some(30.0))],
            date("2026-11-05"),
        )
        .expect("document");
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SAL-QTN-0042", "docstatus": 0, "grand_total": 610.5, "currency": "EUR"}}),
        );
        let creation = creer_depuis_modele(&Dokos::new(&faux), &doc, SiDoublon::Reutiliser).expect("création");
        assert!(matches!(creation, Creation::Cree { ref document, .. } if document.id == "SAL-QTN-0042"));
        let requetes = faux.requetes();
        let filtres: Value = serde_json::from_str(requetes[0].parametre("filters").unwrap_or("[]")).expect("filtres");
        assert!(filtres.as_array().is_some_and(|f| f.contains(&json!(["_user_tags", "not like", "%Template%"]))));
        assert_eq!(requetes[1].methode, Methode::Post);
        assert_eq!(requetes[1].corps.as_ref().map(|c| c["items"][0]["rate"].clone()), Some(json!(18.5)));
    }
}
