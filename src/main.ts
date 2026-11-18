import { api } from "./api";
import { creerAccueil } from "./accueil";
import { initialiserChat } from "./chat";
import { alerte, h } from "./dom";
import { chargerSuivi, donnees } from "./donnees";
import { enregistrerPreference, preference, type Onglet } from "./etat";
import { initialiserImport } from "./import";
import { initialiserExport } from "./export";
import { creerMenu, libelleSection, SECTIONS } from "./navigation";
import { commandesDeBase, creerPalette } from "./palette";
import { erreurConfig, initialiserReglages } from "./reglages";
import { initialiserSuivi } from "./suivi";
import { initialiserVentes } from "./ventes";

const panneau = (id: Onglet) => document.querySelector<HTMLElement>(`#${id}`)!;

// Chaque section n'est construite qu'à sa première visite : au démarrage,
// seuls l'accueil et les réglages le sont.
const constructeurs: Record<Onglet, (p: HTMLElement) => void> = {
  accueil: creerAccueil,
  ventes: initialiserVentes,
  import: initialiserImport,
  export: initialiserExport,
  suivi: initialiserSuivi,
  reglages: () => {},
};
const construits = new Set<Onglet>(["reglages"]);

let courant: Onglet = "accueil";

function afficher(onglet: Onglet, ancre?: string) {
  if (!construits.has(onglet)) {
    construits.add(onglet);
    constructeurs[onglet](panneau(onglet));
  }
  courant = onglet;
  menu.marquer(onglet);
  for (const s of SECTIONS) panneau(s.id).hidden = s.id !== onglet;
  document.title = `${libelleSection(onglet)} — Dokos Asso`;
  panneau(onglet).dispatchEvent(new Event("onglet-affiche"));
  const cible = ancre ? document.getElementById(ancre) : null;
  if (cible) cible.scrollIntoView({ block: "start" });
  else window.scrollTo({ top: 0 });
}

// --- Thème : celui du système, ou forcé depuis la palette ---
type Theme = "auto" | "light" | "dark";
function appliquerTheme(theme: Theme) {
  if (theme === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  enregistrerPreference("theme", theme);
}
appliquerTheme(preference("theme", "auto") as Theme);

const palette = creerPalette({
  commandes: () => [
    ...commandesDeBase(),
    {
      libelle: "Actualiser les données de Dokos",
      groupe: "Affichage",
      icone: "actualiser",
      mots: "rafraichir recharger brouillons factures",
      action: () => {
        void donnees.connexions.charger(true);
        void chargerSuivi(true);
      },
    },
    {
      libelle: "Afficher ou masquer l'assistant",
      groupe: "Affichage",
      icone: "assistant",
      touche: "Ctrl J",
      mots: "chatbot ia discussion",
      action: () => chat.basculer(),
    },
    {
      libelle: "Réduire ou déployer le menu",
      groupe: "Affichage",
      icone: "menu",
      touche: "Ctrl B",
      action: () => menu.basculer(),
    },
    ...(
      [
        ["auto", "Thème du système"],
        ["light", "Thème clair"],
        ["dark", "Thème sombre"],
      ] as [Theme, string][]
    ).map(([theme, libelle]) => ({
      libelle,
      groupe: "Affichage",
      icone: "theme" as const,
      mots: "couleurs apparence nuit jour",
      action: () => appliquerTheme(theme),
    })),
  ],
  demanderAssistant: (texte) => chat.demander(texte),
});

const menu = creerMenu(document.querySelector<HTMLElement>("#menu")!, {
  surSection: (onglet) => afficher(onglet),
  surRecherche: () => palette.ouvrir(),
});

document.addEventListener("ouvrir-onglet", (e) => {
  const { onglet, ancre } = (e as CustomEvent<{ onglet: Onglet; ancre?: string }>).detail;
  afficher(onglet, ancre);
});

const chat = initialiserChat({
  panneau: document.querySelector<HTMLElement>("#chat")!,
  disposition: document.querySelector<HTMLElement>(".disposition")!,
  boutonEntete: menu.boutonAssistant,
  ongletCourant: () => courant,
});
const remplirReglages = initialiserReglages(panneau("reglages"), () => void rafraichir());

// --- Raccourcis clavier ---
document.addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
  const touche = e.key.toLowerCase();
  const section = SECTIONS.find((s) => s.touche === touche);
  if (touche === "k") palette.ouvrir();
  else if (touche === "j") chat.basculer();
  else if (touche === "b") menu.basculer();
  else if (section) afficher(section.id);
  else if (touche === "enter" && !(e.target as HTMLElement).closest("#chat, .palette-fond")) {
    // Ctrl+Entrée : la vérification de la section affichée (jamais une création).
    const verifier = panneau(courant).querySelector<HTMLButtonElement>('[data-raccourci="verifier"]');
    if (!verifier || verifier.disabled || verifier.closest("[hidden]")) return;
    verifier.click();
  } else return;
  e.preventDefault();
});

async function rafraichir(): Promise<boolean> {
  try {
    const etat = await api.etatConfig();
    remplirReglages(etat);
    const pret = etat.configure && etat.profils.some((p) => p.cle);
    // Ouvre les connexions tout de suite : la première recherche n'attend pas,
    // et une clé refusée se voit dans le menu.
    if (pret) void donnees.connexions.charger(true);
    return pret;
  } catch (e) {
    panneau("reglages").prepend(erreurConfig(e));
    return false;
  }
}

void rafraichir().then((pret) => {
  if (pret) return afficher("accueil");
  panneau("reglages").prepend(
    alerte(
      "info",
      "Bienvenue",
      h("p", {}, "Renseignez l'adresse de votre Dokos, puis au moins une clé API pour commencer."),
    ),
  );
  afficher("reglages");
});
