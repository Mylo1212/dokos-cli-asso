//! Assistance par IA : conversion des propositions pour l'interface.
//! Rien n'est écrit dans Dokos ici.

use assistant_core::conversation::{Message, Onglet, discuter};
use assistant_core::ventes::proposer_vente;
use assistant_core::{Choix, ErreurIa, OpenWebUi};
use dokos_core::date::Date;
use dokos_core::fiche::{Fiche, GenreFiche};
use dokos_core::vente::TypeVente;
use dokos_core::{Dokos, Transport};

use crate::services::{Resultat, lien_desk};

use crate::vues::*;

use crate::erreurs::{ErreurVue, erreur as texte};

pub(crate) fn vue_choix<C, V>(choix: Choix<C>, vers_vue: impl Fn(C) -> V) -> ChoixVue<V> {
    match choix {
        Choix::NonPrecise => {
            ChoixVue { statut: "non_precise".into(), texte: String::new(), valeur: None, candidats: Vec::new() }
        }
        Choix::Trouve(c) => {
            ChoixVue { statut: "trouve".into(), texte: String::new(), valeur: Some(vers_vue(c)), candidats: Vec::new() }
        }
        Choix::APreciser { texte, candidats } => ChoixVue {
            statut: "a_preciser".into(),
            texte,
            valeur: None,
            candidats: candidats.into_iter().map(vers_vue).collect(),
        },
        Choix::Introuvable(texte) => {
            ChoixVue { statut: "introuvable".into(), texte, valeur: None, candidats: Vec::new() }
        }
    }
}

pub fn aujourdhui(texte_date: &str) -> Resultat<Date> {
    texte_date.parse().map_err(texte)
}

pub fn vente<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    jour: Date,
) -> Resultat<PropositionVenteVue> {
    let p = proposer_vente(dokos, ia, bases, demande, jour).map_err(texte)?;
    Ok(PropositionVenteVue {
        type_vente: p.type_vente.map(|t| {
            match t {
                TypeVente::Devis => "devis",
                TypeVente::Commande => "commande",
                TypeVente::Facture => "facture",
            }
            .to_owned()
        }),
        client: vue_choix(p.client, |c| ClientVue { id: c.id, nom: c.nom }),
        date: p.date.map(|d| d.to_string()),
        echeance: p.echeance.map(|d| d.to_string()),
        lignes: p
            .lignes
            .into_iter()
            .map(|l| LigneProposeeVue {
                texte: l.texte,
                article: vue_choix(l.article, crate::services::vue_article),
                quantite: l.quantite,
                livraison: l.livraison.map(|d| d.to_string()),
            })
            .collect(),
        questions: p.questions,
        avertissements: p.avertissements,
    })
}

/// Réponse du chatbot. `onglet` est l'onglet affiché (inconnu : ignoré) ;
/// `chercher` lit des fiches clients ou articles quand l'IA le demande.
pub fn chat<I: Transport>(
    ia: &OpenWebUi<I>,
    bases: &[String],
    messages: Vec<MessageChatVue>,
    onglet: &str,
    jour: Date,
    base: &str,
    chercher: impl FnMut(GenreFiche, &str) -> Result<Vec<Fiche>, ErreurIa>,
) -> Resultat<ReponseChatVue> {
    let messages = messages
        .into_iter()
        .map(|m| match m.auteur.as_str() {
            "utilisateur" => Ok(Message::Utilisateur(m.texte)),
            "assistant" => Ok(Message::Assistant { texte: m.texte, donnees: m.donnees }),
            autre => Err(ErreurVue::validation(format!("auteur de message « {autre} » inconnu"))),
        })
        .collect::<Resultat<Vec<_>>>()?;
    let r = discuter(ia, bases, &messages, onglet.parse::<Onglet>().ok(), jour, chercher).map_err(texte)?;
    Ok(ReponseChatVue {
        texte: r.texte,
        etapes: r.etapes,
        onglet: r.onglet.map(|o| o.nom().to_owned()),
        libelle_onglet: r.onglet.map(|o| o.libelle().to_owned()),
        pre_remplir: r.pre_remplir,
        fiches: r
            .fiches
            .into_iter()
            .map(|f| FicheVue {
                genre: f.genre.nom().to_owned(),
                lien: lien_desk(base, f.genre.doctype(), &f.id),
                id: f.id,
                titre: f.titre,
                desactive: f.desactive,
                champs: f.champs.into_iter().map(|(libelle, valeur)| ChampFicheVue { libelle, valeur }).collect(),
            })
            .collect(),
        donnees: r.donnees,
        avertissements: r.avertissements,
    })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    #[test]
    fn chat_vers_un_onglet() {
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &json!({"choices": [{"message": {"content":
                r#"{"reponse": "Dans Export.", "onglet": "export", "etapes": ["Choisir Clients"],
                    "pre_remplir": true, "demande": "liste des clients"}"#}}]}),
        );
        let messages =
            vec![MessageChatVue { auteur: "utilisateur".into(), texte: "la liste des clients".into(), donnees: None }];
        let vue = chat(
            &OpenWebUi::new(&ia, "m"),
            &[],
            messages,
            "ventes",
            "2026-11-05".parse().expect("date"),
            "https://d.test",
            |_, _| Ok(Vec::new()),
        )
        .expect("réponse");
        assert_eq!(vue.onglet.as_deref(), Some("export"));
        assert_eq!(vue.libelle_onglet.as_deref(), Some("Export"));
        assert_eq!(vue.pre_remplir.as_deref(), Some("liste des clients"));

        let messages = vec![MessageChatVue { auteur: "systeme".into(), texte: "x".into(), donnees: None }];
        let jour = "2026-11-05".parse().expect("date");
        assert!(chat(&OpenWebUi::new(&ia, "m"), &[], messages, "", jour, "", |_, _| Ok(Vec::new())).is_err());
    }

    #[test]
    fn vente_proposee() {
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &json!({"choices": [{"message": {"content":
                r#"{"type_document": "commande", "client": "Mairie de Crest", "date": "2026-11-12",
                    "lignes": [{"article": "repas", "quantite": 25}]}"#}}]}),
        );
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": []})); // groupes d'articles
        dokos.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        dokos.repondre_json(200, &json!({"data": []}));
        let vue = vente(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "…", "2026-11-03".parse().expect("date"))
            .expect("proposition");
        assert_eq!(vue.type_vente.as_deref(), Some("commande"));
        assert_eq!(vue.client.statut, "trouve");
        assert_eq!(vue.client.valeur.as_ref().map(|c| c.id.as_str()), Some("CUST-2"));
        assert_eq!(vue.lignes[0].article.statut, "introuvable");
        assert_eq!(vue.lignes[0].article.texte, "repas");
    }
}
