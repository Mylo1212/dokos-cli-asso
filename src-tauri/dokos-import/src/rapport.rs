//! Fichier résultat : le tableau importé, avec le statut de chaque ligne, le
//! document créé et le message.

use rust_xlsxwriter::{Color, Format, Workbook};

use crate::analyse::{Analyse, ResultatDocument, StatutCreation};
use crate::{ErreurImport, lecture::ONGLET_DONNEES};

/// Ligne du résultat : statut, document, message.
pub fn lignes_resultat(analyse: &Analyse, resultats: Option<&[ResultatDocument]>) -> Vec<(String, String, String)> {
    analyse
        .lignes
        .iter()
        .map(|ligne| {
            let resultat = resultats.and_then(|r| ligne.document.and_then(|d| r.get(d)));
            match (resultat, &ligne.statut) {
                (Some(r), _) if !matches!(r.statut, StatutCreation::Ignore) || ligne.statut.est_pret() => (
                    match r.statut {
                        StatutCreation::Cree => "créé",
                        StatutCreation::Existant => "déjà présent",
                        StatutCreation::Ignore => "ignoré",
                        StatutCreation::Erreur => "erreur",
                    }
                    .to_owned(),
                    r.id.clone().unwrap_or_default(),
                    r.message.clone(),
                ),
                (_, statut) => {
                    let doublon = ligne
                        .document
                        .and_then(|d| analyse.documents.get(d))
                        .and_then(|d| d.doublon.as_ref())
                        .map(|e| format!("identique à {} ({})", e.id, e.description));
                    (
                        if statut.est_pret() && doublon.is_some() {
                            "doublon".to_owned()
                        } else {
                            statut.libelle().to_owned()
                        },
                        String::new(),
                        if statut.est_pret() { doublon.unwrap_or_default() } else { statut.message() },
                    )
                }
            }
        })
        .collect()
}

/// Classeur résultat, en mémoire.
pub fn ecrire_rapport(analyse: &Analyse, resultats: Option<&[ResultatDocument]>) -> Result<Vec<u8>, ErreurImport> {
    let construire = || -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
        let mut classeur = Workbook::new();
        let feuille = classeur.add_worksheet();
        feuille.set_name("Résultat")?;
        feuille.set_freeze_panes(1, 0)?;
        let entete = Format::new().set_bold().set_background_color(Color::RGB(0xE3EFE9));
        let ok = Format::new().set_font_color(Color::RGB(0x1F7A4D));
        let ko = Format::new().set_font_color(Color::RGB(0xB3261E));
        let mut col = 0u16;
        for e in &analyse.entetes {
            feuille.write_string_with_format(0, col, e, &entete)?;
            col += 1;
        }
        for titre in ["Statut", "Document", "Message"] {
            feuille.write_string_with_format(0, col, titre, &entete)?;
            feuille.set_column_width(col, if titre == "Message" { 60 } else { 18 })?;
            col += 1;
        }
        for (i, (ligne, (statut, document, message))) in
            analyse.lignes.iter().zip(lignes_resultat(analyse, resultats)).enumerate()
        {
            let r = i as u32 + 1;
            for (c, valeur) in ligne.valeurs.iter().enumerate() {
                feuille.write_string(r, c as u16, valeur)?;
            }
            let c = analyse.entetes.len() as u16;
            let format = if matches!(statut.as_str(), "créé" | "prêt" | "déjà présent") { &ok } else { &ko };
            feuille.write_string_with_format(r, c, &statut, format)?;
            feuille.write_string(r, c + 1, &document)?;
            feuille.write_string(r, c + 2, &message)?;
        }
        let _ = ONGLET_DONNEES;
        classeur.save_to_buffer()
    };
    construire().map_err(|e| ErreurImport::Fichier(format!("rapport : {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyse::{DocumentPrepare, Existant, LigneAnalysee, Statut};
    use crate::cas::Cas;
    use crate::lecture::lire_octets;

    #[test]
    fn rapport_relu() {
        let analyse = Analyse {
            cas: Cas::Factures,
            entetes: vec!["Client".into(), "Quantité".into()],
            avertissements: vec![],
            lignes: vec![
                LigneAnalysee {
                    numero: 2,
                    valeurs: vec!["Mairie".into(), "25".into()],
                    statut: Statut::Pret,
                    document: Some(0),
                },
                LigneAnalysee {
                    numero: 3,
                    valeurs: vec!["?".into(), "1".into()],
                    statut: Statut::Introuvable { champ: "client".into(), texte: "?".into() },
                    document: Some(1),
                },
                LigneAnalysee {
                    numero: 4,
                    valeurs: vec!["CCAS".into(), "2".into()],
                    statut: Statut::Pret,
                    document: Some(2),
                },
            ],
            documents: vec![
                DocumentPrepare {
                    cle: "a".into(),
                    libelle: String::new(),
                    lignes: vec![0],
                    prepare: None,
                    doublon: None,
                },
                DocumentPrepare {
                    cle: "b".into(),
                    libelle: String::new(),
                    lignes: vec![1],
                    prepare: None,
                    doublon: None,
                },
                DocumentPrepare {
                    cle: "c".into(),
                    libelle: String::new(),
                    lignes: vec![2],
                    prepare: None,
                    doublon: Some(Existant {
                        doctype: "Sales Invoice".into(),
                        id: "F-0".into(),
                        description: "brouillon".into(),
                    }),
                },
            ],
        };
        let lignes = lignes_resultat(&analyse, None);
        assert_eq!(lignes[0].0, "prêt");
        assert_eq!(
            lignes[1],
            ("introuvable".to_owned(), String::new(), "client « ? » introuvable dans Dokos".to_owned())
        );
        assert_eq!(lignes[2].0, "doublon");
        let resultats = vec![
            ResultatDocument {
                cle: "a".into(),
                libelle: String::new(),
                doctype: "Sales Invoice".into(),
                id: Some("F-1".into()),
                statut: StatutCreation::Cree,
                message: "ok".into(),
                lignes: vec![0],
            },
            ResultatDocument {
                cle: "b".into(),
                libelle: String::new(),
                doctype: String::new(),
                id: None,
                statut: StatutCreation::Ignore,
                message: "x".into(),
                lignes: vec![1],
            },
            ResultatDocument {
                cle: "c".into(),
                libelle: String::new(),
                doctype: "Sales Invoice".into(),
                id: Some("F-0".into()),
                statut: StatutCreation::Existant,
                message: "déjà".into(),
                lignes: vec![2],
            },
        ];
        let lignes = lignes_resultat(&analyse, Some(&resultats));
        assert_eq!(lignes[0], ("créé".to_owned(), "F-1".to_owned(), "ok".to_owned()));
        assert_eq!(lignes[1].0, "introuvable");
        assert_eq!(lignes[2].0, "déjà présent");
        let octets = ecrire_rapport(&analyse, Some(&resultats)).expect("rapport");
        let relu = lire_octets(&octets, "xlsx").expect("relecture");
        assert_eq!(relu.entetes, ["Client", "Quantité", "Statut", "Document", "Message"]);
        assert_eq!(relu.lignes[0].1[2], "créé");
        assert_eq!(relu.lignes[0].1[3], "F-1");
    }
}
