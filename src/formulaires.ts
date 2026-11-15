// Onglet Formulaires : événement public et formulaire d'inscription.
// Création non publiée, puis publication séparée.

import { api, type ChampVue, type PropositionEvenement } from "./api";
import { enregistrerPreRemplissage } from "./assistant";
import { alerte, alerteErreur, bouton, caseACocher, champ, h, lienDokos, occupe, saisie } from "./dom";
import { perimerSuivi } from "./donnees";
import { demandeEvenement } from "./valeurs";

const CHAMPS_DE_BASE = ["Prénom", "Nom", "E-mail", "Téléphone (facultatif)"];

export function initialiserFormulaires(panneau: HTMLElement): void {
  const titre = saisie({ placeholder: "Atelier poterie" });
  const debut = saisie({ type: "datetime-local" });
  const fin = saisie({ type: "datetime-local" });
  const lieu = saisie({ placeholder: "Grande salle" });
  const places = saisie({ type: "number", min: "1", step: "1", placeholder: "Sans limite" });
  const description = h("textarea", { rows: "3", placeholder: "Présentation affichée sur la page de l'événement" });
  const journee = caseACocher("Journée entière");
  const annulation = caseACocher("Les inscrits peuvent annuler");

  const questions = new Map<string, { coche: HTMLInputElement; obligatoire: HTMLInputElement; champ: ChampVue }>();
  const listeQuestions = h("div", { class: "questions" });
  const etatQuestions = h("div");

  async function chargerQuestions() {
    etatQuestions.replaceChildren(h("p", { class: "note" }, "Chargement des champs depuis Dokos…"));
    try {
      const champs = await api.questionsFormulaire();
      questions.clear();
      listeQuestions.replaceChildren();
      for (const c of champs) {
        const coche = caseACocher("");
        coche.label.append(h("span", {}, c.libelle), h("span", { class: "type" }, c.typeChamp));
        const obligatoire = caseACocher("Obligatoire");
        obligatoire.input.disabled = true;
        coche.input.addEventListener("change", () => {
          obligatoire.input.disabled = !coche.input.checked;
          if (!coche.input.checked) obligatoire.input.checked = false;
        });
        questions.set(c.champ, { coche: coche.input, obligatoire: obligatoire.input, champ: c });
        listeQuestions.append(h("div", { class: "question" }, coche.label, obligatoire.label));
      }
      listeQuestions.hidden = champs.length === 0;
      etatQuestions.replaceChildren(
        champs.length === 0
          ? alerte(
              "info",
              "Aucune question supplémentaire disponible",
              h(
                "p",
                {},
                "Ajoutez des champs à « Event Registration » dans Dokos (Personnaliser le formulaire), puis rechargez.",
              ),
            )
          : h("p", { class: "note" }, "Seuls les champs qui existent dans Dokos peuvent être demandés."),
      );
    } catch (e) {
      listeQuestions.hidden = true;
      etatQuestions.replaceChildren(alerteErreur(e, () => void chargerQuestions()));
    }
  }

  const resultat = h("div", { "aria-live": "polite" });

  enregistrerPreRemplissage("formulaires", async (texte, jour) => {
    const proposition = await api.assistantEvenement(texte, jour);
    if (questions.size === 0) await chargerQuestions();
    remplir(proposition);
    return proposition;
  });

  /** Pré-remplit l'événement et coche les questions proposées par l'IA. */
  function remplir(p: PropositionEvenement) {
    if (p.titre) titre.value = p.titre;
    if (p.debut) debut.value = p.debut;
    fin.value = p.fin ?? "";
    if (p.lieu) lieu.value = p.lieu;
    places.value = p.places === null ? "" : String(p.places);
    if (p.description) description.value = p.description;
    journee.input.checked = p.journee;
    annulation.input.checked = p.annulation;
    for (const q of questions.values()) {
      const proposee = p.questionsFormulaire.find((x) => x.champ === q.champ.champ);
      q.coche.checked = proposee !== undefined;
      q.obligatoire.disabled = !q.coche.checked;
      q.obligatoire.checked = proposee?.obligatoire ?? false;
    }
    resultat.replaceChildren();
  }
  const verifier = bouton("Vérifier", "", () => lancerVerification());
  verifier.dataset.raccourci = "verifier";
  const creer = bouton("Créer (non publié)", "principal", () => lancerCreation(false));

  function demande() {
    return demandeEvenement({
      titre: titre.value,
      debut: debut.value,
      fin: fin.value,
      journee: journee.input.checked,
      lieu: lieu.value,
      description: description.value,
      places: places.value,
      annulation: annulation.input.checked,
      questions: [...questions.values()]
        .filter((q) => q.coche.checked)
        .map((q) => ({ champ: q.champ.champ, obligatoire: q.obligatoire.checked })),
    });
  }

  async function lancerVerification() {
    const d = demande();
    if (typeof d === "string") return resultat.replaceChildren(alerte("attention", "À compléter", d));
    const apercu = await occupe([verifier, creer], resultat, () => api.verifierEvenement(d));
    if (!apercu) return;
    const formulaire = alerte(
      "info",
      `Formulaire « ${apercu.formulaireTitre} »`,
      h("p", {}, `Adresse : /${apercu.formulaireRoute} — réutilisé s'il existe déjà avec les mêmes champs.`),
      h(
        "div",
        { class: "puces" },
        ...apercu.champs.map((c) => h("span", { class: "puce" }, c.libelle + (c.obligatoire ? " *" : ""))),
      ),
    );
    const existant = apercu.existant
      ? alerte(
          "attention",
          "Événement identique déjà dans Dokos",
          h("p", {}, `${apercu.existant.id} (${apercu.existant.statut}) : la création ne fera rien.`),
          h("div", { class: "actions" }, lienDokos(apercu.existant.lien)),
        )
      : alerte("succes", "Prêt à créer", h("p", {}, "L'événement et son formulaire seront créés non publiés."));
    resultat.replaceChildren(existant, formulaire);
  }

  async function lancerCreation(forcer: boolean) {
    const d = demande();
    if (typeof d === "string") return resultat.replaceChildren(alerte("attention", "À compléter", d));
    const r = await occupe([creer, verifier], resultat, () => api.creerEvenement(d, forcer));
    if (!r) return;
    if (r.cree) perimerSuivi();
    const ev = r.evenement;
    if (!r.cree) {
      resultat.replaceChildren(
        alerte(
          "attention",
          "Événement identique déjà dans Dokos : rien n'a été créé",
          h("p", {}, `${ev.id} (${ev.statut})`),
          h(
            "div",
            { class: "actions" },
            lienDokos(ev.lien),
            bouton("Créer quand même", "", () => lancerCreation(true)),
          ),
        ),
      );
      return;
    }
    const zonePublication = h("div");
    const publier = bouton("Publier sur le site", "principal", async () => {
      const p = await occupe([publier], zonePublication, () => api.publierEvenement(ev.id));
      if (!p) return;
      zonePublication.replaceChildren(
        alerte(
          "succes",
          "Événement publié",
          h("p", {}, "Page : ", h("span", { class: "chemin" }, p.page)),
          h("p", {}, "Inscription : ", h("span", { class: "chemin" }, p.inscription)),
          h(
            "div",
            { class: "actions" },
            lienDokos(p.page, "Voir la page"),
            lienDokos(p.inscription, "Voir le formulaire"),
          ),
        ),
      );
      publier.remove();
    });
    resultat.replaceChildren(
      alerte(
        "succes",
        `Événement ${ev.id} créé, non publié`,
        h(
          "p",
          {},
          `Formulaire ${r.formulaireReutilise ? "réutilisé" : "créé"} : ${r.formulaire}. Relisez l'événement dans Dokos avant de publier.`,
        ),
        h("div", { class: "actions" }, lienDokos(ev.lien), publier),
      ),
      zonePublication,
    );
  }

  panneau.append(
    h("h1", {}, "Formulaires"),
    h(
      "p",
      { class: "sous-titre" },
      "Un événement public et son formulaire d'inscription, publiés seulement quand vous le décidez.",
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, h("span", { class: "etape" }, "1"), "Événement"),
      h(
        "div",
        { class: "grille" },
        champ("Titre", titre, "large"),
        champ("Début", debut),
        champ("Fin", fin),
        champ("Lieu", lieu),
        champ("Places", places),
        champ("Description", description, "large"),
      ),
      h("div", { class: "actions" }, journee.label, annulation.label),
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, h("span", { class: "etape" }, "2"), "Questions du formulaire"),
      h("p", { class: "note sans-marge" }, "Toujours demandés :"),
      h("div", { class: "puces espace" }, ...CHAMPS_DE_BASE.map((c) => h("span", { class: "puce" }, c))),
      listeQuestions,
      etatQuestions,
      h("div", { class: "actions" }, bouton("Recharger les champs", "discret", chargerQuestions)),
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, h("span", { class: "etape" }, "3"), "Créer"),
      h("div", { class: "actions sans-marge" }, creer, verifier),
      resultat,
    ),
  );

  let chargees = false;
  panneau.addEventListener("onglet-affiche", () => {
    if (!chargees) {
      chargees = true;
      void chargerQuestions();
    }
  });
}
