// Données lues dans Dokos et gardées en mémoire : affichées tout de suite à
// la visite suivante, puis rafraîchies en arrière-plan si elles ont vieilli.
// Sans DOM ni appel direct à Rust : testé avec Vitest.

export interface EtatRessource<T> {
  /** Dernière valeur lue, gardée pendant un rechargement et après une erreur. */
  valeur: T | undefined;
  /** Erreur du dernier chargement, effacée par le suivant s'il réussit. */
  erreur: unknown;
  /** Heure (ms) de la dernière lecture réussie. */
  date: number | null;
  enCours: boolean;
}

export interface Ressource<T> {
  etat: () => EtatRessource<T>;
  /**
   * Charge si rien n'a été lu, si la valeur a vieilli ou si `forcer`.
   * Deux demandes simultanées ne font qu'un appel.
   */
  charger: (forcer?: boolean) => Promise<void>;
  /** Marque la valeur comme vieillie : le prochain `charger` relira Dokos. */
  perimer: () => void;
  /** Appelle `ecouteur` à chaque changement d'état ; renvoie la fonction de désabonnement. */
  abonner: (ecouteur: () => void) => () => void;
}

export function ressource<T>(
  lire: () => Promise<T>,
  dureeMs = 120_000,
  maintenant: () => number = Date.now,
): Ressource<T> {
  const etat: EtatRessource<T> = { valeur: undefined, erreur: undefined, date: null, enCours: false };
  const ecouteurs = new Set<() => void>();
  let perimee = false;
  let enVol: Promise<void> | null = null;
  const prevenir = () => {
    for (const e of ecouteurs) e();
  };

  function charger(forcer = false): Promise<void> {
    if (enVol) return enVol;
    const fraiche = etat.date !== null && !perimee && maintenant() - etat.date < dureeMs;
    if (fraiche && !forcer && etat.erreur === undefined) return Promise.resolve();
    etat.enCours = true;
    prevenir();
    enVol = lire()
      .then((valeur) => {
        etat.valeur = valeur;
        etat.erreur = undefined;
        etat.date = maintenant();
        perimee = false;
      })
      .catch((e: unknown) => {
        etat.erreur = e ?? new Error("erreur inconnue");
      })
      .finally(() => {
        etat.enCours = false;
        enVol = null;
        prevenir();
      });
    return enVol;
  }

  return {
    etat: () => etat,
    charger,
    perimer: () => {
      perimee = true;
    },
    abonner: (ecouteur) => {
      ecouteurs.add(ecouteur);
      return () => ecouteurs.delete(ecouteur);
    },
  };
}

export interface Memoire<A extends unknown[], R> {
  (...arguments_: A): Promise<R>;
  /** Oublie tout (après une création, par exemple). */
  vider: () => void;
}

/**
 * Garde quelques instants les réponses d'une recherche : effacer une lettre
 * puis la retaper, ou chercher le même client dans deux onglets, ne refait
 * pas de requête. Une recherche qui échoue n'est pas gardée.
 */
export function avecMemoire<A extends unknown[], R>(
  chercher: (...arguments_: A) => Promise<R>,
  dureeMs = 30_000,
  taille = 60,
  maintenant: () => number = Date.now,
): Memoire<A, R> {
  const memoire = new Map<string, { date: number; reponse: Promise<R> }>();
  const fonction = (...arguments_: A): Promise<R> => {
    const cle = JSON.stringify(arguments_).toLowerCase();
    const connue = memoire.get(cle);
    if (connue && maintenant() - connue.date < dureeMs) return connue.reponse;
    const reponse = chercher(...arguments_);
    memoire.delete(cle);
    memoire.set(cle, { date: maintenant(), reponse });
    reponse.catch(() => {
      if (memoire.get(cle)?.reponse === reponse) memoire.delete(cle);
    });
    // La plus ancienne sort quand la mémoire est pleine.
    if (memoire.size > taille) {
      const ancienne = memoire.keys().next().value;
      if (ancienne !== undefined) memoire.delete(ancienne);
    }
    return reponse;
  };
  fonction.vider = () => memoire.clear();
  return fonction;
}

/**
 * Regroupe les demandes faites dans le même instant en un seul appel :
 * choisir trois articles d'affilée ne demande les prix qu'une fois.
 */
export function parLot<C, R>(
  lire: (cles: C[]) => Promise<R[]>,
  cleDe: (resultat: R) => C,
  delaiMs = 40,
): (cle: C) => Promise<R | undefined> {
  let attente: { cle: C; resoudre: (r: R | undefined) => void }[] = [];
  let minuteur: ReturnType<typeof setTimeout> | undefined;
  const envoyer = () => {
    const lot = attente;
    attente = [];
    minuteur = undefined;
    lire([...new Set(lot.map((d) => d.cle))])
      .then((resultats) => {
        const parCle = new Map(resultats.map((r) => [cleDe(r), r]));
        for (const d of lot) d.resoudre(parCle.get(d.cle));
      })
      .catch(() => {
        for (const d of lot) d.resoudre(undefined);
      });
  };
  return (cle) =>
    new Promise((resoudre) => {
      attente.push({ cle, resoudre });
      minuteur ??= setTimeout(envoyer, delaiMs);
    });
}
