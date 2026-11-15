// Onglet Contrats, étape 3 : contrôle d'un format d'impression, proposition
// de correction (fichier des mentions ou IA).

import { api, type ListesRh } from "./api";
import { blocFormatIa, blocsFormat } from "./contrats-rapports";
import { alerte, aujourdhui, bouton, caseACocher, champ, h, occupe, selection } from "./dom";
import { assistantActif, surChangementAssistant } from "./etat";

export function creerSectionFormat(listes: ListesRh): HTMLDivElement {
  const formatAVerifier = selection(listes.formats, "Choisir…");
  const typeFormat = selection(listes.typesContrat, "Choisir…");
  const tempsPartiel = caseACocher("Inclure les mentions du temps partiel");
  const zoneFormat = h("div", { "aria-live": "polite" });
  const incomplet = () =>
    zoneFormat.replaceChildren(alerte("attention", "À compléter", "Choisissez le format et le type de contrat."));

  const verifierLeFormat = bouton("Vérifier le format", "principal", async () => {
    if (!formatAVerifier.value || !typeFormat.value) return incomplet();
    const v = await occupe([verifierLeFormat], zoneFormat, () =>
      api.verifierFormat(formatAVerifier.value, typeFormat.value, tempsPartiel.input.checked),
    );
    if (v)
      zoneFormat.replaceChildren(
        ...blocsFormat(v, formatAVerifier.value, typeFormat.value, tempsPartiel.input.checked),
      );
  });
  const redigerIa = bouton("Rédiger avec l'IA", "", async () => {
    if (!formatAVerifier.value || !typeFormat.value) return incomplet();
    const p = await occupe([redigerIa, verifierLeFormat], zoneFormat, () =>
      api.assistantFormat(formatAVerifier.value, typeFormat.value, tempsPartiel.input.checked, aujourdhui()),
    );
    if (p === undefined) return;
    zoneFormat.replaceChildren(
      p === null
        ? alerte("succes", "Aucune mention ne manque", h("p", {}, "Rien à rédiger pour ce format."))
        : blocFormatIa(p),
    );
  });
  redigerIa.hidden = !assistantActif();
  surChangementAssistant(() => (redigerIa.hidden = !assistantActif()));

  return h(
    "div",
    { class: "carte" },
    h("h2", {}, h("span", { class: "etape" }, "3"), "Format d'impression"),
    h("div", { class: "grille" }, champ("Format", formatAVerifier), champ("Type de contrat imprimé", typeFormat)),
    h("div", { class: "actions" }, tempsPartiel.label),
    h("div", { class: "actions" }, verifierLeFormat, redigerIa),
    h(
      "p",
      { class: "note" },
      "Rien n'est modifié dans Dokos : les corrections sont proposées dans un fichier, à relire puis à coller dans le format.",
    ),
    zoneFormat,
  );
}
