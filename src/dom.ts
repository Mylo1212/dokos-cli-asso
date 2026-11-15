// Petits utilitaires d'interface. Les données venant de Dokos ne passent
// jamais par innerHTML : seulement par textContent.

import { api } from "./api";
import { ouvrirOnglet } from "./etat";
import { actionPourErreur, erreurVue, filtrerOptions, formaterDate, titreErreur } from "./valeurs";

export { message, montant, nombreDepuisTexte, texteOuNull } from "./valeurs";

type Enfant = Node | string | null | undefined | false;

export function h<K extends keyof HTMLElementTagNameMap>(
  balise: K,
  attributs: Record<string, string | boolean | undefined> = {},
  ...enfants: Enfant[]
): HTMLElementTagNameMap[K] {
  const element = document.createElement(balise);
  for (const [nom, valeur] of Object.entries(attributs)) {
    if (valeur === undefined || valeur === false) continue;
    element.setAttribute(nom, valeur === true ? "" : valeur);
  }
  for (const enfant of enfants) {
    if (enfant === null || enfant === undefined || enfant === false) continue;
    element.append(enfant);
  }
  return element;
}

export function champ(libelle: string, controle: HTMLElement, classe = ""): HTMLLabelElement {
  return h("label", { class: `champ ${classe}`.trim() }, libelle, controle);
}

export function saisie(attributs: Record<string, string | boolean | undefined> = {}): HTMLInputElement {
  return h("input", { type: "text", ...attributs });
}

export function selection(options: string[], vide?: string): HTMLSelectElement {
  const select = h("select");
  if (vide !== undefined) select.append(h("option", { value: "" }, vide));
  for (const option of options) select.append(h("option", { value: option }, option));
  return select;
}

export function bouton(texte: string, classe = "", action?: () => void | Promise<void>): HTMLButtonElement {
  const b = h("button", { type: "button", class: `bouton ${classe}`.trim() }, texte);
  if (action) b.addEventListener("click", () => void action());
  return b;
}

export function caseACocher(texte: string, coche = false): { label: HTMLLabelElement; input: HTMLInputElement } {
  const input = h("input", { type: "checkbox", checked: coche });
  return { label: h("label", { class: "case" }, input, texte), input };
}

/** Texte saisi, ou `null` s'il est vide. */
export function valeur(element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement): string | null {
  const texte = element.value.trim();
  return texte === "" ? null : texte;
}

export function nombre(element: HTMLInputElement): number | null {
  const texte = element.value.trim().replace(",", ".");
  if (texte === "") return null;
  const n = Number(texte);
  return Number.isFinite(n) ? n : NaN;
}

export function aujourdhui(): string {
  return formaterDate(new Date());
}

/** Tableau simple : en-têtes puis lignes de texte (jamais de HTML). */
export function tableau(entetes: string[], lignes: (string | Node)[][], classe = ""): HTMLTableElement {
  return h(
    "table",
    { class: `tableau ${classe}`.trim() },
    h("thead", {}, h("tr", {}, ...entetes.map((e) => h("th", {}, e)))),
    h("tbody", {}, ...lignes.map((l) => h("tr", {}, ...l.map((c) => h("td", {}, c))))),
  );
}

/**
 * Désactive les boutons pendant l'appel et affiche l'erreur éventuelle, avec
 * l'action adaptée à son genre (réessayer, ouvrir Réglages).
 */
export async function occupe<T>(
  boutons: HTMLButtonElement[],
  zone: HTMLElement,
  action: () => Promise<T>,
): Promise<T | undefined> {
  const textes = boutons.map((b) => b.textContent);
  for (const b of boutons) b.disabled = true;
  const principal = boutons[0];
  if (principal) principal.textContent = "Patientez…";
  try {
    return await action();
  } catch (e) {
    zone.replaceChildren(alerteErreur(e, () => void occupe(boutons, zone, action)));
    return undefined;
  } finally {
    boutons.forEach((b, i) => {
      b.disabled = false;
      b.textContent = textes[i] ?? b.textContent;
    });
  }
}

/** Alerte d'erreur avec un bouton d'action selon le genre de l'erreur. */
export function alerteErreur(erreur: unknown, reessayer?: () => void): HTMLDivElement {
  const e = erreurVue(erreur);
  const actions: HTMLElement[] = [];
  switch (actionPourErreur(e.genre)) {
    case "reessayer":
      if (reessayer) actions.push(bouton("Réessayer", "", reessayer));
      break;
    case "reglages":
      actions.push(bouton("Ouvrir Réglages", "", () => ouvrirOnglet("reglages")));
      break;
    default:
      break;
  }
  return alerte(
    "erreur",
    titreErreur(e.genre),
    h("p", {}, e.message),
    actions.length > 0 ? h("div", { class: "actions" }, ...actions) : null,
  );
}

export function alerte(
  genre: "erreur" | "attention" | "succes" | "info",
  titre: string,
  ...contenu: Enfant[]
): HTMLDivElement {
  return h(
    "div",
    { class: `alerte ${genre}`, role: genre === "erreur" ? "alert" : "status" },
    h("strong", {}, titre),
    ...contenu,
  );
}

/** Bouton qui ouvre un lien Dokos ; l'erreur éventuelle s'affiche à côté, jamais en boîte de dialogue. */
export function lienDokos(url: string, texte = "Ouvrir dans Dokos"): HTMLElement {
  return boutonAvecErreur(texte, () => api.ouvrirLien(url));
}

export function lienFichier(chemin: string, texte = "Ouvrir"): HTMLElement {
  return boutonAvecErreur(texte, () => api.ouvrirFichier(chemin));
}

function boutonAvecErreur(texte: string, action: () => Promise<void>): HTMLElement {
  const erreur = h("span", { class: "erreur-inline", role: "alert" });
  const b = bouton(texte, "discret", async () => {
    erreur.textContent = "";
    try {
      await action();
    } catch (e) {
      erreur.textContent = erreurVue(e).message;
    }
  });
  return h("span", { class: "avec-erreur" }, b, erreur);
}

/** Copie un texte dans le presse-papiers ; renvoie faux si le navigateur refuse. */
export async function copier(texte: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(texte);
    return true;
  } catch {
    return false;
  }
}

// Un seul écouteur global ferme les listes ouvertes quand on clique ailleurs.
document.addEventListener("click", (e) => {
  for (const liste of document.querySelectorAll<HTMLElement>(".recherche .suggestions:not([hidden])")) {
    if (!liste.parentElement?.contains(e.target as Node)) liste.hidden = true;
  }
});

/**
 * Liste déroulante des éléments de Dokos (clients, articles, groupes…) : elle
 * s'ouvre au clic, au focus ou par la flèche et montre tout, et la frappe
 * filtre sur place (accents et casse ignorés). Les éléments sont lus une fois
 * par `lire` (l'appelant les garde en mémoire), jamais à chaque lettre. Une
 * fois un élément choisi, le champ est remplacé par une étiquette effaçable.
 * Navigation au clavier : flèches, Entrée, Échap.
 */
export function listeDeroulante<T>(options: {
  placeholder: string;
  /**
   * Lit tous les éléments dans Dokos. Appelée à chaque ouverture de la liste :
   * c'est à l'appelant de garder la réponse en mémoire (`listes` de donnees.ts).
   */
  lire: () => Promise<T[]>;
  /** Clé de ce que `lire` renvoie (un groupe choisi, par exemple) : si elle change, la liste est relue. */
  cle?: () => string;
  libelle: (element: T) => string;
  identifiant: (element: T) => string;
  surChoix: (element: T | null) => void;
}): {
  racine: HTMLDivElement;
  choisir: (element: T | null) => void;
  /** Remet le champ avec un texte, sans rien choisir ; avec `ouvrir`, montre aussi les éléments qui lui correspondent. */
  saisir: (texte: string, ouvrir?: boolean) => void;
  /** Texte tapé et non choisi (vide si un élément est choisi). */
  texteSaisi: () => string;
  /** Place le curseur dans le champ (ou sur le bouton « Changer » si un élément est choisi). */
  focus: () => void;
} {
  const racine = h("div", { class: "recherche liste-deroulante" });
  const input = saisie({
    placeholder: options.placeholder,
    autocomplete: "off",
    role: "combobox",
    "aria-expanded": "false",
    "aria-autocomplete": "list",
  });
  const fleche = h(
    "button",
    {
      type: "button",
      class: "liste-fleche",
      title: "Afficher la liste",
      "aria-label": "Afficher la liste",
      tabindex: "-1",
    },
    "▾",
  );
  const champSaisie = h("div", { class: "liste-saisie" }, input, fleche);
  const liste = h("div", { class: "suggestions", hidden: true, role: "listbox" });
  let elements: T[] | null = null;
  let cleLue: string | null = null;
  let lectures = 0;
  let enLecture = false;
  let message = "";

  const texteDe = (e: T) => `${options.libelle(e)} ${options.identifiant(e)}`;
  const afficher = (visible: boolean) => {
    liste.hidden = !visible;
    input.setAttribute("aria-expanded", String(visible));
  };

  const choisir = (element: T | null) => {
    afficher(false);
    if (element === null) {
      input.value = "";
      racine.replaceChildren(champSaisie, liste);
    } else {
      const etiquette = h(
        "div",
        { class: "choisi" },
        h("span", {}, options.libelle(element), h("span", { class: "id" }, options.identifiant(element))),
        h("button", { type: "button", class: "bouton-icone", title: "Changer", "aria-label": "Changer" }, "×"),
      );
      etiquette.querySelector("button")?.addEventListener("click", () => {
        choisir(null);
        input.focus();
      });
      racine.replaceChildren(etiquette);
    }
    options.surChoix(element);
  };

  const boutons = () => [...liste.querySelectorAll<HTMLButtonElement>("button")];
  const deplacer = (depuis: HTMLElement | null, pas: number) => {
    const tous = boutons();
    if (tous.length === 0) return;
    const indice = depuis instanceof HTMLButtonElement ? tous.indexOf(depuis) : -1;
    tous[(indice + pas + tous.length) % tous.length]?.focus();
  };

  /** Redessine la liste d'après la saisie et les éléments déjà lus. */
  const dessiner = () => {
    if (elements === null) {
      liste.replaceChildren(h("div", { class: "vide" }, message || "Lecture de Dokos…"));
      return;
    }
    const { affiches, restants } = filtrerOptions(elements, input.value, texteDe);
    liste.replaceChildren(
      ...(affiches.length === 0
        ? [h("div", { class: "vide" }, elements.length === 0 ? "Aucun élément dans Dokos" : "Aucun résultat")]
        : affiches.map((r) => {
            const b = h(
              "button",
              { type: "button", role: "option" },
              h("span", {}, options.libelle(r)),
              h("span", { class: "id" }, options.identifiant(r)),
            );
            b.addEventListener("click", () => choisir(r));
            return b;
          })),
      ...(restants > 0 ? [h("div", { class: "vide" }, `${restants} autres : continuez à taper pour affiner`)] : []),
    );
  };

  /** Lit (ou relit) les éléments ; une erreur est montrée dans la liste tant qu'il n'y a rien à montrer. */
  const charger = () => {
    const numero = ++lectures;
    enLecture = true;
    message = "";
    options
      .lire()
      .then((lus) => {
        if (numero === lectures) elements = lus;
      })
      .catch((e: unknown) => {
        if (numero === lectures && elements === null) message = erreurVue(e).message;
      })
      .finally(() => {
        if (numero !== lectures) return;
        enLecture = false;
        if (!liste.hidden) dessiner();
      });
  };

  /** Ouvre la liste ; `relire` : redemande les éléments (la mémoire de l'appelant évite les appels inutiles). */
  const ouvrir = (relire: boolean) => {
    const cle = options.cle?.() ?? "";
    if (cle !== cleLue) {
      elements = null;
      cleLue = cle;
      relire = true;
    }
    if (relire || (elements === null && !enLecture && message === "")) charger();
    dessiner();
    afficher(true);
  };

  // Cliquer dans la liste ne retire pas le focus du champ (WebKit ne donne pas le focus aux boutons).
  liste.addEventListener("mousedown", (e) => e.preventDefault());
  racine.addEventListener("focusout", (e) => {
    if (!racine.contains(e.relatedTarget as Node | null)) afficher(false);
  });
  input.addEventListener("focus", () => ouvrir(true));
  input.addEventListener("click", () => {
    if (liste.hidden) ouvrir(true);
  });
  input.addEventListener("input", () => ouvrir(false));
  fleche.addEventListener("click", () => {
    if (liste.hidden) {
      input.focus();
      ouvrir(true);
    } else afficher(false);
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") afficher(false);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (liste.hidden) ouvrir(true);
      deplacer(null, 1);
    }
    if (e.key === "Enter" && !liste.hidden) {
      e.preventDefault();
      boutons()[0]?.click();
    }
  });
  liste.addEventListener("keydown", (e) => {
    const cible = e.target as HTMLElement;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      deplacer(cible, 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (boutons()[0] === cible) input.focus();
      else deplacer(cible, -1);
    } else if (e.key === "Escape") {
      afficher(false);
      input.focus();
    }
  });

  const saisir = (texte: string, ouvrirListe = true) => {
    choisir(null);
    input.value = texte;
    if (ouvrirListe) ouvrir(true);
  };

  racine.append(champSaisie, liste);
  const focus = () => (input.isConnected ? input : racine.querySelector<HTMLElement>("button"))?.focus();
  return { racine, choisir, saisir, texteSaisi: () => (input.isConnected ? input.value.trim() : ""), focus };
}
