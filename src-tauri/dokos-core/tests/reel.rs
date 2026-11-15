//! Tests contre une vraie instance Dokos **de test**. Ignorés par défaut.
//!
//! Deux niveaux :
//! - `connexion_et_recherches`, `hypotheses_sur_les_doctypes` : lecture seule,
//!   vérifient que les champs supposés par l'outil existent dans ce Dokos ;
//! - `creation_puis_suppression_d_un_devis` : crée un devis en brouillon puis
//!   le supprime. Demande `DOKOS_TEST_CLIENT` (identifiant client) et
//!   `DOKOS_TEST_ARTICLE` (code article vendable), et un droit de suppression.
//!
//! ```powershell
//! $env:DOKOS_TEST_URL = "https://dokos-test.exemple.fr"
//! $env:DOKOS_TEST_CLE = "…"
//! $env:DOKOS_TEST_SECRET = "…"
//! $env:DOKOS_TEST_CLIENT = "CUST-0001"
//! $env:DOKOS_TEST_ARTICLE = "REPAS"
//! cargo test -p dokos-core --test reel -- --ignored --nocapture
//! ```

use dokos_core::article::{arbre_des_groupes, liste_de_prix_par_defaut};
use dokos_core::date::Date;
use dokos_core::doublon::SiDoublon;
use dokos_core::meta::lire_meta;
use dokos_core::recherche::{chercher_article, chercher_client};
use dokos_core::vente::{Creation, LigneVente, NouveauDocumentVente, TypeVente, creer_brouillon};
use dokos_core::{Dokos, HttpTransport, Secret};

fn variable(nom: &str) -> String {
    std::env::var(nom).unwrap_or_else(|_| panic!("variable {nom} absente"))
}

fn dokos_de_test() -> Dokos<HttpTransport> {
    let transport = HttpTransport::new(
        &variable("DOKOS_TEST_URL"),
        &variable("DOKOS_TEST_CLE"),
        &Secret::new(variable("DOKOS_TEST_SECRET")),
        std::env::var("DOKOS_TEST_HTTP").is_ok(),
    )
    .expect("transport");
    Dokos::new(transport)
}

#[test]
#[ignore = "nécessite une instance Dokos de test"]
fn connexion_et_recherches() {
    let dokos = dokos_de_test();
    let utilisateur = dokos.utilisateur_connecte().expect("connexion");
    assert!(!utilisateur.is_empty());
    chercher_client(&dokos, "a").expect("recherche client");
    chercher_article(&dokos, "a").expect("recherche article");
    let arbre = arbre_des_groupes(&dokos).expect("groupes d'articles");
    assert!(!arbre.is_empty(), "aucun groupe d'articles");
    println!("liste de prix par défaut : {:?}", liste_de_prix_par_defaut(&dokos).expect("réglages de vente"));
}

/// Les champs que l'outil écrit ou lit doivent exister dans ce Dokos.
#[test]
#[ignore = "nécessite une instance Dokos de test"]
fn hypotheses_sur_les_doctypes() {
    let dokos = dokos_de_test();
    let attendus: &[(&str, &[&str])] = &[
        ("Quotation", &["quotation_to", "party_name", "transaction_date", "valid_till", "items"]),
        ("Sales Order", &["customer", "delivery_date", "po_no", "items"]),
        ("Sales Invoice", &["customer", "posting_date", "set_posting_time", "due_date", "po_no", "outstanding_amount"]),
        ("Item", &["item_name", "item_group", "stock_uom", "is_sales_item", "has_variants", "disabled"]),
        ("Item Group", &["parent_item_group", "is_group"]),
        ("Customer", &["customer_name", "customer_type", "customer_group", "territory"]),
        (
            "Event",
            &[
                "subject",
                "starts_on",
                "ends_on",
                "published",
                "allow_registrations",
                "registration_form",
                "max_number_of_registrations",
                "allow_cancellations",
                "route",
            ],
        ),
        ("Event Registration", &["event", "first_name", "last_name", "email", "mobile_number"]),
        ("Web Form", &["route", "doc_type", "published", "web_form_fields"]),
        ("Employee", &["first_name", "last_name", "gender", "date_of_birth", "date_of_joining", "company"]),
        (
            "Employment Contract",
            &[
                "employee",
                "contract_type",
                "designation",
                "date_of_joining",
                "relieving_date",
                "weekly_working_hours",
                "monday",
                "sunday",
            ],
        ),
    ];
    let mut manquants = Vec::new();
    for (doctype, champs) in attendus {
        match lire_meta(&dokos, doctype) {
            Ok(meta) => {
                for champ in *champs {
                    if meta.champ(champ).is_none() {
                        manquants.push(format!("{doctype}.{champ}"));
                    }
                }
            }
            Err(e) => manquants.push(format!("{doctype} : {e}")),
        }
    }
    assert!(manquants.is_empty(), "champs absents de ce Dokos : {}", manquants.join(", "));
}

#[test]
#[ignore = "nécessite une instance Dokos de test et un droit de suppression"]
fn creation_puis_suppression_d_un_devis() {
    let dokos = dokos_de_test();
    let client = variable("DOKOS_TEST_CLIENT");
    let article = variable("DOKOS_TEST_ARTICLE");
    let aujourdhui = Date::aujourdhui();
    let devis = NouveauDocumentVente::new(
        TypeVente::Devis,
        &client,
        aujourdhui,
        Some(aujourdhui.plus_jours(30).expect("échéance")),
        vec![LigneVente::new(&article, 1.0, None).expect("ligne")],
    )
    .expect("devis");
    let creation = creer_brouillon(&dokos, &devis, SiDoublon::CreerQuandMeme).expect("création");
    let Creation::Cree { document, .. } = creation else { panic!("doublon inattendu : {creation:?}") };
    assert_eq!(document.docstatus, 0, "le devis doit être en brouillon");
    println!("devis de test créé : {} ({} {})", document.id, document.total, document.devise);
    dokos.supprimer_brouillon("Quotation", &document.id).expect("suppression du devis de test");
    println!("devis de test supprimé");
}
