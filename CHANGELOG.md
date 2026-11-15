# Journal des versions

Format : une section par version, les changements regroupés par nature. Les
versions suivent `MAJEUR.MINEUR.CORRECTIF` ; un tag `vX.Y.Z` déclenche la
construction des installateurs (voir `.github/workflows/release.yml`).

## Non publié

### Performance, interface et utilisation

Détail, mesures et suites proposées : `docs/propositions-2026-11.md`.

- **Menu latéral** par usage (Accueil ; Créer ; En nombre ; Suivre), réductible,
  avec l'état de la connexion à Dokos, les onglets à configurer grisés et le
  nombre de brouillons en attente. Les noms des onglets ne changent pas.
- **Accueil** (nouvel écran d'ouverture) : brouillons en attente, factures en
  retard, fins de contrat, CDD sans date de fin, créations en un clic,
  derniers brouillons.
- **Palette de commandes** (`Ctrl+K`) : actions, clients et articles de Dokos,
  question à l'assistant. Raccourcis `Ctrl+1` à `Ctrl+8`, `Ctrl+Entrée`
  (vérifier, jamais créer), `Ctrl+J`, `Ctrl+B`. Thème clair, sombre ou système.
- **Ventes** : barre d'action toujours visible (type, client, lignes, total
  hors taxes indicatif, Vérifier, Créer), prix indicatif dès qu'un article est
  choisi, « Nouveau document » après une création.
- **Suivi** : un seul « Actualiser », heure de la dernière lecture, données
  partagées avec l'accueil et relues après chaque création ou au bout de deux
  minutes.
- **Moins de requêtes vers Dokos** : structure des DocTypes gardée deux
  minutes (relue à chaque « Vérifier » et rechargement), utilisateur API
  demandé une fois, recherches de clients et d'articles gardées 30 secondes.
- **Lectures groupées**, quatre de front au plus : brouillons, alertes RH.
  Une écriture n'est jamais parallèle.
- **Compression gzip** des réponses de Dokos ; connexions ouvertes et clés
  vérifiées dès le démarrage ; durée et taille de chaque requête au journal.
- Sections de l'interface construites à leur première visite.
- Corrigé : arbre du catalogue non indenté dans l'appli installée (style
  refusé par la politique de sécurité) ; assistant ouvert par-dessus la page
  en fenêtre étroite.

### Performances

- **Réponses compressées** (gzip) : les listes de Dokos (500 clients ou
  articles, exports) transitent plusieurs fois moins lourdes ; la connexion
  TLS reste ouverte 5 minutes entre deux actions.
- **Requêtes simultanées** (6 au plus) pour les lectures indépendantes : la
  vérification des doublons d'un import, les documents candidats d'un
  contrôle de doublon, les documents suivants d'une transformation, le
  catalogue (groupes, liste de prix, unités) et le modèle d'import des ventes
  (clients et articles). Un import de 100 factures n'attend plus 200 à 300
  allers-retours l'un après l'autre.
- **Mémoire de 5 minutes** pour la liste de prix par défaut, relue jusqu'ici
  à chaque action ; seules les réponses réussies sont gardées, et un
  changement de réglages vide tout. (La structure des DocTypes est gardée
  deux minutes, voir plus bas.)
- Catalogue : les prix déjà demandés ne le sont plus à chaque frappe dans le
  filtre (« Actualiser » les relit), et une réponse lente ne remplace plus
  une liste plus récente.
- Journal : durée et taille de chaque réponse (niveau debug), pour repérer
  les écrans lents.

### Nouveaux usages

- **Import des ventes : prix unitaire** : nouvelle colonne facultative
  « Prix unitaire HT » dans les modèles d'import de factures, commandes et
  devis. Vide : prix de la liste de prix de Dokos. Rempli : prix de la ligne,
  et les règles de prix de Dokos ne s'appliquent plus à ce document. Les
  fichiers sans cette colonne restent acceptés.
- **Import des ventes : tarif du client** : colonne facultative « Tarif du
  client » (Oui/Non). À « Oui », le prix saisi est aussi enregistré dans la
  liste de prix de Dokos pour cet article et ce client seulement (créé ou
  remplacé), dans la liste de prix du client ou, à défaut, la liste de vente
  par défaut.
- **Ventes à partir d'un modèle** : un devis, une commande ou une facture
  portant l'étiquette Dokos « Template » sert de modèle aux documents du même
  type. Dans Ventes (« À partir d'un modèle ») et dans Import (trois nouveaux
  cas), on ne saisit que le client, les dates d'arrivée et de départ
  (`custom_date_arrivee`, `custom_date_depart`) et les quantités ; une
  quantité vide ou nulle retire l'article. Tout le reste est copié tel quel :
  prix et remises (règles de prix ignorées), taxes, conditions, champs
  personnalisés. Date du document : le jour même ; livraison d'une commande :
  la date de départ. Le modèle est relu et sa version vérifiée avant chaque
  création ; un fichier d'import généré avant une modification du modèle est
  refusé. CLI : `dokos modele devis-modele --modele <identifiant>`.
- Les modèles sont exclus du contrôle de doublons, des listes Excel et de
  « Mes brouillons ».
- **Chatbot** : recherche de clients et d'articles dans Dokos (lecture seule,
  clé Ventes), fiches complètes affichées sous la réponse (coordonnées,
  adresse, TVA, conditions, prix indicatif…) avec « Utiliser dans Ventes » ou
  « Ajouter à une vente ». Les fiches restent dans l'historique de la
  discussion pour les questions suivantes.

- **Assistant en discussion** : la zone « Assistant » en tête des onglets est
  remplacée par un chatbot, dans un panneau à droite de chaque onglet. Il
  qualifie la demande (une question à la fois), indique l'onglet où la faire,
  explique la procédure étape par étape et propose « Pré-remplir dans … »
  (Ventes, Listes, Formulaires, Contrats) avec la demande reformulée à partir
  de toute la discussion. Sa référence est le guide de l'utilisateur, joint à
  la compilation ; il ne lit ni n'écrit rien dans Dokos. Onglet et action
  vérifiés par Rust, texte affiché sans HTML, conversation bornée (20 messages,
  24 000 caractères). Panneau masquable, superposé en fenêtre étroite.

- **Listes** (nouvel onglet) : clients, articles, devis, commandes, factures et
  employés dans un fichier Excel, en lecture seule. L'assistant IA comprend
  « la liste des clients du groupe Collectivités » ou « les factures impayées
  d'octobre » : il choisit une liste et des filtres d'un catalogue fixe (jamais
  un DocType ni un champ), Rust retrouve client, groupe et territoire dans
  Dokos, puis le fichier est créé tout de suite si rien n'est ambigu. Le
  formulaire marche aussi sans l'IA.
- Fichier `Documents\Dokos\listes\<liste>-<date>.xlsx` : dates et montants
  typés, statuts en français, filtre et en-tête figé, feuille « À propos »
  (filtres, date, avertissements). Pagination par 500, 10 000 lignes au plus ;
  colonnes absentes de l'instance écartées avec un avertissement.

### Corrections

- **Modèles d'import : tous les clients et articles** : la liste des clients
  (et celle des articles) jointe au modèle s'arrêtait aux 500 premiers par
  ordre alphabétique, vers la lettre F. Elles sont désormais lues page par
  page, jusqu'à 10 000 éléments.

- **Import : client « introuvable » alors qu'il est dans la liste du modèle** :
  l'import relançait une recherche dans Dokos, qui échouait pour certains noms
  (espaces doublés ou insécables, plus de dix clients au nom voisin). Clients
  et articles sont désormais d'abord cherchés dans les listes du modèle, relues
  à chaque import : une valeur de la liste déroulante est toujours reconnue,
  et l'analyse fait moins de requêtes.

### Construction

- Workflow « Setup Windows » : `setup.exe` (NSIS) construit à chaque push,
  en artefact et dans la pré-release « dernier-build ».
- `npm run tauri build` et `npm run tauri dev` marchent sans option : le
  workspace n'a plus de `default-members`, cargo vise l'appli par défaut.
- `npm run tout` : une seule commande pour tout vérifier et tester (interface,
  bibliothèques, ligne de commande, appli).

## 0.2.0 — 2026-11-03

Suite de l'audit (`docs/audit-2026-11.md`) : toutes les préconisations, sauf
celles qui demandent une instance Dokos réelle ou un cadrage métier (voir « Non
fait » en bas).

### Nouveaux usages

- **Catalogue** (onglet Ventes) : arbre des groupes d'articles, articles d'un
  groupe et de ses sous-groupes, prix indicatif de la liste de prix par défaut,
  ajout d'une ligne en un clic, recherche d'articles limitée au groupe choisi.
  Gestion : création d'article **désactivé** puis activation, sous-groupe,
  déplacement d'un article.
- **Import** (nouvel onglet) : un modèle Excel par cas d'usage, généré depuis
  le Dokos connecté (mode d'emploi, en-têtes, exemple, listes déroulantes
  alimentées par Dokos, marque cachée du cas et de la version). Cas :
  factures, commandes, devis, inscriptions à un événement, articles, employés,
  clients, programme d'événements. Analyse ligne par ligne, résolution exacte
  (identifiant ou nom), choix des ambiguïtés dans l'appli, doublons, création
  tout ou rien (ou « seulement les documents prêts »), fichier résultat.
- **Suivi** (nouvel onglet) : mes brouillons (documents de l'outil en attente),
  « Commander » un devis et « Facturer » une commande (brouillons créés à
  partir des méthodes de Dokos), inscrits d'un événement avec export Excel,
  alertes RH (fins de contrat sous 30 jours, CDD sans fin, contrats récents),
  factures en retard par client.
- **Référence externe** sur commandes et factures (champ standard « N° de
  commande client »).
- **Journal des actions** : `Documents\Dokos\journal-actions.csv` (horodatage,
  profil, action, document, résultat).
- **Ligne de commande** : `articles arbre|lister|creer|activer`, `modele`,
  `importer`, `brouillons`, `devis commander`, `commande facturer`,
  `facture retards`, `evenement inscrits`, `contrat alertes`, `diagnostic`.

### Améliorations

- Erreurs **structurées** (genre + message) : l'interface propose « Réessayer »
  (réseau) ou « Ouvrir Réglages » (clé, droit, configuration, IA).
- **Connexions gardées en mémoire** par profil : plus de relecture du trousseau
  ni de création d'un client HTTPS à chaque recherche.
- **Journal technique** tournant (30 jours, sans secret ni donnée personnelle)
  dans `%APPDATA%\dokos-cli\logs`, et **diagnostic copiable** dans Réglages.
- Réglage **« Assistant IA activé »** : désactivé, aucune zone Assistant et
  aucun appel à Open WebUI. Réglages de l'IA enregistrés même si le test échoue,
  avec un message explicite.
- Rappel quand la liste des mentions a été **validée il y a plus d'un an**.
- Avertissement quand une commande est livrée **avant aujourd'hui**.
- Délai plus long pour les téléchargements de PDF et les transformations.
- Configuration tolérante aux champs d'une version plus récente, tout en
  refusant tout nom de champ qui ressemble à un secret.
- L'assistant IA reçoit les groupes d'articles pour mieux comprendre les
  désignations.
- Suggestions : navigation complète au clavier, un seul écouteur global (plus
  de fuite), erreurs d'ouverture de lien affichées en ligne.
- Onglet Contrats découpé en modules ; fonctions de saisie pures testées avec
  **Vitest** (12 tests) ; tests de service pour événements et RH.
- CI : tests frontend, Prettier, **cargo audit** et **npm audit**
  hebdomadaires ; workflow de **release** sur tag (Windows et macOS).
- Documentation : `docs/guide-utilisateur.md`, `docs/utilisateurs-api.md`,
  scénario réel de création puis suppression d'un devis et contrôle des
  hypothèses sur les DocTypes (`cargo test -p dokos-core --test reel -- --ignored`).

### Non fait, et pourquoi

- Contrôle de doublon en une seule requête (lignes dans la liste) : à valider
  sur une vraie instance avant de changer une vérification qui fonctionne.
- Envoi de PDF par e-mail et saisie de règlements : droits supplémentaires et
  cadrage comptable à faire d'abord.
- Avenants de contrat : après validation de la liste des mentions.
- Signature des installateurs : demande des certificats (secrets du dépôt).

## 0.1.0 — 2026-10

Première version : ventes, formulaires, contrats, assistant IA, ligne de commande.
