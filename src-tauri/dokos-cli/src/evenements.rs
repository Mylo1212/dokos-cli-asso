//! `dokos evenement …` : événements publics avec formulaire d'inscription.

use dokos_core::date::DateHeure;
use dokos_core::evenement::{
    CreationEvenement, NouvelEvenement, chercher_evenement_identique, creer_evenement, publier,
};
use dokos_core::formulaire::{DOCTYPE_INSCRIPTION, NouveauFormulaire, Question, questions_possibles};
use dokos_core::meta::lire_meta;
use dokos_core::vente::SiDoublon;
use dokos_core::{Dokos, Transport};

use crate::erreur::Erreur;

pub fn lister_questions<T: Transport>(dokos: &Dokos<T>) -> Result<(), Erreur> {
    let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION)?;
    let questions = questions_possibles(&meta);
    if questions.is_empty() {
        println!("Aucune question disponible en plus des champs de base (nom, prénom, e-mail, téléphone).");
        println!("Ajoutez des champs à « {DOCTYPE_INSCRIPTION} » dans Dokos (Personnaliser le formulaire).");
        return Ok(());
    }
    let largeur = questions.iter().map(|c| c.fieldname.chars().count()).max().unwrap_or(0);
    for champ in &questions {
        println!("{:<largeur$}  {} ({})", champ.fieldname, champ.libelle(), champ.fieldtype);
    }
    println!(
        "\nUtilisation : --question {}! (obligatoire) ou --question {} (facultative)",
        questions[0].fieldname, questions[0].fieldname
    );
    Ok(())
}

pub struct Demande<'a> {
    pub titre: &'a str,
    pub debut: DateHeure,
    pub fin: Option<DateHeure>,
    pub journee: bool,
    pub lieu: Option<&'a str>,
    pub description: Option<&'a str>,
    pub places: Option<u32>,
    pub annulation: bool,
    pub questions: &'a [Question],
    pub simulation: bool,
    pub si_doublon: SiDoublon,
}

pub fn creer<T: Transport>(dokos: &Dokos<T>, demande: &Demande<'_>) -> Result<(), Erreur> {
    let evenement = NouvelEvenement::new(demande.titre, demande.debut, demande.fin)?
        .journee_entiere(demande.journee)
        .avec_lieu(demande.lieu)
        .avec_description(demande.description)
        .avec_places(demande.places)?
        .avec_annulation(demande.annulation);
    let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION)?;
    let formulaire = NouveauFormulaire::new(&meta, demande.questions)?;

    println!("Événement  : {}", evenement.titre());
    println!("Début      : {}", evenement.debut());
    if let Some(fin) = evenement.fin() {
        println!("Fin        : {fin}");
    }
    println!("Places     : {}", evenement.places().map_or("sans limite".to_owned(), |p| p.to_string()));
    println!("Formulaire : {} (/{})", formulaire.titre(), formulaire.route());
    for champ in formulaire.champs() {
        println!("  {} {}", if champ.obligatoire { "*" } else { " " }, champ.label);
    }

    if demande.simulation {
        if demande.si_doublon == SiDoublon::Reutiliser
            && let Some(existant) = chercher_evenement_identique(dokos, &evenement)?
        {
            println!(
                "\nÉvénement identique déjà dans Dokos : {} — sans --simulation, rien ne serait créé.",
                existant.id
            );
            return Ok(());
        }
        let corps = serde_json::to_string_pretty(&evenement.corps("<formulaire>"))
            .map_err(|e| Erreur::Saisie(e.to_string()))?;
        println!("\nSimulation : rien n'a été écrit dans Dokos. Événement qui serait créé (non publié) :\n{corps}");
        return Ok(());
    }

    match creer_evenement(dokos, &evenement, &formulaire, demande.si_doublon)? {
        CreationEvenement::DejaExistant(existant) => {
            println!("\nÉvénement identique déjà dans Dokos : {}. Rien n'a été créé.", existant.id);
        }
        CreationEvenement::Cree { evenement, formulaire } => {
            let origine = if formulaire.reutilise { "réutilisé" } else { "créé" };
            println!("\nFormulaire {origine} : {} (/{})", formulaire.nom, formulaire.route);
            println!("Événement créé, NON publié : {}", evenement.id);
            println!("Relisez-le dans Dokos, puis : dokos evenement publier {}", evenement.id);
        }
    }
    Ok(())
}

pub fn publier_evenement<T: Transport>(dokos: &Dokos<T>, url_base: &str, id: &str) -> Result<(), Erreur> {
    let publication = publier(dokos, id)?;
    println!("Événement publié : {}", publication.evenement);
    println!("Page publique    : {url_base}/{}", publication.route_evenement);
    println!("Inscription      : {url_base}/{}", publication.route_inscription);
    Ok(())
}
