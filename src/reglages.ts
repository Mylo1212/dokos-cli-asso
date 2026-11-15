// Onglet Réglages : adresse de Dokos, société, et une clé API par profil.

import { api, type EtatConfig } from "./api";
import { alerte, alerteErreur, bouton, caseACocher, champ, copier, h, lienFichier, occupe, saisie } from "./dom";
import { definirAssistantActif } from "./etat";

export function initialiserReglages(panneau: HTMLElement, surEnregistrement: () => void): (etat: EtatConfig) => void {
  const url = saisie({ type: "url", placeholder: "https://dokos.exemple.fr" });
  const http = caseACocher("Autoriser http:// (réseau local uniquement)");
  const societe = saisie({ placeholder: "Société Dokos (obligatoire pour créer des employés)" });
  const zoneConnexion = h("div", { "aria-live": "polite" });
  const enregistrer = bouton("Enregistrer", "principal", async () => {
    const ok = await occupe([enregistrer], zoneConnexion, async () => {
      await api.enregistrerConfig(url.value, http.input.checked, societe.value);
      return true;
    });
    if (ok) {
      zoneConnexion.replaceChildren(alerte("succes", "Adresse enregistrée"));
      surEnregistrement();
    }
  });
  const profils = h("div");

  // --- Assistant IA ---
  const iaActive = caseACocher("Assistant IA activé (zones « Assistant » dans chaque onglet)", true);
  const urlIa = saisie({ type: "url", placeholder: "http://mac-studio:8080" });
  const modele = saisie({ placeholder: "qwen3.6-35b-a3b" });
  const basesVentes = saisie({ placeholder: "Identifiants séparés par des virgules" });
  const basesFormulaires = saisie({ placeholder: "Identifiants séparés par des virgules" });
  const basesContrats = saisie({ placeholder: "Identifiants séparés par des virgules" });
  const cleIa = saisie({ type: "password", autocomplete: "off" });
  const zoneIa = h("div", { "aria-live": "polite" });
  const enregistrerIa = bouton("Enregistrer et tester", "principal", async () => {
    const etat = await occupe([enregistrerIa], zoneIa, () =>
      api.enregistrerIa(
        iaActive.input.checked,
        urlIa.value,
        modele.value,
        basesVentes.value,
        basesFormulaires.value,
        basesContrats.value,
        cleIa.value,
      ),
    );
    definirAssistantActif(iaActive.input.checked);
    if (etat !== undefined) {
      cleIa.value = "";
      cleIa.placeholder = "Clé enregistrée (laisser vide pour la garder)";
      zoneIa.replaceChildren(alerte("succes", etat));
    }
  });
  async function chargerIa() {
    try {
      const etat = await api.etatIa();
      iaActive.input.checked = etat.active;
      definirAssistantActif(etat.active);
      urlIa.value = etat.url;
      modele.value = etat.modele;
      basesVentes.value = etat.basesVentes;
      basesFormulaires.value = etat.basesFormulaires;
      basesContrats.value = etat.basesContrats;
      cleIa.placeholder = etat.cleEnregistree
        ? "Clé enregistrée (laisser vide pour la garder)"
        : "Clé API (Open WebUI › Réglages › Compte)";
    } catch (e) {
      zoneIa.replaceChildren(alerteErreur(e, () => void chargerIa()));
    }
  }
  const chemin = h("p", { class: "note chemin" });

  // --- Diagnostic ---
  const zoneDiagnostic = h("div", { "aria-live": "polite" });
  const texteDiagnostic = h("textarea", { rows: "10", readonly: true, "aria-label": "Diagnostic" });
  const voirDiagnostic = bouton("Préparer le diagnostic", "", async () => {
    const d = await occupe([voirDiagnostic], zoneDiagnostic, () => api.diagnostic());
    if (!d) return;
    texteDiagnostic.value = d.texte;
    const copierBouton = bouton("Copier", "principal", async () => {
      const ok = await copier(d.texte);
      zoneDiagnostic.replaceChildren(
        alerte(
          ok ? "succes" : "attention",
          ok
            ? "Diagnostic copié : collez-le dans votre demande d'aide."
            : "Copie impossible : sélectionnez le texte et copiez-le à la main.",
        ),
      );
    });
    zoneDiagnostic.replaceChildren(
      texteDiagnostic,
      h(
        "div",
        { class: "actions" },
        copierBouton,
        d.dossierJournaux ? lienFichier(d.dossierJournaux, "Ouvrir le dossier des journaux") : null,
        d.dossierDocuments ? lienFichier(d.dossierDocuments, "Ouvrir Documents\\Dokos") : null,
      ),
    );
  });

  panneau.append(
    h("h1", {}, "Réglages"),
    h(
      "p",
      { class: "sous-titre" },
      "Une clé API par usage, chacune sur un utilisateur Dokos au rôle minimal. Les secrets restent dans le Gestionnaire d'identification Windows.",
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, "Dokos"),
      h("div", { class: "grille" }, champ("Adresse", url, "large"), champ("Société", societe)),
      h("div", { class: "actions" }, http.label),
      h("div", { class: "actions" }, enregistrer),
      zoneConnexion,
    ),
    h("div", { class: "carte" }, h("h2", {}, "Clés API"), profils, chemin),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, "Assistant IA (Open WebUI)"),
      h("div", { class: "actions sans-marge" }, iaActive.label),
      h(
        "div",
        { class: "grille" },
        champ("Adresse", urlIa),
        champ("Modèle", modele),
        champ("Clé API", cleIa),
        champ("Bases de connaissance — Ventes", basesVentes),
        champ("Bases — Formulaires", basesFormulaires),
        champ("Bases — Contrats", basesContrats),
      ),
      h(
        "p",
        { class: "note" },
        "http:// n'est accepté que vers une adresse Tailscale (nom court, *.ts.net, 100.x.y.z) ou locale. Les bases sont désignées par leur identifiant Open WebUI ; une base introuvable est ignorée avec un avertissement.",
      ),
      h("div", { class: "actions" }, enregistrerIa),
      zoneIa,
    ),
    h(
      "div",
      { class: "carte" },
      h("h2", {}, "Diagnostic"),
      h(
        "p",
        { class: "note sans-marge" },
        "Versions, réglages sans aucun secret et dernières lignes du journal technique, à copier dans une demande d'aide. Le journal des actions (ce qui a été créé, quand) est dans Documents\\Dokos\\journal-actions.csv.",
      ),
      h("div", { class: "actions" }, voirDiagnostic),
      zoneDiagnostic,
    ),
  );

  return (etat: EtatConfig) => {
    void chargerIa();
    url.value = etat.url;
    http.input.checked = etat.autoriserHttp;
    societe.value = etat.societe;
    chemin.textContent = etat.cheminConfig ? `Configuration : ${etat.cheminConfig}` : "";
    profils.replaceChildren(
      ...etat.profils.map((p) => {
        const cle = saisie({ value: p.cle ?? "", placeholder: "Clé API", autocomplete: "off" });
        const secret = saisie({
          type: "password",
          placeholder: p.cle ? "Secret enregistré (laisser vide pour tester)" : "Secret API",
          autocomplete: "off",
        });
        const etatProfil = h("div", { class: "etat", "aria-live": "polite" });
        const action = bouton(p.cle ? "Tester" : "Enregistrer", "", async () => {
          const utilisateur = await occupe([action], etatProfil, () =>
            secret.value.trim() ? api.enregistrerProfil(p.profil, cle.value, secret.value) : api.testerProfil(p.profil),
          );
          if (utilisateur !== undefined) {
            secret.value = "";
            etatProfil.replaceChildren(
              h("span", { class: "marque-etat ok" }, "✓"),
              `Connecté en tant que ${utilisateur}`,
            );
            surEnregistrement();
          }
        });
        secret.addEventListener(
          "input",
          () => (action.textContent = secret.value ? "Tester et enregistrer" : p.cle ? "Tester" : "Enregistrer"),
        );
        return h(
          "div",
          { class: "profil" },
          h("div", { class: "nom" }, p.libelle),
          champ("Clé", cle),
          champ("Secret", secret),
          action,
          h("div", { class: "roles" }, `Rôles conseillés : ${p.roles}.`),
          etatProfil,
        );
      }),
    );
  };
}

export function erreurConfig(e: unknown): HTMLElement {
  return alerteErreur(e);
}
