// Palette de commandes (Ctrl+K) : aller à une section, lancer une action,
// ou chercher un client ou un article dans Dokos pour démarrer une vente.
// Tout au clavier : flèches, Entrée, Échap.

import type { ArticleVue, ClientVue } from "./api";
import { h } from "./dom";
import { recherches } from "./donnees";
import { assistantActif, ouvrirOnglet, signaler } from "./etat";
import { icone, type NomIcone } from "./icones";
import { SECTIONS } from "./navigation";
import { correspond } from "./valeurs";

export interface Commande {
  libelle: string;
  groupe: string;
  icone: NomIcone;
  /** Mots en plus du libellé pour la retrouver. */
  mots?: string;
  /** Raccourci affiché à droite. */
  touche?: string;
  /** Détail affiché en petit (identifiant, code). */
  detail?: string;
  action: () => void;
}

/** Sections et créations : les commandes qui ne dépendent pas de l'écran. */
export function commandesDeBase(): Commande[] {
  const vente = (type: "devis" | "commande" | "facture", libelle: string, mots: string): Commande => ({
    libelle,
    groupe: "Créer",
    icone: "ventes",
    mots,
    action: () => {
      ouvrirOnglet("ventes");
      signaler("dokos:nouvelle-vente", type);
    },
  });
  return [
    vente("devis", "Nouveau devis", "proposition offre chiffrage"),
    vente("commande", "Nouvelle commande", "bon de commande reservation"),
    vente("facture", "Nouvelle facture", "facturer"),
    {
      libelle: "Nouvel événement avec inscription",
      groupe: "Créer",
      icone: "formulaires",
      mots: "formulaire atelier stage",
      action: () => ouvrirOnglet("formulaires"),
    },
    {
      libelle: "Importer un fichier Excel",
      groupe: "En nombre",
      icone: "import",
      mots: "tableau en nombre lot modele",
      action: () => ouvrirOnglet("import"),
    },
    {
      libelle: "Exporter vers Excel",
      groupe: "En nombre",
      icone: "export",
      mots: "liste clients articles factures employes tableau fichier",
      action: () => ouvrirOnglet("export"),
    },
    {
      libelle: "Exporter le grand livre",
      groupe: "En nombre",
      icone: "export",
      mots: "comptabilite journal ecritures ventes compta",
      action: () => {
        ouvrirOnglet("export");
        signaler("dokos:grand-livre", null);
      },
    },
    {
      libelle: "Mes brouillons",
      groupe: "Suivre",
      icone: "suivi",
      mots: "attente valider commander facturer",
      action: () => ouvrirOnglet("suivi", "suivi-brouillons"),
    },
    {
      libelle: "Factures en retard",
      groupe: "Suivre",
      icone: "suivi",
      mots: "impayes relance encaisser",
      action: () => ouvrirOnglet("suivi", "suivi-impayes"),
    },
    {
      libelle: "Inscrits d'un événement",
      groupe: "Suivre",
      icone: "suivi",
      mots: "participants inscriptions",
      action: () => ouvrirOnglet("suivi", "suivi-inscrits"),
    },
    ...SECTIONS.map((s): Commande => ({
      libelle: `Aller à ${s.libelle}`,
      groupe: "Aller à",
      icone: s.icone,
      touche: `Ctrl ${s.touche}`,
      action: () => ouvrirOnglet(s.id),
    })),
  ];
}

export function creerPalette(options: {
  commandes: () => Commande[];
  /** Envoie un texte à l'assistant (panneau ouvert au besoin). */
  demanderAssistant: (texte: string) => void;
}): { ouvrir: () => void; fermer: () => void } {
  const saisie = h("input", {
    type: "text",
    class: "palette-saisie",
    placeholder: "Une action, un client, un article…",
    autocomplete: "off",
    spellcheck: "false",
    role: "combobox",
    "aria-expanded": "true",
    "aria-controls": "palette-liste",
    "aria-label": "Rechercher ou lancer une action",
  });
  const liste = h("div", { class: "palette-liste", id: "palette-liste", role: "listbox" });
  const boite = h(
    "div",
    { class: "palette", role: "dialog", "aria-modal": "true", "aria-label": "Palette de commandes" },
    h("div", { class: "palette-tete" }, icone("recherche"), saisie),
    liste,
    h(
      "div",
      { class: "palette-pied" },
      h("span", {}, h("kbd", {}, "↑"), h("kbd", {}, "↓"), " choisir"),
      h("span", {}, h("kbd", {}, "Entrée"), " lancer"),
      h("span", {}, h("kbd", {}, "Échap"), " fermer"),
    ),
  );
  const fond = h("div", { class: "palette-fond", hidden: true }, boite);
  document.body.append(fond);

  let visibles: Commande[] = [];
  let choisie = 0;
  let trouves: Commande[] = [];
  let numero = 0;
  let minuteur: number | undefined;
  let retour: HTMLElement | null = null;

  function choisir(indice: number) {
    const elements = liste.querySelectorAll<HTMLElement>("[role=option]");
    if (elements.length === 0) return;
    choisie = (indice + elements.length) % elements.length;
    elements.forEach((e, i) => e.setAttribute("aria-selected", String(i === choisie)));
    const actif = elements[choisie];
    if (actif) {
      saisie.setAttribute("aria-activedescendant", actif.id);
      actif.scrollIntoView({ block: "nearest" });
    }
  }

  function lancer(commande: Commande | undefined) {
    if (!commande) return;
    fermer();
    commande.action();
  }

  function afficher() {
    const texte = saisie.value.trim();
    const commandes = options.commandes().filter((c) => correspond(`${c.libelle} ${c.mots ?? ""} ${c.groupe}`, texte));
    // Sans rien de tapé : seulement l'essentiel (créer, suivre), pas toute la liste.
    visibles = [
      ...(texte === "" ? commandes.filter((c) => c.groupe === "Créer" || c.groupe === "Suivre") : commandes),
      ...trouves,
    ];
    if (texte.length >= 2 && assistantActif()) {
      visibles.push({
        libelle: `Demander à l'assistant : « ${texte} »`,
        groupe: "Assistant",
        icone: "assistant",
        action: () => options.demanderAssistant(texte),
      });
    }
    const enfants: HTMLElement[] = [];
    let groupe = "";
    visibles.forEach((c, i) => {
      if (c.groupe !== groupe) {
        groupe = c.groupe;
        enfants.push(h("div", { class: "palette-groupe", role: "presentation" }, groupe));
      }
      const option = h(
        "div",
        { class: "palette-option", role: "option", id: `palette-option-${i}`, "aria-selected": "false" },
        icone(c.icone),
        h("span", { class: "palette-libelle" }, c.libelle),
        c.detail ? h("span", { class: "id" }, c.detail) : null,
        c.touche ? h("kbd", {}, c.touche) : null,
      );
      option.addEventListener("click", () => lancer(c));
      option.addEventListener("mousemove", () => {
        if (choisie !== i) choisir(i);
      });
      enfants.push(option);
    });
    if (visibles.length === 0) enfants.push(h("div", { class: "palette-vide" }, "Rien ne correspond."));
    liste.replaceChildren(...enfants);
    choisir(Math.min(choisie, Math.max(visibles.length - 1, 0)));
  }

  /** Clients et articles de Dokos pour le texte tapé ; une erreur (clé absente…) ne montre rien. */
  function chercherDansDokos() {
    window.clearTimeout(minuteur);
    const texte = saisie.value.trim();
    const demande = ++numero;
    if (texte.length < 2) {
      trouves = [];
      return;
    }
    minuteur = window.setTimeout(async () => {
      const [clients, articles] = await Promise.allSettled([
        recherches.clients(texte),
        recherches.articles(texte, null),
      ]);
      if (demande !== numero || fond.hidden) return;
      const versClient = (c: ClientVue): Commande => ({
        libelle: `Nouvelle vente pour ${c.nom || c.id}`,
        groupe: "Clients",
        icone: "client",
        detail: c.id,
        action: () => {
          ouvrirOnglet("ventes");
          signaler("dokos:choisir-client", c);
        },
      });
      const versArticle = (a: ArticleVue): Commande => ({
        libelle: `Ajouter ${a.libelle || a.code} à la vente`,
        groupe: "Articles",
        icone: "article",
        detail: a.code,
        action: () => {
          ouvrirOnglet("ventes");
          signaler("dokos:ajouter-article", a);
        },
      });
      trouves = [
        ...(clients.status === "fulfilled" ? clients.value.slice(0, 5).map(versClient) : []),
        ...(articles.status === "fulfilled" ? articles.value.slice(0, 5).map(versArticle) : []),
      ];
      afficher();
    }, 200);
  }

  function ouvrir() {
    if (!fond.hidden) return saisie.select();
    retour = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    saisie.value = "";
    trouves = [];
    choisie = 0;
    fond.hidden = false;
    afficher();
    saisie.focus();
  }

  function fermer() {
    if (fond.hidden) return;
    fond.hidden = true;
    numero += 1;
    window.clearTimeout(minuteur);
    retour?.focus();
  }

  saisie.addEventListener("input", () => {
    choisie = 0;
    trouves = [];
    afficher();
    chercherDansDokos();
  });
  boite.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") choisir(choisie + 1);
    else if (e.key === "ArrowUp") choisir(choisie - 1);
    else if (e.key === "Enter" && !e.isComposing) lancer(visibles[choisie]);
    else if (e.key === "Escape") fermer();
    else if (e.key === "Tab")
      saisie.focus(); // le focus reste dans la palette
    else return;
    e.preventDefault();
  });
  fond.addEventListener("mousedown", (e) => {
    if (e.target === fond) fermer();
  });

  return { ouvrir, fermer };
}
