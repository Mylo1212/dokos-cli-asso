//! Catalogue : groupes d'articles (DocType arborescent `Item Group`), articles
//! d'un groupe, prix indicatifs, et gestion des articles.
//!
//! Lecture : arbre des groupes, articles d'un groupe et de ses sous-groupes,
//! prix de la liste de prix de vente par défaut (indicatif : Dokos reste seul
//! juge du prix appliqué). Écriture : un article est créé **désactivé** puis
//! activé par un geste séparé.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Dokos, Filtre, Liste, lister_noms};
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::recherche::motif_recherche;
use crate::transport::Transport;
use crate::types::{Article, CHAMPS_ARTICLE, DOCTYPE_ARTICLE, texte_ou_vide};

pub const DOCTYPE_GROUPE: &str = "Item Group";
pub const DOCTYPE_PRIX: &str = "Item Price";
pub const DOCTYPE_UNITE: &str = "UOM";
const DOCTYPE_REGLAGES_VENTE: &str = "Selling Settings";
/// Nombre maximal de groupes lus (l'arbre complet).
const GROUPES_MAX: u32 = 500;
/// Nombre maximal d'articles affichés pour un groupe.
pub const ARTICLES_PAR_GROUPE_MAX: u32 = 200;
pub const CODE_MAX: usize = 140;
/// Profondeur maximale de l'arbre reconstruit (protection contre un cycle).
const PROFONDEUR_MAX: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GroupeArticle {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "parent_item_group", default, deserialize_with = "texte_ou_vide")]
    pub parent: String,
    #[serde(default)]
    pub is_group: u8,
}

/// Nœud de l'arbre des groupes, enfants triés par nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noeud {
    pub id: String,
    /// Vrai si le nœud peut contenir des sous-groupes.
    pub groupe: bool,
    pub enfants: Vec<Noeud>,
}

impl Noeud {
    /// Identifiants du nœud et de toute sa descendance.
    pub fn identifiants(&self) -> Vec<String> {
        let mut ids = vec![self.id.clone()];
        for enfant in &self.enfants {
            ids.extend(enfant.identifiants());
        }
        ids
    }
}

/// Tous les groupes d'articles, à plat.
pub fn lister_groupes<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<GroupeArticle>, DokosError> {
    let liste = Liste::new(&["name", "parent_item_group", "is_group"]).ordre("name asc").limite(GROUPES_MAX);
    dokos.lister(DOCTYPE_GROUPE, &liste)
}

/// Reconstruit l'arbre. Un groupe dont le parent est inconnu devient une racine.
pub fn construire_arbre(groupes: &[GroupeArticle]) -> Vec<Noeud> {
    let connus: std::collections::BTreeSet<&str> = groupes.iter().map(|g| g.id.as_str()).collect();
    let mut par_parent: BTreeMap<&str, Vec<&GroupeArticle>> = BTreeMap::new();
    for groupe in groupes {
        let parent = if connus.contains(groupe.parent.as_str()) && groupe.parent != groupe.id {
            groupe.parent.as_str()
        } else {
            ""
        };
        par_parent.entry(parent).or_default().push(groupe);
    }
    fn enfants_de(parent: &str, par_parent: &BTreeMap<&str, Vec<&GroupeArticle>>, profondeur: usize) -> Vec<Noeud> {
        if profondeur > PROFONDEUR_MAX {
            return Vec::new();
        }
        let mut noeuds: Vec<Noeud> = par_parent
            .get(parent)
            .map(|groupes| {
                groupes
                    .iter()
                    .map(|g| Noeud {
                        id: g.id.clone(),
                        groupe: g.is_group == 1,
                        enfants: enfants_de(&g.id, par_parent, profondeur + 1),
                    })
                    .collect()
            })
            .unwrap_or_default();
        noeuds.sort_by_key(|n| n.id.to_lowercase());
        noeuds
    }
    enfants_de("", &par_parent, 0)
}

/// Arbre des groupes, lu dans Dokos.
pub fn arbre_des_groupes<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<Noeud>, DokosError> {
    Ok(construire_arbre(&lister_groupes(dokos)?))
}

/// Articles actifs et vendables d'un groupe et de ses sous-groupes, avec un
/// texte de recherche facultatif.
pub fn articles_du_groupe<T: Transport>(
    dokos: &Dokos<T>,
    groupe: &str,
    texte: Option<&str>,
    limite: u32,
) -> Result<Vec<Article>, DokosError> {
    let groupe = groupe.trim();
    if groupe.is_empty() {
        return Err(DokosError::Configuration("groupe d'articles vide".to_owned()));
    }
    let mut liste = Liste::new(CHAMPS_ARTICLE)
        .filtre(Filtre::egal("disabled", 0))
        .filtre(Filtre::egal("is_sales_item", 1))
        .filtre(Filtre::egal("has_variants", 0))
        .filtre(Filtre::descendant_de("item_group", groupe))
        .ordre("item_name asc")
        .limite(limite);
    if let Some(texte) = texte.map(str::trim).filter(|t| !t.is_empty()) {
        let motif = motif_recherche(texte)?;
        liste = liste.ou(Filtre::contient("name", &motif)).ou(Filtre::contient("item_name", &motif));
    }
    dokos.lister(DOCTYPE_ARTICLE, &liste)
}

/// Prix d'un article dans une liste de prix.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Prix {
    #[serde(rename = "item_code")]
    pub article: String,
    #[serde(rename = "price_list_rate", default)]
    pub prix: f64,
    #[serde(rename = "currency", default, deserialize_with = "texte_ou_vide")]
    pub devise: String,
    #[serde(rename = "price_list", default, deserialize_with = "texte_ou_vide")]
    pub liste: String,
}

/// Liste de prix de vente par défaut (`Selling Settings`). `None` si
/// l'utilisateur API n'a pas le droit de la lire ou si elle n'est pas définie.
pub fn liste_de_prix_par_defaut<T: Transport>(dokos: &Dokos<T>) -> Result<Option<String>, DokosError> {
    #[derive(Deserialize)]
    struct Reglages {
        #[serde(default, deserialize_with = "texte_ou_vide")]
        selling_price_list: String,
    }
    match dokos.lire_memorise::<Reglages>(DOCTYPE_REGLAGES_VENTE, DOCTYPE_REGLAGES_VENTE) {
        Ok(r) if r.selling_price_list.trim().is_empty() => Ok(None),
        Ok(r) => Ok(Some(r.selling_price_list)),
        Err(DokosError::Permission(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Prix indicatifs des articles donnés dans une liste de prix. Les articles
/// sans prix sont absents du résultat ; une permission manquante donne une
/// liste vide, jamais une erreur.
pub fn prix_indicatifs<T: Transport>(
    dokos: &Dokos<T>,
    liste_de_prix: &str,
    codes: &[&str],
) -> Result<Vec<Prix>, DokosError> {
    if codes.is_empty() || liste_de_prix.trim().is_empty() {
        return Ok(Vec::new());
    }
    let liste = Liste::new(&["item_code", "price_list_rate", "currency", "price_list"])
        .filtre(Filtre::egal("price_list", liste_de_prix))
        .filtre(Filtre::egal("selling", 1))
        .filtre(Filtre::parmi("item_code", codes))
        .ordre("valid_from desc")
        .limite(500);
    match dokos.lister::<Prix>(DOCTYPE_PRIX, &liste) {
        Ok(mut prix) => {
            // Un seul prix par article : le plus récent (ordre de la requête).
            let mut vus = std::collections::BTreeSet::new();
            prix.retain(|p| vus.insert(p.article.clone()));
            Ok(prix)
        }
        Err(DokosError::Permission(_)) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Ce qu'a fait [`enregistrer_prix_client`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrixClient {
    /// Nouveau prix de l'article pour ce client.
    Cree { liste: String },
    /// Le prix de l'article pour ce client existait : il a été remplacé.
    Modifie { liste: String },
    /// Le prix enregistré était déjà celui-là : rien n'a été écrit.
    Inchange { liste: String },
}

impl PrixClient {
    /// Liste de prix concernée.
    pub fn liste(&self) -> &str {
        match self {
            Self::Cree { liste } | Self::Modifie { liste } | Self::Inchange { liste } => liste,
        }
    }
}

/// Enregistre `prix` comme prix de vente de `article` pour `client` seulement
/// (`Item Price` lié au client), dans la liste de prix du client ou, à
/// défaut, dans la liste de prix de vente par défaut. Le prix des autres
/// clients ne change pas. Un prix déjà enregistré pour ce client est remplacé.
pub fn enregistrer_prix_client<T: Transport>(
    dokos: &Dokos<T>,
    client: &str,
    article: &str,
    prix: f64,
) -> Result<PrixClient, DokosError> {
    #[derive(Deserialize)]
    struct ListeDuClient {
        #[serde(default, deserialize_with = "texte_ou_vide")]
        default_price_list: String,
    }
    #[derive(Deserialize)]
    struct Existant {
        name: String,
        #[serde(default)]
        price_list_rate: f64,
    }
    #[derive(Deserialize)]
    struct Ecrit {}

    let (client, article) = (client.trim(), article.trim());
    if client.is_empty() || article.is_empty() || !prix.is_finite() || prix < 0.0 {
        return Err(DokosError::Configuration("prix client : client, article ou prix invalide".to_owned()));
    }
    let du_client = dokos.lire::<ListeDuClient>(crate::types::DOCTYPE_CLIENT, client)?.default_price_list;
    let liste = if du_client.trim().is_empty() {
        liste_de_prix_par_defaut(dokos)?.ok_or_else(|| {
            DokosError::Configuration(
                "aucune liste de prix de vente : ni sur le client, ni dans les réglages de vente de Dokos".to_owned(),
            )
        })?
    } else {
        du_client
    };
    let existants: Vec<Existant> = dokos.lister(
        DOCTYPE_PRIX,
        &Liste::new(&["name", "price_list_rate"])
            .filtre(Filtre::egal("item_code", article))
            .filtre(Filtre::egal("price_list", liste.as_str()))
            .filtre(Filtre::egal("customer", client))
            .filtre(Filtre::egal("selling", 1))
            .ordre("valid_from desc")
            .limite(1),
    )?;
    match existants.first() {
        Some(e) if (e.price_list_rate - prix).abs() < 0.000_001 => Ok(PrixClient::Inchange { liste }),
        Some(e) => {
            let _: Ecrit = dokos.modifier(DOCTYPE_PRIX, &e.name, json!({"price_list_rate": prix}))?;
            Ok(PrixClient::Modifie { liste })
        }
        None => {
            let _: Ecrit = dokos.creer(
                DOCTYPE_PRIX,
                json!({
                    "item_code": article,
                    "price_list": liste,
                    "customer": client,
                    "selling": 1,
                    "price_list_rate": prix,
                }),
            )?;
            Ok(PrixClient::Cree { liste })
        }
    }
}

pub fn lister_unites<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<String>, DokosError> {
    lister_noms(dokos, DOCTYPE_UNITE, vec![Filtre::egal("enabled", 1)], 500)
}

// --- Écriture ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurArticle {
    CodeVide,
    CodeTropLong(usize),
    CodeInvalide(String),
    LibelleVide,
    GroupeVide,
    UniteVide,
}

impl fmt::Display for ErreurArticle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodeVide => write!(f, "code article manquant"),
            Self::CodeTropLong(n) => write!(f, "code article trop long : {n} caractères (maximum {CODE_MAX})"),
            Self::CodeInvalide(c) => write!(f, "code article « {c} » : les caractères < > % / \\ ne sont pas permis"),
            Self::LibelleVide => write!(f, "libellé de l'article manquant"),
            Self::GroupeVide => write!(f, "groupe d'articles manquant"),
            Self::UniteVide => write!(f, "unité manquante"),
        }
    }
}

impl std::error::Error for ErreurArticle {}

/// Article à créer, désactivé, dans un groupe existant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NouvelArticle {
    code: String,
    libelle: String,
    groupe: String,
    unite: String,
    vendable: bool,
    stocke: bool,
    description: Option<String>,
}

impl NouvelArticle {
    /// `groupe` et `unite` sont des identifiants Dokos existants.
    pub fn new(code: &str, libelle: &str, groupe: &str, unite: &str) -> Result<Self, ErreurArticle> {
        let net = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
        let (code, libelle, groupe, unite) = (net(code), net(libelle), net(groupe), net(unite));
        if code.is_empty() {
            return Err(ErreurArticle::CodeVide);
        }
        let longueur = code.chars().count();
        if longueur > CODE_MAX {
            return Err(ErreurArticle::CodeTropLong(longueur));
        }
        if code.chars().any(|c| matches!(c, '<' | '>' | '%' | '/' | '\\')) {
            return Err(ErreurArticle::CodeInvalide(code));
        }
        if libelle.is_empty() {
            return Err(ErreurArticle::LibelleVide);
        }
        if groupe.is_empty() {
            return Err(ErreurArticle::GroupeVide);
        }
        if unite.is_empty() {
            return Err(ErreurArticle::UniteVide);
        }
        Ok(Self { code, libelle, groupe, unite, vendable: true, stocke: false, description: None })
    }

    pub fn vendable(mut self, oui: bool) -> Self {
        self.vendable = oui;
        self
    }

    /// Article suivi en stock (par défaut : prestation, non stockée).
    pub fn stocke(mut self, oui: bool) -> Self {
        self.stocke = oui;
        self
    }

    pub fn avec_description(mut self, description: Option<&str>) -> Self {
        self.description = description.map(str::trim).filter(|d| !d.is_empty()).map(str::to_owned);
        self
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn libelle(&self) -> &str {
        &self.libelle
    }

    pub fn groupe(&self) -> &str {
        &self.groupe
    }

    /// Corps JSON envoyé à Dokos. L'article est créé **désactivé**.
    pub fn corps(&self) -> Value {
        let mut corps = json!({
            "item_code": self.code,
            "item_name": self.libelle,
            "item_group": self.groupe,
            "stock_uom": self.unite,
            "is_sales_item": u8::from(self.vendable),
            "is_stock_item": u8::from(self.stocke),
            "include_item_in_manufacturing": 0,
            "disabled": 1,
        });
        if let Some(description) = &self.description {
            corps["description"] = json!(description);
        }
        corps
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArticleExistant {
    #[serde(rename = "name")]
    pub code: String,
    #[serde(rename = "item_name", default, deserialize_with = "texte_ou_vide")]
    pub libelle: String,
    #[serde(rename = "item_group", default, deserialize_with = "texte_ou_vide")]
    pub groupe: String,
    #[serde(default)]
    pub disabled: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationArticle {
    Cree(ArticleExistant),
    /// Rien n'a été écrit : même code, ou même libellé.
    DejaExistant(ArticleExistant),
}

/// Article de même code ou de même libellé, actif ou non.
pub fn chercher_article_identique<T: Transport>(
    dokos: &Dokos<T>,
    article: &NouvelArticle,
) -> Result<Option<ArticleExistant>, DokosError> {
    let liste = Liste::new(&["name", "item_name", "item_group", "disabled"])
        .ou(Filtre::egal("name", article.code.as_str()))
        .ou(Filtre::egal("item_name", article.libelle.as_str()))
        .limite(1);
    Ok(dokos.lister::<ArticleExistant>(DOCTYPE_ARTICLE, &liste)?.into_iter().next())
}

pub fn creer_article<T: Transport>(
    dokos: &Dokos<T>,
    article: &NouvelArticle,
    si_doublon: SiDoublon,
) -> Result<CreationArticle, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = chercher_article_identique(dokos, article)?
    {
        return Ok(CreationArticle::DejaExistant(existant));
    }
    let cree: ArticleExistant = dokos.creer(DOCTYPE_ARTICLE, article.corps())?;
    if cree.disabled != 1 {
        return Err(DokosError::ReponseInvalide(format!("l'article {} a été créé actif", cree.code)));
    }
    Ok(CreationArticle::Cree(cree))
}

/// Active un article créé désactivé.
pub fn activer_article<T: Transport>(dokos: &Dokos<T>, code: &str) -> Result<ArticleExistant, DokosError> {
    dokos.modifier(DOCTYPE_ARTICLE, code, json!({"disabled": 0}))
}

/// Déplace un article dans un autre groupe.
pub fn deplacer_article<T: Transport>(
    dokos: &Dokos<T>,
    code: &str,
    groupe: &str,
) -> Result<ArticleExistant, DokosError> {
    let groupe = groupe.trim();
    if groupe.is_empty() {
        return Err(DokosError::Configuration("groupe d'articles vide".to_owned()));
    }
    dokos.modifier(DOCTYPE_ARTICLE, code, json!({"item_group": groupe}))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreationGroupe {
    Cree(GroupeArticle),
    DejaExistant(GroupeArticle),
}

/// Crée un sous-groupe sous un groupe existant ; rien n'est créé si le nom existe.
pub fn creer_groupe<T: Transport>(
    dokos: &Dokos<T>,
    nom: &str,
    parent: &str,
    peut_contenir_des_groupes: bool,
) -> Result<CreationGroupe, DokosError> {
    let nom = nom.split_whitespace().collect::<Vec<_>>().join(" ");
    let parent = parent.trim();
    if nom.is_empty() || parent.is_empty() {
        return Err(DokosError::Configuration("nom du groupe ou groupe parent vide".to_owned()));
    }
    let liste =
        Liste::new(&["name", "parent_item_group", "is_group"]).filtre(Filtre::egal("name", nom.as_str())).limite(1);
    if let Some(existant) = dokos.lister::<GroupeArticle>(DOCTYPE_GROUPE, &liste)?.into_iter().next() {
        return Ok(CreationGroupe::DejaExistant(existant));
    }
    let cree: GroupeArticle = dokos.creer(
        DOCTYPE_GROUPE,
        json!({"item_group_name": nom, "parent_item_group": parent, "is_group": u8::from(peut_contenir_des_groupes)}),
    )?;
    Ok(CreationGroupe::Cree(cree))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn groupe(id: &str, parent: &str, is_group: u8) -> GroupeArticle {
        GroupeArticle { id: id.into(), parent: parent.into(), is_group }
    }

    #[test]
    fn arbre_reconstruit() {
        let groupes = vec![
            groupe("Tous les groupes", "", 1),
            groupe("Restauration", "Tous les groupes", 1),
            groupe("Repas", "Restauration", 0),
            groupe("Boissons", "Restauration", 0),
            groupe("Salles", "Tous les groupes", 0),
            groupe("Orphelin", "Inconnu", 0),
        ];
        let arbre = construire_arbre(&groupes);
        assert_eq!(arbre.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["Orphelin", "Tous les groupes"]);
        let racine = &arbre[1];
        assert_eq!(racine.enfants.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["Restauration", "Salles"]);
        assert_eq!(racine.enfants[0].enfants.len(), 2);
        assert!(racine.enfants[0].groupe && !racine.enfants[1].groupe);
        assert_eq!(racine.identifiants().len(), 5);
    }

    #[test]
    fn cycle_sans_boucle_infinie() {
        let groupes = vec![groupe("A", "B", 1), groupe("B", "A", 1)];
        let arbre = construire_arbre(&groupes);
        assert!(arbre.is_empty() || arbre.iter().all(|n| n.identifiants().len() <= PROFONDEUR_MAX + 2));
    }

    #[test]
    fn articles_d_un_groupe() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "REPAS", "item_name": "Repas", "stock_uom": "Unité", "item_group": "Repas"}]}),
        );
        let articles = articles_du_groupe(&Dokos::new(&faux), "Restauration", Some("rep"), 50).expect("articles");
        assert_eq!(articles[0].groupe, "Repas");
        let requete = &faux.requetes()[0];
        assert!(requete.parametre("filters").is_some_and(|f| f.contains("descendants of (inclusive)")));
        assert!(requete.parametre("or_filters").is_some_and(|f| f.contains("%rep%")));
        assert!(articles_du_groupe(&Dokos::new(&faux), " ", None, 50).is_err());
    }

    #[test]
    fn prix_indicatifs_sans_droit() {
        let faux = FauxTransport::new();
        faux.repondre(403, "{}");
        assert_eq!(prix_indicatifs(&Dokos::new(&faux), "Vente standard", &["REPAS"]).expect("prix"), vec![]);
        assert!(prix_indicatifs(&Dokos::new(&faux), "Vente standard", &[]).expect("vide").is_empty());
        assert_eq!(faux.requetes().len(), 1);
    }

    #[test]
    fn prix_client_cree_modifie_ou_inchange() {
        // Liste du client ; aucun prix pour lui : création.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"default_price_list": "Tarif mairies"}}));
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "PRIX-1"}}));
        let fait = enregistrer_prix_client(&Dokos::new(&faux), "CUST-1", "REPAS", 12.5).expect("création");
        assert_eq!(fait, PrixClient::Cree { liste: "Tarif mairies".into() });
        let requetes = faux.requetes();
        assert_eq!(requetes[2].methode, Methode::Post);
        let corps = requetes[2].corps.as_ref().expect("corps");
        assert_eq!(corps["customer"], "CUST-1");
        assert_eq!(corps["item_code"], "REPAS");
        assert_eq!(corps["price_list"], "Tarif mairies");
        assert_eq!(corps["price_list_rate"], 12.5);
        assert_eq!(corps["selling"], 1);

        // Client sans liste : liste par défaut ; un prix existe : modification.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"default_price_list": null}}));
        faux.repondre_json(200, &json!({"data": {"selling_price_list": "Vente standard"}}));
        faux.repondre_json(200, &json!({"data": [{"name": "PRIX-7", "price_list_rate": 11.0}]}));
        faux.repondre_json(200, &json!({"data": {"name": "PRIX-7"}}));
        let fait = enregistrer_prix_client(&Dokos::new(&faux), "CUST-1", "REPAS", 12.5).expect("modification");
        assert_eq!(fait, PrixClient::Modifie { liste: "Vente standard".into() });
        let requetes = faux.requetes();
        assert_eq!(requetes[3].methode, Methode::Put);
        assert_eq!(requetes[3].chemin, "/api/resource/Item%20Price/PRIX-7");
        assert_eq!(requetes[3].corps.as_ref().expect("corps")["price_list_rate"], 12.5);

        // Même prix : aucune écriture.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"default_price_list": "Tarif mairies"}}));
        faux.repondre_json(200, &json!({"data": [{"name": "PRIX-7", "price_list_rate": 12.5}]}));
        let fait = enregistrer_prix_client(&Dokos::new(&faux), "CUST-1", "REPAS", 12.5).expect("inchangé");
        assert_eq!(fait, PrixClient::Inchange { liste: "Tarif mairies".into() });
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));

        // Aucune liste de prix : rien n'est écrit.
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {}}));
        faux.repondre_json(200, &json!({"data": {"selling_price_list": null}}));
        assert!(matches!(
            enregistrer_prix_client(&Dokos::new(&faux), "CUST-1", "REPAS", 12.5),
            Err(DokosError::Configuration(_))
        ));
    }

    #[test]
    fn prix_dedoublonnes() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [
                {"item_code": "REPAS", "price_list_rate": 12.5, "currency": "EUR", "price_list": "Vente standard"},
                {"item_code": "REPAS", "price_list_rate": 11.0, "currency": "EUR", "price_list": "Vente standard"},
            ]}),
        );
        let prix = prix_indicatifs(&Dokos::new(&faux), "Vente standard", &["REPAS", "SALLE"]).expect("prix");
        assert_eq!(prix.len(), 1);
        assert!((prix[0].prix - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn liste_de_prix() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"selling_price_list": "Vente standard"}}));
        faux.repondre(403, "{}");
        faux.repondre_json(200, &json!({"data": {"selling_price_list": null}}));
        let dokos = Dokos::new(&faux);
        assert_eq!(liste_de_prix_par_defaut(&dokos).expect("liste"), Some("Vente standard".into()));
        assert_eq!(liste_de_prix_par_defaut(&dokos).expect("sans droit"), None);
        assert_eq!(liste_de_prix_par_defaut(&dokos).expect("vide"), None);
        assert_eq!(faux.requetes()[0].chemin, "/api/resource/Selling%20Settings/Selling%20Settings");
    }

    #[test]
    fn nouvel_article_valide_et_desactive() {
        let article = NouvelArticle::new(" CAFE ", "Café", "Boissons", "Unité").expect("article");
        let corps = article.corps();
        assert_eq!(corps["item_code"], "CAFE");
        assert_eq!(corps["disabled"], 1);
        assert_eq!(corps["is_stock_item"], 0);
        assert_eq!(corps["is_sales_item"], 1);
        assert!(corps.get("description").is_none());
        assert_eq!(NouvelArticle::new("", "x", "g", "u"), Err(ErreurArticle::CodeVide));
        assert_eq!(NouvelArticle::new("a/b", "x", "g", "u"), Err(ErreurArticle::CodeInvalide("a/b".into())));
        assert_eq!(NouvelArticle::new("A", " ", "g", "u"), Err(ErreurArticle::LibelleVide));
        assert_eq!(NouvelArticle::new("A", "x", "", "u"), Err(ErreurArticle::GroupeVide));
        assert_eq!(NouvelArticle::new("A", "x", "g", ""), Err(ErreurArticle::UniteVide));
    }

    #[test]
    fn creation_puis_activation() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "CAFE", "item_name": "Café", "disabled": 1}}));
        faux.repondre_json(200, &json!({"data": {"name": "CAFE", "item_name": "Café", "disabled": 0}}));
        let dokos = Dokos::new(&faux);
        let article = NouvelArticle::new("CAFE", "Café", "Boissons", "Unité").expect("article");
        let creation = creer_article(&dokos, &article, SiDoublon::Reutiliser).expect("création");
        assert!(matches!(creation, CreationArticle::Cree(a) if a.code == "CAFE"));
        let actif = activer_article(&dokos, "CAFE").expect("activation");
        assert_eq!(actif.disabled, 0);
        let requetes = faux.requetes();
        assert_eq!(requetes[1].methode, Methode::Post);
        assert_eq!(requetes[2].methode, Methode::Put);
        assert_eq!(requetes[2].corps, Some(json!({"disabled": 0})));
    }

    #[test]
    fn doublon_d_article() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CAFE", "item_name": "Café", "disabled": 0}]}));
        let article = NouvelArticle::new("CAFE2", "Café", "Boissons", "Unité").expect("article");
        let creation = creer_article(&Dokos::new(&faux), &article, SiDoublon::Reutiliser).expect("doublon");
        assert!(matches!(creation, CreationArticle::DejaExistant(a) if a.code == "CAFE"));
        assert_eq!(faux.requetes().len(), 1);
    }

    #[test]
    fn article_cree_actif_signale() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "CAFE", "disabled": 0}}));
        let article = NouvelArticle::new("CAFE", "Café", "Boissons", "Unité").expect("article");
        let resultat = creer_article(&Dokos::new(&faux), &article, SiDoublon::Reutiliser);
        assert!(matches!(resultat, Err(DokosError::ReponseInvalide(_))));
    }

    #[test]
    fn sous_groupe() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "Goûters", "parent_item_group": "Restauration", "is_group": 0}}),
        );
        faux.repondre_json(200, &json!({"data": [{"name": "Goûters", "parent_item_group": "Restauration"}]}));
        let dokos = Dokos::new(&faux);
        let cree = creer_groupe(&dokos, " Goûters ", "Restauration", false).expect("groupe");
        assert!(matches!(cree, CreationGroupe::Cree(g) if g.id == "Goûters"));
        assert_eq!(faux.requetes()[1].corps.as_ref().map(|c| c["item_group_name"].clone()), Some(json!("Goûters")));
        let existant = creer_groupe(&dokos, "Goûters", "Restauration", false).expect("existant");
        assert!(matches!(existant, CreationGroupe::DejaExistant(_)));
        assert!(creer_groupe(&dokos, "", "Restauration", false).is_err());
    }

    #[test]
    fn deplacement() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"name": "CAFE", "item_group": "Boissons chaudes"}}));
        let deplace = deplacer_article(&Dokos::new(&faux), "CAFE", "Boissons chaudes").expect("déplacement");
        assert_eq!(deplace.groupe, "Boissons chaudes");
        assert!(deplacer_article(&Dokos::new(&faux), "CAFE", " ").is_err());
    }
}
