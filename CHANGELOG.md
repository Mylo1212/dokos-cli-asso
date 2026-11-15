# Journal des versions

Format : une section par version, les changements regroupés par nature. Les
versions suivent `MAJEUR.MINEUR.CORRECTIF` ; un tag `vX.Y.Z` déclenche la
construction des installateurs (voir `.github/workflows/release.yml`).

## 0.1.0

Première version de **dokos-asso**, tirée de dokos-cli (commit `6b4269f`, 0.2.3)
avec le même fonctionnement sous-jacent et un périmètre réduit.

### Conservé tel quel

- **Ventes** (devis, commandes, factures en brouillon, doublons, catalogue,
  « À partir d'un modèle »), **Formulaires** (événement et formulaire
  d'inscription non publiés, publication séparée), **Suivi** (brouillons,
  « Commander » / « Facturer », inscrits, échéances des contrats de travail,
  factures en retard), Accueil, palette `Ctrl+K`, Réglages, assistant IA.
- Ligne de commande `dokos`, réduite aux mêmes fonctions.

### Ajouté

- **Prix unitaire HT par ligne** en saisie libre (Ventes, `--ligne
  ARTICLE=QTE*PRIX` en ligne de commande). Vide : prix de la liste de prix de
  Dokos ; saisi : prix de la ligne (0 : offert), et les règles de prix de
  Dokos (remises) ne s'appliquent alors à aucune ligne du document. Le total de
  la barre du bas prend le prix saisi, sinon le prix indicatif.
- **Listes déroulantes** pour tout ce qui se cherche dans Dokos : client et
  article (Ventes), client, groupe et territoire (Export), article à déplacer
  (catalogue). Elles s'ouvrent au clic ou à la flèche, montrent toutes les
  valeurs, se filtrent à la frappe (accents et casse ignorés) et sont lues une
  fois dans Dokos puis gardées deux minutes.
- **Export** (ancien onglet « Listes »), fichiers dans
  `Documents\Dokos\exports`, et commande `dokos exporter <liste> [filtres]`.

### Retiré

- Contrats de travail, création d'employés, contrôle et rédaction des formats
  d'impression, liste des mentions obligatoires (crate `conformite`), PDF de contrat.
- Import : devis, commandes, inscriptions, employés, programme d'événements et
  les copies de modèles pour devis et commandes. Restent **Factures**,
  **Factures à partir d'un modèle**, **Articles**, **Clients**.
- Suivi : le profil RH ne sert plus qu'à lire les échéances de contrats et à
  exporter les employés.

### Séparé de dokos-cli

- Dossier de configuration `%APPDATA%\dokos-cli-asso`, service du trousseau
  `dokos-cli-asso`, identifiant d'appli `com.lesamanins.dokos-asso`, signature
  des modèles Excel `dokos-asso` : les deux applis cohabitent sans se mélanger.
