//! Listes Excel : clients, articles, devis, commandes, factures, employés.
//! Lecture seule dans Dokos ; le fichier est écrit dans Documents\Dokos\listes.

use std::path::{Path, PathBuf};

use assistant_core::listes::PropositionListe;
use dokos_config::Profil;
use dokos_core::date::Date;
use dokos_core::export::{Cellule, FiltresExport, Reference, Tableau, TypeListe, choisir_valeur, valeurs_possibles};
use dokos_core::recherche::Correspondance;
use dokos_core::{Dokos, Transport};

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
    let Some(texte) = option(texte) else { return Ok(None) };
    let nom = match reference {
        Reference::Groupe => "groupe",
        Reference::Territoire => "territoire",
    };
    let possible = match reference {
        Reference::Groupe => liste.groupe().is_some(),
        Reference::Territoire => liste.a_territoire(),
    };
    if !possible {
        // Refusé ensuite par la vérification des filtres, avec le bon message.
        return Ok(Some(texte));
    }
    match choisir_valeur(&texte, &valeurs_possibles(dokos, liste, reference).map_err(erreur)?) {
        Correspondance::Unique(v) => Ok(Some(v)),
        Correspondance::Plusieurs(v) => {
            Err(ErreurVue::validation(format!("plusieurs {nom}s correspondent à « {texte} » : {}", v.join(", "))))
        }
        Correspondance::Aucune => Err(ErreurVue::validation(format!("{nom} « {texte} » introuvable dans Dokos"))),
    }
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
    let chemin = chemin_export(dossier, liste, jour)?;
    ecrire_classeur(&chemin, &tableau, &description, jour)?;
    tracing::info!(liste = %liste, lignes = tableau.lignes.len(), "liste exportée");
    Ok(ResultatListeVue {
        chemin: chemin.display().to_string(),
        lignes: tableau.lignes.len(),
        description,
        tronque: tableau.tronque,
        avertissements: tableau.avertissements,
    })
}

/// Nouveau fichier à chaque export : `clients-2026-11-04.xlsx`, puis `-2`, `-3`…
pub fn chemin_export(dossier: &Path, liste: TypeListe, jour: Date) -> Resultat<PathBuf> {
    let dossier = dossier.join("listes");
    std::fs::create_dir_all(&dossier).map_err(|e| ErreurVue::fichier(format!("{} : {e}", dossier.display())))?;
    let base = format!("{}-{jour}", liste.nom());
    let mut chemin = dossier.join(format!("{base}.xlsx"));
    let mut n = 2;
    while chemin.exists() {
        if n > 999 {
            return Err(ErreurVue::fichier(format!("trop d'exports {base} dans {}", dossier.display())));
        }
        chemin = dossier.join(format!("{base}-{n}.xlsx"));
        n += 1;
    }
    Ok(chemin)
}

/// Feuille de données (dates et montants typés, filtre automatique, en-tête
/// figé), puis feuille « À propos » : filtres, date, avertissements.
pub fn ecrire_classeur(chemin: &Path, tableau: &Tableau, description: &str, jour: Date) -> Resultat<()> {
    use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, XlsxError};
    let construire = || -> Result<(), XlsxError> {
        let mut classeur = Workbook::new();
        let gras = Format::new().set_bold();
        let format_date = Format::new().set_num_format("dd/mm/yyyy");
        let format_nombre = Format::new().set_num_format("#,##0.00");
        {
            let feuille = classeur.add_worksheet();
            feuille.set_name(tableau.liste.libelle())?;
            for (c, entete) in tableau.entetes.iter().enumerate() {
                feuille.write_string_with_format(0, c as u16, entete, &gras)?;
            }
            for (r, ligne) in tableau.lignes.iter().enumerate() {
                let r = r as u32 + 1;
                for (c, cellule) in ligne.iter().enumerate() {
                    let c = c as u16;
                    match cellule {
                        Cellule::Vide => {}
                        Cellule::Texte(t) => {
                            feuille.write_string(r, c, t)?;
                        }
                        Cellule::Nombre(n) => {
                            feuille.write_number_with_format(r, c, *n, &format_nombre)?;
                        }
                        Cellule::Date(d) => match ExcelDateTime::from_ymd(d.annee(), d.mois(), d.jour()) {
                            Ok(date) => {
                                feuille.write_datetime_with_format(r, c, &date, &format_date)?;
                            }
                            Err(_) => {
                                feuille.write_string(r, c, d.to_string())?;
                            }
                        },
                    }
                }
            }
            if let Some(derniere) = tableau.entetes.len().checked_sub(1) {
                feuille.autofilter(0, 0, tableau.lignes.len() as u32, derniere as u16)?;
                feuille.set_freeze_panes(1, 0)?;
            }
            feuille.autofit();
        }
        let infos = classeur.add_worksheet();
        infos.set_name("À propos")?;
        let lignes = [
            ("Liste", tableau.liste.libelle().to_owned()),
            ("Filtres", description.to_owned()),
            ("Lignes", tableau.lignes.len().to_string()),
            ("Exporté le", jour.to_string()),
            ("Source", "Dokos, en lecture seule, par Dokos Assistant".to_owned()),
        ];
        for (r, (libelle, valeur)) in lignes.iter().enumerate() {
            infos.write_string_with_format(r as u32, 0, *libelle, &gras)?;
            infos.write_string(r as u32, 1, valeur)?;
        }
        for (i, avertissement) in tableau.avertissements.iter().enumerate() {
            let r = (lignes.len() + i) as u32;
            infos.write_string_with_format(r, 0, "Avertissement", &gras)?;
            infos.write_string(r, 1, avertissement)?;
        }
        infos.set_column_width(0, 16)?;
        infos.set_column_width(1, 90)?;
        classeur.save(chemin)
    };
    construire().map_err(|e| ErreurVue::fichier(format!("{} : {e}", chemin.display())))
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    fn dossier_test(nom: &str) -> PathBuf {
        let dossier = std::env::temp_dir().join(format!("dokos-listes-test-{}-{nom}", std::process::id()));
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
        assert!(resultat.avertissements[0].contains("colonnes absentes"));
        let octets = std::fs::read(&resultat.chemin).expect("fichier");
        assert!(octets.starts_with(b"PK"), "classeur xlsx (zip)");

        // Un second export ne remplace pas le premier.
        assert!(
            chemin_export(&dossier, TypeListe::Factures, jour())
                .expect("chemin")
                .ends_with("factures-2026-11-04-2.xlsx")
        );
        let _ = std::fs::remove_dir_all(dossier);
    }
}
