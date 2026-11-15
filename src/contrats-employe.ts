// Onglet Contrats, étape 1 : employé existant (recherche) ou nouvel employé.

import { api, type EmployeVue, type ListesRh } from "./api";
import { alerte, aujourdhui, bouton, champ, h, lienDokos, occupe, recherche, saisie, selection, valeur } from "./dom";

export interface SectionEmploye {
  carte: HTMLDivElement;
  /** Employé choisi ou créé. */
  employe: () => EmployeVue | null;
  choisir: (e: EmployeVue | null) => void;
  saisir: (texte: string) => void;
  /** Bascule vers « Nouvel employé » et pré-remplit. */
  proposerNouveau: (p: {
    prenom: string;
    nom: string;
    naissance: string | null;
    genre: string | null;
    entree: string | null;
    poste: string | null;
  }) => void;
  basculer: (estNouveau: boolean) => void;
}

export function creerSectionEmploye(
  listes: ListesRh,
  options: {
    /** Appelé quand l'employé change, avec sa date d'entrée et son poste éventuels. */
    surEmploye: (e: EmployeVue | null, entree: string | null, poste: string | null) => void;
  },
): SectionEmploye {
  let employe: EmployeVue | null = null;
  const zoneEmploye = h("div", { "aria-live": "polite" });
  const choixEmploye = recherche<EmployeVue>({
    placeholder: "Nom ou identifiant d'un employé actif",
    chercher: api.chercherEmployes,
    libelle: (e) => e.nom || e.id,
    identifiant: (e) => e.id,
    surChoix: (e) => {
      employe = e;
      options.surEmploye(e, e?.entree || null, null);
    },
  });

  const prenom = saisie();
  const nomFamille = saisie();
  const genre = selection(listes.genres, "Choisir…");
  const naissance = saisie({ type: "date" });
  const entreeEmploye = saisie({ type: "date", value: aujourdhui() });
  const email = saisie({ type: "email", placeholder: "Facultatif" });
  const posteEmploye = saisie({ list: "postes", placeholder: "Facultatif" });
  const nouveau = h(
    "div",
    { hidden: true },
    h(
      "div",
      { class: "grille" },
      champ("Prénom", prenom),
      champ("Nom", nomFamille),
      champ("Genre", genre),
      champ("Date de naissance", naissance),
      champ("Date d'entrée", entreeEmploye),
      champ("E-mail personnel", email),
      champ("Poste", posteEmploye),
    ),
  );
  const creerEmploye = bouton("Créer l'employé", "principal", async () => {
    if (!listes.societe) {
      return zoneEmploye.replaceChildren(
        alerte("attention", "Société manquante", h("p", {}, "Renseignez-la dans Réglages.")),
      );
    }
    const manque = (
      [
        [prenom, "le prénom"],
        [nomFamille, "le nom"],
        [genre, "le genre"],
        [naissance, "la date de naissance"],
        [entreeEmploye, "la date d'entrée"],
      ] as [HTMLInputElement | HTMLSelectElement, string][]
    ).find(([c]) => !valeur(c));
    if (manque) return zoneEmploye.replaceChildren(alerte("attention", "À compléter", `Renseignez ${manque[1]}.`));
    const r = await occupe([creerEmploye], zoneEmploye, () =>
      api.creerEmploye({
        prenom: prenom.value,
        nom: nomFamille.value,
        genre: genre.value,
        naissance: naissance.value,
        entree: entreeEmploye.value,
        email: valeur(email),
        poste: valeur(posteEmploye),
      }),
    );
    if (!r) return;
    choixEmploye.choisir(r.employe);
    basculer(false);
    options.surEmploye(r.employe, entreeEmploye.value, valeur(posteEmploye));
    zoneEmploye.replaceChildren(
      alerte(
        r.cree ? "succes" : "info",
        r.cree ? `Employé ${r.employe.id} créé` : `Employé déjà dans Dokos : ${r.employe.id}`,
        h("p", {}, r.employe.nom),
        h("div", { class: "actions" }, lienDokos(r.employe.lien)),
      ),
    );
  });
  nouveau.append(h("div", { class: "actions" }, creerEmploye));

  const existant = h("div", {}, champ("Employé", choixEmploye.racine));
  const basculeExistant = h("button", { type: "button", "aria-pressed": "true" }, "Employé existant");
  const basculeNouveau = h("button", { type: "button", "aria-pressed": "false" }, "Nouvel employé");
  function basculer(estNouveau: boolean) {
    basculeExistant.setAttribute("aria-pressed", String(!estNouveau));
    basculeNouveau.setAttribute("aria-pressed", String(estNouveau));
    existant.hidden = estNouveau;
    nouveau.hidden = !estNouveau;
  }
  basculeExistant.addEventListener("click", () => basculer(false));
  basculeNouveau.addEventListener("click", () => basculer(true));

  const carte = h(
    "div",
    { class: "carte" },
    h("h2", {}, h("span", { class: "etape" }, "1"), "Employé"),
    h("div", { class: "segments", role: "group" }, basculeExistant, basculeNouveau),
    existant,
    nouveau,
    zoneEmploye,
  );

  return {
    carte,
    employe: () => employe,
    choisir: (e) => {
      basculer(false);
      choixEmploye.choisir(e);
    },
    saisir: (texte) => {
      basculer(false);
      choixEmploye.saisir(texte);
    },
    proposerNouveau: (p) => {
      basculer(true);
      prenom.value = p.prenom;
      nomFamille.value = p.nom;
      naissance.value = p.naissance ?? "";
      genre.value = p.genre ?? "";
      if (p.entree) entreeEmploye.value = p.entree;
      if (p.poste) posteEmploye.value = p.poste;
    },
    basculer,
  };
}
