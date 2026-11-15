// Onglet Export : clients, articles, devis, commandes, factures et employés
// en fichier Excel. L'assistant choisit la liste et les filtres ; Rust les
// vérifie, lit Dokos (lecture seule) et écrit le fichier dans
// Documents\Dokos\exports. Le formulaire marche aussi sans l'assistant.

import { api, type ClientVue, type ListeVue, type PropositionListe, type ReferencesExport } from "./api";
import { choixCandidats, enregistrerPreRemplissage } from "./assistant";
import {
  alerte,
  alerteErreur,
  aujourdhui,
  bouton,
  caseACocher,
  champ,
  h,
  lienFichier,
  listeDeroulante,
  occupe,
  saisie,
} from "./dom";
import { listes } from "./donnees";
import { creerGrandLivre } from "./grand-livre";
import { demandeListe, dossierDe } from "./valeurs";

/** Clé du bouton « Grand livre » parmi les listes. */
const GRAND_LIVRE = "grand-livre";

export function initialiserExport(panneau: HTMLElement): void {
  let catalogue: ListeVue[] = [];
  let liste: ListeVue | null = null;
  let client: ClientVue | null = null;
  let groupe: string | null = null;
  let territoire: string | null = null;

  const grandLivre = creerGrandLivre();
  const segments = h("div", { class: "segments", role: "group", "aria-label": "Liste à exporter" });
  const choixClient = listeDeroulante<ClientVue>({
    placeholder: "Choisir un client…",
    lire: () => listes.clients(),
    libelle: (c) => c.nom || c.id,
    identifiant: (c) => c.id,
    surChoix: (c) => (client = c),
  });
  // Groupes et territoires de la liste choisie : relus si on change de liste.
  const reference = (
    placeholder: string,
    valeurs: (r: ReferencesExport) => string[],
    choisi: (v: string | null) => void,
  ) =>
    listeDeroulante<string>({
      placeholder,
      lire: async () => (liste ? valeurs(await listes.references(liste.nom)) : []),
      cle: () => liste?.nom ?? "",
      libelle: (v) => v,
      identifiant: () => "",
      surChoix: choisi,
    });
  const choixGroupe = reference(
    "Choisir un groupe (sous-groupes compris)…",
    (r) => r.groupes,
    (v) => (groupe = v),
  );
  const choixTerritoire = reference(
    "Choisir un territoire…",
    (r) => r.territoires,
    (v) => (territoire = v),
  );
  const du = saisie({ type: "date" });
  const au = saisie({ type: "date" });
  const statut = h("select");
  const texte = saisie({ placeholder: "Mot contenu dans le nom" });
  const inactifs = caseACocher("Inclure les éléments désactivés");
  const vendables = caseACocher("Articles vendables seulement");

  const champClient = champ("Client", choixClient.racine, "large");
  const champGroupe = champ("Groupe", choixGroupe.racine);
  const champTerritoire = champ("Territoire", choixTerritoire.racine);
  const champDu = champ("Du", du);
  const champAu = champ("Au", au);
  const champStatut = champ("Statut", statut);
  const colonnes = h("p", { class: "note" });
  const resultat = h("div", { "aria-live": "polite" });
  const exporter = bouton("Créer le fichier Excel", "principal", () => lancerExport());
  exporter.dataset.raccourci = "verifier";

  /** Montre la carte du grand livre à la place des filtres des listes, ou l'inverse. */
  function montrerGrandLivre(visible: boolean) {
    carteListe.hidden = visible;
    grandLivre.carte.hidden = !visible;
    if (visible) {
      for (const b of segments.querySelectorAll<HTMLButtonElement>("button")) {
        b.setAttribute("aria-pressed", String(b.dataset.liste === GRAND_LIVRE));
      }
      grandLivre.afficher();
    }
  }

  function choisirListe(nom: string) {
    montrerGrandLivre(false);
    // Un groupe de clients n'est pas un groupe d'articles : changer de liste efface groupe et territoire.
    if (liste?.nom !== nom) {
      choixGroupe.choisir(null);
      choixTerritoire.choisir(null);
    }
    liste = catalogue.find((l) => l.nom === nom) ?? null;
    for (const b of segments.querySelectorAll<HTMLButtonElement>("button")) {
      b.setAttribute("aria-pressed", String(b.dataset.liste === nom));
    }
    adapter();
  }

  /** N'affiche que les filtres de la liste choisie. */
  function adapter() {
    const l = liste;
    champClient.hidden = !l?.client;
    champGroupe.hidden = !l?.groupe;
    champTerritoire.hidden = !l?.territoire;
    champDu.hidden = champAu.hidden = !l?.dates;
    if (l?.dates) {
      champDu.firstChild!.textContent = `Du (${l.dates})`;
      champAu.firstChild!.textContent = `Au (${l.dates})`;
    }
    champStatut.hidden = !l || l.statuts.length === 0;
    const statutActuel = statut.value;
    statut.replaceChildren(
      h("option", { value: "" }, "Tous"),
      ...(l?.statuts ?? []).map((s) => h("option", { value: s.cle }, s.libelle)),
    );
    statut.value = l?.statuts.some((s) => s.cle === statutActuel) ? statutActuel : "";
    inactifs.label.lastChild!.textContent = `Inclure les ${l?.inactifs ?? "éléments désactivés"}`;
    vendables.label.hidden = !l?.vendables;
    colonnes.textContent = l ? `Colonnes : ${l.colonnes.join(", ")}.` : "";
    resultat.replaceChildren();
  }

  /** Reprend la proposition de l'assistant dans le formulaire. */
  function remplir(p: PropositionListe, ajouter: (element: HTMLElement) => void) {
    if (p.liste) choisirListe(p.liste);
    if (p.client.statut === "trouve" && p.client.valeur) choixClient.choisir(p.client.valeur);
    else if (p.client.statut === "introuvable") choixClient.saisir(p.client.texte);
    else choixClient.choisir(null);
    const candidats = choixCandidats(
      "Client",
      p.client,
      (c) => c.nom || c.id,
      (c) => choixClient.choisir(c),
    );
    if (candidats) ajouter(candidats);
    choixGroupe.saisir(p.groupe ?? "", false);
    choixTerritoire.saisir(p.territoire ?? "", false);
    du.value = p.du ?? "";
    au.value = p.au ?? "";
    statut.value = p.statut ?? "";
    texte.value = p.texte ?? "";
    inactifs.input.checked = p.inclureInactifs;
    vendables.input.checked = p.vendablesSeulement;
  }

  async function lancerExport() {
    if (!liste) {
      resultat.replaceChildren(alerte("attention", "À compléter", h("p", {}, "Choisissez d'abord une liste.")));
      return;
    }
    const demande = demandeListe(liste, {
      client: client?.id ?? null,
      groupe: groupe ?? choixGroupe.texteSaisi(),
      territoire: territoire ?? choixTerritoire.texteSaisi(),
      du: du.value,
      au: au.value,
      statut: statut.value,
      texte: texte.value,
      inclureInactifs: inactifs.input.checked,
      vendablesSeulement: vendables.input.checked,
    });
    if (typeof demande === "string") {
      resultat.replaceChildren(alerte("attention", "À corriger", h("p", {}, demande)));
      return;
    }
    resultat.replaceChildren(h("p", { class: "note sans-marge" }, "Lecture de Dokos…"));
    const r = await occupe([exporter], resultat, () => api.exporterListe(demande, aujourdhui()));
    if (!r) return;
    resultat.replaceChildren(
      alerte(
        r.lignes === 0 || r.avertissements.length > 0 ? "attention" : "succes",
        r.lignes === 0
          ? "Aucune ligne : fichier vide créé"
          : `${r.lignes} ligne${r.lignes > 1 ? "s" : ""} exportée${r.lignes > 1 ? "s" : ""}`,
        h("p", {}, r.description),
        h("p", { class: "chemin" }, r.chemin),
        ...r.avertissements.map((a) => h("p", { class: "reference" }, a)),
        h(
          "div",
          { class: "actions" },
          lienFichier(r.chemin, "Ouvrir le fichier"),
          lienFichier(dossierDe(r.chemin), "Ouvrir le dossier"),
        ),
      ),
    );
  }

  enregistrerPreRemplissage(
    "export",
    async (demande, jour, ajouter) => {
      const p = await api.assistantListe(demande, jour);
      remplir(p, ajouter);
      // Lecture seule : quand tout est retrouvé dans Dokos, le fichier est créé tout de suite.
      if (p.pret) await lancerExport();
      return p;
    },
    {
      titreSucces: "Export préparé",
      texteSucces:
        "Filtres repris dans l'onglet Export, où le fichier Excel apparaît. Modifiez-les puis recréez-le si besoin.",
    },
  );

  const carteListe = h(
    "div",
    { class: "carte" },
    h("h2", {}, "Filtres"),
    h(
      "div",
      { class: "grille" },
      champClient,
      champGroupe,
      champTerritoire,
      champDu,
      champAu,
      champStatut,
      champ("Nom contenant", texte),
    ),
    h("div", { class: "actions" }, inactifs.label, vendables.label),
    colonnes,
    h(
      "p",
      { class: "note" },
      "Le fichier est enregistré dans Documents\\Dokos\\exports, avec une feuille « À propos » qui rappelle les filtres. 10 000 lignes au plus.",
    ),
    h("div", { class: "actions" }, exporter),
    resultat,
  );

  panneau.append(
    h("h1", {}, "Export"),
    h(
      "p",
      { class: "sous-titre" },
      "Clients, articles, devis, commandes, factures ou employés, et le grand livre d'un journal comptable, dans un fichier Excel lu dans Dokos sans rien y modifier.",
    ),
    segments,
    carteListe,
    grandLivre.carte,
  );

  // Palette : « Exporter le grand livre ».
  document.addEventListener("dokos:grand-livre", () => montrerGrandLivre(true));

  void api
    .listesExportables()
    .then((disponibles) => {
      catalogue = disponibles;
      for (const l of disponibles) {
        const b = h("button", { type: "button", "aria-pressed": "false", "data-liste": l.nom }, l.libelle);
        b.addEventListener("click", () => choisirListe(l.nom));
        segments.append(b);
      }
      const bouton = h("button", { type: "button", "aria-pressed": "false", "data-liste": GRAND_LIVRE }, "Grand livre");
      bouton.addEventListener("click", () => montrerGrandLivre(true));
      segments.append(bouton);
      // Ouvert depuis la palette avant la fin de la lecture : le grand livre reste affiché.
      if (grandLivre.carte.hidden) choisirListe(disponibles[0]?.nom ?? "");
      else montrerGrandLivre(true);
    })
    .catch((e: unknown) => segments.replaceChildren(alerteErreur(e)));
}
