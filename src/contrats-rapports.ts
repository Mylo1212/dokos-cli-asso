// Blocs d'affichage de l'onglet Contrats : problèmes de données, rapport de
// conformité d'un format, propositions (fichier et IA).

import { api, type ProblemeVue, type PropositionFormatIa, type RapportVue, type VerificationFormatVue } from "./api";
import { alerte, bouton, h, lienFichier, occupe } from "./dom";

export function blocProblemes(problemes: ProblemeVue[]): HTMLElement {
  if (problemes.length === 0) return alerte("succes", "Données du contrat complètes et cohérentes");
  const erreurs = problemes.filter((p) => p.gravite === "erreur").length;
  return alerte(
    erreurs > 0 ? "erreur" : "attention",
    erreurs > 0 ? `${erreurs} erreur(s) : le contrat ne sera pas créé` : "Points d'attention",
    h(
      "ul",
      { class: "liste-resultats" },
      ...problemes.map((p) =>
        h(
          "li",
          {},
          h(
            "span",
            { class: `marque-etat ${p.gravite === "erreur" ? "ko" : "alerte-etat"}` },
            p.gravite === "erreur" ? "✗" : "!",
          ),
          h("span", {}, p.message, p.reference ? h("span", { class: "reference" }, ` — ${p.reference}`) : null),
        ),
      ),
    ),
  );
}

export function blocRapport(rapport: RapportVue, titre: string): HTMLElement {
  const elements = rapport.mentions.map((m) =>
    h(
      "li",
      {},
      h("span", { class: `marque-etat ${m.etat === "presente" ? "ok" : "ko"}` }, m.etat === "presente" ? "✓" : "✗"),
      h(
        "span",
        {},
        m.libelle,
        m.detail ? ` — ${m.detail}` : "",
        h("span", { class: "reference" }, ` — ${m.reference}`),
      ),
    ),
  );
  for (const i of rapport.interdits) {
    elements.push(h("li", {}, h("span", { class: "marque-etat ko" }, "✗"), h("span", {}, `Sécurité : ${i}`)));
  }
  for (const n of rapport.nonVerifies) {
    elements.push(
      h("li", {}, h("span", { class: "marque-etat alerte-etat" }, "?"), h("span", {}, `Non vérifié : ${n}`)),
    );
  }
  if (rapport.aCompleter > 0) {
    elements.push(
      h(
        "li",
        {},
        h("span", { class: "marque-etat ko" }, "✗"),
        h("span", {}, `${rapport.aCompleter} texte(s) « [à compléter] » restant(s)`),
      ),
    );
  }
  return alerte(
    rapport.conforme ? "succes" : "attention",
    `${titre} : ${rapport.conforme ? "conforme à la liste des mentions" : "non conforme"}`,
    h("ul", { class: "liste-resultats" }, ...elements),
  );
}

export function blocsFormat(
  v: VerificationFormatVue,
  format: string,
  typeContrat: string,
  tempsPartiel: boolean,
): HTMLElement[] {
  const blocs: HTMLElement[] = [blocRapport(v.rapport, `Format « ${format} »`)];
  const p = v.proposition;
  if (!p) return blocs;
  const zoneFichier = h("div");
  const enregistrer = bouton("Enregistrer la proposition", "principal", async () => {
    const chemin = await occupe([enregistrer], zoneFichier, () =>
      api.enregistrerProposition(format, typeContrat, tempsPartiel),
    );
    if (chemin) {
      zoneFichier.replaceChildren(
        alerte(
          "succes",
          "Proposition enregistrée",
          h("p", { class: "chemin" }, chemin),
          h(
            "p",
            {},
            "Relisez-la, remplacez les « [à compléter] », puis collez-la dans le format d'impression de Dokos.",
          ),
          h("div", { class: "actions" }, lienFichier(chemin)),
        ),
      );
    }
  });
  blocs.push(
    h(
      "div",
      { class: "alerte info" },
      h("strong", {}, "Proposition de modification"),
      h(
        "ul",
        { class: "liste-resultats" },
        ...p.ajouts.map((a) => h("li", {}, h("span", { class: "marque-etat ok" }, "+"), h("span", {}, a))),
      ),
      p.champsACreer.length > 0
        ? h("p", {}, "Champs à créer dans « Employment Contract » : ", h("strong", {}, p.champsACreer.join(", ")))
        : null,
      p.aCompleter > 0 ? h("p", {}, `${p.aCompleter} texte(s) « [à compléter] » à remplacer avant usage.`) : null,
      h("div", { class: "diff" }, ...p.diff.map((l) => h("div", { class: l.genre }, l.texte))),
      h("div", { class: "actions" }, enregistrer),
      zoneFichier,
    ),
  );
  return blocs;
}

export function blocFormatIa(p: PropositionFormatIa): HTMLElement {
  return h(
    "div",
    { class: "alerte info" },
    h("strong", {}, "Mentions rédigées avec l'IA"),
    h(
      "ul",
      { class: "liste-resultats" },
      ...p.ajouts.map((a) =>
        h(
          "li",
          {},
          h("span", { class: `marque-etat ${a.parIa ? "ok" : "alerte-etat"}` }, a.parIa ? "✦" : "↺"),
          h(
            "span",
            {},
            a.libelle,
            h(
              "span",
              { class: "reference" },
              a.parIa ? " — rédigée par l'IA, vérifiée" : ` — texte de la liste repris (${a.raison ?? "bloc refusé"})`,
            ),
          ),
        ),
      ),
    ),
    p.explications ? h("p", {}, p.explications) : null,
    p.champsACreer.length > 0
      ? h("p", {}, "Champs à créer dans « Employment Contract » : ", h("strong", {}, p.champsACreer.join(", ")))
      : null,
    p.aCompleter > 0 ? h("p", {}, `${p.aCompleter} texte(s) « [à compléter] » à remplacer avant usage.`) : null,
    ...p.avertissements.map((a) => h("p", { class: "reference" }, a)),
    h("div", { class: "diff" }, ...p.diff.map((l) => h("div", { class: l.genre }, l.texte))),
    h(
      "p",
      {},
      "Proposition enregistrée : ",
      h("span", { class: "chemin" }, p.chemin),
      ". À relire par la personne qui valide les contrats, puis à coller dans le format de Dokos.",
    ),
    h("div", { class: "actions" }, lienFichier(p.chemin)),
  );
}
