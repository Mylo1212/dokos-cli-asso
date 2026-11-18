//! Dokos Asso : devis, commandes, factures, paiements, catalogue,
//! imports de tableaux, exports Excel et suivi.
//!
//! L'interface ne fait que proposer des demandes ; l'assistant IA (Open
//! WebUI) ne fait que pré-remplir les formulaires ; Rust valide tout et
//! construit seul les requêtes vers Dokos. Aucune permission Tauri n'est
//! accordée à la fenêtre (capacité vide) : tout passe par les commandes
//! de [`commandes`].

// Pas de console en plus de la fenêtre sous Windows, en version finale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commandes;
mod connexions;
mod erreurs;
mod services;
mod services_catalogue;
mod services_ia;
mod services_import;
mod services_listes;
mod services_modeles;
mod services_suivi;
mod vues;

fn main() {
    if let Err(e) = dokos_config::journal::initialiser_journal("appli") {
        eprintln!("journal indisponible : {e}");
    }
    let resultat = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commandes::etat_config,
            commandes::enregistrer_config,
            commandes::enregistrer_profil,
            commandes::tester_profil,
            commandes::etat_connexions,
            commandes::diagnostic,
            commandes::liste_clients,
            commandes::liste_articles,
            commandes::chercher_clients,
            commandes::chercher_articles,
            commandes::verifier_vente,
            commandes::creer_vente,
            commandes::modeles_vente,
            commandes::modele_vente,
            commandes::verifier_vente_modele,
            commandes::creer_vente_modele,
            commandes::catalogue,
            commandes::articles_du_groupe,
            commandes::prix_indicatifs,
            commandes::creer_article,
            commandes::activer_article,
            commandes::deplacer_article,
            commandes::creer_groupe_articles,
            commandes::mes_brouillons,
            commandes::transformer_document,
            commandes::factures_en_retard,
            commandes::references_export,
            commandes::listes_exportables,
            commandes::journaux_comptables,
            commandes::exporter_grand_livre,
            commandes::assistant_liste,
            commandes::exporter_liste,
            commandes::cas_import,
            commandes::generer_modele,
            commandes::choisir_fichier,
            commandes::analyser_import,
            commandes::creer_import,
            commandes::etat_ia,
            commandes::enregistrer_ia,
            commandes::tester_ia,
            commandes::discuter,
            commandes::assistant_vente,
            commandes::ouvrir_lien,
            commandes::ouvrir_fichier,
        ])
        .run(tauri::generate_context!());
    if let Err(erreur) = resultat {
        tracing::error!("démarrage impossible : {erreur}");
        eprintln!("Dokos Asso n'a pas pu démarrer : {erreur}");
        std::process::exit(1);
    }
}
