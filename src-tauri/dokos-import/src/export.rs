//! Export : écriture d'un tableau lu dans Dokos (`dokos_core::export`) en
//! classeur Excel. Lecture seule côté Dokos ; le fichier va dans
//! `Documents\Dokos\exports`.

use std::path::{Path, PathBuf};

use dokos_core::date::Date;
use dokos_core::export::{Cellule, Tableau, TypeListe};
use dokos_core::grand_livre::{CODE_JOURNAL_FICHIER, GrandLivre};
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, XlsxError};

use crate::ErreurImport;

/// Sous-dossier de `Documents\Dokos` qui reçoit les exports.
pub const DOSSIER_EXPORTS: &str = "exports";

/// Nouveau fichier à chaque export : `clients-2026-11-04.xlsx`, puis `-2`, `-3`…
pub fn chemin_export(dossier: &Path, liste: TypeListe, jour: Date) -> Result<PathBuf, ErreurImport> {
    let dossier = dossier.join(DOSSIER_EXPORTS);
    std::fs::create_dir_all(&dossier).map_err(|e| ErreurImport::Fichier(format!("{} : {e}", dossier.display())))?;
    let base = format!("{}-{jour}", liste.nom());
    let mut chemin = dossier.join(format!("{base}.xlsx"));
    let mut n = 2;
    while chemin.exists() {
        if n > 999 {
            return Err(ErreurImport::Fichier(format!("trop d'exports {base} dans {}", dossier.display())));
        }
        chemin = dossier.join(format!("{base}-{n}.xlsx"));
        n += 1;
    }
    Ok(chemin)
}

/// Feuille de données (dates et montants typés, filtre automatique, en-tête
/// figé), puis feuille « À propos » : filtres, date, avertissements.
pub fn ecrire_classeur(chemin: &Path, tableau: &Tableau, description: &str, jour: Date) -> Result<(), ErreurImport> {
    construire(chemin, tableau, description, jour)
        .map_err(|e| ErreurImport::Fichier(format!("{} : {e}", chemin.display())))
}

fn construire(chemin: &Path, tableau: &Tableau, description: &str, jour: Date) -> Result<(), XlsxError> {
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
        ("Source", "Dokos, en lecture seule, par Dokos Asso".to_owned()),
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
}

/// Nom de fichier sûr : lettres, chiffres, `-` et `_`, le reste devient `-`.
fn nom_sur(texte: &str) -> String {
    let nom: String = texte.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '-' }).collect();
    nom.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// Nouveau fichier à chaque export : `grand-livre-VEN-SLA-2026-10-01-2026-10-31.xlsx`, puis `-2`, `-3`…
pub fn chemin_grand_livre(dossier: &Path, journal: &str, du: Date, au: Date) -> Result<PathBuf, ErreurImport> {
    let dossier = dossier.join(DOSSIER_EXPORTS);
    std::fs::create_dir_all(&dossier).map_err(|e| ErreurImport::Fichier(format!("{} : {e}", dossier.display())))?;
    let journal = nom_sur(journal);
    let journal = if journal.is_empty() { CODE_JOURNAL_FICHIER.to_owned() } else { journal };
    let base = format!("grand-livre-{journal}-{du}-{au}");
    let mut chemin = dossier.join(format!("{base}.xlsx"));
    let mut n = 2;
    while chemin.exists() {
        if n > 999 {
            return Err(ErreurImport::Fichier(format!("trop d'exports {base} dans {}", dossier.display())));
        }
        chemin = dossier.join(format!("{base}-{n}.xlsx"));
        n += 1;
    }
    Ok(chemin)
}

/// En-têtes du fichier du grand livre, dans l'ordre.
pub const ENTETES_GRAND_LIVRE: [&str; 8] =
    ["DATE", "JOURNAL", "GENERAL", "AUXILIAIRE", "DEBIT", "CREDIT", "LIBELLE", "REFINTERNE"];

/// Écrit le grand livre : une seule feuille, huit colonnes. Dates `jj/mm/aaaa`,
/// comptes et montants numériques ; la colonne JOURNAL vaut [`CODE_JOURNAL_FICHIER`].
pub fn ecrire_grand_livre(chemin: &Path, grand_livre: &GrandLivre) -> Result<(), ErreurImport> {
    construire_grand_livre(chemin, grand_livre)
        .map_err(|e| ErreurImport::Fichier(format!("{} : {e}", chemin.display())))
}

fn construire_grand_livre(chemin: &Path, grand_livre: &GrandLivre) -> Result<(), XlsxError> {
    let mut classeur = Workbook::new();
    let format_date = Format::new().set_num_format("dd/mm/yyyy");
    let format_compte = Format::new().set_num_format("0");
    let format_montant = Format::new().set_num_format("0.00");
    let feuille = classeur.add_worksheet();
    feuille.set_name("Grand livre")?;
    for (c, entete) in ENTETES_GRAND_LIVRE.iter().enumerate() {
        feuille.write_string(0, c as u16, *entete)?;
    }
    for (i, ligne) in grand_livre.lignes.iter().enumerate() {
        let r = i as u32 + 1;
        match ExcelDateTime::from_ymd(ligne.date.annee(), ligne.date.mois(), ligne.date.jour()) {
            Ok(date) => {
                feuille.write_datetime_with_format(r, 0, &date, &format_date)?;
            }
            Err(_) => {
                feuille.write_string(r, 0, ligne.date.to_string())?;
            }
        }
        feuille.write_string(r, 1, CODE_JOURNAL_FICHIER)?;
        feuille.write_number_with_format(r, 2, ligne.general as f64, &format_compte)?;
        if !ligne.auxiliaire.is_empty() {
            feuille.write_string(r, 3, ligne.auxiliaire)?;
        }
        feuille.write_number_with_format(r, 4, ligne.debit, &format_montant)?;
        feuille.write_number_with_format(r, 5, ligne.credit, &format_montant)?;
        // Un texte qui commence par « = » ne doit jamais être lu comme une formule.
        feuille.write_string(r, 6, &ligne.libelle)?;
        feuille.write_string(r, 7, &ligne.reference)?;
    }
    feuille.set_column_width(0, 12)?;
    feuille.set_column_width(6, 34)?;
    feuille.set_column_width(7, 22)?;
    classeur.save(chemin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jour() -> Date {
        "2026-11-04".parse().expect("date")
    }

    fn tableau() -> Tableau {
        Tableau {
            liste: TypeListe::Factures,
            entetes: vec!["Numéro".into(), "Date".into(), "Total HT".into()],
            lignes: vec![
                vec![Cellule::Texte("FAC-1".into()), Cellule::Date(jour()), Cellule::Nombre(120.5)],
                vec![Cellule::Texte("=1+1".into()), Cellule::Vide, Cellule::Nombre(0.0)],
            ],
            tronque: false,
            avertissements: vec!["colonnes absentes de votre Dokos, non exportées : Reste dû".into()],
        }
    }

    #[test]
    fn grand_livre_ecrit_et_relu() {
        use dokos_core::grand_livre::LigneGrandLivre;

        let dossier = std::env::temp_dir().join(format!("dokos-gl-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).expect("dossier");
        let gl = GrandLivre {
            lignes: vec![LigneGrandLivre {
                date: jour(),
                general: 411,
                auxiliaire: "ERP",
                debit: 120.5,
                credit: 0.0,
                libelle: "=1+1".into(),
                reference: "ACC-SINV-1".into(),
            }],
            ecartees: Vec::new(),
            total_debit: 120.5,
            total_credit: 0.0,
        };
        let au: Date = "2026-11-30".parse().expect("date");
        let chemin = chemin_grand_livre(&dossier, "VEN - SLA", jour(), au).expect("chemin");
        assert!(chemin.ends_with("grand-livre-VEN-SLA-2026-11-04-2026-11-30.xlsx"), "{}", chemin.display());
        ecrire_grand_livre(&chemin, &gl).expect("classeur");

        // Relu par la lecture des imports : en-têtes, puis une ligne de données.
        let tableau = crate::lire_fichier(&chemin).expect("relecture");
        assert_eq!(tableau.entetes, ENTETES_GRAND_LIVRE);
        assert_eq!(tableau.lignes.len(), 1);
        let valeurs = &tableau.lignes[0].1;
        assert_eq!(&valeurs[1..], ["VEN", "411", "ERP", "120.5", "0", "=1+1", "ACC-SINV-1"]);

        // Un second export de la même période ne remplace pas le premier.
        let suivant = chemin_grand_livre(&dossier, "VEN - SLA", jour(), au).expect("chemin");
        assert!(suivant.ends_with("grand-livre-VEN-SLA-2026-11-04-2026-11-30-2.xlsx"));
        let _ = std::fs::remove_dir_all(dossier);
    }

    #[test]
    fn noms_de_fichier_surs() {
        assert_eq!(nom_sur("VEN - SLA"), "VEN-SLA");
        assert_eq!(nom_sur("../Ventes/été"), "Ventes-été");
        assert_eq!(nom_sur("***"), "");
    }

    #[test]
    fn classeur_ecrit_puis_nouveau_nom() {
        let dossier = std::env::temp_dir().join(format!("dokos-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).expect("dossier");

        let chemin = chemin_export(&dossier, TypeListe::Factures, jour()).expect("chemin");
        assert!(
            chemin.ends_with("exports/factures-2026-11-04.xlsx")
                || chemin.ends_with("exports\\factures-2026-11-04.xlsx")
        );
        ecrire_classeur(&chemin, &tableau(), "Factures : payées", jour()).expect("classeur");
        let octets = std::fs::read(&chemin).expect("fichier");
        assert!(octets.starts_with(b"PK"), "classeur xlsx (zip)");

        // Un second export ne remplace pas le premier.
        let suivant = chemin_export(&dossier, TypeListe::Factures, jour()).expect("chemin");
        assert!(suivant.ends_with("factures-2026-11-04-2.xlsx"), "{}", suivant.display());
        let _ = std::fs::remove_dir_all(dossier);
    }
}
