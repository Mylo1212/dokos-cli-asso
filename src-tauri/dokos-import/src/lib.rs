//! Import de tableaux (Excel ou CSV) vérifiés, et modèles générés par l'outil.
//!
//! - [`cas`] : les cas d'usage (factures, paiements, articles, clients) et
//!   leurs colonnes.
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
//! - [`export`] : écriture d'une liste lue dans Dokos en classeur Excel.
//!
//! Rien n'est écrit dans Dokos tant qu'une ligne pose problème, sauf si
//! l'utilisateur demande explicitement « seulement les documents prêts ».

pub mod analyse;
pub mod cas;
pub mod export;
pub mod lecture;
pub mod modele;
pub mod rapport;
pub mod referentiel;

use std::fmt;

use dokos_core::DokosError;

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

pub use analyse::{
    Analyse, DocumentPrepare, LigneAnalysee, Options, ResultatDocument, Statut, analyser, creer, verifier,
};
pub use cas::{Cas, Colonne, Genre};
pub use lecture::{Marque, Tableau, lire_fichier, lire_octets};
pub use modele::generer_modele;
pub use referentiel::Referentiel;

#[cfg(test)]
mod tests {
    use dokos_core::client::Dokos;
    use dokos_core::doublon::SiDoublon;
    use dokos_core::paiement::FactureARegler;
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;
    use crate::analyse::{OptionsCreation, StatutCreation};
    use crate::modele::ContexteModele;
    use crate::referentiel::{LISTE_FACTURES, LISTE_MODES_PAIEMENT};

    fn facture(id: &str, reste: f64, client: &str) -> FactureARegler {
        FactureARegler {
            id: id.into(),
            client: client.into(),
            nom_client: if client == "CUST-2" { "Mairie de Crest".into() } else { "CCAS".into() },
            date: "2026-10-31".into(),
            reste_du: reste,
            devise: "EUR".into(),
            societe: "Les Amanins".into(),
        }
    }

    /// Ce que la lecture de Dokos a mis dans le référentiel : trois factures (deux clients), deux modes.
    fn referentiel() -> Referentiel {
        let factures = vec![
            facture("ACC-SINV-1", 200.0, "CUST-2"),
            facture("ACC-SINV-2", 50.0, "CUST-2"),
            facture("ACC-SINV-3", 80.0, "CUST-5"),
        ];
        let mut r = Referentiel::default();
        r.ajouter(LISTE_FACTURES, factures.iter().map(|f| (f.id.clone(), f.detail())).collect());
        r.ajouter(
            LISTE_MODES_PAIEMENT,
            vec![("Espèces".into(), "Espèces".into()), ("Virement".into(), "Virement".into())],
        );
        r.factures_a_regler = factures;
        r
    }

    /// Compte d'encaissement d'un mode, puis son type.
    fn compte(faux: &FauxTransport, type_compte: &str) {
        faux.repondre_json(200, &json!({"message": {"account": "512 - Banque - SLA"}}));
        faux.repondre_json(200, &json!({"data": {"name": "512 - Banque - SLA", "account_type": type_compte}}));
    }

    /// Préparation de Dokos pour une facture.
    fn ecriture_preparee(facture: &str, montant: f64, client: &str) -> serde_json::Value {
        json!({"message": {
            "doctype": "Payment Entry", "name": "new-payment-entry-1", "docstatus": 0, "payment_type": "Receive",
            "party_type": "Customer", "party": client, "company": "Les Amanins",
            "paid_from": "411 - Clients - SLA", "paid_to": "512 - Banque - SLA",
            "paid_amount": montant, "received_amount": montant,
            "references": [{"reference_name": facture, "allocated_amount": montant}],
        }})
    }

    fn ligne(valeurs: [&str; 6]) -> Vec<String> {
        valeurs.iter().map(|v| (*v).to_owned()).collect()
    }

    fn modele_paiements(referentiel: &Referentiel) -> Tableau {
        let contexte =
            ContexteModele { url_dokos: "https://dokos.test".into(), date: "2026-11-05".into(), societe: None };
        let octets = generer_modele(Cas::Paiements, referentiel, &contexte).expect("modèle");
        let tableau = lire_octets(&octets, "xlsx").expect("lecture");
        tableau.verifier_marque(Cas::Paiements).expect("marque du cas");
        tableau
    }

    #[test]
    fn modele_et_import_de_paiements() {
        let referentiel = referentiel();
        let mut tableau = modele_paiements(&referentiel);
        let noms: Vec<&str> = tableau.entetes.iter().map(String::as_str).collect();
        assert_eq!(noms, ["Lot", "Facture *", "Date du paiement *", "Montant", "Mode de paiement *", "Référence"]);

        tableau.lignes = vec![
            // Un virement pour deux factures de la Mairie : tout le reste dû de la première, 30 sur la seconde.
            (3, ligne(["", "ACC-SINV-1", "05/11/2026", "", "Virement", "VIR-1"])),
            (4, ligne(["", "acc-sinv-2", "05/11/2026", "30", "virement", "VIR-1"])),
            // Même virement, mais facture d'un autre client : paiement à part.
            (5, ligne(["", "ACC-SINV-3", "05/11/2026", "", "Virement", "VIR-1"])),
            // ACC-SINV-2 : 30 déjà réparti, reste 20.
            (6, ligne(["", "ACC-SINV-2", "06/11/2026", "30", "Espèces", ""])),
            (7, ligne(["X", "ACC-SINV-9", "07/11/2026", "10", "Espèces", ""])),
        ];
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);
        compte(&faux, "Bank"); // Virement, demandé une seule fois pour les deux paiements
        let mut analyse =
            analyser(&dokos, Cas::Paiements, &tableau, &referentiel, &Options::default()).expect("analyse");
        let groupes: Vec<&[usize]> = analyse.documents.iter().map(|d| d.lignes.as_slice()).collect();
        assert_eq!(groupes, [&[0, 1][..], &[2], &[3], &[4]]);
        assert!(analyse.documents[0].pret() && analyse.documents[1].pret());
        assert!(
            analyse.documents[0].libelle.contains("Mairie de Crest — 230.00 EUR")
                && analyse.documents[0].libelle.contains("2 factures : ACC-SINV-1, ACC-SINV-2"),
            "{}",
            analyse.documents[0].libelle
        );
        assert!(
            matches!(&analyse.lignes[3].statut, Statut::Invalide(m) if m.contains("reste dû") && m.contains("20.00")),
            "{:?}",
            analyse.lignes[3].statut
        );
        assert!(matches!(&analyse.lignes[4].statut, Statut::Invalide(m) if m.contains("ACC-SINV-9")));

        faux.repondre_json(200, &json!({"data": []})); // doublon, paiement 1
        faux.repondre_json(200, &json!({"data": []})); // doublon, paiement 2
        verifier(&dokos, &mut analyse).expect("vérification");
        assert_eq!(analyse.doublons(), 0);

        // « Seulement les prêts » : les deux paiements sont créés, les autres ignorés.
        faux.repondre_json(200, &json!({"data": []})); // doublon à la création
        faux.repondre_json(200, &ecriture_preparee("ACC-SINV-1", 200.0, "CUST-2"));
        faux.repondre_json(200, &ecriture_preparee("ACC-SINV-2", 30.0, "CUST-2"));
        faux.repondre_json(200, &json!({"data": {"name": "ACC-PAY-1", "docstatus": 0, "paid_amount": 230.0}}));
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &ecriture_preparee("ACC-SINV-3", 80.0, "CUST-5"));
        faux.repondre_json(200, &json!({"data": {"name": "ACC-PAY-2", "docstatus": 0, "paid_amount": 80.0}}));
        let resultats =
            creer(&dokos, &analyse, &OptionsCreation { seulement_prets: true, si_doublon: SiDoublon::Reutiliser })
                .expect("création");
        let statuts: Vec<_> = resultats.iter().map(|r| r.statut.clone()).collect();
        assert_eq!(
            statuts,
            [StatutCreation::Cree, StatutCreation::Cree, StatutCreation::Ignore, StatutCreation::Ignore]
        );
        assert_eq!((resultats[0].doctype.as_str(), resultats[0].id.as_deref()), ("Payment Entry", Some("ACC-PAY-1")));
        let envoyes: Vec<_> = faux
            .requetes()
            .into_iter()
            .filter(|r| r.methode == Methode::Post && r.chemin == "/api/resource/Payment%20Entry")
            .filter_map(|r| r.corps)
            .collect();
        assert_eq!(envoyes.len(), 2);
        assert_eq!(envoyes[0]["references"].as_array().map(Vec::len), Some(2));
        assert_eq!(envoyes[0]["paid_amount"], 230.0);
        assert_eq!(
            (envoyes[0]["mode_of_payment"].as_str(), envoyes[0]["reference_no"].as_str()),
            (Some("Virement"), Some("VIR-1"))
        );
        assert_eq!(envoyes[1]["references"][0]["reference_name"], "ACC-SINV-3");
        assert_eq!(faux.reponses_restantes(), 0);
    }

    #[test]
    fn rien_cree_si_une_ligne_de_paiement_bloque() {
        let referentiel = referentiel();
        let mut tableau = modele_paiements(&referentiel);
        tableau.lignes = vec![
            // Lot L : la seconde ligne n'a pas la date de la première.
            (3, ligne(["L", "ACC-SINV-1", "05/11/2026", "", "Espèces", ""])),
            (4, ligne(["L", "ACC-SINV-2", "06/11/2026", "", "Espèces", ""])),
            // Compte bancaire sans référence.
            (5, ligne(["", "ACC-SINV-3", "05/11/2026", "", "Virement", ""])),
            // Mode inconnu.
            (6, ligne(["", "ACC-SINV-3", "08/11/2026", "1", "Troc", ""])),
            // Même facture deux fois dans un paiement.
            (7, ligne(["M", "ACC-SINV-2", "09/11/2026", "5", "Espèces", ""])),
            (8, ligne(["M", "ACC-SINV-2", "09/11/2026", "5", "Espèces", ""])),
        ];
        let faux = FauxTransport::new();
        compte(&faux, "Bank");
        let dokos = Dokos::new(&faux);
        let analyse = analyser(&dokos, Cas::Paiements, &tableau, &referentiel, &Options::default()).expect("analyse");
        assert!(analyse.lignes[0].statut.est_pret());
        assert!(matches!(&analyse.lignes[1].statut, Statut::Invalide(m) if m.contains("première ligne du lot « L »")));
        assert!(analyse.documents[0].prepare.is_none(), "une ligne à corriger bloque tout le paiement");
        assert!(matches!(&analyse.lignes[2].statut, Statut::Invalide(m) if m.contains("référence obligatoire")));
        assert!(matches!(&analyse.lignes[3].statut, Statut::Introuvable { champ, .. } if champ == "Mode de paiement"));
        assert!(
            matches!(&analyse.lignes[5].statut, Statut::Invalide(m) if m.contains("déjà réglée par une autre ligne"))
        );
        assert_eq!(analyse.documents_prets(), 0);
        let erreur =
            creer(&dokos, &analyse, &OptionsCreation { seulement_prets: false, si_doublon: SiDoublon::Reutiliser })
                .expect_err("bloqué");
        assert_eq!(erreur, ErreurImport::LignesBloquantes(4));
        assert!(faux.requetes().iter().all(|r| r.methode != Methode::Post || r.chemin.contains("/api/method/")));
        assert_eq!(faux.reponses_restantes(), 0);
    }
}
