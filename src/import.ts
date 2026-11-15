// Onglet Import : modèle par cas d'usage, choix du fichier, analyse ligne par
// ligne, choix des ambiguïtés, création tout ou rien, rapport.

import { api, type AnalyseVue, type CasVue, type ChoixImport, type LigneImportVue } from "./api";
import { alerte, bouton, caseACocher, h, lienDokos, lienFichier, occupe, tableau } from "./dom";
import { perimerSuivi, viderListes } from "./donnees";

export function initialiserImport(panneau: HTMLElement): void {
  let cas: CasVue | null = null;
  let chemin: string | null = null;
  let analyse: AnalyseVue | null = null;
  const choix = new Map<string, ChoixImport>();

  const listeCas = h("div", { class: "cas-liste", role: "radiogroup" });
  const descriptionCas = h("div", { class: "note" });
  const zoneModele = h("div", { "aria-live": "polite" });
  const zoneFichier = h("div", { "aria-live": "polite" });
  const zoneAnalyse = h("div", { "aria-live": "polite" });
  const zoneCreation = h("div", { "aria-live": "polite" });
  const nomFichier = h("span", { class: "chemin" }, "Aucun fichier choisi");
  const seulementPrets = caseACocher("Créer seulement les documents prêts, même s'il reste des lignes à corriger");
  const autoriserDoublon = caseACocher("Créer même si un document identique existe déjà");

  const telecharger = bouton("Télécharger le modèle", "principal", async () => {
    if (!cas) return;
    const c = await occupe([telecharger], zoneModele, () => api.genererModele(cas!.nom));
    if (c) {
      zoneModele.replaceChildren(
        alerte(
          "succes",
          "Modèle généré depuis votre Dokos",
          h("p", { class: "chemin" }, c),
          h("p", {}, "Remplissez l'onglet « Données » à partir de la ligne 3, puis revenez ici."),
          h("div", { class: "actions" }, lienFichier(c, "Ouvrir le modèle")),
        ),
      );
    }
  });
  const choisir = bouton("Choisir un fichier…", "", async () => {
    const c = await occupe([choisir], zoneFichier, () => api.choisirFichier());
    if (c === undefined) return;
    if (c === null) return;
    chemin = c;
    nomFichier.textContent = c;
    analyse = null;
    choix.clear();
    zoneAnalyse.replaceChildren();
    zoneCreation.replaceChildren();
    await analyser();
  });
  const reanalyser = bouton("Analyser", "", () => analyser());
  reanalyser.dataset.raccourci = "verifier";
  const creer = bouton("Créer dans Dokos", "principal", () => lancerCreation());

  function choisirCas(c: CasVue) {
    cas = c;
    for (const b of listeCas.querySelectorAll("button"))
      b.setAttribute("aria-checked", String(b.dataset.cas === c.nom));
    descriptionCas.replaceChildren(
      h("p", { class: "sans-marge" }, `Résultat : ${c.resultat}. Profil : ${c.profil}.`),
      h(
        "div",
        { class: "puces espace" },
        ...c.colonnes.map((col) =>
          h("span", { class: "puce", title: col.aide }, col.nom + (col.obligatoire ? " *" : "")),
        ),
      ),
    );
    analyse = null;
    choix.clear();
    zoneModele.replaceChildren();
    zoneAnalyse.replaceChildren();
    zoneCreation.replaceChildren();
  }

  async function chargerCas() {
    try {
      const liste = await api.casImport();
      listeCas.replaceChildren(
        ...liste.map((c) => {
          const b = h(
            "button",
            { type: "button", role: "radio", "aria-checked": "false", "data-cas": c.nom },
            c.libelle,
          );
          b.addEventListener("click", () => choisirCas(c));
          return b;
        }),
      );
      const premier = liste[0];
      if (premier) choisirCas(premier);
    } catch (e) {
      listeCas.replaceChildren(alerte("erreur", "Cas d'import illisibles", String(e)));
    }
  }

  async function analyser() {
    if (!cas || !chemin)
      return zoneAnalyse.replaceChildren(
        alerte("attention", "À compléter", "Choisissez un cas d'usage et un fichier."),
      );
    const a = await occupe([reanalyser, choisir, creer], zoneAnalyse, () =>
      api.analyserImport(cas!.nom, chemin!, [...choix.values()]),
    );
    if (!a) return;
    analyse = a;
    afficherAnalyse(a);
  }

  function ligneTableau(l: LigneImportVue): (string | Node)[] {
    const marque = { pret: "✓", a_preciser: "?", introuvable: "✗", invalide: "✗" }[l.statut];
    const classe = { pret: "ok", a_preciser: "alerte-etat", introuvable: "ko", invalide: "ko" }[l.statut];
    const statut = h("span", {}, h("span", { class: `marque-etat ${classe}` }, marque), " ", l.message || "prêt");
    if (l.statut === "a_preciser" && l.candidats.length > 0) {
      const candidats = h("div", { class: "candidats" });
      for (const c of l.candidats) {
        const b = bouton(`${c.libelle} (${c.id})`, "puce-bouton", () => {
          choix.set(`${l.champ}|${l.texte}`, { champ: l.champ, texte: l.texte, id: c.id });
          void analyser();
        });
        candidats.append(b);
      }
      statut.append(candidats);
    }
    const document = l.document !== null ? analyse?.documents[l.document] : undefined;
    return [String(l.numero), ...l.valeurs, statut, document?.doublon ? `identique à ${document.doublon}` : ""];
  }

  function afficherAnalyse(a: AnalyseVue) {
    const resume = `${a.lignes.length} ligne(s), ${a.documents.length} document(s) : ${a.prets} prêt(s), ${a.bloquantes} ligne(s) à corriger, ${a.doublons} doublon(s).`;
    const entetes = ["Ligne", ...a.entetes, "Statut", "Doublon"];
    zoneAnalyse.replaceChildren(
      alerte(
        a.bloquantes === 0 ? "succes" : "attention",
        a.bloquantes === 0 ? "Fichier prêt" : "Lignes à corriger",
        h("p", {}, resume),
        ...a.avertissements.map((x) => h("p", { class: "reference" }, x)),
      ),
      ...(a.lignes.length > 0
        ? [h("div", { class: "defilement" }, tableau(entetes, a.lignes.map(ligneTableau), "import"))]
        : []),
      ...(a.lignes.some((l) => l.statut === "a_preciser")
        ? [
            h(
              "p",
              { class: "note" },
              "Cliquez sur la bonne valeur pour trancher une ambiguïté : l'analyse est relancée avec votre choix.",
            ),
          ]
        : []),
    );
  }

  async function lancerCreation() {
    if (!cas || !chemin || !analyse)
      return zoneCreation.replaceChildren(alerte("attention", "À compléter", "Analysez d'abord un fichier."));
    const r = await occupe([creer, reanalyser, choisir], zoneCreation, () =>
      api.creerImport(
        cas!.nom,
        chemin!,
        [...choix.values()],
        seulementPrets.input.checked,
        autoriserDoublon.input.checked,
      ),
    );
    if (!r) return;
    if (r.crees > 0) {
      perimerSuivi();
      viderListes();
    }
    const lignes = r.documents.map((d) => [
      h(
        "span",
        {},
        h(
          "span",
          { class: `marque-etat ${d.statut === "cree" ? "ok" : d.statut === "erreur" ? "ko" : "alerte-etat"}` },
          d.statut === "cree" ? "✓" : d.statut === "erreur" ? "✗" : "·",
        ),
        " ",
        { cree: "créé", existant: "déjà présent", ignore: "ignoré", erreur: "erreur" }[d.statut],
      ),
      d.libelle,
      d.id,
      d.message,
      d.lien ? lienDokos(d.lien) : "",
    ]);
    zoneCreation.replaceChildren(
      alerte(
        r.erreurs > 0 ? "erreur" : "succes",
        r.erreurs > 0 ? "Import arrêté sur une erreur" : `${r.crees} document(s) créé(s)`,
        h(
          "p",
          {},
          `${r.crees} créé(s), ${r.existants} déjà présent(s), ${r.ignores} ignoré(s), ${r.erreurs} erreur(s). Les documents créés sont des brouillons à relire dans Dokos.`,
        ),
        h("p", {}, "Rapport : ", h("span", { class: "chemin" }, r.rapport)),
        h("div", { class: "actions" }, lienFichier(r.rapport, "Ouvrir le rapport")),
      ),
      h(
        "div",
        { class: "defilement" },
        tableau(["Statut", "Document", "Identifiant", "Message", ""], lignes, "import"),
      ),
    );
  }

  panneau.append(
    h("h1", {}, "Import"),
    h(
      "p",
      { class: "sous-titre" },
      "Un modèle Excel par cas d'usage, généré depuis votre Dokos. Rien n'est créé tant qu'une ligne pose problème.",
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, h("span", { class: "etape" }, "1"), "Cas d'usage et modèle"),
      listeCas,
      descriptionCas,
      h("div", { class: "actions" }, telecharger),
      zoneModele,
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, h("span", { class: "etape" }, "2"), "Fichier rempli"),
      h(
        "p",
        { class: "note sans-marge" },
        "Fichier .xlsx issu du modèle (ou .csv exporté depuis ce modèle), 2 000 lignes au plus. Le choix passe par une boîte de dialogue : la fenêtre n'a aucun accès au disque.",
      ),
      h("div", { class: "actions" }, choisir, nomFichier),
      zoneFichier,
      h("div", { class: "actions" }, reanalyser),
      zoneAnalyse,
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, h("span", { class: "etape" }, "3"), "Créer"),
      h("div", { class: "actions" }, seulementPrets.label),
      h("div", { class: "actions" }, autoriserDoublon.label),
      h("div", { class: "actions" }, creer),
      h(
        "p",
        { class: "note" },
        "Tout ou rien par défaut. En cas d'erreur de Dokos en cours de route, l'import s'arrête et le rapport dit exactement ce qui a été créé.",
      ),
      zoneCreation,
    ),
  );

  let charge = false;
  panneau.addEventListener("onglet-affiche", () => {
    if (charge) return;
    charge = true;
    void chargerCas();
  });
}
