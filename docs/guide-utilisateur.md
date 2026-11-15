# Guide de l'utilisateur — Dokos Asso

Dokos Asso prépare des documents dans Dokos sans jamais les valider à votre
place : tout ce qu'il crée est un **brouillon** (ou un événement **non
publié**) que vous relisez et validez dans Dokos. Il refuse de créer quand il
n'est pas sûr : client ou article ambigu, doublon, ligne d'import à corriger.

## Premier démarrage

1. Ouvrez **Réglages**. Renseignez l'adresse de votre Dokos
   (`https://…`, sans chemin) et la société si vous en avez plusieurs.
2. Pour chaque usage (Ventes, Événements, RH, Comptabilité), collez la clé et le secret de
   l'utilisateur API correspondant (voir `docs/utilisateurs-api.md`), puis
   **Enregistrer** : l'appli teste la connexion avant de garder quoi que ce soit.
   Le secret va dans le Gestionnaire d'identification Windows, jamais dans un
   fichier. Le profil RH ne sert qu'à exporter les employés, le profil
   Comptabilité qu'à exporter le grand livre : tous deux sont facultatifs et en
   lecture seule.
3. Assistant IA : laissez la case décochée si vous n'avez pas de serveur Open
   WebUI. Sinon, adresse Tailscale, modèle, clé, puis **Enregistrer et tester**.

## L'écran

- **Menu de gauche** : les onglets, rangés par usage. **Accueil** ; *Créer* :
  **Ventes**, **Formulaires** ; *En nombre* : **Import**, **Export** ;
  *Suivre* : **Suivi** ; en bas, l'état de la connexion à Dokos, l'**Assistant**
  et les **Réglages**. Le menu se réduit aux pictogrammes (bouton en haut du
  menu, ou d'office en fenêtre étroite). Un onglet grisé attend sa clé API dans
  Réglages ; le chiffre à côté de Suivi est le nombre de brouillons en attente.
- **Accueil** : ce qui attend (brouillons à valider, factures en retard) — un
  clic ouvre le bloc correspondant de Suivi —, les créations en un clic (devis, commande, facture, événement,
  import, export) et les derniers brouillons.
- **Listes déroulantes** : partout où l'on choisit un élément de Dokos (client,
  article, groupe, territoire), le champ s'ouvre au clic ou avec la flèche et
  montre **toutes** les valeurs ; taper quelques lettres filtre la liste (sans
  tenir compte des accents ni de la casse). Flèches et Entrée au clavier,
  Échap pour fermer. La liste est lue une fois dans Dokos puis gardée deux
  minutes.
- **Rechercher** (`Ctrl+K`) : tapez une action (« facture », « brouillons »,
  « thème sombre »…) ou le nom d'un client ou d'un article. Entrée sur un
  client ouvre Ventes avec ce client ; sur un article, l'ajoute à la vente en
  cours ; « Demander à l'assistant » envoie le texte au chatbot.
- **Raccourcis** : `Ctrl+1` à `Ctrl+7` changent d'onglet (ordre du menu),
  `Ctrl+Entrée` lance **Vérifier** (ou l'analyse, ou l'export) de l'onglet
  affiché — jamais une création —, `Ctrl+J` affiche ou masque l'assistant,
  `Ctrl+B` réduit le menu.
- Les chiffres de l'Accueil et de Suivi sont relus dans Dokos à l'ouverture,
  après chaque création, quand ils ont plus de deux minutes, ou avec
  **Actualiser**.

## Ventes

- Choisissez **Devis**, **Commande** ou **Facture**.
- **Client** : ouvrez la liste et choisissez, ou tapez quelques lettres pour la
  filtrer. Rien n'est créé si vous ne choisissez pas.
- **Lignes** : un article (liste déroulante), une quantité et, si vous le
  voulez, un **prix unitaire HT**.
  - **Prix vide** : Dokos applique ses listes de prix. Le prix indicatif de
    l'article s'affiche en grisé dans le champ.
  - **Prix saisi** (0 : offert) : c'est le prix de la ligne. Dès qu'un prix est
    saisi dans un document, les règles de prix de Dokos (remises automatiques)
    ne s'appliquent plus à aucune de ses lignes ; les lignes sans prix gardent
    celui de la liste de prix. Les taxes restent calculées par Dokos.
- **Catalogue** : « Parcourir le catalogue » montre les groupes d'articles de
  Dokos. Cliquez un groupe, puis « + Ajouter » sur un article. Quand un groupe
  est sélectionné, la liste des articles se limite à ce groupe.
- **Référence** (commande, facture) : numéro de bon de commande ou de lot,
  reporté dans « N° de commande client ».
- La **barre du bas** reste visible : elle résume le document (type, client,
  nombre de lignes, total hors taxes **indicatif** : prix saisis, sinon prix
  indicatifs) et porte **Vérifier** et **Créer**. Le total réel est calculé par
  Dokos.
- **Vérifier** montre les doublons (même client, même date, mêmes lignes) et
  les autres documents de ce client aux mêmes dates.
- **Créer en brouillon** ne crée rien si un document identique existe ; un
  bouton « Créer quand même » apparaît alors. Après la création, **Nouveau
  document** vide le formulaire pour le suivant.
- Une commande livrée avant aujourd'hui est signalée : Dokos la refusera.

### À partir d'un modèle

Un devis, une commande ou une facture de Dokos qui porte l'étiquette
**« Template »** sert de modèle pour les documents du même type.

1. Choisissez **Devis**, **Commande** ou **Facture**, puis **À partir d'un
   modèle**, et le modèle dans la liste.
2. Choisissez le **client**, la **date d'arrivée** et la **date de départ**
   (reportées dans les champs « custom_date_arrivee » et « custom_date_depart »).
3. Ajustez les **quantités** : elles sont reprises du modèle ; videz-en une ou
   mettez 0 pour retirer l'article.
4. **Vérifier**, puis **Créer en brouillon**.

Le document est une copie du modèle : prix et remises des lignes, taxes,
conditions de paiement, conditions générales et champs du modèle sont repris
tels quels (les règles de prix ne s'appliquent pas, et il n'y a pas de prix à
saisir). Sa date est celle du jour ; la date de livraison d'une commande est la
date de départ. Le modèle n'est jamais modifié, et les modèles ne comptent ni
comme doublons, ni dans les exports, ni dans « Mes brouillons ».

### Gérer le catalogue

Dans le panneau Catalogue, « Gérer le catalogue » permet de créer un article
(**désactivé**, à activer après relecture), un sous-groupe, ou de déplacer un
article (choisi dans la liste, ou son code tapé). Un code ou un libellé déjà
pris n'est jamais recréé.

## Formulaires

Événement public et formulaire d'inscription, créés **non publiés** ; le
bouton « Publier sur le site » est séparé. Les questions possibles sont les
champs existants d'« Event Registration » dans Dokos.

## Import

Quatre cas d'usage : **Factures**, **Paiements**, **Articles** et **Clients**.

1. Choisissez le **cas d'usage** et **téléchargez le modèle**. Il est généré
   depuis votre Dokos : les listes déroulantes du classeur contiennent vos
   clients, articles, groupes, territoires… Régénérez-le si Dokos change.
2. Remplissez l'onglet **Données** à partir de la ligne 3 (la ligne 2 est un
   exemple). Le mode d'emploi est dans le premier onglet du classeur.
3. **Choisir un fichier…** : l'appli analyse chaque ligne et affiche un statut :
   prêt, à préciser (plusieurs correspondances : cliquez la bonne),
   introuvable, invalide. Elle cherche aussi les doublons.
4. **Créer dans Dokos** : tout ou rien par défaut. Cochez « seulement les
   documents prêts » pour créer malgré des lignes à corriger. Un fichier
   résultat (`<nom>.resultat.xlsx`) reprend chaque ligne avec son statut.

- **Factures** : une ligne par article ; les lignes qui partagent le même
  **Lot** forment une seule facture. Les factures sont des brouillons.
- **Paiements** : une ligne par paiement reçu en règlement d'une **facture déjà
  validée** dans Dokos. Colonnes : **Facture** (liste des factures dont le solde
  n'est pas nul), **Date du paiement**, **Montant** (facultatif : vide, tout le
  reste dû), **Mode de paiement** (liste des modes actifs de Dokos) et
  **Référence** (numéro de chèque, de virement…). Dokos prépare l'écriture de
  paiement (client, comptes, lien vers la facture) et le mode de paiement
  détermine le compte d'encaissement ; la référence est **obligatoire** quand ce
  compte est bancaire. Plusieurs lignes pour une même facture (règlement en
  plusieurs fois) ne peuvent pas dépasser ensemble son reste dû. Chaque paiement
  est créé en **brouillon** : la facture n'est soldée qu'une fois le paiement
  validé dans Dokos. Le journal comptable est attribué par Dokos.
- **Articles** : créés **désactivés**, à activer depuis le catalogue ou dans
  Dokos. Un code ou un libellé déjà présent n'est pas recréé.
- **Clients** : un client de même nom n'est pas recréé ; contacts et adresses
  détaillés se saisissent ensuite dans Dokos.

**Prix unitaire HT** (factures) : colonne facultative. Vide, Dokos applique sa
liste de prix ; rempli, ce prix est celui de la ligne (0 : offert). Dès qu'un
prix est saisi dans un document, les règles de prix de Dokos (remises
automatiques) ne s'appliquent à aucune de ses lignes. Les taxes restent
calculées par Dokos.

**Tarif du client** (factures, colonne facultative Oui/Non) : à « Oui », le
prix unitaire saisi est aussi enregistré dans la liste de prix de Dokos comme
prix de cet article **pour ce client seulement** — dans la liste de prix du
client s'il en a une, sinon dans la liste de vente par défaut. Un prix déjà
enregistré pour ce client est remplacé ; le prix des autres clients ne change
pas. C'est la seule écriture de l'import qui ne soit pas un brouillon : elle a
lieu à la création du document, et le fichier résultat l'indique.

Règles : un nom approximatif est une ambiguïté, jamais une devinette ;
2 000 lignes et 5 Mo au plus ; les classeurs à macros sont refusés.

## Export

Un fichier Excel de vos **clients**, **articles**, **devis**, **commandes**,
**factures** ou **employés**, lu dans Dokos sans rien y modifier.

- **Avec l'assistant** (panneau de droite) : écrivez par exemple « la liste des
  clients du groupe Collectivités », « les articles vendables du groupe Repas »
  ou « les factures impayées de la mairie de Crest en octobre », puis
  **Pré-remplir dans Export**. Quand la liste et chaque filtre sont retrouvés
  dans Dokos, le fichier est créé tout de suite ; sinon l'assistant pose une
  question (client ambigu, groupe inconnu…) et vous complétez les filtres.
- **Sans l'assistant** : choisissez la liste, les filtres, puis **Créer le
  fichier Excel**.

Filtres selon la liste, tous en listes déroulantes quand ils désignent un
élément de Dokos : client (devis, commandes, factures), groupe (clients,
articles, sous-groupes compris), territoire (clients), période (date du
document, de facture ou d'entrée), statut (par exemple impayées, en retard,
payées), texte contenu dans le nom. Par défaut, les éléments désactivés,
annulés ou partis sont exclus ; une case permet de les inclure.

Le fichier est enregistré dans `Documents\Dokos\exports\<liste>-<date>.xlsx`
(un nouveau fichier à chaque export), avec les dates et montants au format
Excel, un filtre sur l'en-tête, et une feuille « À propos » qui rappelle les
filtres. 10 000 lignes au plus : au-delà, précisez les filtres. Les employés
demandent la clé du profil RH, les autres listes celle du profil Ventes.

### Grand livre

Le bouton **Grand livre**, à côté des listes, exporte les écritures comptables
d'un journal sur une période, **directement au format d'import** : il remplace
l'export du rapport « Grand livre » de Dokos suivi de la macro de mise en forme.
Il demande la clé du profil Comptabilité.

1. **Journal comptable (Dokos)** : liste déroulante des journaux actifs de Dokos
   (code, nom, type). Le dernier journal choisi est retenu.
2. **Journal du fichier** : `VEN` (par défaut) ou `REC`, écrit dans la colonne
   JOURNAL de toutes les lignes. Le dernier choix est retenu.
3. **Du** et **Au** : dates de comptabilisation, comprises ; le mois précédent
   est proposé.
4. **Créer le fichier Excel**.

Le fichier `Documents\Dokos\exports\grand-livre-<journal>-<du>-<au>.xlsx` a une
feuille, une ligne par écriture, et les colonnes :

| Colonne | Contenu |
|---|---|
| DATE | date de comptabilisation (`jj/mm/aaaa`) |
| JOURNAL | `VEN` ou `REC`, au choix, quel que soit le journal de Dokos exporté |
| GENERAL | chiffres en tête du compte (`411 - Clients - SLA` → 411) |
| AUXILIAIRE | `ERP` sur le compte 411, vide ailleurs |
| DEBIT, CREDIT | montants (0 si vide) |
| LIBELLE | tiers |
| REFINTERNE | numéro de la pièce (document de référence) |

Les écritures annulées et les à-nouveaux sont exclus, et les comptes de tiers
(clients, fournisseurs) sont en valeurs nettes, comme dans le rapport réglé de
la même façon. Une écriture dont le compte n'a pas de numéro est écartée et
signalée après l'export ; au-delà de 100 000 écritures, l'export est refusé :
réduisez la période. Rien n'est modifié dans Dokos.

## Suivi

Un seul bouton **Actualiser**, en haut, relit tout ; l'heure de la dernière
lecture est affichée à côté.

- **Mes brouillons** : devis, commandes, factures et événements créés par
  l'outil qui attendent une validation ou une publication, avec lien vers
  Dokos. « Commander » (sur un devis validé) et « Facturer » (sur une commande
  validée) créent le document suivant en brouillon.
- **Inscrits d'un événement** : liste et export Excel (prénom, nom, e-mail,
  téléphone, réponses aux questions).
- **Factures en retard** : échéance dépassée et solde non nul, par client. La
  relance se fait dans Dokos.

## Assistant IA

Le panneau **Assistant**, à droite de chaque onglet, est une discussion. Il
peut aussi **chercher un client ou un article** dans Dokos (« le téléphone de
la mairie de Crest », « avez-vous une salle de réunion ? ») : les fiches
trouvées s'affichent avec toutes leurs informations, un lien vers Dokos et un
bouton **Utiliser dans Ventes** (client) ou **Ajouter à une vente** (article).
Le stock n'est pas affiché.

1. Écrivez ce que vous voulez faire (« faire un devis », « importer des
   factures », « la liste des clients »…), ou cliquez une suggestion. **Entrée**
   envoie, **Maj + Entrée** va à la ligne.
2. L'assistant pose une question si la demande est floue (devis ou facture ?
   quel client ? quelles dates ?), indique l'onglet où la faire et explique la
   procédure étape par étape.
3. Quand il a l'essentiel, il propose **Pré-remplir dans …** : l'onglet s'ouvre
   et son formulaire se remplit (Ventes, Export, Formulaires). Les clients,
   articles et groupes sont recherchés dans Dokos ; s'il y a plusieurs
   candidats, les boutons de choix apparaissent dans la discussion. Il ne
   remplit jamais de prix : saisissez-les vous-même.
4. Relisez le formulaire, puis créez vous-même : l'assistant ne crée et ne
   valide rien, et il ne voit pas vos données Dokos (pour les consulter :
   Export ou Suivi).

**↺** recommence une discussion, **×**, le bouton **Assistant** du menu ou
`Ctrl+J` masque le panneau. Les messages partent vers votre serveur Open WebUI ; si
l'assistant est désactivé dans Réglages, le panneau le signale et les
formulaires restent utilisables à la main.

## En cas de problème

- Une alerte rouge propose **Réessayer** (Dokos injoignable) ou **Ouvrir
  Réglages** (clé refusée, droit manquant, réglage à revoir).
- **Réglages › Diagnostic › Préparer le diagnostic**, puis **Copier** : versions,
  réglages sans secret et dernières lignes du journal, à coller dans votre
  demande d'aide. Les journaux sont dans `%APPDATA%\dokos-cli-asso\logs` (30 jours).
- Ce que l'outil a créé est noté dans `Documents\Dokos\journal-actions.csv`.
