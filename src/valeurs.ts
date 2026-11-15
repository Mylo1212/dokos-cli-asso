// Fonctions pures, sans DOM : lecture des saisies, construction des demandes,
// mise en forme. Testées avec Vitest ; l'interface ne fait que les brancher.

import type {
  DemandeContrat,
  DemandeEvenement,
  DemandeListe,
  DemandeVente,
  DemandeVenteModele,
  ErreurVue,
  ListeVue,
  TypeVente,
} from "./api";

/** Nombre avec virgule ou point ; `null` si vide, `NaN` si illisible. */
export function nombreDepuisTexte(texte: string): number | null {
  const propre = texte.trim().replace(/\s/g, "").replace(",", ".");
  if (propre === "") return null;
  const n = Number(propre);
  return Number.isFinite(n) ? n : NaN;
}

/** Texte sans espaces autour, ou `null` s'il est vide. */
export function texteOuNull(texte: string): string | null {
  const propre = texte.trim();
  return propre === "" ? null : propre;
}

/** `AAAA-MM-JJ` d'une date locale. */
export function formaterDate(d: Date): string {
  const deux = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${deux(d.getMonth() + 1)}-${deux(d.getDate())}`;
}

/** `JJ/MM/AAAA` à partir d'un texte `AAAA-MM-JJ` (le reste est rendu tel quel). */
export function dateLisible(texte: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(texte);
  return m ? `${m[3]}/${m[2]}/${m[1]}` : texte;
}

export function montant(total: number, devise: string): string {
  try {
    return new Intl.NumberFormat("fr-FR", { style: "currency", currency: devise || "EUR" }).format(total);
  } catch {
    return `${total.toFixed(2)} ${devise}`;
  }
}

/** Erreur structurée venant de Rust, ou texte brut si ce n'en est pas une. */
export function erreurVue(erreur: unknown): ErreurVue {
  if (typeof erreur === "object" && erreur !== null && "genre" in erreur && "message" in erreur) {
    const e = erreur as { genre: unknown; message: unknown };
    return { genre: String(e.genre), message: String(e.message) };
  }
  if (typeof erreur === "string") return { genre: "autre", message: erreur };
  if (erreur instanceof Error) return { genre: "autre", message: erreur.message };
  return { genre: "autre", message: String(erreur) };
}

export function message(erreur: unknown): string {
  return erreurVue(erreur).message;
}

/** Action que l'interface peut proposer selon le genre de l'erreur. */
export type ActionErreur = "reessayer" | "reglages" | "aucune";

export function actionPourErreur(genre: string): ActionErreur {
  switch (genre) {
    case "reseau":
      return "reessayer";
    case "authentification":
    case "configuration":
    case "permission":
    case "ia":
      return "reglages";
    default:
      return "aucune";
  }
}

export function titreErreur(genre: string): string {
  switch (genre) {
    case "reseau":
      return "Dokos injoignable";
    case "authentification":
      return "Clé ou secret refusés";
    case "permission":
      return "Droit manquant dans Dokos";
    case "configuration":
      return "Réglage à revoir";
    case "validation":
      return "Demande refusée";
    case "introuvable":
      return "Introuvable dans Dokos";
    case "doublon":
      return "Document déjà présent";
    case "ia":
      return "Assistant IA";
    case "fichier":
      return "Fichier";
    default:
      return "Erreur";
  }
}

// --- Demandes ---------------------------------------------------------------

export interface LigneSaisie {
  article: string | null;
  quantite: string;
  livraison: string;
}

export interface SaisieVente {
  typeVente: TypeVente;
  client: string | null;
  date: string;
  echeance: string;
  reference: string;
  lignes: LigneSaisie[];
}

export const LIBELLES_VENTE: Record<
  TypeVente,
  { nom: string; date: string; echeance: string | null; article: string }
> = {
  devis: { nom: "Devis", date: "Date du devis", echeance: "Valable jusqu'au", article: "un devis" },
  commande: { nom: "Commande", date: "Date de livraison", echeance: null, article: "une commande" },
  facture: { nom: "Facture", date: "Date de facture", echeance: "Échéance", article: "une facture" },
};

/** Demande de vente, ou le message à afficher si la saisie est incomplète. */
export function demandeVente(saisie: SaisieVente): DemandeVente | string {
  const type = LIBELLES_VENTE[saisie.typeVente];
  if (!saisie.client) return "Choisissez un client dans la liste.";
  if (!saisie.date) return `Renseignez la ${type.date.toLowerCase()}.`;
  if (saisie.lignes.length === 0) return "Ajoutez au moins une ligne.";
  const lignes: DemandeVente["lignes"] = [];
  for (const [i, ligne] of saisie.lignes.entries()) {
    if (!ligne.article) return `Ligne ${i + 1} : choisissez un article dans la liste.`;
    const quantite = nombreDepuisTexte(ligne.quantite);
    if (quantite === null || Number.isNaN(quantite) || quantite <= 0) return `Ligne ${i + 1} : quantité invalide.`;
    lignes.push({
      article: ligne.article,
      quantite,
      livraison: saisie.typeVente === "commande" ? texteOuNull(ligne.livraison) : null,
    });
  }
  return {
    typeVente: saisie.typeVente,
    client: saisie.client,
    date: saisie.date,
    echeance: type.echeance ? texteOuNull(saisie.echeance) : null,
    lignes,
    reference: saisie.typeVente === "devis" ? null : texteOuNull(saisie.reference),
  };
}

/** Vrai si une commande est livrée avant aujourd'hui : Dokos la refusera. */
export function livraisonPassee(typeVente: TypeVente, date: string, aujourdhui: string): boolean {
  return typeVente === "commande" && date !== "" && date < aujourdhui;
}

export interface SaisieEvenement {
  titre: string;
  debut: string;
  fin: string;
  journee: boolean;
  lieu: string;
  description: string;
  places: string;
  annulation: boolean;
  questions: { champ: string; obligatoire: boolean }[];
}

export function demandeEvenement(saisie: SaisieEvenement): DemandeEvenement | string {
  const titre = texteOuNull(saisie.titre);
  if (!titre) return "Donnez un titre à l'événement.";
  if (!saisie.debut) return "Renseignez le début.";
  const places = nombreDepuisTexte(saisie.places);
  if (places !== null && (Number.isNaN(places) || places < 1 || !Number.isInteger(places))) {
    return "Nombre de places invalide.";
  }
  return {
    titre,
    debut: saisie.debut,
    fin: texteOuNull(saisie.fin),
    journee: saisie.journee,
    lieu: texteOuNull(saisie.lieu),
    description: texteOuNull(saisie.description),
    places,
    annulation: saisie.annulation,
    questions: saisie.questions,
  };
}

export interface SaisieContrat {
  employe: string | null;
  typeContrat: string;
  poste: string;
  entree: string;
  fin: string;
  heures: string;
  repartition: string[];
  champs: Record<string, string>;
  format: string;
}

export function demandeContrat(saisie: SaisieContrat): DemandeContrat | string {
  if (!saisie.employe) return "Choisissez ou créez l'employé.";
  if (!saisie.typeContrat) return "Choisissez le type de contrat.";
  if (!texteOuNull(saisie.poste)) return "Renseignez le poste.";
  if (!saisie.entree) return "Renseignez la date d'entrée.";
  const heures = nombreDepuisTexte(saisie.heures);
  if (heures !== null && Number.isNaN(heures)) return "Heures hebdomadaires invalides.";
  const repartition = saisie.repartition.map((j) => nombreDepuisTexte(j) ?? 0);
  if (repartition.some(Number.isNaN)) return "Répartition des heures invalide.";
  const champs: Record<string, string> = {};
  for (const [nom, valeur] of Object.entries(saisie.champs)) {
    if (valeur.trim()) champs[nom] = valeur.trim();
  }
  return {
    employe: saisie.employe,
    typeContrat: saisie.typeContrat,
    poste: saisie.poste.trim(),
    entree: saisie.entree,
    fin: texteOuNull(saisie.fin),
    heures,
    repartition: repartition.some((v) => v > 0) ? repartition : null,
    champs,
    format: texteOuNull(saisie.format),
  };
}

/** Total des heures saisies par jour. */
export function totalHeures(jours: string[]): number {
  return jours.reduce((somme, j) => {
    const n = nombreDepuisTexte(j);
    return somme + (n === null || Number.isNaN(n) ? 0 : n);
  }, 0);
}

/** Identifiant d'un choix d'ambiguïté d'import : même clé que côté Rust. */
export function cleChoixImport(champ: string, texte: string): string {
  return `${champ}|${texte.trim().toLowerCase()}`;
}

/** Saisies de l'onglet Listes, telles qu'elles sont à l'écran. */
export interface SaisieListe {
  client: string | null;
  groupe: string;
  territoire: string;
  du: string;
  au: string;
  statut: string;
  texte: string;
  inclureInactifs: boolean;
  vendablesSeulement: boolean;
}

/**
 * Demande d'export : seuls les filtres que la liste accepte sont envoyés
 * (un champ masqué garde sa valeur à l'écran sans être pris en compte).
 */
export function demandeListe(liste: ListeVue, s: SaisieListe): DemandeListe | string {
  const du = liste.dates ? texteOuNull(s.du) : null;
  const au = liste.dates ? texteOuNull(s.au) : null;
  if (du && au && du > au) return "La date de début est après la date de fin.";
  const statut = texteOuNull(s.statut);
  return {
    liste: liste.nom,
    client: liste.client ? s.client : null,
    groupe: liste.groupe ? texteOuNull(s.groupe) : null,
    territoire: liste.territoire ? texteOuNull(s.territoire) : null,
    du,
    au,
    statut: statut && liste.statuts.some((st) => st.cle === statut) ? statut : null,
    texte: texteOuNull(s.texte),
    inclureInactifs: s.inclureInactifs,
    vendablesSeulement: liste.vendables && s.vendablesSeulement,
  };
}

/** Dossier d'un chemin de fichier, Windows ou Unix. */
export function dossierDe(chemin: string): string {
  const i = Math.max(chemin.lastIndexOf("\\"), chemin.lastIndexOf("/"));
  return i > 0 ? chemin.slice(0, i) : chemin;
}

/** Saisies du mode « à partir d'un modèle » de l'onglet Ventes. */
export interface SaisieVenteModele {
  typeVente: TypeVente;
  modele: { id: string; modifie: string } | null;
  client: string | null;
  arrivee: string;
  depart: string;
  /** Quantité saisie par ligne du modèle ; vide ou 0 : ligne retirée. */
  quantites: { index: number; texte: string }[];
}

/** Demande de création à partir d'un modèle, ou le message de ce qui manque. */
export function demandeVenteModele(s: SaisieVenteModele): DemandeVenteModele | string {
  if (!s.modele) return "Choisissez un modèle.";
  if (!s.client) return "Choisissez le client dans la liste.";
  if (!s.arrivee || !s.depart) return "Renseignez les dates d'arrivée et de départ.";
  if (s.depart < s.arrivee) return "La date de départ est avant la date d'arrivée.";
  const quantites: { index: number; quantite: number | null }[] = [];
  for (const q of s.quantites) {
    const n = nombreDepuisTexte(q.texte);
    if (n !== null && (Number.isNaN(n) || n < 0)) return `Quantité « ${q.texte} » invalide.`;
    quantites.push({ index: q.index, quantite: n === 0 ? null : n });
  }
  if (!quantites.some((q) => q.quantite !== null)) return "Saisissez au moins une quantité.";
  return {
    typeVente: s.typeVente,
    modele: s.modele.id,
    modifie: s.modele.modifie,
    client: s.client,
    arrivee: s.arrivee,
    depart: s.depart,
    quantites,
  };
}

// --- Recherche et résumés -------------------------------------------------

/** Minuscules sans accents, pour comparer ce qui est tapé à un libellé. */
export function sansAccents(texte: string): string {
  return texte.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/** Vrai si chaque mot de `recherche` se trouve dans `texte` (accents et casse ignorés). */
export function correspond(texte: string, recherche: string): boolean {
  const cible = sansAccents(texte);
  return sansAccents(recherche)
    .split(/\s+/)
    .filter(Boolean)
    .every((mot) => cible.includes(mot));
}

/** Ligne d'un document en cours de saisie, avec son prix indicatif s'il est connu. */
export interface LigneResumee {
  article: string | null;
  quantite: string;
  prix: { prix: number; devise: string } | null;
}

/** Résumé d'une vente en cours de saisie : ce que la barre d'action affiche. */
export interface ResumeVente {
  /** Lignes dont l'article est choisi. */
  lignes: number;
  /** Total indicatif des lignes dont le prix est connu ; `null` si aucun prix. */
  total: number | null;
  devise: string;
  /** Lignes choisies sans prix connu ou sans quantité lisible : non comptées dans le total. */
  horsTotal: number;
}

export function resumeVente(lignes: LigneResumee[]): ResumeVente {
  const choisies = lignes.filter((l) => l.article !== null);
  let total = 0;
  let comptees = 0;
  let devise = "";
  for (const ligne of choisies) {
    const quantite = nombreDepuisTexte(ligne.quantite);
    if (!ligne.prix || quantite === null || Number.isNaN(quantite) || quantite <= 0) continue;
    // Un total ne mélange pas deux devises : seules les lignes de la première comptent.
    if (devise !== "" && ligne.prix.devise !== devise) continue;
    devise = ligne.prix.devise;
    total += quantite * ligne.prix.prix;
    comptees += 1;
  }
  return {
    lignes: choisies.length,
    total: comptees > 0 ? total : null,
    devise,
    horsTotal: choisies.length - comptees,
  };
}

/** « 1 ligne », « 3 lignes »… */
export function pluriel(nombre: number, singulier: string, plurielTexte = `${singulier}s`): string {
  return `${nombre} ${nombre > 1 ? plurielTexte : singulier}`;
}

/** Heure `HH:MM` d'un instant (ms), pour « actualisé à … ». */
export function heureCourte(instant: number): string {
  const d = new Date(instant);
  const deux = (n: number) => String(n).padStart(2, "0");
  return `${deux(d.getHours())}:${deux(d.getMinutes())}`;
}
