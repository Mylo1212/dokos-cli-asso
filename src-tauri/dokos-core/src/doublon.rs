//! Contrôle de doublon avant la création d'un devis, d'une commande ou d'une facture.
//!
//! Sans champ personnalisé dans Dokos, un document est un doublon d'un
//! document existant non annulé du même type s'ils ont le même client, la même
//! date et les mêmes lignes : mêmes articles, mêmes quantités (et, pour une
//! commande, mêmes dates de livraison). L'ordre des lignes ne compte pas, et
//! deux lignes du même article à la même date sont additionnées.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::client::{Dokos, Filtre, Liste};
use crate::date::Date;
use crate::erreur::DokosError;
use crate::modele_vente::{filtre_sans_modeles, porte_etiquette_modele};
use crate::transport::Transport;
use crate::types::texte_ou_vide;
use crate::vente::{NouveauDocumentVente, TypeVente};

/// Nombre maximal de documents existants examinés.
pub const CANDIDATS_MAX: u32 = 20;
/// Écart toléré entre deux quantités (Frappe stocke des flottants).
const TOLERANCE_QUANTITE: f64 = 1e-6;

/// Devis, commande ou facture déjà présent dans Dokos.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DocumentExistant {
    #[serde(rename = "name")]
    pub id: String,
    pub docstatus: u8,
    /// Date du document : `transaction_date` (devis, commande) ou `posting_date` (facture).
    #[serde(rename = "transaction_date", alias = "posting_date", default, deserialize_with = "texte_ou_vide")]
    pub date_document: String,
    #[serde(rename = "delivery_date", default, deserialize_with = "texte_ou_vide")]
    pub livraison: String,
    #[serde(rename = "grand_total", default)]
    pub total: f64,
    #[serde(rename = "currency", default, deserialize_with = "texte_ou_vide")]
    pub devise: String,
    #[serde(rename = "items", default)]
    lignes: Vec<LigneExistante>,
    #[serde(rename = "_user_tags", default, deserialize_with = "texte_ou_vide")]
    etiquettes: String,
}

impl DocumentExistant {
    pub fn statut(&self) -> &'static str {
        match self.docstatus {
            0 => "brouillon",
            1 => "validé",
            _ => "annulé",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LigneExistante {
    item_code: String,
    qty: f64,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    delivery_date: String,
    /// Ligne gratuite ajoutée par une règle de prix : pas saisie, donc ignorée.
    #[serde(default)]
    is_free_item: u8,
}

/// Que faire si un document identique existe déjà dans Dokos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiDoublon {
    /// Ne rien créer et renvoyer le document existant. Une relance après
    /// plantage ne crée donc pas de deuxième document.
    Reutiliser,
    /// Créer quand même un nouveau document.
    CreerQuandMeme,
}

/// Documents existants qui ressemblent au nouveau.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doublons {
    /// Même client, même date et mêmes lignes.
    pub identiques: Vec<DocumentExistant>,
    /// Même client et même date (ou mêmes dates de livraison), mais lignes différentes.
    pub similaires: Vec<DocumentExistant>,
}

impl Doublons {
    pub fn est_vide(&self) -> bool {
        self.identiques.is_empty() && self.similaires.is_empty()
    }
}

/// Cherche dans Dokos les documents non annulés du même type, du même client
/// (et de la même société si elle est précisée) à la même date. Lecture seule.
pub fn chercher_doublons<T: Transport>(
    dokos: &Dokos<T>,
    document: &NouveauDocumentVente,
) -> Result<Doublons, DokosError> {
    let type_vente = document.type_vente();
    let mut liste = Liste::new(&["name"])
        .filtre(Filtre::egal(type_vente.champ_client(), document.client()))
        .filtre(Filtre::different("docstatus", 2))
        // Les modèles (étiquette « Template ») ne sont jamais des doublons.
        .filtre(filtre_sans_modeles());
    liste = match type_vente {
        TypeVente::Devis => liste
            .filtre(Filtre::egal("quotation_to", "Customer"))
            .filtre(Filtre::egal("transaction_date", document.date().to_string())),
        TypeVente::Commande => {
            // Dokos remplace la date de livraison de l'en-tête par la plus tardive des
            // lignes : on cherche donc sur toute la période, pas sur l'en-tête seul.
            let (debut, fin) = periode_livraison(document);
            liste.filtre(Filtre::entre("delivery_date", debut.to_string(), fin.to_string()))
        }
        TypeVente::Facture => liste.filtre(Filtre::egal("posting_date", document.date().to_string())),
    };
    liste = liste.ordre("creation desc").limite(CANDIDATS_MAX);
    if let Some(societe) = document.societe() {
        liste = liste.filtre(Filtre::egal("company", societe));
    }

    #[derive(Deserialize)]
    struct Nom {
        name: String,
    }
    let noms: Vec<Nom> = dokos.lister(type_vente.doctype(), &liste)?;

    // Les lignes ne sont pas dans la liste : chaque candidat est lu, de front
    // sur le réseau réel. `pour_chacun` plutôt que `lire_lot` : pendant un
    // import, ce contrôle tourne déjà dans un fil et ne doit pas en ouvrir d'autres.
    let existants: Vec<DocumentExistant> =
        dokos.pour_chacun(&noms, |nom| dokos.lire(type_vente.doctype(), &nom.name))?;

    let attendu = lignes_nouvelles(document);
    let mut doublons = Doublons::default();
    for existant in existants {
        if existant.docstatus == 2 || porte_etiquette_modele(&existant.etiquettes) {
            continue;
        }
        if memes_lignes(&lignes_existantes(type_vente, &existant), &attendu) {
            doublons.identiques.push(existant);
        } else {
            doublons.similaires.push(existant);
        }
    }
    Ok(doublons)
}

/// Première et dernière date de livraison, en-tête et lignes compris.
fn periode_livraison(document: &NouveauDocumentVente) -> (Date, Date) {
    let entete = document.date();
    document
        .lignes()
        .iter()
        .map(|l| l.livraison().unwrap_or(entete))
        .fold((entete, entete), |(debut, fin), date| (debut.min(date), fin.max(date)))
}

/// Quantité totale par (article, date de livraison). La date est vide hors commande.
type Empreinte = BTreeMap<(String, String), f64>;

fn lignes_nouvelles(document: &NouveauDocumentVente) -> Empreinte {
    let mut empreinte = Empreinte::new();
    for ligne in document.lignes() {
        let date = match document.type_vente() {
            TypeVente::Commande => ligne.livraison().unwrap_or(document.date()).to_string(),
            TypeVente::Devis | TypeVente::Facture => String::new(),
        };
        *empreinte.entry((ligne.article().to_owned(), date)).or_default() += ligne.quantite();
    }
    empreinte
}

fn lignes_existantes(type_vente: TypeVente, document: &DocumentExistant) -> Empreinte {
    let mut empreinte = Empreinte::new();
    for ligne in document.lignes.iter().filter(|l| l.is_free_item == 0) {
        let date = match type_vente {
            TypeVente::Commande if ligne.delivery_date.is_empty() => document.livraison.clone(),
            TypeVente::Commande => ligne.delivery_date.clone(),
            TypeVente::Devis | TypeVente::Facture => String::new(),
        };
        *empreinte.entry((ligne.item_code.clone(), date)).or_default() += ligne.qty;
    }
    empreinte
}

fn memes_lignes(a: &Empreinte, b: &Empreinte) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((cle_a, qte_a), (cle_b, qte_b))| cle_a == cle_b && (qte_a - qte_b).abs() < TOLERANCE_QUANTITE)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;
    use crate::vente::LigneVente;

    fn date(texte: &str) -> Date {
        texte.parse().expect("date de test")
    }

    /// 25 repas le 12/11, grande salle le 13/11.
    fn commande_exemple() -> NouveauDocumentVente {
        let lignes = vec![
            LigneVente::new("REPAS", 25.0, None).expect("ligne"),
            LigneVente::new("GRANDE-SALLE", 1.0, Some(date("2026-11-13"))).expect("ligne"),
        ];
        NouveauDocumentVente::new(TypeVente::Commande, "CUST-2", date("2026-11-12"), None, lignes).expect("commande")
    }

    fn existante(name: &str, docstatus: u8, items: Value) -> Value {
        json!({"data": {
            "name": name, "docstatus": docstatus, "customer": "CUST-2",
            "transaction_date": "2026-11-03", "delivery_date": "2026-11-13",
            "grand_total": 512.5, "currency": "EUR", "items": items,
        }})
    }

    fn lignes_exemple() -> Value {
        json!([
            {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": "2026-11-13"},
            {"item_code": "REPAS", "qty": 25.0, "delivery_date": "2026-11-12"},
        ])
    }

    #[test]
    fn requete_de_recherche() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        let commande = commande_exemple().avec_societe(Some("Les Amanins"));
        let doublons = chercher_doublons(&Dokos::new(&faux), &commande).expect("recherche");
        assert!(doublons.est_vide());

        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 1);
        assert_eq!(requetes[0].methode, Methode::Get);
        assert_eq!(requetes[0].chemin, "/api/resource/Sales%20Order");
        assert_eq!(
            requetes[0].parametre("filters"),
            Some(concat!(
                r#"[["customer","=","CUST-2"],["docstatus","!=",2],["_user_tags","not like","%Template%"],"#,
                r#"["delivery_date","between",["2026-11-12","2026-11-13"]],["company","=","Les Amanins"]]"#
            ))
        );
        assert_eq!(requetes[0].parametre("limit_page_length"), Some("20"));
    }

    #[test]
    fn identique_dans_un_autre_ordre() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "SO-1"}]}));
        faux.repondre_json(200, &existante("SO-1", 0, lignes_exemple()));
        let doublons = chercher_doublons(&Dokos::new(&faux), &commande_exemple()).expect("recherche");
        assert_eq!(doublons.identiques.len(), 1);
        assert_eq!(doublons.identiques[0].id, "SO-1");
        assert_eq!(doublons.identiques[0].statut(), "brouillon");
        assert!(doublons.similaires.is_empty());
        assert_eq!(faux.requetes()[1].chemin, "/api/resource/Sales%20Order/SO-1");
    }

    #[test]
    fn lignes_scindees_et_ligne_gratuite() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "SO-1"}]}));
        faux.repondre_json(
            200,
            &existante(
                "SO-1",
                1,
                json!([
                    {"item_code": "REPAS", "qty": 10.0, "delivery_date": "2026-11-12"},
                    {"item_code": "REPAS", "qty": 15.0, "delivery_date": "2026-11-12"},
                    {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": null},
                    {"item_code": "CAFE-OFFERT", "qty": 25.0, "delivery_date": "2026-11-12", "is_free_item": 1},
                ]),
            ),
        );
        let doublons = chercher_doublons(&Dokos::new(&faux), &commande_exemple()).expect("recherche");
        assert_eq!(doublons.identiques.len(), 1, "{doublons:?}");
        assert_eq!(doublons.identiques[0].statut(), "validé");
    }

    #[test]
    fn differences_qui_comptent() {
        let variantes = [
            json!([{"item_code": "REPAS", "qty": 24.0, "delivery_date": "2026-11-12"},
                   {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": "2026-11-13"}]),
            json!([{"item_code": "REPAS", "qty": 25.0, "delivery_date": "2026-11-13"},
                   {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": "2026-11-13"}]),
            json!([{"item_code": "REPAS", "qty": 25.0, "delivery_date": "2026-11-12"}]),
            json!([{"item_code": "REPAS", "qty": 25.0, "delivery_date": "2026-11-12"},
                   {"item_code": "GRANDE-SALLE", "qty": 1.0, "delivery_date": "2026-11-13"},
                   {"item_code": "VIN", "qty": 1.0, "delivery_date": "2026-11-12"}]),
        ];
        for items in variantes {
            let faux = FauxTransport::new();
            faux.repondre_json(200, &json!({"data": [{"name": "SO-1"}]}));
            faux.repondre_json(200, &existante("SO-1", 0, items.clone()));
            let doublons = chercher_doublons(&Dokos::new(&faux), &commande_exemple()).expect("recherche");
            assert!(doublons.identiques.is_empty(), "{items}");
            assert_eq!(doublons.similaires.len(), 1, "{items}");
        }
    }

    #[test]
    fn commande_annulee_ignoree() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "SO-1"}]}));
        faux.repondre_json(200, &existante("SO-1", 2, lignes_exemple()));
        let doublons = chercher_doublons(&Dokos::new(&faux), &commande_exemple()).expect("recherche");
        assert!(doublons.est_vide());
    }

    #[test]
    fn erreur_de_lecture_propagee() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "SO-1"}]}));
        faux.repondre(403, "{}");
        let resultat = chercher_doublons(&Dokos::new(&faux), &commande_exemple());
        assert!(matches!(resultat, Err(DokosError::Permission(_))));
    }

    #[test]
    fn periode_avec_lignes_anterieures() {
        let lignes = vec![LigneVente::new("REPAS", 1.0, Some(date("2026-11-10"))).expect("ligne")];
        let commande =
            NouveauDocumentVente::new(TypeVente::Commande, "C", date("2026-11-12"), None, lignes).expect("commande");
        assert_eq!(periode_livraison(&commande), (date("2026-11-10"), date("2026-11-12")));
    }

    #[test]
    fn filtres_devis_et_facture() {
        let ligne = || LigneVente::new("REPAS", 25.0, None).expect("ligne");
        let jour = date("2026-11-03");
        for (type_vente, chemin, filtres) in [
            (
                TypeVente::Devis,
                "/api/resource/Quotation",
                r#"[["party_name","=","CUST-2"],["docstatus","!=",2],["_user_tags","not like","%Template%"],["quotation_to","=","Customer"],["transaction_date","=","2026-11-03"]]"#,
            ),
            (
                TypeVente::Facture,
                "/api/resource/Sales%20Invoice",
                r#"[["customer","=","CUST-2"],["docstatus","!=",2],["_user_tags","not like","%Template%"],["posting_date","=","2026-11-03"]]"#,
            ),
        ] {
            let faux = FauxTransport::new();
            faux.repondre_json(200, &json!({"data": [{"name": "X-1"}]}));
            // Les lignes existantes ont une date de livraison : elle est ignorée hors commande.
            faux.repondre_json(
                200,
                &json!({"data": {"name": "X-1", "docstatus": 0, "posting_date": "2026-11-03",
                                 "items": [{"item_code": "REPAS", "qty": 25.0, "delivery_date": "2026-11-20"}]}}),
            );
            let document =
                NouveauDocumentVente::new(type_vente, "CUST-2", jour, None, vec![ligne()]).expect("document");
            let doublons = chercher_doublons(&Dokos::new(&faux), &document).expect("recherche");
            assert_eq!(doublons.identiques.len(), 1, "{type_vente}");
            let requete = &faux.requetes()[0];
            assert_eq!(requete.chemin, chemin);
            assert_eq!(requete.parametre("filters"), Some(filtres));
        }
    }
}
