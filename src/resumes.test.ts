import { describe, expect, it } from "vitest";

import { correspond, heureCourte, pluriel, resumeVente, sansAccents } from "./valeurs";

describe("recherche dans la palette", () => {
  it("ignore accents et casse", () => {
    expect(sansAccents("Événement À Créer")).toBe("evenement a creer");
    expect(correspond("Créer un événement avec inscription", "evenement")).toBe(true);
    expect(correspond("Factures en retard", "FACT ret")).toBe(true);
  });

  it("demande tous les mots, dans n'importe quel ordre", () => {
    expect(correspond("Nouveau devis", "devis nouveau")).toBe(true);
    expect(correspond("Nouveau devis", "devis facture")).toBe(false);
    expect(correspond("Nouveau devis", "   ")).toBe(true);
  });
});

describe("résumé d'une vente", () => {
  const prix = (p: number, devise = "EUR") => ({ prix: p, devise });

  it("compte les lignes choisies et additionne les prix connus", () => {
    const r = resumeVente([
      { article: "REPAS", quantite: "25", prix: prix(18.5) },
      { article: "SALLE", quantite: "1,5", prix: prix(300) },
      { article: null, quantite: "1", prix: null },
    ]);
    expect(r).toEqual({ lignes: 2, total: 912.5, devise: "EUR", horsTotal: 0 });
  });

  it("signale les lignes sans prix ou sans quantité lisible", () => {
    const r = resumeVente([
      { article: "REPAS", quantite: "10", prix: prix(10) },
      { article: "NUIT", quantite: "2", prix: null },
      { article: "SALLE", quantite: "abc", prix: prix(300) },
    ]);
    expect(r).toEqual({ lignes: 3, total: 100, devise: "EUR", horsTotal: 2 });
  });

  it("accepte un prix saisi sans devise connue", () => {
    const r = resumeVente([
      { article: "A", quantite: "2", prix: prix(10, "EUR") },
      { article: "B", quantite: "1", prix: prix(5, "") },
    ]);
    expect(r).toEqual({ lignes: 2, total: 25, devise: "EUR", horsTotal: 0 });
    expect(resumeVente([{ article: "B", quantite: "1", prix: prix(5, "") }])).toMatchObject({ total: 5, devise: "" });
  });

  it("n'annonce aucun total sans prix connu, et ne mélange pas les devises", () => {
    expect(resumeVente([{ article: "NUIT", quantite: "2", prix: null }])).toEqual({
      lignes: 1,
      total: null,
      devise: "",
      horsTotal: 1,
    });
    const r = resumeVente([
      { article: "A", quantite: "1", prix: prix(10, "EUR") },
      { article: "B", quantite: "1", prix: prix(10, "CHF") },
    ]);
    expect(r).toEqual({ lignes: 2, total: 10, devise: "EUR", horsTotal: 1 });
  });
});

describe("petits textes", () => {
  it("accorde le pluriel", () => {
    expect(pluriel(0, "ligne")).toBe("0 ligne");
    expect(pluriel(1, "ligne")).toBe("1 ligne");
    expect(pluriel(3, "ligne")).toBe("3 lignes");
    expect(pluriel(2, "fin de contrat", "fins de contrat")).toBe("2 fins de contrat");
  });

  it("donne l'heure sur deux chiffres", () => {
    expect(heureCourte(new Date(2026, 10, 11, 9, 5).getTime())).toBe("09:05");
  });
});
