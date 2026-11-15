// Chatbot à droite de chaque onglet : il qualifie la demande, indique
// l'onglet et la procédure, et propose de pré-remplir le formulaire. Les
// réponses de l'IA ne passent jamais par innerHTML : seulement textContent.

import { api, type ArticleVue, type ClientVue, type FicheVue, type MessageChat, type ReponseChat } from "./api";
import { blocRemarques, preRemplissage } from "./assistant";
import { alerteErreur, aujourdhui, bouton, h, lienDokos } from "./dom";
import {
  assistantActif,
  enregistrerPreference,
  ouvrirOnglet,
  preference,
  signaler,
  surChangementAssistant,
  type Onglet,
} from "./etat";

const PISTES = [
  "Faire un devis",
  "Coordonnées d'un client",
  "Exporter la liste des clients",
  "Embaucher quelqu'un en CDD",
  "Créer un événement avec inscription",
  "Voir les factures en retard",
];

const lireOuvert = () => preference("chat-ouvert", "oui") !== "non";
const ecrireOuvert = (ouvert: boolean) => enregistrerPreference("chat-ouvert", ouvert ? "oui" : "non");

/** Ce que le reste de l'appli peut demander au chatbot. */
export interface Chat {
  /** Affiche ou masque le panneau. */
  basculer: () => void;
  /** Ouvre le panneau et envoie ce texte à l'assistant. */
  demander: (texte: string) => void;
}

export function initialiserChat(options: {
  panneau: HTMLElement;
  disposition: HTMLElement;
  boutonEntete: HTMLButtonElement;
  ongletCourant: () => Onglet;
}): Chat {
  const { panneau, disposition, boutonEntete, ongletCourant } = options;
  const historique: MessageChat[] = [];

  const fil = h("div", { class: "fil-chat", role: "log", "aria-live": "polite" });
  const saisie = h("textarea", {
    rows: "2",
    placeholder: "Décrivez votre demande… (Entrée pour envoyer)",
    "aria-label": "Message pour l'assistant",
  });
  const envoyer = bouton("Envoyer", "principal", () => soumettre());
  const nouvelle = h(
    "button",
    { type: "button", class: "bouton-icone", title: "Nouvelle conversation", "aria-label": "Nouvelle conversation" },
    "↺",
  );
  const fermer = h(
    "button",
    { type: "button", class: "bouton-icone", title: "Masquer l'assistant", "aria-label": "Masquer l'assistant" },
    "×",
  );
  const zoneSaisie = h(
    "div",
    { class: "chat-saisie" },
    saisie,
    h(
      "div",
      { class: "chat-saisie-pied" },
      h("span", { class: "note sans-marge" }, "Envoyé à votre serveur Open WebUI. Rien n'est créé sans vous."),
      envoyer,
    ),
  );
  const desactive = h(
    "div",
    { class: "chat-desactive" },
    h("p", {}, "L'assistant IA est désactivé."),
    bouton("Ouvrir Réglages", "", () => ouvrirOnglet("reglages")),
  );

  panneau.append(
    h(
      "div",
      { class: "chat-entete" },
      h("h2", {}, h("span", { class: "etincelle", "aria-hidden": "true" }, "✦"), "Assistant"),
      nouvelle,
      fermer,
    ),
    fil,
    zoneSaisie,
    desactive,
  );

  // --- Ouverture du panneau ---
  function afficherPanneau(ouvert: boolean) {
    disposition.classList.toggle("chat-ferme", !ouvert);
    boutonEntete.setAttribute("aria-pressed", String(ouvert));
    ecrireOuvert(ouvert);
    if (ouvert && assistantActif()) saisie.focus();
  }
  boutonEntete.addEventListener("click", () => afficherPanneau(disposition.classList.contains("chat-ferme")));
  fermer.addEventListener("click", () => afficherPanneau(false));
  // Fenêtre étroite : l'assistant passerait par-dessus la section ; il reste
  // fermé au démarrage et s'ouvre à la demande (Ctrl+J, menu, palette).
  const ouvertAuDepart = lireOuvert() && !window.matchMedia("(max-width: 1120px)").matches;
  disposition.classList.toggle("chat-ferme", !ouvertAuDepart);
  boutonEntete.setAttribute("aria-pressed", String(ouvertAuDepart));

  const majActivation = () => {
    const actif = assistantActif();
    fil.hidden = zoneSaisie.hidden = nouvelle.hidden = !actif;
    desactive.hidden = actif;
  };
  surChangementAssistant(majActivation);
  majActivation();

  // --- Fil de discussion ---
  function bulle(auteur: "utilisateur" | "assistant", ...enfants: (Node | string | null)[]): HTMLDivElement {
    const b = h("div", { class: `bulle ${auteur}` }, ...enfants);
    fil.append(b);
    fil.scrollTop = fil.scrollHeight;
    return b;
  }

  function accueil() {
    fil.replaceChildren();
    const pistes = h("div", { class: "pistes" });
    for (const piste of PISTES) {
      pistes.append(bouton(piste, "puce-bouton", () => void soumettre(piste)));
    }
    bulle(
      "assistant",
      h(
        "p",
        { class: "texte" },
        "Bonjour ! Dites-moi ce que vous voulez faire : un devis, une facture, un contrat, une liste Excel… Je vous indique où aller et comment faire, et je peux pré-remplir le formulaire.",
      ),
      pistes,
    );
  }

  nouvelle.addEventListener("click", () => {
    historique.length = 0;
    accueil();
    saisie.focus();
  });

  async function soumettre(texteImpose?: string) {
    const texte = (texteImpose ?? saisie.value).trim();
    if (!texte || envoyer.disabled) return;
    if (texteImpose === undefined) saisie.value = "";
    historique.push({ auteur: "utilisateur", texte });
    bulle("utilisateur", texte);
    await demander();
  }

  /** Envoie la conversation ; en cas d'erreur, le dernier message reste à renvoyer. */
  async function demander() {
    const attente = bulle("assistant", h("span", { class: "attente", "aria-label": "L'assistant écrit" }));
    envoyer.disabled = true;
    try {
      const r = await api.discuter(historique, ongletCourant(), aujourdhui());
      attente.remove();
      historique.push({ auteur: "assistant", texte: r.texte, donnees: r.donnees });
      afficherReponse(r);
    } catch (e) {
      attente.remove();
      bulle(
        "assistant",
        alerteErreur(e),
        h(
          "div",
          { class: "actions" },
          bouton("Réessayer", "discret", (): void => {
            void demander();
          }),
        ),
      );
    } finally {
      envoyer.disabled = false;
    }
  }

  function afficherReponse(r: ReponseChat) {
    const actions = h("div", { class: "actions" });
    const onglet = r.onglet as Onglet | null;
    const libelle = r.libelleOnglet ?? "";
    if (onglet && r.preRemplir) {
      const demande = r.preRemplir;
      actions.append(bouton(`Pré-remplir dans ${libelle}`, "principal", () => void preRemplir(onglet, demande)));
    }
    if (onglet && onglet !== ongletCourant()) {
      actions.append(bouton(`Ouvrir ${libelle}`, "discret", () => ouvrirOnglet(onglet)));
    }
    bulle(
      "assistant",
      h("p", { class: "texte" }, r.texte),
      r.etapes.length > 0 ? h("ol", { class: "etapes-chat" }, ...r.etapes.map((e) => h("li", {}, e))) : null,
      r.fiches.length > 0 ? h("div", { class: "fiches" }, ...r.fiches.map(carteFiche)) : null,
      r.preRemplir ? h("p", { class: "citation" }, `Demande transmise : « ${r.preRemplir} »`) : null,
      ...r.avertissements.map((a) => h("p", { class: "reference" }, a)),
      actions.childElementCount > 0 ? actions : null,
    );
  }

  /** Fiche client ou article lue dans Dokos : tous ses champs, un lien, et son usage dans Ventes. */
  function carteFiche(f: FicheVue): HTMLElement {
    const valeur = (libelle: string) => f.champs.find((c) => c.libelle === libelle)?.valeur ?? "";
    const actions = h("div", { class: "actions" }, lienDokos(f.lien));
    if (f.genre === "client" && !f.desactive) {
      actions.append(
        bouton("Utiliser dans Ventes", "discret", () => {
          ouvrirOnglet("ventes");
          signaler<ClientVue>("dokos:choisir-client", { id: f.id, nom: f.titre });
        }),
      );
    }
    if (f.genre === "article" && !f.desactive && valeur("Vendable") !== "non") {
      actions.append(
        bouton("Ajouter à une vente", "discret", () => {
          ouvrirOnglet("ventes");
          signaler<ArticleVue>("dokos:ajouter-article", {
            code: f.id,
            libelle: f.titre,
            unite: valeur("Unité"),
            groupe: valeur("Groupe"),
          });
        }),
      );
    }
    return h(
      "div",
      { class: "fiche" },
      h(
        "div",
        { class: "fiche-titre" },
        h("strong", {}, f.titre),
        h("span", { class: "id" }, f.id),
        f.desactive ? h("span", { class: "puce" }, "désactivé") : null,
      ),
      f.champs.length > 0
        ? h("dl", {}, ...f.champs.flatMap((c) => [h("dt", {}, c.libelle), h("dd", {}, c.valeur)]))
        : null,
      actions,
    );
  }

  /** Ouvre l'onglet et pré-remplit son formulaire ; le résultat s'affiche dans le fil. */
  async function preRemplir(onglet: Onglet, demande: string) {
    ouvrirOnglet(onglet);
    const extras = h("div");
    const resultat = bulle("assistant", h("p", { class: "note sans-marge" }, "Pré-remplissage du formulaire…"));
    try {
      const p = await preRemplissage(onglet);
      const remarques = await p.lancer(demande, aujourdhui(), (element) => extras.append(element));
      resultat.replaceChildren(blocRemarques(remarques, p), extras);
    } catch (e) {
      resultat.replaceChildren(
        alerteErreur(e),
        h(
          "div",
          { class: "actions" },
          bouton("Réessayer", "discret", () => void preRemplir(onglet, demande)),
        ),
      );
    }
    fil.scrollTop = fil.scrollHeight;
  }

  saisie.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void soumettre();
    }
  });

  accueil();
  return {
    basculer: () => afficherPanneau(disposition.classList.contains("chat-ferme")),
    demander: (texte) => {
      afficherPanneau(true);
      if (assistantActif()) void soumettre(texte);
    },
  };
}
