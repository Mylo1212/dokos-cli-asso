// Ventes : mode « à partir d'un modèle ». Le modèle est un devis, une
// commande ou une facture de Dokos portant l'étiquette « Template ». On
// choisit le client (carte Client de l'onglet), les dates d'arrivée et de
// départ et les quantités ; tout le reste vient du modèle.

import { api, type ModeleVente, type ResumeModele, type TypeVente } from "./api";
import { alerte, alerteErreur, champ, h, lienDokos, montant, saisie, tableau } from "./dom";
import { demandeVenteModele } from "./valeurs";
import type { DemandeVenteModele } from "./api";

export interface SectionModele {
  carte: HTMLDivElement;
  /** Recharge la liste des modèles du type donné. */
  charger: (typeVente: TypeVente) => void;
  demande: (client: string | null) => DemandeVenteModele | string;
}

export function creerSectionModele(): SectionModele {
  let typeVente: TypeVente = "devis";
  let modele: ModeleVente | null = null;
  let quantites: { index: number; input: HTMLInputElement }[] = [];
  let numero = 0;

  const choix = h("select", { "aria-label": "Modèle" });
  const lien = h("span");
  const arrivee = saisie({ type: "date" });
  const depart = saisie({ type: "date" });
  const zoneLignes = h("div", { "aria-live": "polite" });
  const note = h("p", { class: "note" });

  async function charger(type: TypeVente) {
    typeVente = type;
    const demande = ++numero;
    modele = null;
    quantites = [];
    lien.replaceChildren();
    choix.replaceChildren(h("option", { value: "" }, "Chargement des modèles…"));
    zoneLignes.replaceChildren();
    try {
      const liste: ResumeModele[] = await api.modelesVente(type);
      if (demande !== numero) return;
      choix.replaceChildren(
        h("option", { value: "" }, liste.length === 0 ? "Aucun modèle « Template » de ce type" : "Choisir un modèle…"),
        ...liste.map((m) => h("option", { value: m.id }, `${m.titre} — ${montant(m.total, m.devise)} (${m.id})`)),
      );
      if (liste.length === 0) {
        zoneLignes.replaceChildren(
          alerte(
            "info",
            "Aucun modèle",
            h(
              "p",
              {},
              "Dans Dokos, ajoutez l'étiquette « Template » à un document de ce type pour qu'il serve de modèle.",
            ),
          ),
        );
      }
    } catch (e) {
      if (demande !== numero) return;
      choix.replaceChildren(h("option", { value: "" }, "Modèles indisponibles"));
      zoneLignes.replaceChildren(alerteErreur(e, () => void charger(type)));
    }
  }

  async function choisir(id: string) {
    const demande = ++numero;
    modele = null;
    quantites = [];
    lien.replaceChildren();
    if (!id) return zoneLignes.replaceChildren();
    zoneLignes.replaceChildren(h("p", { class: "note sans-marge" }, "Lecture du modèle…"));
    try {
      const m = await api.modeleVente(typeVente, id);
      if (demande !== numero) return;
      modele = m;
      lien.replaceChildren(lienDokos(m.lien, "Ouvrir le modèle dans Dokos"));
      quantites = m.lignes.map((l) => ({
        index: l.index,
        input: saisie({
          type: "number",
          min: "0",
          step: "any",
          inputmode: "decimal",
          value: String(l.quantite),
          "aria-label": `Quantité ${l.libelle || l.code}`,
        }),
      }));
      zoneLignes.replaceChildren(
        h(
          "div",
          { class: "defilement" },
          tableau(
            ["Article", "Prix du modèle", "Quantité", "Unité"],
            m.lignes.map((l, i) => [
              h("span", {}, l.libelle || l.code, h("span", { class: "id" }, ` ${l.code}`)),
              montant(l.prix, m.devise),
              quantites[i]!.input,
              l.unite,
            ]),
          ),
        ),
      );
    } catch (e) {
      if (demande !== numero) return;
      zoneLignes.replaceChildren(alerteErreur(e, () => void choisir(id)));
    }
  }

  choix.addEventListener("change", () => void choisir(choix.value));

  function majNote() {
    note.textContent =
      typeVente === "commande"
        ? "Date de la commande : aujourd'hui ; date de livraison : la date de départ. Videz une quantité ou mettez 0 pour retirer l'article."
        : "Date du document : aujourd'hui. Videz une quantité ou mettez 0 pour retirer l'article.";
  }

  const carte = h(
    "div",
    { class: "carte" },
    h("h2", {}, "Modèle"),
    h("div", { class: "grille" }, champ("Modèle « Template »", choix, "large")),
    h("div", { class: "actions" }, lien),
    h("div", { class: "grille" }, champ("Date d'arrivée", arrivee), champ("Date de départ", depart)),
    zoneLignes,
    note,
    h(
      "p",
      { class: "note" },
      "Prix, remises, taxes, conditions de paiement et conditions générales sont ceux du modèle, repris tels quels. Le modèle n'est pas modifié.",
    ),
  );

  return {
    carte,
    charger: (type) => {
      majNote();
      void charger(type);
    },
    demande: (client) =>
      demandeVenteModele({
        typeVente,
        modele: modele ? { id: modele.id, modifie: modele.modifie } : null,
        client,
        arrivee: arrivee.value,
        depart: depart.value,
        quantites: quantites.map((q) => ({ index: q.index, texte: q.input.value })),
      }),
  };
}
