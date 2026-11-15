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
  rh: ressource(() => api.alertesRh(jour())),
  evenements: ressource(() => api.evenementsRecents(jour())),
};

/** Ce que l'accueil et le suivi montrent : tout sauf la liste des événements. */
const SUIVIES = [donnees.brouillons, donnees.impayes, donnees.rh] as const;

/** Rafraîchit ce qui a vieilli (ou tout, si `forcer`). */
export function chargerSuivi(forcer = false): Promise<void> {
  return Promise.all(SUIVIES.map((r) => r.charger(forcer))).then(() => undefined);
}

/** Après une création : les brouillons et alertes seront relus à la prochaine visite. */
export function perimerSuivi(): void {
  for (const r of [...SUIVIES, donnees.evenements]) r.perimer();
}

/** Recherches de clients et d'articles, gardées quelques instants. */
export const recherches = {
  clients: avecMemoire(api.chercherClients),
  articles: avecMemoire(api.chercherArticles),
};

/** Prix indicatif d'un article ; les demandes rapprochées partent ensemble. */
export const prixIndicatif = parLot(api.prixIndicatifs, (p) => p.article);
