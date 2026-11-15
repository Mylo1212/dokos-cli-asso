//! Suivi : mes brouillons, transformation de documents, inscrits d'un
//! événement, alertes RH, factures en retard. Lecture seule, sauf la
//! transformation (création d'un brouillon).

use conformite::Regles;
use dokos_core::brouillons::{Brouillon, brouillons_de_vente, evenements_non_publies};
use dokos_core::contrat::DOCTYPE_CONTRAT;
use dokos_core::date::Date;
use dokos_core::doublon::SiDoublon;
use dokos_core::evenement::DOCTYPE_EVENEMENT;
use dokos_core::formulaire::{CHAMPS_DE_BASE, DOCTYPE_INSCRIPTION, questions_possibles};
use dokos_core::inscription::{lister_evenements, lister_inscrits};
use dokos_core::meta::lire_meta;
use dokos_core::suivi::{EcheanceContrat, Echeances, echeances_de_contrats, factures_en_retard, soldes_par_client};
use dokos_core::transformation::{ResultatTransformation, Transformation, transformer};
use dokos_core::types::DOCTYPE_FACTURE;
use dokos_core::{Dokos, Transport};

use crate::erreurs::{ErreurVue, erreur as texte};
use crate::services::{Resultat, lien_desk};
use crate::vues::*;

/// Jours d'horizon des alertes RH.
pub const HORIZON_RH: i64 = 30;
/// Jours pendant lesquels un contrat est « récent » (période d'essai probable).
pub const RECENT_RH: i64 = 60;

fn vue_brouillon(base: &str, b: Brouillon) -> BrouillonVue {
    let genre = match b.doctype {
        "Quotation" => "devis",
        "Sales Order" => "commande",
        "Sales Invoice" => "facture",
        _ => "evenement",
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

/// Événements non publiés de l'utilisateur API du profil événements.
pub fn brouillons_evenements<T: Transport>(dokos: &Dokos<T>, base: &str) -> Resultat<Vec<BrouillonVue>> {
    let utilisateur = dokos.utilisateur().map_err(texte)?;
    Ok(evenements_non_publies(dokos, &utilisateur)
        .map_err(texte)?
        .into_iter()
        .map(|b| vue_brouillon(base, b))
        .collect())
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

/// Événements depuis trente jours, pour choisir celui dont on veut les inscrits.
pub fn evenements<T: Transport>(dokos: &Dokos<T>, base: &str, aujourdhui: Date) -> Resultat<Vec<EvenementVue>> {
    let depuis = aujourdhui.plus_jours(-30).unwrap_or(aujourdhui).to_string();
    Ok(lister_evenements(dokos, &depuis, 100)
        .map_err(texte)?
        .into_iter()
        .map(|e| EvenementVue {
            lien: lien_desk(base, DOCTYPE_EVENEMENT, &e.id),
            id: e.id,
            titre: e.titre,
            debut: e.debut,
            publie: e.published == 1,
        })
        .collect())
}

/// Inscrits d'un événement : colonnes de base, puis questions du formulaire.
pub fn inscrits<T: Transport>(dokos: &Dokos<T>, evenement: &str) -> Resultat<InscritsVue> {
    let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION).map_err(texte)?;
    let mut colonnes: Vec<ChampVue> = CHAMPS_DE_BASE
        .iter()
        .filter(|(nom, _)| *nom != "event")
        .filter_map(|(nom, obligatoire)| {
            meta.champ(nom).map(|c| ChampVue {
                champ: c.fieldname.clone(),
                libelle: c.libelle().to_owned(),
                type_champ: c.fieldtype.clone(),
                obligatoire: *obligatoire,
            })
        })
        .collect();
    colonnes.extend(questions_possibles(&meta).iter().map(|c| ChampVue {
        champ: c.fieldname.clone(),
        libelle: c.libelle().to_owned(),
        type_champ: c.fieldtype.clone(),
        obligatoire: false,
    }));
    let lignes = lister_inscrits(dokos, evenement).map_err(texte)?;
    Ok(InscritsVue { evenement: evenement.to_owned(), colonnes, lignes })
}

/// Tableau des inscrits : en-têtes puis lignes, pour l'export.
pub fn tableau_inscrits(vue: &InscritsVue) -> (Vec<String>, Vec<Vec<String>>) {
    let entetes = vue.colonnes.iter().map(|c| c.libelle.clone()).collect();
    let lignes = vue
        .lignes
        .iter()
        .map(|l| vue.colonnes.iter().map(|c| l.get(&c.champ).cloned().unwrap_or_default()).collect())
        .collect();
    (entetes, lignes)
}

fn vue_echeance(base: &str, aujourdhui: Date, e: EcheanceContrat, par_rapport_a_la_fin: bool) -> EcheanceVue {
    let reference = if par_rapport_a_la_fin { &e.relieving_date } else { &e.date_of_joining };
    let jours = reference
        .parse::<Date>()
        .map(|d| if par_rapport_a_la_fin { aujourdhui.jours_jusqua(d) } else { d.jours_jusqua(aujourdhui) })
        .unwrap_or(0);
    EcheanceVue {
        lien: lien_desk(base, DOCTYPE_CONTRAT, &e.id),
        id: e.id,
        employe: e.employee,
        nom: e.employee_name,
        type_contrat: e.contract_type,
        entree: e.date_of_joining,
        fin: e.relieving_date,
        jours,
    }
}

pub fn alertes_rh<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    regles: &Regles,
    aujourdhui: Date,
) -> Resultat<AlertesRhVue> {
    let Echeances { fins, sans_fin, recents } =
        echeances_de_contrats(dokos, aujourdhui, HORIZON_RH, &regles.categories.cdd, RECENT_RH).map_err(texte)?;
    Ok(AlertesRhVue {
        fins_proches: fins.into_iter().map(|e| vue_echeance(base, aujourdhui, e, true)).collect(),
        cdd_sans_fin: sans_fin.into_iter().map(|e| vue_echeance(base, aujourdhui, e, true)).collect(),
        recents: recents.into_iter().map(|e| vue_echeance(base, aujourdhui, e, false)).collect(),
        horizon_jours: HORIZON_RH,
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
    fn inscrits_avec_questions() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"docs": [{"name": DOCTYPE_INSCRIPTION, "fields": [
                {"fieldname": "event", "fieldtype": "Link", "label": "Event"},
                {"fieldname": "first_name", "fieldtype": "Data", "label": "Prénom"},
                {"fieldname": "last_name", "fieldtype": "Data", "label": "Nom"},
                {"fieldname": "email", "fieldtype": "Data", "label": "E-mail"},
                {"fieldname": "mobile_number", "fieldtype": "Data", "label": "Téléphone"},
                {"fieldname": "allergies", "fieldtype": "Small Text", "label": "Allergies"},
            ]}]}),
        );
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "R-1", "first_name": "Marie", "last_name": "Dupont", "allergies": "arachide"}]}),
        );
        let vue = inscrits(&Dokos::new(&faux), "EV-1").expect("inscrits");
        assert_eq!(
            vue.colonnes.iter().map(|c| c.champ.as_str()).collect::<Vec<_>>(),
            ["first_name", "last_name", "email", "mobile_number", "allergies"]
        );
        let (entetes, lignes) = tableau_inscrits(&vue);
        assert_eq!(entetes[4], "Allergies");
        assert_eq!(lignes[0], ["Marie", "Dupont", "", "", "arachide"]);
        assert_eq!(vue.lignes[0].get("allergies").map(String::as_str), Some("arachide"));
    }

    #[test]
    fn alertes_et_impayes() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "C-1", "employee_name": "Marie", "relieving_date": "2026-11-20", "date_of_joining": "2026-06-01"}]}));
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "C-2", "employee_name": "Paul", "date_of_joining": "2026-10-15"}]}),
        );
        let vue = alertes_rh(&Dokos::new(&faux), BASE, &Regles::par_defaut(), jour()).expect("alertes");
        assert_eq!(vue.fins_proches[0].jours, 17);
        assert_eq!(vue.recents[0].jours, 19);
        assert!(vue.cdd_sans_fin.is_empty());

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
