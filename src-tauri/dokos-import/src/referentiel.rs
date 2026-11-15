//! Listes de valeurs valides lues dans Dokos : jointes au modèle (listes
//! déroulantes) et affichées à l'utilisateur. L'import, lui, résout chaque
//! valeur contre Dokos au moment de l'analyse.

use std::collections::BTreeMap;

use dokos_core::article::{lister_groupes, lister_unites};
use dokos_core::client::Dokos;
use dokos_core::clientele::{lister_groupes_clients, lister_territoires};
use dokos_core::erreur::DokosError;
use dokos_core::modele_vente::ModeleVente;
use dokos_core::recherche::{lister_articles, lister_clients as lister_tous_les_clients};
use dokos_core::transport::Transport;
use dokos_core::types::Article;

use crate::cas::Cas;

pub const LISTE_CLIENTS: &str = "Clients";
pub const LISTE_ARTICLES: &str = "Articles";
pub const LISTE_GROUPES_ARTICLES: &str = "Groupes d'articles";
pub const LISTE_UNITES: &str = "Unités";
pub const LISTE_GROUPES_CLIENTS: &str = "Groupes de clients";
pub const LISTE_TERRITOIRES: &str = "Territoires";

/// Listes nommées de couples (valeur, libellé).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Referentiel {
    listes: BTreeMap<String, Vec<(String, String)>>,
    /// Modèle copié, pour les cas « à partir d'un modèle ».
    pub modele: Option<ModeleVente>,
}

impl Referentiel {
    pub fn ajouter(&mut self, nom: &str, elements: Vec<(String, String)>) {
        self.listes.insert(nom.to_owned(), elements);
    }

    pub fn liste(&self, nom: &str) -> &[(String, String)] {
        self.listes.get(nom).map(Vec::as_slice).unwrap_or_default()
    }

    pub fn noms(&self) -> impl Iterator<Item = &str> {
        self.listes.keys().map(String::as_str)
    }

    /// Lit dans Dokos les listes utiles au cas donné.
    pub fn lire<T: Transport>(dokos: &Dokos<T>, cas: Cas) -> Result<Self, DokosError> {
        let mut referentiel = Self::default();
        let simples = |noms: Vec<String>| noms.into_iter().map(|n| (n.clone(), n)).collect::<Vec<_>>();
        match cas {
            Cas::FacturesModele => referentiel.lire_clients(dokos)?,
            Cas::Factures => {
                // Deux grandes listes indépendantes : lues en même temps, page par page.
                let (clients, articles) = dokos.joindre(|| lister_clients(dokos), || lister_articles(dokos, None));
                referentiel.ajouter(LISTE_CLIENTS, clients?);
                referentiel.ajouter(LISTE_ARTICLES, articles?.into_iter().map(detail_article).collect());
            }
            Cas::Articles => {
                let groupes = lister_groupes(dokos)?.into_iter().filter(|g| g.is_group == 0).map(|g| g.id).collect();
                referentiel.ajouter(LISTE_GROUPES_ARTICLES, simples(groupes));
                referentiel.ajouter(LISTE_UNITES, simples(lister_unites(dokos)?));
            }
            Cas::Clients => {
                referentiel.ajouter(LISTE_GROUPES_CLIENTS, simples(lister_groupes_clients(dokos)?));
                referentiel.ajouter(LISTE_TERRITOIRES, simples(lister_territoires(dokos)?));
            }
        }
        Ok(referentiel)
    }

    fn lire_clients<T: Transport>(&mut self, dokos: &Dokos<T>) -> Result<(), DokosError> {
        self.ajouter(LISTE_CLIENTS, lister_clients(dokos)?);
        Ok(())
    }
}

/// Tous les clients actifs (identifiant, nom), par ordre alphabétique.
fn lister_clients<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<(String, String)>, DokosError> {
    Ok(lister_tous_les_clients(dokos)?.into_iter().map(|c| (c.id, c.nom)).collect())
}

/// Le détail d'un article n'est pas son libellé seul : libellé, unité et groupe.
fn detail_article(a: Article) -> (String, String) {
    let detail = [a.libelle.as_str(), a.unite.as_str(), a.groupe.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" — ");
    (a.code, detail)
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    #[test]
    fn referentiel_des_ventes() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-1", "customer_name": "Mairie"}]}));
        faux.repondre_json(200, &json!({"data": [{"name": "REPAS", "item_name": "Repas", "stock_uom": "Unité", "item_group": "Restauration"}]}));
        let r = Referentiel::lire(&Dokos::new(&faux), Cas::Factures).expect("référentiel");
        assert_eq!(r.liste(LISTE_CLIENTS), [("CUST-1".to_owned(), "Mairie".to_owned())]);
        assert_eq!(r.liste(LISTE_ARTICLES)[0].1, "Repas — Unité — Restauration");
        assert!(r.liste("Inconnue").is_empty());
    }
}
