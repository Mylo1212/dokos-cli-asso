//! Suivi : mes brouillons, transformation de documents, factures en
//! retard. Lecture seule, sauf la transformation (création d'un brouillon).

use dokos_core::brouillons::{Brouillon, brouillons_de_vente};
use dokos_core::date::Date;
use dokos_core::doublon::SiDoublon;
use dokos_core::suivi::{factures_en_retard, soldes_par_client};
use dokos_core::transformation::{ResultatTransformation, Transformation, transformer};
use dokos_core::types::DOCTYPE_FACTURE;
use dokos_core::{Dokos, Transport};

use crate::erreurs::{ErreurVue, erreur as texte};
use crate::services::{Resultat, lien_desk};
use crate::vues::*;

fn vue_brouillon(base: &str, b: Brouillon) -> BrouillonVue {
    let genre = match b.doctype {
        "Quotation" => "devis",
        "Sales Order" => "commande",
        _ => "facture",
    };
    BrouillonVue {
        genre: genre.to_owned(),
        lien: lien_desk(base, b.doctype, &b.id),
        id: b.id,
        tiers: b.tiers,
        date: b.date,
        total: b.total,
        devise: b.devise,
        modifie: b.modifie,
    }
}

/// Brouillons de vente de l'utilisateur API du profil ventes.
pub fn brouillons_ventes<T: Transport>(dokos: &Dokos<T>, base: &str) -> Resultat<Vec<BrouillonVue>> {
    let utilisateur = dokos.utilisateur().map_err(texte)?;
    Ok(brouillons_de_vente(dokos, &utilisateur).map_err(texte)?.into_iter().map(|b| vue_brouillon(base, b)).collect())
}

pub fn transformation(genre: &str) -> Resultat<Transformation> {
    match genre {
        "devis_en_commande" => Ok(Transformation::DevisEnCommande),
        "commande_en_facture" => Ok(Transformation::CommandeEnFacture),
        autre => Err(ErreurVue::validation(format!("transformation inconnue : {autre}"))),
    }
}

pub fn transformer_un_document<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    transformation: Transformation,
    source: &str,
    si_doublon: SiDoublon,
) -> Resultat<ResultatTransformationVue> {
    Ok(match transformer(dokos, transformation, source, si_doublon).map_err(texte)? {
        ResultatTransformation::Cree(d) => ResultatTransformationVue {
            cree: true,
            document: DocumentVue {
                lien: lien_desk(base, transformation.cible(), &d.id),
                id: d.id,
                statut: "brouillon".to_owned(),
                date: String::new(),
                total: d.total,
                devise: d.devise,
            },
        },
        ResultatTransformation::DejaExistant(d) => ResultatTransformationVue {
            cree: false,
            document: DocumentVue {
                lien: lien_desk(base, transformation.cible(), &d.id),
                id: d.id.clone(),
                statut: d.statut().to_owned(),
                date: d.date_document.clone(),
                total: d.total,
                devise: d.devise.clone(),
            },
        },
    })
}

pub fn impayes<T: Transport>(dokos: &Dokos<T>, base: &str, aujourdhui: Date) -> Resultat<ImpayesVue> {
    let factures = factures_en_retard(dokos, aujourdhui).map_err(texte)?;
    let par_client = soldes_par_client(&factures)
        .into_iter()
        .map(|(client, solde, nombre)| SoldeVue { client, solde, nombre })
        .collect();
    let total = factures.iter().map(|f| f.outstanding_amount).sum();
    let devise = factures.iter().map(|f| f.currency.clone()).find(|d| !d.is_empty()).unwrap_or_default();
    Ok(ImpayesVue {
        factures: factures
            .into_iter()
            .map(|f| FactureRetardVue {
                retard: f.retard(aujourdhui),
                lien: lien_desk(base, DOCTYPE_FACTURE, &f.id),
                id: f.id,
                client: if f.customer_name.is_empty() { f.customer } else { f.customer_name },
                date: f.posting_date,
                echeance: f.due_date,
                total: f.grand_total,
                solde: f.outstanding_amount,
                devise: f.currency,
            })
            .collect(),
        par_client,
        total,
        devise,
    })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    const BASE: &str = "https://dokos.exemple.fr";

    fn jour() -> Date {
        "2026-11-03".parse().expect("date")
    }

    #[test]
    fn brouillons_par_utilisateur() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"message": "api-ventes@exemple.fr"}));
        faux.repondre_json(200, &json!({"data": [{"name": "Q-1", "party_name": "Mairie", "transaction_date": "2026-11-02", "modified": "2026-11-02 10:00:00"}]}));
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": []}));
        let vue = brouillons_ventes(&Dokos::new(&faux), BASE).expect("brouillons");
        assert_eq!(vue[0].genre, "devis");
        assert_eq!(vue[0].lien, "https://dokos.exemple.fr/app/quotation/Q-1");
    }

    #[test]
    fn transformation_devis() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"message": {"doctype": "Sales Order", "customer": "C"}}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SO-1", "docstatus": 0, "grand_total": 10.0, "currency": "EUR"}}),
        );
        let t = transformation("devis_en_commande").expect("genre");
        let r =
            transformer_un_document(&Dokos::new(&faux), BASE, t, "Q-1", SiDoublon::Reutiliser).expect("transformation");
        assert!(r.cree);
        assert_eq!(r.document.lien, "https://dokos.exemple.fr/app/sales-order/SO-1");
        assert!(transformation("x").is_err());
    }

    #[test]
    fn impayes_par_client() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [
            {"name": "F-1", "customer": "C1", "customer_name": "Mairie", "due_date": "2026-10-04", "outstanding_amount": 100.0, "grand_total": 100.0, "currency": "EUR"},
            {"name": "F-2", "customer": "C1", "customer_name": "Mairie", "due_date": "2026-10-24", "outstanding_amount": 50.0, "grand_total": 80.0, "currency": "EUR"},
        ]}));
        let vue = impayes(&Dokos::new(&faux), BASE, jour()).expect("impayés");
        assert_eq!(vue.factures[0].retard, 30);
        assert!((vue.total - 150.0).abs() < f64::EPSILON);
        assert_eq!(vue.par_client[0].nombre, 2);
        assert_eq!(vue.devise, "EUR");
    }
}
