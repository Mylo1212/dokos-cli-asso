//! Import de tableaux (Excel ou CSV) vérifiés, et modèles générés par l'outil.
//!
//! - [`cas`] : les cas d'usage (factures, commandes, devis, inscriptions,
//!   articles, employés, clients, programme d'événements, ventes à partir
//!   d'un modèle « Template ») et leurs colonnes.
//! - [`referentiel`] : les listes de valeurs valides lues dans Dokos, jointes
//!   au modèle et utilisées à l'import.
//! - [`modele`] : génération du classeur modèle (mode d'emploi, données,
//!   listes déroulantes, marque cachée du cas et de la version).
//! - [`lecture`] : lecture d'un fichier en tableau de textes, reconnaissance
//!   du modèle et des en-têtes.
//! - [`analyse`] : chaque ligne passe par les types de `dokos-core` et la
//!   résolution exacte (identifiant ou nom), puis les documents sont vérifiés
//!   (doublons) et créés, tout ou rien par défaut.
//! - [`rapport`] : fichier résultat (copie du tableau avec statut et document).
//!
//! Rien n'est écrit dans Dokos tant qu'une ligne pose problème, sauf si
//! l'utilisateur demande explicitement « seulement les documents prêts ».

pub mod analyse;
pub mod cas;
pub mod lecture;
pub mod modele;
pub mod rapport;
pub mod referentiel;

use std::fmt;

use dokos_core::DokosError;
use dokos_core::client::Dokos;
use dokos_core::date::Date;
use dokos_core::modele_vente::{ErreurModele, lire_modele, verifier_version};
use dokos_core::transport::Transport;

/// Version du format des modèles. À incrémenter quand les colonnes changent.
pub const VERSION_MODELE: u32 = 1;
/// Taille maximale d'un fichier importé.
pub const TAILLE_MAX: u64 = 5 * 1024 * 1024;
/// Nombre maximal de lignes de données.
pub const LIGNES_MAX: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurImport {
    Fichier(String),
    /// Le fichier n'est pas issu du modèle attendu, ou d'une version incompatible.
    Modele(String),
    Colonnes(String),
    /// Des lignes bloquent : rien n'a été créé.
    LignesBloquantes(usize),
    Dokos(DokosError),
}

impl fmt::Display for ErreurImport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fichier(m) => write!(f, "fichier : {m}"),
            Self::Modele(m) => write!(f, "modèle : {m}"),
            Self::Colonnes(m) => write!(f, "colonnes : {m}"),
            Self::LignesBloquantes(n) => {
                write!(
                    f,
                    "{n} ligne(s) à corriger : rien n'a été créé (ou choisissez « seulement les documents prêts »)"
                )
            }
            Self::Dokos(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ErreurImport {}

impl From<DokosError> for ErreurImport {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

/// Référentiel pour générer le modèle d'un cas. `modele` : identifiant du
/// document « Template » à copier, obligatoire pour les cas « à partir d'un modèle ».
pub fn referentiel_pour_generer<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    aujourdhui: Date,
    modele: Option<&str>,
) -> Result<Referentiel, ErreurImport> {
    let mut referentiel = Referentiel::lire(dokos, cas, aujourdhui)?;
    if let Some(type_vente) = cas.type_modele() {
        let id = modele.map(str::trim).filter(|m| !m.is_empty()).ok_or_else(|| {
            ErreurImport::Modele("choisissez le modèle Dokos (étiquette « Template ») à copier".into())
        })?;
        referentiel.modele = Some(lire_modele(dokos, type_vente, id).map_err(erreur_modele)?);
    }
    Ok(referentiel)
}

/// Référentiel pour analyser un fichier : pour un cas « à partir d'un
/// modèle », le modèle désigné par la marque du fichier, inchangé depuis.
pub fn referentiel_pour_fichier<T: Transport>(
    dokos: &Dokos<T>,
    cas: Cas,
    tableau: &Tableau,
    aujourdhui: Date,
) -> Result<Referentiel, ErreurImport> {
    let mut referentiel = Referentiel::lire(dokos, cas, aujourdhui)?;
    if let Some(type_vente) = cas.type_modele() {
        let marque = tableau.marque.as_ref();
        let id = marque.and_then(|m| m.modele.as_deref()).ok_or_else(|| {
            ErreurImport::Modele("ce fichier n'indique pas de modèle Dokos : régénérez-le depuis l'appli".into())
        })?;
        let modele = lire_modele(dokos, type_vente, id).map_err(erreur_modele)?;
        verifier_version(&modele, marque.and_then(|m| m.modele_modifie.as_deref()).unwrap_or(""))
            .map_err(erreur_modele)?;
        referentiel.modele = Some(modele);
    }
    Ok(referentiel)
}

fn erreur_modele(e: ErreurModele) -> ErreurImport {
    match e {
        ErreurModele::Dokos(d) => ErreurImport::Dokos(d),
        autre => ErreurImport::Modele(autre.to_string()),
    }
}

pub use analyse::{
    Analyse, DocumentPrepare, LigneAnalysee, Options, ResultatDocument, Statut, analyser, creer, verifier,
};
pub use cas::{Cas, Colonne, Genre};
pub use lecture::{Marque, Tableau, lire_fichier, lire_octets};
pub use modele::generer_modele;
pub use referentiel::Referentiel;

#[cfg(test)]
mod tests {
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;
    use crate::analyse::{OptionsCreation, StatutCreation};
    use crate::modele::ContexteModele;

    fn jour() -> Date {
        "2026-11-05".parse().expect("date")
    }

    fn clients(faux: &FauxTransport) {
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
    }

    /// Lecture du modèle, puis de la structure de son DocType.
    fn modele(faux: &FauxTransport, modifie: &str) {
        document_modele(faux, modifie);
        faux.repondre_json(
            200,
            &json!({"docs": [{"name": "Quotation", "fields": [
                {"fieldname": "custom_date_arrivee", "fieldtype": "Date"},
                {"fieldname": "custom_date_depart", "fieldtype": "Date"},
            ]}]}),
        );
    }

    /// Lecture du modèle seul : la structure du DocType est déjà en mémoire du client.
    fn document_modele(faux: &FauxTransport, modifie: &str) {
        faux.repondre_json(200, &json!({"data": [{"name": "SAL-QTN-0007", "_user_tags": ",Template"}]}));
        faux.repondre_json(
            200,
            &json!({"data": {
                "name": "SAL-QTN-0007", "title": "Séminaire", "docstatus": 0, "currency": "EUR",
                "modified": modifie, "taxes_and_charges": "TVA 10 %",
                "items": [
                    {"name": "l1", "item_code": "REPAS", "item_name": "Repas complet", "qty": 20, "rate": 18.5, "uom": "Unité"},
                    {"name": "l2", "item_code": "SALLE-G", "item_name": "Grande salle", "qty": 2, "rate": 300, "uom": "Jour"},
                ],
            }}),
        );
    }

    #[test]
    fn import_a_partir_d_un_modele() {
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);
        clients(&faux);
        modele(&faux, "2026-10-01 09:00:00");
        let referentiel =
            referentiel_pour_generer(&dokos, Cas::DevisModele, jour(), Some("SAL-QTN-0007")).expect("référentiel");
        let colonnes = Cas::DevisModele.colonnes(&referentiel);
        let noms: Vec<&str> = colonnes.iter().map(|c| c.nom.as_str()).collect();
        assert_eq!(
            noms,
            ["Client", "Date d'arrivée", "Date de départ", "Qté Repas complet (REPAS)", "Qté Grande salle (SALLE-G)"]
        );
        let contexte =
            ContexteModele { url_dokos: "https://dokos.test".into(), date: jour().to_string(), societe: None };
        let octets = generer_modele(Cas::DevisModele, &referentiel, &contexte).expect("modèle");
        let mut tableau = lire_octets(&octets, "xlsx").expect("lecture");
        let marque = tableau.marque.clone().expect("marque");
        assert_eq!(marque.modele.as_deref(), Some("SAL-QTN-0007"));
        assert_eq!(marque.modele_modifie.as_deref(), Some("2026-10-01 09:00:00"));
        tableau.verifier_marque(Cas::DevisModele).expect("marque du cas");

        // Deux lignes saisies : une complète, une sans aucune quantité.
        tableau.lignes = vec![
            (3, vec!["CUST-2".into(), "01/12/2026".into(), "03/12/2026".into(), "30".into(), String::new()]),
            (4, vec!["CUST-2".into(), "01/12/2026".into(), "03/12/2026".into(), "0".into(), String::new()]),
        ];
        clients(&faux);
        document_modele(&faux, "2026-10-01 09:00:00");
        let referentiel = referentiel_pour_fichier(&dokos, Cas::DevisModele, &tableau, jour()).expect("référentiel");
        // « CUST-2 » est reconnu dans la liste des clients du référentiel : aucune recherche.
        let options = Options { aujourdhui: Some(jour()), ..Options::default() };
        let mut analyse = analyser(&dokos, Cas::DevisModele, &tableau, &referentiel, &options).expect("analyse");
        assert_eq!(analyse.documents.len(), 2);
        assert!(analyse.documents[0].pret());
        assert!(analyse.documents[0].libelle.contains("Mairie de Crest"), "{}", analyse.documents[0].libelle);
        assert!(matches!(&analyse.lignes[1].statut, Statut::Invalide(m) if m.contains("aucune quantité")));

        faux.repondre_json(200, &json!({"data": []})); // doublons
        verifier(&dokos, &mut analyse).expect("vérification");
        faux.repondre_json(200, &json!({"data": []})); // doublons, à la création
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SAL-QTN-0050", "docstatus": 0, "grand_total": 610.5, "currency": "EUR"}}),
        );
        let resultats = creer(
            &dokos,
            &analyse,
            &OptionsCreation { seulement_prets: true, si_doublon: dokos_core::doublon::SiDoublon::Reutiliser },
        )
        .expect("création");
        assert_eq!(resultats[0].statut, StatutCreation::Cree);
        assert_eq!(resultats[1].statut, StatutCreation::Ignore);
        let post = faux.requetes().into_iter().find(|r| r.methode == Methode::Post).expect("POST");
        let corps = post.corps.expect("corps");
        assert_eq!(corps["party_name"], "CUST-2");
        assert_eq!(corps["transaction_date"], "2026-11-05");
        assert_eq!(corps["custom_date_depart"], "2026-12-03");
        assert_eq!(corps["taxes_and_charges"], "TVA 10 %");
        assert_eq!(corps["items"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            corps["items"][0],
            json!({"item_code": "REPAS", "item_name": "Repas complet", "rate": 18.5, "uom": "Unité", "qty": 30.0})
        );
        assert_eq!(faux.reponses_restantes(), 0);
    }

    #[test]
    fn modele_modifie_depuis_la_generation() {
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);
        let tableau = Tableau {
            marque: Some(Marque {
                cas: Cas::DevisModele,
                version: VERSION_MODELE,
                dokos: String::new(),
                modele: Some("SAL-QTN-0007".into()),
                modele_modifie: Some("2026-10-01 09:00:00".into()),
            }),
            ..Tableau::default()
        };
        clients(&faux);
        modele(&faux, "2026-11-04 17:30:00");
        let erreur = referentiel_pour_fichier(&dokos, Cas::DevisModele, &tableau, jour()).expect_err("modifié");
        assert!(matches!(erreur, ErreurImport::Modele(m) if m.contains("modifié")));

        // Génération sans modèle choisi : refusée avant de lire le modèle.
        let faux = FauxTransport::new();
        clients(&faux);
        let erreur = referentiel_pour_generer(&Dokos::new(&faux), Cas::FacturesModele, jour(), None).expect_err("sans");
        assert!(matches!(erreur, ErreurImport::Modele(_)));
    }
}
