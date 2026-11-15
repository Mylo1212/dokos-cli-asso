//! Lecture d'un fichier `.xlsx`, `.xls`, `.ods` ou `.csv` en tableau de textes.

use std::io::Cursor;
use std::path::Path;

use calamine::{Data, Range, Reader};
use dokos_core::date::Date;
use dokos_core::recherche::normaliser;

use crate::cas::{Cas, Colonne};
use crate::{ErreurImport, LIGNES_MAX, TAILLE_MAX, VERSION_MODELE};

/// Onglet caché qui identifie le modèle.
pub const ONGLET_MARQUE: &str = "_dokos";
pub const ONGLET_DONNEES: &str = "Données";
pub const ONGLET_LISTES: &str = "Listes";
pub const ONGLET_MODE_EMPLOI: &str = "Mode d'emploi";
const SIGNATURE: &str = "dokos-assistant";

/// Identité du modèle d'où vient le fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marque {
    pub cas: Cas,
    pub version: u32,
    pub dokos: String,
    /// Modèle Dokos copié (cas « à partir d'un modèle ») et sa date de modification.
    pub modele: Option<String>,
    pub modele_modifie: Option<String>,
}

/// Tableau lu : en-têtes et lignes de texte, avec le numéro de ligne du fichier.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tableau {
    pub marque: Option<Marque>,
    pub entetes: Vec<String>,
    pub lignes: Vec<(u32, Vec<String>)>,
}

impl Tableau {
    /// Vérifie que le fichier vient du modèle du cas, à une version lisible.
    pub fn verifier_marque(&self, cas: Cas) -> Result<(), ErreurImport> {
        match &self.marque {
            None => Err(ErreurImport::Modele(format!(
                "ce fichier ne vient pas d'un modèle de l'appli : téléchargez le modèle « {} » et remplissez-le",
                cas.libelle()
            ))),
            Some(m) if m.cas != cas => Err(ErreurImport::Modele(format!(
                "ce fichier est un modèle « {} », pas « {} »",
                m.cas.libelle(),
                cas.libelle()
            ))),
            Some(m) if m.version > VERSION_MODELE => Err(ErreurImport::Modele(format!(
                "modèle de version {} plus récent que l'appli (version {VERSION_MODELE}) : mettez l'appli à jour",
                m.version
            ))),
            Some(_) => Ok(()),
        }
    }

    /// Retire la ligne d'exemple du modèle et les lignes vides.
    pub fn sans_exemple(mut self, exemple: &[String]) -> Self {
        self.lignes.retain(|(_, valeurs)| {
            let vide = valeurs.iter().all(|v| v.trim().is_empty());
            let est_exemple = !exemple.is_empty()
                && valeurs.len() == exemple.len()
                && valeurs.iter().zip(exemple).all(|(v, e)| normaliser(v) == normaliser(e));
            !vide && !est_exemple
        });
        self
    }
}

/// Association des colonnes du cas aux colonnes du fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correspondance {
    /// Pour chaque colonne du cas, l'indice de la colonne du fichier, s'il y en a une.
    pub indices: Vec<Option<usize>>,
    pub avertissements: Vec<String>,
}

impl Correspondance {
    pub fn valeur<'a>(&self, ligne: &'a [String], colonne: usize) -> &'a str {
        self.indices.get(colonne).copied().flatten().and_then(|i| ligne.get(i)).map(|v| v.trim()).unwrap_or_default()
    }
}

/// Retrouve chaque colonne attendue par son nom (casse, accents, `*` ignorés).
pub fn associer_colonnes(entetes: &[String], colonnes: &[Colonne]) -> Result<Correspondance, ErreurImport> {
    let cle = |texte: &str| normaliser(&texte.replace('*', ""));
    let entetes_normalisees: Vec<String> = entetes.iter().map(|e| cle(e)).collect();
    let mut indices = Vec::with_capacity(colonnes.len());
    let mut manquantes = Vec::new();
    let mut utilisees = vec![false; entetes.len()];
    for colonne in colonnes {
        let voulu = cle(&colonne.nom);
        let indice = entetes_normalisees.iter().position(|e| *e == voulu);
        if let Some(i) = indice {
            utilisees[i] = true;
        } else if colonne.obligatoire {
            manquantes.push(colonne.nom.clone());
        }
        indices.push(indice);
    }
    if !manquantes.is_empty() {
        return Err(ErreurImport::Colonnes(format!("colonnes obligatoires absentes : {}", manquantes.join(", "))));
    }
    let avertissements = entetes
        .iter()
        .zip(&utilisees)
        .filter(|(e, utilisee)| !**utilisee && !e.trim().is_empty())
        .map(|(e, _)| format!("colonne « {} » ignorée : inconnue du modèle", e.trim()))
        .collect();
    Ok(Correspondance { indices, avertissements })
}

/// Lit le fichier. Les classeurs à macros (`.xlsm`) sont refusés.
pub fn lire_fichier(chemin: &Path) -> Result<Tableau, ErreurImport> {
    let extension = chemin.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
    if extension == "xlsm" || extension == "xlsb" {
        return Err(ErreurImport::Fichier("classeur à macros refusé : enregistrez-le en .xlsx".to_owned()));
    }
    let taille =
        std::fs::metadata(chemin).map_err(|e| ErreurImport::Fichier(format!("{} : {e}", chemin.display())))?.len();
    if taille > TAILLE_MAX {
        return Err(ErreurImport::Fichier(format!(
            "fichier trop gros ({} Mo, maximum {} Mo)",
            taille >> 20,
            TAILLE_MAX >> 20
        )));
    }
    let octets = std::fs::read(chemin).map_err(|e| ErreurImport::Fichier(format!("{} : {e}", chemin.display())))?;
    lire_octets(&octets, &extension)
}

/// Lit un fichier déjà en mémoire, d'après son extension (`xlsx`, `xls`, `ods`, `csv`).
pub fn lire_octets(octets: &[u8], extension: &str) -> Result<Tableau, ErreurImport> {
    if octets.len() as u64 > TAILLE_MAX {
        return Err(ErreurImport::Fichier("fichier trop gros".to_owned()));
    }
    match extension.to_lowercase().as_str() {
        "csv" | "txt" | "tsv" => lire_csv(&String::from_utf8_lossy(octets)),
        _ => lire_classeur(octets),
    }
}

fn lire_classeur(octets: &[u8]) -> Result<Tableau, ErreurImport> {
    let mut classeur = calamine::open_workbook_auto_from_rs(Cursor::new(octets.to_vec()))
        .map_err(|e| ErreurImport::Fichier(format!("classeur illisible : {e}")))?;
    let noms = classeur.sheet_names();
    let marque = if noms.iter().any(|n| n == ONGLET_MARQUE) {
        classeur.worksheet_range(ONGLET_MARQUE).ok().and_then(|r| lire_marque(&r))
    } else {
        None
    };
    let onglet = if noms.iter().any(|n| n == ONGLET_DONNEES) {
        ONGLET_DONNEES.to_owned()
    } else {
        noms.iter()
            .find(|n| ![ONGLET_MARQUE, ONGLET_LISTES, ONGLET_MODE_EMPLOI].contains(&n.as_str()))
            .cloned()
            .ok_or_else(|| ErreurImport::Fichier("aucun onglet de données".to_owned()))?
    };
    let plage =
        classeur.worksheet_range(&onglet).map_err(|e| ErreurImport::Fichier(format!("onglet {onglet} : {e}")))?;
    let mut tableau = tableau_depuis_plage(&plage)?;
    tableau.marque = marque;
    Ok(tableau)
}

fn lire_marque(plage: &Range<Data>) -> Option<Marque> {
    let mut valeurs = std::collections::BTreeMap::new();
    for ligne in plage.rows() {
        if let (Some(cle), Some(valeur)) = (ligne.first(), ligne.get(1)) {
            valeurs.insert(texte_cellule(cle), texte_cellule(valeur));
        }
    }
    if valeurs.get("signature").map(String::as_str) != Some(SIGNATURE) {
        return None;
    }
    let cas = Cas::depuis_nom(valeurs.get("cas")?)?;
    let version = valeurs.get("version")?.parse().ok()?;
    let non_vide = |cle: &str| valeurs.get(cle).filter(|v| !v.is_empty()).cloned();
    Some(Marque {
        cas,
        version,
        dokos: valeurs.get("dokos").cloned().unwrap_or_default(),
        modele: non_vide("modele"),
        modele_modifie: non_vide("modele_modifie"),
    })
}

fn tableau_depuis_plage(plage: &Range<Data>) -> Result<Tableau, ErreurImport> {
    let decalage = plage.start().map(|(ligne, _)| ligne).unwrap_or(0);
    let mut lignes_brutes = plage.rows().enumerate().map(|(i, l)| (decalage + i as u32 + 1, l));
    let (_, entetes) = lignes_brutes
        .by_ref()
        .find(|(_, l)| l.iter().any(|c| !texte_cellule(c).is_empty()))
        .ok_or_else(|| ErreurImport::Fichier("onglet de données vide".to_owned()))?;
    let entetes: Vec<String> = entetes.iter().map(texte_cellule).collect();
    let mut lignes = Vec::new();
    for (numero, ligne) in lignes_brutes {
        let valeurs: Vec<String> = ligne.iter().map(texte_cellule).collect();
        if valeurs.iter().all(String::is_empty) {
            continue;
        }
        lignes.push((numero, valeurs));
        if lignes.len() > LIGNES_MAX {
            return Err(ErreurImport::Fichier(format!("plus de {LIGNES_MAX} lignes : découpez le fichier")));
        }
    }
    Ok(Tableau { marque: None, entetes, lignes })
}

/// Valeur d'une cellule en texte : nombres sans zéro inutile, dates au format
/// `AAAA-MM-JJ` (et `HH:MM` s'il y a une heure), booléens en Oui/Non.
pub fn texte_cellule(cellule: &Data) -> String {
    match cellule {
        Data::Empty | Data::Error(_) => String::new(),
        Data::String(s) => s.trim().to_owned(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => nombre_en_texte(*f),
        Data::Bool(b) => if *b { "Oui" } else { "Non" }.to_owned(),
        Data::DateTime(d) => {
            let serie = d.as_f64();
            if d.is_datetime() { date_excel(serie) } else { nombre_en_texte(serie) }
        }
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.replace('T', " ").chars().take(16).collect(),
    }
}

fn nombre_en_texte(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 { format!("{f:.0}") } else { f.to_string() }
}

/// Numéro de série Excel (jours depuis le 30/12/1899) vers une date, avec l'heure si elle est non nulle.
pub fn date_excel(serie: f64) -> String {
    let jours = serie.floor();
    let Ok(date) = Date::depuis_jours_epoch(jours as i64 - 25_569) else { return nombre_en_texte(serie) };
    let minutes = ((serie - jours) * 1440.0).round() as u32;
    if minutes == 0 { date.to_string() } else { format!("{date} {:02}:{:02}", minutes / 60, minutes % 60) }
}

/// CSV en `;` (Excel français) ou `,`, guillemets doublés, première ligne en en-tête.
pub fn lire_csv(texte: &str) -> Result<Tableau, ErreurImport> {
    let texte = texte.trim_start_matches('\u{feff}');
    let premiere = texte.lines().find(|l| !l.trim().is_empty()).unwrap_or_default();
    let separateur = if premiere.matches(';').count() >= premiere.matches(',').count() { ';' } else { ',' };
    let enregistrements = decouper_csv(texte, separateur);
    let mut iterateur = enregistrements.into_iter().enumerate().map(|(i, l)| (i as u32 + 1, l));
    let (_, entetes) = iterateur
        .by_ref()
        .find(|(_, l)| l.iter().any(|c| !c.trim().is_empty()))
        .ok_or_else(|| ErreurImport::Fichier("fichier CSV vide".to_owned()))?;
    let mut lignes = Vec::new();
    for (numero, valeurs) in iterateur {
        if valeurs.iter().all(|v| v.trim().is_empty()) {
            continue;
        }
        lignes.push((numero, valeurs.into_iter().map(|v| v.trim().to_owned()).collect()));
        if lignes.len() > LIGNES_MAX {
            return Err(ErreurImport::Fichier(format!("plus de {LIGNES_MAX} lignes : découpez le fichier")));
        }
    }
    Ok(Tableau { marque: None, entetes: entetes.into_iter().map(|e| e.trim().to_owned()).collect(), lignes })
}

fn decouper_csv(texte: &str, separateur: char) -> Vec<Vec<String>> {
    let mut lignes = Vec::new();
    let mut ligne = Vec::new();
    let mut champ = String::new();
    let mut entre_guillemets = false;
    let mut caracteres = texte.chars().peekable();
    while let Some(c) = caracteres.next() {
        match c {
            '"' if entre_guillemets && caracteres.peek() == Some(&'"') => {
                champ.push('"');
                caracteres.next();
            }
            '"' => entre_guillemets = !entre_guillemets,
            c if c == separateur && !entre_guillemets => ligne.push(std::mem::take(&mut champ)),
            '\r' if !entre_guillemets => {}
            '\n' if !entre_guillemets => {
                ligne.push(std::mem::take(&mut champ));
                lignes.push(std::mem::take(&mut ligne));
            }
            c => champ.push(c),
        }
    }
    if !champ.is_empty() || !ligne.is_empty() {
        ligne.push(champ);
        lignes.push(ligne);
    }
    lignes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::Genre;

    #[test]
    fn csv_francais() {
        let tableau =
            lire_csv("\u{feff}Lot;Client;Quantité\n;\"Mairie; de Crest\";25\n\n;CCAS;\"1,5\"\n").expect("csv");
        assert_eq!(tableau.entetes, ["Lot", "Client", "Quantité"]);
        assert_eq!(tableau.lignes.len(), 2);
        assert_eq!(tableau.lignes[0], (2, vec!["".to_owned(), "Mairie; de Crest".to_owned(), "25".to_owned()]));
        assert_eq!(tableau.lignes[1].1[2], "1,5");
        assert!(lire_csv("\n\n").is_err());
    }

    #[test]
    fn cellules_en_texte() {
        assert_eq!(texte_cellule(&Data::Float(25.0)), "25");
        assert_eq!(texte_cellule(&Data::Float(2.5)), "2.5");
        assert_eq!(texte_cellule(&Data::Bool(true)), "Oui");
        assert_eq!(texte_cellule(&Data::String("  x ".into())), "x");
        assert_eq!(date_excel(46_329.0), "2026-11-03");
        assert_eq!(date_excel(46_329.5), "2026-11-03 12:00");
        assert_eq!(texte_cellule(&Data::DateTimeIso("2026-12-14T14:00:00".into())), "2026-12-14 14:00");
    }

    #[test]
    fn association_des_colonnes() {
        let colonnes = vec![
            Colonne::new("Client", true, Genre::Texte, ""),
            Colonne::new("Échéance", false, Genre::Date, ""),
            Colonne::new("Quantité", true, Genre::Nombre, ""),
        ];
        let entetes: Vec<String> = ["client *", "QUANTITE", "Couleur"].iter().map(|s| (*s).to_owned()).collect();
        let c = associer_colonnes(&entetes, &colonnes).expect("correspondance");
        assert_eq!(c.indices, [Some(0), None, Some(1)]);
        assert_eq!(c.avertissements, ["colonne « Couleur » ignorée : inconnue du modèle"]);
        let ligne: Vec<String> = vec!["Mairie".into(), "25".into(), "bleu".into()];
        assert_eq!(c.valeur(&ligne, 2), "25");
        assert_eq!(c.valeur(&ligne, 1), "");
        let sans_client: Vec<String> = vec!["Quantité".into()];
        assert!(
            matches!(associer_colonnes(&sans_client, &colonnes), Err(ErreurImport::Colonnes(m)) if m.contains("Client"))
        );
    }

    #[test]
    fn marque_et_exemple() {
        let tableau = Tableau {
            marque: Some(Marque {
                cas: Cas::Factures,
                version: 1,
                dokos: String::new(),
                modele: None,
                modele_modifie: None,
            }),
            entetes: vec!["Client".into()],
            lignes: vec![(2, vec!["Mairie de Crest".into()]), (3, vec![" ".into()]), (4, vec!["CCAS".into()])],
        };
        assert!(tableau.verifier_marque(Cas::Factures).is_ok());
        assert!(tableau.verifier_marque(Cas::Devis).is_err());
        let tableau = tableau.sans_exemple(&["mairie de crest".to_owned()]);
        assert_eq!(tableau.lignes.len(), 1);
        let sans_marque = Tableau::default();
        assert!(matches!(sans_marque.verifier_marque(Cas::Factures), Err(ErreurImport::Modele(_))));
        let future = Tableau {
            marque: Some(Marque {
                cas: Cas::Factures,
                version: 99,
                dokos: String::new(),
                modele: None,
                modele_modifie: None,
            }),
            ..Default::default()
        };
        assert!(future.verifier_marque(Cas::Factures).is_err());
    }

    #[test]
    fn xlsm_refuse() {
        assert!(matches!(lire_fichier(Path::new("x.xlsm")), Err(ErreurImport::Fichier(m)) if m.contains("macros")));
    }
}
