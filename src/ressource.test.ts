import { describe, expect, it, vi } from "vitest";
import { avecMemoire, parLot, ressource } from "./ressource";

describe("ressource", () => {
  it("ne relit pas une valeur fraîche, relit une valeur vieillie", async () => {
    let heure = 0;
    const lire = vi.fn(() => Promise.resolve(`lecture à ${heure}`));
    const r = ressource(lire, 1000, () => heure);

    await r.charger();
    expect(r.etat().valeur).toBe("lecture à 0");
    heure = 500;
    await r.charger();
    expect(lire).toHaveBeenCalledTimes(1);

    heure = 1500;
    await r.charger();
    expect(lire).toHaveBeenCalledTimes(2);
    expect(r.etat().valeur).toBe("lecture à 1500");
    expect(r.etat().date).toBe(1500);
  });

  it("relit si on force ou après perimer()", async () => {
    const lire = vi.fn(() => Promise.resolve(1));
    const r = ressource(lire, 1000, () => 0);
    await r.charger();
    await r.charger(true);
    expect(lire).toHaveBeenCalledTimes(2);
    r.perimer();
    await r.charger();
    expect(lire).toHaveBeenCalledTimes(3);
    await r.charger();
    expect(lire).toHaveBeenCalledTimes(3);
  });

  it("ne fait qu'un appel pour deux demandes simultanées", async () => {
    const lire = vi.fn(() => Promise.resolve(1));
    const r = ressource(lire);
    await Promise.all([r.charger(), r.charger(true)]);
    expect(lire).toHaveBeenCalledTimes(1);
  });

  it("garde la dernière valeur après une erreur, et réessaie ensuite", async () => {
    let echec = false;
    const lire = vi.fn(() => (echec ? Promise.reject(new Error("réseau")) : Promise.resolve("ok")));
    const r = ressource(lire, 1000, () => 0);
    await r.charger();
    echec = true;
    await r.charger(true);
    expect(r.etat().valeur).toBe("ok");
    expect(r.etat().erreur).toBeInstanceOf(Error);
    echec = false;
    await r.charger(); // une erreur ne compte pas comme une lecture fraîche
    expect(r.etat().erreur).toBeUndefined();
    expect(lire).toHaveBeenCalledTimes(3);
  });

  it("prévient les abonnés au début et à la fin du chargement", async () => {
    const r = ressource(() => Promise.resolve(1));
    const etats: boolean[] = [];
    const desabonner = r.abonner(() => etats.push(r.etat().enCours));
    await r.charger();
    expect(etats).toEqual([true, false]);
    desabonner();
    await r.charger(true);
    expect(etats).toHaveLength(2);
  });
});

describe("avecMemoire", () => {
  it("ne refait pas la même recherche, sans tenir compte de la casse", async () => {
    let heure = 0;
    const chercher = vi.fn((texte: string) => Promise.resolve([texte]));
    const memo = avecMemoire(chercher, 1000, 10, () => heure);
    await memo("Crest");
    await memo("crest");
    expect(chercher).toHaveBeenCalledTimes(1);
    heure = 2000;
    await memo("crest");
    expect(chercher).toHaveBeenCalledTimes(2);
    memo.vider();
    await memo("crest");
    expect(chercher).toHaveBeenCalledTimes(3);
  });

  it("distingue les arguments et oublie une recherche qui échoue", async () => {
    let echec = true;
    const chercher = vi.fn((texte: string, groupe: string | null) =>
      echec ? Promise.reject(new Error("réseau")) : Promise.resolve([`${texte}/${groupe}`]),
    );
    const memo = avecMemoire(chercher);
    await expect(memo("repas", null)).rejects.toThrow("réseau");
    echec = false;
    expect(await memo("repas", null)).toEqual(["repas/null"]);
    expect(await memo("repas", "Restauration")).toEqual(["repas/Restauration"]);
    expect(chercher).toHaveBeenCalledTimes(3);
  });

  it("oublie la plus ancienne quand la mémoire est pleine", async () => {
    const chercher = vi.fn((texte: string) => Promise.resolve(texte));
    const memo = avecMemoire(chercher, 1000, 2, () => 0);
    await memo("a");
    await memo("b");
    await memo("c");
    await memo("b");
    await memo("a");
    expect(chercher).toHaveBeenCalledTimes(4);
  });
});

describe("parLot", () => {
  it("regroupe les demandes rapprochées en un appel", async () => {
    vi.useFakeTimers();
    const lire = vi.fn((codes: string[]) =>
      Promise.resolve(codes.filter((c) => c !== "SANS-PRIX").map((c) => ({ code: c, prix: c.length }))),
    );
    const prix = parLot(lire, (p) => p.code, 40);
    const demandes = Promise.all([prix("REPAS"), prix("SALLE"), prix("REPAS"), prix("SANS-PRIX")]);
    await vi.advanceTimersByTimeAsync(50);
    expect(await demandes).toEqual([
      { code: "REPAS", prix: 5 },
      { code: "SALLE", prix: 5 },
      { code: "REPAS", prix: 5 },
      undefined,
    ]);
    expect(lire).toHaveBeenCalledTimes(1);
    expect(lire).toHaveBeenCalledWith(["REPAS", "SALLE", "SANS-PRIX"]);
    vi.useRealTimers();
  });

  it("répond undefined si la lecture échoue", async () => {
    vi.useFakeTimers();
    const prix = parLot(
      () => Promise.reject(new Error("réseau")),
      (p: { code: string }) => p.code,
    );
    const demande = prix("REPAS");
    await vi.advanceTimersByTimeAsync(50);
    expect(await demande).toBeUndefined();
    vi.useRealTimers();
  });
});
