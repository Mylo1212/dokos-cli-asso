# dokos-asso

Créer devis, commandes, factures et formulaires d'inscription dans Dokos, par
son API REST (Frappe), sans Server Script ; importer des factures, articles et
clients depuis Excel ; exporter des listes vers Excel ; suivre ce qui attend.

Version « asso » de [dokos-cli](https://github.com/Mylo1212/dokos-cli) : même
fonctionnement sous-jacent, périmètre réduit (ni contrats de travail, ni
employés à créer, ni formats d'impression) et deux apports : un **prix** par
ligne en saisie libre, et des **listes déroulantes** pour tout ce qui se
cherche dans Dokos.

Règle de conception : **l'outil propose, Rust vérifie, l'utilisateur valide, Dokos enregistre.**

Rien n'a encore été essayé contre une vraie instance Dokos ni contre votre
Open WebUI : commencez par une instance de test.

## Ce que fait l'appli

| Section | Contenu |
|---|---|
| **Ventes** (Créer) | Devis, commande ou facture en brouillon. Client et articles choisis dans des listes déroulantes ; **prix unitaire HT facultatif** par ligne (vide : prix de Dokos). Ou « À partir d'un modèle » : copie d'un document étiqueté « Template ». Catalogue des groupes d'articles. |
| **Formulaires** (Créer) | Un événement et son formulaire d'inscription, créés non publiés, puis publiés d'un clic séparé. |
| **Import** | Un modèle Excel généré depuis votre Dokos, analysé ligne par ligne, création tout ou rien : **Factures**, **Factures à partir d'un modèle**, **Articles**, **Clients**. |
| **Export** | Clients, articles, devis, commandes, factures ou employés dans un fichier Excel, avec filtres (client, groupe, territoire, période, statut, nom) en listes déroulantes. Lecture seule. |
| **Suivi** | Mes brouillons (et « Commander » / « Facturer »), inscrits d'un événement, échéances des contrats de travail, factures en retard. Lecture seule, sauf la transformation qui crée un brouillon. |
| **Accueil**, **Réglages**, **Assistant** | Tableau de bord ; adresse de Dokos et clés API ; chatbot Open WebUI qui qualifie la demande et pré-remplit Ventes, Formulaires ou Export. |

Guide complet : `docs/guide-utilisateur.md`. Droits des utilisateurs API :
`docs/utilisateurs-api.md`.

`Ctrl+K` cherche une action, un client ou un article ; `Ctrl+1` à `Ctrl+7`
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
│   formulaire.rs, evenement.rs, inscription.rs   formulaire web d'inscription, événement, inscrits
│   article.rs, clientele.rs groupes d'articles, prix, articles désactivés ; clients
│   brouillons.rs, suivi.rs, transformation.rs   brouillons, retards, échéances de contrats, devis → commande → facture
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
   | `ventes` | clients, articles, devis, commandes, factures, export | lecture clients/articles, lecture et création des documents de vente, **sans** validation |
   | `evenements` | événements, formulaires web, inscrits | Website Manager, création d'événements |
   | `rh` (facultatif) | échéances de contrats, export des employés | lecture seule |

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
(Ventes, Export, Formulaires). On relit, puis on crée comme d'habitude. Sa
référence est `docs/guide-utilisateur.md`, joint à la compilation ; il ne voit
pas les données de Dokos. Il ne remplit jamais de prix.

- **Réglages › Assistant IA** : adresse (par défaut `http://mac-studio:8080`,
  nom Tailscale), modèle (par défaut `qwen3.6-35b-a3b`), clé API Open WebUI
  (gardée dans le trousseau) et bases de connaissance (`prestations`,
  `evenements` : noms d'attente à remplacer par les identifiants réels).
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

dokos evenement creer --titre "Atelier poterie" --debut "14/12/2026 14h" --fin "14/12/2026 17h" --places 12 --question age! --question allergies --simulation
dokos evenement publier EV00042

dokos articles arbre
dokos articles lister Restauration
dokos modele factures
dokos importer factures "Documents\Dokos\modeles\modele-factures.xlsx" --simulation
dokos importer clients clients.xlsx --rapport resultat.xlsx
dokos exporter factures --statut impayee --du 01/10/2026 --au 31/10/2026
dokos exporter clients --groupe Collectivités --sortie clients.xlsx
dokos brouillons
dokos devis commander SAL-QTN-2026-00012
dokos commande facturer SAL-ORD-2026-00031
dokos facture retards
dokos evenement inscrits EV00042 --sortie inscrits.csv
dokos contrat alertes
dokos diagnostic
```

`--ligne ARTICLE=QUANTITÉ[*PRIX][@DATE]` : `*PRIX` est le prix unitaire HT de
la ligne (vide : prix de Dokos), `@DATE` la livraison d'une ligne de commande.
`--simulation` n'écrit rien. `--autoriser-doublon` force la création malgré un doublon.

## Contrôles

- **Doublons** : devis, commande, facture (même client, même date, mêmes lignes),
  événement (même titre, même début), article (même code ou libellé), client
  (même nom). Un doublon n'est jamais recréé sans `--autoriser-doublon`.
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
| Facture | `Sales Invoice` | `customer`, `posting_date`, `set_posting_time`, `due_date`, `items[].rate` |
| Événement | `Event` (+ bookings) | `subject`, `starts_on`, `published`, `allow_registrations`, `registration_form`, `max_number_of_registrations` |
| Formulaire | `Web Form` → `Event Registration` | `first_name`, `last_name`, `email`, `mobile_number` + vos champs |
| Contrat (lecture) | `Employment Contract` (hrms) | `employee`, `contract_type`, `date_of_joining`, `relieving_date` |

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
