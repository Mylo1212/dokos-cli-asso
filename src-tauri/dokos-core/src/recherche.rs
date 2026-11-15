//! Recherche de clients et d'articles, et interprétation des résultats.
//!
//! Deux modes : la recherche par texte ([`chercher_client`], [`chercher_article`]),
//! limitée à [`LIMITE_RECHERCHE`] résultats, et la lecture complète
//! ([`lister_clients`], [`lister_articles`]) qui alimente les listes déroulantes
//! de l'appli et les listes jointes aux modèles d'import.

use crate::client::{Dokos, Filtre, Liste};
use crate::erreur::DokosError;
use crate::transport::Transport;
use crate::types::{Article, CHAMPS_ARTICLE, CHAMPS_CLIENT, Client, DOCTYPE_ARTICLE, DOCTYPE_CLIENT};

/// Nombre maximal de candidats renvoyés par une recherche.
pub const LIMITE_RECHERCHE: u32 = 10;
/// Nombre maximal d'éléments d'une lecture complète (lue page par page).
pub const LISTE_MAX: usize = 10_000;
/// Longueur maximale, en caractères, d'un texte de recherche.
const LONGUEUR_MAX: usize = 100;

/// Clients actifs dont l'identifiant ou le nom contient `texte`.
pub fn chercher_client<T: Transport>(dokos: &Dokos<T>, texte: &str) -> Result<Vec<Client>, DokosError> {
    let motif = motif_recherche(texte)?;
    let liste = Liste::new(CHAMPS_CLIENT)
        .filtre(Filtre::egal("disabled", 0))
        .ou(Filtre::contient("name", &motif))
        .ou(Filtre::contient("customer_name", &motif))
        .ordre("customer_name asc")
        .limite(LIMITE_RECHERCHE);
    dokos.lister(DOCTYPE_CLIENT, &liste)
}

/// Articles actifs et vendables dont le code ou le libellé contient `texte`.
/// Les modèles à variantes sont exclus : seule une variante peut être commandée.
pub fn chercher_article<T: Transport>(dokos: &Dokos<T>, texte: &str) -> Result<Vec<Article>, DokosError> {
    chercher_article_dans(dokos, texte, None)
}

/// Même recherche, limitée à un groupe d'articles et à ses sous-groupes.
pub fn chercher_article_dans<T: Transport>(
    dokos: &Dokos<T>,
    texte: &str,
    groupe: Option<&str>,
) -> Result<Vec<Article>, DokosError> {
    let motif = motif_recherche(texte)?;
    let mut liste = Liste::new(CHAMPS_ARTICLE)
        .filtre(Filtre::egal("disabled", 0))
        .filtre(Filtre::egal("is_sales_item", 1))
        .filtre(Filtre::egal("has_variants", 0));
    if let Some(groupe) = groupe.map(str::trim).filter(|g| !g.is_empty()) {
        liste = liste.filtre(Filtre::descendant_de("item_group", groupe));
    }
    let liste = liste
        .ou(Filtre::contient("name", &motif))
        .ou(Filtre::contient("item_name", &motif))
        .ordre("item_name asc")
        .limite(LIMITE_RECHERCHE);
    dokos.lister(DOCTYPE_ARTICLE, &liste)
}

/// Tous les clients actifs, par ordre alphabétique (au plus [`LISTE_MAX`]).
pub fn lister_clients<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<Client>, DokosError> {
    dokos.lister_tout(
        DOCTYPE_CLIENT,
        &Liste::new(CHAMPS_CLIENT).filtre(Filtre::egal("disabled", 0)).ordre("customer_name asc"),
        LISTE_MAX,
    )
}

/// Tous les articles actifs et vendables, hors modèles à variantes, par groupe
/// puis par libellé (au plus [`LISTE_MAX`]). Avec `groupe` : ce groupe et ses sous-groupes.
pub fn lister_articles<T: Transport>(dokos: &Dokos<T>, groupe: Option<&str>) -> Result<Vec<Article>, DokosError> {
    let mut liste = Liste::new(CHAMPS_ARTICLE)
        .filtre(Filtre::egal("disabled", 0))
        .filtre(Filtre::egal("is_sales_item", 1))
        .filtre(Filtre::egal("has_variants", 0));
    if let Some(groupe) = groupe.map(str::trim).filter(|g| !g.is_empty()) {
        liste = liste.filtre(Filtre::descendant_de("item_group", groupe));
    }
    dokos.lister_tout(DOCTYPE_ARTICLE, &liste.ordre("item_group asc, item_name asc"), LISTE_MAX)
}

pub fn trouver_client<T: Transport>(dokos: &Dokos<T>, texte: &str) -> Result<Correspondance<Client>, DokosError> {
    Ok(resoudre(texte, chercher_client(dokos, texte)?))
}

pub fn trouver_article<T: Transport>(dokos: &Dokos<T>, texte: &str) -> Result<Correspondance<Article>, DokosError> {
    Ok(resoudre(texte, chercher_article(dokos, texte)?))
}

/// Résultat d'une recherche, du point de vue de l'utilisateur.
#[derive(Debug, Clone, PartialEq)]
pub enum Correspondance<C> {
    /// Rien trouvé : à signaler, jamais à créer en silence.
    Aucune,
    Unique(C),
    /// Plusieurs candidats : l'utilisateur choisit.
    Plusieurs(Vec<C>),
}

/// Document qui peut être retrouvé par son identifiant ou son nom.
pub trait Candidat {
    fn identifiant(&self) -> &str;
    fn nom(&self) -> &str;
}

impl Candidat for Client {
    fn identifiant(&self) -> &str {
        &self.id
    }

    fn nom(&self) -> &str {
        &self.nom
    }
}

impl Candidat for Article {
    fn identifiant(&self) -> &str {
        &self.code
    }

    fn nom(&self) -> &str {
        &self.libelle
    }
}

/// `Unique` s'il n'y a qu'un candidat, ou un seul dont l'identifiant ou le nom
/// est exactement `texte` (casse, accents et ponctuation ignorés).
pub fn resoudre<C: Candidat>(texte: &str, mut candidats: Vec<C>) -> Correspondance<C> {
    match candidats.len() {
        0 => return Correspondance::Aucune,
        1 => return candidats.pop().map_or(Correspondance::Aucune, Correspondance::Unique),
        _ => {}
    }
    let cible = normaliser(texte);
    let exacts: Vec<usize> = candidats
        .iter()
        .enumerate()
        .filter(|(_, c)| normaliser(c.identifiant()) == cible || normaliser(c.nom()) == cible)
        .map(|(i, _)| i)
        .collect();
    match exacts.as_slice() {
        [i] => Correspondance::Unique(candidats.swap_remove(*i)),
        _ => Correspondance::Plusieurs(candidats),
    }
}

/// Texte de recherche nettoyé : espaces compactés, `%` retirés, longueur bornée.
pub fn motif_recherche(texte: &str) -> Result<String, DokosError> {
    let motif = texte.replace('%', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    if motif.is_empty() {
        return Err(DokosError::Configuration("texte de recherche vide".to_owned()));
    }
    Ok(motif.chars().take(LONGUEUR_MAX).collect())
}

/// Minuscules, sans accents, ponctuation remplacée par des espaces compactés.
pub fn normaliser(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for c in texte.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => sortie.push('a'),
            'ç' => sortie.push('c'),
            'è' | 'é' | 'ê' | 'ë' => sortie.push('e'),
            'ì' | 'í' | 'î' | 'ï' => sortie.push('i'),
            'ñ' => sortie.push('n'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => sortie.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => sortie.push('u'),
            'ý' | 'ÿ' => sortie.push('y'),
            'œ' => sortie.push_str("oe"),
            'æ' => sortie.push_str("ae"),
            c if c.is_alphanumeric() => sortie.push(c),
            _ => sortie.push(' '),
        }
    }
    sortie.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::faux::FauxTransport;

    fn client(id: &str, nom: &str) -> Client {
        Client { id: id.into(), nom: nom.into() }
    }

    #[test]
    fn requete_de_recherche_client() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-1", "customer_name": "Mairie de Crest"}]}));
        let clients = chercher_client(&Dokos::new(&faux), "  mairie   de Crest ").expect("recherche");
        assert_eq!(clients, vec![client("CUST-1", "Mairie de Crest")]);

        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/resource/Customer");
        assert_eq!(requete.parametre("filters"), Some(r#"[["disabled","=",0]]"#));
        assert_eq!(
            requete.parametre("or_filters"),
            Some(r#"[["name","like","%mairie de Crest%"],["customer_name","like","%mairie de Crest%"]]"#)
        );
        assert_eq!(requete.parametre("limit_page_length"), Some("10"));
    }

    #[test]
    fn requete_de_recherche_article() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        let articles = chercher_article(&Dokos::new(&faux), "grande salle").expect("recherche");
        assert!(articles.is_empty());
        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/resource/Item");
        assert_eq!(
            requete.parametre("filters"),
            Some(r#"[["disabled","=",0],["is_sales_item","=",1],["has_variants","=",0]]"#)
        );
        faux.repondre_json(200, &json!({"data": []}));
        chercher_article_dans(&Dokos::new(&faux), "repas", Some("Restauration")).expect("recherche");
        assert!(faux.requetes()[1].parametre("filters").is_some_and(|f| f.contains("descendants of (inclusive)")));
    }

    #[test]
    fn lectures_completes() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-1", "customer_name": "Mairie de Crest"}]}));
        let clients = lister_clients(&Dokos::new(&faux)).expect("clients");
        assert_eq!(clients, vec![client("CUST-1", "Mairie de Crest")]);
        let requete = &faux.requetes()[0];
        assert_eq!(requete.parametre("filters"), Some(r#"[["disabled","=",0]]"#));
        assert_eq!(requete.parametre("order_by"), Some("customer_name asc"));
        assert_eq!(requete.parametre("or_filters"), None, "pas de texte cherché : tous les clients");

        faux.repondre_json(
            200,
            &json!({"data": [{"name": "REPAS", "item_name": "Repas", "stock_uom": "Unité", "item_group": "Restauration"}]}),
        );
        let articles = lister_articles(&Dokos::new(&faux), Some(" Restauration ")).expect("articles");
        assert_eq!(articles[0].code, "REPAS");
        let filtres = faux.requetes()[1].parametre("filters").unwrap_or_default().to_owned();
        assert!(filtres.contains(r#"["is_sales_item","=",1]"#) && filtres.contains("descendants of (inclusive)"));
    }

    #[test]
    fn recherche_vide_sans_requete() {
        let faux = FauxTransport::new();
        assert!(chercher_client(&Dokos::new(&faux), " % ").is_err());
        assert!(faux.requetes().is_empty());
    }

    #[test]
    fn motif_borne() {
        assert_eq!(motif_recherche(&"a".repeat(500)).map(|m| m.len()), Ok(LONGUEUR_MAX));
        assert_eq!(motif_recherche("100%  bio").as_deref(), Ok("100 bio"));
    }

    #[test]
    fn normalisation() {
        assert_eq!(normaliser("  Mairie  de CREST "), "mairie de crest");
        assert_eq!(normaliser("Écomusée Œuvre"), "ecomusee oeuvre");
        assert_eq!(normaliser("GRANDE-SALLE"), "grande salle");
        assert_eq!(normaliser("grande_salle"), "grande salle");
    }

    #[test]
    fn resolution_aucune_et_unique() {
        assert_eq!(resoudre::<Client>("x", vec![]), Correspondance::Aucune);
        assert_eq!(
            resoudre("crest", vec![client("C1", "Mairie de Crest")]),
            Correspondance::Unique(client("C1", "Mairie de Crest"))
        );
    }

    #[test]
    fn resolution_par_nom_exact() {
        let candidats = vec![client("C1", "CCAS de Crest"), client("C2", "Mairie de Crest")];
        assert_eq!(resoudre("mairie de crest", candidats), Correspondance::Unique(client("C2", "Mairie de Crest")));
    }

    #[test]
    fn resolution_par_identifiant_exact() {
        let candidats = vec![client("CUST-0001", "A"), client("CUST-00012", "B")];
        assert_eq!(resoudre("cust-0001", candidats), Correspondance::Unique(client("CUST-0001", "A")));
    }

    #[test]
    fn resolution_ambigue() {
        let candidats = vec![client("C1", "CCAS de Crest"), client("C2", "Mairie de Crest")];
        assert!(matches!(resoudre("crest", candidats), Correspondance::Plusieurs(v) if v.len() == 2));

        // Deux homonymes exacts : ambigu, pas de choix arbitraire.
        let homonymes = vec![client("C1", "Dupont"), client("C2", "Dupont")];
        assert!(matches!(resoudre("dupont", homonymes), Correspondance::Plusieurs(v) if v.len() == 2));
    }
}
