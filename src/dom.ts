// Petits utilitaires d'interface. Les données venant de Dokos ne passent
// jamais par innerHTML : seulement par textContent.

import { api } from "./api";
import { ouvrirOnglet } from "./etat";
import { actionPourErreur, erreurVue, formaterDate, titreErreur } from "./valeurs";

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

/**
 * Attente après la dernière frappe avant d'interroger Dokos. Les connexions
 * restent ouvertes et les réponses récentes sont gardées (donnees.ts) : un
 * délai court suffit à ne pas envoyer une requête par lettre.
 */
const DELAI_RECHERCHE = 180;

// Un seul écouteur global ferme les suggestions ouvertes quand on clique ailleurs.
document.addEventListener("click", (e) => {
  for (const liste of document.querySelectorAll<HTMLElement>(".recherche .suggestions:not([hidden])")) {
    if (!liste.parentElement?.contains(e.target as Node)) liste.hidden = true;
  }
});

/**
 * Champ de recherche avec suggestions venant de Dokos. Une fois un élément
 * choisi, le champ est remplacé par une étiquette effaçable. Navigation au
 * clavier : flèches, Entrée, Échap.
 */
export function recherche<T>(options: {
  placeholder: string;
  chercher: (texte: string) => Promise<T[]>;
  libelle: (element: T) => string;
  identifiant: (element: T) => string;
  surChoix: (element: T | null) => void;
}): {
  racine: HTMLDivElement;
  choisir: (element: T | null) => void;
  saisir: (texte: string, chercher?: boolean) => void;
  /** Place le curseur dans le champ (ou sur le bouton « Changer » si un élément est choisi). */
  focus: () => void;
} {
  const racine = h("div", { class: "recherche" });
  const input = saisie({ placeholder: options.placeholder, autocomplete: "off", role: "combobox" });
  const liste = h("div", { class: "suggestions", hidden: true, role: "listbox" });
  let minuteur: number | undefined;
  let demande = 0;

  const choisir = (element: T | null) => {
    liste.hidden = true;
    if (element === null) {
      input.value = "";
      racine.replaceChildren(input, liste);
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
    const suivant = tous[(indice + pas + tous.length) % tous.length];
    suivant?.focus();
  };

  input.addEventListener("input", () => {
    window.clearTimeout(minuteur);
    const texte = input.value.trim();
    if (texte.length < 2) {
      liste.hidden = true;
      return;
    }
    minuteur = window.setTimeout(async () => {
      const numero = ++demande;
      try {
        const resultats = await options.chercher(texte);
        if (numero !== demande) return;
        liste.replaceChildren(
          ...(resultats.length === 0
            ? [h("div", { class: "vide" }, "Aucun résultat dans Dokos")]
            : resultats.map((r) => {
                const b = h(
                  "button",
                  { type: "button", role: "option" },
                  h("span", {}, options.libelle(r)),
                  h("span", { class: "id" }, options.identifiant(r)),
                );
                b.addEventListener("click", () => choisir(r));
                return b;
              })),
        );
      } catch (e) {
        liste.replaceChildren(h("div", { class: "vide" }, erreurVue(e).message));
      }
      liste.hidden = false;
    }, DELAI_RECHERCHE);
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") liste.hidden = true;
    if (e.key === "ArrowDown") {
      e.preventDefault();
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
      liste.hidden = true;
      input.focus();
    }
  });

  /** Remet le champ avec un texte ; avec `chercher`, affiche aussi les suggestions de Dokos. */
  const saisir = (texte: string, chercher = true) => {
    choisir(null);
    input.value = texte;
    if (chercher) input.dispatchEvent(new Event("input"));
  };

  racine.append(input, liste);
  const focus = () => (input.isConnected ? input : racine.querySelector<HTMLElement>("button"))?.focus();
  return { racine, choisir, saisir, focus };
}
