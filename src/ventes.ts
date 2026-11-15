// Onglet Ventes : devis, commandes et factures, toujours en brouillon.

import {
  api,
  type ApercuVente,
  type ArticleVue,
  type ClientVue,
  type DocumentVue,
  type PropositionVente,
  type ResultatVente,
  type TypeVente,
} from "./api";
import { choixCandidats, enregistrerPreRemplissage } from "./assistant";
import { creerCatalogue } from "./catalogue";
import { alerte, aujourdhui, bouton, champ, h, lienDokos, montant, occupe, recherche, saisie } from "./dom";
import { perimerSuivi, prixIndicatif, recherches } from "./donnees";
import { ouvrirOnglet } from "./etat";
import { LIBELLES_VENTE, demandeVente, livraisonPassee, pluriel, resumeVente, type LigneSaisie } from "./valeurs";
import { creerSectionModele } from "./ventes-modele";

interface Ligne {
  element: HTMLDivElement;
  article: ArticleVue | null;
  quantite: HTMLInputElement;
  livraison: HTMLInputElement;
  prix: HTMLSpanElement;
  /** Prix indicatif de l'article choisi, s'il est connu. */
  prixConnu: { prix: number; devise: string } | null;
  choix: { choisir: (a: ArticleVue | null) => void; saisir: (texte: string, chercher?: boolean) => void };
  aide: HTMLDivElement;
}

/** Saisie libre des lignes, ou copie d'un document « Template » de Dokos. */
type Source = "libre" | "modele";

export function initialiserVentes(panneau: HTMLElement): void {
  let typeVente: TypeVente = "devis";
  let source: Source = "libre";
  let client: ClientVue | null = null;
  const lignes: Ligne[] = [];
  const sectionModele = creerSectionModele();

  const segmentsSource = h("div", { class: "segments", role: "group", "aria-label": "Point de départ" });
  const boutonsSource = new Map<Source, HTMLButtonElement>();
  const choisirSource = (cle: Source) => {
    source = cle;
    for (const [autre, b] of boutonsSource) b.setAttribute("aria-pressed", String(autre === cle));
    if (cle === "modele") sectionModele.charger(typeVente);
    adapter();
  };
  for (const [cle, libelle] of [
    ["libre", "Saisie libre"],
    ["modele", "À partir d'un modèle"],
  ] as [Source, string][]) {
    const b = h("button", { type: "button", "aria-pressed": String(cle === source) }, libelle);
    b.addEventListener("click", () => choisirSource(cle));
    boutonsSource.set(cle, b);
    segmentsSource.append(b);
  }

  const segments = h("div", { class: "segments", role: "group", "aria-label": "Type de document" });
  const boutonsType = new Map<TypeVente, HTMLButtonElement>();
  const choisirType = (cle: TypeVente) => {
    typeVente = cle;
    for (const [autre, b] of boutonsType) b.setAttribute("aria-pressed", String(autre === cle));
    if (source === "modele") sectionModele.charger(cle);
    adapter();
  };
  for (const [cle, type] of Object.entries(LIBELLES_VENTE) as [TypeVente, (typeof LIBELLES_VENTE)[TypeVente]][]) {
    const b = h("button", { type: "button", "aria-pressed": String(cle === typeVente) }, type.nom);
    b.addEventListener("click", () => choisirType(cle));
    boutonsType.set(cle, b);
    segments.append(b);
  }

  const catalogue = creerCatalogue({
    surAjout: (article) => {
      const existante = lignes.find((l) => l.article?.code === article.code);
      if (existante) {
        const q = Number(existante.quantite.value.replace(",", ".")) || 0;
        existante.quantite.value = String(q + 1);
        majResume();
        return;
      }
      // Une ligne encore vide est reprise plutôt que laissée derrière.
      const ligne = lignes.find((l) => l.article === null) ?? ajouterLigne();
      ligne.choix.choisir(article);
    },
  });

  const choixClient = recherche<ClientVue>({
    placeholder: "Nom ou identifiant du client",
    chercher: recherches.clients,
    libelle: (c) => c.nom || c.id,
    identifiant: (c) => c.id,
    surChoix: (c) => {
      client = c;
      majResume();
    },
  });
  const date = saisie({ type: "date", value: aujourdhui() });
  const echeance = saisie({ type: "date" });
  const reference = saisie({ placeholder: "N° de bon de commande, lot…" });
  const libelleDate = champ(LIBELLES_VENTE.devis.date, date);
  const libelleEcheance = champ(LIBELLES_VENTE.devis.echeance ?? "", echeance);
  const libelleReference = champ("Référence (facultatif)", reference);
  const avertissementDate = h("div");
  date.addEventListener("input", verifierDate);

  function verifierDate() {
    avertissementDate.replaceChildren(
      ...(livraisonPassee(typeVente, date.value, aujourdhui())
        ? [
            alerte(
              "attention",
              "Livraison antérieure à aujourd'hui",
              h(
                "p",
                {},
                "Dokos refuse une commande livrée avant sa date de création. Pour une régularisation, saisissez-la directement dans Dokos.",
              ),
            ),
          ]
        : []),
    );
  }

  const listeLignes = h("div", { class: "lignes" });
  const ajouterLigne = () => {
    const quantite = saisie({ type: "number", min: "0", step: "any", value: "1", inputmode: "decimal" });
    const livraison = saisie({ type: "date" });
    const aide = h("div", { class: "ligne-aide" });
    const prix = h("span", { class: "prix-indicatif" });
    const ligne: Ligne = {
      element: h("div", { class: "ligne" }),
      article: null,
      quantite,
      livraison,
      prix,
      prixConnu: null,
      choix: { choisir: () => {}, saisir: () => {} },
      aide,
    };
    const montrerPrix = () => {
      const p = ligne.prixConnu;
      prix.textContent = p ? `${montant(p.prix, p.devise)} indicatif` : "";
      majResume();
    };
    const choixArticle = recherche<ArticleVue>({
      placeholder: "Article",
      chercher: (texte) => recherches.articles(texte, catalogue.groupeChoisi()),
      libelle: (a) => a.libelle || a.code,
      identifiant: (a) => a.code,
      surChoix: (a) => {
        ligne.article = a;
        ligne.prixConnu = (a && catalogue.prixDe(a.code)) || null;
        montrerPrix();
        // Prix pas encore connu (article trouvé par la recherche) : demandé à
        // Dokos, sans bloquer la saisie ; rien ne s'affiche s'il n'y en a pas.
        if (a && !ligne.prixConnu) {
          void prixIndicatif(a.code).then((p) => {
            if (!p || ligne.article?.code !== a.code) return;
            ligne.prixConnu = p;
            montrerPrix();
          });
        }
      },
    });
    ligne.choix = choixArticle;
    const retirer = h(
      "button",
      { type: "button", class: "bouton-icone", title: "Retirer la ligne", "aria-label": "Retirer la ligne" },
      "×",
    );
    retirer.addEventListener("click", () => {
      lignes.splice(lignes.indexOf(ligne), 1);
      ligne.element.remove();
      majResume();
    });
    quantite.addEventListener("input", majResume);
    ligne.element.append(
      champ("Article", choixArticle.racine),
      champ("Quantité", quantite),
      champ("Livraison (si différente)", livraison),
      retirer,
      aide,
      prix,
    );
    lignes.push(ligne);
    listeLignes.append(ligne.element);
    adapter();
    return ligne;
  };

  const resultat = h("div", { class: "resultat-action", "aria-live": "polite" });
  // Résultat ou erreur : amené à l'écran, au-dessus de la barre d'action.
  new MutationObserver(() => {
    if (resultat.childElementCount > 0) resultat.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }).observe(resultat, { childList: true });

  enregistrerPreRemplissage("ventes", async (texte, jour, ajouter) => {
    const proposition = await api.assistantVente(texte, jour);
    remplir(proposition, ajouter);
    return proposition;
  });

  /** Pré-remplit le formulaire avec la proposition de l'IA ; les choix à faire vont dans le chatbot. */
  function remplir(p: PropositionVente, ajouter: (element: HTMLElement) => void) {
    if (source !== "libre") choisirSource("libre");
    if (p.typeVente) choisirType(p.typeVente);
    if (p.client.statut === "trouve" && p.client.valeur) choixClient.choisir(p.client.valeur);
    else if (p.client.statut === "introuvable") choixClient.saisir(p.client.texte);
    else choixClient.choisir(null);
    const candidatsClient = choixCandidats(
      "Client",
      p.client,
      (c) => c.nom || c.id,
      (c) => choixClient.choisir(c),
    );
    if (candidatsClient) ajouter(candidatsClient);
    if (p.date) date.value = p.date;
    echeance.value = p.echeance ?? "";
    verifierDate();

    if (p.lignes.length > 0) {
      for (const ligne of [...lignes]) ligne.element.remove();
      lignes.length = 0;
      for (const proposee of p.lignes) {
        const ligne = ajouterLigne();
        ligne.quantite.value = proposee.quantite === null ? "" : String(proposee.quantite);
        ligne.livraison.value = proposee.livraison ?? "";
        const choix = proposee.article;
        if (choix.statut === "trouve" && choix.valeur) ligne.choix.choisir(choix.valeur);
        else ligne.choix.saisir(proposee.texte, choix.statut === "introuvable");
        const candidats = choixCandidats(
          "Article",
          choix,
          (a) => a.libelle || a.code,
          (a) => ligne.choix.choisir(a),
        );
        if (candidats) ligne.aide.replaceChildren(candidats);
      }
    }
    resultat.replaceChildren();
  }
  const verifier = bouton("Vérifier", "", () => lancerVerification());
  verifier.dataset.raccourci = "verifier";
  verifier.title = "Chercher les doublons sans rien créer (Ctrl+Entrée)";
  const creer = bouton("Créer en brouillon", "principal", () => lancerCreation(false));

  const montrer = (...blocs: HTMLElement[]) => resultat.replaceChildren(...blocs);

  /** Barre d'action : ce qui va être créé, en une ligne, toujours visible. */
  const resume = h("div", { class: "barre-resume", "aria-live": "polite" });
  function majResume() {
    const type = LIBELLES_VENTE[typeVente];
    const morceaux: (Node | string)[] = [
      h("strong", {}, source === "modele" ? `${type.nom} à partir d'un modèle` : type.nom),
      client ? client.nom || client.id : "client à choisir",
    ];
    if (source === "libre") {
      const r = resumeVente(
        lignes.map((l) => ({ article: l.article?.code ?? null, quantite: l.quantite.value, prix: l.prixConnu })),
      );
      morceaux.push(r.lignes === 0 ? "aucune ligne" : pluriel(r.lignes, "ligne"));
      if (r.total !== null) {
        const hors = r.horsTotal > 0 ? `, hors ${pluriel(r.horsTotal, "ligne")} sans prix` : "";
        morceaux.push(h("span", { class: "total" }, `≈ ${montant(r.total, r.devise)} HT indicatif${hors}`));
      }
    }
    resume.replaceChildren(...morceaux.flatMap((m, i) => (i === 0 ? [m] : [" · ", m])));
  }

  function adapter() {
    const type = LIBELLES_VENTE[typeVente];
    const libre = source === "libre";
    libelleDate.firstChild!.textContent = type.date;
    libelleDate.hidden = !libre;
    libelleEcheance.hidden = !libre || type.echeance === null;
    libelleReference.hidden = !libre || typeVente === "devis";
    titreClient.textContent = libre ? "Client et dates" : "Client";
    catalogue.carte.hidden = !libre;
    carteLignes.hidden = !libre;
    sectionModele.carte.hidden = libre;
    noteCreation.textContent = libre
      ? "Aucun prix n'est saisi ici : Dokos applique ses listes de prix et ses taxes."
      : "";
    noteCreation.hidden = !libre;
    if (type.echeance) libelleEcheance.firstChild!.textContent = type.echeance;
    for (const ligne of lignes) {
      const commande = typeVente === "commande";
      ligne.element.classList.toggle("avec-livraison", commande);
      ligne.livraison.parentElement!.hidden = !commande;
    }
    creer.textContent = `Créer ${type.article} en brouillon`;
    verifierDate();
    resultat.replaceChildren();
    majResume();
  }

  function demande() {
    const saisies: LigneSaisie[] = lignes.map((l) => ({
      article: l.article?.code ?? null,
      quantite: l.quantite.value,
      livraison: l.livraison.value,
    }));
    return demandeVente({
      typeVente,
      client: client?.id ?? null,
      date: date.value,
      echeance: echeance.value,
      reference: reference.value,
      lignes: saisies,
    });
  }

  const ligneDocument = (d: DocumentVue) =>
    h(
      "li",
      {},
      h("span", {}, `${d.id} — ${d.statut}${d.date ? `, du ${d.date}` : ""}, ${montant(d.total, d.devise)}`),
      lienDokos(d.lien),
    );

  /** Appels de vérification et de création selon le point de départ, ou ce qui manque. */
  function appels():
    { verifier: () => Promise<ApercuVente>; creer: (forcer: boolean) => Promise<ResultatVente> } | string {
    if (source === "modele") {
      const d = sectionModele.demande(client?.id ?? null);
      if (typeof d === "string") return d;
      return { verifier: () => api.verifierVenteModele(d), creer: (forcer) => api.creerVenteModele(d, forcer) };
    }
    const d = demande();
    if (typeof d === "string") return d;
    return { verifier: () => api.verifierVente(d), creer: (forcer) => api.creerVente(d, forcer) };
  }

  async function lancerVerification() {
    const a = appels();
    if (typeof a === "string") return montrer(alerte("attention", "À compléter", a));
    const apercu = await occupe([verifier, creer], resultat, a.verifier);
    if (!apercu) return;
    const blocs: HTMLElement[] = [];
    if (apercu.identiques.length > 0) {
      blocs.push(
        alerte(
          "attention",
          `${LIBELLES_VENTE[typeVente].nom} identique déjà dans Dokos`,
          h("p", {}, "La création ne fera rien et renverra ce document, sauf si vous forcez."),
          h("ul", { class: "liste-resultats" }, ...apercu.identiques.map(ligneDocument)),
        ),
      );
    }
    if (apercu.similaires.length > 0) {
      blocs.push(
        alerte(
          "info",
          "Autres documents de ce client aux mêmes dates",
          h("ul", { class: "liste-resultats" }, ...apercu.similaires.map(ligneDocument)),
        ),
      );
    }
    if (blocs.length === 0)
      blocs.push(
        alerte(
          "succes",
          "Prêt à créer",
          h(
            "p",
            {},
            source === "modele"
              ? "Aucun doublon. Prix, taxes et conditions seront ceux du modèle."
              : "Aucun doublon. Prix et taxes seront calculés par Dokos.",
          ),
        ),
      );
    montrer(...blocs);
  }

  async function lancerCreation(forcer: boolean) {
    const a = appels();
    if (typeof a === "string") return montrer(alerte("attention", "À compléter", a));
    const r = await occupe([creer, verifier], resultat, () => a.creer(forcer));
    if (!r) return;
    const doc = r.document;
    if (!r.cree) {
      montrer(
        alerte(
          "attention",
          `${LIBELLES_VENTE[typeVente].nom} identique déjà dans Dokos : rien n'a été créé`,
          h("ul", { class: "liste-resultats" }, ligneDocument(doc)),
          h(
            "div",
            { class: "actions" },
            bouton("Créer quand même", "", () => lancerCreation(true)),
          ),
        ),
      );
      return;
    }
    perimerSuivi();
    montrer(
      alerte(
        "succes",
        `${LIBELLES_VENTE[typeVente].nom} ${doc.id} créé(e) en brouillon`,
        h("p", {}, `Total calculé par Dokos : ${montant(doc.total, doc.devise)}. Relisez puis validez dans Dokos.`),
        h(
          "div",
          { class: "actions" },
          lienDokos(doc.lien),
          bouton("Nouveau document", "", nouveauDocument),
          bouton("Voir mes brouillons", "discret", () => ouvrirOnglet("suivi", "suivi-brouillons")),
        ),
      ),
      ...(r.autres.length > 0
        ? [
            alerte(
              "info",
              "Autres documents de ce client aux mêmes dates",
              h("ul", { class: "liste-resultats" }, ...r.autres.map(ligneDocument)),
            ),
          ]
        : []),
    );
  }

  /** Vide le formulaire pour le document suivant ; le type et le point de départ restent. */
  function nouveauDocument() {
    choixClient.choisir(null);
    for (const ligne of [...lignes]) ligne.element.remove();
    lignes.length = 0;
    ajouterLigne();
    date.value = aujourdhui();
    echeance.value = "";
    reference.value = "";
    adapter();
    window.scrollTo({ top: 0 });
    choixClient.focus();
  }

  const titreClient = h("h2", {}, "Client et dates");
  const noteCreation = h("p", { class: "note" });
  const barreAction = h("div", { class: "barre-action" }, resume, h("div", { class: "actions" }, verifier, creer));
  const carteLignes = h(
    "div",
    { class: "carte" },
    h("h2", {}, "Lignes"),
    listeLignes,
    h(
      "div",
      { class: "actions" },
      bouton("+ Ajouter une ligne", "discret", () => void ajouterLigne()),
    ),
  );

  // Fiches trouvées par le chatbot : « Utiliser dans Ventes ».
  document.addEventListener("dokos:choisir-client", (e) => {
    const c = (e as CustomEvent<ClientVue>).detail;
    choixClient.choisir(c);
  });
  document.addEventListener("dokos:ajouter-article", (e) => {
    const a = (e as CustomEvent<ArticleVue>).detail;
    if (source !== "libre") choisirSource("libre");
    const vide = lignes.find((l) => l.article === null);
    (vide ?? ajouterLigne()).choix.choisir(a);
  });
  // Accueil et palette : « Nouveau devis », « Nouvelle commande », « Nouvelle facture ».
  document.addEventListener("dokos:nouvelle-vente", (e) => {
    choisirType((e as CustomEvent<TypeVente>).detail);
    choixClient.focus();
  });

  panneau.append(
    h("h1", {}, "Ventes"),
    h("p", { class: "sous-titre" }, "Devis, commandes et factures créés en brouillon : vous les validez dans Dokos."),
    h("div", { class: "barre-segments" }, segments, segmentsSource),
    h(
      "div",
      { class: "carte" },
      titreClient,
      h(
        "div",
        { class: "grille" },
        champ("Client", choixClient.racine, "large"),
        libelleDate,
        libelleEcheance,
        libelleReference,
      ),
      avertissementDate,
    ),
    carteLignes,
    catalogue.carte,
    sectionModele.carte,
    noteCreation,
    resultat,
    barreAction,
  );
  ajouterLigne();
  adapter();
}
