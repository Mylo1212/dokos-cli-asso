//! Import de tableaux : modèles, analyse, création, rapport.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dokos_core::date::Date;
use dokos_core::doublon::SiDoublon;
use dokos_core::{Dokos, Transport};
use dokos_import::analyse::{OptionsCreation, StatutCreation, cle_choix};
use dokos_import::modele::{ContexteModele, nom_fichier_modele};
use dokos_import::rapport::ecrire_rapport;
use dokos_import::{
    Analyse, Cas, Options, Referentiel, Statut, analyser, creer, generer_modele, lire_fichier,
    referentiel_pour_fichier, referentiel_pour_generer, verifier,
};

use crate::erreurs::{ErreurVue, erreur as texte};
use crate::services::{Resultat, lien_desk};
use crate::vues::*;

pub const DOSSIER_MODELES: &str = "modeles";

pub fn cas(nom: &str) -> Resultat<Cas> {
    Cas::depuis_nom(nom).ok_or_else(|| ErreurVue::validation(format!("cas d'import inconnu : {nom}")))
}

/// Cas disponibles, avec leurs colonnes.
pub fn cas_disponibles() -> Vec<CasVue> {
    let referentiel = Referentiel::default();
    Cas::TOUS
        .into_iter()
        .map(|c| CasVue {
            nom: c.nom().to_owned(),
            libelle: c.libelle().to_owned(),
            resultat: c.resultat().to_owned(),
            profil: c.profil().to_owned(),
            colonnes: c
                .colonnes(&referentiel)
                .into_iter()
                .map(|col| ColonneVue { nom: col.nom, obligatoire: col.obligatoire, aide: col.aide })
                .collect(),
            type_modele: c.type_modele().map(|t| t.libelle().to_owned()),
        })
        .collect()
}

/// Génère le modèle du cas dans `dossier/modeles/` et renvoie son chemin.
/// `modele` : document « Template » copié, pour les cas « à partir d'un modèle ».
#[allow(clippy::too_many_arguments)]
pub fn generer_un_modele<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    modele: Option<&str>,
    url_dokos: &str,
    societe: Option<&str>,
    dossier: &Path,
    aujourdhui: Date,
) -> Resultat<PathBuf> {
    let referentiel = referentiel_pour_generer(dokos, cas, modele).map_err(texte)?;
    let contexte = ContexteModele {
        url_dokos: url_dokos.to_owned(),
        date: aujourdhui.to_string(),
        societe: societe.map(str::to_owned),
    };
    let octets = generer_modele(cas, &referentiel, &contexte).map_err(texte)?;
    let dossier = dossier.join(DOSSIER_MODELES);
    std::fs::create_dir_all(&dossier).map_err(|e| ErreurVue::fichier(format!("{} : {e}", dossier.display())))?;
    let chemin = dossier.join(nom_fichier_modele(cas, &referentiel));
    std::fs::write(&chemin, octets).map_err(|e| ErreurVue::fichier(format!("{} : {e}", chemin.display())))?;
    tracing::info!(cas = %cas, chemin = %chemin.display(), "modèle généré");
    Ok(chemin)
}

fn options(societe: Option<&str>, choix: &[ChoixImport], aujourdhui: Date) -> Options {
    let mut options =
        Options { societe: societe.map(str::to_owned), choix: BTreeMap::new(), aujourdhui: Some(aujourdhui) };
    for c in choix {
        options.choix.insert(cle_choix(&c.champ, &c.texte), c.id.clone());
    }
    options
}

/// Lit, analyse et vérifie (doublons) le fichier. Rien n'est écrit.
fn preparer<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    chemin: &Path,
    societe: Option<&str>,
    choix: &[ChoixImport],
    aujourdhui: Date,
) -> Resultat<Analyse> {
    let tableau = lire_fichier(chemin).map_err(texte)?;
    tableau.verifier_marque(cas).map_err(texte)?;
    let referentiel = referentiel_pour_fichier(dokos, cas, &tableau).map_err(texte)?;
    let mut analyse =
        analyser(dokos, cas, &tableau, &referentiel, &options(societe, choix, aujourdhui)).map_err(texte)?;
    verifier(dokos, &mut analyse).map_err(texte)?;
    Ok(analyse)
}

pub fn vue_analyse(analyse: &Analyse, chemin: &Path) -> AnalyseVue {
    AnalyseVue {
        cas: analyse.cas.nom().to_owned(),
        fichier: chemin.display().to_string(),
        entetes: analyse.entetes.clone(),
        avertissements: analyse.avertissements.clone(),
        lignes: analyse
            .lignes
            .iter()
            .map(|l| {
                let (champ, texte_valeur, candidats) = match &l.statut {
                    Statut::APreciser { champ, texte, candidats } => (
                        champ.clone(),
                        texte.clone(),
                        candidats
                            .iter()
                            .map(|c| CandidatVue { id: c.id.clone(), libelle: c.libelle.clone() })
                            .collect(),
                    ),
                    Statut::Introuvable { champ, texte } => (champ.clone(), texte.clone(), Vec::new()),
                    _ => (String::new(), String::new(), Vec::new()),
                };
                LigneImportVue {
                    numero: l.numero,
                    valeurs: l.valeurs.clone(),
                    statut: match &l.statut {
                        Statut::Pret => "pret",
                        Statut::APreciser { .. } => "a_preciser",
                        Statut::Introuvable { .. } => "introuvable",
                        Statut::Invalide(_) => "invalide",
                    }
                    .to_owned(),
                    message: l.statut.message(),
                    champ,
                    texte: texte_valeur,
                    candidats,
                    document: l.document,
                }
            })
            .collect(),
        documents: analyse
            .documents
            .iter()
            .map(|d| DocumentImportVue {
                cle: d.cle.clone(),
                libelle: d.libelle.clone(),
                pret: d.pret(),
                doublon: d.doublon.as_ref().map(|e| format!("{} ({})", e.id, e.description)),
                lignes: d.lignes.clone(),
            })
            .collect(),
        prets: analyse.documents_prets(),
        bloquantes: analyse.lignes_bloquantes(),
        doublons: analyse.doublons(),
    }
}

pub fn analyser_un_fichier<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    chemin: &Path,
    societe: Option<&str>,
    choix: &[ChoixImport],
    aujourdhui: Date,
) -> Resultat<AnalyseVue> {
    let analyse = preparer(dokos, cas, chemin, societe, choix, aujourdhui)?;
    Ok(vue_analyse(&analyse, chemin))
}

/// Chemin du fichier résultat, à côté du dossier Dokos : `<nom>.resultat.xlsx`.
pub fn chemin_rapport(dossier: &Path, fichier: &Path) -> PathBuf {
    let nom = fichier.file_stem().and_then(|n| n.to_str()).unwrap_or("import");
    dossier.join(format!("{}.resultat.xlsx", dokos_config::nom_de_fichier(nom)))
}

/// Analyse de nouveau puis crée. Le rapport est écrit même si la création s'arrête.
#[allow(clippy::too_many_arguments)]
pub fn executer_un_import<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    cas: Cas,
    chemin: &Path,
    societe: Option<&str>,
    choix: &[ChoixImport],
    seulement_prets: bool,
    si_doublon: SiDoublon,
    dossier: &Path,
    aujourdhui: Date,
) -> Resultat<ResultatImportVue> {
    let analyse = preparer(dokos, cas, chemin, societe, choix, aujourdhui)?;
    let resultats = creer(dokos, &analyse, &OptionsCreation { seulement_prets, si_doublon }).map_err(texte)?;
    let rapport = chemin_rapport(dossier, chemin);
    let octets = ecrire_rapport(&analyse, Some(&resultats)).map_err(texte)?;
    std::fs::write(&rapport, octets).map_err(|e| ErreurVue::fichier(format!("{} : {e}", rapport.display())))?;
    let compte = |statut: StatutCreation| resultats.iter().filter(|r| r.statut == statut).count();
    let vue = ResultatImportVue {
        crees: compte(StatutCreation::Cree),
        existants: compte(StatutCreation::Existant),
        erreurs: compte(StatutCreation::Erreur),
        ignores: compte(StatutCreation::Ignore),
        documents: resultats
            .into_iter()
            .map(|r| DocumentCreeVue {
                cle: r.cle,
                libelle: r.libelle,
                statut: match r.statut {
                    StatutCreation::Cree => "cree",
                    StatutCreation::Existant => "existant",
                    StatutCreation::Ignore => "ignore",
                    StatutCreation::Erreur => "erreur",
                }
                .to_owned(),
                lien: r
                    .id
                    .as_ref()
                    .filter(|_| !r.doctype.is_empty())
                    .map(|id| lien_desk(base, &r.doctype, id))
                    .unwrap_or_default(),
                id: r.id.unwrap_or_default(),
                message: r.message,
            })
            .collect(),
        rapport: rapport.display().to_string(),
    };
    tracing::info!(cas = %cas, crees = vue.crees, existants = vue.existants, erreurs = vue.erreurs, "import terminé");
    Ok(vue)
}

/// Écrit un tableau simple (en-têtes + lignes) en `.xlsx`.
pub fn ecrire_tableau(chemin: &Path, titre: &str, entetes: &[String], lignes: &[Vec<String>]) -> Resultat<()> {
    use rust_xlsxwriter::{Format, Workbook};
    let construire = || -> Result<(), rust_xlsxwriter::XlsxError> {
        let mut classeur = Workbook::new();
        let feuille = classeur.add_worksheet();
        feuille.set_name(titre.chars().take(31).collect::<String>())?;
        feuille.set_freeze_panes(1, 0)?;
        let gras = Format::new().set_bold();
        for (c, e) in entetes.iter().enumerate() {
            feuille.write_string_with_format(0, c as u16, e, &gras)?;
            feuille.set_column_width(c as u16, 22)?;
        }
        for (r, ligne) in lignes.iter().enumerate() {
            for (c, v) in ligne.iter().enumerate() {
                feuille.write_string(r as u32 + 1, c as u16, v)?;
            }
        }
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
        let dossier = std::env::temp_dir().join(format!("dokos-import-test-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).expect("dossier");
        dossier
    }

    #[test]
    fn modele_puis_import_de_factures() {
        let dossier = dossier_test("factures");
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);
        let jour: Date = "2026-11-03".parse().expect("date");
        // Référentiel : clients, articles.
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        faux.repondre_json(200, &json!({"data": [{"name": "REPAS", "item_name": "Repas", "stock_uom": "Unité", "item_group": "Restauration"}]}));
        let modele = generer_un_modele(
            &dokos,
            Cas::Factures,
            None,
            "https://dokos.exemple.fr",
            Some("Les Amanins"),
            &dossier,
            jour,
        )
        .expect("modèle");
        assert!(modele.ends_with("modeles/modele-factures.xlsx"));

        // Le modèle rempli : on réécrit un CSV ? Non : on relit le modèle lui-même (sa ligne d'exemple
        // est ignorée), ce qui donne un import vide mais valide.
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        faux.repondre_json(200, &json!({"data": []}));
        let analyse =
            analyser_un_fichier(&dokos, Cas::Factures, &modele, Some("Les Amanins"), &[], jour).expect("analyse");
        assert_eq!(analyse.cas, "factures");
        assert!(analyse.lignes.is_empty());
        assert!(analyse.avertissements.iter().any(|a| a.contains("aucune ligne")));

        // Un CSV n'a pas de marque : refusé.
        let csv = dossier.join("factures.csv");
        std::fs::write(&csv, "Lot;Client\n;Mairie\n").expect("csv");
        let erreur = analyser_un_fichier(&dokos, Cas::Factures, &csv, None, &[], jour).expect_err("sans marque");
        assert_eq!(erreur.genre, "fichier");
        let _ = std::fs::remove_dir_all(dossier);
    }

    #[test]
    fn cas_et_rapport() {
        assert_eq!(cas("Factures").expect("cas"), Cas::Factures);
        assert!(cas("avoirs").is_err());
        let liste = cas_disponibles();
        assert_eq!(liste.len(), 4);
        assert!(liste[0].colonnes.iter().any(|c| c.obligatoire));
        let modele = liste.iter().find(|c| c.nom == "factures-modele").expect("cas modèle");
        assert_eq!(modele.type_modele.as_deref(), Some("facture"));
        assert_eq!(liste.iter().filter(|c| c.type_modele.is_none()).count(), 3);
        assert!(liste.iter().all(|c| c.nom != "employes" && c.nom != "evenements" && c.nom != "devis"));
        let rapport = chemin_rapport(Path::new("/tmp/Dokos"), Path::new("/x/factures octobre.xlsx"));
        assert!(rapport.ends_with("factures-octobre.resultat.xlsx"));
        let dossier = dossier_test("tableau");
        let chemin = dossier.join("inscrits.xlsx");
        ecrire_tableau(&chemin, "Inscrits", &["Prénom".into()], &[vec!["Marie".into()]]).expect("tableau");
        let relu = dokos_import::lire_fichier(&chemin).expect("relecture");
        assert_eq!(relu.entetes, ["Prénom"]);
        assert_eq!(relu.lignes[0].1, ["Marie"]);
        let _ = std::fs::remove_dir_all(dossier);
    }
}
