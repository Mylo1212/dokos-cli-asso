# Utilisateurs API de Dokos pour Dokos Asso

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
| Customer | oui | oui (import Clients) | | listes déroulantes, export, création à l'import |
| Item | oui | oui (catalogue, import Articles) | oui (activer, déplacer) | catalogue, listes déroulantes, export |
| Item Group | oui | oui (sous-groupes) | | arbre du catalogue |
| Item Price, Selling Settings, UOM | oui | | | prix indicatif, unités |
| Customer Group, Territory | oui | | | listes du modèle Clients, filtres de l'export |
| Quotation, Sales Order, Sales Invoice | oui | oui | | brouillons, doublons, transformation, brouillons en attente, factures en retard |

Les méthodes `make_sales_order` et `make_sales_invoice` (Commander, Facturer)
demandent la lecture du document source validé et la création de la cible.
(Rôle standard proche : *Sales User*, sans « Soumettre ».)

## Profil `evenements`

| DocType | Lire | Créer | Modifier | Pourquoi |
|---|---|---|---|---|
| Event | oui | oui | oui (publier) | événements |
| Web Form | oui | oui | oui (publier) | formulaires d'inscription |
| Event Registration | oui | | | inscrits d'un événement (Suivi) |

La lecture de la structure d'« Event Registration » (`getdoctype`) est
nécessaire pour les questions. (Rôle standard proche : *Website Manager*.)

## Profil `rh`

Facultatif : il ne sert qu'à lire. Sans lui, l'export des employés est
indisponible. L'appli ne crée ni employé ni contrat.

| DocType | Lire | Créer | Modifier | Pourquoi |
|---|---|---|---|---|
| Employee | oui | | | export des employés |

(Rôle standard proche : *HR User*, plus large que nécessaire.)

## Profil `compta`

Facultatif : il ne sert qu'à l'export du grand livre, en lecture seule. Aucun
droit de création ni de validation.

| DocType | Lire | Pourquoi |
|---|---|---|
| GL Entry | oui | écritures du journal et de la période |
| Accounting Journal | oui | liste déroulante des journaux |
| Account | oui | numéros et types de compte (valeurs nettes des comptes de tiers) |

(Rôles standard proches : *Accounts User* ou *Auditor*.)

## Vérifier

```powershell
dokos connexion tester
cargo test -p dokos-core --test reel -- --ignored --nocapture   # hypothèses sur les DocTypes
```

Le second test lit la structure de chaque DocType utilisé et signale les
champs absents de votre Dokos.
