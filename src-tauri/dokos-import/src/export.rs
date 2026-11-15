//! Export : écriture d'un tableau lu dans Dokos (`dokos_core::export`) en
//! classeur Excel. Lecture seule côté Dokos ; le fichier va dans
//! `Documents\Dokos\exports`.

use std::path::{Path, PathBuf};

use dokos_core::date::Date;
use dokos_core::export::{Cellule, Tableau, TypeListe};
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
