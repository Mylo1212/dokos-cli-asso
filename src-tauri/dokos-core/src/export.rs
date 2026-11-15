//! Listes exportables en tableau : clients, articles, devis, commandes,
//! factures, employés. **Lecture seule.**
//!
//! Les colonnes, les filtres possibles et les statuts sont fixés ici : ni
//! l'IA ni l'interface ne choisissent un DocType ou un champ. Les colonnes
//! absentes de l'instance (champ standard retiré, version différente) sont
//! écartées après lecture de la structure du DocType, avec un avertissement.

use std::fmt;
use std::str::FromStr;

use serde_json::{Map, Value};

use crate::article::DOCTYPE_GROUPE;
use crate::client::{Dokos, Filtre, Liste, lister_noms};
use crate::clientele::{DOCTYPE_GROUPE_CLIENT, DOCTYPE_TERRITOIRE};
use crate::date::Date;
use crate::employe::DOCTYPE_EMPLOYE;
use crate::erreur::DokosError;
use crate::meta::lire_meta;
use crate::modele_vente::filtre_sans_modeles;
use crate::recherche::{Correspondance, normaliser};
use crate::transport::Transport;
use crate::types::{DOCTYPE_ARTICLE, DOCTYPE_CLIENT, DOCTYPE_COMMANDE, DOCTYPE_DEVIS, DOCTYPE_FACTURE};

/// Nombre maximal de lignes d'un export.
pub const LIGNES_MAX: usize = 10_000;
/// Longueur maximale du texte cherché dans les noms.
pub const TEXTE_MAX: usize = 140;
/// Valeurs de référence (groupes, territoires) lues au plus.
const REFERENCES_MAX: u32 = 500;
/// Candidats proposés quand une valeur de référence est ambiguë.
const CANDIDATS_MAX: usize = 10;
/// Champs présents dans tous les DocTypes, absents de la structure renvoyée par Dokos.
const CHAMPS_STANDARD: &[&str] = &["name", "creation", "modified", "owner", "docstatus"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeListe {
    Clients,
    Articles,
    Devis,
    Commandes,
    Factures,
    Employes,
}

/// Façon de présenter une valeur dans le tableau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    Texte,
    Nombre,
    Date,
    /// Case à cocher de Dokos (0/1) : « oui » ou « non ».
    OuiNon,
    /// Valeur d'une liste de Dokos traduite en français (statut, type de client).
    Statut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colonne {
    pub champ: &'static str,
    pub entete: &'static str,
    pub genre: Genre,
}

const fn col(champ: &'static str, entete: &'static str, genre: Genre) -> Colonne {
    Colonne { champ, entete, genre }
}

/// Statut proposé à l'utilisateur et à l'IA, et les statuts Dokos qu'il regroupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statut {
    pub cle: &'static str,
    pub libelle: &'static str,
    pub valeurs: &'static [&'static str],
}

const fn statut(cle: &'static str, libelle: &'static str, valeurs: &'static [&'static str]) -> Statut {
    Statut { cle, libelle, valeurs }
}

use Genre::{Date as D, Nombre as N, OuiNon as O, Statut as S, Texte as T};

const COLONNES_CLIENTS: &[Colonne] = &[
    col("name", "Identifiant", T),
    col("customer_name", "Nom", T),
    col("customer_type", "Type", S),
    col("customer_group", "Groupe", T),
    col("territory", "Territoire", T),
    col("email_id", "E-mail", T),
    col("mobile_no", "Téléphone", T),
    col("disabled", "Désactivé", O),
    col("creation", "Créé le", D),
];

const COLONNES_ARTICLES: &[Colonne] = &[
    col("name", "Code", T),
    col("item_name", "Libellé", T),
    col("item_group", "Groupe", T),
    col("stock_uom", "Unité", T),
    col("is_sales_item", "Vendable", O),
    col("is_stock_item", "Stocké", O),
    col("disabled", "Désactivé", O),
];

const COLONNES_DEVIS: &[Colonne] = &[
    col("name", "Numéro", T),
    col("party_name", "Code client", T),
    col("customer_name", "Client", T),
    col("transaction_date", "Date", D),
    col("valid_till", "Valable jusqu'au", D),
    col("net_total", "Total HT", N),
    col("grand_total", "Total TTC", N),
    col("currency", "Devise", T),
    col("status", "Statut", S),
];

const COLONNES_COMMANDES: &[Colonne] = &[
    col("name", "Numéro", T),
    col("customer", "Code client", T),
    col("customer_name", "Client", T),
    col("transaction_date", "Date", D),
    col("delivery_date", "Livraison", D),
    col("net_total", "Total HT", N),
    col("grand_total", "Total TTC", N),
    col("currency", "Devise", T),
    col("per_delivered", "% livré", N),
    col("per_billed", "% facturé", N),
    col("status", "Statut", S),
];

const COLONNES_FACTURES: &[Colonne] = &[
    col("name", "Numéro", T),
    col("customer", "Code client", T),
    col("customer_name", "Client", T),
    col("posting_date", "Date", D),
    col("due_date", "Échéance", D),
    col("net_total", "Total HT", N),
    col("grand_total", "Total TTC", N),
    col("outstanding_amount", "Reste dû", N),
    col("currency", "Devise", T),
    col("status", "Statut", S),
];

const COLONNES_EMPLOYES: &[Colonne] = &[
    col("name", "Matricule", T),
    col("employee_name", "Nom", T),
    col("department", "Service", T),
    col("designation", "Poste", T),
    col("date_of_joining", "Entrée", D),
    col("relieving_date", "Sortie", D),
    col("status", "Statut", S),
    col("company_email", "E-mail professionnel", T),
    col("cell_number", "Téléphone", T),
];

const STATUTS_DEVIS: &[Statut] = &[
    statut("brouillon", "brouillons", &["Draft"]),
    statut("ouvert", "en cours", &["Open", "Replied", "Partially Ordered"]),
    statut("commande", "commandés", &["Ordered"]),
    statut("perdu", "perdus", &["Lost"]),
    statut("expire", "expirés", &["Expired"]),
    statut("annule", "annulés", &["Cancelled"]),
];

const STATUTS_COMMANDES: &[Statut] = &[
    statut("brouillon", "brouillons", &["Draft"]),
    statut("a_livrer", "à livrer", &["To Deliver and Bill", "To Deliver"]),
    statut("a_facturer", "à facturer", &["To Deliver and Bill", "To Bill"]),
    statut("terminee", "terminées", &["Completed"]),
    statut("suspendue", "suspendues", &["On Hold"]),
    statut("fermee", "fermées", &["Closed"]),
    statut("annulee", "annulées", &["Cancelled"]),
];

const STATUTS_FACTURES: &[Statut] = &[
    statut("brouillon", "brouillons", &["Draft"]),
    statut(
        "impayee",
        "impayées",
        &[
            "Unpaid",
            "Overdue",
            "Partly Paid",
            "Unpaid and Discounted",
            "Overdue and Discounted",
            "Partly Paid and Discounted",
        ],
    ),
    statut("en_retard", "en retard", &["Overdue", "Overdue and Discounted"]),
    statut("payee", "payées", &["Paid"]),
    statut("avoir", "avoirs et retours", &["Return", "Credit Note Issued"]),
    statut("annulee", "annulées", &["Cancelled"]),
];

const STATUTS_EMPLOYES: &[Statut] = &[
    statut("actif", "actifs", &["Active"]),
    statut("inactif", "inactifs", &["Inactive"]),
    statut("suspendu", "suspendus", &["Suspended"]),
    statut("parti", "partis", &["Left"]),
];

impl TypeListe {
    pub const TOUS: [TypeListe; 6] = [
        TypeListe::Clients,
        TypeListe::Articles,
        TypeListe::Devis,
        TypeListe::Commandes,
        TypeListe::Factures,
        TypeListe::Employes,
    ];

    pub fn nom(self) -> &'static str {
        match self {
            Self::Clients => "clients",
            Self::Articles => "articles",
            Self::Devis => "devis",
            Self::Commandes => "commandes",
            Self::Factures => "factures",
            Self::Employes => "employes",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Clients => "Clients",
            Self::Articles => "Articles",
            Self::Devis => "Devis",
            Self::Commandes => "Commandes",
            Self::Factures => "Factures",
            Self::Employes => "Employés",
        }
    }

    pub fn doctype(self) -> &'static str {
        match self {
            Self::Clients => DOCTYPE_CLIENT,
            Self::Articles => DOCTYPE_ARTICLE,
            Self::Devis => DOCTYPE_DEVIS,
            Self::Commandes => DOCTYPE_COMMANDE,
            Self::Factures => DOCTYPE_FACTURE,
            Self::Employes => DOCTYPE_EMPLOYE,
        }
    }

    pub fn colonnes(self) -> &'static [Colonne] {
        match self {
            Self::Clients => COLONNES_CLIENTS,
            Self::Articles => COLONNES_ARTICLES,
            Self::Devis => COLONNES_DEVIS,
            Self::Commandes => COLONNES_COMMANDES,
            Self::Factures => COLONNES_FACTURES,
            Self::Employes => COLONNES_EMPLOYES,
        }
    }

    pub fn statuts(self) -> &'static [Statut] {
        match self {
            Self::Clients | Self::Articles => &[],
            Self::Devis => STATUTS_DEVIS,
            Self::Commandes => STATUTS_COMMANDES,
            Self::Factures => STATUTS_FACTURES,
            Self::Employes => STATUTS_EMPLOYES,
        }
    }

    pub fn statut(self, cle: &str) -> Option<&'static Statut> {
        self.statuts().iter().find(|s| s.cle == cle)
    }

    /// Champ comparé au client choisi.
    pub fn champ_client(self) -> Option<&'static str> {
        match self {
            Self::Devis => Some("party_name"),
            Self::Commandes | Self::Factures => Some("customer"),
            Self::Clients | Self::Articles | Self::Employes => None,
        }
    }

    /// Champ date filtré par « du … au … », et son libellé.
    pub fn champ_date(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Devis | Self::Commandes => Some(("transaction_date", "date du document")),
            Self::Factures => Some(("posting_date", "date de facture")),
            Self::Employes => Some(("date_of_joining", "date d'entrée")),
            Self::Clients | Self::Articles => None,
        }
    }

    /// Champ du nom, pour « dont le nom contient … ».
    pub fn champ_nom(self) -> &'static str {
        match self {
            Self::Clients | Self::Devis | Self::Commandes | Self::Factures => "customer_name",
            Self::Articles => "item_name",
            Self::Employes => "employee_name",
        }
    }

    /// Champ et DocType arborescent du filtre « groupe ».
    pub fn groupe(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Clients => Some(("customer_group", DOCTYPE_GROUPE_CLIENT)),
            Self::Articles => Some(("item_group", DOCTYPE_GROUPE)),
            _ => None,
        }
    }

    pub fn a_territoire(self) -> bool {
        self == Self::Clients
    }

    fn ordre(self) -> &'static str {
        match self {
            Self::Clients => "customer_name asc",
            Self::Articles => "item_name asc",
            Self::Devis | Self::Commandes => "transaction_date desc",
            Self::Factures => "posting_date desc",
            Self::Employes => "employee_name asc",
        }
    }

    /// Ce que « inclure les inactifs » ajoute à la liste.
    pub fn libelle_inactifs(self) -> &'static str {
        match self {
            Self::Clients | Self::Articles => "désactivés",
            Self::Devis | Self::Commandes | Self::Factures => "annulés",
            Self::Employes => "inactifs et partis",
        }
    }
}

impl fmt::Display for TypeListe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

impl FromStr for TypeListe {
    type Err = ErreurExport;

    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        let cible = normaliser(texte);
        Self::TOUS
            .into_iter()
            .find(|t| t.nom() == cible || normaliser(t.libelle()) == cible)
            .ok_or_else(|| ErreurExport::Filtre(format!("liste « {texte} » inconnue")))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurExport {
    /// Filtre incohérent avec la liste, ou valeur invalide.
    Filtre(String),
    Dokos(DokosError),
}

impl fmt::Display for ErreurExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Filtre(m) => f.write_str(m),
            Self::Dokos(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ErreurExport {}

impl From<DokosError> for ErreurExport {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

/// Filtres d'un export. Par défaut : tout, sauf les éléments désactivés,
/// annulés ou partis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FiltresExport {
    /// Identifiant Dokos du client (`name`).
    pub client: Option<String>,
    /// Groupe de clients ou d'articles, sous-groupes compris.
    pub groupe: Option<String>,
    /// Territoire des clients, sous-territoires compris.
    pub territoire: Option<String>,
    pub du: Option<Date>,
    pub au: Option<Date>,
    /// Clé d'un [`Statut`] de la liste.
    pub statut: Option<String>,
    /// Texte contenu dans le nom.
    pub texte: Option<String>,
    pub inclure_inactifs: bool,
    /// Articles vendables seulement.
    pub vendables_seulement: bool,
}

impl FiltresExport {
    /// Vérifie que chaque filtre s'applique à la liste.
    pub fn verifier(&self, liste: TypeListe) -> Result<(), ErreurExport> {
        let refuse = |filtre: &str| {
            Err(ErreurExport::Filtre(format!(
                "le filtre « {filtre} » ne s'applique pas à la liste des {}",
                liste.libelle().to_lowercase()
            )))
        };
        if self.client.is_some() && liste.champ_client().is_none() {
            return refuse("client");
        }
        if self.groupe.is_some() && liste.groupe().is_none() {
            return refuse("groupe");
        }
        if self.territoire.is_some() && !liste.a_territoire() {
            return refuse("territoire");
        }
        if (self.du.is_some() || self.au.is_some()) && liste.champ_date().is_none() {
            return refuse("dates");
        }
        if self.vendables_seulement && liste != TypeListe::Articles {
            return refuse("vendables seulement");
        }
        if let Some(cle) = &self.statut
            && liste.statut(cle).is_none()
        {
            let possibles: Vec<&str> = liste.statuts().iter().map(|s| s.cle).collect();
            return Err(ErreurExport::Filtre(if possibles.is_empty() {
                format!("pas de statut pour la liste des {}", liste.libelle().to_lowercase())
            } else {
                format!("statut « {cle} » inconnu : {}", possibles.join(", "))
            }));
        }
        if let (Some(du), Some(au)) = (self.du, self.au)
            && du > au
        {
            return Err(ErreurExport::Filtre(format!("période inversée : du {du} au {au}")));
        }
        if self.texte.as_ref().is_some_and(|t| t.chars().count() > TEXTE_MAX) {
            return Err(ErreurExport::Filtre(format!("texte cherché trop long ({TEXTE_MAX} caractères au plus)")));
        }
        Ok(())
    }

    fn en_filtres(&self, liste: TypeListe) -> Vec<Filtre> {
        let mut filtres = Vec::new();
        if !self.inclure_inactifs {
            match liste {
                TypeListe::Clients | TypeListe::Articles => filtres.push(Filtre::egal("disabled", 0)),
                // Un statut demandé (« annulées », « partis ») prime.
                TypeListe::Devis | TypeListe::Commandes | TypeListe::Factures if self.statut.is_none() => {
                    filtres.push(Filtre::different("docstatus", 2))
                }
                TypeListe::Employes if self.statut.is_none() => filtres.push(Filtre::egal("status", "Active")),
                _ => {}
            }
        }
        // Les modèles de devis, commandes et factures (étiquette « Template ») ne sont pas exportés.
        if liste.champ_client().is_some() {
            filtres.push(filtre_sans_modeles());
        }
        if let (Some(id), Some(champ)) = (&self.client, liste.champ_client()) {
            if liste == TypeListe::Devis {
                filtres.push(Filtre::egal("quotation_to", "Customer"));
            }
            filtres.push(Filtre::egal(champ, id.as_str()));
        }
        if let (Some(groupe), Some((champ, _))) = (&self.groupe, liste.groupe()) {
            filtres.push(Filtre::descendant_de(champ, groupe));
        }
        if let Some(territoire) = self.territoire.as_ref().filter(|_| liste.a_territoire()) {
            filtres.push(Filtre::descendant_de("territory", territoire));
        }
        if let Some((champ, _)) = liste.champ_date() {
            match (self.du, self.au) {
                (Some(du), Some(au)) => filtres.push(Filtre::entre(champ, du.to_string(), au.to_string())),
                (Some(du), None) => filtres.push(Filtre::a_partir_de(champ, du.to_string())),
                (None, Some(au)) => filtres.push(Filtre::jusqua(champ, au.to_string())),
                (None, None) => {}
            }
        }
        if let Some(statut) = self.statut.as_deref().and_then(|c| liste.statut(c)) {
            filtres.push(Filtre::parmi("status", statut.valeurs));
        }
        if let Some(texte) = self.texte.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            filtres.push(Filtre::contient(liste.champ_nom(), texte));
        }
        if self.vendables_seulement && liste == TypeListe::Articles {
            filtres.push(Filtre::egal("is_sales_item", 1));
        }
        filtres
    }

    /// Description lisible : « client CUST-1, du 2026-10-01 au 2026-10-31, impayées ».
    pub fn decrire(&self, liste: TypeListe) -> String {
        let mut parties = Vec::new();
        if let Some(client) = &self.client {
            parties.push(format!("client {client}"));
        }
        if let Some(groupe) = &self.groupe {
            parties.push(format!("groupe {groupe}"));
        }
        if let Some(territoire) = &self.territoire {
            parties.push(format!("territoire {territoire}"));
        }
        match (self.du, self.au) {
            (Some(du), Some(au)) => parties.push(format!("du {du} au {au}")),
            (Some(du), None) => parties.push(format!("à partir du {du}")),
            (None, Some(au)) => parties.push(format!("jusqu'au {au}")),
            (None, None) => {}
        }
        if let Some(statut) = self.statut.as_deref().and_then(|c| liste.statut(c)) {
            parties.push(statut.libelle.to_owned());
        }
        if let Some(texte) = &self.texte {
            parties.push(format!("nom contenant « {texte} »"));
        }
        if self.vendables_seulement {
            parties.push("vendables".to_owned());
        }
        if self.inclure_inactifs {
            parties.push(format!("{} compris", liste.libelle_inactifs()));
        }
        if parties.is_empty() {
            format!("{} : tous", liste.libelle())
        } else {
            format!("{} : {}", liste.libelle(), parties.join(", "))
        }
    }
}

/// Valeur d'une case du tableau.
#[derive(Debug, Clone, PartialEq)]
pub enum Cellule {
    Vide,
    Texte(String),
    Nombre(f64),
    Date(Date),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tableau {
    pub liste: TypeListe,
    pub entetes: Vec<String>,
    pub lignes: Vec<Vec<Cellule>>,
    /// Vrai si la liste dépassait [`LIGNES_MAX`] lignes.
    pub tronque: bool,
    pub avertissements: Vec<String>,
}

/// Lit la liste dans Dokos, page par page, dans la limite de [`LIGNES_MAX`].
pub fn lire_tableau<T: Transport>(
    dokos: &Dokos<T>,
    liste: TypeListe,
    filtres: &FiltresExport,
) -> Result<Tableau, ErreurExport> {
    filtres.verifier(liste)?;
    let mut avertissements = Vec::new();
    let colonnes: Vec<Colonne> = match lire_meta(dokos, liste.doctype()) {
        Ok(meta) => {
            let (gardees, absentes): (Vec<Colonne>, Vec<Colonne>) =
                liste.colonnes().iter().partition(|c| CHAMPS_STANDARD.contains(&c.champ) || meta.a_champ(c.champ));
            if !absentes.is_empty() {
                let noms: Vec<&str> = absentes.iter().map(|c| c.entete).collect();
                avertissements.push(format!("colonnes absentes de votre Dokos, non exportées : {}", noms.join(", ")));
            }
            gardees
        }
        Err(e) => {
            avertissements.push(format!("structure de {} non lue ({e}) : colonnes standard", liste.doctype()));
            liste.colonnes().to_vec()
        }
    };
    let champs: Vec<&str> = colonnes.iter().map(|c| c.champ).collect();
    let page = Liste::limite_max();
    let preparer =
        || filtres.en_filtres(liste).into_iter().fold(Liste::new(&champs).ordre(liste.ordre()), Liste::filtre);

    let mut lignes = Vec::new();
    let mut tronque = false;
    loop {
        let debut = u32::try_from(lignes.len()).unwrap_or(u32::MAX);
        let documents: Vec<Map<String, Value>> =
            dokos.lister(liste.doctype(), &preparer().limite(page).debut(debut))?;
        let complete = documents.len() as u32 == page;
        lignes.extend(documents.iter().map(|d| colonnes.iter().map(|c| cellule(d.get(c.champ), c.genre)).collect()));
        if !complete {
            break;
        }
        if lignes.len() >= LIGNES_MAX {
            lignes.truncate(LIGNES_MAX);
            let suite: Vec<Map<String, Value>> =
                dokos.lister(liste.doctype(), &preparer().limite(1).debut(LIGNES_MAX as u32))?;
            tronque = !suite.is_empty();
            break;
        }
    }
    if tronque {
        avertissements.push(format!("liste limitée aux {LIGNES_MAX} premières lignes : précisez les filtres"));
    }
    Ok(Tableau {
        liste,
        entetes: colonnes.iter().map(|c| c.entete.to_owned()).collect(),
        lignes,
        tronque,
        avertissements,
    })
}

fn cellule(valeur: Option<&Value>, genre: Genre) -> Cellule {
    let texte = |v: &Value| match v {
        Value::String(s) => s.trim().to_owned(),
        Value::Null => String::new(),
        autre => autre.to_string(),
    };
    let Some(valeur) = valeur.filter(|v| !v.is_null()) else {
        return Cellule::Vide;
    };
    let brut = texte(valeur);
    match genre {
        Genre::Nombre => match valeur.as_f64().or_else(|| brut.parse().ok()) {
            Some(n) if n.is_finite() => Cellule::Nombre(n),
            _ if brut.is_empty() => Cellule::Vide,
            _ => Cellule::Texte(brut),
        },
        Genre::Date => match brut.get(..10).and_then(|d| d.parse().ok()) {
            Some(d) => Cellule::Date(d),
            None if brut.is_empty() => Cellule::Vide,
            None => Cellule::Texte(brut),
        },
        Genre::OuiNon => {
            let oui = valeur.as_bool().unwrap_or(false) || valeur.as_i64().is_some_and(|n| n != 0) || brut == "1";
            Cellule::Texte(if oui { "oui" } else { "non" }.to_owned())
        }
        Genre::Statut if !brut.is_empty() => Cellule::Texte(traduire(&brut).to_owned()),
        Genre::Texte | Genre::Statut if brut.is_empty() => Cellule::Vide,
        Genre::Texte | Genre::Statut => Cellule::Texte(brut),
    }
}

/// Statuts et types de Dokos en français ; une valeur inconnue reste telle quelle.
fn traduire(valeur: &str) -> &str {
    match valeur {
        "Draft" => "Brouillon",
        "Open" => "Ouvert",
        "Replied" => "Répondu",
        "Partially Ordered" => "Partiellement commandé",
        "Ordered" => "Commandé",
        "Lost" => "Perdu",
        "Expired" => "Expiré",
        "Cancelled" => "Annulé",
        "To Deliver and Bill" => "À livrer et facturer",
        "To Deliver" => "À livrer",
        "To Bill" => "À facturer",
        "Completed" => "Terminé",
        "Closed" => "Fermé",
        "On Hold" => "Suspendu",
        "Unpaid" => "Impayé",
        "Overdue" => "En retard",
        "Partly Paid" => "Partiellement payé",
        "Paid" => "Payé",
        "Return" => "Retour",
        "Credit Note Issued" => "Avoir émis",
        "Unpaid and Discounted" => "Impayé (escompté)",
        "Overdue and Discounted" => "En retard (escompté)",
        "Partly Paid and Discounted" => "Partiellement payé (escompté)",
        "Active" => "Actif",
        "Inactive" => "Inactif",
        "Suspended" => "Suspendu",
        "Left" => "Parti",
        "Company" => "Société",
        "Individual" => "Particulier",
        autre => autre,
    }
}

/// Référentiel d'un filtre : groupes ou territoires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
    Groupe,
    Territoire,
}

/// Valeurs possibles du filtre `reference` pour la liste (groupes de
/// premier niveau compris : le filtre inclut les sous-groupes).
pub fn valeurs_possibles<T: Transport>(
    dokos: &Dokos<T>,
    liste: TypeListe,
    reference: Reference,
) -> Result<Vec<String>, DokosError> {
    let doctype = match reference {
        Reference::Groupe => liste.groupe().map(|(_, d)| d),
        Reference::Territoire => liste.a_territoire().then_some(DOCTYPE_TERRITOIRE),
    };
    match doctype {
        Some(doctype) => lister_noms(dokos, doctype, Vec::new(), REFERENCES_MAX),
        None => Ok(Vec::new()),
    }
}

/// Retrouve `texte` parmi `valeurs` : égalité (casse, accents et ponctuation
/// ignorés), sinon valeurs qui le contiennent.
pub fn choisir_valeur(texte: &str, valeurs: &[String]) -> Correspondance<String> {
    let cible = normaliser(texte);
    if cible.is_empty() {
        return Correspondance::Aucune;
    }
    if let Some(exacte) = valeurs.iter().find(|v| normaliser(v) == cible) {
        return Correspondance::Unique(exacte.clone());
    }
    let mut proches: Vec<String> = valeurs.iter().filter(|v| normaliser(v).contains(&cible)).cloned().collect();
    match proches.len() {
        0 => Correspondance::Aucune,
        1 => proches.pop().map_or(Correspondance::Aucune, Correspondance::Unique),
        _ => {
            proches.truncate(CANDIDATS_MAX);
            Correspondance::Plusieurs(proches)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::meta::tests::reponse_meta;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn meta_complete(liste: TypeListe) -> Value {
        let champs: Vec<Value> = liste
            .colonnes()
            .iter()
            .filter(|c| !CHAMPS_STANDARD.contains(&c.champ))
            .map(|c| json!({"fieldname": c.champ, "fieldtype": "Data"}))
            .collect();
        reponse_meta(liste.doctype(), Value::Array(champs))
    }

    fn filtres_envoyes(faux: &FauxTransport, index: usize) -> Value {
        let requete = &faux.requetes()[index];
        serde_json::from_str(requete.parametre("filters").unwrap_or("[]")).expect("filtres JSON")
    }

    #[test]
    fn noms_des_listes() {
        assert_eq!("Employés".parse::<TypeListe>().expect("liste"), TypeListe::Employes);
        assert_eq!(" factures ".parse::<TypeListe>().expect("liste"), TypeListe::Factures);
        assert!("Fournisseurs".parse::<TypeListe>().is_err());
        for liste in TypeListe::TOUS {
            assert_eq!(liste.nom().parse::<TypeListe>().expect("liste"), liste);
            assert_eq!(liste.colonnes()[0].champ, "name");
        }
    }

    #[test]
    fn clients_actifs_par_defaut() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &meta_complete(TypeListe::Clients));
        faux.repondre_json(
            200,
            &json!({"data": [
                {"name": "CUST-1", "customer_name": "Mairie de Crest", "customer_type": "Company",
                 "customer_group": "Collectivités", "territory": "France", "email_id": null,
                 "mobile_no": "", "disabled": 0, "creation": "2026-01-15 10:22:11.123456"},
            ]}),
        );
        let tableau = lire_tableau(&Dokos::new(&faux), TypeListe::Clients, &FiltresExport::default()).expect("tableau");
        assert_eq!(tableau.entetes.len(), COLONNES_CLIENTS.len());
        assert_eq!(tableau.entetes[1], "Nom");
        let ligne = &tableau.lignes[0];
        assert_eq!(ligne[1], Cellule::Texte("Mairie de Crest".into()));
        assert_eq!(ligne[2], Cellule::Texte("Société".into()));
        assert_eq!(ligne[5], Cellule::Vide);
        assert_eq!(ligne[6], Cellule::Vide);
        assert_eq!(ligne[7], Cellule::Texte("non".into()));
        assert_eq!(ligne[8], Cellule::Date("2026-01-15".parse().expect("date")));
        assert!(!tableau.tronque);
        assert!(tableau.avertissements.is_empty());

        let requetes = faux.requetes();
        assert!(requetes.iter().all(|r| r.methode == Methode::Get));
        assert_eq!(requetes[1].chemin, "/api/resource/Customer");
        assert_eq!(filtres_envoyes(&faux, 1), json!([["disabled", "=", 0]]));
        assert_eq!(requetes[1].parametre("order_by"), Some("customer_name asc"));
        assert_eq!(requetes[1].parametre("limit_page_length"), Some("500"));
        assert_eq!(requetes[1].parametre("limit_start"), None);
    }

    #[test]
    fn factures_filtrees() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &meta_complete(TypeListe::Factures));
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "FAC-1", "customer": "CUST-1", "posting_date": "2026-10-03",
                              "grand_total": 1250.5, "outstanding_amount": "200", "status": "Overdue"}]}),
        );
        let filtres = FiltresExport {
            client: Some("CUST-1".into()),
            du: Some("2026-10-01".parse().expect("date")),
            au: Some("2026-10-31".parse().expect("date")),
            statut: Some("impayee".into()),
            ..FiltresExport::default()
        };
        let tableau = lire_tableau(&Dokos::new(&faux), TypeListe::Factures, &filtres).expect("tableau");
        let ligne = &tableau.lignes[0];
        assert_eq!(ligne[3], Cellule::Date("2026-10-03".parse().expect("date")));
        assert_eq!(ligne[4], Cellule::Vide);
        assert_eq!(ligne[6], Cellule::Nombre(1250.5));
        assert_eq!(ligne[7], Cellule::Nombre(200.0));
        assert_eq!(ligne[9], Cellule::Texte("En retard".into()));
        let envoyes = filtres_envoyes(&faux, 1);
        // Statut demandé : pas de filtre « non annulées » en plus ; les modèles sont toujours écartés.
        assert_eq!(envoyes[0], json!(["_user_tags", "not like", "%Template%"]));
        assert_eq!(envoyes[1], json!(["customer", "=", "CUST-1"]));
        assert_eq!(envoyes[2], json!(["posting_date", "between", ["2026-10-01", "2026-10-31"]]));
        assert_eq!(envoyes[3][0], "status");
        assert_eq!(envoyes[3][1], "in");
        assert_eq!(envoyes.as_array().map(Vec::len), Some(4));
        assert_eq!(
            filtres.decrire(TypeListe::Factures),
            "Factures : client CUST-1, du 2026-10-01 au 2026-10-31, impayées"
        );
    }

    #[test]
    fn devis_et_articles() {
        let devis = FiltresExport {
            client: Some("CUST-2".into()),
            du: Some("2026-01-01".parse().expect("date")),
            ..FiltresExport::default()
        };
        assert_eq!(
            Value::Array(devis.en_filtres(TypeListe::Devis).iter().map(Filtre::en_json).collect()),
            json!([
                ["docstatus", "!=", 2],
                ["_user_tags", "not like", "%Template%"],
                ["quotation_to", "=", "Customer"],
                ["party_name", "=", "CUST-2"],
                ["transaction_date", ">=", "2026-01-01"],
            ])
        );
        let articles = FiltresExport {
            groupe: Some("Repas".into()),
            texte: Some("végé".into()),
            vendables_seulement: true,
            inclure_inactifs: true,
            ..FiltresExport::default()
        };
        assert_eq!(
            Value::Array(articles.en_filtres(TypeListe::Articles).iter().map(Filtre::en_json).collect()),
            json!([
                ["item_group", "descendants of (inclusive)", "Repas"],
                ["item_name", "like", "%végé%"],
                ["is_sales_item", "=", 1],
            ])
        );
        assert_eq!(
            articles.decrire(TypeListe::Articles),
            "Articles : groupe Repas, nom contenant « végé », vendables, désactivés compris"
        );
        let employes = FiltresExport { au: Some("2025-12-31".parse().expect("date")), ..FiltresExport::default() };
        assert_eq!(
            Value::Array(employes.en_filtres(TypeListe::Employes).iter().map(Filtre::en_json).collect()),
            json!([["status", "=", "Active"], ["date_of_joining", "<=", "2025-12-31"]])
        );
    }

    #[test]
    fn filtres_incoherents_refuses_sans_appel() {
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);
        let cas = [
            (TypeListe::Clients, FiltresExport { client: Some("C".into()), ..Default::default() }),
            (TypeListe::Factures, FiltresExport { groupe: Some("G".into()), ..Default::default() }),
            (TypeListe::Articles, FiltresExport { du: Some("2026-01-01".parse().expect("d")), ..Default::default() }),
            (TypeListe::Factures, FiltresExport { statut: Some("perdu".into()), ..Default::default() }),
            (TypeListe::Clients, FiltresExport { statut: Some("actif".into()), ..Default::default() }),
            (TypeListe::Devis, FiltresExport { vendables_seulement: true, ..Default::default() }),
            (
                TypeListe::Factures,
                FiltresExport {
                    du: Some("2026-02-01".parse().expect("d")),
                    au: Some("2026-01-01".parse().expect("d")),
                    ..Default::default()
                },
            ),
            (TypeListe::Clients, FiltresExport { texte: Some("x".repeat(200)), ..Default::default() }),
        ];
        for (liste, filtres) in cas {
            assert!(
                matches!(lire_tableau(&dokos, liste, &filtres), Err(ErreurExport::Filtre(_))),
                "{liste} {filtres:?}"
            );
        }
        assert!(faux.requetes().is_empty());
    }

    #[test]
    fn pagination_et_limite() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &meta_complete(TypeListe::Articles));
        let page: Vec<Value> = (0..500).map(|i| json!({"name": format!("A{i}")})).collect();
        for _ in 0..(LIGNES_MAX / 500) {
            faux.repondre_json(200, &json!({ "data": page }));
        }
        faux.repondre_json(200, &json!({"data": [{"name": "suivant"}]}));
        let tableau =
            lire_tableau(&Dokos::new(&faux), TypeListe::Articles, &FiltresExport::default()).expect("tableau");
        assert_eq!(tableau.lignes.len(), LIGNES_MAX);
        assert!(tableau.tronque);
        assert_eq!(tableau.avertissements.len(), 1);
        let requetes = faux.requetes();
        assert_eq!(requetes[2].parametre("limit_start"), Some("500"));
        assert_eq!(requetes.last().and_then(|r| r.parametre("limit_start")), Some("10000"));
        assert_eq!(requetes.last().and_then(|r| r.parametre("limit_page_length")), Some("1"));
        assert_eq!(faux.reponses_restantes(), 0);
    }

    #[test]
    fn colonnes_absentes_ecartees() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_meta("Employee", json!([{"fieldname": "employee_name", "fieldtype": "Data"}])),
        );
        faux.repondre_json(200, &json!({"data": [{"name": "EMP-1", "employee_name": "Ana"}]}));
        let tableau =
            lire_tableau(&Dokos::new(&faux), TypeListe::Employes, &FiltresExport::default()).expect("tableau");
        assert_eq!(tableau.entetes, ["Matricule", "Nom"]);
        assert!(tableau.avertissements[0].contains("Service"));
        assert_eq!(faux.requetes()[1].parametre("fields"), Some(r#"["name","employee_name"]"#));

        // Structure illisible : colonnes standard, avec un avertissement.
        let faux = FauxTransport::new();
        faux.repondre(403, "{}");
        faux.repondre_json(200, &json!({"data": []}));
        let tableau = lire_tableau(&Dokos::new(&faux), TypeListe::Clients, &FiltresExport::default()).expect("tableau");
        assert_eq!(tableau.entetes.len(), COLONNES_CLIENTS.len());
        assert_eq!(tableau.avertissements.len(), 1);
    }

    #[test]
    fn valeurs_de_reference() {
        let groupes: Vec<String> = ["Collectivités", "Associations", "Associations sportives"].map(String::from).into();
        assert_eq!(choisir_valeur("collectivites", &groupes), Correspondance::Unique("Collectivités".into()));
        assert_eq!(choisir_valeur("Associations", &groupes), Correspondance::Unique("Associations".into()));
        assert_eq!(choisir_valeur("sport", &groupes), Correspondance::Unique("Associations sportives".into()));
        assert!(matches!(choisir_valeur("asso", &groupes), Correspondance::Plusieurs(v) if v.len() == 2));
        assert_eq!(choisir_valeur("Entreprises", &groupes), Correspondance::Aucune);
        assert_eq!(choisir_valeur(" ", &groupes), Correspondance::Aucune);

        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "France"}]}));
        let dokos = Dokos::new(&faux);
        assert_eq!(valeurs_possibles(&dokos, TypeListe::Clients, Reference::Territoire).expect("liste"), ["France"]);
        assert_eq!(faux.requetes()[0].chemin, "/api/resource/Territory");
        assert!(valeurs_possibles(&dokos, TypeListe::Factures, Reference::Groupe).expect("liste").is_empty());
        assert_eq!(faux.requetes().len(), 1);
    }
}
