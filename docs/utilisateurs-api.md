# Utilisateurs API de Dokos pour Dokos Assistant

Un utilisateur API **par profil**, chacun avec le strict nécessaire. L'outil ne
valide, ne soumet et n'annule jamais : aucun profil n'a besoin du droit
« Soumettre » ni « Annuler ». Les clés se créent dans Dokos : *Utilisateur ›
Accès API › Générer les clés*.

Les droits se donnent par **rôle** (Gestionnaire des rôles) ; le plus simple
est de créer trois rôles dédiés et d'y attacher les DocTypes ci-dessous. Les
rôles standard entre parenthèses conviennent aussi, mais donnent plus que
nécessaire.

## Profil `ventes`

| DocType | Lire | Créer | Modifier | Pourquoi |
|---|---|---|---|---|
| Customer | oui | oui (import Clients) | | recherche, création à l'import |
| Item | oui | oui (catalogue, import) | oui (activer, déplacer) | catalogue |
| Item Group | oui | oui (sous-groupes) | | arbre du catalogue |
| Item Price, Selling Settings, UOM | oui | | | prix indicatif, unités |
| Customer Group, Territory | oui | | | listes du modèle Clients |
| Quotation, Sales Order, Sales Invoice | oui | oui | | brouillons, doublons, transformation, brouillons en attente, factures en retard |

Les méthodes `make_sales_order` et `make_sales_invoice` (Commander, Facturer)
demandent la lecture du document source validé et la création de la cible.
(Rôle standard proche : *Sales User*, sans « Soumettre ».)

## Profil `evenements`

| DocType | Lire | Créer | Modifier | Pourquoi |
|---|---|---|---|---|
| Event | oui | oui | oui (publier) | événements |
| Web Form | oui | oui | oui (publier) | formulaires d'inscription |
| Event Registration | oui | oui (inscriptions saisies, import) | | inscrits |

La lecture de la structure d'« Event Registration » (`getdoctype`) est
nécessaire pour les questions. (Rôle standard proche : *Website Manager*.)

## Profil `rh`

| DocType | Lire | Créer | Modifier | Pourquoi |
|---|---|---|---|---|
| Employee | oui | oui | | employés |
| Employment Contract | oui | oui | | contrats, alertes |
| Employment Contract Type, Designation, Gender | oui | | | listes |
| Print Format | oui | | | contrôle des formats, PDF |

Le PDF utilise `frappe.utils.print_format.download_pdf` : lecture du contrat
et du format suffisent. L'outil ne modifie **jamais** un format d'impression.
(Rôle standard proche : *HR User*.)

## Vérifier

```powershell
dokos connexion tester
cargo test -p dokos-core --test reel -- --ignored --nocapture   # hypothèses sur les DocTypes
```

Le second test lit la structure de chaque DocType utilisé et signale les
champs absents de votre Dokos (champs personnalisés des contrats compris).
