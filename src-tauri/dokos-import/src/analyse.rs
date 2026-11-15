//! Analyse d'un tableau lu : chaque ligne devient une demande validée par les
//! types de `dokos-core`, chaque valeur nommée est résolue dans Dokos
//! (identifiant ou nom exact, jamais de devinette), puis les documents
//! prêts sont vérifiés (doublons) et créés.

use std::collections::BTreeMap;

use dokos_core::article::{
    ArticleExistant, CreationArticle, NouvelArticle, PrixClient, chercher_article_identique, creer_article,
    enregistrer_prix_client,
};
use dokos_core::client::Dokos;
use dokos_core::clientele::{CreationClient, NouveauClient, TypeClient, chercher_client_identique, creer_client};
use dokos_core::date::Date;
use dokos_core::doublon::{SiDoublon, chercher_doublons};
use dokos_core::erreur::DokosError;
use dokos_core::paiement::{
    CompteEncaissement, CreationPaiement, DOCTYPE_PAIEMENT, NouveauPaiement, chercher_paiement_identique,
    compte_du_mode, creer_paiement,
};
use dokos_core::recherche::{Candidat as _, Correspondance, normaliser, trouver_article, trouver_client};
use dokos_core::transport::Transport;
use dokos_core::types::{DOCTYPE_ARTICLE, DOCTYPE_CLIENT};
use dokos_core::vente::{Creation, LigneVente, NouveauDocumentVente, TypeVente, creer_brouillon};

use crate::ErreurImport;
use crate::cas::Cas;
use crate::lecture::{Correspondance as Colonnes, Tableau, associer_colonnes};
use crate::referentiel::{
    LISTE_ARTICLES, LISTE_CLIENTS, LISTE_GROUPES_ARTICLES, LISTE_GROUPES_CLIENTS, LISTE_MODES_PAIEMENT,
    LISTE_TERRITOIRES, LISTE_UNITES, Referentiel,
};

/// Réglages de l'analyse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Société Dokos, utile seulement s'il y en a plusieurs.
    pub societe: Option<String>,
    /// Choix de l'utilisateur pour les valeurs ambiguës : `cle_choix(champ, texte)` → identifiant.
    pub choix: BTreeMap<String, String>,
}

/// Clé d'un choix d'ambiguïté.
pub fn cle_choix(champ: &str, texte: &str) -> String {
    format!("{champ}|{}", normaliser(texte))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidat {
    pub id: String,
    pub libelle: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statut {
    Pret,
    /// Plusieurs éléments de Dokos correspondent : l'utilisateur choisit.
    APreciser {
        champ: String,
        texte: String,
        candidats: Vec<Candidat>,
    },
    Introuvable {
        champ: String,
        texte: String,
    },
    Invalide(String),
}

impl Statut {
    pub fn est_pret(&self) -> bool {
        matches!(self, Self::Pret)
    }

    pub fn libelle(&self) -> &'static str {
        match self {
            Self::Pret => "prêt",
            Self::APreciser { .. } => "à préciser",
            Self::Introuvable { .. } => "introuvable",
            Self::Invalide(_) => "invalide",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Pret => String::new(),
            Self::APreciser { champ, texte, candidats } => format!(
                "{champ} « {texte} » : plusieurs correspondances ({})",
                candidats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(", ")
            ),
            Self::Introuvable { champ, texte } => format!("{champ} « {texte} » introuvable dans Dokos"),
            Self::Invalide(m) => m.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LigneAnalysee {
    /// Numéro de ligne dans le fichier.
    pub numero: u32,
    pub valeurs: Vec<String>,
    pub statut: Statut,
    /// Indice du document dans [`Analyse::documents`].
    pub document: Option<usize>,
}

/// Demande validée, prête à être créée.
#[derive(Debug, Clone, PartialEq)]
pub enum Prepare {
    Vente(NouveauDocumentVente),
    Article(NouvelArticle),
    Client(NouveauClient),
    Paiement(NouveauPaiement),
}

impl Prepare {
    pub fn doctype(&self) -> &'static str {
        match self {
            Self::Vente(v) => v.type_vente().doctype(),
            Self::Article(_) => DOCTYPE_ARTICLE,
            Self::Client(_) => DOCTYPE_CLIENT,
            Self::Paiement(_) => DOCTYPE_PAIEMENT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existant {
    pub doctype: String,
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentPrepare {
    /// Lot, ou numéro de ligne.
    pub cle: String,
    pub libelle: String,
    /// Indices dans [`Analyse::lignes`].
    pub lignes: Vec<usize>,
    /// `None` si une de ses lignes bloque.
    pub prepare: Option<Prepare>,
    /// Document identique déjà dans Dokos, après [`verifier`].
    pub doublon: Option<Existant>,
}

impl DocumentPrepare {
    pub fn pret(&self) -> bool {
        self.prepare.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analyse {
    pub cas: Cas,
    pub entetes: Vec<String>,
    pub avertissements: Vec<String>,
    pub lignes: Vec<LigneAnalysee>,
    pub documents: Vec<DocumentPrepare>,
}

impl Analyse {
    pub fn documents_prets(&self) -> usize {
        self.documents.iter().filter(|d| d.pret()).count()
    }

    pub fn lignes_bloquantes(&self) -> usize {
        self.lignes.iter().filter(|l| !l.statut.est_pret()).count()
    }

    pub fn doublons(&self) -> usize {
        self.documents.iter().filter(|d| d.doublon.is_some()).count()
    }
}

// --- Résolution dans Dokos ---------------------------------------------------

enum Resolution {
    Trouve(Candidat),
    Plusieurs(Vec<Candidat>),
    Aucune,
}

struct Resolveur<'a, T> {
    dokos: &'a Dokos<T>,
    choix: &'a BTreeMap<String, String>,
    /// Listes lues dans Dokos pour ce cas (clients, articles) : les mêmes que
    /// celles du modèle, consultées avant toute recherche.
    referentiel: &'a Referentiel,
    cache: BTreeMap<String, Resolution>,
}

/// Cherche `texte` dans une liste du référentiel : identifiant exact d'abord,
/// puis identifiant ou libellé à la casse, aux accents et aux espaces près.
/// Une valeur choisie dans la liste déroulante du modèle est ainsi toujours
/// reconnue, même si la recherche de Dokos ne la retrouve pas (espaces doublés
/// ou insécables dans le nom, plus de dix clients au nom voisin…).
/// `None` : absente de la liste (ou liste non lue), à chercher dans Dokos.
fn dans_referentiel(referentiel: &Referentiel, liste: &str, texte: &str, par_libelle: bool) -> Option<Resolution> {
    let elements = referentiel.liste(liste);
    let candidat = |(id, libelle): &(String, String)| Candidat { id: id.clone(), libelle: libelle.clone() };
    if let Some(exact) = elements.iter().find(|(id, _)| id == texte) {
        return Some(Resolution::Trouve(candidat(exact)));
    }
    let cible = normaliser(texte);
    let mut proches: Vec<Candidat> = elements
        .iter()
        .filter(|(id, libelle)| normaliser(id) == cible || (par_libelle && normaliser(libelle) == cible))
        .map(candidat)
        .collect();
    match proches.len() {
        0 => None,
        1 => proches.pop().map(Resolution::Trouve),
        _ => Some(Resolution::Plusieurs(proches)),
    }
}

impl<T: Transport> Resolveur<'_, T> {
    fn resoudre(&mut self, champ: &str, texte: &str) -> Result<&Resolution, DokosError> {
        let cle = cle_choix(champ, texte);
        if let Some(id) = self.choix.get(&cle) {
            return Ok(self
                .cache
                .entry(cle)
                .or_insert_with(|| Resolution::Trouve(Candidat { id: id.clone(), libelle: id.clone() })));
        }
        if !self.cache.contains_key(&cle) {
            let connu = match champ {
                "client" => dans_referentiel(self.referentiel, LISTE_CLIENTS, texte, true),
                // Le détail d'un article (libellé, unité, groupe) n'est pas son libellé seul.
                "article" => dans_referentiel(self.referentiel, LISTE_ARTICLES, texte, false),
                _ => None,
            };
            if let Some(resolution) = connu {
                self.cache.insert(cle.clone(), resolution);
                return Ok(self.cache.get(&cle).unwrap_or(&Resolution::Aucune));
            }
            let resolution = match champ {
                "client" => depuis_correspondance(trouver_client(self.dokos, texte)?, |c| Candidat {
                    id: c.identifiant().to_owned(),
                    libelle: c.nom().to_owned(),
                }),
                "article" => depuis_correspondance(trouver_article(self.dokos, texte)?, |a| Candidat {
                    id: a.identifiant().to_owned(),
                    libelle: a.nom().to_owned(),
                }),
                _ => Resolution::Aucune,
            };
            self.cache.insert(cle.clone(), resolution);
        }
        Ok(self.cache.get(&cle).unwrap_or(&Resolution::Aucune))
    }
}

fn depuis_correspondance<C>(correspondance: Correspondance<C>, vers: impl Fn(&C) -> Candidat) -> Resolution {
    match correspondance {
        Correspondance::Unique(c) => Resolution::Trouve(vers(&c)),
        Correspondance::Plusieurs(candidats) => Resolution::Plusieurs(candidats.iter().map(vers).collect()),
        Correspondance::Aucune => Resolution::Aucune,
    }
}

/// Résultat d'une valeur résolue : l'identifiant, ou le statut qui bloque.
fn identifiant<T: Transport>(
    resolveur: &mut Resolveur<'_, T>,
    champ: &str,
    texte: &str,
) -> Result<Result<Candidat, Statut>, DokosError> {
    Ok(match resolveur.resoudre(champ, texte)? {
        Resolution::Trouve(c) => Ok(c.clone()),
        Resolution::Plusieurs(candidats) => {
            Err(Statut::APreciser { champ: champ.to_owned(), texte: texte.to_owned(), candidats: candidats.clone() })
        }
        Resolution::Aucune => Err(Statut::Introuvable { champ: champ.to_owned(), texte: texte.to_owned() }),
    })
}

/// Valeur exacte d'une liste du référentiel (casse et accents ignorés). Une
/// liste vide (non lue) laisse passer la valeur telle quelle.
fn dans_liste(referentiel: &Referentiel, liste: &str, champ: &str, valeur: &str) -> Result<String, Statut> {
    let elements = referentiel.liste(liste);
    if elements.is_empty() {
        return Ok(valeur.to_owned());
    }
    let cible = normaliser(valeur);
    elements
        .iter()
        .find(|(v, l)| normaliser(v) == cible || normaliser(l) == cible)
        .map(|(v, _)| v.clone())
        .ok_or_else(|| Statut::Introuvable { champ: champ.to_owned(), texte: valeur.to_owned() })
}

// --- Analyse des valeurs ---------------------------------------------------------

fn obligatoire<'a>(valeur: &'a str, nom: &str) -> Result<&'a str, Statut> {
    if valeur.trim().is_empty() { Err(Statut::Invalide(format!("{nom} manquant(e)"))) } else { Ok(valeur.trim()) }
}

fn date(valeur: &str, nom: &str) -> Result<Option<Date>, Statut> {
    let valeur = valeur.trim();
    if valeur.is_empty() {
        return Ok(None);
    }
    // Une cellule date-heure lue d'Excel : on garde la date.
    let seule = valeur.split_whitespace().next().unwrap_or(valeur);
    seule.parse().map(Some).map_err(|_| Statut::Invalide(format!("{nom} « {valeur} » illisible (JJ/MM/AAAA)")))
}

fn date_obligatoire(valeur: &str, nom: &str) -> Result<Date, Statut> {
    date(valeur, nom)?.ok_or_else(|| Statut::Invalide(format!("{nom} manquante")))
}

fn nombre(valeur: &str, nom: &str) -> Result<Option<f64>, Statut> {
    let valeur = valeur.trim();
    if valeur.is_empty() {
        return Ok(None);
    }
    valeur
        .replace(',', ".")
        .replace(' ', "")
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(Some)
        .ok_or_else(|| Statut::Invalide(format!("{nom} « {valeur} » n'est pas un nombre")))
}

fn oui_non(valeur: &str, nom: &str, defaut: bool) -> Result<bool, Statut> {
    match normaliser(valeur).as_str() {
        "" => Ok(defaut),
        "oui" | "o" | "yes" | "y" | "true" | "vrai" | "1" | "x" => Ok(true),
        "non" | "n" | "no" | "false" | "faux" | "0" => Ok(false),
        autre => Err(Statut::Invalide(format!("{nom} « {autre} » : attendu Oui ou Non"))),
    }
}

fn invalide<E: ToString>(e: E) -> Statut {
    Statut::Invalide(e.to_string())
}

// --- Analyse ---------------------------------------------------------------------

/// Analyse le tableau pour le cas donné. Rien n'est écrit dans Dokos.
pub fn analyser<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    tableau: &Tableau,
    referentiel: &Referentiel,
    options: &Options,
) -> Result<Analyse, ErreurImport> {
    let colonnes_cas = cas.colonnes();
    let colonnes = associer_colonnes(&tableau.entetes, &colonnes_cas)?;
    let tableau = tableau.clone().sans_exemple(&cas.exemple());
    let mut analyse = Analyse {
        cas,
        entetes: tableau.entetes.clone(),
        avertissements: colonnes.avertissements.clone(),
        lignes: tableau
            .lignes
            .iter()
            .map(|(n, v)| LigneAnalysee { numero: *n, valeurs: v.clone(), statut: Statut::Pret, document: None })
            .collect(),
        documents: Vec::new(),
    };
    if analyse.lignes.is_empty() {
        analyse.avertissements.push("aucune ligne à importer (hors exemple et lignes vides)".to_owned());
        return Ok(analyse);
    }
    let mut resolveur = Resolveur { dokos, choix: &options.choix, referentiel, cache: BTreeMap::new() };
    let valeur = |ligne: &LigneAnalysee, colonne: usize| colonnes.valeur(&ligne.valeurs, colonne).to_owned();
    match cas {
        Cas::Factures => analyser_factures(&mut analyse, &colonnes, &mut resolveur, options)?,
        Cas::Paiements => analyser_paiements(&mut analyse, &colonnes, dokos, referentiel),
        Cas::Articles => {
            for i in 0..analyse.lignes.len() {
                let ligne = &analyse.lignes[i];
                let v: Vec<String> = (0..6).map(|c| valeur(ligne, c)).collect();
                let resultat = (|| -> Result<Prepare, Statut> {
                    let groupe =
                        dans_liste(referentiel, LISTE_GROUPES_ARTICLES, "Groupe", obligatoire(&v[2], "Groupe")?)?;
                    let unite = dans_liste(referentiel, LISTE_UNITES, "Unité", obligatoire(&v[3], "Unité")?)?;
                    let article = NouvelArticle::new(&v[0], &v[1], &groupe, &unite)
                        .map_err(invalide)?
                        .vendable(oui_non(&v[4], "Vendable", true)?)
                        .avec_description(Some(&v[5]));
                    Ok(Prepare::Article(article))
                })();
                document_simple(&mut analyse, i, format!("Article {} — {}", v[0], v[1]), resultat);
            }
        }
        Cas::Clients => {
            for i in 0..analyse.lignes.len() {
                let ligne = &analyse.lignes[i];
                let v: Vec<String> = (0..6).map(|c| valeur(ligne, c)).collect();
                let resultat = (|| -> Result<Prepare, Statut> {
                    let type_client = match normaliser(&v[1]).as_str() {
                        "" | "organisation" | "entreprise" | "association" | "collectivite" | "societe" => {
                            TypeClient::Organisation
                        }
                        "particulier" | "individu" | "personne" => TypeClient::Particulier,
                        autre => {
                            return Err(Statut::Invalide(format!(
                                "Type « {autre} » : attendu Organisation ou Particulier"
                            )));
                        }
                    };
                    let groupe = if v[2].is_empty() {
                        None
                    } else {
                        Some(dans_liste(referentiel, LISTE_GROUPES_CLIENTS, "Groupe de clients", &v[2])?)
                    };
                    let territoire = if v[3].is_empty() {
                        None
                    } else {
                        Some(dans_liste(referentiel, LISTE_TERRITOIRES, "Territoire", &v[3])?)
                    };
                    let client = NouveauClient::new(&v[0], type_client)
                        .map_err(invalide)?
                        .avec_classement(groupe.as_deref(), territoire.as_deref())
                        .avec_contact(Some(&v[4]), Some(&v[5]))
                        .map_err(invalide)?;
                    Ok(Prepare::Client(client))
                })();
                document_simple(&mut analyse, i, format!("Client {}", v[0]), resultat);
            }
        }
    }
    Ok(analyse)
}

/// Une ligne = un document.
fn document_simple(analyse: &mut Analyse, i: usize, libelle: String, resultat: Result<Prepare, Statut>) {
    let numero = analyse.lignes[i].numero;
    let (prepare, statut) = match resultat {
        Ok(p) => (Some(p), Statut::Pret),
        Err(s) => (None, s),
    };
    analyse.lignes[i].statut = statut;
    analyse.lignes[i].document = Some(analyse.documents.len());
    analyse.documents.push(DocumentPrepare {
        cle: format!("ligne {numero}"),
        libelle,
        lignes: vec![i],
        prepare,
        doublon: None,
    });
}

/// Paiements : une ligne = un paiement, en règlement d'une facture validée.
///
/// Tout ce que Dokos refuserait à la création est refusé ici, ligne par ligne :
/// facture absente des factures à régler, montant au-delà du reste dû (en
/// tenant compte des lignes précédentes du fichier pour la même facture), mode
/// de paiement inconnu ou sans compte, référence manquante sur un compte bancaire.
fn analyser_paiements<T: Transport>(
    analyse: &mut Analyse,
    colonnes: &Colonnes,
    dokos: &Dokos<T>,
    referentiel: &Referentiel,
) {
    // Compte d'encaissement par (mode, société) : une seule question à Dokos pour chaque couple.
    let mut comptes: BTreeMap<(String, String), Result<CompteEncaissement, String>> = BTreeMap::new();
    // Déjà réparti par les lignes précédentes, par facture.
    let mut reparti: BTreeMap<String, f64> = BTreeMap::new();
    for i in 0..analyse.lignes.len() {
        let valeurs = analyse.lignes[i].valeurs.clone();
        let v = |c: usize| colonnes.valeur(&valeurs, c).to_owned();
        let (facture_texte, date_texte, montant_texte, mode_texte, reference) = (v(0), v(1), v(2), v(3), v(4));
        let mut libelle = format!("Paiement {facture_texte}");
        let resultat = (|| -> Result<Prepare, Statut> {
            let facture_texte = obligatoire(&facture_texte, "Facture")?;
            let cible = normaliser(facture_texte);
            let facture = referentiel
                .factures_a_regler
                .iter()
                .find(|f| f.id == facture_texte)
                .or_else(|| referentiel.factures_a_regler.iter().find(|f| normaliser(&f.id) == cible))
                .ok_or_else(|| {
                    Statut::Invalide(format!(
                        "Facture « {facture_texte} » : inconnue, en brouillon, annulée ou déjà soldée dans Dokos"
                    ))
                })?;
            libelle = format!("Paiement {}", facture.id);
            let date_paiement = date_obligatoire(&date_texte, "Date du paiement")?;
            let restant = arrondi(facture.reste_du - reparti.get(&facture.id).copied().unwrap_or(0.0));
            if restant <= 0.0 {
                return Err(Statut::Invalide(format!(
                    "Facture {} : déjà réglée en totalité par les lignes précédentes du fichier",
                    facture.id
                )));
            }
            let montant = nombre(&montant_texte, "Montant")?.map_or(restant, arrondi);
            if montant <= 0.0 {
                return Err(Statut::Invalide(format!(
                    "Montant « {montant_texte} » : un nombre supérieur à zéro est attendu"
                )));
            }
            if montant > restant + 0.005 {
                return Err(Statut::Invalide(format!(
                    "Montant {montant:.2} supérieur au reste dû de la facture {} : {restant:.2} {} (lignes précédentes du fichier déduites)",
                    facture.id, facture.devise
                )));
            }
            let client = if facture.nom_client.is_empty() { &facture.client } else { &facture.nom_client };
            libelle = format!("Paiement {} — {client} — {montant:.2} {} le {date_texte}", facture.id, facture.devise);
            let mode = dans_liste(
                referentiel,
                LISTE_MODES_PAIEMENT,
                "Mode de paiement",
                obligatoire(&mode_texte, "Mode de paiement")?,
            )?;
            let cle = (mode.clone(), facture.societe.clone());
            if !comptes.contains_key(&cle) {
                match compte_du_mode(dokos, &mode, &facture.societe) {
                    Ok(compte) => {
                        comptes.insert(cle.clone(), Ok(compte));
                    }
                    // Refus de Dokos (mode sans compte pour la société) : le même pour toutes les lignes.
                    Err(DokosError::Validation(message)) => {
                        comptes.insert(cle.clone(), Err(message));
                    }
                    Err(autre) => return Err(invalide(autre)),
                }
            }
            let compte = match comptes.get(&cle) {
                Some(Ok(compte)) => compte.clone(),
                Some(Err(message)) => return Err(Statut::Invalide(message.clone())),
                None => return Err(Statut::Invalide("compte d'encaissement non résolu".to_owned())),
            };
            let paiement = NouveauPaiement::new(&facture.id, date_paiement, montant, &mode, compte, Some(&reference))
                .map_err(invalide)?;
            *reparti.entry(facture.id.clone()).or_insert(0.0) += paiement.montant();
            Ok(Prepare::Paiement(paiement))
        })();
        document_simple(analyse, i, libelle, resultat);
    }
}

fn arrondi(montant: f64) -> f64 {
    (montant * 100.0).round() / 100.0
}

/// Factures : les lignes d'un même lot forment un document.
fn analyser_factures<T: Transport>(
    analyse: &mut Analyse,
    colonnes: &Colonnes,
    resolveur: &mut Resolveur<'_, T>,
    options: &Options,
) -> Result<(), ErreurImport> {
    let type_vente = TypeVente::Facture;
    // Indices des colonnes : lot, client, date, échéance, article, quantité, prix, tarif du client, référence.
    let (col_echeance, col_article, col_quantite, col_prix, col_reference) = (3, 4, 5, 6, 8);
    let col_tarif = col_prix + 1;
    let nom_date = "Date de facture";
    struct Lot {
        document: usize,
        client: String,
        date: String,
        echeance: String,
        reference: String,
        lignes_vente: Vec<Result<LigneVente, ()>>,
    }
    let mut lots: BTreeMap<String, Lot> = BTreeMap::new();
    let mut ordre: Vec<String> = Vec::new();

    for i in 0..analyse.lignes.len() {
        let valeurs = analyse.lignes[i].valeurs.clone();
        let numero = analyse.lignes[i].numero;
        let v = |c: usize| colonnes.valeur(&valeurs, c).to_owned();
        let lot_texte = v(0);
        let cle = if lot_texte.is_empty() { format!("ligne {numero}") } else { normaliser(&lot_texte) };
        let client_texte = v(1);
        let date_texte = v(2);
        let echeance_texte = v(col_echeance);
        let reference_texte = v(col_reference);

        let document = match lots.get(&cle) {
            Some(lot) => lot.document,
            None => {
                let document = analyse.documents.len();
                analyse.documents.push(DocumentPrepare {
                    cle: if lot_texte.is_empty() { cle.clone() } else { lot_texte.clone() },
                    libelle: String::new(),
                    lignes: Vec::new(),
                    prepare: None,
                    doublon: None,
                });
                lots.insert(
                    cle.clone(),
                    Lot {
                        document,
                        client: client_texte.clone(),
                        date: date_texte.clone(),
                        echeance: echeance_texte.clone(),
                        reference: reference_texte.clone(),
                        lignes_vente: Vec::new(),
                    },
                );
                ordre.push(cle.clone());
                document
            }
        };
        analyse.documents[document].lignes.push(i);
        analyse.lignes[i].document = Some(document);

        let statut = (|| -> Result<LigneVente, Statut> {
            let lot = lots.get(&cle).ok_or_else(|| Statut::Invalide("lot inconnu".to_owned()))?;
            if normaliser(&client_texte) != normaliser(&lot.client)
                || date_texte != lot.date
                || echeance_texte != lot.echeance
            {
                return Err(Statut::Invalide(format!(
                    "client ou dates différents de la première ligne du lot « {} »",
                    if lot_texte.is_empty() { cle.as_str() } else { lot_texte.as_str() }
                )));
            }
            let client = obligatoire(&client_texte, "Client")?;
            identifiant(resolveur, "client", client).map_err(invalide)??;
            date_obligatoire(&date_texte, nom_date)?;
            let article_texte = v(col_article);
            let article = obligatoire(&article_texte, "Article")?;
            let article = identifiant(resolveur, "article", article).map_err(invalide)??;
            let quantite = nombre(&v(col_quantite), "Quantité")?
                .ok_or_else(|| Statut::Invalide("Quantité manquante".to_owned()))?;
            let prix = nombre(&v(col_prix), "Prix unitaire HT")?;
            let tarif_client = oui_non(&v(col_tarif), "Tarif du client", false)?;
            LigneVente::new(&article.id, quantite, None)
                .and_then(|l| l.avec_prix(prix))
                .and_then(|l| l.avec_tarif_client(tarif_client))
                .map_err(invalide)
        })();
        let lot = lots.get_mut(&cle).ok_or_else(|| ErreurImport::Colonnes("lot perdu".to_owned()))?;
        match statut {
            Ok(ligne) => {
                lot.lignes_vente.push(Ok(ligne));
                analyse.lignes[i].statut = Statut::Pret;
            }
            Err(s) => {
                lot.lignes_vente.push(Err(()));
                analyse.lignes[i].statut = s;
            }
        }
    }

    for cle in ordre {
        let Some(lot) = lots.remove(&cle) else { continue };
        let document = &mut analyse.documents[lot.document];
        let client_libelle = match resolveur.resoudre("client", &lot.client) {
            Ok(Resolution::Trouve(c)) => c.libelle.clone(),
            _ => lot.client.clone(),
        };
        document.libelle = format!(
            "{} {} du {} ({} ligne{})",
            majuscule(type_vente.libelle()),
            client_libelle,
            lot.date,
            document.lignes.len(),
            if document.lignes.len() > 1 { "s" } else { "" }
        );
        if lot.lignes_vente.iter().any(Result::is_err) {
            continue;
        }
        let lignes: Vec<LigneVente> = lot.lignes_vente.into_iter().flatten().collect();
        let prepare = (|| -> Result<NouveauDocumentVente, Statut> {
            let client = match resolveur.resoudre("client", &lot.client).map_err(invalide)? {
                Resolution::Trouve(c) => c.id.clone(),
                _ => return Err(Statut::Invalide("client non résolu".to_owned())),
            };
            let date_document = date_obligatoire(&lot.date, nom_date)?;
            let echeance = date(&lot.echeance, "Échéance")?;
            NouveauDocumentVente::new(type_vente, &client, date_document, echeance, lignes)
                .map_err(invalide)?
                .avec_reference(Some(&lot.reference))
                .map_err(invalide)
                .map(|d| d.avec_societe(options.societe.as_deref()))
        })();
        match prepare {
            Ok(d) => document.prepare = Some(Prepare::Vente(d)),
            Err(s) => {
                // L'erreur porte sur l'en-tête : on la signale sur la première ligne du lot.
                if let Some(&premiere) = document.lignes.first() {
                    analyse.lignes[premiere].statut = s;
                }
            }
        }
    }
    Ok(())
}

fn majuscule(texte: &str) -> String {
    let mut c = texte.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

// --- Vérification et création ------------------------------------------------------

/// Cherche, pour chaque document prêt, un document identique dans Dokos.
///
/// Une ou plusieurs requêtes par document : elles partent en parallèle (en
/// nombre limité) quand le transport le permet, ce qui divise d'autant
/// l'attente sur un gros fichier.
pub fn verifier<T: Transport>(dokos: &Dokos<T>, analyse: &mut Analyse) -> Result<(), ErreurImport> {
    let doublons = dokos.pour_chacun(&analyse.documents, |document| match &document.prepare {
        Some(prepare) => doublon(dokos, prepare),
        None => Ok(None),
    })?;
    for (document, doublon) in analyse.documents.iter_mut().zip(doublons) {
        if document.prepare.is_some() {
            document.doublon = doublon;
        }
    }
    Ok(())
}

/// Document identique déjà présent dans Dokos, s'il y en a un.
fn doublon<T: Transport>(dokos: &Dokos<T>, prepare: &Prepare) -> Result<Option<Existant>, ErreurImport> {
    Ok(match prepare {
        Prepare::Vente(v) => chercher_doublons(dokos, v)?.identiques.into_iter().next().map(|d| Existant {
            doctype: v.type_vente().doctype().to_owned(),
            id: d.id.clone(),
            description: format!("{} du {}, {:.2} {}", d.statut(), d.date_document, d.total, d.devise),
        }),
        Prepare::Article(a) => chercher_article_identique(dokos, a)?.map(|e: ArticleExistant| Existant {
            doctype: DOCTYPE_ARTICLE.to_owned(),
            id: e.code,
            description: e.libelle,
        }),
        Prepare::Client(c) => chercher_client_identique(dokos, c)?.map(|x| Existant {
            doctype: DOCTYPE_CLIENT.to_owned(),
            id: x.id,
            description: x.nom,
        }),
        Prepare::Paiement(p) => chercher_paiement_identique(dokos, p)?.map(|e| Existant {
            doctype: DOCTYPE_PAIEMENT.to_owned(),
            description: format!("{} du {}, {:.2}", e.statut(), e.date, e.montant),
            id: e.id,
        }),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatutCreation {
    Cree,
    /// Document identique déjà dans Dokos : rien créé.
    Existant,
    /// Non créé : ligne bloquante, ou arrêt après une erreur.
    Ignore,
    Erreur,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultatDocument {
    pub cle: String,
    pub libelle: String,
    pub doctype: String,
    pub id: Option<String>,
    pub statut: StatutCreation,
    pub message: String,
    pub lignes: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionsCreation {
    /// Faux : rien n'est créé s'il reste une ligne bloquante.
    pub seulement_prets: bool,
    pub si_doublon: SiDoublon,
}

/// Crée les documents prêts, dans l'ordre du fichier. S'arrête à la première
/// erreur de Dokos : les documents suivants sont marqués ignorés.
pub fn creer<T: Transport>(
    dokos: &Dokos<T>,
    analyse: &Analyse,
    options: &OptionsCreation,
) -> Result<Vec<ResultatDocument>, ErreurImport> {
    let bloquantes = analyse.lignes_bloquantes();
    if bloquantes > 0 && !options.seulement_prets {
        return Err(ErreurImport::LignesBloquantes(bloquantes));
    }
    let mut resultats = Vec::with_capacity(analyse.documents.len());
    let mut arret = false;
    for document in &analyse.documents {
        let base = |statut: StatutCreation, id: Option<String>, message: String, doctype: &str| ResultatDocument {
            cle: document.cle.clone(),
            libelle: document.libelle.clone(),
            doctype: doctype.to_owned(),
            id,
            statut,
            message,
            lignes: document.lignes.clone(),
        };
        let Some(prepare) = &document.prepare else {
            resultats.push(base(StatutCreation::Ignore, None, "ligne(s) à corriger".to_owned(), ""));
            continue;
        };
        if arret {
            resultats.push(base(
                StatutCreation::Ignore,
                None,
                "non traité : arrêt après une erreur".to_owned(),
                prepare.doctype(),
            ));
            continue;
        }
        let resultat: Result<(bool, String, String), DokosError> = match prepare {
            Prepare::Vente(v) => creer_brouillon(dokos, v, options.si_doublon).map(|c| match c {
                Creation::Cree { document, autres } => {
                    let message = if autres.is_empty() {
                        format!("brouillon créé, total {:.2} {}", document.total, document.devise)
                    } else {
                        format!("brouillon créé ; {} autre(s) document(s) de ce client aux mêmes dates", autres.len())
                    };
                    (true, document.id, message + &tarifs_du_client(dokos, v))
                }
                Creation::DejaExistant(d) => (false, d.id, "document identique déjà dans Dokos".to_owned()),
            }),
            Prepare::Paiement(p) => creer_paiement(dokos, p, options.si_doublon).map(|c| match c {
                CreationPaiement::Cree(e) => (
                    true,
                    e.id,
                    format!(
                        "paiement de {:.2} créé en brouillon pour {} : à valider dans Dokos pour solder la facture",
                        e.montant,
                        p.facture()
                    ),
                ),
                CreationPaiement::DejaExistant(e) => {
                    (false, e.id, format!("paiement identique déjà dans Dokos ({})", e.statut()))
                }
            }),
            Prepare::Article(a) => creer_article(dokos, a, options.si_doublon).map(|c| match c {
                CreationArticle::Cree(e) => (true, e.code, "article créé, désactivé".to_owned()),
                CreationArticle::DejaExistant(e) => (false, e.code, "code ou libellé déjà utilisé".to_owned()),
            }),
            Prepare::Client(c) => creer_client(dokos, c, options.si_doublon).map(|r| match r {
                CreationClient::Cree(x) => (true, x.id, "client créé".to_owned()),
                CreationClient::DejaExistant(x) => (false, x.id, "client de même nom déjà présent".to_owned()),
            }),
        };
        match resultat {
            Ok((true, id, message)) => resultats.push(base(StatutCreation::Cree, Some(id), message, prepare.doctype())),
            Ok((false, id, message)) => {
                resultats.push(base(StatutCreation::Existant, Some(id), message, prepare.doctype()))
            }
            Err(e) => {
                tracing::warn!(document = %document.cle, "import : {e}");
                resultats.push(base(StatutCreation::Erreur, None, e.to_string(), prepare.doctype()));
                arret = true;
            }
        }
    }
    Ok(resultats)
}

/// Enregistre dans Dokos les prix marqués « Tarif du client » d'un document
/// qui vient d'être créé, et le dit en fin de message. Un échec n'annule pas
/// le brouillon : il est signalé, le prix du document reste celui saisi.
fn tarifs_du_client<T: Transport>(dokos: &Dokos<T>, vente: &NouveauDocumentVente) -> String {
    // Un prix par article : le dernier saisi dans le document.
    let mut prix: BTreeMap<&str, f64> = BTreeMap::new();
    for ligne in vente.lignes().iter().filter(|l| l.tarif_client()) {
        if let Some(p) = ligne.prix() {
            prix.insert(ligne.article(), p);
        }
    }
    let mut message = String::new();
    for (article, p) in prix {
        match enregistrer_prix_client(dokos, vente.client(), article, p) {
            Ok(PrixClient::Inchange { .. }) => {}
            Ok(fait) => {
                message.push_str(&format!(" ; tarif du client enregistré pour {article} ({})", fait.liste()));
            }
            Err(e) => {
                tracing::warn!(article, "import : tarif du client non enregistré : {e}");
                message.push_str(&format!(" ; tarif du client NON enregistré pour {article} : {e}"));
            }
        }
    }
    message
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;
    use crate::lecture::lire_csv;
    use crate::referentiel::{LISTE_GROUPES_ARTICLES, LISTE_UNITES};

    fn options() -> Options {
        Options { societe: Some("Les Amanins".into()), choix: BTreeMap::new() }
    }

    fn reponse_clients(faux: &FauxTransport, noms: &[(&str, &str)]) {
        let data: Vec<_> = noms.iter().map(|(id, nom)| json!({"name": id, "customer_name": nom})).collect();
        faux.repondre_json(200, &json!({"data": data}));
    }

    fn reponse_articles(faux: &FauxTransport, codes: &[(&str, &str)]) {
        let data: Vec<_> = codes.iter().map(|(code, nom)| json!({"name": code, "item_name": nom})).collect();
        faux.repondre_json(200, &json!({"data": data}));
    }

    #[test]
    fn factures_par_lot() {
        let csv = "Lot;Client;Date de facture;Échéance;Article;Quantité;Référence\n\
                   OCT;Mairie de Crest;31/10/2026;;REPAS;25;Bon 118\n\
                   OCT;Mairie de Crest;31/10/2026;;grande salle;1;\n\
                   ;Crest;31/10/2026;30/11/2026;REPAS;dix;\n\
                   OCT;CCAS;31/10/2026;;REPAS;2;\n";
        let tableau = lire_csv(csv).expect("csv");
        let faux = FauxTransport::new();
        reponse_clients(&faux, &[("CUST-2", "Mairie de Crest")]);
        reponse_articles(&faux, &[("REPAS", "Repas")]);
        reponse_articles(&faux, &[("SALLE-G", "Grande salle")]);
        reponse_clients(&faux, &[("CUST-3", "CCAS de Crest"), ("CUST-4", "Mairie de Crest")]);
        let analyse = analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &Referentiel::default(), &options())
            .expect("analyse");
        assert_eq!(analyse.documents.len(), 2);
        assert_eq!(analyse.documents[0].lignes, [0, 1, 3]);
        assert!(analyse.documents[0].prepare.is_none(), "le lot a une ligne invalide");
        assert_eq!(analyse.lignes[0].statut, Statut::Pret);
        assert!(matches!(&analyse.lignes[3].statut, Statut::Invalide(m) if m.contains("première ligne du lot")));
        assert!(
            matches!(&analyse.lignes[2].statut, Statut::APreciser { champ, candidats, .. } if champ == "client" && candidats.len() == 2)
        );
        assert_eq!(analyse.lignes_bloquantes(), 2);
        assert_eq!(analyse.documents_prets(), 0);
        assert!(analyse.documents[0].libelle.starts_with("Facture Mairie de Crest du 31/10/2026 (3 lignes)"));
        // Les lectures sont mises en cache : une seule recherche du client « Mairie de Crest ».
        assert_eq!(faux.requetes().len(), 4);
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));
    }

    #[test]
    fn client_de_la_liste_du_modele_toujours_reconnu() {
        // Nom à espace doublé et à espace insécable : la recherche « contient » de
        // Dokos ne le retrouverait pas. Il est dans la liste du modèle : aucune requête.
        let mut referentiel = Referentiel::default();
        referentiel.ajouter(
            LISTE_CLIENTS,
            vec![
                ("Mairie  de\u{a0}Crest".into(), "Mairie  de\u{a0}Crest".into()),
                ("CUST-9".into(), "Zoé Martin".into()),
            ],
        );
        referentiel.ajouter(LISTE_ARTICLES, vec![("REPAS".into(), "Repas — Unité".into())]);
        let csv = "Lot;Client;Date de facture;Échéance;Article;Quantité;Référence\n\
                   ;Mairie  de\u{a0}Crest;31/10/2026;;REPAS;1;\n\
                   ;zoe martin;31/10/2026;;repas;1;\n";
        let tableau = lire_csv(csv).expect("csv");
        let faux = FauxTransport::new();
        let analyse = analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &referentiel, &options()).expect("analyse");
        assert_eq!(analyse.documents_prets(), 2);
        let Some(Prepare::Vente(v)) = &analyse.documents[0].prepare else { panic!() };
        assert_eq!(v.client(), "Mairie  de\u{a0}Crest");
        let Some(Prepare::Vente(v)) = &analyse.documents[1].prepare else { panic!() };
        assert_eq!(v.client(), "CUST-9");
        assert!(faux.requetes().is_empty());
    }

    #[test]
    fn prix_unitaire_facultatif() {
        let csv = "Lot;Client;Date de facture;Échéance;Article;Quantité;Prix unitaire HT;Référence\n\
                   ;CCAS;31/10/2026;;REPAS;10;12,50;\n\
                   ;CCAS;31/10/2026;;REPAS;10;;\n\
                   ;CCAS;31/10/2026;;REPAS;10;-3;\n";
        let tableau = lire_csv(csv).expect("csv");
        let faux = FauxTransport::new();
        reponse_articles(&faux, &[("REPAS", "Repas")]);
        let mut options = options();
        options.choix.insert(cle_choix("client", "ccas"), "CUST-3".into());
        let analyse =
            analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &Referentiel::default(), &options).expect("analyse");
        assert_eq!(analyse.documents_prets(), 2);
        let Some(Prepare::Vente(avec)) = &analyse.documents[0].prepare else { panic!() };
        assert_eq!(avec.corps()["items"][0]["rate"], 12.5);
        assert_eq!(avec.corps()["ignore_pricing_rule"], 1);
        let Some(Prepare::Vente(sans)) = &analyse.documents[1].prepare else { panic!() };
        assert!(sans.corps()["items"][0].get("rate").is_none());
        assert!(matches!(&analyse.lignes[2].statut, Statut::Invalide(m) if m.contains("prix invalide")));
    }

    #[test]
    fn choix_d_ambiguite_puis_creation() {
        let csv = "Lot;Client;Date de facture;Échéance;Article;Quantité;Référence\n;CCAS;31/10/2026;;REPAS;10;\n";
        let tableau = lire_csv(csv).expect("csv");
        let faux = FauxTransport::new();
        reponse_articles(&faux, &[("REPAS", "Repas")]);
        let mut options = options();
        options.choix.insert(cle_choix("client", "ccas"), "CUST-3".into());
        let mut analyse =
            analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &Referentiel::default(), &options).expect("analyse");
        assert_eq!(analyse.documents_prets(), 1);
        let Some(Prepare::Vente(v)) = &analyse.documents[0].prepare else { panic!() };
        assert_eq!(v.client(), "CUST-3");
        assert_eq!(v.corps()["company"], "Les Amanins");

        faux.repondre_json(200, &json!({"data": []})); // doublons
        verifier(&Dokos::new(&faux), &mut analyse).expect("vérification");
        assert_eq!(analyse.doublons(), 0);

        faux.repondre_json(200, &json!({"data": []})); // doublons à la création
        faux.repondre_json(
            200,
            &json!({"data": {"name": "F-1", "docstatus": 0, "grand_total": 120.0, "currency": "EUR"}}),
        );
        let resultats = creer(
            &Dokos::new(&faux),
            &analyse,
            &OptionsCreation { seulement_prets: false, si_doublon: SiDoublon::Reutiliser },
        )
        .expect("création");
        assert_eq!(resultats[0].statut, StatutCreation::Cree);
        assert_eq!(resultats[0].id.as_deref(), Some("F-1"));
        assert_eq!(resultats[0].doctype, "Sales Invoice");
        assert!(faux.requetes().last().is_some_and(|r| r.methode == Methode::Post));
    }

    #[test]
    fn tarif_du_client_enregistre_a_la_creation() {
        let csv = "Lot;Client;Date de facture;Échéance;Article;Quantité;Prix unitaire HT;Tarif du client;Référence\n\
                   ;CCAS;31/10/2026;;REPAS;10;12,50;Oui;\n\
                   ;CCAS;31/10/2026;;REPAS;10;;oui;\n";
        let tableau = lire_csv(csv).expect("csv");
        let faux = FauxTransport::new();
        reponse_articles(&faux, &[("REPAS", "Repas")]);
        let mut options = options();
        options.choix.insert(cle_choix("client", "ccas"), "CUST-3".into());
        let analyse =
            analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &Referentiel::default(), &options).expect("analyse");
        // Tarif du client demandé sans prix : ligne refusée.
        assert!(matches!(&analyse.lignes[1].statut, Statut::Invalide(m) if m.contains("aucun prix unitaire")));
        assert_eq!(analyse.documents_prets(), 1);

        faux.repondre_json(200, &json!({"data": []})); // doublons à la création
        faux.repondre_json(
            200,
            &json!({"data": {"name": "F-1", "docstatus": 0, "grand_total": 125.0, "currency": "EUR"}}),
        );
        faux.repondre_json(200, &json!({"data": {"default_price_list": "Tarif CCAS"}})); // liste du client
        faux.repondre_json(200, &json!({"data": []})); // aucun prix pour ce client
        faux.repondre_json(200, &json!({"data": {"name": "PRIX-1"}}));
        let resultats = creer(
            &Dokos::new(&faux),
            &analyse,
            &OptionsCreation { seulement_prets: true, si_doublon: SiDoublon::Reutiliser },
        )
        .expect("création");
        assert_eq!(resultats[0].statut, StatutCreation::Cree);
        assert!(resultats[0].message.contains("tarif du client enregistré pour REPAS (Tarif CCAS)"));
        let requetes = faux.requetes();
        let derniere = requetes.last().expect("requête");
        assert_eq!(derniere.methode, Methode::Post);
        assert_eq!(derniere.chemin, "/api/resource/Item%20Price");
        let corps = derniere.corps.as_ref().expect("corps");
        assert_eq!(corps["customer"], "CUST-3");
        assert_eq!(corps["price_list_rate"], 12.5);
    }

    #[test]
    fn rien_cree_si_une_ligne_bloque() {
        let csv = "Lot;Client;Date de facture;Échéance;Article;Quantité;Référence\n;Licorne;31/10/2026;;REPAS;1;\n";
        let tableau = lire_csv(csv).expect("csv");
        let faux = FauxTransport::new();
        reponse_clients(&faux, &[]);
        let analyse = analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &Referentiel::default(), &options())
            .expect("analyse");
        assert!(matches!(&analyse.lignes[0].statut, Statut::Introuvable { champ, .. } if champ == "client"));
        let erreur = creer(
            &Dokos::new(&faux),
            &analyse,
            &OptionsCreation { seulement_prets: false, si_doublon: SiDoublon::Reutiliser },
        )
        .expect_err("bloqué");
        assert_eq!(erreur, ErreurImport::LignesBloquantes(1));
        let resultats = creer(
            &Dokos::new(&faux),
            &analyse,
            &OptionsCreation { seulement_prets: true, si_doublon: SiDoublon::Reutiliser },
        )
        .expect("ignoré");
        assert_eq!(resultats[0].statut, StatutCreation::Ignore);
        assert_eq!(faux.requetes().len(), 1);
    }

    #[test]
    fn articles_et_clients() {
        let mut referentiel = Referentiel::default();
        referentiel.ajouter(LISTE_GROUPES_ARTICLES, vec![("Restauration".into(), "Restauration".into())]);
        referentiel.ajouter(LISTE_UNITES, vec![("Unité".into(), "Unité".into())]);
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);

        let articles = lire_csv("Code;Libellé;Groupe;Unité;Vendable;Description\nGOUTER;Goûter;restauration;unité;non;\nA/B;x;Restauration;Unité;;\nC;x;Boissons;Unité;;\n").expect("csv");
        let analyse = analyser(&dokos, Cas::Articles, &articles, &referentiel, &options()).expect("analyse");
        assert_eq!(analyse.lignes[0].statut, Statut::Pret);
        let Some(Prepare::Article(a)) = &analyse.documents[0].prepare else { panic!() };
        assert_eq!(a.corps()["is_sales_item"], 0);
        assert_eq!(a.groupe(), "Restauration");
        assert!(matches!(&analyse.lignes[1].statut, Statut::Invalide(_)));
        assert!(matches!(&analyse.lignes[2].statut, Statut::Introuvable { champ, .. } if champ == "Groupe"));

        let clients = lire_csv("Nom;Type;Groupe de clients;Territoire;E-mail;Téléphone\nMairie de Crest;;;;mairie@crest.fr;\nX;robot;;;;\n").expect("csv");
        let analyse = analyser(&dokos, Cas::Clients, &clients, &referentiel, &options()).expect("analyse");
        assert_eq!(analyse.lignes[0].statut, Statut::Pret);
        assert!(matches!(&analyse.lignes[1].statut, Statut::Invalide(m) if m.contains("Organisation")));
        assert!(faux.requetes().is_empty(), "aucune lecture nécessaire pour ces cas");
    }

    #[test]
    fn colonnes_manquantes_et_valeurs() {
        let tableau = lire_csv("Client;Quantité\nMairie;1\n").expect("csv");
        let faux = FauxTransport::new();
        let erreur = analyser(&Dokos::new(&faux), Cas::Factures, &tableau, &Referentiel::default(), &options())
            .expect_err("colonnes");
        assert!(matches!(erreur, ErreurImport::Colonnes(_)));
        assert_eq!(oui_non(" ", "x", true), Ok(true));
        assert_eq!(oui_non("x", "x", false), Ok(true));
        assert!(oui_non("peut-être", "x", false).is_err());
        assert_eq!(nombre("1 250,5", "q").expect("nombre"), Some(1250.5));
        assert!(nombre("abc", "q").is_err());
        assert_eq!(date("2026-11-03 00:00", "d").expect("date").map(|d| d.to_string()), Some("2026-11-03".into()));
        assert_eq!(cle_choix("client", " Mairie de CREST "), "client|mairie de crest");
    }
}
