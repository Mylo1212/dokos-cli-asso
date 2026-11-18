// Pictogrammes au trait, dessinés en SVG par le DOM (pas de HTML en chaîne,
// pas de fichier externe). Décoratifs : toujours accompagnés d'un texte.

type Forme = string | [cx: number, cy: number, r: number];

const FORMES = {
  accueil: ["M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"],
  ventes: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5", "M9 13h6", "M9 17h4"],
  import: ["M12 16V4", "M7 9l5-5 5 5", "M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3"],
  export: ["M12 4v12", "M7 11l5 5 5-5", "M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3"],
  suivi: ["M3 12h4l3-8 4 16 3-8h4"],
  reglages: ["M4 6h8", "M16 6h4", [14, 6, 2], "M4 12h2", "M10 12h10", [8, 12, 2], "M4 18h10", "M18 18h2", [16, 18, 2]],
  assistant: [
    "M11 3l1.9 5.1L18 10l-5.1 1.9L11 17l-1.9-5.1L4 10l5.1-1.9z",
    "M18.5 15l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z",
  ],
  recherche: [[11, 11, 7], "M21 21l-4.3-4.3"],
  actualiser: ["M20 11a8 8 0 1 0-2.3 5.7", "M20 4v7h-7"],
  fleche: ["M5 12h14", "M13 6l6 6-6 6"],
  menu: ["M4 5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z", "M9 4v16"],
  client: [[12, 8, 4], "M4 21v-1a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v1"],
  article: ["M21 8l-9-5-9 5v8l9 5 9-5z", "M3 8l9 5 9-5", "M12 13v8"],
  theme: [
    [12, 12, 4],
    "M12 2v2",
    "M12 20v2",
    "M4.9 4.9l1.4 1.4",
    "M17.7 17.7l1.4 1.4",
    "M2 12h2",
    "M20 12h2",
    "M4.9 19.1l1.4-1.4",
    "M17.7 6.3l1.4-1.4",
  ],
} satisfies Record<string, Forme[]>;

export type NomIcone = keyof typeof FORMES;

const SVG = "http://www.w3.org/2000/svg";

export function icone(nom: NomIcone): SVGSVGElement {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", "icone");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  for (const forme of FORMES[nom] as Forme[]) {
    if (typeof forme === "string") {
      const trace = document.createElementNS(SVG, "path");
      trace.setAttribute("d", forme);
      svg.append(trace);
    } else {
      const cercle = document.createElementNS(SVG, "circle");
      cercle.setAttribute("cx", String(forme[0]));
      cercle.setAttribute("cy", String(forme[1]));
      cercle.setAttribute("r", String(forme[2]));
      svg.append(cercle);
    }
  }
  return svg;
}
