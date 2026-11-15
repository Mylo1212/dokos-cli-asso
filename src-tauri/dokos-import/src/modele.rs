//! Génération du classeur modèle d'un cas d'usage.
//!
//! Onglets : « Mode d'emploi », « Données » (en-têtes figés, ligne d'exemple,
//! commentaires, listes déroulantes), « Listes » (valeurs lues dans Dokos) et
//! `_dokos` (caché : cas, version, date, adresse de Dokos).

use rust_xlsxwriter::{
    Color, DataValidation, DocProperties, Format, FormatAlign, Formula, Note, Workbook, Worksheet, XlsxError,
};

use crate::cas::{Cas, Colonne, Genre};
use crate::lecture::{ONGLET_DONNEES, ONGLET_LISTES, ONGLET_MARQUE, ONGLET_MODE_EMPLOI};
use crate::referentiel::Referentiel;
use crate::{ErreurImport, LIGNES_MAX, VERSION_MODELE};

/// Lignes couvertes par les listes déroulantes et les formats.
const LIGNES_PREPAREES: u32 = LIGNES_MAX as u32 + 1;

/// Contexte inscrit dans le modèle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContexteModele {
    pub url_dokos: String,
    pub date: String,
    pub societe: Option<String>,
}

/// Nom de fichier conseillé pour le modèle d'un cas.
pub fn nom_fichier_modele(cas: Cas) -> String {
    format!("modele-{}.xlsx", cas.nom())
}

/// Classeur modèle, en mémoire.
pub fn generer_modele(cas: Cas, referentiel: &Referentiel, contexte: &ContexteModele) -> Result<Vec<u8>, ErreurImport> {
    construire(cas, referentiel, contexte).map_err(|e| ErreurImport::Fichier(format!("génération du modèle : {e}")))
}

fn construire(cas: Cas, referentiel: &Referentiel, contexte: &ContexteModele) -> Result<Vec<u8>, XlsxError> {
    let colonnes = cas.colonnes();
    let exemple = cas.exemple();
    let mut classeur = Workbook::new();
    classeur.set_properties(
        &DocProperties::new()
            .set_title(format!("Modèle d'import — {}", cas.libelle()))
            .set_author("Dokos Asso")
            .set_comment(format!(
                "Modèle {} version {VERSION_MODELE}, généré le {} depuis {}",
                cas.nom(),
                contexte.date,
                contexte.url_dokos
            )),
    );

    mode_emploi(classeur.add_worksheet(), cas, &colonnes, contexte)?;
    let positions = listes(&mut classeur, referentiel)?;
    // L'onglet Données est ajouté après Listes pour que les validations y fassent référence,
    // puis placé en premier onglet actif.
    donnees(classeur.add_worksheet(), cas, &colonnes, &exemple, &positions)?;
    marque(classeur.add_worksheet(), cas, contexte)?;

    // Ordre d'affichage : Mode d'emploi, Données, Listes.
    let feuilles = classeur.worksheets_mut();
    feuilles.swap(1, 2);
    if let Some(donnees) = feuilles.get_mut(1) {
        donnees.set_active(true);
    }
    classeur.save_to_buffer()
}

fn mode_emploi(
    feuille: &mut Worksheet,
    cas: Cas,
    colonnes: &[Colonne],
    contexte: &ContexteModele,
) -> Result<(), XlsxError> {
    feuille.set_name(ONGLET_MODE_EMPLOI)?;
    feuille.set_column_width(0, 110)?;
    let titre = Format::new().set_bold().set_font_size(14);
    let texte = Format::new().set_text_wrap().set_align(FormatAlign::Top);
    let gras = Format::new().set_bold();
    let mut ligne = 0;
    feuille.write_string_with_format(ligne, 0, format!("Modèle d'import — {}", cas.libelle()), &titre)?;
    ligne += 1;
    feuille.write_string_with_format(
        ligne,
        0,
        format!(
            "Généré le {} depuis {} (version du modèle : {VERSION_MODELE}){}",
            contexte.date,
            contexte.url_dokos,
            contexte.societe.as_deref().map(|s| format!(", société {s}")).unwrap_or_default()
        ),
        &texte,
    )?;
    ligne += 1;
    ligne += 1;
    let paragraphes = [
        format!("Résultat : {}.", cas.resultat()),
        cas.consignes().to_owned(),
        "Remplissez l'onglet « Données » à partir de la ligne 3 (la ligne 2 est un exemple, ignoré à l'import). Ne modifiez pas la ligne des en-têtes.".to_owned(),
        "Les colonnes marquées * sont obligatoires. Les valeurs à choisir dans une liste (clients, articles…) sont celles de l'onglet « Listes », telles qu'elles étaient dans Dokos à la génération du modèle : régénérez le modèle si Dokos a changé.".to_owned(),
        "Un client ou un article se désigne par son identifiant ou par son nom exact (accents et majuscules sans importance). Un nom approximatif est signalé comme ambigu : l'appli vous demande alors de choisir, sans rien créer.".to_owned(),
        "Dates : JJ/MM/AAAA ou AAAA-MM-JJ. Nombres : virgule ou point. Oui/Non : « Oui » ou « Non ».".to_owned(),
        "À l'import, l'appli analyse toutes les lignes, montre ce qui bloque, cherche les doublons, puis crée tout ou rien. Un fichier résultat reprend chaque ligne avec son statut.".to_owned(),
    ];
    for paragraphe in paragraphes {
        feuille.write_string_with_format(ligne, 0, paragraphe, &texte)?;
        ligne += 1;
    }
    ligne += 1;
    feuille.write_string_with_format(ligne, 0, "Colonnes", &gras)?;
    ligne += 1;
    for colonne in colonnes {
        let detail = format!(
            "{}{} — {}{}",
            colonne.nom,
            if colonne.obligatoire { " (obligatoire)" } else { "" },
            match colonne.genre {
                Genre::Texte => "texte",
                Genre::Nombre => "nombre",
                Genre::Date => "date",
                Genre::OuiNon => "Oui / Non",
            },
            if colonne.aide.is_empty() { String::new() } else { format!(". {}", colonne.aide) }
        );
        feuille.write_string_with_format(ligne, 0, detail, &texte)?;
        ligne += 1;
    }
    Ok(())
}

/// Position (colonne de la valeur, nombre d'éléments) de chaque liste dans l'onglet Listes.
fn listes(classeur: &mut Workbook, referentiel: &Referentiel) -> Result<Vec<(String, u16, u32)>, XlsxError> {
    let feuille = classeur.add_worksheet();
    feuille.set_name(ONGLET_LISTES)?;
    let entete = Format::new().set_bold().set_background_color(Color::RGB(0xE3EFE9));
    let mut positions = Vec::new();
    let mut colonne: u16 = 0;
    for nom in referentiel.noms() {
        let elements = referentiel.liste(nom);
        feuille.write_string_with_format(0, colonne, nom, &entete)?;
        feuille.write_string_with_format(0, colonne + 1, format!("{nom} — détail"), &entete)?;
        for (i, (valeur, libelle)) in elements.iter().enumerate() {
            let ligne = i as u32 + 1;
            feuille.write_string(ligne, colonne, valeur)?;
            if libelle != valeur {
                feuille.write_string(ligne, colonne + 1, libelle)?;
            }
        }
        feuille.set_column_width(colonne, 28)?;
        feuille.set_column_width(colonne + 1, 44)?;
        positions.push((nom.to_owned(), colonne, elements.len() as u32));
        colonne += 2;
    }
    if positions.is_empty() {
        feuille.write_string(0, 0, "Aucune liste pour ce cas d'usage.")?;
    }
    feuille.protect();
    Ok(positions)
}

fn donnees(
    feuille: &mut Worksheet,
    cas: Cas,
    colonnes: &[Colonne],
    exemple: &[String],
    positions: &[(String, u16, u32)],
) -> Result<(), XlsxError> {
    feuille.set_name(ONGLET_DONNEES)?;
    feuille.set_tab_color(Color::RGB(0x246354));
    feuille.set_freeze_panes(1, 0)?;
    let entete = Format::new().set_bold().set_background_color(Color::RGB(0xE3EFE9)).set_locked();
    let exemple_format = Format::new().set_italic().set_font_color(Color::RGB(0x7A7A7A));
    let date = Format::new().set_num_format("dd/mm/yyyy");
    for (i, colonne) in colonnes.iter().enumerate() {
        let c = i as u16;
        feuille.write_string_with_format(0, c, colonne.entete(), &entete)?;
        let largeur = match colonne.genre {
            Genre::Date => 16,
            Genre::Nombre | Genre::OuiNon => 11,
            Genre::Texte => (colonne.nom.chars().count() as f64 + 10.0).clamp(14.0, 34.0) as u16,
        };
        // Nombres : format « Standard » d'Excel (`1`, `12,5`), pas de séparateur décimal en trop.
        if colonne.genre == Genre::Date {
            feuille.set_column_format(c, &date)?;
        }
        feuille.set_column_width(c, f64::from(largeur))?;
        if !colonne.aide.is_empty() {
            let note = Note::new(colonne.aide.as_str()).set_author("Dokos Asso").add_author_prefix(false);
            feuille.insert_note(0, c, &note)?;
        }
        if let Some(valeur) = exemple.get(i) {
            feuille.write_string_with_format(1, c, valeur, &exemple_format)?;
        }
        let validation = match colonne.genre {
            Genre::OuiNon => Some(DataValidation::new().allow_list_strings(&["Oui", "Non"])?),
            _ => match colonne.liste.and_then(|nom| positions.iter().find(|(n, _, _)| n == nom)) {
                Some((_, col_liste, n)) if *n > 0 => {
                    let lettre = lettre_colonne(*col_liste);
                    let formule = Formula::new(format!("={ONGLET_LISTES}!${lettre}$2:${lettre}${}", n + 1));
                    Some(
                        DataValidation::new()
                            .allow_list_formula(formule)
                            .ignore_blank(true)
                            .set_error_title("Valeur hors liste")?
                            .set_error_message(format!(
                                "Choisissez une valeur de l'onglet Listes ({}).",
                                colonne.nom
                            ))?,
                    )
                }
                _ => None,
            },
        };
        if let Some(validation) = validation {
            // Dès la ligne d'exemple (ligne 2 du classeur), pour qu'elle montre aussi la liste.
            feuille.add_data_validation(1, c, LIGNES_PREPAREES, c, &validation)?;
        }
    }
    let _ = cas;
    Ok(())
}

fn marque(feuille: &mut Worksheet, cas: Cas, contexte: &ContexteModele) -> Result<(), XlsxError> {
    feuille.set_name(ONGLET_MARQUE)?;
    let lignes = [
        ("signature", "dokos-asso".to_owned()),
        ("cas", cas.nom().to_owned()),
        ("version", VERSION_MODELE.to_string()),
        ("genere", contexte.date.clone()),
        ("dokos", contexte.url_dokos.clone()),
    ];
    for (i, (cle, valeur)) in lignes.iter().enumerate() {
        feuille.write_string(i as u32, 0, *cle)?;
        feuille.write_string(i as u32, 1, valeur)?;
    }
    feuille.set_hidden(true);
    Ok(())
}

/// `0 → A`, `25 → Z`, `26 → AA`.
pub fn lettre_colonne(indice: u16) -> String {
    let mut n = u32::from(indice) + 1;
    let mut lettres = Vec::new();
    while n > 0 {
        n -= 1;
        lettres.push(char::from(b'A' + (n % 26) as u8));
        n /= 26;
    }
    lettres.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lecture::lire_octets;
    use crate::referentiel::{LISTE_ARTICLES, LISTE_CLIENTS};

    fn contexte() -> ContexteModele {
        ContexteModele {
            url_dokos: "https://dokos.exemple.fr".into(),
            date: "2026-11-03".into(),
            societe: Some("Les Amanins".into()),
        }
    }

    #[test]
    fn lettres() {
        assert_eq!(lettre_colonne(0), "A");
        assert_eq!(lettre_colonne(25), "Z");
        assert_eq!(lettre_colonne(26), "AA");
        assert_eq!(lettre_colonne(27), "AB");
    }

    #[test]
    fn modele_relu_par_la_lecture() {
        let mut referentiel = Referentiel::default();
        referentiel.ajouter(LISTE_CLIENTS, vec![("CUST-1".into(), "Mairie de Crest".into())]);
        referentiel.ajouter(LISTE_ARTICLES, vec![("REPAS".into(), "Repas — Unité".into())]);
        for cas in Cas::TOUS {
            let octets = generer_modele(cas, &referentiel, &contexte()).expect("modèle");
            assert!(octets.len() > 2_000, "{cas}");
            let tableau = lire_octets(&octets, "xlsx").expect("relecture");
            let marque = tableau.marque.clone().expect("marque");
            assert_eq!(marque.cas, cas);
            assert_eq!(marque.version, VERSION_MODELE);
            assert_eq!(marque.dokos, "https://dokos.exemple.fr");
            let colonnes = cas.colonnes();
            assert_eq!(tableau.entetes.len(), colonnes.len(), "{cas}");
            assert_eq!(tableau.entetes[0], colonnes[0].entete());
            // La ligne d'exemple est lue puis retirée.
            assert_eq!(tableau.lignes.len(), 1, "{cas}");
            let sans = tableau.sans_exemple(&cas.exemple());
            assert!(sans.lignes.is_empty(), "{cas}");
        }
    }

    #[test]
    fn nom_du_fichier() {
        assert_eq!(nom_fichier_modele(Cas::Clients), "modele-clients.xlsx");
        assert_eq!(nom_fichier_modele(Cas::Paiements), "modele-paiements.xlsx");
    }
}
