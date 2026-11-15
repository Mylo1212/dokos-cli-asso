# dokos-assistant

Créer devis, commandes, factures, formulaires d'inscription et contrats de
travail dans Dokos, par son API REST (Frappe), sans Server Script.

Règle de conception : **l'outil propose, Rust vérifie, l'utilisateur valide, Dokos enregistre.**

## État

| Étape | Contenu | État |
|---|---|---|
| 1 | `dokos-core`, `conformite`, ligne de commande `dokos` | fait |
| 2 | Appli Tauri « Dokos Assistant » : Ventes, Formulaires, Contrats, Réglages | fait, sans IA |
| 3 | IA via Open WebUI : pré-remplissage, questions, rédaction des mentions | fait |
| 4 | Suite de l'audit : catalogue, import Excel, suivi, journal, erreurs structurées | fait (voir `CHANGELOG.md`) |
| 5 | Listes Excel (clients, articles, devis, commandes, factures, employés), demandées à l'IA ou au formulaire | fait |
| 6 | Chatbot à droite de chaque onglet : qualifie la demande, oriente vers l'onglet, explique, pré-remplit | fait |
| 7 | Fiches clients et articles dans le chatbot ; devis, commandes, factures à partir d'un modèle « Template » (Ventes et Import) | fait |
| 8 | Performance (mémoire des structures, lectures groupées, compression) ; menu latéral, accueil, palette `Ctrl+K`, barre d'action (voir `docs/propositions-2026-11.md`) | fait |

Tout est testé sans réseau (faux transports Dokos et Open WebUI). Rien n'a
encore été essayé contre une vraie instance Dokos ni contre votre Open WebUI :
commencez par une instance de test.

## Arborescence

```
index.html, src/             interface TypeScript (Vite), sans framework
src-tauri/                   appli Tauri + racine du workspace (resolver 3, MSRV 1.90)
├─ src/                      commandes Tauri (capacité vide), services testables
├─ dokos-core/               API Dokos, sans Tauri ni IA
│   transport.rs, http.rs    trait Transport (lots de lectures), faux transport, HTTPS (rustls/ring, gzip)
│   client.rs, meta.rs       /api/resource, /api/method, structure des DocTypes (gardée 2 min)
│   vente.rs, doublon.rs     devis, commandes, factures en brouillon + doublons
│   formulaire.rs            formulaire web d'inscription (Event Registration)
│   evenement.rs             événement non publié, puis publication
│   employe.rs, contrat.rs   employés, contrats de travail
│   impression.rs            formats d'impression (lecture seule), PDF
│   article.rs               groupes d'articles (arbre), prix indicatifs, articles créés désactivés
│   clientele.rs, inscription.rs   clients, inscriptions et liste des inscrits
│   brouillons.rs, suivi.rs  brouillons en attente, factures en retard, échéances de contrats
│   transformation.rs        devis → commande → facture, par les méthodes de Dokos
│   export.rs                listes exportables : colonnes et filtres fixes, pagination, lecture seule
│   modele_vente.rs          documents « Template » : liste, lecture, copie avec client, dates, quantités
│   fiche.rs                 fiches clients et articles détaillées (chatbot), lecture seule
├─ dokos-import/             modèles Excel générés depuis Dokos, analyse, création tout ou rien
├─ assistant-core/           IA : client Open WebUI, extractions, propositions vérifiées
│   adresse.rs               http:// seulement vers Tailscale ou la machine locale
│   owui.rs                  /api/chat/completions, bases de connaissance, JSON strict
│   ventes.rs, evenements.rs, contrats.rs   demande en texte → formulaire pré-rempli
│   formats.rs               rédaction des mentions manquantes, bloc par bloc
│   listes.rs                « les factures impayées d'octobre » → liste et filtres vérifiés
│   conversation.rs          chatbot : qualification, onglet, procédure (guide joint), pré-remplissage
├─ conformite/               mentions obligatoires, analyse des gabarits Jinja
│   mentions-contrats.toml   liste livrée — BROUILLON À FAIRE VALIDER
├─ dokos-config/             config.toml, profils, trousseau, journal et diagnostic
└─ dokos-cli/                binaire `dokos`
```

## Mise en place

1. Dans Dokos, créez **un utilisateur API par profil**, chacun avec le rôle minimal :

   | Profil | Sert à | Rôles conseillés |
   |---|---|---|
   | `ventes` | clients, articles, devis, commandes, factures | lecture clients/articles, lecture et création des documents de vente, **sans** validation |
   | `evenements` | événements, formulaires web | Website Manager, création d'événements |
   | `rh` | employés, contrats, formats d'impression (lecture) | HR User |

2. Renseignez l'adresse, la société et les clés : onglet **Réglages** de l'appli,
   ou `dokos connexion enregistrer --profil ventes --url https://… --cle … --societe "…"`.
   Les secrets vont dans le Gestionnaire d'identification Windows (service
   `dokos-cli`) ; `%APPDATA%\dokos-cli\config.toml` ne contient que l'adresse et les clés.

3. Pour les contrats, créez sur `Employment Contract` les champs utilisés par la
   liste des mentions : `remuneration_brute`, `lieu_travail`, `periode_essai`,
   `motif_cdd`. L'outil ne crée jamais de champ.

4. Faites **relire la liste des mentions** : `dokos contrat regles --exporter`,
   puis renseignez `valide_par` et `valide_le`. Tant que ce n'est pas fait, chaque
   rapport affiche « liste non validée ».

## L'appli

```powershell
npm ci
npm run tauri dev      # développement
npm run tauri build    # installateur Windows (NSIS, MSI)
```

Le menu de gauche range les onglets par usage (créer, traiter en nombre,
suivre) ; `Ctrl+K` cherche une action, un client ou un article ; `Ctrl+1` à
`Ctrl+8` changent d'onglet, `Ctrl+Entrée` vérifie, `Ctrl+J` montre l'assistant.

- **Accueil** : brouillons en attente, factures en retard, échéances de
  contrats, créations en un clic, derniers brouillons.
- **Ventes** : devis, commande ou facture. Client et articles choisis dans les
  suggestions de Dokos, aucun prix saisi. « Vérifier » montre les doublons ;
  « Créer en brouillon » ne crée rien si un document identique existe. La barre
  du bas résume le document, avec un total hors taxes indicatif.
- **Formulaires** : un événement et son formulaire d'inscription. Les questions
  possibles sont les champs existants d'`Event Registration`. Création **non
  publiée**, puis bouton « Publier sur le site » séparé.
- **Contrats** : employé (existant ou nouveau), contrat vérifié avant création
  (rien n'est créé s'il reste une erreur), PDF, et contrôle d'un format d'impression
  avec proposition de correction enregistrée dans `Documents\Dokos`.

- **Catalogue** (dans Ventes) : arbre des groupes d'articles, prix indicatif,
  ajout d'une ligne en un clic ; création d'article désactivé puis activation.
- **Import** : un modèle Excel par cas d'usage (factures, commandes, devis,
  inscriptions, articles, employés, clients, programme d'événements), généré
  depuis votre Dokos avec ses listes déroulantes. Analyse ligne par ligne,
  ambiguïtés tranchées dans l'appli, doublons, création tout ou rien, rapport.
- **Modèles « Template »** (Ventes et Import) : un devis, une commande ou une
  facture étiqueté « Template » dans Dokos est copié tel quel (prix, taxes,
  conditions) ; on ne saisit que le client, les dates d'arrivée et de départ
  (`custom_date_arrivee`, `custom_date_depart`) et les quantités.
- **Listes** : clients, articles, devis, commandes, factures ou employés dans
  un fichier Excel (`Documents\Dokos\listes`), avec filtres (client, groupe,
  territoire, période, statut, nom). Demandez-la à l'assistant (« la liste des
  clients du groupe Collectivités ») ou remplissez les filtres. Lecture seule.
- **Suivi** : mes brouillons, « Commander » un devis et « Facturer » une
  commande, inscrits d'un événement, alertes RH, factures en retard.

Guide complet : `docs/guide-utilisateur.md`. Droits des utilisateurs API :
`docs/utilisateurs-api.md`. Performance, interface et suites proposées :
`docs/propositions-2026-11.md`.

La fenêtre n'a aucune permission Tauri : tout passe par les commandes Rust, qui
construisent seules les requêtes. Les liens ne s'ouvrent que vers votre Dokos,
les fichiers que dans `Documents\Dokos`.

## Assistant IA

Un panneau **Assistant**, à droite de chaque onglet, est un chatbot : il
qualifie la demande (une question à la fois), indique l'onglet où la faire,
explique la procédure et, quand il a l'essentiel, propose **Pré-remplir dans …**
(Ventes, Listes, Formulaires, Contrats). On relit, puis on crée comme
d'habitude. Sa référence est `docs/guide-utilisateur.md`, joint à la
compilation ; il ne voit pas les données de Dokos. Dans Contrats, « Rédiger avec
l'IA » complète un format d'impression.

- **Réglages › Assistant IA** : adresse (par défaut `http://mac-studio:8080`,
  nom Tailscale), modèle (par défaut `qwen3.6-35b-a3b`), clé API Open WebUI
  (gardée dans le trousseau) et bases de connaissance par domaine.
- **Bases de connaissance** : les valeurs par défaut (`prestations`,
  `evenements`, `convention-collective`) sont des noms d'attente. Remplacez-les
  par les identifiants réels, visibles dans l'adresse de la base dans Open WebUI.
  Une base introuvable est ignorée, avec un avertissement.
- **Réseau** : `http://` n'est accepté que vers une adresse Tailscale (nom court,
  `*.ts.net`, `100.64.0.0/10`) ou locale ; ailleurs, `https://` est obligatoire.
  Le texte des demandes (noms de clients, de salariés) part vers ce serveur.

Ce que l'IA peut et ne peut pas faire :

- Elle **lit seulement** : aucun outil d'écriture, aucune clé Dokos. Elle ne fait
  que pré-remplir ; la création reste un clic, avec toutes les vérifications.
- Clients, articles, employés, types de contrat et postes sont **recherchés dans
  Dokos** : ambigu → choix proposés, introuvable → question, jamais de création.
- Les questions du formulaire d'inscription et les champs du contrat sont pris
  **uniquement parmi les champs existants** de Dokos ; le reste est signalé « à créer ».
- Pour un format d'impression, elle ne renvoie que des **blocs**, un par mention
  manquante. Chaque bloc doit être sûr (pas de `frappe.db`, de script…), n'afficher
  que des champs connus et contenir la mention ; sinon le bloc de la liste relue
  est repris, avec la raison. Le fichier est enregistré dans `Documents\Dokos`.
- Si Open WebUI est injoignable (Mac éteint, Tailscale coupé), l'erreur s'affiche
  dans la zone Assistant et le reste de l'appli fonctionne normalement.

Test réel, contre votre serveur :

```powershell
$env:OWUI_URL = "http://mac-studio:8080"; $env:OWUI_CLE = "sk-…"; $env:OWUI_MODELE = "qwen3.6-35b-a3b"
cargo test -p assistant-core --test reel -- --ignored --nocapture
```

## Ligne de commande

```powershell
dokos connexion tester
dokos devis brouillon --client "Mairie de Crest" --date 12/11/2026 --ligne "repas=25" --simulation
dokos commande brouillon --client CUST-0002 --livraison 12/11/2026 --ligne "repas=25" --ligne "grande salle=1@13/11/2026"
dokos facture brouillon --client CUST-0002 --date 03/11/2026 --echeance 03/12/2026 --ligne "repas=25"

dokos evenement questions
dokos evenement creer --titre "Atelier poterie" --debut "14/12/2026 14h" --fin "14/12/2026 17h" --places 12 --question age! --question allergies --simulation
dokos evenement publier EV00042

dokos employe creer --prenom Marie --nom Dupont --genre Femme --naissance 02/04/1995 --entree 01/12/2026
dokos contrat creer --employe HR-EMP-00012 --type CDD --poste Animatrice --entree 01/12/2026 --fin 31/03/2027 `
    --heures 24 --repartition 6/6/6/6/0/0/0 --champ "motif_cdd=Accroissement temporaire d'activité" --format "Contrat CDD" --simulation
dokos contrat verifier HR-EMP-CONT-00003 --format "Contrat CDD"
dokos contrat pdf HR-EMP-CONT-00003 --format "Contrat CDD"
dokos format verifier "Contrat CDD" --type CDD --temps-partiel
dokos format proposer "Contrat CDD" --type CDD

dokos articles arbre
dokos articles lister Restauration
dokos modele factures
dokos importer factures "Documents\Dokos\modeles\modele-factures.xlsx" --simulation
dokos importer factures factures-octobre.xlsx --choix "client=Crest=CUST-2" --rapport resultat.xlsx
dokos brouillons
dokos devis commander SAL-QTN-2026-00012
dokos commande facturer SAL-ORD-2026-00031
dokos facture retards
dokos evenement inscrits EV00042 --sortie inscrits.csv
dokos contrat alertes
dokos diagnostic
```

`--simulation` n'écrit rien. `--autoriser-doublon` force la création malgré un doublon.

## Contrôles

- **Doublons** : devis, commande, facture (même client, même date, mêmes lignes),
  événement (même titre, même début), employé (prénom, nom, naissance),
  contrat (employé, type, date d'entrée). Un doublon n'est jamais recréé sans `--autoriser-doublon`.
- **Formats d'impression** : chaque mention applicable (selon CDI/CDD/temps
  partiel) doit être affichée par un champ existant dans Dokos ou par un texte
  attendu. Le gabarit est aussi contrôlé côté sécurité : `frappe.db`,
  `frappe.get_list`, `doc.db_set`, `{% include %}`… sont refusés.
- **Contrats** : CDD sans fin ou sans motif, temps partiel sans répartition ou
  dont la répartition ne correspond pas aux heures, plus de 48 h, plus de 10 h par
  jour, mentions dont la donnée est vide.

L'outil vérifie la présence et la cohérence des mentions ; **il ne garantit pas la
validité juridique d'un contrat.**

## Hypothèses à vérifier dans votre Dokos

Vérifiées dans le code de Dokos v5.18, de l'appli `bookings` et de l'appli `hrms` :

| Usage | DocType | Champs |
|---|---|---|
| Client, article | `Customer`, `Item` | `customer_name`, `item_name`, `disabled`, `is_sales_item`, `has_variants` |
| Devis | `Quotation` | `quotation_to`, `party_name`, `transaction_date`, `valid_till`, `items[]` |
| Commande | `Sales Order` | `customer`, `delivery_date`, `items[].delivery_date` |
| Facture | `Sales Invoice` | `customer`, `posting_date`, `set_posting_time`, `due_date` |
| Événement | `Event` (+ bookings) | `subject`, `starts_on`, `published`, `allow_registrations`, `registration_form`, `max_number_of_registrations` |
| Formulaire | `Web Form` → `Event Registration` | `first_name`, `last_name`, `email`, `mobile_number` + vos champs |
| Employé | `Employee` | `first_name`, `last_name`, `gender`, `date_of_birth`, `date_of_joining`, `company` |
| Contrat | `Employment Contract` (hrms) | `employee`, `contract_type`, `designation`, `date_of_joining`, `relieving_date`, `weekly_working_hours`, `monday`…`sunday` |

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
`%APPDATA%\dokos-cli\logs` ; actions : `Documents\Dokos\journal-actions.csv`.
