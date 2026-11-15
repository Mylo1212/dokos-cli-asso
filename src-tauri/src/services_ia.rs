//! Assistance par IA : conversion des propositions pour l'interface.
//! Rien n'est écrit dans Dokos ici ; seul le fichier de proposition de
//! format d'impression est enregistré, dans Documents\Dokos.

use std::path::Path;

use assistant_core::contrats::proposer_contrat;
use assistant_core::conversation::{Message, Onglet, discuter};
use assistant_core::evenements::proposer_evenement;
use assistant_core::formats::proposer_format;
use assistant_core::ventes::proposer_vente;
use assistant_core::{Choix, ErreurIa, OpenWebUi};
use conformite::Regles;
use conformite::dokos::champs_contrat;
use conformite::gabarit::analyser;
use conformite::proposition::difference;
use conformite::rapport::verifier_gabarit;
use conformite::regles::Contexte;
use dokos_core::contrat::DOCTYPE_CONTRAT;
use dokos_core::date::{Date, DateHeure};
use dokos_core::employe::DOCTYPE_EMPLOYE;
use dokos_core::fiche::{Fiche, GenreFiche};
use dokos_core::impression::lire_format;
use dokos_core::vente::TypeVente;
use dokos_core::{Dokos, Transport};

use crate::services::{Resultat, compacter_diff, lien_desk};

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

/// `2026-12-14 14:00:00` → `2026-12-14T14:00`, pour un champ `datetime-local`.
pub fn date_heure_locale(d: DateHeure) -> String {
    let texte = d.to_string();
    texte.get(..16).unwrap_or(&texte).replacen(' ', "T", 1)
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

pub fn evenement<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    jour: Date,
) -> Resultat<PropositionEvenementVue> {
    let p = proposer_evenement(dokos, ia, bases, demande, jour).map_err(texte)?;
    Ok(PropositionEvenementVue {
        titre: p.titre,
        debut: p.debut.map(date_heure_locale),
        fin: p.fin.map(date_heure_locale),
        journee: p.journee_entiere,
        lieu: p.lieu,
        description: p.description,
        places: p.places,
        annulation: p.annulation_autorisee,
        questions_formulaire: p
            .questions_formulaire
            .into_iter()
            .map(|q| QuestionVue { champ: q.champ, obligatoire: q.obligatoire })
            .collect(),
        champs_a_creer: p.champs_a_creer,
        questions: p.questions,
        avertissements: p.avertissements,
    })
}

pub fn contrat<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    base: &str,
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    jour: Date,
) -> Resultat<PropositionContratVue> {
    let p = proposer_contrat(dokos, ia, bases, demande, jour).map_err(texte)?;
    Ok(PropositionContratVue {
        employe: vue_choix(p.employe, |e| EmployeVue {
            lien: lien_desk(base, DOCTYPE_EMPLOYE, &e.id),
            id: e.id,
            nom: e.nom_complet,
            entree: e.entree,
        }),
        nouvel_employe: p.nouvel_employe.map(|e| EmployeProposeVue {
            prenom: e.prenom,
            nom: e.nom,
            naissance: e.naissance.map(|d| d.to_string()),
            genre: e.genre,
        }),
        type_contrat: p.type_contrat,
        poste: p.poste,
        poste_connu: p.poste_connu,
        entree: p.entree.map(|d| d.to_string()),
        fin: p.fin.map(|d| d.to_string()),
        heures: p.heures,
        repartition: p.repartition,
        champs: p.champs,
        questions: p.questions,
        avertissements: p.avertissements,
    })
}

/// Réponse du chatbot. `onglet` est l'onglet affiché (inconnu : ignoré).
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

/// Rédaction par l'IA des mentions manquantes d'un format, enregistrée
/// dans `dossier`. Le format dans Dokos n'est pas modifié.
#[allow(clippy::too_many_arguments)]
pub fn format<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    ia: &OpenWebUi<I>,
    bases: &[String],
    regles: &Regles,
    nom_format: &str,
    contexte: &Contexte,
    jour: Date,
    dossier: &Path,
) -> Resultat<Option<PropositionFormatIaVue>> {
    let format = lire_format(dokos, nom_format).map_err(texte)?;
    if format.doc_type != DOCTYPE_CONTRAT || !format.est_jinja() {
        return Err(ErreurVue::validation(format!(
            "le format « {nom_format} » n'est pas un gabarit HTML/Jinja de contrat de travail"
        )));
    }
    let champs = champs_contrat(dokos).map_err(texte)?;
    let rapport = verifier_gabarit(regles, &analyser(&format.html), contexte, &champs);
    let Some(p) = proposer_format(ia, bases, regles, &format.html, &rapport, &champs, jour).map_err(texte)? else {
        return Ok(None);
    };
    let mut champs_apres = champs.clone();
    champs_apres.extend(p.champs_a_creer.iter().cloned());
    let apres = verifier_gabarit(regles, &p.analyse, contexte, &champs_apres);
    if !apres.interdits.is_empty() {
        return Err(ErreurVue::validation(
            "proposition refusée : le gabarit obtenu contient des constructions interdites",
        ));
    }
    let chemin = dossier.join(format!("{}.proposition-ia.html", dokos_config::nom_de_fichier(nom_format)));
    std::fs::write(&chemin, &p.gabarit).map_err(|e| ErreurVue::fichier(format!("{} : {e}", chemin.display())))?;
    Ok(Some(PropositionFormatIaVue {
        ajouts: p
            .ajouts
            .into_iter()
            .map(|a| AjoutIaVue { libelle: a.libelle, par_ia: a.par_ia, raison: a.raison })
            .collect(),
        explications: p.explications,
        champs_a_creer: p.champs_a_creer,
        a_completer: apres.a_completer,
        diff: compacter_diff(difference(&format.html, &p.gabarit)),
        chemin: chemin.display().to_string(),
        avertissements: p.avertissements,
    }))
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
                r#"{"reponse": "Dans Listes.", "onglet": "listes", "etapes": ["Choisir Clients"],
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
        assert_eq!(vue.onglet.as_deref(), Some("listes"));
        assert_eq!(vue.libelle_onglet.as_deref(), Some("Listes"));
        assert_eq!(vue.pre_remplir.as_deref(), Some("liste des clients"));

        let messages = vec![MessageChatVue { auteur: "systeme".into(), texte: "x".into(), donnees: None }];
        let jour = "2026-11-05".parse().expect("date");
        assert!(chat(&OpenWebUi::new(&ia, "m"), &[], messages, "", jour, "", |_, _| Ok(Vec::new())).is_err());
    }

    #[test]
    fn dates_heures_pour_le_formulaire() {
        let d: DateHeure = "2026-12-14 14:30".parse().expect("date");
        assert_eq!(date_heure_locale(d), "2026-12-14T14:30");
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
