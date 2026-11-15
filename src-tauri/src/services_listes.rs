//! Export Excel : clients, articles, devis, commandes, factures, employés.
//! Lecture seule dans Dokos ; le fichier est écrit dans Documents\Dokos\exports.

use std::path::Path;

use assistant_core::listes::PropositionListe;
use dokos_config::Profil;
use dokos_core::date::Date;
use dokos_core::export::{FiltresExport, Reference, TypeListe, retrouver_reference, valeurs_possibles};
use dokos_core::{Dokos, Transport};
use dokos_import::export::{chemin_export, ecrire_classeur};

use crate::erreurs::erreur;
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
