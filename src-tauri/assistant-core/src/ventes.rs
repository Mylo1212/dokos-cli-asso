//! « 25 repas pour la mairie de Crest le 12/11 » → devis, commande ou facture pré-remplis.

use dokos_core::article::lister_groupes;
use dokos_core::date::Date;
use dokos_core::recherche::{trouver_article, trouver_client};
use dokos_core::types::{Article, Client};
use dokos_core::vente::{QUANTITE_MAX, TypeVente};
use dokos_core::{Dokos, Transport};
use serde::Deserialize;

use crate::commun::{Choix, choisir, consignes, date, nettoyer_questions, texte};
use crate::owui::{ErreurIa, OpenWebUi, borner};

/// Nombre maximal de lignes reprises d'une réponse.
const LIGNES_MAX: usize = 50;
/// Groupes d'articles envoyés à l'IA au plus.
const GROUPES_MAX: usize = 100;

const SCHEMA: &str = r#"{
  "type_document": "devis" | "commande" | "facture" | null,
  "client": "nom du client tel qu'il est écrit" | null,
  "date": "AAAA-MM-JJ" | null,
  "echeance": "AAAA-MM-JJ" | null,
  "lignes": [{"article": "désignation telle qu'elle est écrite", "quantite": nombre | null, "livraison": "AAAA-MM-JJ" | null}],
  "questions": ["question courte"]
}"#;

const SPECIFIQUE: &str = "Il s'agit d'un devis, d'une commande ou d'une facture client.\n\
- \"date\" : date du devis ou de la facture, ou date de livraison (prestation) pour une commande.\n\
- \"echeance\" : date de validité d'un devis ou d'échéance d'une facture, seulement si elle est donnée.\n\
- \"livraison\" d'une ligne : seulement si elle diffère de \"date\" (commande).\n\
- Sans indication claire du type de document, mets null.";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Extraction {
    type_document: Option<String>,
    client: Option<String>,
    date: Option<String>,
    echeance: Option<String>,
    lignes: Vec<LigneExtraite>,
    questions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LigneExtraite {
    article: String,
    quantite: Option<f64>,
    livraison: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LigneProposee {
    /// Désignation donnée dans la demande.
    pub texte: String,
    pub article: Choix<Article>,
    pub quantite: Option<f64>,
    pub livraison: Option<Date>,
}

/// Pré-remplissage proposé : rien n'est écrit dans Dokos.
#[derive(Debug, Clone, PartialEq)]
pub struct PropositionVente {
    pub type_vente: Option<TypeVente>,
    pub client: Choix<Client>,
    pub date: Option<Date>,
    pub echeance: Option<Date>,
    pub lignes: Vec<LigneProposee>,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

pub fn proposer_vente<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    aujourdhui: Date,
) -> Result<PropositionVente, ErreurIa> {
    let demande = borner(demande)?;
    let mut avertissements = Vec::new();
    // Les groupes d'articles aident à comprendre « repas », « salle »… ; leur
    // absence (droit manquant) n'empêche rien.
    let groupes: Vec<String> = match lister_groupes(dokos) {
        Ok(groupes) => groupes.into_iter().filter(|g| g.is_group == 0).map(|g| g.id).collect(),
        Err(e) => {
            avertissements.push(format!("groupes d'articles non lus ({e}) : l'IA travaille sans eux"));
            Vec::new()
        }
    };
    let specifique = if groupes.is_empty() {
        SPECIFIQUE.to_owned()
    } else {
        format!(
            "{SPECIFIQUE}\n- Familles d'articles du catalogue, pour comprendre les désignations : {}.",
            groupes.iter().take(GROUPES_MAX).cloned().collect::<Vec<_>>().join(", ")
        )
    };
    let reponse = ia.demander_json::<Extraction>(&consignes(aujourdhui, &specifique, SCHEMA), &demande, bases)?;
    avertissements.extend(reponse.avertissements);
    let extraction = reponse.donnees;
    let mut questions = Vec::new();

    let type_vente = match texte(extraction.type_document).as_deref().map(str::to_lowercase).as_deref() {
        Some("devis") => Some(TypeVente::Devis),
        Some("commande") => Some(TypeVente::Commande),
        Some("facture") => Some(TypeVente::Facture),
        _ => None,
    };

    let client = choisir(texte(extraction.client).as_deref(), |t| trouver_client(dokos, t))?;
    match &client {
        Choix::NonPrecise => questions.push("Pour quel client ?".to_owned()),
        Choix::Introuvable(t) => questions.push(format!("Aucun client actif « {t} » dans Dokos : lequel ?")),
        Choix::APreciser { texte, .. } => {
            questions.push(format!("Plusieurs clients correspondent à « {texte} » : lequel ?"))
        }
        Choix::Trouve(_) => {}
    }

    let date_document = date(extraction.date, "Date", &mut questions);
    let echeance = date(extraction.echeance, "Échéance", &mut questions);

    let mut lignes = Vec::new();
    for ligne in extraction.lignes.into_iter().take(LIGNES_MAX) {
        let designation = ligne.article.trim().to_owned();
        if designation.is_empty() {
            continue;
        }
        let article = choisir(Some(&designation), |t| trouver_article(dokos, t))?;
        match &article {
            Choix::Introuvable(t) => questions.push(format!("Aucun article vendable « {t} » dans Dokos : lequel ?")),
            Choix::APreciser { texte, .. } => questions.push(format!("Plusieurs articles pour « {texte} » : lequel ?")),
            Choix::Trouve(_) | Choix::NonPrecise => {}
        }
        let quantite = ligne.quantite.filter(|q| q.is_finite() && *q > 0.0 && *q <= QUANTITE_MAX);
        if quantite.is_none() {
            questions.push(format!("Quelle quantité pour « {designation} » ?"));
        }
        let livraison = date(ligne.livraison, &format!("Livraison de « {designation} »"), &mut questions);
        lignes.push(LigneProposee { texte: designation, article, quantite, livraison });
    }
    if lignes.is_empty() {
        questions.push("Quels articles, et en quelle quantité ?".to_owned());
    }

    questions.extend(extraction.questions);
    Ok(PropositionVente {
        type_vente,
        client,
        date: date_document,
        echeance,
        lignes,
        questions: nettoyer_questions(questions),
        avertissements,
    })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;
    use crate::owui::tests::reponse_ia;

    fn jour() -> Date {
        "2026-11-03".parse().expect("date")
    }

    #[test]
    fn demande_complete() {
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &reponse_ia(
                r#"<think>…</think>{"type_document": "Devis", "client": "mairie de Crest", "date": "2026-11-12",
                "echeance": null, "lignes": [{"article": "repas", "quantite": 25, "livraison": null},
                {"article": "grande salle", "quantite": 1}], "questions": ["Repas végétariens ?"]}"#,
            ),
        );
        let dokos = FauxTransport::new();
        dokos.repondre_json(
            200,
            &json!({"data": [{"name": "Restauration", "is_group": 1}, {"name": "Repas", "is_group": 0}]}),
        );
        dokos.repondre_json(
            200,
            &json!({"data": [{"name": "CUST-1", "customer_name": "CCAS de Crest"},
                             {"name": "CUST-2", "customer_name": "Mairie de Crest"}]}),
        );
        dokos.repondre_json(
            200,
            &json!({"data": [{"name": "REPAS-C", "item_name": "Repas complet"}, {"name": "REPAS-V", "item_name": "Repas végétarien"}]}),
        );
        dokos.repondre_json(200, &json!({"data": [{"name": "SALLE-G", "item_name": "Grande salle"}]}));

        let proposition = proposer_vente(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "25 repas…", jour())
            .expect("proposition");
        assert_eq!(proposition.type_vente, Some(TypeVente::Devis));
        assert_eq!(proposition.client.trouve().map(|c| c.id.as_str()), Some("CUST-2"));
        assert_eq!(proposition.date.map(|d| d.to_string()).as_deref(), Some("2026-11-12"));
        assert!(matches!(&proposition.lignes[0].article, Choix::APreciser { candidats, .. } if candidats.len() == 2));
        assert_eq!(proposition.lignes[1].article.trouve().map(|a| a.code.as_str()), Some("SALLE-G"));
        assert_eq!(proposition.questions, ["Plusieurs articles pour « repas » : lequel ?", "Repas végétariens ?"]);
        // L'IA ne fait que lire : seules des requêtes GET partent vers Dokos.
        assert!(dokos.requetes().iter().all(|r| r.methode == Methode::Get));
        let consigne =
            ia.requetes()[0].corps.as_ref().map(|c| c["messages"][0]["content"].to_string()).unwrap_or_default();
        assert!(consigne.contains("mardi 2026-11-03"));
        assert!(consigne.contains("Repas") && !consigne.contains("Restauration"), "{consigne}");
        assert!(proposition.avertissements.is_empty());
    }

    #[test]
    fn informations_manquantes_en_questions() {
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &reponse_ia(
                r#"{"client": "Licorne SA", "date": "le douze", "lignes": [{"article": "repas", "quantite": -3}]}"#,
            ),
        );
        let dokos = FauxTransport::new();
        dokos.repondre(403, "{}"); // groupes non lisibles : simple avertissement
        dokos.repondre_json(200, &json!({"data": []}));
        dokos.repondre_json(200, &json!({"data": [{"name": "REPAS", "item_name": "Repas"}]}));
        let proposition =
            proposer_vente(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "…", jour()).expect("proposition");
        assert_eq!(proposition.type_vente, None);
        assert_eq!(proposition.avertissements.len(), 1);
        assert_eq!(proposition.client, Choix::Introuvable("Licorne SA".into()));
        assert_eq!(proposition.lignes[0].quantite, None);
        assert_eq!(
            proposition.questions,
            [
                "Aucun client actif « Licorne SA » dans Dokos : lequel ?",
                "Date « le douze » illisible : quelle date ?",
                "Quelle quantité pour « repas » ?",
            ]
        );
    }

    #[test]
    fn demande_vide_sans_appel() {
        let ia = FauxTransport::new();
        let dokos = FauxTransport::new();
        assert!(proposer_vente(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "  ", jour()).is_err());
        assert!(ia.requetes().is_empty());
    }
}
