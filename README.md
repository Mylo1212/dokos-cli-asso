# dokos-asso

Créer devis, commandes et factures dans Dokos, par son API REST (Frappe), sans
Server Script ; importer des factures, paiements, articles et clients depuis
Excel ; exporter des listes vers Excel ; suivre ce qui attend.

Version « asso » de [dokos-cli](https://github.com/Mylo1212/dokos-cli) : même
fonctionnement sous-jacent, périmètre réduit (ni contrats de travail, ni
employés à créer, ni formats d'impression) et trois apports : un **prix** par
ligne en saisie libre, des **listes déroulantes** pour tout ce qui se cherche
dans Dokos, et l'**export du grand livre** au format d'import.

Règle de conception : **l'outil propose, Rust vérifie, l'utilisateur valide, Dokos enregistre.**

Rien n'a encore été essayé contre une vraie instance Dokos ni contre votre
Open WebUI : commencez par une instance de test.

## Ce que fait l'appli

| Section | Contenu |
|---|---|
| **Ventes** (Créer) | Devis, commande ou facture en brouillon. Client et articles choisis dans des listes déroulantes ; **prix unitaire HT facultatif** par ligne (vide : prix de Dokos). Ou « À partir d'un modèle » : copie d'un document étiqueté « Template ». Catalogue des groupes d'articles. |
| **Import** | Un modèle Excel généré depuis votre Dokos, analysé ligne par ligne, création tout ou rien : **Factures** (plusieurs articles par facture, regroupés automatiquement), **Paiements** (écritures de paiement en brouillon, un paiement pouvant régler plusieurs factures d'un client avec le montant de chacune), **Articles**, **Clients**. |
| **Export** | Clients, articles, devis, commandes, factures ou employés dans un fichier Excel, avec filtres (client, groupe, territoire, période, statut, nom) en listes déroulantes. **Grand livre** d'un journal comptable et d'une période, directement au format d'import (colonnes DATE à REFINTERNE), avec le journal du fichier au choix : **VEN** ou **REC**. Lecture seule. |
| **Suivi** | Mes brouillons (et « Commander » / « Facturer »), factures en retard. Lecture seule, sauf la transformation qui crée un brouillon. |
| **Accueil**, **Réglages**, **Assistant** | Tableau de bord ; adresse de Dokos et clés API ; chatbot Open WebUI qui qualifie la demande et pré-remplit Ventes ou Export. |

Guide complet : `docs/guide-utilisateur.md`. Droits des utilisateurs API :
`docs/utilisateurs-api.md`.

`Ctrl+K` cherche une action, un client ou un article ; `Ctrl+1` à `Ctrl+6`
changent d'onglet, `Ctrl+Entrée` vérifie, `Ctrl+J` montre l'assistant.

## Arborescence

```
index.html, src/             interface TypeScript (Vite), sans framework
src-tauri/                   appli Tauri + racine du workspace (resolver 3, MSRV 1.90)
├─ src/                      commandes Tauri (capacité vide), services testables
├─ dokos-core/               API Dokos, sans Tauri ni IA
│   transport.rs, http.rs    trait Transport (lots de lectures), faux transport, HTTPS (rustls/ring, gzip)
│   client.rs, meta.rs       /api/resource, /api/method, structure des DocTypes (gardée 2 min)
│   vente.rs, doublon.rs     devis, commandes, factures en brouillon (prix par ligne facultatif) + doublons
│   recherche.rs             recherche par texte, lecture complète des clients et articles (listes déroulantes)
│   paiement.rs              écritures de paiement en brouillon (une ou plusieurs factures), modes et comptes
│   article.rs, clientele.rs groupes d'articles, prix, articles désactivés ; clients
│   brouillons.rs, suivi.rs, transformation.rs   brouillons, retards, devis → commande → facture
│   export.rs                listes exportables : colonnes et filtres fixes, pagination, lecture seule
│   modele_vente.rs          documents « Template » : liste, lecture, copie avec client, dates, quantités
│   fiche.rs                 fiches clients et articles détaillées (chatbot), lecture seule
├─ dokos-import/             modèles Excel générés depuis Dokos, analyse, création tout ou rien ; écriture des exports
├─ assistant-core/           IA : client Open WebUI, extractions, propositions vérifiées
├─ dokos-config/             config.toml, profils, trousseau, journal et diagnostic
└─ dokos-cli/                binaire `dokos`
```

## Mise en place

1. Dans Dokos, créez **un utilisateur API par profil**, chacun avec le rôle minimal :

   | Profil | Sert à | Rôles conseillés |
   |---|---|---|
   | `ventes` | clients, articles, devis, commandes, factures, paiements, export | lecture clients/articles, lecture et création des documents de vente et des écritures de paiement, **sans** validation |
   | `rh` (facultatif) | export des employés | lecture seule |
   | `compta` (facultatif) | export du grand livre | lecture de GL Entry, Accounting Journal, Account |

2. Renseignez l'adresse, la société et les clés : onglet **Réglages** de l'appli,
   ou `dokos connexion enregistrer --profil ventes --url https://… --cle … --societe "…"`.
   Les secrets vont dans le Gestionnaire d'identification Windows (service
   `dokos-cli-asso`) ; `%APPDATA%\dokos-cli-asso\config.toml` ne contient que
   l'adresse et les clés. Les deux applis (dokos-cli et dokos-cli-asso) ont des
   réglages séparés.

## L'appli

```powershell
npm ci
npm run tauri dev      # développement
npm run tauri build    # installateur Windows (NSIS, MSI)
```

La fenêtre n'a aucune permission Tauri : tout passe par les commandes Rust, qui
construisent seules les requêtes. Les liens ne s'ouvrent que vers votre Dokos,
les fichiers que dans `Documents\Dokos` (`modeles`, `exports`, journal des actions).

## Assistant IA

Un panneau **Assistant**, à droite de chaque onglet, est un chatbot : il
qualifie la demande (une question à la fois), indique l'onglet où la faire,
explique la procédure et, quand il a l'essentiel, propose **Pré-remplir dans …**
(Ventes, Export). On relit, puis on crée comme d'habitude. Sa
référence est `docs/guide-utilisateur.md`, joint à la compilation ; il ne voit
pas les données de Dokos. Il ne remplit jamais de prix.

- **Réglages › Assistant IA** : adresse (par défaut `http://mac-studio:8080`,
  nom Tailscale), modèle (par défaut `qwen3.6-35b-a3b`), clé API Open WebUI
  (gardée dans le trousseau) et bases de connaissance des ventes (`prestations` :
  nom d'attente à remplacer par l'identifiant réel).
- **Réseau** : `http://` n'est accepté que vers une adresse Tailscale (nom court,
  `*.ts.net`, `100.64.0.0/10`) ou locale ; ailleurs, `https://` est obligatoire.
  Le texte des demandes (noms de clients) part vers ce serveur.
- Elle **lit seulement** : aucun outil d'écriture, aucune clé Dokos. Clients,
  articles et groupes sont **recherchés dans Dokos** : ambigu → choix proposés,
  introuvable → question, jamais de création.
- Si Open WebUI est injoignable, l'erreur s'affiche dans la zone Assistant et
  le reste de l'appli fonctionne normalement.

Test réel, contre votre serveur :

```powershell
$env:OWUI_URL = "http://mac-studio:8080"; $env:OWUI_CLE = "sk-…"; $env:OWUI_MODELE = "qwen3.6-35b-a3b"
cargo test -p assistant-core --test reel -- --ignored --nocapture
```

## Ligne de commande

```powershell
dokos connexion tester
dokos devis brouillon --client "Mairie de Crest" --date 12/11/2026 --ligne "repas=25" --simulation
dokos facture brouillon --client CUST-0002 --date 03/11/2026 --echeance 03/12/2026 --ligne "repas=25*12,5" --ligne "grande salle=1"
dokos commande brouillon --client CUST-0002 --livraison 12/11/2026 --ligne "repas=25" --ligne "grande salle=1*300@13/11/2026"

dokos articles arbre
dokos articles lister Restauration
dokos modele factures
dokos importer factures "Documents\Dokos\modeles\modele-factures.xlsx" --simulation
dokos importer clients clients.xlsx --rapport resultat.xlsx
dokos modele paiements
dokos importer paiements "Documents\Dokos\modeles\modele-paiements.xlsx" --simulation
dokos exporter factures --statut impayee --du 01/10/2026 --au 31/10/2026
dokos exporter clients --groupe Collectivités --sortie clients.xlsx
dokos brouillons
dokos devis commander SAL-QTN-2026-00012
dokos commande facturer SAL-ORD-2026-00031
dokos facture retards
dokos journaux
dokos grand-livre --journal VEN --du 01/10/2026 --au 31/10/2026
dokos grand-livre --journal VEN --du 01/10/2026 --au 31/10/2026 --journal-fichier REC
dokos diagnostic
```

`--ligne ARTICLE=QUANTITÉ[*PRIX][@DATE]` : `*PRIX` est le prix unitaire HT de
la ligne (vide : prix de Dokos), `@DATE` la livraison d'une ligne de commande.
`--simulation` n'écrit rien. `--autoriser-doublon` force la création malgré un doublon.

## Contrôles

- **Doublons** : devis, commande, facture (même client, même date, mêmes lignes),
  paiement (même première facture, même date, même montant), article (même code
  ou libellé), client (même nom). Un doublon n'est jamais recréé sans `--autoriser-doublon`.
- **Prix saisi** : dès qu'une ligne a un prix, les règles de prix de Dokos
  (remises) ne s'appliquent à aucune ligne du document ; les taxes restent
  calculées par Dokos.

## Hypothèses à vérifier dans votre Dokos

Vérifiées dans le code de Dokos v5.18 et de l'appli `bookings` :

| Usage | DocType | Champs |
|---|---|---|
| Client, article | `Customer`, `Item` | `customer_name`, `item_name`, `disabled`, `is_sales_item`, `has_variants` |
| Devis | `Quotation` | `quotation_to`, `party_name`, `transaction_date`, `valid_till`, `items[]` |
| Commande | `Sales Order` | `customer`, `delivery_date`, `items[].delivery_date` |
| Facture | `Sales Invoice` | `customer`, `posting_date`, `set_posting_time`, `due_date`, `items[].rate`, `company`, `outstanding_amount` |
| Paiement | `Payment Entry` (+ `Payment Entry Reference`, `Mode of Payment`, `Account`) | `payment_type`, `posting_date`, `paid_amount`, `received_amount`, `mode_of_payment`, `reference_no`, `reference_date`, `references[]` ; méthodes `get_payment_entry` et `get_bank_cash_account` ; `enabled`, `account_type` |
| Grand livre (lecture) | `GL Entry`, `Accounting Journal`, `Account` | `posting_date`, `account`, `debit`, `credit`, `party`, `voucher_no`, `accounting_journal`, `is_cancelled`, `is_opening` ; `journal_code`, `journal_name`, `type` ; `account_number`, `account_type` |

## Développement

```powershell
npm run tout    # tout vérifier et tester : interface, bibliothèques, CLI, appli
```

`npm run tout` enchaîne, en s'arrêtant à la première erreur : mise en forme
(`prettier`), types et construction de l'interface (`npm run build`), tests de
l'interface (`npm test`), puis dans `src-tauri` `cargo fmt --all --check`,
`cargo clippy --workspace` et `cargo test --workspace`. Sans `--workspace`,
cargo ne vise que l'appli.

```powershell
# Tests réels contre une instance de test : hypothèses sur les DocTypes (lecture),
# puis création et suppression d'un devis brouillon.
$env:DOKOS_TEST_URL = "https://dokos-test.exemple.fr"
$env:DOKOS_TEST_CLE = "…"; $env:DOKOS_TEST_SECRET = "…"
$env:DOKOS_TEST_CLIENT = "CUST-0001"; $env:DOKOS_TEST_ARTICLE = "REPAS"
cargo test -p dokos-core --test reel -- --ignored --nocapture
```

Livraison : un tag `vX.Y.Z` construit les installateurs Windows et macOS dans
une release GitHub en brouillon (`.github/workflows/release.yml`). Journaux :
`%APPDATA%\dokos-cli-asso\logs` ; actions : `Documents\Dokos\journal-actions.csv`.
