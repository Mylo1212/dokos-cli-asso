// Onglet Suivi : mes brouillons, transformation devis → commande → facture,
// inscrits d'un événement, factures en retard. Lecture seule, sauf
// la transformation qui crée un brouillon.

import { api, type BrouillonVue, type BrouillonsVue, type EvenementVue, type ImpayesVue } from "./api";
import {
  alerte,
  alerteErreur,
  bouton,
  h,
  lienDokos,
  lienFichier,
  montant,
  occupe,
  saisie,
  selection,
  tableau,
} from "./dom";
import { chargerSuivi, donnees, perimerSuivi } from "./donnees";
import type { Ressource } from "./ressource";
import { dateLisible, heureCourte } from "./valeurs";

/**
 * Branche une zone de la page sur une ressource : la dernière valeur lue
 * s'affiche tout de suite, même pendant qu'elle se rafraîchit ; l'erreur ne
 * remplace la zone que s'il n'y a encore rien à montrer.
 */
function brancher<T>(zone: HTMLElement, r: Ressource<T>, afficher: (valeur: T) => (Node | string)[]): void {
  const maj = () => {
    const etat = r.etat();
    zone.classList.toggle("en-cours", etat.enCours);
    if (etat.erreur !== undefined) {
      const alerte = alerteErreur(etat.erreur, () => void r.charger(true));
      zone.replaceChildren(alerte, ...(etat.valeur !== undefined ? afficher(etat.valeur) : []));
    } else if (etat.valeur !== undefined) zone.replaceChildren(...afficher(etat.valeur));
    else zone.replaceChildren(h("p", { class: "note sans-marge" }, "Lecture de Dokos…"));
  };
  r.abonner(maj);
  maj();
}

export function initialiserSuivi(panneau: HTMLElement): void {
  // --- Mes brouillons ---
  const zoneBrouillons = h("div", { "aria-live": "polite" });

  function ligneBrouillon(b: BrouillonVue): (string | Node)[] {
    const genre = { devis: "Devis", commande: "Commande", facture: "Facture", evenement: "Événement" }[b.genre];
    const actions = h("span", { class: "actions-ligne" }, lienDokos(b.lien));
    if (b.genre === "devis") actions.append(boutonTransformation("Commander", "devis_en_commande", b.id));
    if (b.genre === "commande") actions.append(boutonTransformation("Facturer", "commande_en_facture", b.id));
    return [
      genre,
      b.id,
      b.tiers,
      dateLisible(b.date),
      b.genre === "evenement" ? "non publié" : montant(b.total, b.devise),
      actions,
    ];
  }

  function boutonTransformation(
    texte: string,
    genre: "devis_en_commande" | "commande_en_facture",
    source: string,
  ): HTMLElement {
    const zone = h("span", { class: "avec-erreur" });
    const b = bouton(texte, "discret", async () => {
      const r = await occupe([b], zone, () => api.transformerDocument(genre, source, false));
      if (!r) return;
      // Le nouveau brouillon apparaîtra à la prochaine actualisation (pas tout de
      // suite : la liste se redessinerait et ce message disparaîtrait).
      if (r.cree) perimerSuivi();
      zone.replaceChildren(
        alerte(
          r.cree ? "succes" : "attention",
          r.cree
            ? `${r.document.id} créé en brouillon (${montant(r.document.total, r.document.devise)})`
            : `Un document suivant existe déjà : ${r.document.id}`,
          h("p", {}, "Le document source doit être validé dans Dokos pour qu'une transformation aboutisse."),
          h("div", { class: "actions" }, lienDokos(r.document.lien)),
        ),
      );
    });
    return h("span", {}, b, zone);
  }

  const afficherBrouillons = (v: BrouillonsVue): (Node | string)[] => {
    const lignes = [...v.ventes, ...v.evenements].map(ligneBrouillon);
    return [
      ...v.avertissements.map((a) => alerte("info", "Profil non configuré", a)),
      lignes.length === 0
        ? h("p", { class: "note sans-marge" }, "Aucun brouillon créé par l'outil en attente.")
        : h(
            "div",
            { class: "defilement" },
            tableau(["Type", "Identifiant", "Client / titre", "Date", "Total", ""], lignes),
          ),
    ];
  };

  // --- Inscrits ---
  const evenements = selection([], "Choisir un événement…");
  const zoneInscrits = h("div", { "aria-live": "polite" });
  let listeEvenements: EvenementVue[] = [];
  const voirInscrits = bouton("Voir les inscrits", "principal", async () => {
    if (!evenements.value)
      return zoneInscrits.replaceChildren(alerte("attention", "À compléter", "Choisissez un événement."));
    const v = await occupe([voirInscrits], zoneInscrits, () => api.inscritsEvenement(evenements.value));
    if (!v) return;
    const zoneExport = h("div");
    const exporter = bouton("Exporter en Excel", "", async () => {
      const chemin = await occupe([exporter], zoneExport, () => api.exporterInscrits(v.evenement));
      if (chemin)
        zoneExport.replaceChildren(
          alerte(
            "succes",
            "Liste enregistrée",
            h("p", { class: "chemin" }, chemin),
            h("div", { class: "actions" }, lienFichier(chemin)),
          ),
        );
    });
    zoneInscrits.replaceChildren(
      alerte("info", `${v.lignes.length} inscrit(s)`, h("div", { class: "actions" }, exporter)),
      zoneExport,
      ...(v.lignes.length > 0
        ? [
            h(
              "div",
              { class: "defilement" },
              tableau(
                v.colonnes.map((c) => c.libelle),
                v.lignes.map((l) => v.colonnes.map((c) => l[c.champ] ?? "")),
              ),
            ),
          ]
        : []),
    );
  });
  donnees.evenements.abonner(() => {
    const etat = donnees.evenements.etat();
    if (etat.erreur !== undefined && etat.valeur === undefined) {
      zoneInscrits.replaceChildren(alerteErreur(etat.erreur, () => void donnees.evenements.charger(true)));
      return;
    }
    if (etat.valeur === undefined || etat.valeur === listeEvenements) return;
    listeEvenements = etat.valeur;
    const choisi = evenements.value;
    evenements.replaceChildren(
      h("option", { value: "" }, "Choisir un événement…"),
      ...listeEvenements.map((e) =>
        h("option", { value: e.id }, `${e.titre} — ${dateLisible(e.debut)}${e.publie ? "" : " (non publié)"}`),
      ),
    );
    if (listeEvenements.some((e) => e.id === choisi)) evenements.value = choisi;
  });

  // --- Factures en retard ---
  const zoneImpayes = h("div", { "aria-live": "polite" });
  const afficherImpayes = (v: ImpayesVue): (Node | string)[] => {
    if (v.factures.length === 0) return [alerte("succes", "Aucune facture en retard")];
    return [
      alerte(
        "attention",
        `${v.factures.length} facture(s) en retard, ${montant(v.total, v.devise)} à encaisser`,
        h("p", {}, "La relance se fait dans Dokos ; cette liste est en lecture seule."),
      ),
      h("h3", {}, "Par client"),
      tableau(
        ["Client", "Solde", "Factures"],
        v.parClient.map((c) => [c.client, montant(c.solde, v.devise), String(c.nombre)]),
      ),
      h("h3", {}, "Factures"),
      h(
        "div",
        { class: "defilement" },
        tableau(
          ["Facture", "Client", "Échéance", "Retard", "Solde", ""],
          v.factures.map((f) => [
            f.id,
            f.client,
            dateLisible(f.echeance),
            `${f.retard} j`,
            montant(f.solde, f.devise),
            lienDokos(f.lien),
          ]),
        ),
      ),
    ];
  };

  const filtreTexte = saisie({ placeholder: "Filtrer les brouillons (identifiant, client)" });
  const filtrer = () => {
    const texte = filtreTexte.value.trim().toLowerCase();
    for (const ligne of zoneBrouillons.querySelectorAll<HTMLTableRowElement>("tbody tr")) {
      ligne.hidden = texte !== "" && !ligne.textContent?.toLowerCase().includes(texte);
    }
  };
  filtreTexte.addEventListener("input", filtrer);

  brancher(zoneBrouillons, donnees.brouillons, afficherBrouillons);
  donnees.brouillons.abonner(filtrer);
  brancher(zoneImpayes, donnees.impayes, afficherImpayes);

  // --- Une seule actualisation pour toute la page ---
  const suivies = [donnees.brouillons, donnees.impayes];
  const fraicheur = h("span", { class: "fraicheur" });
  const actualiser = bouton("Actualiser", "discret", async () => {
    await Promise.all([chargerSuivi(true), donnees.evenements.charger(true)]);
  });
  const majFraicheur = () => {
    const enCours = suivies.some((r) => r.etat().enCours);
    const dates = suivies.map((r) => r.etat().date).filter((d): d is number => d !== null);
    fraicheur.textContent = enCours
      ? "Lecture de Dokos…"
      : dates.length > 0
        ? `Actualisé à ${heureCourte(Math.min(...dates))}`
        : "";
    actualiser.disabled = enCours;
  };
  for (const r of suivies) r.abonner(majFraicheur);

  panneau.append(
    h(
      "div",
      { class: "entete-page" },
      h(
        "div",
        {},
        h("h1", {}, "Suivi"),
        h(
          "p",
          { class: "sous-titre" },
          "Ce que l'outil a créé et qui attend, et ce qui mérite un œil. Tout est en lecture seule, sauf « Commander » et « Facturer » qui créent un brouillon.",
        ),
      ),
      h("div", { class: "entete-actions" }, fraicheur, actualiser),
    ),
    h(
      "div",
      { class: "carte", id: "suivi-brouillons" },
      h("h2", {}, "Mes brouillons"),
      h("div", { class: "actions sans-marge" }, filtreTexte),
      zoneBrouillons,
    ),
    h("div", { class: "carte", id: "suivi-impayes" }, h("h2", {}, "Factures en retard"), zoneImpayes),
    h(
      "div",
      { class: "carte", id: "suivi-inscrits" },
      h("h2", {}, "Inscrits d'un événement"),
      h("div", { class: "grille" }, h("label", { class: "champ large" }, "Événement", evenements)),
      h("div", { class: "actions" }, voirInscrits),
      zoneInscrits,
    ),
  );

  // À chaque visite : ce qui a plus de deux minutes, ou qu'une création a
  // rendu caduc, est relu ; le reste s'affiche sans attendre.
  majFraicheur();
  panneau.addEventListener("onglet-affiche", () => {
    void chargerSuivi();
    void donnees.evenements.charger();
  });
}
