//! Fiches détaillées de clients et d'articles, pour le chatbot. Lecture seule.
//!
//! Recherche sur l'identifiant, le nom (et l'e-mail ou le téléphone d'un
//! client) ; désactivés compris, signalés. Les champs absents de l'instance
//! sont écartés après lecture de la structure du DocType.

use std::fmt;

use serde_json::{Map, Value};

use crate::article::{liste_de_prix_par_defaut, prix_indicatifs};
use crate::client::{Dokos, Filtre, Liste};
use crate::erreur::DokosError;
use crate::meta::lire_meta;
use crate::recherche::motif_recherche;
use crate::transport::Transport;
use crate::types::{DOCTYPE_ARTICLE, DOCTYPE_CLIENT};

/// Fiches renvoyées au plus par recherche.
pub const FICHES_MAX: u32 = 10;
/// Longueur maximale d'une valeur affichée (descriptions, notes).
const VALEUR_MAX: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreFiche {
    Client,
    Article,
}

impl GenreFiche {
    pub fn nom(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Article => "article",
        }
    }

    pub fn doctype(self) -> &'static str {
        match self {
            Self::Client => DOCTYPE_CLIENT,
            Self::Article => DOCTYPE_ARTICLE,
        }
    }

    /// (champ, libellé, champ recherché) ; le premier champ est le titre de la fiche.
    fn champs(self) -> &'static [(&'static str, &'static str, bool)] {
        match self {
            Self::Client => &[
                ("customer_name", "Nom", true),
                ("customer_type", "Type", false),
                ("customer_group", "Groupe", false),
                ("territory", "Territoire", false),
                ("email_id", "E-mail", true),
                ("mobile_no", "Téléphone", true),
                ("tax_id", "N° TVA / SIRET", false),
                ("primary_address", "Adresse principale", false),
                ("customer_primary_contact", "Contact principal", false),
                ("default_price_list", "Liste de prix", false),
                ("payment_terms", "Conditions de paiement", false),
                ("default_currency", "Devise", false),
                ("website", "Site web", false),
                ("customer_details", "Notes", false),
                ("disabled", "Désactivé", false),
                ("creation", "Client depuis", false),
            ],
            Self::Article => &[
                ("item_name", "Libellé", true),
                ("item_group", "Groupe", false),
                ("stock_uom", "Unité", false),
                ("description", "Description", false),
                ("is_sales_item", "Vendable", false),
                ("disabled", "Désactivé", false),
            ],
        }
    }
}

impl fmt::Display for GenreFiche {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

impl std::str::FromStr for GenreFiche {
    type Err = String;

    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        match texte.trim().to_lowercase().as_str() {
            "client" | "clients" => Ok(Self::Client),
            "article" | "articles" | "prestation" => Ok(Self::Article),
            autre => Err(format!("type de fiche « {autre} » inconnu (client ou article)")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fiche {
    pub genre: GenreFiche,
    pub id: String,
    pub titre: String,
    pub desactive: bool,
    /// (libellé, valeur), valeurs vides écartées.
    pub champs: Vec<(String, String)>,
}

/// Texte lisible d'un champ HTML de Dokos (adresse, description).
pub fn texte_simple(html: &str) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut dans_balise = false;
    let mut balise = String::new();
    for c in html.chars() {
        match c {
            '<' => {
                dans_balise = true;
                balise.clear();
            }
            '>' if dans_balise => {
                dans_balise = false;
                let nom = balise.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_lowercase();
                if matches!(nom.as_str(), "br" | "br/" | "p" | "div" | "li" | "tr") {
                    sortie.push('\n');
                }
            }
            _ if dans_balise => balise.push(c),
            _ => sortie.push(c),
        }
    }
    let decode = sortie
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    decode
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn valeur_lisible(champ: &str, valeur: Option<&Value>) -> String {
    let brut = match valeur {
        None | Some(Value::Null) => return String::new(),
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Bool(b)) => if *b { "1" } else { "0" }.to_owned(),
        Some(autre) => autre.to_string(),
    };
    let texte = match champ {
        "disabled" | "is_sales_item" => match brut.as_str() {
            "1" | "true" => "oui".to_owned(),
            _ => "non".to_owned(),
        },
        "customer_type" => match brut.as_str() {
            "Company" => "Société".to_owned(),
            "Individual" => "Particulier".to_owned(),
            autre => autre.to_owned(),
        },
        "creation" => brut.get(..10).unwrap_or(&brut).to_owned(),
        "primary_address" | "description" | "customer_details" => texte_simple(&brut),
        _ => brut,
    };
    texte.chars().take(VALEUR_MAX).collect()
}

/// Cherche des clients ou des articles par identifiant ou nom (désactivés compris).
pub fn chercher_fiches<T: Transport>(
    dokos: &Dokos<T>,
    genre: GenreFiche,
    texte: &str,
) -> Result<Vec<Fiche>, DokosError> {
    let motif = motif_recherche(texte)?;
    let doctype = genre.doctype();
    let champs: Vec<(&str, &str, bool)> = match lire_meta(dokos, doctype) {
        Ok(meta) => genre.champs().iter().copied().filter(|(c, _, _)| *c == "creation" || meta.a_champ(c)).collect(),
        Err(DokosError::Permission(_)) => genre.champs().iter().copied().take(3).collect(),
        Err(e) => return Err(e),
    };
    let mut demandes: Vec<&str> = vec!["name"];
    demandes.extend(champs.iter().map(|(c, _, _)| *c));
    let mut liste = Liste::new(&demandes).ou(Filtre::contient("name", &motif)).ordre("disabled asc, modified desc");
    for (champ, _, cherche) in &champs {
        if *cherche {
            liste = liste.ou(Filtre::contient(champ, &motif));
        }
    }
    let documents: Vec<Map<String, Value>> = dokos.lister(doctype, &liste.limite(FICHES_MAX))?;

    let mut fiches: Vec<Fiche> = documents
        .iter()
        .map(|d| {
            let id = valeur_lisible("name", d.get("name"));
            let titre = champs.first().map(|(c, _, _)| valeur_lisible(c, d.get(*c))).filter(|t| !t.is_empty());
            Fiche {
                genre,
                titre: titre.unwrap_or_else(|| id.clone()),
                desactive: valeur_lisible("disabled", d.get("disabled")) == "oui",
                champs: champs
                    .iter()
                    .skip(1)
                    .map(|(c, libelle, _)| ((*libelle).to_owned(), valeur_lisible(c, d.get(*c))))
                    .filter(|(_, v)| !v.is_empty())
                    .collect(),
                id,
            }
        })
        .collect();

    if genre == GenreFiche::Article && !fiches.is_empty() {
        // Prix indicatif de la liste de prix par défaut ; son absence n'empêche rien.
        if let Ok(Some(liste_prix)) = liste_de_prix_par_defaut(dokos) {
            let codes: Vec<&str> = fiches.iter().map(|f| f.id.as_str()).collect();
            let prix = prix_indicatifs(dokos, &liste_prix, &codes).unwrap_or_default();
            for fiche in &mut fiches {
                if let Some(p) = prix.iter().find(|p| p.article == fiche.id) {
                    fiche.champs.push((format!("Prix ({liste_prix})"), format!("{:.2} {}", p.prix, p.devise)));
                }
            }
        }
    }
    Ok(fiches)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::meta::tests::reponse_meta;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    #[test]
    fn html_en_texte() {
        assert_eq!(
            texte_simple("12 rue des Écoles<br>26400 Crest<br/>\nFrance&nbsp;<br>"),
            "12 rue des Écoles, 26400 Crest, France"
        );
        assert_eq!(
            texte_simple("<p>Salle <b>équipée</b> &amp; lumineuse</p><p>40 places</p>"),
            "Salle équipée & lumineuse, 40 places"
        );
        assert_eq!(texte_simple("simple"), "simple");
    }

    #[test]
    fn fiche_client_complete() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_meta(
                "Customer",
                json!([
                    {"fieldname": "customer_name", "fieldtype": "Data"},
                    {"fieldname": "customer_type", "fieldtype": "Select"},
                    {"fieldname": "email_id", "fieldtype": "Data"},
                    {"fieldname": "mobile_no", "fieldtype": "Data"},
                    {"fieldname": "primary_address", "fieldtype": "Text"},
                    {"fieldname": "disabled", "fieldtype": "Check"},
                ]),
            ),
        );
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest", "customer_type": "Company",
                              "email_id": "accueil@crest.fr", "mobile_no": null,
                              "primary_address": "Place du Dr Rozier<br>26400 Crest<br>", "disabled": 0,
                              "creation": "2024-03-02 10:00:00"}]}),
        );
        let fiches = chercher_fiches(&Dokos::new(&faux), GenreFiche::Client, "crest").expect("fiches");
        assert_eq!(fiches.len(), 1);
        let f = &fiches[0];
        assert_eq!((f.id.as_str(), f.titre.as_str(), f.desactive), ("CUST-2", "Mairie de Crest", false));
        assert_eq!(
            f.champs,
            [
                ("Type".to_owned(), "Société".to_owned()),
                ("E-mail".to_owned(), "accueil@crest.fr".to_owned()),
                ("Adresse principale".to_owned(), "Place du Dr Rozier, 26400 Crest".to_owned()),
                ("Désactivé".to_owned(), "non".to_owned()),
                ("Client depuis".to_owned(), "2024-03-02".to_owned()),
            ]
        );
        let requete = &faux.requetes()[1];
        assert_eq!(requete.methode, Methode::Get);
        assert_eq!(
            requete.parametre("or_filters"),
            Some(
                r#"[["name","like","%crest%"],["customer_name","like","%crest%"],["email_id","like","%crest%"],["mobile_no","like","%crest%"]]"#
            )
        );
        assert_eq!(requete.parametre("limit_page_length"), Some("10"));
    }

    #[test]
    fn fiche_article_avec_prix() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_meta(
                "Item",
                json!([
                    {"fieldname": "item_name", "fieldtype": "Data"},
                    {"fieldname": "description", "fieldtype": "Text Editor"},
                    {"fieldname": "disabled", "fieldtype": "Check"},
                ]),
            ),
        );
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "SALLE-R", "item_name": "Salle de réunion", "description": "<p>20 places</p>", "disabled": 1}]}),
        );
        faux.repondre_json(200, &json!({"data": {"selling_price_list": "Tarif 2026"}}));
        faux.repondre_json(
            200,
            &json!({"data": [{"item_code": "SALLE-R", "price_list_rate": 150, "currency": "EUR", "price_list": "Tarif 2026"}]}),
        );
        let fiches = chercher_fiches(&Dokos::new(&faux), GenreFiche::Article, "salle").expect("fiches");
        let f = &fiches[0];
        assert!(f.desactive);
        assert!(f.champs.contains(&("Description".to_owned(), "20 places".to_owned())));
        assert!(f.champs.contains(&("Prix (Tarif 2026)".to_owned(), "150.00 EUR".to_owned())));
    }

    #[test]
    fn recherche_vide_refusee() {
        let faux = FauxTransport::new();
        assert!(chercher_fiches(&Dokos::new(&faux), GenreFiche::Client, " % ").is_err());
        assert!(faux.requetes().is_empty());
        assert_eq!("Articles".parse::<GenreFiche>(), Ok(GenreFiche::Article));
        assert!("fournisseur".parse::<GenreFiche>().is_err());
    }
}
