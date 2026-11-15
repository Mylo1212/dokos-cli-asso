//! Devis, commandes et factures à partir d'un modèle « Template » de Dokos.

use dokos_core::date::Date;
use dokos_core::modele_vente::{
    DocumentDepuisModele, ModeleVente, creer_depuis_modele, lire_modele, lister_modeles, verifier_version,
};
use dokos_core::vente::{Creation, SiDoublon};
use dokos_core::{Dokos, Transport};

use crate::erreurs::erreur;
use crate::services::{Resultat, lien_desk, type_vente, vue_document};
use crate::vues::*;

pub fn modeles<T: Transport>(dokos: &Dokos<T>, type_nom: &str) -> Resultat<Vec<ResumeModeleVue>> {
    let modeles = lister_modeles(dokos, type_vente(type_nom)?).map_err(erreur)?;
    Ok(modeles
        .into_iter()
        .map(|m| ResumeModeleVue {
            titre: if m.titre.is_empty() { m.id.clone() } else { m.titre },
            id: m.id,
            total: m.total,
            devise: m.devise,
        })
        .collect())
}

pub fn vue_modele(base: &str, m: &ModeleVente) -> ModeleVenteVue {
    ModeleVenteVue {
        id: m.id.clone(),
        titre: m.titre.clone(),
        devise: m.devise.clone(),
        modifie: m.modifie.clone(),
        lignes: m
            .lignes
            .iter()
            .map(|l| LigneModeleVue {
                index: l.index,
                code: l.code.clone(),
                libelle: l.libelle.clone(),
                quantite: l.quantite,
                prix: l.prix,
                unite: l.unite.clone(),
            })
            .collect(),
        lien: lien_desk(base, m.type_vente.doctype(), &m.id),
    }
}

pub fn modele<T: Transport>(dokos: &Dokos<T>, base: &str, type_nom: &str, id: &str) -> Resultat<ModeleVenteVue> {
    let m = lire_modele(dokos, type_vente(type_nom)?, id).map_err(erreur)?;
    Ok(vue_modele(base, &m))
}

/// Relit le modèle (inchangé depuis l'affichage) et construit le document.
fn document<T: Transport>(
    dokos: &Dokos<T>,
    demande: &DemandeVenteModele,
    aujourdhui: Date,
) -> Resultat<DocumentDepuisModele> {
    let modele = lire_modele(dokos, type_vente(&demande.type_vente)?, &demande.modele).map_err(erreur)?;
    verifier_version(&modele, &demande.modifie).map_err(erreur)?;
    let arrivee: Date = demande.arrivee.trim().parse().map_err(erreur)?;
    let depart: Date = demande.depart.trim().parse().map_err(erreur)?;
    let quantites: Vec<(usize, Option<f64>)> = demande.quantites.iter().map(|q| (q.index, q.quantite)).collect();
    DocumentDepuisModele::new(&modele, &demande.client, arrivee, depart, &quantites, aujourdhui).map_err(erreur)
}

pub fn apercu<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeVenteModele,
    aujourdhui: Date,
) -> Resultat<ApercuVente> {
    let doc = document(dokos, demande, aujourdhui)?;
    let t = doc.modele().type_vente;
    let doublons = dokos_core::doublon::chercher_doublons(dokos, &doc.empreinte().map_err(erreur)?).map_err(erreur)?;
    Ok(ApercuVente {
        identiques: doublons.identiques.iter().map(|d| vue_document(base, t, d)).collect(),
        similaires: doublons.similaires.iter().map(|d| vue_document(base, t, d)).collect(),
    })
}

pub fn creer<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeVenteModele,
    si_doublon: SiDoublon,
    aujourdhui: Date,
) -> Resultat<ResultatVente> {
    let doc = document(dokos, demande, aujourdhui)?;
    let t = doc.modele().type_vente;
    match creer_depuis_modele(dokos, &doc, si_doublon).map_err(erreur)? {
        Creation::DejaExistant(existant) => {
            Ok(ResultatVente { cree: false, document: vue_document(base, t, &existant), autres: Vec::new() })
        }
        Creation::Cree { document: cree, autres } => Ok(ResultatVente {
            cree: true,
            document: DocumentVue {
                id: cree.id.clone(),
                statut: "brouillon".to_owned(),
                date: doc.date_document().to_string(),
                total: cree.total,
                devise: cree.devise,
                lien: lien_desk(base, t.doctype(), &cree.id),
            },
            autres: autres.iter().map(|d| vue_document(base, t, d)).collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    fn repondre_modele(faux: &FauxTransport, modifie: &str) {
        faux.repondre_json(200, &json!({"data": [{"name": "SAL-ORD-0003", "_user_tags": ",Template"}]}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SAL-ORD-0003", "title": "Classe verte", "modified": modifie, "currency": "EUR",
                             "items": [{"item_code": "REPAS", "item_name": "Repas", "qty": 25, "rate": 9.5, "uom": "Unité"}]}}),
        );
        faux.repondre_json(
            200,
            &json!({"docs": [{"name": "Sales Order", "fields": [
                {"fieldname": "custom_date_arrivee", "fieldtype": "Date"},
                {"fieldname": "custom_date_depart", "fieldtype": "Date"}]}]}),
        );
    }

    fn demande(modifie: &str) -> DemandeVenteModele {
        DemandeVenteModele {
            type_vente: "commande".into(),
            modele: "SAL-ORD-0003".into(),
            modifie: modifie.into(),
            client: "CUST-2".into(),
            arrivee: "2026-12-01".into(),
            depart: "2026-12-04".into(),
            quantites: vec![QuantiteModele { index: 0, quantite: Some(28.0) }],
        }
    }

    #[test]
    fn creation_depuis_le_modele() {
        let faux = FauxTransport::new();
        repondre_modele(&faux, "v1");
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SAL-ORD-0100", "docstatus": 0, "grand_total": 266, "currency": "EUR"}}),
        );
        let r = creer(
            &Dokos::new(&faux),
            "https://d.test",
            &demande("v1"),
            SiDoublon::Reutiliser,
            "2026-11-05".parse().expect("date"),
        )
        .expect("création");
        assert!(r.cree);
        assert_eq!(r.document.date, "2026-12-04", "date de livraison = départ");
        assert_eq!(r.document.lien, "https://d.test/app/sales-order/SAL-ORD-0100");
    }

    #[test]
    fn modele_change_entre_temps() {
        let faux = FauxTransport::new();
        repondre_modele(&faux, "v2");
        let erreur = apercu(&Dokos::new(&faux), "https://d.test", &demande("v1"), "2026-11-05".parse().expect("date"))
            .expect_err("modifié");
        assert_eq!(erreur.genre, "validation");
        assert!(erreur.message.contains("modifié"));
    }
}
