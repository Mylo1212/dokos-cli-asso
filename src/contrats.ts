// Onglet Contrats : employé, contrat de travail vérifié, format d'impression.
// Les sous-formulaires sont dans contrats-employe.ts et contrats-format.ts ;
// les blocs d'affichage dans contrats-rapports.ts.

import { api, type ListesRh, type PropositionContrat } from "./api";
import { choixCandidats, enregistrerPreRemplissage } from "./assistant";
import { creerSectionEmploye } from "./contrats-employe";
import { creerSectionFormat } from "./contrats-format";
import { blocProblemes, blocRapport } from "./contrats-rapports";
import {
  alerte,
  alerteErreur,
  bouton,
  champ,
  h,
  lienDokos,
  lienFichier,
  occupe,
  saisie,
  selection,
  valeur,
} from "./dom";
import { perimerSuivi } from "./donnees";
import { demandeContrat, totalHeures } from "./valeurs";

const JOURS = ["Lun", "Mar", "Mer", "Jeu", "Ven", "Sam", "Dim"];

export function initialiserContrats(panneau: HTMLElement): void {
  const banniere = h("div", { class: "banniere" });
  const contenu = h("div");
  panneau.append(
    h("h1", {}, "Contrats"),
    h("p", { class: "sous-titre" }, "Employé, contrat vérifié avant création, et contrôle du format d'impression."),
    banniere,
    contenu,
  );

  let charge = false;
  panneau.addEventListener("onglet-affiche", () => {
    if (charge) return;
    charge = true;
    void charger();
  });

  async function charger() {
    contenu.replaceChildren(h("p", { class: "note" }, "Chargement des listes depuis Dokos…"));
    try {
      const listes = await api.listesRh();
      const alertes: HTMLElement[] = [];
      if (!listes.reglesValidees) {
        alertes.push(
          alerte(
            "attention",
            `Liste des mentions « ${listes.reglesVersion} » non validée`,
            h(
              "p",
              {},
              "Les vérifications utilisent une liste brouillon. Faites-la relire (expert-comptable, juriste) : « dokos contrat regles --exporter ».",
            ),
          ),
        );
      } else if (listes.reglesARelire) {
        alertes.push(
          alerte(
            "attention",
            `Liste des mentions « ${listes.reglesVersion} » validée il y a plus d'un an`,
            h("p", {}, "Le droit du travail bouge : faites-la relire et mettez à jour « valide_le »."),
          ),
        );
      }
      banniere.replaceChildren(...alertes);
      construire(listes);
    } catch (e) {
      charge = false;
      contenu.replaceChildren(alerteErreur(e, () => void ((charge = true), charger())));
    }
  }

  function construire(listes: ListesRh) {
    const listePostes = h("datalist", { id: "postes" }, ...listes.postes.map((p) => h("option", { value: p })));

    // --- 2. Contrat ---
    const typeContrat = selection(listes.typesContrat, "Choisir…");
    const poste = saisie({ list: "postes" });
    const entreeContrat = saisie({ type: "date" });
    const finContrat = saisie({ type: "date" });
    const heures = saisie({ type: "number", min: "0", step: "0.25", inputmode: "decimal", placeholder: "35" });
    const jours = JOURS.map(() => saisie({ type: "number", min: "0", max: "24", step: "0.25", inputmode: "decimal" }));
    const total = h("span", { class: "total" });
    const majTotal = () => {
      const somme = totalHeures(jours.map((j) => j.value));
      total.textContent = somme > 0 ? `Total : ${somme} h` : "";
    };
    for (const j of jours) j.addEventListener("input", majTotal);
    const champsLibres = new Map(listes.champsLibres.map((c) => [c.champ, saisie()]));
    const format = selection(listes.formats, "Aucun");

    // --- 1. Employé ---
    const employe = creerSectionEmploye(listes, {
      surEmploye: (_e, entree, posteEmploye) => {
        if (entree && !entreeContrat.value) entreeContrat.value = entree;
        if (posteEmploye && !valeur(poste)) poste.value = posteEmploye;
      },
    });

    const zoneContrat = h("div", { "aria-live": "polite" });
    const verifierContrat = bouton("Vérifier", "", () => lancerVerification());
    verifierContrat.dataset.raccourci = "verifier";
    const creerContrat = bouton("Créer le contrat", "principal", () => lancerCreation(false));

    function demande() {
      const champs: Record<string, string> = {};
      for (const [nom, input] of champsLibres) champs[nom] = input.value;
      return demandeContrat({
        employe: employe.employe()?.id ?? null,
        typeContrat: typeContrat.value,
        poste: poste.value,
        entree: entreeContrat.value,
        fin: finContrat.value,
        heures: heures.value,
        repartition: jours.map((j) => j.value),
        champs,
        format: format.value,
      });
    }

    async function lancerVerification() {
      const d = demande();
      if (typeof d === "string") return zoneContrat.replaceChildren(alerte("attention", "À compléter", d));
      const v = await occupe([verifierContrat, creerContrat], zoneContrat, () => api.verifierContrat(d));
      if (!v) return;
      zoneContrat.replaceChildren(
        blocProblemes(v.problemes),
        ...(v.gabarit ? [blocRapport(v.gabarit, "Format d'impression choisi")] : []),
      );
    }

    async function lancerCreation(forcer: boolean) {
      const d = demande();
      if (typeof d === "string") return zoneContrat.replaceChildren(alerte("attention", "À compléter", d));
      const r = await occupe([creerContrat, verifierContrat], zoneContrat, () => api.creerContrat(d, forcer));
      if (r?.cree) perimerSuivi();
      if (!r) return;
      const zonePdf = h("div");
      const actions = h("div", { class: "actions" }, lienDokos(r.lien));
      if (d.format) {
        const formatChoisi = d.format;
        const pdf = bouton("Télécharger le PDF", "", async () => {
          const chemin = await occupe([pdf], zonePdf, () => api.pdfContrat(r.id, formatChoisi));
          if (chemin)
            zonePdf.replaceChildren(
              alerte(
                "succes",
                "PDF enregistré",
                h("p", { class: "chemin" }, chemin),
                h("div", { class: "actions" }, lienFichier(chemin)),
              ),
            );
        });
        actions.append(pdf);
      }
      zoneContrat.replaceChildren(
        r.cree
          ? alerte(
              "succes",
              `Contrat ${r.id} créé`,
              h("p", {}, "Il est actif dès maintenant dans Dokos (pas d'état brouillon pour ce document)."),
              actions,
            )
          : alerte(
              "attention",
              `Contrat identique déjà dans Dokos : ${r.id}`,
              h("p", {}, "Même employé, même type, même date d'entrée : rien n'a été créé."),
              actions,
              h(
                "div",
                { class: "actions" },
                bouton("Créer quand même", "", () => lancerCreation(true)),
              ),
            ),
        zonePdf,
      );
    }

    // --- Assistant ---
    enregistrerPreRemplissage("contrats", async (texte, jour, ajouter) => {
      const proposition = await api.assistantContrat(texte, jour);
      remplir(proposition, ajouter);
      return proposition;
    });

    function remplir(p: PropositionContrat, ajouter: (element: HTMLElement) => void) {
      if (p.employe.statut === "trouve" && p.employe.valeur) employe.choisir(p.employe.valeur);
      else if (p.nouvelEmploye) employe.proposerNouveau({ ...p.nouvelEmploye, entree: p.entree, poste: p.poste });
      else if (p.employe.statut === "introuvable") employe.saisir(p.employe.texte);
      else employe.choisir(null);
      const candidats = choixCandidats(
        "Employé",
        p.employe,
        (e) => e.nom || e.id,
        (e) => employe.choisir(e),
      );
      if (candidats) ajouter(candidats);

      typeContrat.value = p.typeContrat ?? "";
      if (p.poste) poste.value = p.poste;
      if (p.entree) entreeContrat.value = p.entree;
      finContrat.value = p.fin ?? "";
      heures.value = p.heures === null ? "" : String(p.heures);
      jours.forEach((j, i) => (j.value = p.repartition && p.repartition[i] ? String(p.repartition[i]) : ""));
      majTotal();
      for (const [nom, input] of champsLibres) input.value = p.champs[nom] ?? "";
      zoneContrat.replaceChildren();
    }

    contenu.replaceChildren(
      listePostes,
      employe.carte,
      h(
        "div",
        { class: "carte" },
        h("h2", {}, h("span", { class: "etape" }, "2"), "Contrat"),
        h(
          "div",
          { class: "grille" },
          champ("Type de contrat", typeContrat),
          champ("Poste", poste),
          champ("Date d'entrée", entreeContrat),
          champ("Date de fin (CDD)", finContrat),
          champ("Heures par semaine", heures),
          champ("Format d'impression", format),
        ),
        h("p", { class: "note" }, "Répartition des heures (obligatoire à temps partiel) ", total),
        h("div", { class: "repartition" }, ...jours.map((j, i) => champ(JOURS[i] ?? "", j))),
        ...(listes.champsLibres.length > 0
          ? [
              h("p", { class: "note" }, "Champs complémentaires (définis dans Dokos)"),
              h(
                "div",
                { class: "grille" },
                ...listes.champsLibres.map((c) =>
                  champ(c.libelle + (c.obligatoire ? " *" : ""), champsLibres.get(c.champ)!),
                ),
              ),
            ]
          : [
              alerte(
                "info",
                "Aucun champ complémentaire dans Dokos",
                h(
                  "p",
                  {},
                  "Rémunération, période d'essai, motif du CDD, lieu de travail : créez ces champs sur « Employment Contract ».",
                ),
              ),
            ]),
        h("div", { class: "actions" }, creerContrat, verifierContrat),
        h("p", { class: "note" }, "Le contrat est vérifié avant création : s'il reste une erreur, rien n'est créé."),
        zoneContrat,
      ),
      creerSectionFormat(listes),
    );
  }
}
