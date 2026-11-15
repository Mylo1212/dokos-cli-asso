//! `dokos employe|contrat|format …` : employés, contrats et formats d'impression.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use conformite::Regles;
use conformite::dokos::{verifier_contrat_existant, verifier_format, verifier_nouveau_contrat};
use conformite::donnees::{Probleme, a_des_erreurs};
use conformite::proposition::{LigneDiff, difference};
use conformite::rapport::{EtatMention, RapportGabarit};
use conformite::regles::Contexte;
use dokos_core::contrat::{CreationContrat, DOCTYPE_CONTRAT, NouveauContrat, creer_contrat};
use dokos_core::date::Date;
use dokos_core::employe::{CreationEmploye, NouvelEmploye, chercher_employe_identique, creer_employe};
use dokos_core::impression::{lister_formats, telecharger_pdf};
use dokos_core::meta::lire_meta;
use dokos_core::vente::SiDoublon;
use dokos_core::{Dokos, Transport};

use crate::erreur::Erreur;

pub struct DemandeEmploye<'a> {
    pub prenom: &'a str,
    pub nom: &'a str,
    pub genre: &'a str,
    pub naissance: Date,
    pub entree: Date,
    pub email: Option<&'a str>,
    pub poste: Option<&'a str>,
    pub simulation: bool,
}

pub fn creer_un_employe<T: Transport>(
    dokos: &Dokos<T>,
    societe: Option<&str>,
    demande: &DemandeEmploye<'_>,
) -> Result<(), Erreur> {
    let societe = societe.ok_or_else(|| {
        Erreur::Saisie("société manquante : enregistrez-la avec « dokos connexion enregistrer --societe … »".into())
    })?;
    let employe =
        NouvelEmploye::new(demande.prenom, demande.nom, demande.genre, demande.naissance, demande.entree, societe)?
            .avec_email(demande.email)?
            .avec_poste(demande.poste);
    println!("Employé : {} — entrée le {}", employe.nom_complet(), employe.entree());
    if demande.simulation {
        if let Some(existant) = chercher_employe_identique(dokos, &employe)? {
            println!(
                "Déjà dans Dokos : {} ({}). Sans --simulation, rien ne serait créé.",
                existant.id, existant.nom_complet
            );
            return Ok(());
        }
        let corps = serde_json::to_string_pretty(&employe.corps()).map_err(|e| Erreur::Saisie(e.to_string()))?;
        println!("\nSimulation : rien n'a été écrit dans Dokos. Corps qui serait envoyé :\n{corps}");
        return Ok(());
    }
    match creer_employe(dokos, &employe, SiDoublon::Reutiliser)? {
        CreationEmploye::DejaExistant(e) => {
            println!("Déjà dans Dokos : {} ({}). Rien n'a été créé.", e.id, e.nom_complet)
        }
        CreationEmploye::Cree(e) => println!("Employé créé : {} ({})", e.id, e.nom_complet),
    }
    Ok(())
}

pub struct DemandeContrat<'a> {
    pub employe: &'a str,
    pub type_contrat: &'a str,
    pub poste: &'a str,
    pub entree: Date,
    pub fin: Option<Date>,
    pub heures: Option<f64>,
    pub repartition: Option<[f64; 7]>,
    pub champs: BTreeMap<String, String>,
    pub format: Option<&'a str>,
    pub simulation: bool,
    pub si_doublon: SiDoublon,
}

pub fn creer_un_contrat<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    societe: Option<&str>,
    demande: DemandeContrat<'_>,
) -> Result<(), Erreur> {
    let meta = lire_meta(dokos, DOCTYPE_CONTRAT)?;
    let contrat =
        NouveauContrat::new(demande.employe, demande.type_contrat, demande.poste, demande.entree, demande.fin)?
            .avec_heures(demande.heures, demande.repartition)?
            .avec_societe(societe)
            .avec_champs(&meta, demande.champs)?;

    avertir_si_non_validee(regles);
    let verification = verifier_nouveau_contrat(dokos, regles, &contrat, demande.format)?;
    afficher_problemes(&verification.problemes);
    if let Some(rapport) = &verification.gabarit {
        afficher_rapport(rapport);
    }
    if a_des_erreurs(&verification.problemes) {
        return Err(Erreur::Bloque("le contrat a des erreurs : rien n'a été créé".into()));
    }
    if demande.simulation {
        let corps = serde_json::to_string_pretty(&contrat.corps()).map_err(|e| Erreur::Saisie(e.to_string()))?;
        println!("\nSimulation : rien n'a été écrit dans Dokos. Corps qui serait envoyé :\n{corps}");
        return Ok(());
    }
    match creer_contrat(dokos, &contrat, demande.si_doublon)? {
        CreationContrat::DejaExistant(c) => {
            println!("\nContrat identique déjà dans Dokos : {}. Rien n'a été créé.", c.id);
        }
        CreationContrat::Cree(c) => {
            println!("\nContrat créé : {} (actif dès maintenant : ce DocType n'a pas de brouillon)", c.id);
            if let Some(format) = demande.format {
                println!("PDF : dokos contrat pdf {} --format \"{format}\"", c.id);
            }
        }
    }
    Ok(())
}

pub fn verifier_un_contrat<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    id: &str,
    format: Option<&str>,
) -> Result<(), Erreur> {
    avertir_si_non_validee(regles);
    let verification = verifier_contrat_existant(dokos, regles, id, format)?;
    afficher_problemes(&verification.problemes);
    if let Some(rapport) = &verification.gabarit {
        afficher_rapport(rapport);
    }
    if verification.problemes.is_empty() && verification.gabarit.as_ref().is_none_or(RapportGabarit::conforme) {
        println!("Aucun problème trouvé.");
    }
    Ok(())
}

pub fn pdf<T: Transport>(dokos: &Dokos<T>, id: &str, format: &str, sortie: Option<PathBuf>) -> Result<(), Erreur> {
    let pdf = telecharger_pdf(dokos, DOCTYPE_CONTRAT, id, format)?;
    let chemin = match sortie {
        Some(chemin) => chemin,
        None => dokos_config::dossier_documents()?.join(format!("{}.pdf", dokos_config::nom_de_fichier(id))),
    };
    ecrire(&chemin, &pdf)?;
    println!("PDF enregistré : {}", chemin.display());
    Ok(())
}

pub fn lister_les_formats<T: Transport>(dokos: &Dokos<T>) -> Result<(), Erreur> {
    let formats = lister_formats(dokos, DOCTYPE_CONTRAT)?;
    if formats.is_empty() {
        println!("Aucun format d'impression actif pour « {DOCTYPE_CONTRAT} ».");
    }
    for format in formats {
        println!("{format}");
    }
    Ok(())
}

pub fn verifier_un_format<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    format: &str,
    contexte: &Contexte,
    sortie_proposition: Option<Option<PathBuf>>,
) -> Result<(), Erreur> {
    avertir_si_non_validee(regles);
    let verification = verifier_format(dokos, regles, format, contexte)?;
    afficher_rapport(&verification.rapport);
    let Some(sortie) = sortie_proposition else {
        if verification.proposition.is_some() {
            println!("\nPour obtenir un gabarit corrigé : dokos format proposer \"{format}\" (mêmes options)");
        }
        return Ok(());
    };
    let Some(proposition) = verification.proposition else {
        println!("\nAucune mention à ajouter.");
        return Ok(());
    };
    println!("\nMentions ajoutées :");
    for ajout in &proposition.ajouts {
        println!("  + {ajout}");
    }
    if !proposition.champs_a_creer.is_empty() {
        println!(
            "Champs à créer dans « {DOCTYPE_CONTRAT} » (Personnaliser le formulaire) : {}",
            proposition.champs_a_creer.join(", ")
        );
    }
    if let Some(apres) = &verification.rapport_propose
        && apres.a_completer > 0
    {
        println!("Textes « [à compléter] » à remplacer : {}", apres.a_completer);
    }
    println!("\nDifférences :");
    for ligne in difference(&verification.gabarit, &proposition.gabarit) {
        match ligne {
            LigneDiff::Ajoutee(l) => println!("+ {l}"),
            LigneDiff::Retiree(l) => println!("- {l}"),
            LigneDiff::Commune(_) => {}
        }
    }
    let chemin = match sortie {
        Some(chemin) => chemin,
        None => dokos_config::dossier_documents()?
            .join(format!("{}.proposition.html", dokos_config::nom_de_fichier(format))),
    };
    ecrire(&chemin, proposition.gabarit.as_bytes())?;
    println!("\nGabarit proposé : {}", chemin.display());
    println!(
        "Rien n'a été modifié dans Dokos : relisez le fichier, complétez-le, puis collez-le dans le format d'impression."
    );
    Ok(())
}

fn ecrire(chemin: &Path, contenu: &[u8]) -> Result<(), Erreur> {
    fs::write(chemin, contenu).map_err(|e| Erreur::Fichier(format!("{} : {e}", chemin.display())))
}

fn avertir_si_non_validee(regles: &Regles) {
    if !regles.est_validee() {
        println!(
            "⚠ Liste des mentions « {} » non validée : à faire relire (dokos contrat regles --exporter).\n",
            regles.version
        );
    }
}

fn afficher_problemes(problemes: &[Probleme]) {
    if problemes.is_empty() {
        println!("Données du contrat : aucun problème.");
        return;
    }
    println!("Données du contrat :");
    for probleme in problemes {
        println!("  {probleme}");
    }
}

pub fn afficher_rapport(rapport: &RapportGabarit) {
    println!("\nMentions obligatoires du format :");
    for mention in &rapport.mentions {
        let (signe, detail) = match &mention.etat {
            EtatMention::Presente => ("✓", String::new()),
            EtatMention::Manquante => ("✗", " — manquante".to_owned()),
            EtatMention::ChampsAbsentsDeDokos(champs) => {
                ("✗", format!(" — champ à créer dans Dokos : {}", champs.join(", ")))
            }
        };
        println!("  {signe} {}{detail} ({})", mention.libelle, mention.reference);
    }
    for interdit in &rapport.interdits {
        println!("  ✗ Sécurité : {interdit}");
    }
    for non_verifie in &rapport.non_verifies {
        println!("  ? Non vérifié : {non_verifie}");
    }
    if rapport.a_completer > 0 {
        println!("  ✗ Textes « [à compléter] » restants : {}", rapport.a_completer);
    }
    if !rapport.champs_inconnus.is_empty() {
        println!("  ? Champs affichés absents de Dokos : {}", rapport.champs_inconnus.join(", "));
    }
    println!(
        "{}",
        if rapport.conforme() { "Format conforme à la liste des mentions." } else { "Format NON conforme." }
    );
}
