//! `dokos` : piloter Dokos (version asso) depuis le terminal, par l'API REST de Frappe.

mod erreur;
mod plus;
mod saisie;
mod ventes;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use dokos_config::{Config, Profil};
use dokos_core::date::Date;
use dokos_core::export::TypeListe;
use dokos_core::grand_livre::JournalFichier;
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
    /// Écrit le modèle Excel d'un cas d'import (factures, paiements, articles, clients)
    Modele {
        cas: String,
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
    /// Exporte une liste de Dokos (clients, articles, devis, commandes, factures, employes) en fichier Excel, en lecture seule
    Exporter {
        /// clients, articles, devis, commandes, factures ou employes
        liste: TypeListe,
        /// Client : identifiant ou nom (devis, commandes, factures)
        #[arg(long)]
        client: Option<String>,
        /// Groupe de clients ou d'articles, sous-groupes compris
        #[arg(long)]
        groupe: Option<String>,
        /// Territoire des clients
        #[arg(long)]
        territoire: Option<String>,
        /// Début de période (date du document, d'entrée pour les employés)
        #[arg(long, value_parser = saisie::date)]
        du: Option<Date>,
        /// Fin de période
        #[arg(long, value_parser = saisie::date)]
        au: Option<Date>,
        /// Statut : brouillon, impayee, en_retard, payee… (voir l'onglet Export de l'appli)
        #[arg(long)]
        statut: Option<String>,
        /// Texte contenu dans le nom
        #[arg(long)]
        texte: Option<String>,
        /// Inclut les éléments désactivés, annulés ou partis
        #[arg(long)]
        inclure_inactifs: bool,
        /// Articles vendables seulement
        #[arg(long)]
        vendables_seulement: bool,
        /// Fichier .xlsx à écrire ; par défaut Documents\Dokos\exports
        #[arg(long)]
        sortie: Option<PathBuf>,
    },
    /// Journaux comptables actifs de Dokos (profil compta)
    Journaux,
    /// Exporte le grand livre d'un journal comptable en fichier Excel au format d'import, en lecture seule (profil compta)
    GrandLivre {
        /// Journal comptable : identifiant (« VEN - SLA ») ou code (« VEN »), voir `dokos journaux`
        #[arg(long)]
        journal: String,
        /// Début de période (date de comptabilisation)
        #[arg(long, value_parser = saisie::date)]
        du: Date,
        /// Fin de période, comprise
        #[arg(long, value_parser = saisie::date)]
        au: Date,
        /// Valeur de la colonne JOURNAL du fichier : VEN (par défaut) ou REC
        #[arg(long, default_value = "VEN")]
        journal_fichier: JournalFichier,
        /// Fichier .xlsx à écrire ; par défaut Documents\Dokos\exports
        #[arg(long)]
        sortie: Option<PathBuf>,
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
}

#[derive(Subcommand)]
enum Connexion {
    /// Teste la connexion d'un profil puis enregistre l'URL, la clé et le secret (demandé au clavier)
    Enregistrer {
        /// ventes, rh (export des employés) ou compta (grand livre)
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
        /// Société Dokos à utiliser s'il y en a plusieurs
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
    /// ARTICLE=QUANTITÉ[*PRIX][@DATE], à répéter pour chaque ligne ; *PRIX : prix unitaire HT
    #[arg(long = "ligne", value_name = "ARTICLE=QTE[*PRIX]", required = true, value_parser = saisie::ligne)]
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
        Action::Modele { cas, sortie } => {
            let cas = cas_import(&cas)?;
            let (config, dokos) = dokos_config::connecter(cas.profil().parse().map_err(Erreur::Saisie)?)?;
            plus::generer_un_modele(&dokos, cas, &config.url_base(), config.societe.as_deref(), sortie)
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
        Action::Exporter {
            liste,
            client,
            groupe,
            territoire,
            du,
            au,
            statut,
            texte,
            inclure_inactifs,
            vendables_seulement,
            sortie,
        } => {
            let profil = if liste == TypeListe::Employes { Profil::Rh } else { Profil::Ventes };
            let (_, dokos) = dokos_config::connecter(profil)?;
            plus::exporter(
                &dokos,
                &plus::DemandeExport {
                    liste,
                    client: client.as_deref(),
                    groupe: groupe.as_deref(),
                    territoire: territoire.as_deref(),
                    du,
                    au,
                    statut: statut.as_deref(),
                    texte: texte.as_deref(),
                    inclure_inactifs,
                    vendables_seulement,
                    sortie,
                },
            )
        }
        Action::Journaux => {
            let (_, dokos) = dokos_config::connecter(Profil::Compta)?;
            plus::afficher_journaux(&dokos)
        }
        Action::GrandLivre { journal, du, au, journal_fichier, sortie } => {
            let (_, dokos) = dokos_config::connecter(Profil::Compta)?;
            plus::exporter_grand_livre(&dokos, &journal, du, au, journal_fichier, sortie)
        }
        Action::Brouillons => {
            let (_, dokos) = dokos_config::connecter(Profil::Ventes)?;
            plus::afficher_brouillons(&dokos)
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
