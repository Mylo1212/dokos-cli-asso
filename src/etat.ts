// État partagé minimal entre sections : assistant IA actif ou non, et
// navigation demandée depuis n'importe où (alerte, chatbot, palette, accueil).

type Ecouteur = () => void;

let iaActive = true;
const ecouteurs = new Set<Ecouteur>();

export function assistantActif(): boolean {
  return iaActive;
}

export function definirAssistantActif(actif: boolean): void {
  if (iaActive === actif) return;
  iaActive = actif;
  for (const e of ecouteurs) e();
}

/** Appelé à chaque changement d'état de l'assistant. */
export function surChangementAssistant(ecouteur: Ecouteur): void {
  ecouteurs.add(ecouteur);
}

export type Onglet = "accueil" | "ventes" | "formulaires" | "contrats" | "import" | "listes" | "suivi" | "reglages";

/** Demande l'affichage d'une section ; `ancre` est l'identifiant d'un bloc à montrer dans celle-ci. */
export function ouvrirOnglet(onglet: Onglet, ancre?: string): void {
  document.dispatchEvent(new CustomEvent("ouvrir-onglet", { detail: { onglet, ancre } }));
}

/** Événement interne adressé à une section (construite au besoin par `ouvrirOnglet`). */
export function signaler<T>(nom: string, detail: T): void {
  document.dispatchEvent(new CustomEvent(nom, { detail }));
}

/** Lit une préférence d'affichage ; `defaut` si le stockage est indisponible. */
export function preference(cle: string, defaut: string): string {
  try {
    return localStorage.getItem(`dokos-assistant.${cle}`) ?? defaut;
  } catch {
    return defaut;
  }
}

export function enregistrerPreference(cle: string, valeur: string): void {
  try {
    localStorage.setItem(`dokos-assistant.${cle}`, valeur);
  } catch {
    // Préférence d'affichage seulement : rien à faire si le stockage est indisponible.
  }
}
