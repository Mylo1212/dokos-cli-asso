// Export du grand livre : un journal comptable de Dokos (liste déroulante) et
// une période. Rust lit les écritures (lecture seule), les met au format
// d'import (DATE, JOURNAL, GENERAL, AUXILIAIRE, DEBIT, CREDIT, LIBELLE,
// REFINTERNE) et écrit le fichier Excel dans Documents\Dokos\exports : plus
// besoin de passer par une macro après l'export.

import { api, type JournalVue } from "./api";
import { alerte, bouton, champ, h, lienFichier, listeDeroulante, occupe, saisie } from "./dom";
import { listes } from "./donnees";
import { enregistrerPreference, preference } from "./etat";
import { demandeGrandLivre, dossierDe, moisPrecedent, nombreFrancais, pluriel } from "./valeurs";

const PREFERENCE_JOURNAL = "grand-livre.journal";
/** Écritures écartées montrées au plus (le fichier n'en tient aucune). */
const ECARTEES_MONTREES = 10;

export interface GrandLivre {
  carte: HTMLDivElement;
  /** Appelée quand la carte est montrée : retrouve le dernier journal choisi. */
  afficher: () => void;
}

export function creerGrandLivre(): GrandLivre {
  let journal: JournalVue | null = null;
  const choixJournal = listeDeroulante<JournalVue>({
    placeholder: "Choisir un journal comptable…",
    lire: () => listes.journaux(),
    libelle: (j) => (j.typeJournal ? `${j.libelle} · ${j.typeJournal}` : j.libelle),
    identifiant: (j) => j.id,
    surChoix: (j) => {
      journal = j;
      if (j) enregistrerPreference(PREFERENCE_JOURNAL, j.id);
    },
  });
  const periode = moisPrecedent(new Date());
  const du = saisie({ type: "date", value: periode.du });
  const au = saisie({ type: "date", value: periode.au });
  const resultat = h("div", { "aria-live": "polite" });
  const exporter = bouton("Créer le fichier Excel", "principal", () => lancerExport());

  async function lancerExport() {
    const demande = demandeGrandLivre({ journal: journal?.id ?? null, du: du.value, au: au.value });
    if (typeof demande === "string") {
      resultat.replaceChildren(alerte("attention", "À compléter", h("p", {}, demande)));
      return;
    }
    resultat.replaceChildren(h("p", { class: "note sans-marge" }, "Lecture des écritures dans Dokos…"));
    const r = await occupe([exporter], resultat, () => api.exporterGrandLivre(demande));
    if (!r) return;
    const aEcarter = r.ecartees.length > 0;
    resultat.replaceChildren(
      alerte(
        r.lignes === 0 || aEcarter ? "attention" : "succes",
        r.lignes === 0
          ? "Aucune écriture sur cette période : fichier vide créé"
          : `${pluriel(r.lignes, "ligne")} exportée${r.lignes > 1 ? "s" : ""}`,
        h("p", {}, `Débit ${nombreFrancais(r.totalDebit)} · crédit ${nombreFrancais(r.totalCredit)}`),
        h("p", { class: "chemin" }, r.chemin),
        ...(aEcarter
          ? [
              h("p", {}, `${pluriel(r.ecartees.length, "cas")} non exporté${r.ecartees.length > 1 ? "s" : ""} :`),
              ...r.ecartees.slice(0, ECARTEES_MONTREES).map((e) => h("p", { class: "reference" }, e)),
              ...(r.ecartees.length > ECARTEES_MONTREES
                ? [h("p", { class: "reference" }, `… et ${r.ecartees.length - ECARTEES_MONTREES} autres`)]
                : []),
            ]
          : []),
        h(
          "div",
          { class: "actions" },
          lienFichier(r.chemin, "Ouvrir le fichier"),
          lienFichier(dossierDe(r.chemin), "Ouvrir le dossier"),
        ),
      ),
    );
  }

  const carte = h(
    "div",
    { class: "carte", hidden: true },
    h("h2", {}, "Grand livre"),
    h(
      "div",
      { class: "grille" },
      champ("Journal comptable", choixJournal.racine, "large"),
      champ("Du", du),
      champ("Au", au),
    ),
    h(
      "p",
      { class: "note" },
      "Une ligne par écriture, au format d'import : DATE, JOURNAL (toujours VEN), GENERAL (numéro du compte), AUXILIAIRE (ERP sur le compte 411), DEBIT, CREDIT, LIBELLE (tiers), REFINTERNE (pièce). Les écritures annulées et les à-nouveaux sont exclus ; les comptes de tiers sont en valeurs nettes. Lecture seule dans Dokos.",
    ),
    h("div", { class: "actions" }, exporter),
    resultat,
  );

  return {
    carte,
    afficher: () => {
      if (journal) return;
      // Retrouve le dernier journal choisi, sans bloquer l'écran si Dokos ne répond pas.
      const dernier = preference(PREFERENCE_JOURNAL, "");
      if (!dernier) return;
      void listes
        .journaux()
        .then((tous) => {
          const trouve = tous.find((j) => j.id === dernier);
          if (trouve && !journal) choixJournal.choisir(trouve);
        })
        .catch(() => undefined);
    },
  };
}
