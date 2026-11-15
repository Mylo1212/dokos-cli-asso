//! Données échangées avec l'interface. L'interface n'envoie que des demandes
//! (`Demande…`) ; Rust les valide avec les types de `dokos-core` et construit
//! lui-même toutes les requêtes vers Dokos.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilVue {
    pub profil: String,
    pub libelle: String,
    pub cle: Option<String>,
    pub roles: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatConfig {
    pub configure: bool,
    pub url: String,
    pub autoriser_http: bool,
    pub societe: String,
    pub profils: Vec<ProfilVue>,
    pub chemin_config: String,
}

/// État de la connexion d'un profil à Dokos.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnexionVue {
    pub profil: String,
    pub libelle: String,
    /// `ok`, `erreur` ou `non_configure`.
    pub etat: String,
    /// Utilisateur API connecté (si `ok`).
    pub utilisateur: String,
    /// Genre et message de l'erreur (si `erreur`).
    pub genre: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientVue {
    pub id: String,
    pub nom: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleVue {
    pub code: String,
    pub libelle: String,
    pub unite: String,
    pub groupe: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneDemande {
    /// Code article Dokos, choisi dans les suggestions.
    pub article: String,
    pub quantite: f64,
    pub livraison: Option<String>,
    /// Prix unitaire hors taxes saisi ; sans lui, Dokos applique ses listes de prix.
    #[serde(default)]
    pub prix: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeVente {
    /// `devis`, `commande` ou `facture`.
    pub type_vente: String,
    /// Identifiant Dokos du client, choisi dans les suggestions.
    pub client: String,
    pub date: String,
    pub echeance: Option<String>,
    pub lignes: Vec<LigneDemande>,
    /// Référence externe facultative (commande, facture) : « N° de commande client ».
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentVue {
    pub id: String,
    pub statut: String,
    pub date: String,
    pub total: f64,
    pub devise: String,
    pub lien: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApercuVente {
    pub identiques: Vec<DocumentVue>,
    pub similaires: Vec<DocumentVue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatVente {
    /// Faux : un document identique existait, rien n'a été créé.
    pub cree: bool,
    pub document: DocumentVue,
    pub autres: Vec<DocumentVue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionDemande {
    pub champ: String,
    pub obligatoire: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeEvenement {
    pub titre: String,
    pub debut: String,
    pub fin: Option<String>,
    pub journee: bool,
    pub lieu: Option<String>,
    pub description: Option<String>,
    pub places: Option<u32>,
    pub annulation: bool,
    pub questions: Vec<QuestionDemande>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampVue {
    pub champ: String,
    pub libelle: String,
    pub type_champ: String,
    pub obligatoire: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApercuEvenement {
    pub formulaire_titre: String,
    pub formulaire_route: String,
    pub champs: Vec<ChampVue>,
    /// Événement identique déjà dans Dokos.
    pub existant: Option<DocumentVue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatEvenement {
    pub cree: bool,
    pub evenement: DocumentVue,
    pub formulaire: String,
    pub formulaire_reutilise: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationVue {
    pub page: String,
    pub inscription: String,
}

// --- Assistant IA ---------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatIa {
    pub active: bool,
    pub url: String,
    pub modele: String,
    pub bases_ventes: String,
    pub bases_formulaires: String,
    pub cle_enregistree: bool,
}

/// Élément nommé par l'IA, après recherche dans Dokos.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoixVue<T> {
    /// `non_precise`, `trouve`, `a_preciser` ou `introuvable`.
    pub statut: String,
    /// Texte de la demande.
    pub texte: String,
    pub valeur: Option<T>,
    pub candidats: Vec<T>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneProposeeVue {
    pub texte: String,
    pub article: ChoixVue<ArticleVue>,
    pub quantite: Option<f64>,
    pub livraison: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropositionVenteVue {
    pub type_vente: Option<String>,
    pub client: ChoixVue<ClientVue>,
    pub date: Option<String>,
    pub echeance: Option<String>,
    pub lignes: Vec<LigneProposeeVue>,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionVue {
    pub champ: String,
    pub obligatoire: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropositionEvenementVue {
    pub titre: Option<String>,
    /// Format des champs `datetime-local` : `AAAA-MM-JJTHH:MM`.
    pub debut: Option<String>,
    pub fin: Option<String>,
    pub journee: bool,
    pub lieu: Option<String>,
    pub description: Option<String>,
    pub places: Option<u32>,
    pub annulation: bool,
    pub questions_formulaire: Vec<QuestionVue>,
    pub champs_a_creer: Vec<String>,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

// --- Catalogue --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoeudVue {
    pub id: String,
    pub groupe: bool,
    pub enfants: Vec<NoeudVue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueVue {
    pub arbre: Vec<NoeudVue>,
    pub liste_de_prix: Option<String>,
    pub unites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrixVue {
    pub article: String,
    pub prix: f64,
    pub devise: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeArticle {
    pub code: String,
    pub libelle: String,
    pub groupe: String,
    pub unite: String,
    pub vendable: bool,
    #[serde(default)]
    pub stocke: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatArticle {
    pub cree: bool,
    pub code: String,
    pub libelle: String,
    pub actif: bool,
    pub lien: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatGroupe {
    pub cree: bool,
    pub id: String,
}

// --- Mes brouillons, transformation ----------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrouillonVue {
    /// `devis`, `commande`, `facture` ou `evenement`.
    pub genre: String,
    pub id: String,
    pub tiers: String,
    pub date: String,
    pub total: f64,
    pub devise: String,
    pub modifie: String,
    pub lien: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrouillonsVue {
    pub ventes: Vec<BrouillonVue>,
    pub evenements: Vec<BrouillonVue>,
    pub avertissements: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatTransformationVue {
    pub cree: bool,
    pub document: DocumentVue,
}

// --- Inscrits ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvenementVue {
    pub id: String,
    pub titre: String,
    pub debut: String,
    pub publie: bool,
    pub lien: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InscritsVue {
    pub evenement: String,
    pub colonnes: Vec<ChampVue>,
    pub lignes: Vec<BTreeMap<String, String>>,
}

// --- Suivi : alertes RH, factures en retard -------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcheanceVue {
    pub id: String,
    pub employe: String,
    pub nom: String,
    pub type_contrat: String,
    pub entree: String,
    pub fin: String,
    /// Jours restants (fin) ou écoulés (entrée).
    pub jours: i64,
    pub lien: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertesRhVue {
    pub fins_proches: Vec<EcheanceVue>,
    pub cdd_sans_fin: Vec<EcheanceVue>,
    pub recents: Vec<EcheanceVue>,
    pub horizon_jours: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactureRetardVue {
    pub id: String,
    pub client: String,
    pub date: String,
    pub echeance: String,
    pub total: f64,
    pub solde: f64,
    pub devise: String,
    pub retard: i64,
    pub lien: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoldeVue {
    pub client: String,
    pub solde: f64,
    pub nombre: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpayesVue {
    pub factures: Vec<FactureRetardVue>,
    pub par_client: Vec<SoldeVue>,
    pub total: f64,
    pub devise: String,
}

// --- Import -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColonneVue {
    pub nom: String,
    pub obligatoire: bool,
    pub aide: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CasVue {
    pub nom: String,
    pub libelle: String,
    pub resultat: String,
    pub profil: String,
    pub colonnes: Vec<ColonneVue>,
    /// `devis`, `commande` ou `facture` pour un cas « à partir d'un modèle ».
    pub type_modele: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoixImport {
    pub champ: String,
    pub texte: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidatVue {
    pub id: String,
    pub libelle: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneImportVue {
    pub numero: u32,
    pub valeurs: Vec<String>,
    /// `pret`, `a_preciser`, `introuvable` ou `invalide`.
    pub statut: String,
    pub message: String,
    pub champ: String,
    pub texte: String,
    pub candidats: Vec<CandidatVue>,
    pub document: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentImportVue {
    pub cle: String,
    pub libelle: String,
    pub pret: bool,
    pub doublon: Option<String>,
    pub lignes: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseVue {
    pub cas: String,
    pub fichier: String,
    pub entetes: Vec<String>,
    pub avertissements: Vec<String>,
    pub lignes: Vec<LigneImportVue>,
    pub documents: Vec<DocumentImportVue>,
    pub prets: usize,
    pub bloquantes: usize,
    pub doublons: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCreeVue {
    pub cle: String,
    pub libelle: String,
    /// `cree`, `existant`, `ignore` ou `erreur`.
    pub statut: String,
    pub id: String,
    pub lien: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatImportVue {
    pub documents: Vec<DocumentCreeVue>,
    pub crees: usize,
    pub existants: usize,
    pub erreurs: usize,
    pub ignores: usize,
    pub rapport: String,
}

// --- Diagnostic ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticVue {
    pub texte: String,
    pub dossier_journaux: String,
    pub dossier_documents: String,
}

// --- Listes Excel ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatutVue {
    pub cle: String,
    pub libelle: String,
}

/// Liste exportable et filtres qu'elle accepte.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeVue {
    pub nom: String,
    pub libelle: String,
    pub client: bool,
    pub groupe: bool,
    pub territoire: bool,
    /// Libellé de la date filtrée, si la liste a un filtre de période.
    pub dates: Option<String>,
    pub statuts: Vec<StatutVue>,
    pub vendables: bool,
    /// Ce que « inclure les inactifs » ajoute : désactivés, annulés…
    pub inactifs: String,
    pub colonnes: Vec<String>,
}

/// Export demandé par l'interface ; Rust vérifie chaque filtre.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DemandeListe {
    pub liste: String,
    pub client: Option<String>,
    pub groupe: Option<String>,
    pub territoire: Option<String>,
    pub du: Option<String>,
    pub au: Option<String>,
    pub statut: Option<String>,
    pub texte: Option<String>,
    pub inclure_inactifs: bool,
    pub vendables_seulement: bool,
}

/// Valeurs possibles des filtres « groupe » et « territoire » d'une liste.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencesExportVue {
    pub groupes: Vec<String>,
    pub territoires: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropositionListeVue {
    pub liste: Option<String>,
    pub client: ChoixVue<ClientVue>,
    pub groupe: Option<String>,
    pub territoire: Option<String>,
    pub du: Option<String>,
    pub au: Option<String>,
    pub statut: Option<String>,
    pub texte: Option<String>,
    pub inclure_inactifs: bool,
    pub vendables_seulement: bool,
    pub description: String,
    /// Vrai si le fichier peut être produit tel quel.
    pub pret: bool,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultatListeVue {
    /// Fichier enregistré dans Documents\Dokos\listes.
    pub chemin: String,
    pub lignes: usize,
    pub description: String,
    pub tronque: bool,
    pub avertissements: Vec<String>,
}

// --- Chatbot ----------------------------------------------------------------------------

/// Message de la conversation : `utilisateur` ou `assistant`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageChatVue {
    pub auteur: String,
    pub texte: String,
    /// Fiches Dokos (JSON) reçues par une réponse de l'assistant.
    #[serde(default)]
    pub donnees: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampFicheVue {
    pub libelle: String,
    pub valeur: String,
}

/// Fiche client ou article trouvée par le chatbot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FicheVue {
    /// `client` ou `article`.
    pub genre: String,
    pub id: String,
    pub titre: String,
    pub desactive: bool,
    pub champs: Vec<ChampFicheVue>,
    pub lien: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReponseChatVue {
    pub texte: String,
    pub etapes: Vec<String>,
    pub onglet: Option<String>,
    pub libelle_onglet: Option<String>,
    /// Demande reformulée à pré-remplir dans `onglet`.
    pub pre_remplir: Option<String>,
    pub fiches: Vec<FicheVue>,
    /// Fiches en JSON, à renvoyer avec ce message dans l'historique.
    pub donnees: Option<String>,
    pub avertissements: Vec<String>,
}

// --- Ventes à partir d'un modèle ----------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeModeleVue {
    pub id: String,
    pub titre: String,
    pub total: f64,
    pub devise: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneModeleVue {
    pub index: usize,
    pub code: String,
    pub libelle: String,
    pub quantite: f64,
    pub prix: f64,
    pub unite: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeleVenteVue {
    pub id: String,
    pub titre: String,
    pub devise: String,
    /// Version du modèle, renvoyée à la création pour détecter un modèle changé entre-temps.
    pub modifie: String,
    pub lignes: Vec<LigneModeleVue>,
    pub lien: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantiteModele {
    pub index: usize,
    pub quantite: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeVenteModele {
    /// `devis`, `commande` ou `facture`.
    pub type_vente: String,
    pub modele: String,
    pub modifie: String,
    /// Identifiant Dokos du client.
    pub client: String,
    pub arrivee: String,
    pub depart: String,
    pub quantites: Vec<QuantiteModele>,
}
