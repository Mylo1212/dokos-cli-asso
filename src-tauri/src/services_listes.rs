//! Export Excel : clients, articles, devis, commandes, factures, employés.
//! Lecture seule dans Dokos ; le fichier est écrit dans Documents\Dokos\exports.

use std::path::Path;

use assistant_core::listes::PropositionListe;
use dokos_config::Profil;
use dokos_core::date::Date;
use dokos_core::export::{FiltresExport, Reference, TypeListe, retrouver_reference, valeurs_possibles};
use dokos_core::grand_livre::{JournalFichier, lire_grand_livre, lister_journaux};
use dokos_core::{Dokos, Transport};
use dokos_import::export::{chemin_export, chemin_grand_livre, ecrire_classeur, ecrire_grand_livre};

use crate::erreurs::{ErreurVue, erreur};
use crate::services::Resultat;
use crate::services_ia::vue_choix;
use crate::vues::*;

/// Profil dont la clé lit la liste.
pub fn profil(liste: TypeListe) -> Profil {
    match liste {
        TypeListe::Employes => Profil::Rh,
        _ => Profil::Ventes,
    }
}

pub fn catalogue() -> Vec<ListeVue> {
    TypeListe::TOUS
        .into_iter()
        .map(|l| ListeVue {
            nom: l.nom().to_owned(),
            libelle: l.libelle().to_owned(),
            client: l.champ_client().is_some(),
            groupe: l.groupe().is_some(),
            territoire: l.a_territoire(),
            dates: l.champ_date().map(|(_, libelle)| libelle.to_owned()),
            statuts: l
                .statuts()
                .iter()
                .map(|s| StatutVue { cle: s.cle.to_owned(), libelle: s.libelle.to_owned() })
                .collect(),
            vendables: l == TypeListe::Articles,
            inactifs: l.libelle_inactifs().to_owned(),
            colonnes: l.colonnes().iter().map(|c| c.entete.to_owned()).collect(),
        })
        .collect()
}

/// Libellé français du type d'un journal comptable de Dokos.
fn type_de_journal(type_dokos: &str) -> &str {
    match type_dokos {
        "Sales" => "Ventes",
        "Purchase" => "Achats",
        "Cash" => "Caisse",
        "Bank" => "Banque",
        "Opening" => "À-nouveaux",
        "Miscellaneous" => "Opérations diverses",
        autre => autre,
    }
}

/// Journaux comptables actifs, pour la liste déroulante du grand livre.
pub fn journaux<T: Transport>(dokos: &Dokos<T>) -> Resultat<Vec<JournalVue>> {
    let journaux = lister_journaux(dokos).map_err(erreur)?;
    Ok(journaux
        .into_iter()
        .map(|j| JournalVue {
            libelle: if j.journal_name.is_empty() { j.id.clone() } else { j.journal_name.clone() },
            type_journal: type_de_journal(&j.type_journal).to_owned(),
            code: j.journal_code,
            id: j.id,
        })
        .collect())
}

/// Lit le grand livre du journal sur la période et l'écrit en `dossier/exports/`
/// au format d'import (colonnes DATE à REFINTERNE). Lecture seule dans Dokos.
pub fn exporter_grand_livre<T: Transport>(
    dokos: &Dokos<T>,
    demande: &DemandeGrandLivre,
    dossier: &Path,
) -> Resultat<ResultatGrandLivreVue> {
    let du: Date = demande.du.trim().parse().map_err(erreur)?;
    let au: Date = demande.au.trim().parse().map_err(erreur)?;
    let journal_fichier: JournalFichier = demande.journal_fichier.parse().map_err(ErreurVue::validation)?;
    let grand_livre = lire_grand_livre(dokos, &demande.journal, du, au).map_err(erreur)?;
    let chemin = chemin_grand_livre(dossier, &demande.journal, du, au).map_err(erreur)?;
    ecrire_grand_livre(&chemin, &grand_livre, journal_fichier).map_err(erreur)?;
    tracing::info!(
        journal = %demande.journal,
        journal_fichier = %journal_fichier,
        lignes = grand_livre.lignes.len(),
        "grand livre exporté"
    );
    Ok(ResultatGrandLivreVue {
        chemin: chemin.display().to_string(),
        lignes: grand_livre.lignes.len(),
        total_debit: grand_livre.total_debit,
        total_credit: grand_livre.total_credit,
        ecartees: grand_livre.ecartees,
    })
}

/// Groupes et territoires proposés dans les listes déroulantes de la liste choisie.
pub fn references<T: Transport>(dokos: &Dokos<T>, liste: TypeListe) -> Resultat<ReferencesExportVue> {
    Ok(ReferencesExportVue {
        groupes: valeurs_possibles(dokos, liste, Reference::Groupe).map_err(erreur)?,
        territoires: valeurs_possibles(dokos, liste, Reference::Territoire).map_err(erreur)?,
    })
}

pub fn vue_proposition(p: PropositionListe) -> PropositionListeVue {
    let f = &p.filtres;
    PropositionListeVue {
        liste: p.liste.map(|l| l.nom().to_owned()),
        groupe: f.groupe.clone(),
        territoire: f.territoire.clone(),
        du: f.du.map(|d| d.to_string()),
        au: f.au.map(|d| d.to_string()),
        statut: f.statut.clone(),
        texte: f.texte.clone(),
        inclure_inactifs: f.inclure_inactifs,
        vendables_seulement: f.vendables_seulement,
        description: p.liste.map(|l| f.decrire(l)).unwrap_or_default(),
        client: vue_choix(p.client, |c| ClientVue { id: c.id, nom: c.nom }),
        pret: p.pret,
        questions: p.questions,
        avertissements: p.avertissements,
    }
}

fn option(texte: Option<String>) -> Option<String> {
    texte.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// Groupe ou territoire saisi, retrouvé tel qu'il est écrit dans Dokos.
fn reference<T: Transport>(
    dokos: &Dokos<T>,
    liste: TypeListe,
    reference: Reference,
    texte: Option<String>,
) -> Resultat<Option<String>> {
    retrouver_reference(dokos, liste, reference, texte.as_deref()).map_err(erreur)
}

/// Liste et filtres validés.
pub fn filtres<T: Transport>(dokos: &Dokos<T>, demande: DemandeListe) -> Resultat<(TypeListe, FiltresExport)> {
    let liste: TypeListe = demande.liste.parse().map_err(erreur)?;
    let date = |texte: Option<String>| -> Resultat<Option<Date>> {
        option(texte).map(|t| t.parse::<Date>().map_err(erreur)).transpose()
    };
    let filtres = FiltresExport {
        client: option(demande.client),
        groupe: reference(dokos, liste, Reference::Groupe, demande.groupe)?,
        territoire: reference(dokos, liste, Reference::Territoire, demande.territoire)?,
        du: date(demande.du)?,
        au: date(demande.au)?,
        statut: option(demande.statut),
        texte: option(demande.texte),
        inclure_inactifs: demande.inclure_inactifs,
        vendables_seulement: demande.vendables_seulement,
    };
    filtres.verifier(liste).map_err(erreur)?;
    Ok((liste, filtres))
}

/// Lit la liste et l'écrit dans `dossier/listes/<liste>-<jour>.xlsx`.
pub fn exporter<T: Transport>(
    dokos: &Dokos<T>,
    demande: DemandeListe,
    dossier: &Path,
    jour: Date,
) -> Resultat<ResultatListeVue> {
    let (liste, filtres) = filtres(dokos, demande)?;
    let tableau = dokos_core::export::lire_tableau(dokos, liste, &filtres).map_err(erreur)?;
    let description = filtres.decrire(liste);
    let chemin = chemin_export(dossier, liste, jour).map_err(erreur)?;
    ecrire_classeur(&chemin, &tableau, &description, jour).map_err(erreur)?;
    tracing::info!(liste = %liste, lignes = tableau.lignes.len(), "liste exportée");
    Ok(ResultatListeVue {
        chemin: chemin.display().to_string(),
        lignes: tableau.lignes.len(),
        description,
        tronque: tableau.tronque,
        avertissements: tableau.avertissements,
    })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    fn dossier_test(nom: &str) -> std::path::PathBuf {
        let dossier = std::env::temp_dir().join(format!("dokos-export-test-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).expect("dossier");
        dossier
    }

    fn jour() -> Date {
        "2026-11-04".parse().expect("date")
    }

    #[test]
    fn catalogue_complet() {
        let listes = catalogue();
        assert_eq!(listes.len(), 6);
        let factures = listes.iter().find(|l| l.nom == "factures").expect("factures");
        assert!(factures.client && !factures.groupe && factures.dates.is_some());
        assert!(factures.statuts.iter().any(|s| s.cle == "impayee"));
        assert_eq!(profil(TypeListe::Employes), Profil::Rh);
        assert_eq!(profil(TypeListe::Articles), Profil::Ventes);
    }

    #[test]
    fn grand_livre_exporte_et_journaux_listes() {
        let dokos = FauxTransport::new();
        dokos.repondre_json(
            200,
            &json!({"data": [{"name": "VEN - SLA", "journal_code": "VEN", "journal_name": "Ventes", "type": "Sales"}]}),
        );
        let liste = journaux(&Dokos::new(&dokos)).expect("journaux");
        assert_eq!(
            (liste[0].id.as_str(), liste[0].libelle.as_str(), liste[0].type_journal.as_str()),
            ("VEN - SLA", "Ventes", "Ventes")
        );

        let dossier = dossier_test("grand-livre");
        dokos.repondre_json(
            200,
            &json!({"data": [{"name": "411 - Clients - SLA", "account_number": "411", "account_type": "Receivable"}]}),
        );
        dokos.repondre_json(
            200,
            &json!({"data": [{"posting_date": "2026-10-31", "account": "411 - Clients - SLA", "debit": 120.0, "credit": 0.0, "party": "Mairie", "voucher_no": "F-1"}]}),
        );
        let demande = DemandeGrandLivre {
            journal: "VEN - SLA".into(),
            du: "2026-10-01".into(),
            au: "2026-10-31".into(),
            journal_fichier: "REC".into(),
        };
        let r = exporter_grand_livre(&Dokos::new(&dokos), &demande, &dossier).expect("export");
        assert_eq!((r.lignes, r.total_debit, r.total_credit), (1, 120.0, 0.0));
        assert!(
            r.chemin.contains("exports") && r.chemin.ends_with("grand-livre-VEN-SLA-2026-10-01-2026-10-31.xlsx"),
            "{}",
            r.chemin
        );
        assert!(std::fs::read(&r.chemin).expect("fichier").starts_with(b"PK"));
        // Le journal choisi est écrit dans la colonne JOURNAL.
        let relu = dokos_import::lire_fichier(std::path::Path::new(&r.chemin)).expect("relecture");
        assert_eq!(relu.lignes[0].1[1], "REC");

        // Dates illisibles, période inversée ou journal du fichier inconnu : refusés avant toute lecture.
        let appels = dokos.requetes().len();
        let mauvaise = DemandeGrandLivre { du: "hier".into(), ..demande.clone() };
        assert_eq!(
            exporter_grand_livre(&Dokos::new(&dokos), &mauvaise, &dossier).expect_err("date").genre,
            "validation"
        );
        let inconnu = DemandeGrandLivre { journal_fichier: "ACH".into(), ..demande.clone() };
        assert_eq!(
            exporter_grand_livre(&Dokos::new(&dokos), &inconnu, &dossier).expect_err("journal").genre,
            "validation"
        );
        let inversee = DemandeGrandLivre { du: "2026-11-01".into(), ..demande };
        assert!(exporter_grand_livre(&Dokos::new(&dokos), &inversee, &dossier).is_err());
        assert_eq!(dokos.requetes().len(), appels);
        let _ = std::fs::remove_dir_all(dossier);
    }

    #[test]
    fn references_des_listes_deroulantes() {
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": [{"name": "Associations"}, {"name": "Collectivités"}]}));
        dokos.repondre_json(200, &json!({"data": [{"name": "France"}]}));
        let r = references(&Dokos::new(&dokos), TypeListe::Clients).expect("références");
        assert_eq!(r.groupes, ["Associations", "Collectivités"]);
        assert_eq!(r.territoires, ["France"]);

        // Une liste sans groupe ni territoire : rien à lire.
        let appels = dokos.requetes().len();
        let r = references(&Dokos::new(&dokos), TypeListe::Factures).expect("références");
        assert!(r.groupes.is_empty() && r.territoires.is_empty());
        assert_eq!(dokos.requetes().len(), appels);
    }

    #[test]
    fn groupe_saisi_retrouve_ou_refuse() {
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": [{"name": "Collectivités"}, {"name": "Associations"}]}));
        let demande =
            DemandeListe { liste: "clients".into(), groupe: Some(" collectivites ".into()), ..Default::default() };
        let (liste, f) = filtres(&Dokos::new(&dokos), demande).expect("filtres");
        assert_eq!(liste, TypeListe::Clients);
        assert_eq!(f.groupe.as_deref(), Some("Collectivités"));

        dokos.repondre_json(200, &json!({"data": [{"name": "Collectivités"}]}));
        let demande =
            DemandeListe { liste: "clients".into(), groupe: Some("Entreprises".into()), ..Default::default() };
        assert_eq!(filtres(&Dokos::new(&dokos), demande).expect_err("refus").genre, "validation");

        // Filtre hors liste : refusé sans appel à Dokos.
        let appels = dokos.requetes().len();
        let demande = DemandeListe { liste: "factures".into(), groupe: Some("X".into()), ..Default::default() };
        assert!(filtres(&Dokos::new(&dokos), demande).is_err());
        let demande = DemandeListe { liste: "factures".into(), du: Some("hier".into()), ..Default::default() };
        assert!(filtres(&Dokos::new(&dokos), demande).is_err());
        assert!(filtres(&Dokos::new(&dokos), DemandeListe { liste: "stocks".into(), ..Default::default() }).is_err());
        assert_eq!(dokos.requetes().len(), appels);
    }

    #[test]
    fn export_ecrit_un_classeur() {
        let dossier = dossier_test("export");
        let dokos = FauxTransport::new();
        dokos.repondre_json(
            200,
            &json!({"docs": [{"name": "Sales Invoice", "fields": [
                {"fieldname": "customer_name", "fieldtype": "Data"},
                {"fieldname": "posting_date", "fieldtype": "Date"},
                {"fieldname": "grand_total", "fieldtype": "Currency"},
                {"fieldname": "status", "fieldtype": "Select"},
            ]}]}),
        );
        dokos.repondre_json(200, &json!({"data": [
            {"name": "FAC-1", "customer_name": "Mairie", "posting_date": "2026-10-03", "grand_total": 120.5, "status": "Paid"},
            {"name": "FAC-2", "customer_name": "=1+1", "posting_date": null, "grand_total": 0, "status": "Overdue"},
        ]}));
        let demande = DemandeListe { liste: "factures".into(), statut: Some("payee".into()), ..Default::default() };
        let resultat = exporter(&Dokos::new(&dokos), demande, &dossier, jour()).expect("export");
        assert_eq!(resultat.lignes, 2);
        assert_eq!(resultat.description, "Factures : payées");
        assert!(resultat.chemin.ends_with("factures-2026-11-04.xlsx"), "{}", resultat.chemin);
        assert!(resultat.chemin.contains("exports"), "{}", resultat.chemin);
        assert!(resultat.avertissements[0].contains("colonnes absentes"));
        let octets = std::fs::read(&resultat.chemin).expect("fichier");
        assert!(octets.starts_with(b"PK"), "classeur xlsx (zip)");

        let _ = std::fs::remove_dir_all(dossier);
    }
}
