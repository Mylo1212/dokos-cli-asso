// Appels aux commandes Rust (src-tauri/src/commandes.rs). L'interface
// n'envoie que des demandes : Rust valide tout et construit les requêtes.

import { invoke } from "@tauri-apps/api/core";

export interface ProfilVue {
  profil: string;
  libelle: string;
  cle: string | null;
  roles: string;
}

export interface EtatConfig {
  configure: boolean;
  url: string;
  autoriserHttp: boolean;
  societe: string;
  profils: ProfilVue[];
  cheminConfig: string;
}

/** État de la connexion d'un profil à Dokos. */
export interface ConnexionVue {
  profil: string;
  libelle: string;
  etat: "ok" | "erreur" | "non_configure";
  utilisateur: string;
  genre: string;
  message: string;
}

export interface ClientVue {
  id: string;
  nom: string;
}

export interface ArticleVue {
  code: string;
  libelle: string;
  unite: string;
  groupe: string;
}

/** Erreur structurée renvoyée par les commandes Rust. */
export interface ErreurVue {
  genre: string;
  message: string;
}

export type TypeVente = "devis" | "commande" | "facture";

export interface DemandeVente {
  typeVente: TypeVente;
  client: string;
  date: string;
  echeance: string | null;
  lignes: { article: string; quantite: number; livraison: string | null; prix: number | null }[];
  reference: string | null;
}

export interface DocumentVue {
  id: string;
  statut: string;
  date: string;
  total: number;
  devise: string;
  lien: string;
}

export interface ApercuVente {
  identiques: DocumentVue[];
  similaires: DocumentVue[];
}

export interface ResultatVente {
  cree: boolean;
  document: DocumentVue;
  autres: DocumentVue[];
}

export interface ChampVue {
  champ: string;
  libelle: string;
  typeChamp: string;
  obligatoire: boolean;
}

export interface DemandeEvenement {
  titre: string;
  debut: string;
  fin: string | null;
  journee: boolean;
  lieu: string | null;
  description: string | null;
  places: number | null;
  annulation: boolean;
  questions: { champ: string; obligatoire: boolean }[];
}

export interface ApercuEvenement {
  formulaireTitre: string;
  formulaireRoute: string;
  champs: ChampVue[];
  existant: DocumentVue | null;
}

export interface ResultatEvenement {
  cree: boolean;
  evenement: DocumentVue;
  formulaire: string;
  formulaireReutilise: boolean;
}

export interface PublicationVue {
  page: string;
  inscription: string;
}

export interface EtatIa {
  active: boolean;
  url: string;
  modele: string;
  basesVentes: string;
  basesFormulaires: string;
  cleEnregistree: boolean;
}

/** Élément nommé par l'IA, après recherche dans Dokos. */
export interface ChoixVue<T> {
  statut: "non_precise" | "trouve" | "a_preciser" | "introuvable";
  texte: string;
  valeur: T | null;
  candidats: T[];
}

export interface Remarques {
  questions: string[];
  avertissements: string[];
}

export interface PropositionVente extends Remarques {
  typeVente: TypeVente | null;
  client: ChoixVue<ClientVue>;
  date: string | null;
  echeance: string | null;
  lignes: { texte: string; article: ChoixVue<ArticleVue>; quantite: number | null; livraison: string | null }[];
}

export interface PropositionEvenement extends Remarques {
  titre: string | null;
  debut: string | null;
  fin: string | null;
  journee: boolean;
  lieu: string | null;
  description: string | null;
  places: number | null;
  annulation: boolean;
  questionsFormulaire: { champ: string; obligatoire: boolean }[];
  champsACreer: string[];
}

// --- Catalogue ---

export interface NoeudVue {
  id: string;
  groupe: boolean;
  enfants: NoeudVue[];
}

export interface CatalogueVue {
  arbre: NoeudVue[];
  listeDePrix: string | null;
  unites: string[];
}

export interface PrixVue {
  article: string;
  prix: number;
  devise: string;
}

export interface DemandeArticle {
  code: string;
  libelle: string;
  groupe: string;
  unite: string;
  vendable: boolean;
  stocke: boolean;
  description: string | null;
}

export interface ResultatArticle {
  cree: boolean;
  code: string;
  libelle: string;
  actif: boolean;
  lien: string;
}

// --- Suivi ---

export interface BrouillonVue {
  genre: "devis" | "commande" | "facture" | "evenement";
  id: string;
  tiers: string;
  date: string;
  total: number;
  devise: string;
  modifie: string;
  lien: string;
}

export interface BrouillonsVue {
  ventes: BrouillonVue[];
  evenements: BrouillonVue[];
  avertissements: string[];
}

export interface EvenementVue {
  id: string;
  titre: string;
  debut: string;
  publie: boolean;
  lien: string;
}

export interface InscritsVue {
  evenement: string;
  colonnes: ChampVue[];
  lignes: Record<string, string>[];
}

export interface FactureRetardVue {
  id: string;
  client: string;
  date: string;
  echeance: string;
  total: number;
  solde: number;
  devise: string;
  retard: number;
  lien: string;
}

export interface ImpayesVue {
  factures: FactureRetardVue[];
  parClient: { client: string; solde: number; nombre: number }[];
  total: number;
  devise: string;
}

// --- Import ---

export interface CasVue {
  nom: string;
  libelle: string;
  resultat: string;
  profil: string;
  colonnes: { nom: string; obligatoire: boolean; aide: string }[];
}

export interface ChoixImport {
  champ: string;
  texte: string;
  id: string;
}

export interface LigneImportVue {
  numero: number;
  valeurs: string[];
  statut: "pret" | "a_preciser" | "introuvable" | "invalide";
  message: string;
  champ: string;
  texte: string;
  candidats: { id: string; libelle: string }[];
  document: number | null;
}

export interface AnalyseVue {
  cas: string;
  fichier: string;
  entetes: string[];
  avertissements: string[];
  lignes: LigneImportVue[];
  documents: { cle: string; libelle: string; pret: boolean; doublon: string | null; lignes: number[] }[];
  prets: number;
  bloquantes: number;
  doublons: number;
}

export interface ResultatImportVue {
  documents: {
    cle: string;
    libelle: string;
    statut: "cree" | "existant" | "ignore" | "erreur";
    id: string;
    lien: string;
    message: string;
  }[];
  crees: number;
  existants: number;
  erreurs: number;
  ignores: number;
  rapport: string;
}

/** Journal comptable de Dokos, pour la liste déroulante du grand livre. */
export interface JournalVue {
  /** Identifiant Dokos (« VEN - SLA »). */
  id: string;
  code: string;
  libelle: string;
  typeJournal: string;
}

/** Valeur de la colonne JOURNAL du fichier du grand livre. */
export type JournalFichier = "VEN" | "REC";

export interface DemandeGrandLivre {
  journal: string;
  du: string;
  au: string;
  /** Colonne JOURNAL du fichier final. */
  journalFichier: JournalFichier;
}

export interface ResultatGrandLivre {
  chemin: string;
  lignes: number;
  totalDebit: number;
  totalCredit: number;
  /** Écritures non exportées (compte sans numéro, date illisible). */
  ecartees: string[];
}

/** Valeurs possibles des filtres « groupe » et « territoire » d'une liste. */
export interface ReferencesExport {
  groupes: string[];
  territoires: string[];
}

export interface StatutListe {
  cle: string;
  libelle: string;
}

/** Liste exportable en Excel et filtres qu'elle accepte. */
export interface ListeVue {
  nom: string;
  libelle: string;
  client: boolean;
  groupe: boolean;
  territoire: boolean;
  dates: string | null;
  statuts: StatutListe[];
  vendables: boolean;
  inactifs: string;
  colonnes: string[];
}

export interface DemandeListe {
  liste: string;
  client: string | null;
  groupe: string | null;
  territoire: string | null;
  du: string | null;
  au: string | null;
  statut: string | null;
  texte: string | null;
  inclureInactifs: boolean;
  vendablesSeulement: boolean;
}

export interface PropositionListe extends Remarques {
  liste: string | null;
  client: ChoixVue<ClientVue>;
  groupe: string | null;
  territoire: string | null;
  du: string | null;
  au: string | null;
  statut: string | null;
  texte: string | null;
  inclureInactifs: boolean;
  vendablesSeulement: boolean;
  description: string;
  pret: boolean;
}

export interface ResultatListe {
  chemin: string;
  lignes: number;
  description: string;
  tronque: boolean;
  avertissements: string[];
}

/** Message du chatbot, tel qu'il est renvoyé à Rust à chaque tour. */
export interface MessageChat {
  auteur: "utilisateur" | "assistant";
  texte: string;
  /** Fiches Dokos (JSON) reçues par cette réponse de l'assistant. */
  donnees?: string | null;
}

/** Fiche client ou article trouvée par le chatbot. */
export interface FicheVue {
  genre: "client" | "article";
  id: string;
  titre: string;
  desactive: boolean;
  champs: { libelle: string; valeur: string }[];
  lien: string;
}

export interface ResumeModele {
  id: string;
  titre: string;
  total: number;
  devise: string;
}

export interface LigneModele {
  index: number;
  code: string;
  libelle: string;
  quantite: number;
  prix: number;
  unite: string;
}

/** Devis, commande ou facture « Template » de Dokos. */
export interface ModeleVente {
  id: string;
  titre: string;
  devise: string;
  modifie: string;
  lignes: LigneModele[];
  lien: string;
}

export interface DemandeVenteModele {
  typeVente: TypeVente;
  modele: string;
  modifie: string;
  client: string;
  arrivee: string;
  depart: string;
  quantites: { index: number; quantite: number | null }[];
}

export interface ReponseChat {
  texte: string;
  etapes: string[];
  onglet: string | null;
  libelleOnglet: string | null;
  /** Demande reformulée à pré-remplir dans `onglet`. */
  preRemplir: string | null;
  fiches: FicheVue[];
  donnees: string | null;
  avertissements: string[];
}

export interface DiagnosticVue {
  texte: string;
  dossierJournaux: string;
  dossierDocuments: string;
}

export const api = {
  etatConfig: () => invoke<EtatConfig>("etat_config"),
  enregistrerConfig: (url: string, autoriserHttp: boolean, societe: string) =>
    invoke<void>("enregistrer_config", { url, autoriserHttp, societe }),
  enregistrerProfil: (profilNom: string, cle: string, secret: string) =>
    invoke<string>("enregistrer_profil", { profilNom, cle, secret }),
  testerProfil: (profilNom: string) => invoke<string>("tester_profil", { profilNom }),
  etatConnexions: () => invoke<ConnexionVue[]>("etat_connexions"),

  /** Tous les clients actifs, pour les listes déroulantes. */
  listeClients: () => invoke<ClientVue[]>("liste_clients"),
  /** Tous les articles vendables, ou ceux d'un groupe et de ses sous-groupes. */
  listeArticles: (groupe: string | null = null) => invoke<ArticleVue[]>("liste_articles", { groupe }),
  chercherClients: (recherche: string) => invoke<ClientVue[]>("chercher_clients", { recherche }),
  chercherArticles: (recherche: string, groupe: string | null = null) =>
    invoke<ArticleVue[]>("chercher_articles", { recherche, groupe }),
  diagnostic: () => invoke<DiagnosticVue>("diagnostic"),

  catalogue: () => invoke<CatalogueVue>("catalogue"),
  articlesDuGroupe: (groupe: string, recherche: string | null = null) =>
    invoke<ArticleVue[]>("articles_du_groupe", { groupe, recherche }),
  prixIndicatifs: (codes: string[]) => invoke<PrixVue[]>("prix_indicatifs", { codes }),
  creerArticle: (demande: DemandeArticle, autoriserDoublon: boolean) =>
    invoke<ResultatArticle>("creer_article", { demande, autoriserDoublon }),
  activerArticle: (code: string) => invoke<ResultatArticle>("activer_article", { code }),
  deplacerArticle: (code: string, groupe: string) => invoke<ResultatArticle>("deplacer_article", { code, groupe }),
  creerGroupeArticles: (nom: string, parent: string, conteneur: boolean) =>
    invoke<{ cree: boolean; id: string }>("creer_groupe_articles", { nom, parent, conteneur }),

  mesBrouillons: () => invoke<BrouillonsVue>("mes_brouillons"),
  transformerDocument: (
    genre: "devis_en_commande" | "commande_en_facture",
    source: string,
    autoriserDoublon: boolean,
  ) => invoke<{ cree: boolean; document: DocumentVue }>("transformer_document", { genre, source, autoriserDoublon }),
  evenementsRecents: (aujourdhuiTexte: string) => invoke<EvenementVue[]>("evenements_recents", { aujourdhuiTexte }),
  inscritsEvenement: (id: string) => invoke<InscritsVue>("inscrits_evenement", { id }),
  exporterInscrits: (id: string) => invoke<string>("exporter_inscrits", { id }),
  facturesEnRetard: (aujourdhuiTexte: string) => invoke<ImpayesVue>("factures_en_retard", { aujourdhuiTexte }),

  listesExportables: () => invoke<ListeVue[]>("listes_exportables"),
  journauxComptables: () => invoke<JournalVue[]>("journaux_comptables"),
  exporterGrandLivre: (demande: DemandeGrandLivre) => invoke<ResultatGrandLivre>("exporter_grand_livre", { demande }),
  /** Groupes et territoires que les filtres de la liste acceptent. */
  referencesExport: (liste: string) => invoke<ReferencesExport>("references_export", { liste }),
  assistantListe: (demande: string, aujourdhui: string) =>
    invoke<PropositionListe>("assistant_liste", { demande, aujourdhui }),
  exporterListe: (demande: DemandeListe, aujourdhui: string) =>
    invoke<ResultatListe>("exporter_liste", { demande, aujourdhui }),

  casImport: () => invoke<CasVue[]>("cas_import"),
  genererModele: (casNom: string) => invoke<string>("generer_modele", { casNom }),
  choisirFichier: () => invoke<string | null>("choisir_fichier"),
  analyserImport: (casNom: string, chemin: string, choix: ChoixImport[]) =>
    invoke<AnalyseVue>("analyser_import", { casNom, chemin, choix }),
  creerImport: (
    casNom: string,
    chemin: string,
    choix: ChoixImport[],
    seulementPrets: boolean,
    autoriserDoublon: boolean,
  ) => invoke<ResultatImportVue>("creer_import", { casNom, chemin, choix, seulementPrets, autoriserDoublon }),

  verifierVente: (demande: DemandeVente) => invoke<ApercuVente>("verifier_vente", { demande }),
  creerVente: (demande: DemandeVente, autoriserDoublon: boolean) =>
    invoke<ResultatVente>("creer_vente", { demande, autoriserDoublon }),
  modelesVente: (typeVente: TypeVente) => invoke<ResumeModele[]>("modeles_vente", { typeVente }),
  modeleVente: (typeVente: TypeVente, id: string) => invoke<ModeleVente>("modele_vente", { typeVente, id }),
  verifierVenteModele: (demande: DemandeVenteModele) => invoke<ApercuVente>("verifier_vente_modele", { demande }),
  creerVenteModele: (demande: DemandeVenteModele, autoriserDoublon: boolean) =>
    invoke<ResultatVente>("creer_vente_modele", { demande, autoriserDoublon }),

  questionsFormulaire: () => invoke<ChampVue[]>("questions_formulaire"),
  verifierEvenement: (demande: DemandeEvenement) => invoke<ApercuEvenement>("verifier_evenement", { demande }),
  creerEvenement: (demande: DemandeEvenement, autoriserDoublon: boolean) =>
    invoke<ResultatEvenement>("creer_evenement", { demande, autoriserDoublon }),
  publierEvenement: (id: string) => invoke<PublicationVue>("publier_evenement", { id }),

  etatIa: () => invoke<EtatIa>("etat_ia"),
  enregistrerIa: (
    active: boolean,
    url: string,
    modele: string,
    basesVentes: string,
    basesFormulaires: string,
    cle: string,
  ) => invoke<string>("enregistrer_ia", { active, url, modele, basesVentes, basesFormulaires, cle }),
  testerIa: () => invoke<string>("tester_ia"),
  discuter: (messages: MessageChat[], onglet: string, aujourdhui: string) =>
    invoke<ReponseChat>("discuter", { messages, onglet, aujourdhui }),
  assistantVente: (demande: string, aujourdhui: string) =>
    invoke<PropositionVente>("assistant_vente", { demande, aujourdhui }),
  assistantEvenement: (demande: string, aujourdhui: string) =>
    invoke<PropositionEvenement>("assistant_evenement", { demande, aujourdhui }),

  ouvrirLien: (url: string) => invoke<void>("ouvrir_lien", { url }),
  ouvrirFichier: (chemin: string) => invoke<void>("ouvrir_fichier", { chemin }),
};
