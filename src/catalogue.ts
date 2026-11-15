// Catalogue : arbre des groupes d'articles, articles du groupe choisi avec
// prix indicatif, ajout d'une ligne au document, création d'article (désactivé)
// et de sous-groupe.

import { api, type ArticleVue, type CatalogueVue, type NoeudVue, type PrixVue } from "./api";
import { listes, recherches, viderListes } from "./donnees";
import {
  alerte,
  alerteErreur,
  bouton,
  caseACocher,
  champ,
  h,
  lienDokos,
  listeDeroulante,
  montant,
  occupe,
  saisie,
  selection,
  valeur,
} from "./dom";

export interface Catalogue {
  carte: HTMLDivElement;
  /** Groupe sélectionné, pour filtrer la recherche d'articles. */
  groupeChoisi: () => string | null;
  /** Prix indicatif connu d'un article, après affichage du groupe. */
  prixDe: (code: string) => PrixVue | undefined;
  charger: () => Promise<void>;
}

export function creerCatalogue(options: { surAjout: (article: ArticleVue) => void }): Catalogue {
  let groupe: string | null = null;
  let catalogue: CatalogueVue | null = null;
  const prix = new Map<string, PrixVue>();
  const arbre = h("div", { class: "arbre", role: "tree" });
  const liste = h("div", { class: "articles-groupe" });
  const filtre = saisie({ placeholder: "Filtrer dans ce groupe", autocomplete: "off" });
  const titreGroupe = h("span", { class: "groupe-choisi" }, "Choisissez un groupe");
  const zone = h("div", { "aria-live": "polite" });
  const zoneGestion = h("div");
  const actualiser = bouton("Actualiser", "discret", () => charger());
  const corps = h(
    "div",
    { class: "catalogue", hidden: true },
    arbre,
    h(
      "div",
      { class: "catalogue-droite" },
      h("div", { class: "catalogue-entete" }, titreGroupe, filtre),
      liste,
      zoneGestion,
    ),
  );
  const ouvrir = bouton("Parcourir le catalogue", "discret", async () => {
    corps.hidden = !corps.hidden;
    ouvrir.textContent = corps.hidden ? "Parcourir le catalogue" : "Masquer le catalogue";
    if (!corps.hidden && !catalogue) await charger();
  });

  function noeud(n: NoeudVue, profondeur: number): HTMLElement {
    const ligne = h("div", { class: "arbre-ligne", role: "treeitem" });
    // Par le CSSOM : la politique de sécurité de la fenêtre refuse l'attribut `style`.
    ligne.style.setProperty("--profondeur", String(profondeur));
    const enfants = h("div", { class: "arbre-enfants", role: "group", hidden: profondeur > 0 });
    const pliage = h(
      "button",
      {
        type: "button",
        class: "arbre-pliage",
        "aria-label": n.enfants.length > 0 ? "Déplier" : "",
        disabled: n.enfants.length === 0,
      },
      n.enfants.length > 0 ? "▸" : "·",
    );
    pliage.addEventListener("click", () => {
      enfants.hidden = !enfants.hidden;
      pliage.textContent = enfants.hidden ? "▸" : "▾";
    });
    const nom = h("button", { type: "button", class: "arbre-nom" }, n.id);
    nom.addEventListener("click", () => void choisirGroupe(n.id, nom));
    ligne.append(pliage, nom);
    for (const e of n.enfants) enfants.append(noeud(e, profondeur + 1));
    if (profondeur === 0 && n.enfants.length > 0) {
      enfants.hidden = false;
      pliage.textContent = "▾";
    }
    return h("div", {}, ligne, enfants);
  }

  async function charger() {
    // « Actualiser » relit aussi les prix.
    prix.clear();
    prixDemandes.clear();
    arbre.replaceChildren(h("p", { class: "note sans-marge" }, "Chargement du catalogue…"));
    try {
      catalogue = await api.catalogue();
      arbre.replaceChildren(...catalogue.arbre.map((n) => noeud(n, 0)));
      if (catalogue.arbre.length === 0)
        arbre.replaceChildren(h("p", { class: "note sans-marge" }, "Aucun groupe d'articles dans Dokos."));
      construireGestion();
    } catch (e) {
      arbre.replaceChildren(alerteErreur(e, () => void charger()));
    }
  }

  let minuteur: number | undefined;
  filtre.addEventListener("input", () => {
    window.clearTimeout(minuteur);
    minuteur = window.setTimeout(() => void afficherArticles(), 250);
  });

  async function choisirGroupe(id: string, boutonNom: HTMLElement) {
    groupe = id;
    for (const b of arbre.querySelectorAll(".arbre-nom[aria-current]")) b.removeAttribute("aria-current");
    boutonNom.setAttribute("aria-current", "true");
    titreGroupe.textContent = id;
    await afficherArticles();
  }

  // Numéro de la dernière demande : une réponse lente ne remplace pas une plus récente.
  let demande = 0;
  // Articles dont le prix a déjà été demandé (trouvé ou non) : pas redemandé à chaque frappe.
  const prixDemandes = new Set<string>();

  async function afficherArticles() {
    if (!groupe) return;
    const numero = ++demande;
    liste.replaceChildren(h("p", { class: "note sans-marge" }, "Chargement…"));
    try {
      const articles = await api.articlesDuGroupe(groupe, valeur(filtre));
      if (numero !== demande) return;
      if (articles.length === 0) {
        liste.replaceChildren(h("p", { class: "note sans-marge" }, "Aucun article vendable dans ce groupe."));
        return;
      }
      const manquants = articles.map((a) => a.code).filter((c) => !prixDemandes.has(c));
      if (catalogue?.listeDePrix && manquants.length > 0) {
        const prixListe = await api.prixIndicatifs(manquants).catch(() => null);
        if (prixListe) {
          for (const c of manquants) prixDemandes.add(c);
          for (const p of prixListe) prix.set(p.article, p);
        }
        if (numero !== demande) return;
      }
      liste.replaceChildren(
        ...articles.map((a) => {
          const p = prix.get(a.code);
          const ajouter = bouton("+ Ajouter", "discret", () => options.surAjout(a));
          return h(
            "div",
            { class: "article-ligne" },
            h("span", { class: "article-libelle" }, a.libelle || a.code, h("span", { class: "id" }, a.code)),
            h(
              "span",
              { class: "article-detail" },
              [a.unite, a.groupe !== groupe ? a.groupe : ""].filter(Boolean).join(" · "),
            ),
            h("span", { class: "article-prix" }, p ? `${montant(p.prix, p.devise)} indicatif` : "prix par Dokos"),
            ajouter,
          );
        }),
      );
    } catch (e) {
      if (numero !== demande) return;
      liste.replaceChildren(alerteErreur(e, () => void afficherArticles()));
    }
  }

  function construireGestion() {
    const groupesFeuilles = (noeuds: NoeudVue[]): string[] =>
      noeuds.flatMap((n) => (n.enfants.length > 0 ? groupesFeuilles(n.enfants) : [n.id]));
    const tousGroupes = (noeuds: NoeudVue[]): string[] => noeuds.flatMap((n) => [n.id, ...tousGroupes(n.enfants)]);
    const feuilles = catalogue ? groupesFeuilles(catalogue.arbre) : [];
    const tous = catalogue ? tousGroupes(catalogue.arbre) : [];

    const code = saisie({ placeholder: "CODE-ARTICLE" });
    const libelle = saisie({ placeholder: "Libellé affiché" });
    const groupeArticle = selection(feuilles, "Choisir…");
    const unite = selection(catalogue?.unites ?? [], "Choisir…");
    const vendable = caseACocher("Vendable", true);
    const description = saisie({ placeholder: "Facultatif" });
    const zoneArticle = h("div", { "aria-live": "polite" });
    const creer = bouton("Créer désactivé", "principal", async () => {
      if (!valeur(code) || !valeur(libelle) || !groupeArticle.value || !unite.value) {
        return zoneArticle.replaceChildren(
          alerte("attention", "À compléter", "Code, libellé, groupe et unité sont obligatoires."),
        );
      }
      const r = await occupe([creer], zoneArticle, () =>
        api.creerArticle(
          {
            code: code.value,
            libelle: libelle.value,
            groupe: groupeArticle.value,
            unite: unite.value,
            vendable: vendable.input.checked,
            stocke: false,
            description: valeur(description),
          },
          false,
        ),
      );
      if (!r) return;
      const zoneActivation = h("div");
      const activer = bouton("Activer maintenant", "", async () => {
        const a = await occupe([activer], zoneActivation, () => api.activerArticle(r.code));
        if (a) {
          recherches.articles.vider();
          viderListes();
        }
        if (a)
          zoneActivation.replaceChildren(
            alerte("succes", `Article ${a.code} actif`, h("div", { class: "actions" }, lienDokos(a.lien))),
          );
      });
      zoneArticle.replaceChildren(
        alerte(
          r.cree ? "succes" : "attention",
          r.cree ? `Article ${r.code} créé, désactivé` : `Déjà dans Dokos : ${r.code} (${r.libelle})`,
          h(
            "p",
            {},
            r.cree
              ? "Relisez-le puis activez-le : il apparaîtra alors dans les recherches."
              : "Même code ou même libellé : rien n'a été créé.",
          ),
          h("div", { class: "actions" }, lienDokos(r.lien), r.actif ? null : activer),
        ),
        zoneActivation,
      );
    });

    const nomGroupe = saisie({ placeholder: "Nom du sous-groupe" });
    const parent = selection(tous, "Groupe parent…");
    const conteneur = caseACocher("Peut contenir des sous-groupes");
    const zoneGroupe = h("div", { "aria-live": "polite" });
    const creerGroupe = bouton("Créer le sous-groupe", "", async () => {
      if (!valeur(nomGroupe) || !parent.value)
        return zoneGroupe.replaceChildren(
          alerte("attention", "À compléter", "Nom et groupe parent sont obligatoires."),
        );
      const r = await occupe([creerGroupe], zoneGroupe, () =>
        api.creerGroupeArticles(nomGroupe.value, parent.value, conteneur.input.checked),
      );
      if (!r) return;
      zoneGroupe.replaceChildren(
        alerte(
          r.cree ? "succes" : "attention",
          r.cree ? `Groupe « ${r.id} » créé` : `Le groupe « ${r.id} » existe déjà`,
        ),
      );
      if (r.cree) await charger();
    });

    // Article à déplacer : choisi dans la liste, ou code tapé (un article encore désactivé n'y figure pas).
    let articleDeplace: ArticleVue | null = null;
    const choixDeplace = listeDeroulante<ArticleVue>({
      placeholder: "Choisir un article ou taper son code…",
      lire: () => listes.articles(null),
      libelle: (a) => a.libelle || a.code,
      identifiant: (a) => a.code,
      surChoix: (a) => (articleDeplace = a),
    });
    const versGroupe = selection(feuilles, "Nouveau groupe…");
    const zoneDeplacement = h("div", { "aria-live": "polite" });
    const deplacer = bouton("Déplacer", "", async () => {
      const codeADeplacer = articleDeplace?.code ?? choixDeplace.texteSaisi();
      if (!codeADeplacer || !versGroupe.value)
        return zoneDeplacement.replaceChildren(
          alerte("attention", "À compléter", "Article et groupe sont obligatoires."),
        );
      const r = await occupe([deplacer], zoneDeplacement, () => api.deplacerArticle(codeADeplacer, versGroupe.value));
      if (r) {
        viderListes();
        zoneDeplacement.replaceChildren(
          alerte("succes", `${r.code} déplacé`, h("div", { class: "actions" }, lienDokos(r.lien))),
        );
      }
    });

    const details = h(
      "details",
      { class: "gestion" },
      h("summary", {}, "Gérer le catalogue (article, sous-groupe, déplacement)"),
      h("h3", {}, "Nouvel article"),
      h(
        "div",
        { class: "grille" },
        champ("Code", code),
        champ("Libellé", libelle),
        champ("Groupe", groupeArticle),
        champ("Unité", unite),
        champ("Description", description, "large"),
      ),
      h("div", { class: "actions" }, vendable.label, creer),
      zoneArticle,
      h("h3", {}, "Nouveau sous-groupe"),
      h("div", { class: "grille" }, champ("Nom", nomGroupe), champ("Parent", parent)),
      h("div", { class: "actions" }, conteneur.label, creerGroupe),
      zoneGroupe,
      h("h3", {}, "Déplacer un article"),
      h("div", { class: "grille" }, champ("Article", choixDeplace.racine), champ("Vers le groupe", versGroupe)),
      h("div", { class: "actions" }, deplacer),
      zoneDeplacement,
    );
    zoneGestion.replaceChildren(details);
  }

  const carte = h(
    "div",
    { class: "carte" },
    h("h2", {}, "Catalogue"),
    h(
      "p",
      { class: "note sans-marge" },
      "Articles rangés par groupe. Le prix affiché est indicatif : Dokos applique ses listes de prix et ses taxes.",
    ),
    h("div", { class: "actions" }, ouvrir, actualiser),
    corps,
    zone,
  );
  return { carte, groupeChoisi: () => (corps.hidden ? null : groupe), prixDe: (code) => prix.get(code), charger };
}
