//! « Atelier poterie le 14 décembre, 12 places, demander l'âge » → événement
//! et questions du formulaire, choisies parmi les champs existants de Dokos.

use dokos_core::date::{Date, DateHeure};
use dokos_core::evenement::PLACES_MAX;
use dokos_core::formulaire::{DOCTYPE_INSCRIPTION, Question, questions_possibles};
use dokos_core::meta::lire_meta;
use dokos_core::{Dokos, Transport};
use serde::Deserialize;

use crate::commun::{consignes, date_heure, nettoyer_questions, texte};
use crate::owui::{ErreurIa, OpenWebUi, borner};

const SCHEMA: &str = r#"{
  "titre": "titre court de l'événement" | null,
  "debut": "AAAA-MM-JJ HH:MM" | null,
  "fin": "AAAA-MM-JJ HH:MM" | null,
  "journee_entiere": true | false,
  "lieu": "texte" | null,
  "description": "deux ou trois phrases pour la page publique, à partir de la demande seulement" | null,
  "places": nombre entier | null,
  "annulation_autorisee": true | false,
  "champs_formulaire": [{"champ": "nom technique pris dans la liste", "obligatoire": true | false}],
  "champs_manquants": ["information demandée qui n'a aucun champ dans la liste"],
  "questions": ["question courte"]
}"#;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Extraction {
    titre: Option<String>,
    debut: Option<String>,
    fin: Option<String>,
    journee_entiere: bool,
    lieu: Option<String>,
    description: Option<String>,
    places: Option<f64>,
    annulation_autorisee: bool,
    champs_formulaire: Vec<ChampExtrait>,
    champs_manquants: Vec<String>,
    questions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ChampExtrait {
    champ: String,
    obligatoire: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropositionEvenement {
    pub titre: Option<String>,
    pub debut: Option<DateHeure>,
    pub fin: Option<DateHeure>,
    pub journee_entiere: bool,
    pub lieu: Option<String>,
    pub description: Option<String>,
    pub places: Option<u32>,
    pub annulation_autorisee: bool,
    /// Questions retenues : toutes existent dans `Event Registration`.
    pub questions_formulaire: Vec<Question>,
    /// Informations demandées sans champ dans Dokos : à créer à la main.
    pub champs_a_creer: Vec<String>,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

pub fn proposer_evenement<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    aujourdhui: Date,
) -> Result<PropositionEvenement, ErreurIa> {
    let demande = borner(demande)?;
    let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION)?;
    let possibles = questions_possibles(&meta);
    let liste = if possibles.is_empty() {
        "(aucun champ supplémentaire : prénom, nom, e-mail et téléphone sont toujours demandés)".to_owned()
    } else {
        possibles
            .iter()
            .map(|c| format!("- {} : {} ({})", c.fieldname, c.libelle(), c.fieldtype))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let specifique = format!(
        "Il s'agit d'un événement public avec un formulaire d'inscription.\n\
Prénom, nom, e-mail et téléphone sont toujours demandés : ne les mets pas dans \"champs_formulaire\".\n\
Pour \"champs_formulaire\", n'utilise QUE ces champs existants (nom technique avant les deux-points) :\n{liste}\n\
Une information demandée qui ne correspond à aucun de ces champs va dans \"champs_manquants\"."
    );
    let reponse = ia.demander_json::<Extraction>(&consignes(aujourdhui, &specifique, SCHEMA), &demande, bases)?;
    let extraction = reponse.donnees;
    let mut questions = Vec::new();

    let titre = texte(extraction.titre);
    if titre.is_none() {
        questions.push("Quel titre pour l'événement ?".to_owned());
    }
    let debut = date_heure(extraction.debut, "Début", &mut questions);
    if debut.is_none() {
        questions.push("Quand commence l'événement (date et heure) ?".to_owned());
    }
    let fin = date_heure(extraction.fin, "Fin", &mut questions).filter(|fin| debut.is_none_or(|d| *fin > d));
    let places = extraction
        .places
        .filter(|p| p.is_finite() && p.fract() == 0.0 && *p >= 1.0 && *p <= f64::from(PLACES_MAX))
        .map(|p| p as u32);

    let mut questions_formulaire: Vec<Question> = Vec::new();
    let mut champs_a_creer: Vec<String> =
        extraction.champs_manquants.into_iter().filter_map(|c| texte(Some(c))).collect();
    for champ in extraction.champs_formulaire {
        let nom = champ.champ.trim();
        if nom.is_empty() || questions_formulaire.iter().any(|q| q.champ == nom) {
            continue;
        }
        if possibles.iter().any(|c| c.fieldname == nom) {
            questions_formulaire.push(Question { champ: nom.to_owned(), obligatoire: champ.obligatoire });
        } else if !["first_name", "last_name", "email", "mobile_number", "event"].contains(&nom) {
            champs_a_creer.push(nom.to_owned());
        }
    }
    champs_a_creer.sort();
    champs_a_creer.dedup();
    if !champs_a_creer.is_empty() {
        questions.push(format!(
            "Sans champ dans Dokos, impossible de demander : {}. Créez-les dans « {DOCTYPE_INSCRIPTION} » (Personnaliser le formulaire) ?",
            champs_a_creer.join(", ")
        ));
    }

    questions.extend(extraction.questions);
    Ok(PropositionEvenement {
        titre,
        debut,
        fin,
        journee_entiere: extraction.journee_entiere,
        lieu: texte(extraction.lieu),
        description: texte(extraction.description),
        places,
        annulation_autorisee: extraction.annulation_autorisee,
        questions_formulaire,
        champs_a_creer,
        questions: nettoyer_questions(questions),
        avertissements: reponse.avertissements,
    })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;
    use crate::owui::tests::reponse_ia;

    fn meta() -> serde_json::Value {
        let champ = |f: &str, t: &str, l: &str| json!({"fieldname": f, "fieldtype": t, "label": l});
        json!({"docs": [{"name": DOCTYPE_INSCRIPTION, "fields": [
            champ("first_name", "Data", "First Name"), champ("last_name", "Data", "Last Name"),
            champ("email", "Data", "Email"), champ("mobile_number", "Data", "Mobile"), champ("event", "Link", "Event"),
            champ("age", "Int", "Âge"), champ("allergies", "Small Text", "Allergies"),
            champ("payment_status", "Select", "Payment Status"),
        ]}]})
    }

    #[test]
    fn evenement_et_champs_valides() {
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &meta());
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &reponse_ia(
                r#"{"titre": "Atelier poterie", "debut": "2026-12-14 14:00", "fin": "2026-12-14 17:00",
                "lieu": "Grande salle", "places": 12, "annulation_autorisee": true,
                "champs_formulaire": [{"champ": "age", "obligatoire": true}, {"champ": "regime", "obligatoire": false},
                                      {"champ": "payment_status", "obligatoire": true}, {"champ": "email", "obligatoire": true}],
                "champs_manquants": ["taille de t-shirt"], "questions": []}"#,
            ),
        );
        let jour: Date = "2026-11-03".parse().expect("date");
        let p =
            proposer_evenement(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "…", jour).expect("proposition");
        assert_eq!(p.titre.as_deref(), Some("Atelier poterie"));
        assert_eq!(p.places, Some(12));
        assert_eq!(p.questions_formulaire, [Question { champ: "age".into(), obligatoire: true }]);
        assert_eq!(p.champs_a_creer, ["payment_status", "regime", "taille de t-shirt"]);
        assert_eq!(p.questions.len(), 1);

        let consigne =
            ia.requetes()[0].corps.as_ref().map(|c| c["messages"][0]["content"].to_string()).unwrap_or_default();
        assert!(consigne.contains("age : Âge (Int)"), "{consigne}");
        assert!(!consigne.contains("payment_status"), "les champs réservés ne sont pas proposés à l'IA");
    }

    #[test]
    fn valeurs_incoherentes_ecartees() {
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &meta());
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &reponse_ia(r#"{"titre": " ", "debut": "2026-12-14 14:00", "fin": "2026-12-14 10:00", "places": 2.5}"#),
        );
        let jour: Date = "2026-11-03".parse().expect("date");
        let p =
            proposer_evenement(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "…", jour).expect("proposition");
        assert_eq!((p.titre, p.fin, p.places), (None, None, None));
        assert_eq!(p.questions, ["Quel titre pour l'événement ?"]);
    }
}
