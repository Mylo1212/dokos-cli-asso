// Menu latéral : les sections rangées par usage (créer, traiter en nombre,
// suivre), la recherche, l'état de la connexion à Dokos, l'assistant et les
// réglages. Se réduit aux pictogrammes quand la fenêtre est étroite ou sur
// demande.

import type { ConnexionVue } from "./api";
import { h } from "./dom";
import { donnees } from "./donnees";
import { enregistrerPreference, preference, type Onglet } from "./etat";
import { icone, type NomIcone } from "./icones";
import { pluriel } from "./valeurs";

export interface Section {
  id: Onglet;
  libelle: string;
  icone: NomIcone;
  /** Titre du groupe dans le menu (vide : hors groupe). */
  groupe: string;
  /** Chiffre du raccourci Ctrl+chiffre. */
  touche: string;
  /** Profils Dokos dont la section a besoin. */
  profils: string[];
}

export const SECTIONS: Section[] = [
  { id: "accueil", libelle: "Accueil", icone: "accueil", groupe: "", touche: "1", profils: [] },
  { id: "ventes", libelle: "Ventes", icone: "ventes", groupe: "Créer", touche: "2", profils: ["ventes"] },
  {
    id: "formulaires",
    libelle: "Formulaires",
    icone: "formulaires",
    groupe: "Créer",
    touche: "3",
    profils: ["evenements"],
  },
  { id: "contrats", libelle: "Contrats", icone: "contrats", groupe: "Créer", touche: "4", profils: ["rh"] },
  { id: "import", libelle: "Import", icone: "import", groupe: "En nombre", touche: "5", profils: [] },
  { id: "listes", libelle: "Listes", icone: "listes", groupe: "En nombre", touche: "6", profils: [] },
  { id: "suivi", libelle: "Suivi", icone: "suivi", groupe: "Suivre", touche: "7", profils: [] },
  { id: "reglages", libelle: "Réglages", icone: "reglages", groupe: "pied", touche: "8", profils: [] },
];

export function libelleSection(onglet: Onglet): string {
  return SECTIONS.find((s) => s.id === onglet)?.libelle ?? onglet;
}

export interface Menu {
  /** Marque la section affichée. */
  marquer: (onglet: Onglet) => void;
  boutonAssistant: HTMLButtonElement;
  basculer: () => void;
}

/** Résumé de l'état des connexions : une phrase, et son niveau. */
export function resumeConnexions(connexions: ConnexionVue[] | undefined): {
  niveau: "attente" | "ok" | "erreur" | "vide";
  texte: string;
  detail: string;
} {
  if (!connexions) return { niveau: "attente", texte: "Connexion à Dokos…", detail: "" };
  const configurees = connexions.filter((c) => c.etat !== "non_configure");
  const erreurs = configurees.filter((c) => c.etat === "erreur");
  const detail = connexions
    .map((c) => {
      if (c.etat === "ok") return `${c.libelle} : connecté (${c.utilisateur})`;
      if (c.etat === "erreur") return `${c.libelle} : ${c.message}`;
      return `${c.libelle} : clé à renseigner`;
    })
    .join("\n");
  if (configurees.length === 0) return { niveau: "vide", texte: "Dokos à configurer", detail };
  if (erreurs.length > 0)
    return { niveau: "erreur", texte: `Dokos : ${pluriel(erreurs.length, "profil")} en erreur`, detail };
  return { niveau: "ok", texte: "Dokos connecté", detail };
}

export function creerMenu(
  racine: HTMLElement,
  options: { surSection: (onglet: Onglet) => void; surRecherche: () => void },
): Menu {
  const liens = new Map<Onglet, HTMLButtonElement>();
  const pastilleSuivi = h("span", { class: "pastille", hidden: true });

  const lien = (s: Section): HTMLButtonElement => {
    const b = h(
      "button",
      { type: "button", class: "menu-lien", "data-onglet": s.id, title: `${s.libelle} (Ctrl+${s.touche})` },
      icone(s.icone),
      h("span", { class: "menu-texte" }, s.libelle),
      s.id === "suivi" ? pastilleSuivi : null,
    );
    b.addEventListener("click", () => options.surSection(s.id));
    liens.set(s.id, b);
    return b;
  };

  const reduire = h(
    "button",
    { type: "button", class: "menu-reduire", title: "Réduire ou déployer le menu (Ctrl+B)", "aria-label": "Menu" },
    icone("menu"),
  );
  const recherche = h(
    "button",
    { type: "button", class: "menu-recherche", title: "Rechercher ou lancer une action (Ctrl+K)" },
    icone("recherche"),
    h("span", { class: "menu-texte" }, "Rechercher…"),
    h("kbd", { class: "menu-texte" }, "Ctrl K"),
  );
  recherche.addEventListener("click", options.surRecherche);

  const groupes: HTMLElement[] = [];
  for (const nom of [...new Set(SECTIONS.map((s) => s.groupe))].filter((g) => g !== "pied")) {
    groupes.push(
      h(
        "div",
        { class: "menu-groupe" },
        nom ? h("div", { class: "menu-titre menu-texte" }, nom) : null,
        ...SECTIONS.filter((s) => s.groupe === nom).map(lien),
      ),
    );
  }

  const pointConnexion = h("span", { class: "point attente", "aria-hidden": "true" });
  const texteConnexion = h("span", { class: "menu-texte" }, "Connexion à Dokos…");
  const connexion = h("button", { type: "button", class: "menu-lien etat-connexion" }, pointConnexion, texteConnexion);
  connexion.addEventListener("click", () => options.surSection("reglages"));

  const boutonAssistant = h(
    "button",
    { type: "button", class: "menu-lien", "aria-pressed": "true", title: "Afficher ou masquer l'assistant (Ctrl+J)" },
    icone("assistant"),
    h("span", { class: "menu-texte" }, "Assistant"),
  );

  racine.append(
    h(
      "div",
      { class: "menu-tete" },
      h("span", { class: "logo", "aria-hidden": "true" }),
      h("span", { class: "menu-nom menu-texte" }, "Dokos Assistant"),
      reduire,
    ),
    recherche,
    h("div", { class: "menu-sections" }, ...groupes),
    h(
      "div",
      { class: "menu-groupe menu-pied" },
      connexion,
      boutonAssistant,
      ...SECTIONS.filter((s) => s.groupe === "pied").map(lien),
    ),
  );

  // --- Menu réduit aux pictogrammes ---
  const basculer = () => {
    const reduit = !document.body.classList.contains("menu-reduit");
    document.body.classList.toggle("menu-reduit", reduit);
    enregistrerPreference("menu-reduit", reduit ? "oui" : "non");
  };
  document.body.classList.toggle("menu-reduit", preference("menu-reduit", "non") === "oui");
  reduire.addEventListener("click", basculer);
  // Fenêtre étroite : réduit d'office, pour laisser la place au contenu et à l'assistant.
  const etroite = window.matchMedia("(max-width: 1180px)");
  const majEtroite = () => document.body.classList.toggle("menu-etroit", etroite.matches);
  etroite.addEventListener("change", majEtroite);
  majEtroite();

  // --- État des connexions et sections à configurer ---
  donnees.connexions.abonner(() => {
    const connexions = donnees.connexions.etat().valeur;
    const resume = resumeConnexions(connexions);
    pointConnexion.className = `point ${resume.niveau}`;
    texteConnexion.textContent = resume.texte;
    connexion.title = resume.detail || resume.texte;
    connexion.setAttribute("aria-label", resume.texte);
    for (const s of SECTIONS) {
      const manquants = (connexions ?? []).filter((c) => s.profils.includes(c.profil) && c.etat !== "ok");
      const b = liens.get(s.id);
      if (!b) continue;
      b.classList.toggle("a-configurer", manquants.length > 0);
      const raison = manquants
        .map((c) => (c.etat === "erreur" ? `clé ${c.libelle} en erreur` : `clé ${c.libelle} à renseigner`))
        .join(", ");
      b.title = `${s.libelle} (Ctrl+${s.touche})${raison ? ` — ${raison} dans Réglages` : ""}`;
    }
  });

  // --- Brouillons en attente, sur « Suivi » ---
  donnees.brouillons.abonner(() => {
    const v = donnees.brouillons.etat().valeur;
    const nombre = v ? v.ventes.length + v.evenements.length : 0;
    pastilleSuivi.hidden = nombre === 0;
    pastilleSuivi.textContent = String(nombre);
    pastilleSuivi.title = `${pluriel(nombre, "brouillon")} en attente de validation dans Dokos`;
  });

  return {
    marquer: (onglet) => {
      for (const [id, b] of liens) {
        if (id === onglet) b.setAttribute("aria-current", "page");
        else b.removeAttribute("aria-current");
      }
    },
    boutonAssistant,
    basculer,
  };
}
