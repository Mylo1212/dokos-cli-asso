//! Catalogue : arbre des groupes d'articles, articles d'un groupe, prix
//! indicatifs, création (désactivée) et activation d'articles, sous-groupes.

use dokos_core::article::{
    ARTICLES_PAR_GROUPE_MAX, CreationArticle, CreationGroupe, Noeud, NouvelArticle, activer_article, arbre_des_groupes,
    articles_du_groupe, creer_article, creer_groupe, deplacer_article, liste_de_prix_par_defaut, lister_unites,
    prix_indicatifs,
};
use dokos_core::doublon::SiDoublon;
use dokos_core::types::DOCTYPE_ARTICLE;
use dokos_core::{Dokos, Transport};

use crate::erreurs::erreur as texte;
use crate::services::{Resultat, lien_desk, vue_article};
use crate::vues::*;

fn vue_noeud(n: &Noeud) -> NoeudVue {
    NoeudVue { id: n.id.clone(), groupe: n.groupe, enfants: n.enfants.iter().map(vue_noeud).collect() }
}

/// Arbre des groupes, liste de prix par défaut et unités (pour créer un article).
pub fn catalogue<T: Transport>(dokos: &Dokos<T>) -> Resultat<CatalogueVue> {
    // Trois lectures indépendantes : en même temps quand le transport le permet.
    let (arbre, (liste_de_prix, unites)) = dokos.joindre(
        || arbre_des_groupes(dokos),
        || dokos.joindre(|| liste_de_prix_par_defaut(dokos), || lister_unites(dokos).unwrap_or_default()),
    );
    let arbre = arbre.map_err(texte)?;
    let liste_de_prix = liste_de_prix.map_err(texte)?;
    Ok(CatalogueVue { arbre: arbre.iter().map(vue_noeud).collect(), liste_de_prix, unites })
}

pub fn articles_d_un_groupe<T: Transport>(
    dokos: &Dokos<T>,
    groupe: &str,
    texte_recherche: Option<&str>,
) -> Resultat<Vec<ArticleVue>> {
    let articles = articles_du_groupe(dokos, groupe, texte_recherche, ARTICLES_PAR_GROUPE_MAX).map_err(texte)?;
    Ok(articles.into_iter().map(vue_article).collect())
}

/// Prix indicatifs (liste de prix de vente par défaut). Vide si elle n'est pas
/// lisible : l'interface affiche alors « prix par Dokos ».
pub fn prix<T: Transport>(dokos: &Dokos<T>, codes: &[String]) -> Resultat<Vec<PrixVue>> {
    let Some(liste) = liste_de_prix_par_defaut(dokos).map_err(texte)? else { return Ok(Vec::new()) };
    let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
    let prix = prix_indicatifs(dokos, &liste, &codes).map_err(texte)?;
    Ok(prix.into_iter().map(|p| PrixVue { article: p.article, prix: p.prix, devise: p.devise }).collect())
}

pub fn creer_un_article<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeArticle,
    si_doublon: SiDoublon,
) -> Resultat<ResultatArticle> {
    let article = NouvelArticle::new(&demande.code, &demande.libelle, &demande.groupe, &demande.unite)
        .map_err(texte)?
        .vendable(demande.vendable)
        .stocke(demande.stocke)
        .avec_description(demande.description.as_deref());
    let (cree, existant) = match creer_article(dokos, &article, si_doublon).map_err(texte)? {
        CreationArticle::Cree(a) => (true, a),
        CreationArticle::DejaExistant(a) => (false, a),
    };
    Ok(ResultatArticle {
        cree,
        lien: lien_desk(base, DOCTYPE_ARTICLE, &existant.code),
        code: existant.code,
        libelle: existant.libelle,
        actif: existant.disabled == 0,
    })
}

pub fn activer_un_article<T: Transport>(dokos: &Dokos<T>, base: &str, code: &str) -> Resultat<ResultatArticle> {
    let a = activer_article(dokos, code).map_err(texte)?;
    Ok(ResultatArticle {
        cree: false,
        lien: lien_desk(base, DOCTYPE_ARTICLE, &a.code),
        code: a.code,
        libelle: a.libelle,
        actif: a.disabled == 0,
    })
}

pub fn deplacer_un_article<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    code: &str,
    groupe: &str,
) -> Resultat<ResultatArticle> {
    let a = deplacer_article(dokos, code, groupe).map_err(texte)?;
    Ok(ResultatArticle {
        cree: false,
        lien: lien_desk(base, DOCTYPE_ARTICLE, &a.code),
        code: a.code,
        libelle: a.libelle,
        actif: a.disabled == 0,
    })
}

pub fn creer_un_groupe<T: Transport>(
    dokos: &Dokos<T>,
    nom: &str,
    parent: &str,
    conteneur: bool,
) -> Resultat<ResultatGroupe> {
    Ok(match creer_groupe(dokos, nom, parent, conteneur).map_err(texte)? {
        CreationGroupe::Cree(g) => ResultatGroupe { cree: true, id: g.id },
        CreationGroupe::DejaExistant(g) => ResultatGroupe { cree: false, id: g.id },
    })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    #[test]
    fn catalogue_complet() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "Tous", "parent_item_group": "", "is_group": 1},
                             {"name": "Repas", "parent_item_group": "Tous", "is_group": 0}]}),
        );
        faux.repondre_json(200, &json!({"data": {"selling_price_list": "Vente standard"}}));
        faux.repondre_json(200, &json!({"data": [{"name": "Unité"}]}));
        let vue = catalogue(&Dokos::new(&faux)).expect("catalogue");
        assert_eq!(vue.arbre[0].id, "Tous");
        assert_eq!(vue.arbre[0].enfants[0].id, "Repas");
        assert_eq!(vue.liste_de_prix.as_deref(), Some("Vente standard"));
        assert_eq!(vue.unites, ["Unité"]);
    }

    #[test]
    fn prix_sans_liste() {
        let faux = FauxTransport::new();
        faux.repondre(403, "{}");
        assert!(prix(&Dokos::new(&faux), &["REPAS".into()]).expect("prix").is_empty());
    }

    #[test]
    fn article_cree_puis_active() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "CAFE", "item_name": "Café", "disabled": 1}}));
        faux.repondre_json(200, &json!({"data": {"name": "CAFE", "item_name": "Café", "disabled": 0}}));
        let dokos = Dokos::new(&faux);
        let demande = DemandeArticle {
            code: "CAFE".into(),
            libelle: "Café".into(),
            groupe: "Boissons".into(),
            unite: "Unité".into(),
            vendable: true,
            stocke: false,
            description: None,
        };
        let r = creer_un_article(&dokos, "https://d.fr", &demande, SiDoublon::Reutiliser).expect("article");
        assert!(r.cree && !r.actif);
        assert_eq!(r.lien, "https://d.fr/app/item/CAFE");
        let a = activer_un_article(&dokos, "https://d.fr", "CAFE").expect("activation");
        assert!(a.actif);
    }
}
