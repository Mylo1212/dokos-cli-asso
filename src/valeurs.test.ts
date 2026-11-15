import { describe, expect, it } from "vitest";

import type { ListeVue } from "./api";

import {
  actionPourErreur,
  dateLisible,
  demandeEvenement,
  demandeListe,
  demandeVenteModele,
  dossierDe,
  demandeVente,
  erreurVue,
  filtrerOptions,
  formaterDate,
  livraisonPassee,
  montant,
  nombreDepuisTexte,
  prixDeLigne,
  texteOuNull,
  titreErreur,
  type SaisieEvenement,
  type SaisieListe,
  type SaisieVenteModele,
  type SaisieVente,
} from "./valeurs";

describe("saisies", () => {
  it("lit les nombres à la française", () => {
    expect(nombreDepuisTexte("2,5")).toBe(2.5);
    expect(nombreDepuisTexte(" 1 250 ")).toBe(1250);
    expect(nombreDepuisTexte("")).toBeNull();
    expect(nombreDepuisTexte("dix")).toBeNaN();
  });

  it("nettoie les textes et les dates", () => {
    expect(texteOuNull("  ")).toBeNull();
    expect(texteOuNull(" x ")).toBe("x");
    expect(formaterDate(new Date(2026, 10, 3))).toBe("2026-11-03");
    expect(dateLisible("2026-11-03")).toBe("03/11/2026");
    expect(dateLisible("2026-11-03 14:00:00")).toBe("03/11/2026");
    expect(dateLisible("bientôt")).toBe("bientôt");
    expect(montant(12.5, "EUR")).toMatch(/12,50/);
    expect(montant(1, "???")).toBe("1.00 ???");
  });
});

describe("erreurs", () => {
  it("reconnaît une erreur structurée", () => {
    expect(erreurVue({ genre: "reseau", message: "délai" })).toEqual({ genre: "reseau", message: "délai" });
    expect(erreurVue("texte")).toEqual({ genre: "autre", message: "texte" });
    expect(erreurVue(new Error("boum")).message).toBe("boum");
  });

  it("propose l'action adaptée", () => {
    expect(actionPourErreur("reseau")).toBe("reessayer");
    expect(actionPourErreur("authentification")).toBe("reglages");
    expect(actionPourErreur("validation")).toBe("aucune");
    expect(titreErreur("permission")).toBe("Droit manquant dans Dokos");
    expect(titreErreur("inconnu")).toBe("Erreur");
  });
});

describe("demande de vente", () => {
  const base: SaisieVente = {
    typeVente: "facture",
    client: "CUST-2",
    date: "2026-11-03",
    echeance: "",
    reference: " LOT-1 ",
    lignes: [{ article: "REPAS", quantite: "25", livraison: "", prix: "" }],
  };

  it("construit une facture", () => {
    expect(demandeVente(base)).toEqual({
      typeVente: "facture",
      client: "CUST-2",
      date: "2026-11-03",
      echeance: null,
      lignes: [{ article: "REPAS", quantite: 25, livraison: null, prix: null }],
      reference: "LOT-1",
    });
  });

  it("refuse les saisies incomplètes", () => {
    expect(demandeVente({ ...base, client: null })).toBe("Choisissez un client dans la liste.");
    expect(demandeVente({ ...base, date: "" })).toBe("Renseignez la date de facture.");
    expect(demandeVente({ ...base, lignes: [] })).toBe("Ajoutez au moins une ligne.");
    expect(demandeVente({ ...base, lignes: [{ article: null, quantite: "1", livraison: "", prix: "" }] })).toMatch(
      /Ligne 1/,
    );
    expect(demandeVente({ ...base, lignes: [{ article: "A", quantite: "0", livraison: "", prix: "" }] })).toMatch(
      /quantité/,
    );
  });

  it("adapte les champs au type", () => {
    const commande = demandeVente({
      ...base,
      typeVente: "commande",
      echeance: "2026-12-01",
      lignes: [{ article: "A", quantite: "1", livraison: "2026-11-05", prix: "" }],
    });
    expect(commande).toMatchObject({ echeance: null, lignes: [{ livraison: "2026-11-05" }], reference: "LOT-1" });
    const devis = demandeVente({ ...base, typeVente: "devis", echeance: "2026-12-01" });
    expect(devis).toMatchObject({ echeance: "2026-12-01", reference: null, lignes: [{ livraison: null }] });
  });

  it("envoie le prix saisi, ou rien pour garder celui de Dokos", () => {
    const lignes = [
      { article: "REPAS", quantite: "25", livraison: "", prix: " 12,50 " },
      { article: "SALLE", quantite: "1", livraison: "", prix: "" },
      { article: "OFFERT", quantite: "1", livraison: "", prix: "0" },
    ];
    expect(demandeVente({ ...base, lignes })).toMatchObject({
      lignes: [{ prix: 12.5 }, { prix: null }, { prix: 0 }],
    });
  });

  it("refuse un prix illisible, négatif ou démesuré", () => {
    for (const prix of ["cher", "-1", "99999999"]) {
      const ligne = { article: "REPAS", quantite: "1", livraison: "", prix };
      expect(demandeVente({ ...base, lignes: [ligne] }), prix).toBe("Ligne 1 : prix invalide.");
    }
  });

  it("retient le prix saisi, sinon l'indicatif", () => {
    const indicatif = { prix: 18.5, devise: "EUR" };
    expect(prixDeLigne("", indicatif)).toEqual(indicatif);
    expect(prixDeLigne("12,5", indicatif)).toEqual({ prix: 12.5, devise: "EUR" });
    expect(prixDeLigne("12,5", null)).toEqual({ prix: 12.5, devise: "" });
    expect(prixDeLigne("abc", indicatif)).toBeNull();
    expect(prixDeLigne("", null)).toBeNull();
  });

  it("signale une livraison passée", () => {
    expect(livraisonPassee("commande", "2026-11-01", "2026-11-03")).toBe(true);
    expect(livraisonPassee("commande", "2026-11-03", "2026-11-03")).toBe(false);
    expect(livraisonPassee("facture", "2026-11-01", "2026-11-03")).toBe(false);
  });
});

describe("demande d'événement", () => {
  const base: SaisieEvenement = {
    titre: " Atelier ",
    debut: "2026-12-14T14:00",
    fin: "",
    journee: false,
    lieu: "",
    description: "",
    places: "12",
    annulation: true,
    questions: [{ champ: "age", obligatoire: true }],
  };

  it("construit l'événement", () => {
    expect(demandeEvenement(base)).toMatchObject({ titre: "Atelier", fin: null, lieu: null, places: 12 });
    expect(demandeEvenement({ ...base, places: "" })).toMatchObject({ places: null });
  });

  it("refuse titre, début ou places invalides", () => {
    expect(demandeEvenement({ ...base, titre: " " })).toMatch(/titre/);
    expect(demandeEvenement({ ...base, debut: "" })).toMatch(/début/);
    expect(demandeEvenement({ ...base, places: "1,5" })).toMatch(/places/);
    expect(demandeEvenement({ ...base, places: "0" })).toMatch(/places/);
  });
});

describe("export", () => {
  const factures: ListeVue = {
    nom: "factures",
    libelle: "Factures",
    client: true,
    groupe: false,
    territoire: false,
    dates: "date de facture",
    statuts: [{ cle: "impayee", libelle: "impayées" }],
    vendables: false,
    inactifs: "annulés",
    colonnes: ["Numéro"],
  };
  const saisie: SaisieListe = {
    client: "CUST-1",
    groupe: "Collectivités",
    territoire: "",
    du: "2026-10-01",
    au: "2026-10-31",
    statut: "impayee",
    texte: "  ",
    inclureInactifs: false,
    vendablesSeulement: true,
  };

  it("n'envoie que les filtres de la liste", () => {
    expect(demandeListe(factures, saisie)).toEqual({
      liste: "factures",
      client: "CUST-1",
      groupe: null,
      territoire: null,
      du: "2026-10-01",
      au: "2026-10-31",
      statut: "impayee",
      texte: null,
      inclureInactifs: false,
      vendablesSeulement: false,
    });
    const clients: ListeVue = { ...factures, nom: "clients", client: false, groupe: true, dates: null, statuts: [] };
    const d = demandeListe(clients, saisie);
    expect(typeof d === "object" && d.client === null && d.groupe === "Collectivités" && d.du === null).toBe(true);
    expect(typeof d === "object" && d.statut).toBe(null);
  });

  it("refuse une période inversée", () => {
    expect(typeof demandeListe(factures, { ...saisie, du: "2026-11-01" })).toBe("string");
  });

  it("dossier d'un fichier", () => {
    expect(dossierDe("C:\\Users\\Ana\\Documents\\Dokos\\listes\\clients.xlsx")).toBe(
      "C:\\Users\\Ana\\Documents\\Dokos\\listes",
    );
    expect(dossierDe("/home/ana/Documents/Dokos/listes/clients.xlsx")).toBe("/home/ana/Documents/Dokos/listes");
    expect(dossierDe("clients.xlsx")).toBe("clients.xlsx");
  });
});

describe("ventes à partir d'un modèle", () => {
  const saisie: SaisieVenteModele = {
    typeVente: "commande",
    modele: { id: "SAL-ORD-0003", modifie: "v1" },
    client: "CUST-2",
    arrivee: "2026-12-01",
    depart: "2026-12-04",
    quantites: [
      { index: 0, texte: "28" },
      { index: 1, texte: "" },
      { index: 2, texte: "0" },
      { index: 3, texte: "1,5" },
    ],
  };

  it("retire les quantités vides ou nulles", () => {
    expect(demandeVenteModele(saisie)).toEqual({
      typeVente: "commande",
      modele: "SAL-ORD-0003",
      modifie: "v1",
      client: "CUST-2",
      arrivee: "2026-12-01",
      depart: "2026-12-04",
      quantites: [
        { index: 0, quantite: 28 },
        { index: 1, quantite: null },
        { index: 2, quantite: null },
        { index: 3, quantite: 1.5 },
      ],
    });
  });

  it("signale ce qui manque", () => {
    expect(demandeVenteModele({ ...saisie, modele: null })).toBe("Choisissez un modèle.");
    expect(demandeVenteModele({ ...saisie, client: null })).toContain("client");
    expect(demandeVenteModele({ ...saisie, depart: "" })).toContain("dates");
    expect(demandeVenteModele({ ...saisie, depart: "2026-11-30" })).toContain("avant");
    expect(demandeVenteModele({ ...saisie, quantites: [{ index: 0, texte: "-2" }] })).toContain("invalide");
    expect(demandeVenteModele({ ...saisie, quantites: [{ index: 0, texte: "dix" }] })).toContain("invalide");
    expect(demandeVenteModele({ ...saisie, quantites: [{ index: 0, texte: "0" }] })).toContain("au moins");
  });
});

describe("liste déroulante", () => {
  const clients = [
    { id: "CUST-1", nom: "Mairie de Crest" },
    { id: "CUST-2", nom: "CCAS de Crest" },
    { id: "CUST-3", nom: "Crèche des Amanins" },
    { id: "CUST-4", nom: "École" },
  ];
  const texte = (c: (typeof clients)[number]) => `${c.nom} ${c.id}`;

  it("montre tout sans saisie", () => {
    expect(filtrerOptions(clients, "", texte)).toEqual({ affiches: clients, restants: 0 });
    expect(filtrerOptions(clients, "   ", texte).affiches).toHaveLength(4);
  });

  it("filtre sans tenir compte des accents ni de la casse, début de texte d'abord", () => {
    const r = filtrerOptions(clients, "crest", texte);
    expect(r.affiches.map((c) => c.id)).toEqual(["CUST-1", "CUST-2"]);
    expect(filtrerOptions(clients, "ecole", texte).affiches.map((c) => c.id)).toEqual(["CUST-4"]);
    expect(filtrerOptions(clients, "cr", texte).affiches.map((c) => c.id)).toEqual(["CUST-3", "CUST-1", "CUST-2"]);
    expect(filtrerOptions(clients, "cust-3", texte).affiches).toHaveLength(1);
    expect(filtrerOptions(clients, "zzz", texte)).toEqual({ affiches: [], restants: 0 });
  });

  it("limite l'affichage et dit combien restent cachés", () => {
    const beaucoup = Array.from({ length: 450 }, (_, i) => `Client ${i}`);
    const r = filtrerOptions(beaucoup, "client", (c) => c);
    expect(r.affiches).toHaveLength(200);
    expect(r.restants).toBe(250);
    expect(filtrerOptions(beaucoup, "client 44", (c) => c)).toMatchObject({ restants: 0 });
  });
});
