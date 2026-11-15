//! Test contre un vrai serveur Open WebUI. Ignoré par défaut.
//!
//! ```powershell
//! $env:OWUI_URL = "http://mac-studio:8080"
//! $env:OWUI_CLE = "sk-…"
//! $env:OWUI_MODELE = "qwen3.6-35b-a3b"
//! cargo test -p assistant-core --test reel -- --ignored --nocapture
//! ```

use std::time::Duration;

use assistant_core::OpenWebUi;
use assistant_core::adresse::valider_url_ia;
use dokos_core::{HttpTransport, Secret};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Extraction {
    client: Option<String>,
    quantite: Option<f64>,
}

#[test]
#[ignore = "nécessite un serveur Open WebUI"]
fn modele_et_extraction() {
    let variable = |nom: &str| std::env::var(nom).unwrap_or_else(|_| panic!("variable {nom} absente"));
    let (url, http) = valider_url_ia(&variable("OWUI_URL")).expect("adresse");
    let transport = HttpTransport::bearer(&url, &Secret::new(variable("OWUI_CLE")), http, Duration::from_secs(180))
        .expect("transport");
    let modele = variable("OWUI_MODELE");
    let ia = OpenWebUi::new(transport, &modele);

    let modeles = ia.modeles().expect("liste des modèles");
    assert!(modeles.contains(&modele), "modèle {modele} absent : {modeles:?}");

    let reponse = ia
        .demander_json::<Extraction>(
            "Réponds uniquement par un objet JSON : {\"client\": texte, \"quantite\": nombre}.",
            "25 repas pour la mairie de Crest",
            &[],
        )
        .expect("réponse");
    println!("{reponse:?}");
    assert_eq!(reponse.donnees.quantite, Some(25.0));
    assert!(reponse.donnees.client.is_some_and(|c| c.to_lowercase().contains("crest")));
}
