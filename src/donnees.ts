// Données de Dokos partagées entre l'accueil, le suivi et le menu : lues une
// fois, montrées partout, rafraîchies quand elles ont vieilli (ressource.ts).

import { api } from "./api";
import { avecMemoire, parLot, ressource } from "./ressource";
import { formaterDate } from "./valeurs";

const jour = () => formaterDate(new Date());

export const donnees = {
  connexions: ressource(() => api.etatConnexions(), 60_000),
  brouillons: ressource(() => api.mesBrouillons()),
  impayes: ressource(() => api.facturesEnRetard(jour())),
  evenements: ressource(() => api.evenementsRecents(jour())),
};

/** Ce que l'accueil et le suivi montrent : tout sauf la liste des événements. */
const SUIVIES = [donnees.brouillons, donnees.impayes] as const;

/** Rafraîchit ce qui a vieilli (ou tout, si `forcer`). */
export function chargerSuivi(forcer = false): Promise<void> {
  return Promise.all(SUIVIES.map((r) => r.charger(forcer))).then(() => undefined);
}

/** Après une création : les brouillons et les factures en retard seront relus à la prochaine visite. */
export function perimerSuivi(): void {
  for (const r of [...SUIVIES, donnees.evenements]) r.perimer();
}

/** Recherches de clients et d'articles par texte (palette, assistant), gardées quelques instants. */
export const recherches = {
  clients: avecMemoire(api.chercherClients),
  articles: avecMemoire(api.chercherArticles),
};

/**
 * Contenu des listes déroulantes : lu en entier dans Dokos à la première
 * ouverture, puis gardé deux minutes (`viderListes` après une création).
 */
const DUREE_LISTES = 120_000;
export const listes = {
  clients: avecMemoire(() => api.listeClients(), DUREE_LISTES, 1),
  /** Articles vendables, ou ceux d'un groupe (sous-groupes compris). */
  articles: avecMemoire((groupe: string | null) => api.listeArticles(groupe), DUREE_LISTES, 40),
  /** Journaux comptables actifs (grand livre). */
  journaux: avecMemoire(() => api.journauxComptables(), DUREE_LISTES, 1),
  /** Groupes et territoires des filtres d'export, par liste. */
  references: avecMemoire((liste: string) => api.referencesExport(liste), DUREE_LISTES, 10),
};

/** Après la création d'un client ou d'un article : les listes seront relues à la prochaine ouverture. */
export function viderListes(): void {
  listes.clients.vider();
  listes.articles.vider();
  listes.references.vider();
  listes.journaux.vider();
}

/** Prix indicatif d'un article ; les demandes rapprochées partent ensemble. */
export const prixIndicatif = parLot(api.prixIndicatifs, (p) => p.article);
