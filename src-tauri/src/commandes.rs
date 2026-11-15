//! Commandes appelées par l'interface. Chacune prend la connexion du profil
//! concerné dans le cache ([`crate::connexions`]) et délègue aux services.
//!
//! Les appels à Dokos sont bloquants : ils passent sur un fil dédié pour ne
//! pas figer la fenêtre. Les créations sont notées dans le journal des actions.

use std::path::{Path, PathBuf};

use dokos_config::journal::{Action, journaliser_action};
use dokos_config::{Config, Profil};
use dokos_core::date::Date;
use dokos_core::http::valider_url_base;
use dokos_core::vente::SiDoublon;
use dokos_core::{Dokos, Secret};

use crate::connexions;
use crate::erreurs::{ErreurVue, erreur};
use crate::services::{self, Resultat};
use crate::vues::*;
use crate::{services_catalogue, services_ia, services_import, services_listes, services_modeles, services_suivi};

async fn bloquant<T, F>(f: F) -> Resultat<T>
where
    T: Send + 'static,
    F: FnOnce() -> Resultat<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| ErreurVue::new("autre", format!("tâche interrompue : {e}")))?
}

fn si_doublon(autoriser: bool) -> SiDoublon {
    if autoriser { SiDoublon::CreerQuandMeme } else { SiDoublon::Reutiliser }
}

fn profil(nom: &str) -> Resultat<Profil> {
    nom.parse().map_err(ErreurVue::validation)
}

fn noter(profil: Profil, action: &str, doctype: &str, document: &str, resultat: &str, detail: &str) {
    journaliser_action(&Action { profil: profil.nom(), action, doctype, document, resultat, detail });
}

fn aujourdhui(texte: &str) -> Resultat<Date> {
    if texte.trim().is_empty() { Ok(Date::aujourdhui()) } else { texte.parse().map_err(erreur) }
}

// --- Réglages -------------------------------------------------------------

#[tauri::command]
pub async fn etat_config() -> Resultat<EtatConfig> {
    bloquant(|| {
        let config = dokos_config::lire().map_err(erreur)?;
        let chemin = dokos_config::chemin().map(|c| c.display().to_string()).unwrap_or_default();
        let profils = Profil::TOUS
            .iter()
            .map(|p| ProfilVue {
                profil: p.nom().to_owned(),
                libelle: p.libelle().to_owned(),
                cle: config.as_ref().and_then(|c| c.cles.get(*p)).map(str::to_owned),
                roles: p.roles_conseilles().to_owned(),
            })
            .collect();
        Ok(EtatConfig {
            configure: config.is_some(),
            url: config.as_ref().map(|c| c.url.clone()).unwrap_or_default(),
            autoriser_http: config.as_ref().is_some_and(|c| c.autoriser_http),
            societe: config.as_ref().and_then(|c| c.societe.clone()).unwrap_or_default(),
            profils,
            chemin_config: chemin,
        })
    })
    .await
}

#[tauri::command]
pub async fn enregistrer_config(url: String, autoriser_http: bool, societe: String) -> Resultat<()> {
    bloquant(move || {
        let url = valider_url_base(&url, autoriser_http).map_err(erreur)?;
        let mut config =
            dokos_config::lire().map_err(erreur)?.unwrap_or_else(|| Config::nouvelle(&url, autoriser_http));
        config.url = url;
        config.autoriser_http = autoriser_http;
        config.societe = Some(societe.trim().to_owned()).filter(|s| !s.is_empty());
        dokos_config::ecrire(&config).map_err(erreur)?;
        connexions::invalider();
        Ok(())
    })
    .await
}

/// Teste la clé et le secret du profil, puis les enregistre. Renvoie l'utilisateur Dokos.
#[tauri::command]
pub async fn enregistrer_profil(profil_nom: String, cle: String, secret: String) -> Resultat<String> {
    bloquant(move || {
        let profil = profil(&profil_nom)?;
        let mut config = dokos_config::lire()
            .map_err(erreur)?
            .ok_or_else(|| ErreurVue::configuration("enregistrez d'abord l'adresse de Dokos"))?;
        let ancienne_cle = config.cles.get(profil).map(str::to_owned);
        config.cles.set(profil, Some(cle));
        let secret = secret.trim().to_owned();
        let transport = config.transport(profil, &Secret::new(secret.clone())).map_err(erreur)?;
        let utilisateur = Dokos::new(transport).utilisateur_connecte().map_err(erreur)?;
        let cle = config.cle(profil).map_err(erreur)?.to_owned();
        if let Some(ancienne) = ancienne_cle
            && ancienne != cle
        {
            dokos_config::oublier_secret(profil, &ancienne).map_err(erreur)?;
        }
        dokos_config::enregistrer_secret(profil, &cle, &secret).map_err(erreur)?;
        dokos_config::ecrire(&config).map_err(erreur)?;
        connexions::invalider();
        Ok(utilisateur)
    })
    .await
}

#[tauri::command]
pub async fn tester_profil(profil_nom: String) -> Resultat<String> {
    bloquant(move || {
        connexions::invalider();
        let (_, dokos) = connexions::connecter(profil(&profil_nom)?)?;
        dokos.utilisateur_connecte().map_err(erreur)
    })
    .await
}

/// État de la connexion de chaque profil, les trois de front. Appelée au
/// démarrage : les connexions HTTPS sont ainsi déjà ouvertes à la première
/// recherche, et une clé qui ne marche plus se voit tout de suite.
#[tauri::command]
pub async fn etat_connexions() -> Resultat<Vec<ConnexionVue>> {
    bloquant(|| {
        let config = dokos_config::lire().map_err(erreur)?;
        let configure = |p: Profil| config.as_ref().is_some_and(|c| c.cles.get(p).is_some());
        Ok(std::thread::scope(|portee| {
            let taches: Vec<_> = Profil::TOUS
                .into_iter()
                .map(|p| (p, configure(p)))
                .map(|(p, configure)| (p, portee.spawn(move || etat_connexion(p, configure))))
                .collect();
            taches
                .into_iter()
                .map(|(p, tache)| {
                    tache.join().unwrap_or_else(|_| {
                        vue_connexion(p, "erreur", "", &ErreurVue::new("autre", "contrôle interrompu"))
                    })
                })
                .collect()
        }))
    })
    .await
}

fn etat_connexion(profil: Profil, configure: bool) -> ConnexionVue {
    let aucune = ErreurVue::new("", "");
    if !configure {
        return vue_connexion(profil, "non_configure", "", &aucune);
    }
    match connexions::connecter(profil).and_then(|(_, dokos)| dokos.utilisateur_connecte().map_err(erreur)) {
        Ok(utilisateur) => vue_connexion(profil, "ok", &utilisateur, &aucune),
        Err(e) => vue_connexion(profil, "erreur", "", &e),
    }
}

fn vue_connexion(profil: Profil, etat: &str, utilisateur: &str, e: &ErreurVue) -> ConnexionVue {
    ConnexionVue {
        profil: profil.nom().to_owned(),
        libelle: profil.libelle().to_owned(),
        etat: etat.to_owned(),
        utilisateur: utilisateur.to_owned(),
        genre: e.genre.clone(),
        message: e.message.clone(),
    }
}

#[tauri::command]
pub async fn diagnostic() -> Resultat<DiagnosticVue> {
    bloquant(|| {
        Ok(DiagnosticVue {
            texte: dokos_config::journal::diagnostic("appli"),
            dossier_journaux: dokos_config::journal::dossier_journaux()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
            dossier_documents: dossier_documents().map(|d| d.display().to_string()).unwrap_or_default(),
        })
    })
    .await
}

// --- Ventes -------------------------------------------------------------

/// Tous les clients actifs, pour les listes déroulantes.
#[tauri::command]
pub async fn liste_clients() -> Resultat<Vec<ClientVue>> {
    bloquant(|| services::lister_les_clients(&connexions::connecter(Profil::Ventes)?.1)).await
}

/// Tous les articles vendables, ou ceux d'un groupe et de ses sous-groupes, pour les listes déroulantes.
#[tauri::command]
pub async fn liste_articles(groupe: Option<String>) -> Resultat<Vec<ArticleVue>> {
    bloquant(move || services::lister_les_articles(&connexions::connecter(Profil::Ventes)?.1, groupe.as_deref())).await
}

#[tauri::command]
pub async fn chercher_clients(recherche: String) -> Resultat<Vec<ClientVue>> {
    bloquant(move || services::chercher_clients(&connexions::connecter(Profil::Ventes)?.1, &recherche)).await
}

#[tauri::command]
pub async fn chercher_articles(recherche: String, groupe: Option<String>) -> Resultat<Vec<ArticleVue>> {
    bloquant(move || {
        services::chercher_articles(&connexions::connecter(Profil::Ventes)?.1, &recherche, groupe.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn verifier_vente(demande: DemandeVente) -> Resultat<ApercuVente> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        services::apercu_vente(&dokos, &config.url_base(), &demande, config.societe.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn creer_vente(demande: DemandeVente, autoriser_doublon: bool) -> Resultat<ResultatVente> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        let doctype = services::type_vente(&demande.type_vente)?.doctype();
        let resultat = services::creer_vente(
            &dokos,
            &config.url_base(),
            &demande,
            config.societe.as_deref(),
            si_doublon(autoriser_doublon),
        );
        match &resultat {
            Ok(r) => noter(
                Profil::Ventes,
                "creation",
                doctype,
                &r.document.id,
                if r.cree { "cree" } else { "existant" },
                &format!("client {} ; total {:.2} {}", demande.client, r.document.total, r.document.devise),
            ),
            Err(e) => noter(Profil::Ventes, "creation", doctype, "", "erreur", &e.message),
        }
        resultat
    })
    .await
}

// --- Ventes à partir d'un modèle « Template » ---------------------------------

#[tauri::command]
pub async fn modeles_vente(type_vente: String) -> Resultat<Vec<ResumeModeleVue>> {
    bloquant(move || services_modeles::modeles(&connexions::connecter(Profil::Ventes)?.1, &type_vente)).await
}

#[tauri::command]
pub async fn modele_vente(type_vente: String, id: String) -> Resultat<ModeleVenteVue> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        services_modeles::modele(&dokos, &config.url_base(), &type_vente, &id)
    })
    .await
}

#[tauri::command]
pub async fn verifier_vente_modele(demande: DemandeVenteModele) -> Resultat<ApercuVente> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        services_modeles::apercu(&dokos, &config.url_base(), &demande, Date::aujourdhui())
    })
    .await
}

#[tauri::command]
pub async fn creer_vente_modele(demande: DemandeVenteModele, autoriser_doublon: bool) -> Resultat<ResultatVente> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        let doctype = services::type_vente(&demande.type_vente)?.doctype();
        let resultat = services_modeles::creer(
            &dokos,
            &config.url_base(),
            &demande,
            si_doublon(autoriser_doublon),
            Date::aujourdhui(),
        );
        match &resultat {
            Ok(r) => noter(
                Profil::Ventes,
                "creation",
                doctype,
                &r.document.id,
                if r.cree { "cree" } else { "existant" },
                &format!(
                    "modèle {} ; client {} ; total {:.2} {}",
                    demande.modele, demande.client, r.document.total, r.document.devise
                ),
            ),
            Err(e) => noter(Profil::Ventes, "creation", doctype, "", "erreur", &e.message),
        }
        resultat
    })
    .await
}

// --- Catalogue -----------------------------------------------------------

#[tauri::command]
pub async fn catalogue() -> Resultat<CatalogueVue> {
    bloquant(|| services_catalogue::catalogue(&connexions::connecter(Profil::Ventes)?.1)).await
}

#[tauri::command]
pub async fn articles_du_groupe(groupe: String, recherche: Option<String>) -> Resultat<Vec<ArticleVue>> {
    bloquant(move || {
        services_catalogue::articles_d_un_groupe(
            &connexions::connecter(Profil::Ventes)?.1,
            &groupe,
            recherche.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn prix_indicatifs(codes: Vec<String>) -> Resultat<Vec<PrixVue>> {
    bloquant(move || services_catalogue::prix(&connexions::connecter(Profil::Ventes)?.1, &codes)).await
}

#[tauri::command]
pub async fn creer_article(demande: DemandeArticle, autoriser_doublon: bool) -> Resultat<ResultatArticle> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        let resultat =
            services_catalogue::creer_un_article(&dokos, &config.url_base(), &demande, si_doublon(autoriser_doublon));
        if let Ok(r) = &resultat {
            noter(Profil::Ventes, "creation", "Item", &r.code, if r.cree { "cree" } else { "existant" }, &r.libelle);
        }
        resultat
    })
    .await
}

#[tauri::command]
pub async fn activer_article(code: String) -> Resultat<ResultatArticle> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        let resultat = services_catalogue::activer_un_article(&dokos, &config.url_base(), &code);
        if resultat.is_ok() {
            noter(Profil::Ventes, "activation", "Item", &code, "ok", "");
        }
        resultat
    })
    .await
}

#[tauri::command]
pub async fn deplacer_article(code: String, groupe: String) -> Resultat<ResultatArticle> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        let resultat = services_catalogue::deplacer_un_article(&dokos, &config.url_base(), &code, &groupe);
        if resultat.is_ok() {
            noter(Profil::Ventes, "deplacement", "Item", &code, "ok", &groupe);
        }
        resultat
    })
    .await
}

#[tauri::command]
pub async fn creer_groupe_articles(nom: String, parent: String, conteneur: bool) -> Resultat<ResultatGroupe> {
    bloquant(move || {
        let (_, dokos) = connexions::connecter(Profil::Ventes)?;
        let resultat = services_catalogue::creer_un_groupe(&dokos, &nom, &parent, conteneur);
        if let Ok(r) = &resultat {
            noter(Profil::Ventes, "creation", "Item Group", &r.id, if r.cree { "cree" } else { "existant" }, &parent);
        }
        resultat
    })
    .await
}

// --- Mes brouillons, transformation, suivi ---------------------------------

#[tauri::command]
pub async fn mes_brouillons() -> Resultat<BrouillonsVue> {
    bloquant(|| {
        let mut vue = BrouillonsVue { ventes: Vec::new(), evenements: Vec::new(), avertissements: Vec::new() };
        match connexions::connecter(Profil::Ventes) {
            Ok((config, dokos)) => vue.ventes = services_suivi::brouillons_ventes(&dokos, &config.url_base())?,
            Err(e) => vue.avertissements.push(format!("ventes : {}", e.message)),
        }
        match connexions::connecter(Profil::Evenements) {
            Ok((config, dokos)) => vue.evenements = services_suivi::brouillons_evenements(&dokos, &config.url_base())?,
            Err(e) => vue.avertissements.push(format!("événements : {}", e.message)),
        }
        Ok(vue)
    })
    .await
}

#[tauri::command]
pub async fn transformer_document(
    genre: String,
    source: String,
    autoriser_doublon: bool,
) -> Resultat<ResultatTransformationVue> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        let transformation = services_suivi::transformation(&genre)?;
        let resultat = services_suivi::transformer_un_document(
            &dokos,
            &config.url_base(),
            transformation,
            &source,
            si_doublon(autoriser_doublon),
        );
        if let Ok(r) = &resultat {
            noter(
                Profil::Ventes,
                "transformation",
                transformation.cible(),
                &r.document.id,
                if r.cree { "cree" } else { "existant" },
                &source,
            );
        }
        resultat
    })
    .await
}

#[tauri::command]
pub async fn evenements_recents(aujourdhui_texte: String) -> Resultat<Vec<EvenementVue>> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Evenements)?;
        services_suivi::evenements(&dokos, &config.url_base(), aujourdhui(&aujourdhui_texte)?)
    })
    .await
}

#[tauri::command]
pub async fn inscrits_evenement(id: String) -> Resultat<InscritsVue> {
    bloquant(move || services_suivi::inscrits(&connexions::connecter(Profil::Evenements)?.1, &id)).await
}

/// Exporte les inscrits en `Documents\Dokos\inscrits-<événement>.xlsx`.
#[tauri::command]
pub async fn exporter_inscrits(id: String) -> Resultat<String> {
    bloquant(move || {
        let vue = services_suivi::inscrits(&connexions::connecter(Profil::Evenements)?.1, &id)?;
        let (entetes, lignes) = services_suivi::tableau_inscrits(&vue);
        let chemin = dossier_documents()?.join(format!("inscrits-{}.xlsx", dokos_config::nom_de_fichier(&id)));
        services_import::ecrire_tableau(&chemin, "Inscrits", &entetes, &lignes)?;
        Ok(chemin.display().to_string())
    })
    .await
}

#[tauri::command]
pub async fn alertes_rh(aujourdhui_texte: String) -> Resultat<AlertesRhVue> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Rh)?;
        services_suivi::alertes_rh(&dokos, &config.url_base(), aujourdhui(&aujourdhui_texte)?)
    })
    .await
}

#[tauri::command]
pub async fn factures_en_retard(aujourdhui_texte: String) -> Resultat<ImpayesVue> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Ventes)?;
        services_suivi::impayes(&dokos, &config.url_base(), aujourdhui(&aujourdhui_texte)?)
    })
    .await
}

// --- Import ------------------------------------------------------------------

#[tauri::command]
pub async fn cas_import() -> Resultat<Vec<CasVue>> {
    bloquant(|| Ok(services_import::cas_disponibles())).await
}

#[tauri::command]
pub async fn generer_modele(cas_nom: String, modele: Option<String>) -> Resultat<String> {
    bloquant(move || {
        let cas = services_import::cas(&cas_nom)?;
        let (config, dokos) = connexions::connecter(profil(cas.profil())?)?;
        let chemin = services_import::generer_un_modele(
            &dokos,
            cas,
            modele.as_deref(),
            &config.url_base(),
            config.societe.as_deref(),
            &dossier_documents()?,
            Date::aujourdhui(),
        )?;
        Ok(chemin.display().to_string())
    })
    .await
}

/// Boîte de dialogue native, côté Rust : la fenêtre n'a aucun accès au disque.
#[tauri::command]
pub async fn choisir_fichier() -> Resultat<Option<String>> {
    bloquant(|| {
        let dialogue = rfd::AsyncFileDialog::new()
            .set_title("Tableau à importer")
            .add_filter("Tableaux", &["xlsx", "xls", "ods", "csv"]);
        let choisi = tauri::async_runtime::block_on(dialogue.pick_file());
        Ok(choisi.map(|f| f.path().display().to_string()))
    })
    .await
}

#[tauri::command]
pub async fn analyser_import(cas_nom: String, chemin: String, choix: Vec<ChoixImport>) -> Resultat<AnalyseVue> {
    bloquant(move || {
        let cas = services_import::cas(&cas_nom)?;
        let (config, dokos) = connexions::connecter(profil(cas.profil())?)?;
        services_import::analyser_un_fichier(
            &dokos,
            cas,
            Path::new(&chemin),
            config.societe.as_deref(),
            &choix,
            Date::aujourdhui(),
        )
    })
    .await
}

#[tauri::command]
pub async fn creer_import(
    cas_nom: String,
    chemin: String,
    choix: Vec<ChoixImport>,
    seulement_prets: bool,
    autoriser_doublon: bool,
) -> Resultat<ResultatImportVue> {
    bloquant(move || {
        let cas = services_import::cas(&cas_nom)?;
        let profil = profil(cas.profil())?;
        let (config, dokos) = connexions::connecter(profil)?;
        let resultat = services_import::executer_un_import(
            &dokos,
            &config.url_base(),
            cas,
            Path::new(&chemin),
            config.societe.as_deref(),
            &choix,
            seulement_prets,
            si_doublon(autoriser_doublon),
            &dossier_documents()?,
            Date::aujourdhui(),
        );
        match &resultat {
            Ok(r) => {
                for d in &r.documents {
                    noter(profil, "import", cas.nom(), &d.id, &d.statut, &d.message);
                }
            }
            Err(e) => noter(profil, "import", cas.nom(), "", "erreur", &e.message),
        }
        resultat
    })
    .await
}

// --- Formulaires ------------------------------------------------------------

#[tauri::command]
pub async fn questions_formulaire() -> Resultat<Vec<ChampVue>> {
    bloquant(|| {
        let (_, dokos) = connexions::connecter(Profil::Evenements)?;
        // Chargement ou « Recharger les champs » : les champs ajoutés dans Dokos doivent apparaître.
        dokos.oublier_structures();
        services::questions_formulaire(&dokos)
    })
    .await
}

#[tauri::command]
pub async fn verifier_evenement(demande: DemandeEvenement) -> Resultat<ApercuEvenement> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Evenements)?;
        services::apercu_evenement(&dokos, &config.url_base(), &demande)
    })
    .await
}

#[tauri::command]
pub async fn creer_evenement(demande: DemandeEvenement, autoriser_doublon: bool) -> Resultat<ResultatEvenement> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Evenements)?;
        let resultat =
            services::creer_un_evenement(&dokos, &config.url_base(), &demande, si_doublon(autoriser_doublon));
        if let Ok(r) = &resultat {
            noter(
                Profil::Evenements,
                "creation",
                "Event",
                &r.evenement.id,
                if r.cree { "cree" } else { "existant" },
                &demande.titre,
            );
        }
        resultat
    })
    .await
}

#[tauri::command]
pub async fn publier_evenement(id: String) -> Resultat<PublicationVue> {
    bloquant(move || {
        let (config, dokos) = connexions::connecter(Profil::Evenements)?;
        let resultat = services::publier_un_evenement(&dokos, &config.url_base(), &id);
        if resultat.is_ok() {
            noter(Profil::Evenements, "publication", "Event", &id, "ok", "");
        }
        resultat
    })
    .await
}

fn dossier_documents() -> Resultat<PathBuf> {
    dokos_config::dossier_documents().map_err(erreur)
}

// --- Assistant IA (Open WebUI) -------------------------------------------

#[tauri::command]
pub async fn etat_ia() -> Resultat<EtatIa> {
    bloquant(|| {
        let ia = dokos_config::lire().map_err(erreur)?.map(|c| c.ia).unwrap_or_default();
        Ok(EtatIa {
            active: ia.active,
            url: ia.url,
            modele: ia.modele,
            bases_ventes: ia.bases.ventes.join(", "),
            bases_formulaires: ia.bases.formulaires.join(", "),
            cle_enregistree: dokos_config::cle_ia_enregistree(),
        })
    })
    .await
}

/// Enregistre les réglages de l'IA (et la clé si elle est donnée), puis teste.
/// Si le test échoue, les réglages restent enregistrés et le message le dit.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn enregistrer_ia(
    active: bool,
    url: String,
    modele: String,
    bases_ventes: String,
    bases_formulaires: String,
    cle: String,
) -> Resultat<String> {
    bloquant(move || {
        let (url, _) = assistant_core::adresse::valider_url_ia(&url).map_err(erreur)?;
        let mut config = dokos_config::lire()
            .map_err(erreur)?
            .ok_or_else(|| ErreurVue::configuration("enregistrez d'abord l'adresse de Dokos"))?;
        config.ia.active = active;
        config.ia.url = url;
        config.ia.modele = Some(modele.trim().to_owned()).filter(|m| !m.is_empty()).unwrap_or(config.ia.modele);
        config.ia.bases.ventes = dokos_config::liste_bases(&bases_ventes);
        config.ia.bases.formulaires = dokos_config::liste_bases(&bases_formulaires);
        dokos_config::ecrire(&config).map_err(erreur)?;
        if !cle.trim().is_empty() {
            dokos_config::enregistrer_cle_ia(&cle).map_err(erreur)?;
        }
        connexions::invalider();
        if !active {
            return Ok("Assistant IA désactivé : les zones Assistant sont masquées.".to_owned());
        }
        tester().map_err(|e| {
            ErreurVue::new(&e.genre, format!("Réglages enregistrés, mais le test a échoué : {}", e.message))
        })
    })
    .await
}

fn tester() -> Resultat<String> {
    let ia = connexions::connecter_ia()?;
    let (config, client) = &*ia;
    let modeles = client.modeles().map_err(erreur)?;
    if modeles.iter().any(|m| m == &config.modele) {
        Ok(format!("Open WebUI joignable, modèle « {} » disponible", config.modele))
    } else {
        Err(ErreurVue::new(
            "ia",
            format!(
                "Open WebUI joignable, mais le modèle « {} » est absent. Disponibles : {}",
                config.modele,
                if modeles.is_empty() { "aucun".to_owned() } else { modeles.join(", ") }
            ),
        ))
    }
}

#[tauri::command]
pub async fn tester_ia() -> Resultat<String> {
    bloquant(tester).await
}

/// Chatbot : qualifie la demande, indique l'onglet et la procédure. Ne
/// touche pas à Dokos. Les bases de connaissance de tous les domaines sont jointes.
#[tauri::command]
pub async fn discuter(messages: Vec<MessageChatVue>, onglet: String, aujourdhui: String) -> Resultat<ReponseChatVue> {
    bloquant(move || {
        let ia = connexions::connecter_ia()?;
        let b = &ia.0.bases;
        let mut bases: Vec<String> = Vec::new();
        for base in b.ventes.iter().chain(&b.formulaires) {
            if !bases.contains(base) {
                bases.push(base.clone());
            }
        }
        // Recherche de clients et d'articles, avec la clé Ventes, seulement si l'IA la demande.
        let base = connexions::config()?.url_base();
        let chercher = |genre, texte: &str| {
            let (_, dokos) = connexions::connecter(Profil::Ventes)
                .map_err(|e| assistant_core::ErreurIa::Demande(e.message.clone()))?;
            dokos_core::fiche::chercher_fiches(&dokos, genre, texte).map_err(assistant_core::ErreurIa::Dokos)
        };
        services_ia::chat(&ia.1, &bases, messages, &onglet, services_ia::aujourdhui(&aujourdhui)?, &base, chercher)
    })
    .await
}

#[tauri::command]
pub async fn assistant_vente(demande: String, aujourdhui: String) -> Resultat<PropositionVenteVue> {
    bloquant(move || {
        let (_, dokos) = connexions::connecter(Profil::Ventes)?;
        let ia = connexions::connecter_ia()?;
        services_ia::vente(&dokos, &ia.1, &ia.0.bases.ventes, &demande, services_ia::aujourdhui(&aujourdhui)?)
    })
    .await
}

#[tauri::command]
pub async fn assistant_evenement(demande: String, aujourdhui: String) -> Resultat<PropositionEvenementVue> {
    bloquant(move || {
        let (_, dokos) = connexions::connecter(Profil::Evenements)?;
        let ia = connexions::connecter_ia()?;
        services_ia::evenement(&dokos, &ia.1, &ia.0.bases.formulaires, &demande, services_ia::aujourdhui(&aujourdhui)?)
    })
    .await
}

// --- Export Excel ----------------------------------------------------------

/// Groupes et territoires que les filtres de la liste acceptent, pour les listes déroulantes.
#[tauri::command]
pub async fn references_export(liste: String) -> Resultat<ReferencesExportVue> {
    bloquant(move || {
        let liste: dokos_core::export::TypeListe = liste.parse().map_err(erreur)?;
        let (_, dokos) = connexions::connecter(services_listes::profil(liste))?;
        services_listes::references(&dokos, liste)
    })
    .await
}

#[tauri::command]
pub async fn listes_exportables() -> Resultat<Vec<ListeVue>> {
    bloquant(|| Ok(services_listes::catalogue())).await
}

/// L'IA choisit la liste et les filtres ; Rust les retrouve dans Dokos avec
/// le profil de la liste. Rien n'est lu au-delà de ces vérifications.
#[tauri::command]
pub async fn assistant_liste(demande: String, aujourdhui: String) -> Resultat<PropositionListeVue> {
    use assistant_core::listes::{completer, comprendre, sans_liste};
    bloquant(move || {
        let ia = connexions::connecter_ia()?;
        let comprise = comprendre(&ia.1, &[], &demande, services_ia::aujourdhui(&aujourdhui)?).map_err(erreur)?;
        let proposition = match comprise.liste {
            Some(liste) => {
                let (_, dokos) = connexions::connecter(services_listes::profil(liste))?;
                completer(&dokos, comprise).map_err(erreur)?
            }
            None => sans_liste(comprise),
        };
        Ok(services_listes::vue_proposition(proposition))
    })
    .await
}

/// Exporte la liste en `Documents\Dokos\listes\<liste>-<date>.xlsx`. Lecture seule.
#[tauri::command]
pub async fn exporter_liste(demande: DemandeListe, aujourdhui: String) -> Resultat<ResultatListeVue> {
    bloquant(move || {
        let liste: dokos_core::export::TypeListe = demande.liste.parse().map_err(erreur)?;
        let (_, dokos) = connexions::connecter(services_listes::profil(liste))?;
        services_listes::exporter(&dokos, demande, &dossier_documents()?, self::aujourdhui(&aujourdhui)?)
    })
    .await
}

// --- Ouverture de liens et de fichiers -------------------------------------

/// Ouvre une page de Dokos dans le navigateur. Seules les adresses du Dokos
/// configuré sont acceptées.
#[tauri::command]
pub async fn ouvrir_lien(url: String) -> Resultat<()> {
    bloquant(move || {
        let config = connexions::config()?;
        if !url.starts_with(&format!("{}/", config.url_base())) {
            return Err(ErreurVue::validation("lien refusé : il ne pointe pas vers votre Dokos"));
        }
        open::that_detached(&url).map_err(erreur)
    })
    .await
}

/// Ouvre un fichier ou un dossier de l'appli (PDF, proposition, modèle,
/// rapport, journaux), et seulement ceux-là.
#[tauri::command]
pub async fn ouvrir_fichier(chemin: String) -> Resultat<()> {
    bloquant(move || {
        let documents = dossier_documents()?;
        let journaux = dokos_config::journal::dossier_journaux().map_err(erreur)?;
        let cible = Path::new(&chemin);
        if !dans_dossier(&documents, cible) && !dans_dossier(&journaux, cible) {
            return Err(ErreurVue::validation(
                "fichier refusé : hors des dossiers de l'appli (Documents\\Dokos, journaux)",
            ));
        }
        open::that_detached(&chemin).map_err(erreur)
    })
    .await
}

/// Vrai si `chemin` est le dossier lui-même, ou un fichier ou dossier qu'il contient.
fn dans_dossier(dossier: &Path, chemin: &Path) -> bool {
    match (dossier.canonicalize(), chemin.canonicalize()) {
        (Ok(dossier), Ok(chemin)) => chemin.starts_with(&dossier) && (chemin.is_file() || chemin.is_dir()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichiers_hors_dossier_refuses() {
        let racine = std::env::temp_dir().join(format!("dokos-asso-test-{}", std::process::id()));
        let dossier = racine.join("Dokos");
        std::fs::create_dir_all(dossier.join("modeles")).expect("dossier");
        let dedans = dossier.join("facture.pdf");
        let dehors = racine.join("autre.pdf");
        std::fs::write(&dedans, b"%PDF").expect("fichier");
        std::fs::write(&dehors, b"%PDF").expect("fichier");
        assert!(dans_dossier(&dossier, &dedans));
        assert!(dans_dossier(&dossier, &dossier.join("modeles")));
        assert!(dans_dossier(&dossier, &dossier));
        assert!(!dans_dossier(&dossier, &dehors));
        assert!(!dans_dossier(&dossier, &dossier.join("..").join("autre.pdf")));
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn date_du_jour_ou_donnee() {
        assert_eq!(aujourdhui("2026-11-03").expect("date").to_string(), "2026-11-03");
        assert!(aujourdhui("").is_ok());
        assert!(aujourdhui("hier").is_err());
    }
}
