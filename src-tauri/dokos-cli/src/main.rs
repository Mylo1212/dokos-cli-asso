//! `dokos` : piloter Dokos depuis le terminal, par l'API REST de Frappe.

mod erreur;
mod evenements;
mod plus;
mod rh;
mod saisie;
mod ventes;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use conformite::regles::Contexte;
use dokos_config::{Config, Profil};
use dokos_core::date::{Date, DateHeure};
use dokos_core::employe::chercher_employes;
use dokos_core::formulaire::Question;
use dokos_core::recherche::{LIMITE_RECHERCHE, chercher_client};
use dokos_core::transformation::Transformation;
use dokos_core::vente::{SiDoublon, TypeVente};
use dokos_core::{Dokos, Secret};
use dokos_import::Cas;

use crate::erreur::Erreur;
use crate::saisie::LigneSaisie;

#[derive(Parser)]
#[command(name = "dokos", version, about = "Piloter Dokos depuis le terminal")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Enregistrer, tester ou oublier les connexions à Dokos (une par profil)
    #[command(subcommand)]
    Connexion(Connexion),
    /// Clients (profil ventes)
    #[command(subcommand)]
    Clients(Recherche),
    /// Articles vendables et catalogue (profil ventes)
    #[command(subcommand)]
    Articles(Articles),
    /// Écrit le modèle Excel d'un cas d'import (factures, commandes, devis, inscriptions, articles, employes, clients, evenements, devis-modele, commandes-modele, factures-modele)
    Modele {
        cas: String,
        /// Document « Template » à copier, pour les cas devis-modele, commandes-modele, factures-modele
        #[arg(long)]
        modele: Option<String>,
        #[arg(long)]
        sortie: Option<PathBuf>,
    },
    /// Importe un tableau rempli à partir d'un modèle : analyse, doublons, création tout ou rien
    Importer {
        cas: String,
        fichier: PathBuf,
        /// Analyse seulement, sans rien écrire
        #[arg(long)]
        simulation: bool,
        /// Crée les documents prêts même s'il reste des lignes à corriger
        #[arg(long)]
        seulement_prets: bool,
        #[arg(long)]
        autoriser_doublon: bool,
        /// Tranche une ambiguïté : champ=texte=identifiant (ex. client=Crest=CUST-2). À répéter.
        #[arg(long = "choix", value_parser = plus::choix)]
        choix: Vec<(String, String)>,
        /// Fichier résultat .xlsx (statut par ligne)
        #[arg(long)]
        rapport: Option<PathBuf>,
    },
    /// Documents créés par l'outil en attente de validation ou de publication
    Brouillons,
    /// Diagnostic à copier dans une demande d'aide (réglages sans secret, journal)
    Diagnostic,
    /// Devis client (profil ventes)
    #[command(subcommand)]
    Devis(Devis),
    /// Commandes client (profil ventes)
    #[command(subcommand)]
    Commande(Commande),
    /// Factures client (profil ventes)
    #[command(subcommand)]
    Facture(Facture),
    /// Événements publics et formulaires d'inscription (profil evenements)
    #[command(subcommand)]
    Evenement(Evenement),
    /// Employés (profil rh)
    #[command(subcommand)]
    Employe(Employe),
    /// Contrats de travail (profil rh)
    #[command(subcommand)]
    Contrat(Contrat),
    /// Formats d'impression des contrats : vérification et propositions (profil rh)
    #[command(subcommand)]
    Format(Format),
}

#[derive(Subcommand)]
enum Connexion {
    /// Teste la connexion d'un profil puis enregistre l'URL, la clé et le secret (demandé au clavier)
    Enregistrer {
        /// ventes, evenements ou rh
        #[arg(long)]
        profil: Profil,
        /// Racine du site, par exemple https://dokos.exemple.fr
        #[arg(long)]
        url: String,
        /// Clé API de l'utilisateur Dokos dédié à ce profil
        #[arg(long)]
        cle: String,
        /// Accepte une URL en http:// (réseau local uniquement)
        #[arg(long)]
        autoriser_http: bool,
        /// Société Dokos à utiliser s'il y en a plusieurs (obligatoire pour créer des employés)
        #[arg(long)]
        societe: Option<String>,
    },
    /// Vérifie que Dokos accepte les identifiants enregistrés
    Tester {
        /// Un seul profil ; par défaut, tous ceux configurés
        #[arg(long)]
        profil: Option<Profil>,
    },
    /// Supprime un profil, ou toute la configuration et tous les secrets
    Oublier {
        #[arg(long)]
        profil: Option<Profil>,
    },
}

#[derive(Subcommand)]
enum Recherche {
    /// Cherche par identifiant ou par nom
    Chercher {
        #[arg(required = true, num_args = 1..)]
        texte: Vec<String>,
    },
}

#[derive(Subcommand)]
enum Articles {
    /// Cherche par code ou par libellé
    Chercher {
        #[arg(required = true, num_args = 1..)]
        texte: Vec<String>,
        /// Limite la recherche à un groupe d'articles et à ses sous-groupes
        #[arg(long)]
        groupe: Option<String>,
    },
    /// Affiche l'arbre des groupes d'articles
    Arbre,
    /// Liste les articles vendables d'un groupe et de ses sous-groupes
    Lister {
        groupe: String,
        /// Texte à chercher dans le code ou le libellé
        texte: Option<String>,
    },
    /// Crée un article désactivé dans un groupe existant
    Creer {
        #[arg(long)]
        code: String,
        #[arg(long)]
        libelle: String,
        #[arg(long)]
        groupe: String,
        #[arg(long)]
        unite: String,
        /// Article non vendable (par défaut : vendable)
        #[arg(long)]
        non_vendable: bool,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        simulation: bool,
        #[arg(long)]
        autoriser_doublon: bool,
    },
    /// Active un article créé désactivé
    Activer { code: String },
}

#[derive(Args)]
struct Vente {
    /// Client : identifiant ou nom
    #[arg(long)]
    client: String,
    /// ARTICLE=QUANTITÉ, à répéter pour chaque ligne
    #[arg(long = "ligne", value_name = "ARTICLE=QTE", required = true, value_parser = saisie::ligne)]
    lignes: Vec<LigneSaisie>,
    /// Affiche ce qui serait envoyé, sans rien écrire dans Dokos
    #[arg(long)]
    simulation: bool,
    /// Crée le document même si un document identique existe déjà
    #[arg(long)]
    autoriser_doublon: bool,
}

#[derive(Subcommand)]
enum Devis {
    /// Crée un devis en brouillon (jamais validé)
    Brouillon {
        #[command(flatten)]
        vente: Vente,
        /// Date du devis : 2026-11-12 ou 12/11/2026
        #[arg(long, value_parser = saisie::date)]
        date: Date,
        #[arg(long, value_parser = saisie::date)]
        valable_jusqu_au: Option<Date>,
    },
    /// Crée en brouillon la commande correspondant à un devis validé
    Commander {
        id: String,
        #[arg(long)]
        autoriser_doublon: bool,
    },
}

#[derive(Subcommand)]
enum Commande {
    /// Crée une commande en brouillon (jamais validée)
    Brouillon {
        #[command(flatten)]
        vente: Vente,
        /// Date de livraison ; une ligne peut avoir la sienne : ARTICLE=QTE@DATE
        #[arg(long, value_parser = saisie::date)]
        livraison: Date,
    },
    /// Crée en brouillon la facture correspondant à une commande validée
    Facturer {
        id: String,
        #[arg(long)]
        autoriser_doublon: bool,
    },
}

#[derive(Subcommand)]
enum Facture {
    /// Crée une facture en brouillon (jamais validée)
    Brouillon {
        #[command(flatten)]
        vente: Vente,
        /// Date de facture
        #[arg(long, value_parser = saisie::date)]
        date: Date,
        #[arg(long, value_parser = saisie::date)]
        echeance: Option<Date>,
    },
    /// Factures validées dont l'échéance est dépassée (lecture seule)
    Retards,
}

#[derive(Subcommand)]
enum Evenement {
    /// Liste les questions possibles du formulaire (champs d'Event Registration)
    Questions,
    /// Crée l'événement et son formulaire d'inscription, NON publiés
    Creer {
        #[arg(long)]
        titre: String,
        /// Début : « 14/12/2026 14h00 » ou « 2026-12-14 14:00 »
        #[arg(long, value_parser = saisie::date_heure)]
        debut: DateHeure,
        #[arg(long, value_parser = saisie::date_heure)]
        fin: Option<DateHeure>,
        /// Événement sur la journée entière
        #[arg(long)]
        journee: bool,
        #[arg(long)]
        lieu: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Nombre de places ; sans limite par défaut
        #[arg(long)]
        places: Option<u32>,
        /// Permet aux inscrits d'annuler
        #[arg(long)]
        annulation: bool,
        /// Champ demandé : « age! » obligatoire, « allergies » facultatif. À répéter.
        #[arg(long = "question", value_parser = saisie::question)]
        questions: Vec<Question>,
        #[arg(long)]
        simulation: bool,
        #[arg(long)]
        autoriser_doublon: bool,
    },
    /// Publie l'événement et son formulaire sur le site
    Publier { id: String },
    /// Liste les inscrits d'un événement ; --sortie écrit un CSV
    Inscrits {
        id: String,
        #[arg(long)]
        sortie: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Employe {
    Chercher {
        #[arg(required = true, num_args = 1..)]
        texte: Vec<String>,
    },
    /// Crée un employé (société : celle de la configuration)
    Creer {
        #[arg(long)]
        prenom: String,
        #[arg(long)]
        nom: String,
        /// Un genre de Dokos (DocType Gender)
        #[arg(long)]
        genre: String,
        #[arg(long, value_parser = saisie::date)]
        naissance: Date,
        #[arg(long, value_parser = saisie::date)]
        entree: Date,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        poste: Option<String>,
        #[arg(long)]
        simulation: bool,
    },
}

#[derive(Subcommand)]
enum Contrat {
    /// Vérifie puis crée un contrat de travail ; rien n'est créé s'il y a des erreurs
    Creer {
        /// Identifiant de l'employé (HR-EMP-…)
        #[arg(long)]
        employe: String,
        /// Type de contrat Dokos (CDI, CDD…)
        #[arg(long = "type")]
        type_contrat: String,
        /// Poste (DocType Designation)
        #[arg(long)]
        poste: String,
        #[arg(long, value_parser = saisie::date)]
        entree: Date,
        #[arg(long, value_parser = saisie::date)]
        fin: Option<Date>,
        /// Heures par semaine
        #[arg(long, value_parser = saisie::heures)]
        heures: Option<f64>,
        /// Heures du lundi au dimanche : 6/6/6/6/0/0/0
        #[arg(long, value_parser = saisie::repartition)]
        repartition: Option<[f64; 7]>,
        /// Champ personnalisé : remuneration_brute=2100, à répéter
        #[arg(long = "champ", value_name = "CHAMP=VALEUR", value_parser = saisie::champ)]
        champs: Vec<(String, String)>,
        /// Format d'impression à vérifier avec le contrat
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        simulation: bool,
        #[arg(long)]
        autoriser_doublon: bool,
    },
    /// Vérifie un contrat existant, et son format d'impression s'il est donné
    Verifier {
        id: String,
        #[arg(long)]
        format: Option<String>,
    },
    /// Télécharge le PDF d'un contrat
    Pdf {
        id: String,
        #[arg(long)]
        format: String,
        #[arg(long)]
        sortie: Option<PathBuf>,
    },
    /// Liste des mentions obligatoires utilisée
    Regles {
        /// Copie la liste livrée dans le dossier de configuration pour l'adapter
        #[arg(long)]
        exporter: bool,
    },
    /// Fins de contrat proches, CDD sans fin, contrats récents (lecture seule)
    Alertes,
}

#[derive(Args)]
struct ContexteFormat {
    /// Nom du format d'impression
    format: String,
    /// Type de contrat imprimé par ce format (CDI, CDD…)
    #[arg(long = "type")]
    type_contrat: String,
    /// Vérifie aussi les mentions du temps partiel
    #[arg(long)]
    temps_partiel: bool,
}

#[derive(Subcommand)]
enum Format {
    /// Liste les formats d'impression des contrats
    Lister,
    /// Vérifie les mentions obligatoires et la sécurité d'un format
    Verifier {
        #[command(flatten)]
        contexte: ContexteFormat,
    },
    /// Écrit un gabarit corrigé dans un fichier, sans rien modifier dans Dokos
    Proposer {
        #[command(flatten)]
        contexte: ContexteFormat,
        #[arg(long)]
        sortie: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let _ = dokos_config::journal::initialiser_journal("cli");
    match executer(Cli::parse().action) {
        Ok(()) => ExitCode::SUCCESS,
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            ExitCode::FAILURE
        }
    }
}

fn si_doublon(autoriser: bool) -> SiDoublon {
    if autoriser { SiDoublon::CreerQuandMeme } else { SiDoublon::Reutiliser }
}

fn executer(action: Action) -> Result<(), Erreur> {
    match action {
        Action::Connexion(connexion) => gerer_connexion(connexion),
        Action::Clients(Recherche::Chercher { texte }) => {
            let (_, dokos) = dokos_config::connecter(Profil::Ventes)?;
            let clients = chercher_client(&dokos, &texte.join(" "))?;
            afficher_resultats(clients.iter().map(|c| (c.id.as_str(), c.nom.as_str())));
            Ok(())
        }
        Action::Articles(articles) => gerer_articles(articles),
        Action::Modele { cas, modele, sortie } => {
            let cas = cas_import(&cas)?;
            let (config, dokos) = dokos_config::connecter(cas.profil().parse().map_err(Erreur::Saisie)?)?;
            plus::generer_un_modele(
                &dokos,
                cas,
                modele.as_deref(),
                &config.url_base(),
                config.societe.as_deref(),
                sortie,
            )
        }
        Action::Importer { cas, fichier, simulation, seulement_prets, autoriser_doublon, choix, rapport } => {
            let cas = cas_import(&cas)?;
            let (config, dokos) = dokos_config::connecter(cas.profil().parse().map_err(Erreur::Saisie)?)?;
            plus::importer(
                &dokos,
                config.societe.as_deref(),
                &plus::DemandeImport {
                    cas,
                    fichier: &fichier,
                    choix,
                    simulation,
                    seulement_prets,
                    si_doublon: si_doublon(autoriser_doublon),
                    rapport,
                },
            )
        }
        Action::Brouillons => {
            let ventes = dokos_config::connecter(Profil::Ventes).ok().map(|(_, d)| d);
            let evenements = dokos_config::connecter(Profil::Evenements).ok().map(|(_, d)| d);
            if ventes.is_none() && evenements.is_none() {
                return Err(Erreur::Saisie("aucun profil ventes ni evenements configuré".to_owned()));
            }
            plus::afficher_brouillons(ventes.as_ref(), evenements.as_ref())
        }
        Action::Diagnostic => {
            print!("{}", dokos_config::journal::diagnostic("cli"));
            Ok(())
        }
        Action::Devis(Devis::Brouillon { vente, date, valable_jusqu_au }) => {
            vendre(TypeVente::Devis, &vente, date, valable_jusqu_au)
        }
        Action::Devis(Devis::Commander { id, autoriser_doublon }) => {
            let (_, dokos) = dokos_config::connecter(Profil::Ventes)?;
            plus::transformer_document(&dokos, Transformation::DevisEnCommande, &id, si_doublon(autoriser_doublon))
        }
        Action::Commande(Commande::Brouillon { vente, livraison }) => {
            vendre(TypeVente::Commande, &vente, livraison, None)
        }
        Action::Commande(Commande::Facturer { id, autoriser_doublon }) => {
            let (_, dokos) = dokos_config::connecter(Profil::Ventes)?;
            plus::transformer_document(&dokos, Transformation::CommandeEnFacture, &id, si_doublon(autoriser_doublon))
        }
        Action::Facture(Facture::Brouillon { vente, date, echeance }) => {
            vendre(TypeVente::Facture, &vente, date, echeance)
        }
        Action::Facture(Facture::Retards) => {
            let (_, dokos) = dokos_config::connecter(Profil::Ventes)?;
            plus::afficher_retards(&dokos)
        }
        Action::Evenement(evenement) => gerer_evenement(evenement),
        Action::Employe(employe) => gerer_employe(employe),
        Action::Contrat(contrat) => gerer_contrat(contrat),
        Action::Format(format) => gerer_format(format),
    }
}

fn cas_import(nom: &str) -> Result<Cas, Erreur> {
    Cas::depuis_nom(nom).ok_or_else(|| {
        Erreur::Saisie(format!(
            "cas d'import inconnu : « {nom} » (attendu : {})",
            Cas::TOUS.iter().map(|c| c.nom()).collect::<Vec<_>>().join(", ")
        ))
    })
}

fn gerer_articles(articles: Articles) -> Result<(), Erreur> {
    let (_, dokos) = dokos_config::connecter(Profil::Ventes)?;
    match articles {
        Articles::Chercher { texte, groupe } => {
            let articles = dokos_core::recherche::chercher_article_dans(&dokos, &texte.join(" "), groupe.as_deref())?;
            afficher_resultats(articles.iter().map(|a| (a.code.as_str(), a.libelle.as_str())));
            Ok(())
        }
        Articles::Arbre => plus::afficher_arbre(&dokos),
        Articles::Lister { groupe, texte } => plus::lister_articles(&dokos, &groupe, texte.as_deref()),
        Articles::Creer { code, libelle, groupe, unite, non_vendable, description, simulation, autoriser_doublon } => {
            plus::creer_un_article(
                &dokos,
                &plus::DemandeArticle {
                    code: &code,
                    libelle: &libelle,
                    groupe: &groupe,
                    unite: &unite,
                    vendable: !non_vendable,
                    description: description.as_deref(),
                    simulation,
                    si_doublon: si_doublon(autoriser_doublon),
                },
            )
        }
        Articles::Activer { code } => plus::activer_un_article(&dokos, &code),
    }
}

fn vendre(type_vente: TypeVente, vente: &Vente, date: Date, echeance: Option<Date>) -> Result<(), Erreur> {
    let (config, dokos) = dokos_config::connecter(Profil::Ventes)?;
    if type_vente == TypeVente::Commande && date < Date::aujourdhui() {
        println!(
            "Attention : livraison le {date}, avant aujourd'hui. Dokos refuse une livraison antérieure à la date de la commande ; \
             pour une régularisation, saisissez-la directement dans Dokos."
        );
    }
    let demande = ventes::Demande {
        type_vente,
        client: &vente.client,
        date,
        echeance,
        lignes: &vente.lignes,
        simulation: vente.simulation,
        si_doublon: si_doublon(vente.autoriser_doublon),
    };
    ventes::brouillon(&dokos, config.societe.as_deref(), &demande)
}

fn gerer_connexion(connexion: Connexion) -> Result<(), Erreur> {
    match connexion {
        Connexion::Enregistrer { profil, url, cle, autoriser_http, societe } => {
            enregistrer_connexion(profil, url, cle, autoriser_http, societe)
        }
        Connexion::Tester { profil } => {
            let config = dokos_config::charger()?;
            let profils: Vec<Profil> = match profil {
                Some(p) => vec![p],
                None => Profil::TOUS.into_iter().filter(|p| config.cles.get(*p).is_some()).collect(),
            };
            for profil in profils {
                let (config, dokos) = dokos_config::connecter(profil)?;
                println!(
                    "{:<11} {} — connecté en tant que {}",
                    profil.libelle(),
                    config.url,
                    dokos.utilisateur_connecte()?
                );
            }
            Ok(())
        }
        Connexion::Oublier { profil } => {
            let Some(mut config) = dokos_config::lire()? else {
                println!("Aucune connexion enregistrée.");
                return Ok(());
            };
            let profils: Vec<Profil> = profil.map_or(Profil::TOUS.to_vec(), |p| vec![p]);
            for profil in &profils {
                if let Some(cle) = config.cles.get(*profil).map(str::to_owned) {
                    dokos_config::oublier_secret(*profil, &cle)?;
                }
                config.cles.set(*profil, None);
            }
            if Profil::TOUS.iter().all(|p| config.cles.get(*p).is_none()) {
                dokos_config::supprimer()?;
                println!("Toutes les connexions sont oubliées.");
            } else {
                dokos_config::ecrire(&config)?;
                println!("Profil oublié.");
            }
            Ok(())
        }
    }
}

fn enregistrer_connexion(
    profil: Profil,
    url: String,
    cle: String,
    autoriser_http: bool,
    societe: Option<String>,
) -> Result<(), Erreur> {
    let ancienne = dokos_config::lire()?;
    let mut nouvelle = ancienne.clone().unwrap_or_else(|| Config::nouvelle(&url, autoriser_http));
    nouvelle.url = url;
    nouvelle.autoriser_http = autoriser_http;
    if societe.is_some() {
        nouvelle.societe = societe.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    }
    nouvelle.cles.set(profil, Some(cle));
    // Vérifie l'URL et la clé avant de demander le secret.
    nouvelle.transport(profil, &Secret::new("verification"))?;

    let secret = rpassword::prompt_password(format!("Secret API du profil « {profil} » (saisie masquée) : "))
        .map_err(|e| Erreur::Saisie(format!("lecture du secret : {e}")))?;
    let secret = secret.trim().to_owned();
    let dokos = Dokos::new(nouvelle.transport(profil, &Secret::new(secret.clone()))?);
    let utilisateur = dokos.utilisateur_connecte()?;

    let cle = nouvelle.cle(profil)?.to_owned();
    if let Some(ancienne_cle) = ancienne.as_ref().and_then(|c| c.cles.get(profil))
        && ancienne_cle != cle
    {
        dokos_config::oublier_secret(profil, ancienne_cle)?;
    }
    dokos_config::enregistrer_secret(profil, &cle, &secret)?;
    let chemin = dokos_config::ecrire(&nouvelle)?;
    println!("Profil « {profil} » : connecté en tant que {utilisateur}.");
    println!("Rôles conseillés pour cet utilisateur : {}.", profil.roles_conseilles());
    println!("Configuration : {}", chemin.display());
    println!("Secret : Gestionnaire d'identification, service « {} »", dokos_config::SERVICE_TROUSSEAU);
    Ok(())
}

fn gerer_evenement(evenement: Evenement) -> Result<(), Erreur> {
    let (config, dokos) = dokos_config::connecter(Profil::Evenements)?;
    match evenement {
        Evenement::Questions => evenements::lister_questions(&dokos),
        Evenement::Creer {
            titre,
            debut,
            fin,
            journee,
            lieu,
            description,
            places,
            annulation,
            questions,
            simulation,
            autoriser_doublon,
        } => evenements::creer(
            &dokos,
            &evenements::Demande {
                titre: &titre,
                debut,
                fin,
                journee,
                lieu: lieu.as_deref(),
                description: description.as_deref(),
                places,
                annulation,
                questions: &questions,
                simulation,
                si_doublon: si_doublon(autoriser_doublon),
            },
        ),
        Evenement::Publier { id } => evenements::publier_evenement(&dokos, &config.url_base(), &id),
        Evenement::Inscrits { id, sortie } => plus::afficher_inscrits(&dokos, &id, sortie),
    }
}

fn gerer_employe(employe: Employe) -> Result<(), Erreur> {
    let (config, dokos) = dokos_config::connecter(Profil::Rh)?;
    match employe {
        Employe::Chercher { texte } => {
            let employes = chercher_employes(&dokos, &texte.join(" "))?;
            afficher_resultats(employes.iter().map(|e| (e.id.as_str(), e.nom_complet.as_str())));
            Ok(())
        }
        Employe::Creer { prenom, nom, genre, naissance, entree, email, poste, simulation } => rh::creer_un_employe(
            &dokos,
            config.societe.as_deref(),
            &rh::DemandeEmploye {
                prenom: &prenom,
                nom: &nom,
                genre: &genre,
                naissance,
                entree,
                email: email.as_deref(),
                poste: poste.as_deref(),
                simulation,
            },
        ),
    }
}

fn gerer_contrat(contrat: Contrat) -> Result<(), Erreur> {
    if let Contrat::Regles { exporter } = contrat {
        if exporter {
            println!("Liste exportée : {}", dokos_config::exporter_regles()?.display());
            println!("Faites-la relire, puis renseignez « valide_par » et « valide_le ».");
            return Ok(());
        }
        let (regles, chemin) = dokos_config::charger_regles()?;
        let source = chemin.map_or("liste livrée avec l'outil".to_owned(), |c| c.display().to_string());
        let validation = if regles.est_validee() {
            format!("validée par {} le {}", regles.valide_par, regles.valide_le)
        } else {
            "NON validée".to_owned()
        };
        println!("Mentions « {} » ({source}) : {validation}", regles.version);
        for mention in &regles.mentions {
            println!("  {:<26} {} ({})", mention.id, mention.libelle, mention.reference);
        }
        return Ok(());
    }

    let (config, dokos) = dokos_config::connecter(Profil::Rh)?;
    let (regles, _) = dokos_config::charger_regles()?;
    match contrat {
        Contrat::Creer {
            employe,
            type_contrat,
            poste,
            entree,
            fin,
            heures,
            repartition,
            champs,
            format,
            simulation,
            autoriser_doublon,
        } => rh::creer_un_contrat(
            &dokos,
            &regles,
            config.societe.as_deref(),
            rh::DemandeContrat {
                employe: &employe,
                type_contrat: &type_contrat,
                poste: &poste,
                entree,
                fin,
                heures,
                repartition,
                champs: champs.into_iter().collect::<BTreeMap<_, _>>(),
                format: format.as_deref(),
                simulation,
                si_doublon: si_doublon(autoriser_doublon),
            },
        ),
        Contrat::Verifier { id, format } => rh::verifier_un_contrat(&dokos, &regles, &id, format.as_deref()),
        Contrat::Pdf { id, format, sortie } => rh::pdf(&dokos, &id, &format, sortie),
        Contrat::Alertes => plus::afficher_alertes_rh(&dokos, &regles.categories.cdd),
        Contrat::Regles { .. } => Ok(()),
    }
}

fn gerer_format(format: Format) -> Result<(), Erreur> {
    let (_, dokos) = dokos_config::connecter(Profil::Rh)?;
    let (regles, _) = dokos_config::charger_regles()?;
    let contexte = |c: &ContexteFormat| {
        let mut contexte = regles.contexte_contrat(&c.type_contrat, None);
        contexte.temps_partiel = c.temps_partiel;
        contexte
    };
    match format {
        Format::Lister => rh::lister_les_formats(&dokos),
        Format::Verifier { contexte: c } => {
            avertir_type_inconnu(&regles, &c.type_contrat, &contexte(&c));
            rh::verifier_un_format(&dokos, &regles, &c.format, &contexte(&c), None)
        }
        Format::Proposer { contexte: c, sortie } => {
            avertir_type_inconnu(&regles, &c.type_contrat, &contexte(&c));
            rh::verifier_un_format(&dokos, &regles, &c.format, &contexte(&c), Some(sortie))
        }
    }
}

fn avertir_type_inconnu(regles: &conformite::Regles, type_contrat: &str, contexte: &Contexte) {
    if !contexte.cdi && !contexte.cdd {
        println!(
            "Attention : « {type_contrat} » n'est ni dans cdi ({}) ni dans cdd ({}) : seules les mentions communes sont vérifiées.\n",
            regles.categories.cdi.join(", "),
            regles.categories.cdd.join(", ")
        );
    }
}

fn afficher_resultats<'a>(resultats: impl Iterator<Item = (&'a str, &'a str)>) {
    let resultats: Vec<_> = resultats.collect();
    if resultats.is_empty() {
        println!("Aucun résultat.");
        return;
    }
    let largeur = resultats.iter().map(|(id, _)| id.chars().count()).max().unwrap_or(0);
    for (id, nom) in &resultats {
        println!("{id:<largeur$}  {nom}");
    }
    if resultats.len() == LIMITE_RECHERCHE as usize {
        println!("(liste limitée à {} résultats : précisez la recherche)", resultats.len());
    }
}
