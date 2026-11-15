//! `dokos articles|modele|importer|brouillons|…` : catalogue, imports et suivi.

use std::path::{Path, PathBuf};

use dokos_core::article::{
    ARTICLES_PAR_GROUPE_MAX, CreationArticle, Noeud, NouvelArticle, activer_article, arbre_des_groupes,
    articles_du_groupe, chercher_article_identique, creer_article,
};
use dokos_core::brouillons::{brouillons_de_vente, evenements_non_publies};
use dokos_core::date::Date;
use dokos_core::doublon::SiDoublon;
use dokos_core::inscription::lister_inscrits;
use dokos_core::suivi::{
    contrats_qui_se_terminent, contrats_recents, contrats_sans_fin, factures_en_retard, soldes_par_client,
};
use dokos_core::transformation::{ResultatTransformation, Transformation, transformer};
use dokos_core::{Dokos, Transport};
use dokos_import::analyse::{OptionsCreation, StatutCreation, cle_choix};
use dokos_import::modele::{ContexteModele, nom_fichier_modele};
use dokos_import::rapport::ecrire_rapport;
use dokos_import::{
    Cas, Options, Statut, analyser, creer, generer_modele, lire_fichier, referentiel_pour_fichier,
    referentiel_pour_generer, verifier,
};

use crate::erreur::Erreur;

// --- Catalogue --------------------------------------------------------------

pub fn afficher_arbre<T: Transport>(dokos: &Dokos<T>) -> Result<(), Erreur> {
    let arbre = arbre_des_groupes(dokos)?;
    if arbre.is_empty() {
        println!("Aucun groupe d'articles.");
    }
    fn imprimer(noeuds: &[Noeud], profondeur: usize) {
        for noeud in noeuds {
            println!("{}{}{}", "  ".repeat(profondeur), noeud.id, if noeud.groupe { "/" } else { "" });
            imprimer(&noeud.enfants, profondeur + 1);
        }
    }
    imprimer(&arbre, 0);
    Ok(())
}

pub fn lister_articles<T: Transport>(dokos: &Dokos<T>, groupe: &str, texte: Option<&str>) -> Result<(), Erreur> {
    let articles = articles_du_groupe(dokos, groupe, texte, ARTICLES_PAR_GROUPE_MAX)?;
    if articles.is_empty() {
        println!("Aucun article vendable dans « {groupe} ».");
        return Ok(());
    }
    let largeur = articles.iter().map(|a| a.code.chars().count()).max().unwrap_or(0);
    for a in &articles {
        println!("{:<largeur$}  {}  [{}] {}", a.code, a.libelle, a.groupe, a.unite);
    }
    if articles.len() as u32 == ARTICLES_PAR_GROUPE_MAX {
        println!("(liste limitée à {} articles)", articles.len());
    }
    Ok(())
}

pub struct DemandeArticle<'a> {
    pub code: &'a str,
    pub libelle: &'a str,
    pub groupe: &'a str,
    pub unite: &'a str,
    pub vendable: bool,
    pub description: Option<&'a str>,
    pub simulation: bool,
    pub si_doublon: SiDoublon,
}

pub fn creer_un_article<T: Transport>(dokos: &Dokos<T>, demande: &DemandeArticle<'_>) -> Result<(), Erreur> {
    let article = NouvelArticle::new(demande.code, demande.libelle, demande.groupe, demande.unite)?
        .vendable(demande.vendable)
        .avec_description(demande.description);
    println!("Article : {} — {} (groupe {})", article.code(), article.libelle(), article.groupe());
    if demande.simulation {
        if let Some(existant) = chercher_article_identique(dokos, &article)? {
            println!(
                "Déjà dans Dokos : {} ({}). Sans --simulation, rien ne serait créé.",
                existant.code, existant.libelle
            );
            return Ok(());
        }
        let corps = serde_json::to_string_pretty(&article.corps()).map_err(|e| Erreur::Saisie(e.to_string()))?;
        println!("\nSimulation : rien n'a été écrit dans Dokos. Corps qui serait envoyé :\n{corps}");
        return Ok(());
    }
    match creer_article(dokos, &article, demande.si_doublon)? {
        CreationArticle::DejaExistant(a) => {
            println!("Déjà dans Dokos : {} ({}). Rien n'a été créé.", a.code, a.libelle)
        }
        CreationArticle::Cree(a) => {
            println!("Article {} créé, désactivé. Pour l'activer : dokos articles activer {}", a.code, a.code)
        }
    }
    Ok(())
}

pub fn activer_un_article<T: Transport>(dokos: &Dokos<T>, code: &str) -> Result<(), Erreur> {
    let a = activer_article(dokos, code)?;
    println!("Article {} {}.", a.code, if a.disabled == 0 { "actif" } else { "toujours désactivé" });
    Ok(())
}

// --- Modèles et import -------------------------------------------------------------

pub fn generer_un_modele<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    modele: Option<&str>,
    url_dokos: &str,
    societe: Option<&str>,
    sortie: Option<PathBuf>,
) -> Result<(), Erreur> {
    let aujourdhui = Date::aujourdhui();
    let referentiel = referentiel_pour_generer(dokos, cas, aujourdhui, modele)?;
    let contexte = ContexteModele {
        url_dokos: url_dokos.to_owned(),
        date: aujourdhui.to_string(),
        societe: societe.map(str::to_owned),
    };
    let octets = generer_modele(cas, &referentiel, &contexte)?;
    let chemin = match sortie {
        Some(c) => c,
        None => dokos_config::dossier_documents()?.join("modeles").join(nom_fichier_modele(cas, &referentiel)),
    };
    if let Some(dossier) = chemin.parent() {
        std::fs::create_dir_all(dossier).map_err(|e| Erreur::Fichier(format!("{} : {e}", dossier.display())))?;
    }
    std::fs::write(&chemin, octets).map_err(|e| Erreur::Fichier(format!("{} : {e}", chemin.display())))?;
    println!("Modèle « {} » écrit : {}", cas.libelle(), chemin.display());
    println!(
        "Remplissez l'onglet Données à partir de la ligne 3, puis : dokos importer {} \"{}\" --simulation",
        cas.nom(),
        chemin.display()
    );
    Ok(())
}

/// `champ=texte=identifiant`, pour trancher une ambiguïté sans retoucher le fichier.
pub fn choix(texte: &str) -> Result<(String, String), String> {
    let morceaux: Vec<&str> = texte.splitn(3, '=').collect();
    let [champ, valeur, id] = morceaux.as_slice() else {
        return Err(format!("« {texte} » : format attendu champ=texte=identifiant (ex. client=Crest=CUST-2)"));
    };
    if champ.trim().is_empty() || valeur.trim().is_empty() || id.trim().is_empty() {
        return Err(format!("« {texte} » : champ, texte et identifiant sont obligatoires"));
    }
    Ok((cle_choix(champ.trim(), valeur.trim()), id.trim().to_owned()))
}

pub struct DemandeImport<'a> {
    pub cas: Cas,
    pub fichier: &'a Path,
    pub choix: Vec<(String, String)>,
    pub simulation: bool,
    pub seulement_prets: bool,
    pub si_doublon: SiDoublon,
    pub rapport: Option<PathBuf>,
}

pub fn importer<T: Transport>(
    dokos: &Dokos<T>,
    societe: Option<&str>,
    demande: &DemandeImport<'_>,
) -> Result<(), Erreur> {
    let tableau = lire_fichier(demande.fichier)?;
    tableau.verifier_marque(demande.cas)?;
    let referentiel = referentiel_pour_fichier(dokos, demande.cas, &tableau, Date::aujourdhui())?;
    let options = Options {
        societe: societe.map(str::to_owned),
        choix: demande.choix.iter().cloned().collect(),
        aujourdhui: None,
    };
    let mut analyse = analyser(dokos, demande.cas, &tableau, &referentiel, &options)?;
    verifier(dokos, &mut analyse)?;

    for avertissement in &analyse.avertissements {
        println!("Attention : {avertissement}");
    }
    println!(
        "{} ligne(s), {} document(s), {} prêt(s), {} ligne(s) à corriger, {} doublon(s).",
        analyse.lignes.len(),
        analyse.documents.len(),
        analyse.documents_prets(),
        analyse.lignes_bloquantes(),
        analyse.doublons()
    );
    for ligne in analyse.lignes.iter().filter(|l| !l.statut.est_pret()) {
        let detail = match &ligne.statut {
            Statut::APreciser { champ, texte, candidats } => format!(
                "{champ} « {texte} » ambigu : {} — tranchez avec --choix {champ}={texte}=IDENTIFIANT",
                candidats.iter().map(|c| format!("{} ({})", c.id, c.libelle)).collect::<Vec<_>>().join(", ")
            ),
            autre => autre.message(),
        };
        println!("  ligne {:>4} : {detail}", ligne.numero);
    }
    for document in analyse.documents.iter().filter(|d| d.doublon.is_some()) {
        if let Some(e) = &document.doublon {
            println!("  {} : identique à {} ({})", document.libelle, e.id, e.description);
        }
    }

    if demande.simulation {
        println!("\nSimulation : rien n'a été écrit dans Dokos.");
        ecrire_rapport_si_demande(demande.rapport.as_deref(), &analyse, None)?;
        return Ok(());
    }
    let resultats = creer(
        dokos,
        &analyse,
        &OptionsCreation { seulement_prets: demande.seulement_prets, si_doublon: demande.si_doublon },
    )?;
    let mut crees = 0;
    for r in &resultats {
        let statut = match r.statut {
            StatutCreation::Cree => {
                crees += 1;
                "créé"
            }
            StatutCreation::Existant => "déjà présent",
            StatutCreation::Ignore => "ignoré",
            StatutCreation::Erreur => "ERREUR",
        };
        println!("  {:<13} {} {} — {}", statut, r.libelle, r.id.as_deref().unwrap_or(""), r.message);
    }
    println!("\n{crees} document(s) créé(s).");
    ecrire_rapport_si_demande(demande.rapport.as_deref(), &analyse, Some(&resultats))?;
    if resultats.iter().any(|r| r.statut == StatutCreation::Erreur) {
        return Err(Erreur::Bloque("l'import s'est arrêté sur une erreur de Dokos ; voir ci-dessus".to_owned()));
    }
    Ok(())
}

fn ecrire_rapport_si_demande(
    chemin: Option<&Path>,
    analyse: &dokos_import::Analyse,
    resultats: Option<&[dokos_import::ResultatDocument]>,
) -> Result<(), Erreur> {
    let Some(chemin) = chemin else { return Ok(()) };
    let octets = ecrire_rapport(analyse, resultats)?;
    std::fs::write(chemin, octets).map_err(|e| Erreur::Fichier(format!("{} : {e}", chemin.display())))?;
    println!("Rapport : {}", chemin.display());
    Ok(())
}

// --- Suivi ---------------------------------------------------------------------------

pub fn afficher_brouillons<T: Transport>(
    ventes: Option<&Dokos<T>>,
    evenements: Option<&Dokos<T>>,
) -> Result<(), Erreur> {
    let mut rien = true;
    if let Some(dokos) = ventes {
        let utilisateur = dokos.utilisateur_connecte()?;
        for b in brouillons_de_vente(dokos, &utilisateur)? {
            rien = false;
            println!(
                "{:<14} {:<22} {} {:<12} {:>10.2} {}  (modifié {})",
                b.doctype, b.id, b.date, b.tiers, b.total, b.devise, b.modifie
            );
        }
    }
    if let Some(dokos) = evenements {
        let utilisateur = dokos.utilisateur_connecte()?;
        for b in evenements_non_publies(dokos, &utilisateur)? {
            rien = false;
            println!("{:<14} {:<22} {} {}  (non publié, modifié {})", b.doctype, b.id, b.date, b.tiers, b.modifie);
        }
    }
    if rien {
        println!("Aucun brouillon créé par l'outil en attente.");
    }
    Ok(())
}

pub fn transformer_document<T: Transport>(
    dokos: &Dokos<T>,
    transformation: Transformation,
    source: &str,
    si_doublon: SiDoublon,
) -> Result<(), Erreur> {
    match transformer(dokos, transformation, source, si_doublon)? {
        ResultatTransformation::Cree(d) => {
            println!(
                "{} créée en brouillon : {} (total {:.2} {})",
                majuscule(transformation.libelle()),
                d.id,
                d.total,
                d.devise
            )
        }
        ResultatTransformation::DejaExistant(d) => {
            println!(
                "Un document suivant existe déjà : {} ({}). Rien n'a été créé ; --autoriser-doublon pour forcer.",
                d.id,
                d.statut()
            )
        }
    }
    Ok(())
}

pub fn afficher_inscrits<T: Transport>(
    dokos: &Dokos<T>,
    evenement: &str,
    sortie: Option<PathBuf>,
) -> Result<(), Erreur> {
    let inscrits = lister_inscrits(dokos, evenement)?;
    println!("{} inscrit(s) à {evenement}.", inscrits.len());
    let colonnes = ["first_name", "last_name", "email", "mobile_number"];
    for i in &inscrits {
        let valeurs: Vec<&str> = colonnes.iter().map(|c| i.get(*c).map(String::as_str).unwrap_or("")).collect();
        println!("  {}", valeurs.join("  "));
    }
    if let Some(chemin) = sortie {
        let mut entetes: Vec<String> = Vec::new();
        for i in &inscrits {
            for cle in i.keys() {
                if !entetes.contains(cle) {
                    entetes.push(cle.clone());
                }
            }
        }
        let lignes: Vec<Vec<String>> =
            inscrits.iter().map(|i| entetes.iter().map(|e| i.get(e).cloned().unwrap_or_default()).collect()).collect();
        let contenu = std::iter::once(entetes.join(";"))
            .chain(lignes.iter().map(|l| l.iter().map(|v| v.replace(';', ",")).collect::<Vec<_>>().join(";")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&chemin, contenu).map_err(|e| Erreur::Fichier(format!("{} : {e}", chemin.display())))?;
        println!("Liste écrite : {}", chemin.display());
    }
    Ok(())
}

pub fn afficher_retards<T: Transport>(dokos: &Dokos<T>) -> Result<(), Erreur> {
    let aujourdhui = Date::aujourdhui();
    let factures = factures_en_retard(dokos, aujourdhui)?;
    if factures.is_empty() {
        println!("Aucune facture en retard.");
        return Ok(());
    }
    for f in &factures {
        println!(
            "{:<22} {:<28} échéance {}  {:>4} j  solde {:>10.2} {}",
            f.id,
            f.customer_name,
            f.due_date,
            f.retard(aujourdhui),
            f.outstanding_amount,
            f.currency
        );
    }
    println!("\nPar client :");
    for (client, solde, nombre) in soldes_par_client(&factures) {
        println!("  {client:<30} {solde:>10.2}  ({nombre} facture(s))");
    }
    Ok(())
}

pub fn afficher_alertes_rh<T: Transport>(dokos: &Dokos<T>, types_cdd: &[String]) -> Result<(), Erreur> {
    let aujourdhui = Date::aujourdhui();
    let fins = contrats_qui_se_terminent(dokos, aujourdhui, 30)?;
    println!("Contrats qui se terminent dans les 30 jours : {}", fins.len());
    for c in &fins {
        println!("  {} {} ({}) — fin le {}", c.id, c.employee_name, c.contract_type, c.relieving_date);
    }
    let sans_fin = contrats_sans_fin(dokos, types_cdd)?;
    println!("CDD sans date de fin : {}", sans_fin.len());
    for c in &sans_fin {
        println!("  {} {} ({}) — entré le {}", c.id, c.employee_name, c.contract_type, c.date_of_joining);
    }
    let recents = contrats_recents(dokos, aujourdhui, 60)?;
    println!("Contrats de moins de 60 jours (période d'essai probable) : {}", recents.len());
    for c in &recents {
        println!("  {} {} ({}) — entré le {}", c.id, c.employee_name, c.contract_type, c.date_of_joining);
    }
    Ok(())
}

fn majuscule(texte: &str) -> String {
    let mut c = texte.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choix_analyse() {
        assert_eq!(choix("client=Crest=CUST-2"), Ok(("client|crest".to_owned(), "CUST-2".to_owned())));
        assert_eq!(choix("article=Repas du midi=REPAS-M").map(|c| c.1), Ok("REPAS-M".to_owned()));
        assert!(choix("client=Crest").is_err());
        assert!(choix("=x=y").is_err());
    }
}
