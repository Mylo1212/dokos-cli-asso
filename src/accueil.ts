// Accueil : ce qui attend dans Dokos (brouillons, factures en retard,
// échéances de contrats), les créations en un clic et les derniers brouillons.
// Lecture seule ; les mêmes données que la section Suivi (donnees.ts).

import type { BrouillonVue } from "./api";
import { bouton, h, lienDokos, montant, tableau } from "./dom";
import { chargerSuivi, donnees } from "./donnees";
import { ouvrirOnglet, signaler, type Onglet } from "./etat";
import { icone, type NomIcone } from "./icones";
import { resumeConnexions } from "./navigation";
import type { EtatRessource } from "./ressource";
import { actionPourErreur, dateLisible, erreurVue, heureCourte, pluriel, titreErreur } from "./valeurs";

const GENRES: Record<BrouillonVue["genre"], string> = {
  devis: "Devis",
  commande: "Commande",
  facture: "Facture",
  evenement: "Événement",
};

/** Tuile chiffrée : un libellé, une valeur, une ligne de détail ; ouvre le bloc correspondant du suivi. */
function tuile(libelle: string, ancre: string) {
  const valeur = h("span", { class: "tuile-valeur" }, "…");
  const detail = h("span", { class: "tuile-detail" }, "Lecture de Dokos…");
  let action = () => ouvrirOnglet("suivi", ancre);
  const racine = h(
    "button",
    { type: "button", class: "tuile" },
    h("span", { class: "tuile-libelle" }, libelle),
    valeur,
    detail,
  );
  racine.addEventListener("click", () => action());

  /** Affiche l'état d'une ressource ; `lire` donne la valeur et le détail quand elle est connue. */
  function montrer<T>(
    etat: EtatRessource<T>,
    recharger: () => void,
    lire: (v: T) => { nombre: number; detail: string },
  ) {
    racine.classList.toggle("en-cours", etat.enCours);
    if (etat.valeur !== undefined) {
      const r = lire(etat.valeur);
      valeur.textContent = String(r.nombre);
      detail.textContent = r.detail;
      racine.classList.toggle("vide", r.nombre === 0);
      racine.classList.remove("indisponible");
      racine.removeAttribute("title");
      action = () => ouvrirOnglet("suivi", ancre);
      return;
    }
    racine.classList.remove("vide");
    racine.classList.toggle("indisponible", etat.erreur !== undefined);
    if (etat.erreur === undefined) {
      valeur.textContent = "…";
      detail.textContent = "Lecture de Dokos…";
      return;
    }
    const e = erreurVue(etat.erreur);
    valeur.textContent = "—";
    const versReglages = actionPourErreur(e.genre) === "reglages";
    const raison = e.genre === "configuration" ? "À configurer" : titreErreur(e.genre);
    detail.textContent = `${raison} · ${versReglages ? "ouvrir Réglages" : "réessayer"}`;
    racine.title = e.message;
    action = versReglages ? () => ouvrirOnglet("reglages") : recharger;
  }
  return { racine, montrer };
}

function raccourci(titre: string, detail: string, nom: NomIcone, action: () => void): HTMLButtonElement {
  const b = h(
    "button",
    { type: "button", class: "raccourci" },
    icone(nom),
    h("span", { class: "raccourci-titre" }, titre),
    h("span", { class: "raccourci-detail" }, detail),
  );
  b.addEventListener("click", action);
  return b;
}

export function creerAccueil(panneau: HTMLElement): void {
  const jour = new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long" }).format(new Date());
  const fraicheur = h("span", { class: "fraicheur" });
  const actualiser = bouton("Actualiser", "discret", async () => {
    void donnees.connexions.charger(true);
    await chargerSuivi(true);
  });
  const zoneConnexion = h("div", { "aria-live": "polite" });

  const brouillons = tuile("Brouillons en attente", "suivi-brouillons");
  const impayes = tuile("Factures en retard", "suivi-impayes");
  const fins = tuile("Fins de contrat sous 30 jours", "suivi-rh");
  const sansFin = tuile("CDD sans date de fin", "suivi-rh");
  const zoneDerniers = h("div", { "aria-live": "polite" });

  const vente = (type: "devis" | "commande" | "facture") => () => {
    ouvrirOnglet("ventes");
    signaler("dokos:nouvelle-vente", type);
  };
  const vers = (onglet: Onglet) => () => ouvrirOnglet(onglet);

  function majBrouillons() {
    const etat = donnees.brouillons.etat();
    brouillons.montrer(
      etat,
      () => void donnees.brouillons.charger(true),
      (v) => {
        const tous = [...v.ventes, ...v.evenements];
        const parGenre = (Object.keys(GENRES) as BrouillonVue["genre"][])
          .map((g) => ({ genre: g, nombre: tous.filter((b) => b.genre === g).length }))
          .filter((g) => g.nombre > 0)
          .map((g) => `${g.nombre} ${GENRES[g.genre].toLowerCase()}`);
        return {
          nombre: tous.length,
          detail: tous.length === 0 ? "Rien à valider dans Dokos" : parGenre.join(" · "),
        };
      },
    );
    const v = etat.valeur;
    if (!v) return zoneDerniers.replaceChildren();
    const derniers = [...v.ventes, ...v.evenements].sort((a, b) => b.modifie.localeCompare(a.modifie)).slice(0, 5);
    zoneDerniers.replaceChildren(
      h("h2", { class: "titre-bloc" }, "Derniers brouillons"),
      ...(derniers.length === 0
        ? [h("p", { class: "note sans-marge" }, "Aucun brouillon créé par l'outil n'attend de validation.")]
        : [
            h(
              "div",
              { class: "carte sans-rembourrage" },
              tableau(
                ["Type", "Identifiant", "Client / titre", "Date", "Total", ""],
                derniers.map((b) => [
                  GENRES[b.genre],
                  h("span", { class: "id" }, b.id),
                  b.tiers,
                  dateLisible(b.date),
                  b.genre === "evenement" ? "non publié" : montant(b.total, b.devise),
                  lienDokos(b.lien),
                ]),
              ),
            ),
            h(
              "div",
              { class: "actions" },
              bouton("Tout voir dans Suivi", "discret", () => ouvrirOnglet("suivi", "suivi-brouillons")),
            ),
          ]),
    );
  }

  function majImpayes() {
    impayes.montrer(
      donnees.impayes.etat(),
      () => void donnees.impayes.charger(true),
      (v) => ({
        nombre: v.factures.length,
        detail:
          v.factures.length === 0
            ? "Aucune échéance dépassée"
            : `${montant(v.total, v.devise)} à encaisser · ${pluriel(v.parClient.length, "client")}`,
      }),
    );
  }

  function majRh() {
    const etat = donnees.rh.etat();
    const recharger = () => void donnees.rh.charger(true);
    fins.montrer(etat, recharger, (v) => ({
      nombre: v.finsProches.length,
      detail:
        v.finsProches.length === 0
          ? "Aucune fin de contrat proche"
          : `Prochaine : ${v.finsProches[0]!.nom || v.finsProches[0]!.employe}, dans ${v.finsProches[0]!.jours} j`,
    }));
    sansFin.montrer(etat, recharger, (v) => ({
      nombre: v.cddSansFin.length,
      detail: v.cddSansFin.length === 0 ? "Tous les CDD ont une date de fin" : "Date de fin à renseigner dans Dokos",
    }));
  }

  function majFraicheur() {
    const dates = [donnees.brouillons, donnees.impayes, donnees.rh].map((r) => r.etat().date);
    const enCours = [donnees.brouillons, donnees.impayes, donnees.rh].some((r) => r.etat().enCours);
    const connues = dates.filter((d): d is number => d !== null);
    fraicheur.textContent = enCours
      ? "Lecture de Dokos…"
      : connues.length > 0
        ? `Actualisé à ${heureCourte(Math.min(...connues))}`
        : "";
    actualiser.disabled = enCours;
  }

  function majConnexion() {
    const resume = resumeConnexions(donnees.connexions.etat().valeur);
    zoneConnexion.replaceChildren(
      ...(resume.niveau === "erreur"
        ? [
            h(
              "div",
              { class: "alerte attention", role: "status" },
              h("strong", {}, resume.texte),
              h("p", { class: "pre" }, resume.detail),
              h(
                "div",
                { class: "actions" },
                bouton("Ouvrir Réglages", "", () => ouvrirOnglet("reglages")),
              ),
            ),
          ]
        : []),
    );
  }

  donnees.brouillons.abonner(() => (majBrouillons(), majFraicheur()));
  donnees.impayes.abonner(() => (majImpayes(), majFraicheur()));
  donnees.rh.abonner(() => (majRh(), majFraicheur()));
  donnees.connexions.abonner(majConnexion);

  panneau.append(
    h(
      "div",
      { class: "entete-page" },
      h("div", {}, h("h1", {}, "Bonjour"), h("p", { class: "sous-titre" }, `Nous sommes ${jour}.`)),
      h("div", { class: "entete-actions" }, fraicheur, actualiser),
    ),
    zoneConnexion,
    h("h2", { class: "titre-bloc" }, "À traiter"),
    h("div", { class: "tuiles" }, brouillons.racine, impayes.racine, fins.racine, sansFin.racine),
    h("h2", { class: "titre-bloc" }, "Créer"),
    h(
      "div",
      { class: "raccourcis" },
      raccourci("Devis", "Une proposition chiffrée", "ventes", vente("devis")),
      raccourci("Commande", "Une prestation confirmée", "ventes", vente("commande")),
      raccourci("Facture", "À valider ensuite dans Dokos", "ventes", vente("facture")),
      raccourci("Événement", "Avec son formulaire d'inscription", "formulaires", vers("formulaires")),
      raccourci("Contrat de travail", "Employé, contrat vérifié, PDF", "contrats", vers("contrats")),
      raccourci("Import Excel", "Plusieurs documents d'un coup", "import", vers("import")),
      raccourci("Liste Excel", "Clients, articles, ventes, employés", "listes", vers("listes")),
    ),
    zoneDerniers,
    h(
      "p",
      { class: "note astuce" },
      h("kbd", {}, "Ctrl"),
      " ",
      h("kbd", {}, "K"),
      " : chercher un client, un article ou une action, depuis n'importe quelle section.",
    ),
  );

  majBrouillons();
  majImpayes();
  majRh();
  majFraicheur();
  majConnexion();
  panneau.addEventListener("onglet-affiche", () => void chargerSuivi());
}
