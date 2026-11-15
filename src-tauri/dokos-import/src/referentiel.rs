//! Listes de valeurs valides lues dans Dokos : jointes au modèle (listes
//! déroulantes) et affichées à l'utilisateur. L'import, lui, résout chaque
//! valeur contre Dokos au moment de l'analyse.

use std::collections::BTreeMap;

use dokos_core::article::{lister_groupes, lister_unites};
use dokos_core::client::{Dokos, Filtre, Liste};
use dokos_core::clientele::{lister_groupes_clients, lister_territoires};
use dokos_core::contrat::lister_types_contrat;
use dokos_core::date::Date;
use dokos_core::employe::{lister_genres, lister_postes};
use dokos_core::erreur::DokosError;
use dokos_core::formulaire::{DOCTYPE_INSCRIPTION, questions_possibles};
use dokos_core::inscription::lister_evenements;
use dokos_core::meta::{Meta, lire_meta};
use dokos_core::modele_vente::ModeleVente;
use dokos_core::transport::Transport;
use dokos_core::types::{Article, CHAMPS_ARTICLE, CHAMPS_CLIENT, Client, DOCTYPE_ARTICLE, DOCTYPE_CLIENT};

use crate::cas::Cas;

pub const LISTE_CLIENTS: &str = "Clients";
pub const LISTE_ARTICLES: &str = "Articles";
pub const LISTE_EVENEMENTS: &str = "Événements";
pub const LISTE_QUESTIONS: &str = "Questions";
pub const LISTE_GROUPES_ARTICLES: &str = "Groupes d'articles";
pub const LISTE_UNITES: &str = "Unités";
pub const LISTE_GENRES: &str = "Genres";
pub const LISTE_POSTES: &str = "Postes";
pub const LISTE_TYPES_CONTRAT: &str = "Types de contrat";
pub const LISTE_GROUPES_CLIENTS: &str = "Groupes de clients";
pub const LISTE_TERRITOIRES: &str = "Territoires";

/// Nombre maximal d'éléments par liste jointe au modèle (lue page par page).
const ELEMENTS_MAX: usize = 10_000;

/// Nombre maximal d'événements proposés (à venir ou récents : une page suffit).
const EVENEMENTS_MAX: u32 = 500;

/// Listes nommées de couples (valeur, libellé).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Referentiel {
    listes: BTreeMap<String, Vec<(String, String)>>,
    /// Structure d'`Event Registration`, pour les inscriptions et les événements.
    pub meta_inscription: Option<Meta>,
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

    /// Nom technique d'une question du formulaire d'après son libellé (ou son nom technique).
    pub fn champ_question(&self, libelle: &str) -> Option<&str> {
        let cible = dokos_core::recherche::normaliser(libelle);
        self.liste(LISTE_QUESTIONS)
            .iter()
            .find(|(champ, lib)| dokos_core::recherche::normaliser(lib) == cible || champ == libelle)
            .map(|(champ, _)| champ.as_str())
    }

    /// Lit dans Dokos les listes utiles au cas donné.
    pub fn lire<T: Transport>(dokos: &Dokos<T>, cas: Cas, aujourdhui: Date) -> Result<Self, DokosError> {
        let mut referentiel = Self::default();
        let simples = |noms: Vec<String>| noms.into_iter().map(|n| (n.clone(), n)).collect::<Vec<_>>();
        match cas {
            Cas::DevisModele | Cas::CommandesModele | Cas::FacturesModele => referentiel.lire_clients(dokos)?,
            Cas::Factures | Cas::Commandes | Cas::Devis => {
                // Deux grandes listes indépendantes : lues en même temps, page par page.
                let (clients, articles) = dokos.joindre(
                    || lister_clients(dokos),
                    || {
                        dokos.lister_tout::<Article>(
                            DOCTYPE_ARTICLE,
                            &Liste::new(CHAMPS_ARTICLE)
                                .filtre(Filtre::egal("disabled", 0))
                                .filtre(Filtre::egal("is_sales_item", 1))
                                .filtre(Filtre::egal("has_variants", 0))
                                .ordre("item_group asc, item_name asc"),
                            ELEMENTS_MAX,
                        )
                    },
                );
                referentiel.ajouter(LISTE_CLIENTS, clients?);
                let articles = articles?;
                referentiel.ajouter(
                    LISTE_ARTICLES,
                    articles
                        .into_iter()
                        .map(|a| {
                            let detail = [a.libelle.as_str(), a.unite.as_str(), a.groupe.as_str()]
                                .iter()
                                .filter(|s| !s.is_empty())
                                .copied()
                                .collect::<Vec<_>>()
                                .join(" — ");
                            (a.code, detail)
                        })
                        .collect(),
                );
            }
            Cas::Inscriptions => {
                let evenements = lister_evenements(
                    dokos,
                    &aujourdhui.plus_jours(-30).unwrap_or(aujourdhui).to_string(),
                    EVENEMENTS_MAX,
                )?;
                referentiel.ajouter(
                    LISTE_EVENEMENTS,
                    evenements.into_iter().map(|e| (e.id, format!("{} — {}", e.titre, e.debut))).collect(),
                );
                referentiel.lire_questions(dokos)?;
            }
            Cas::Evenements => referentiel.lire_questions(dokos)?,
            Cas::Articles => {
                let groupes = lister_groupes(dokos)?.into_iter().filter(|g| g.is_group == 0).map(|g| g.id).collect();
                referentiel.ajouter(LISTE_GROUPES_ARTICLES, simples(groupes));
                referentiel.ajouter(LISTE_UNITES, simples(lister_unites(dokos)?));
            }
            Cas::Employes => {
                referentiel.ajouter(LISTE_GENRES, simples(lister_genres(dokos)?));
                referentiel.ajouter(LISTE_POSTES, simples(lister_postes(dokos)?));
                referentiel.ajouter(LISTE_TYPES_CONTRAT, simples(lister_types_contrat(dokos)?));
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

    fn lire_questions<T: Transport>(&mut self, dokos: &Dokos<T>) -> Result<(), DokosError> {
        let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION)?;
        let questions =
            questions_possibles(&meta).iter().map(|c| (c.fieldname.clone(), c.libelle().to_owned())).collect();
        self.ajouter(LISTE_QUESTIONS, questions);
        self.meta_inscription = Some(meta);
        Ok(())
    }
}

/// Tous les clients actifs (identifiant, nom), par ordre alphabétique.
fn lister_clients<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<(String, String)>, DokosError> {
    let clients: Vec<Client> = dokos.lister_tout(
        DOCTYPE_CLIENT,
        &Liste::new(CHAMPS_CLIENT).filtre(Filtre::egal("disabled", 0)).ordre("customer_name asc"),
        ELEMENTS_MAX,
    )?;
    Ok(clients.into_iter().map(|c| (c.id, c.nom)).collect())
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
        let r = Referentiel::lire(&Dokos::new(&faux), Cas::Factures, "2026-11-03".parse().expect("date"))
            .expect("référentiel");
        assert_eq!(r.liste(LISTE_CLIENTS), [("CUST-1".to_owned(), "Mairie".to_owned())]);
        assert_eq!(r.liste(LISTE_ARTICLES)[0].1, "Repas — Unité — Restauration");
        assert!(r.liste("Inconnue").is_empty());
    }

    #[test]
    fn questions_par_libelle() {
        let mut r = Referentiel::default();
        r.ajouter(LISTE_QUESTIONS, vec![("age".into(), "Âge de l'enfant".into())]);
        assert_eq!(r.champ_question("age de l'enfant"), Some("age"));
        assert_eq!(r.champ_question("age"), Some("age"));
        assert_eq!(r.champ_question("taille"), None);
    }
}
