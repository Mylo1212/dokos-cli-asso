// Pré-remplissage des formulaires par l'IA, déclenché depuis le chatbot.
// Chaque onglet enregistre sa fonction ; le chatbot ouvre l'onglet et
// l'appelle avec la demande reformulée. Rien n'est créé ici : l'utilisateur
// relit le formulaire.

import type { ChoixVue, Remarques } from "./api";
import { alerte, bouton, h } from "./dom";
import type { Onglet } from "./etat";

/** Textes du message affiché dans le chatbot après le pré-remplissage. */
export interface TextesPreRemplissage {
  titreSucces: string;
  texteSucces: string;
}

export interface PreRemplissage extends TextesPreRemplissage {
  /** Remplit le formulaire ; `ajouter` place un élément (choix à faire…) dans le fil du chatbot. */
  lancer: (demande: string, jour: string, ajouter: (element: HTMLElement) => void) => Promise<Remarques>;
}

const TEXTES_FORMULAIRE: TextesPreRemplissage = {
  titreSucces: "Formulaire pré-rempli",
  texteSucces: "Relisez chaque champ avant de créer.",
};

const enregistres = new Map<Onglet, PreRemplissage>();
const attentes = new Map<Onglet, ((p: PreRemplissage) => void)[]>();

/** Déclare le pré-remplissage d'un onglet (de nouveau si l'onglet se reconstruit). */
export function enregistrerPreRemplissage(
  onglet: Onglet,
  lancer: PreRemplissage["lancer"],
  textes: TextesPreRemplissage = TEXTES_FORMULAIRE,
): void {
  const p: PreRemplissage = { lancer, ...textes };
  enregistres.set(onglet, p);
  for (const resoudre of attentes.get(onglet) ?? []) resoudre(p);
  attentes.delete(onglet);
}

/**
 * Pré-remplissage de l'onglet, dès qu'il est prêt : un onglet peut d'abord
 * charger ses listes depuis Dokos avant de construire son formulaire.
 */
export function preRemplissage(onglet: Onglet, delai = 20_000): Promise<PreRemplissage> {
  const deja = enregistres.get(onglet);
  if (deja) return Promise.resolve(deja);
  return new Promise((resoudre, rejeter) => {
    const minuteur = window.setTimeout(
      () => rejeter(new Error("Le formulaire de cet onglet n'est pas prêt : vérifiez l'onglet, puis réessayez.")),
      delai,
    );
    const liste = attentes.get(onglet) ?? [];
    liste.push((p) => {
      window.clearTimeout(minuteur);
      resoudre(p);
    });
    attentes.set(onglet, liste);
  });
}

/** Questions et avertissements de l'IA après un pré-remplissage. */
export function blocRemarques(r: Remarques, textes: TextesPreRemplissage): HTMLElement {
  if (r.questions.length === 0 && r.avertissements.length === 0) {
    return alerte("succes", textes.titreSucces, h("p", {}, textes.texteSucces));
  }
  return alerte(
    r.questions.length > 0 ? "attention" : "info",
    r.questions.length > 0 ? `${textes.titreSucces}, à compléter` : textes.titreSucces,
    r.questions.length > 0
      ? h(
          "ul",
          { class: "liste-resultats" },
          ...r.questions.map((q) =>
            h("li", {}, h("span", { class: "marque-etat alerte-etat" }, "?"), h("span", {}, q)),
          ),
        )
      : null,
    ...r.avertissements.map((a) => h("p", { class: "reference" }, a)),
  );
}

/** Boutons pour trancher entre plusieurs candidats trouvés dans Dokos. */
export function choixCandidats<T>(
  titre: string,
  choix: ChoixVue<T>,
  libelle: (c: T) => string,
  surChoix: (c: T) => void,
): HTMLElement | null {
  if (choix.statut !== "a_preciser" || choix.candidats.length === 0) return null;
  const conteneur = h(
    "div",
    { class: "candidats" },
    h("span", { class: "note sans-marge" }, `${titre} « ${choix.texte} » :`),
  );
  for (const candidat of choix.candidats) {
    const b = bouton(libelle(candidat), "puce-bouton", () => {
      surChoix(candidat);
      conteneur.remove();
    });
    conteneur.append(b);
  }
  return conteneur;
}
