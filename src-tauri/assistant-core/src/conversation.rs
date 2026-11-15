//! Chatbot de l'appli : qualifie la demande, indique l'onglet où la faire,
//! explique la procédure et, quand la demande est assez complète, propose
//! de pré-remplir le formulaire de l'onglet.
//!
//! Il ne lit ni n'écrit rien dans Dokos : sa seule source est le guide de
//! l'utilisateur, joint à la compilation. Sa réponse est un JSON strict ;
//! l'onglet et l'action sont vérifiés ici, le texte n'est jamais interprété
//! comme du HTML par l'interface.

use std::fmt;
use std::str::FromStr;

use dokos_core::Transport;
use dokos_core::date::Date;
use dokos_core::fiche::{Fiche, GenreFiche};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::owui::{ErreurIa, OpenWebUi, TEXTE_MAX, Tour, borner};

/// Guide de l'utilisateur : la référence du chatbot sur l'appli.
const GUIDE: &str = include_str!("../../../docs/guide-utilisateur.md");

/// Messages envoyés au modèle au plus (les plus récents).
pub const MESSAGES_MAX: usize = 20;
/// Taille totale de la conversation envoyée, en caractères.
const CONVERSATION_MAX: usize = 24_000;
/// Longueur maximale de la réponse affichée.
pub const REPONSE_MAX: usize = 2_000;
const ETAPES_MAX: usize = 8;
const ETAPE_MAX: usize = 300;
/// Taille maximale des fiches renvoyées au modèle et gardées dans l'historique.
pub const DONNEES_MAX: usize = 8_000;
/// Longueur maximale du texte cherché.
const RECHERCHE_MAX: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Onglet {
    Ventes,
    Import,
    Export,
    Suivi,
    Formulaires,
    Reglages,
}

impl Onglet {
    pub const TOUS: [Onglet; 6] =
        [Onglet::Ventes, Onglet::Import, Onglet::Export, Onglet::Suivi, Onglet::Formulaires, Onglet::Reglages];

    pub fn nom(self) -> &'static str {
        match self {
            Self::Ventes => "ventes",
            Self::Import => "import",
            Self::Export => "export",
            Self::Suivi => "suivi",
            Self::Formulaires => "formulaires",
            Self::Reglages => "reglages",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Ventes => "Ventes",
            Self::Import => "Import",
            Self::Export => "Export",
            Self::Suivi => "Suivi",
            Self::Formulaires => "Formulaires",
            Self::Reglages => "Réglages",
        }
    }

    /// Ce que l'onglet permet, pour les consignes du modèle.
    fn role(self) -> &'static str {
        match self {
            Self::Ventes => "devis, commandes et factures clients en brouillon ; catalogue d'articles",
            Self::Import => {
                "créer en masse depuis un tableau Excel (factures, factures à partir d'un modèle, articles, clients ; modèle généré, analyse, création tout ou rien)"
            }
            Self::Export => "fichier Excel des clients, articles, devis, commandes, factures ou employés, avec filtres",
            Self::Suivi => {
                "brouillons en attente, devis → commande → facture, inscrits d'un événement, échéances des contrats de travail, factures en retard"
            }
            Self::Formulaires => "événement public et formulaire d'inscription, créés non publiés",
            Self::Reglages => "adresse de Dokos, clés API par profil, assistant IA, diagnostic",
        }
    }

    /// Onglet dont le formulaire peut être pré-rempli à partir d'une demande.
    pub fn pre_remplissable(self) -> bool {
        matches!(self, Self::Ventes | Self::Export | Self::Formulaires)
    }
}

impl fmt::Display for Onglet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

impl FromStr for Onglet {
    type Err = String;

    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        let cible = texte.trim().to_lowercase();
        Self::TOUS
            .into_iter()
            .find(|o| o.nom() == cible || o.libelle().to_lowercase() == cible)
            .ok_or_else(|| format!("onglet « {texte} » inconnu"))
    }
}

/// Message de la conversation, tel qu'il est affiché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Utilisateur(String),
    /// Réponse affichée, et fiches Dokos qu'elle a reçues (JSON), s'il y en a eu.
    Assistant {
        texte: String,
        donnees: Option<String>,
    },
}

/// Réponse vérifiée du chatbot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReponseChat {
    pub texte: String,
    /// Procédure dans l'appli, étape par étape (souvent vide).
    pub etapes: Vec<String>,
    /// Onglet où se fait la démarche.
    pub onglet: Option<Onglet>,
    /// Demande reformulée à transmettre au pré-remplissage de `onglet`.
    pub pre_remplir: Option<String>,
    /// Fiches trouvées par une recherche dans Dokos, affichées sous la réponse.
    pub fiches: Vec<Fiche>,
    /// Les mêmes fiches en JSON, à renvoyer avec ce message dans l'historique.
    pub donnees: Option<String>,
    pub avertissements: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Brut {
    reponse: String,
    onglet: Option<String>,
    etapes: Vec<String>,
    pre_remplir: Option<bool>,
    demande: Option<String>,
    recherche: Option<RechercheBrute>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RechercheBrute {
    #[serde(rename = "type")]
    genre: String,
    texte: String,
}

const SCHEMA: &str = r#"{
  "reponse": "texte affiché à l'utilisateur",
  "onglet": "ventes" | "import" | "export" | "suivi" | "formulaires" | "reglages" | null,
  "etapes": ["étape courte de la procédure dans l'appli"],
  "pre_remplir": true | false,
  "demande": "demande complète et autonome à pré-remplir" | null,
  "recherche": {"type": "client" | "article", "texte": "nom, code, e-mail ou téléphone cherché"} | null
}"#;

fn consignes(aujourdhui: Date, courant: Option<Onglet>) -> String {
    let onglets = Onglet::TOUS
        .iter()
        .map(|o| {
            format!(
                "- {} ({}) : {}{}",
                o.nom(),
                o.libelle(),
                o.role(),
                if o.pre_remplissable() { " — pré-remplissage possible" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let ici = courant.map_or("aucun".to_owned(), |o| format!("{} ({})", o.nom(), o.libelle()));
    format!(
        "Tu es l'assistant de « Dokos Asso », une application de bureau qui prépare des documents dans \
Dokos, le logiciel de gestion de l'utilisateur. Tu qualifies sa demande, tu lui indiques l'onglet où la faire \
et tu expliques la procédure.\n\
Onglets de l'application :\n{onglets}\n\
Règles :\n\
- Réponds uniquement par un objet JSON conforme au schéma ci-dessous, sans aucun texte autour.\n\
- \"reponse\" : en français, en vouvoyant, cinq phrases au plus, sans mise en forme (ni astérisques, ni dièses).\n\
- Si la demande est floue ou incomplète, pose UNE question précise pour la qualifier (type de document, client, \
dates, quantités…).\n\
- \"onglet\" : la clé de l'onglet où se fait la démarche, ou null (question générale ou hors application).\n\
- \"etapes\" : la procédure dans l'application, de deux à six étapes courtes, seulement quand tu expliques comment \
faire ; sinon [].\n\
- \"pre_remplir\" : true seulement si l'onglet permet le pré-remplissage et que la conversation donne l'essentiel \
(par exemple le client et les articles d'un devis). \"demande\" reprend alors, en une ou deux phrases autonomes, \
toutes les informations données dans la conversation, sans rien ajouter.\n\
- Pour une question sur un client ou un article précis (existe-t-il, coordonnées, adresse, prix, description…), \
demande une recherche dans Dokos avec \"recherche\" et laisse \"reponse\" vide : tu recevras les fiches trouvées, \
puis tu répondras. Une seule recherche par message.\n\
- Des données de Dokos, tu ne connais que les fiches reçues par une recherche, dans ce message ou les précédents : \
n'invente jamais un client, un article, un montant ni un nombre. Pour des listes ou des totaux, renvoie vers Export \
ou Suivi.\n\
- Les fiches sont affichées à l'utilisateur sous ta réponse : résume l'essentiel sans tout recopier. Si un client \
ou un article trouvé sert ensuite à pré-remplir, reprends son identifiant exact dans \"demande\".\n\
- Tu ne crées et ne valides rien : l'utilisateur relit le formulaire et clique lui-même. Ne dis jamais qu'un \
document est créé.\n\
- Hors du périmètre de l'application (comptabilité, paie, conseil juridique), dis-le simplement et renvoie vers \
Dokos ou un professionnel.\n\
Aujourd'hui, nous sommes le {} {aujourdhui}. L'utilisateur est dans l'onglet : {ici}.\n\
Guide de l'application (ta référence) :\n<<<\n{GUIDE}\n>>>\n\
Schéma :\n{SCHEMA}",
        aujourdhui.jour_semaine()
    )
}

/// Garde les messages les plus récents, dans la limite de taille ; la
/// conversation envoyée commence par un message de l'utilisateur.
fn preparer(messages: &[Message]) -> Result<Vec<Tour>, ErreurIa> {
    let Some(Message::Utilisateur(dernier)) = messages.last() else {
        return Err(ErreurIa::Demande("écrivez d'abord votre message".to_owned()));
    };
    borner(dernier)?;
    let mut tours = Vec::new();
    let mut total = 0;
    for message in messages.iter().rev().take(MESSAGES_MAX) {
        let tour = match message {
            Message::Utilisateur(t) => {
                let t: String = t.trim().chars().take(TEXTE_MAX).collect();
                if t.is_empty() {
                    continue;
                }
                Tour::Utilisateur(t)
            }
            // Les réponses passées sont rendues au modèle sous la forme qu'il doit produire,
            // avec les fiches Dokos qu'elles avaient reçues.
            Message::Assistant { texte, donnees } => {
                let t: String = texte.trim().chars().take(REPONSE_MAX).collect();
                let mut tour = json!({ "reponse": t });
                if let Some(d) = donnees.as_deref().filter(|d| d.chars().count() <= DONNEES_MAX) {
                    tour["fiches_dokos"] = serde_json::from_str(d).unwrap_or_else(|_| Value::String(d.to_owned()));
                }
                Tour::Assistant(tour.to_string())
            }
        };
        let taille = match &tour {
            Tour::Utilisateur(t) | Tour::Assistant(t) => t.chars().count(),
        };
        if total + taille > CONVERSATION_MAX && !tours.is_empty() {
            break;
        }
        total += taille;
        tours.push(tour);
    }
    tours.reverse();
    while matches!(tours.first(), Some(Tour::Assistant(_))) {
        tours.remove(0);
    }
    Ok(tours)
}

fn nettoyer(texte: &str, max: usize) -> String {
    texte.trim().chars().take(max).collect()
}

/// Fiches en JSON compact pour le modèle, dans la limite de [`DONNEES_MAX`].
fn fiches_json(fiches: &[Fiche]) -> String {
    let mut valeurs: Vec<Value> = fiches
        .iter()
        .map(|f| {
            let infos: serde_json::Map<String, Value> =
                f.champs.iter().map(|(l, v)| (l.clone(), Value::String(v.clone()))).collect();
            json!({"type": f.genre.nom(), "id": f.id, "nom": f.titre, "desactive": f.desactive, "infos": infos})
        })
        .collect();
    loop {
        let texte = Value::Array(valeurs.clone()).to_string();
        if texte.chars().count() <= DONNEES_MAX || valeurs.is_empty() {
            return texte;
        }
        valeurs.pop();
    }
}

/// Une réponse du chatbot. Si le modèle demande une recherche, `chercher`
/// la fait (lecture seule dans Dokos), puis le modèle répond avec les fiches.
pub fn discuter<I: Transport>(
    ia: &OpenWebUi<I>,
    bases: &[String],
    messages: &[Message],
    courant: Option<Onglet>,
    aujourdhui: Date,
    mut chercher: impl FnMut(GenreFiche, &str) -> Result<Vec<Fiche>, ErreurIa>,
) -> Result<ReponseChat, ErreurIa> {
    let mut tours = preparer(messages)?;
    let consignes = consignes(aujourdhui, courant);
    let reponse = ia.demander_json_conversation::<Brut>(&consignes, &tours, bases)?;
    let mut brut = reponse.donnees;
    let mut avertissements = reponse.avertissements;
    let mut fiches = Vec::new();
    let mut donnees = None;

    if let Some(recherche) = brut.recherche.take() {
        let texte = nettoyer(&recherche.texte, RECHERCHE_MAX);
        match recherche.genre.parse::<GenreFiche>() {
            Ok(genre) if !texte.is_empty() => {
                let resultat = match chercher(genre, &texte) {
                    Ok(trouvees) => {
                        let json_fiches = fiches_json(&trouvees);
                        fiches = trouvees;
                        donnees = Some(json_fiches.clone());
                        format!(
                            "Résultat de la recherche dans Dokos ({} fiche(s) {genre} pour « {texte} ») : {json_fiches}",
                            fiches.len()
                        )
                    }
                    Err(e) => {
                        avertissements.push(format!("recherche dans Dokos impossible : {e}"));
                        format!("La recherche dans Dokos a échoué ({e}) : dis-le simplement.")
                    }
                };
                tours.push(Tour::Assistant(json!({"recherche": {"type": genre.nom(), "texte": texte}}).to_string()));
                tours.push(Tour::Utilisateur(format!(
                    "{resultat}\nRéponds maintenant à ma demande avec ces données, sans nouvelle recherche."
                )));
                let suite = ia.demander_json_conversation::<Brut>(&consignes, &tours, bases)?;
                avertissements.extend(suite.avertissements);
                brut = suite.donnees;
                if brut.recherche.is_some() {
                    avertissements.push("seconde recherche demandée par l'IA : ignorée".to_owned());
                }
            }
            Ok(_) => avertissements.push("recherche demandée sans texte : ignorée".to_owned()),
            Err(e) => avertissements.push(format!("{e} : recherche ignorée")),
        }
    }

    let onglet = match brut.onglet.as_deref().map(str::trim).filter(|o| !o.is_empty() && *o != "null") {
        Some(o) => match o.parse::<Onglet>() {
            Ok(o) => Some(o),
            Err(e) => {
                avertissements.push(format!("{e} dans la réponse de l'IA : ignoré"));
                None
            }
        },
        None => None,
    };
    let etapes: Vec<String> =
        brut.etapes.iter().map(|e| nettoyer(e, ETAPE_MAX)).filter(|e| !e.is_empty()).take(ETAPES_MAX).collect();
    let demande = brut.demande.as_deref().map(|d| nettoyer(d, TEXTE_MAX)).filter(|d| !d.is_empty() && d != "null");
    let pre_remplir = match (brut.pre_remplir.unwrap_or(false), onglet, demande) {
        (true, Some(o), Some(d)) if o.pre_remplissable() => Some(d),
        _ => None,
    };
    let mut texte = nettoyer(&brut.reponse, REPONSE_MAX);
    if texte.is_empty() {
        texte = match (onglet, fiches.len()) {
            (_, n) if n > 0 => {
                format!("Voici ce que j'ai trouvé dans Dokos ({n} fiche{}).", if n > 1 { "s" } else { "" })
            }
            (Some(o), _) => format!("Cela se fait dans l'onglet {}.", o.libelle()),
            (None, _) => "Pouvez-vous préciser ce que vous voulez faire ?".to_owned(),
        };
    }
    Ok(ReponseChat { texte, etapes, onglet, pre_remplir, fiches, donnees, avertissements })
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::Value;

    use super::*;
    use crate::owui::tests::reponse_ia;

    fn jour() -> Date {
        "2026-11-05".parse().expect("date")
    }

    fn u(t: &str) -> Message {
        Message::Utilisateur(t.to_owned())
    }

    fn a(t: &str) -> Message {
        Message::Assistant { texte: t.to_owned(), donnees: None }
    }

    fn sans_recherche(_: GenreFiche, _: &str) -> Result<Vec<Fiche>, ErreurIa> {
        panic!("aucune recherche attendue")
    }

    fn envoyes(faux: &FauxTransport) -> Vec<Value> {
        faux.requetes()[0].corps.as_ref().and_then(|c| c["messages"].as_array().cloned()).unwrap_or_default()
    }

    #[test]
    fn question_de_qualification() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_ia(
                r#"{"reponse": "Pour quel client, et s'agit-il d'un devis ou d'une facture ?", "onglet": "ventes",
                    "etapes": [], "pre_remplir": false, "demande": null}"#,
            ),
        );
        let r = discuter(
            &OpenWebUi::new(&faux, "m"),
            &[],
            &[u("je dois facturer des repas")],
            None,
            jour(),
            sans_recherche,
        )
        .expect("réponse");
        assert_eq!(r.onglet, Some(Onglet::Ventes));
        assert_eq!(r.pre_remplir, None);
        assert!(r.texte.starts_with("Pour quel client"));

        let messages = envoyes(&faux);
        let systeme = messages[0]["content"].as_str().unwrap_or_default();
        assert!(systeme.contains("jeudi 2026-11-05"));
        assert!(systeme.contains("Guide de l'utilisateur — Dokos Asso"), "guide joint");
        assert!(systeme.contains("export (Export)"));
        assert_eq!(messages[1], json!({"role": "user", "content": "je dois facturer des repas"}));
    }

    #[test]
    fn pre_remplissage_apres_qualification() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_ia(
                r#"<think>…</think>{"reponse": "Je pré-remplis le devis.", "onglet": "Ventes",
                    "etapes": ["Relisez le client", " ", "Cliquez sur Vérifier"], "pre_remplir": true,
                    "demande": "Devis pour la mairie de Crest : 25 repas le 12 novembre 2026"}"#,
            ),
        );
        let conversation = [
            u("je veux faire un devis de repas"),
            a("Pour quel client et combien de repas ?"),
            u("mairie de Crest, 25 repas le 12/11"),
        ];
        let r = discuter(&OpenWebUi::new(&faux, "m"), &[], &conversation, Some(Onglet::Suivi), jour(), sans_recherche)
            .expect("réponse");
        assert_eq!(r.onglet, Some(Onglet::Ventes));
        assert_eq!(r.pre_remplir.as_deref(), Some("Devis pour la mairie de Crest : 25 repas le 12 novembre 2026"));
        assert_eq!(r.etapes, ["Relisez le client", "Cliquez sur Vérifier"]);

        let messages = envoyes(&faux);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], r#"{"reponse":"Pour quel client et combien de repas ?"}"#);
        assert!(messages[0]["content"].as_str().unwrap_or_default().contains("l'onglet : suivi (Suivi)"));
    }

    #[test]
    fn action_refusee_hors_formulaire() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_ia(r#"{"reponse": "", "onglet": "suivi", "pre_remplir": true, "demande": "factures en retard"}"#),
        );
        faux.repondre_json(200, &reponse_ia(r#"{"reponse": "Voici.", "onglet": "comptabilite"}"#));
        let ia = OpenWebUi::new(&faux, "m");
        let r = discuter(&ia, &[], &[u("relances")], None, jour(), sans_recherche).expect("réponse");
        assert_eq!(r.onglet, Some(Onglet::Suivi));
        assert_eq!(r.pre_remplir, None, "Suivi n'a pas de formulaire à pré-remplir");
        assert_eq!(r.texte, "Cela se fait dans l'onglet Suivi.");

        let r = discuter(&ia, &[], &[u("bilan comptable")], None, jour(), sans_recherche).expect("réponse");
        assert_eq!(r.onglet, None);
        assert_eq!(r.avertissements.len(), 1);
    }

    #[test]
    fn conversation_bornee() {
        assert!(preparer(&[]).is_err());
        assert!(preparer(&[u("bonjour"), a("Bonjour !")]).is_err(), "le dernier message est celui de l'utilisateur");
        assert!(preparer(&[u("  ")]).is_err());
        assert!(preparer(&[u(&"x".repeat(TEXTE_MAX + 1))]).is_err());

        let longue: Vec<Message> =
            (0..40).flat_map(|i| [u(&format!("question {i}")), a(&format!("réponse {i}"))]).chain([u("fin")]).collect();
        let tours = preparer(&longue).expect("tours");
        assert!(tours.len() <= MESSAGES_MAX);
        assert!(matches!(tours.first(), Some(Tour::Utilisateur(_))));
        assert_eq!(tours.last(), Some(&Tour::Utilisateur("fin".into())));

        let lourde: Vec<Message> = (0..6).map(|_| u(&"y".repeat(7_000))).collect();
        let total: usize = preparer(&lourde)
            .expect("tours")
            .iter()
            .map(|t| match t {
                Tour::Utilisateur(t) | Tour::Assistant(t) => t.len(),
            })
            .sum();
        assert!(total <= CONVERSATION_MAX);
    }

    #[test]
    fn onglets() {
        assert_eq!("Réglages".parse::<Onglet>(), Ok(Onglet::Reglages));
        assert_eq!(" export ".parse::<Onglet>(), Ok(Onglet::Export));
        assert!("contrats".parse::<Onglet>().is_err());
        assert!("Comptabilité".parse::<Onglet>().is_err());
        assert!(Onglet::Export.pre_remplissable() && !Onglet::Import.pre_remplissable());
    }

    fn fiche_mairie() -> Fiche {
        Fiche {
            genre: GenreFiche::Client,
            id: "CUST-2".into(),
            titre: "Mairie de Crest".into(),
            desactive: false,
            champs: vec![("Téléphone".into(), "04 75 25 00 00".into())],
        }
    }

    #[test]
    fn recherche_puis_reponse() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &reponse_ia(r#"{"reponse": "", "recherche": {"type": "client", "texte": "mairie de Crest"}}"#),
        );
        faux.repondre_json(
            200,
            &reponse_ia(
                r#"{"reponse": "Le téléphone de la Mairie de Crest (CUST-2) est le 04 75 25 00 00.", "onglet": null,
                    "recherche": {"type": "article", "texte": "salle"}}"#,
            ),
        );
        let mut cherches = Vec::new();
        let r = discuter(
            &OpenWebUi::new(&faux, "m"),
            &[],
            &[u("le téléphone de la mairie de Crest ?")],
            None,
            jour(),
            |g, t| {
                cherches.push((g, t.to_owned()));
                Ok(vec![fiche_mairie()])
            },
        )
        .expect("réponse");
        assert_eq!(cherches, [(GenreFiche::Client, "mairie de Crest".to_owned())]);
        assert!(r.texte.contains("04 75 25 00 00"));
        assert_eq!(r.fiches.len(), 1);
        assert!(r.donnees.as_deref().is_some_and(|d| d.contains("CUST-2")));
        assert_eq!(r.avertissements, ["seconde recherche demandée par l'IA : ignorée"]);

        // Second appel : la demande de recherche puis les fiches, rendues au modèle.
        let messages =
            faux.requetes()[1].corps.as_ref().and_then(|c| c["messages"].as_array().cloned()).unwrap_or_default();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2]["role"], "assistant");
        assert!(messages[3]["content"].as_str().is_some_and(|c| c.contains("\"id\":\"CUST-2\"")));
    }

    #[test]
    fn fiches_gardees_dans_l_historique() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &reponse_ia(r#"{"reponse": "Je pré-remplis.", "onglet": "ventes"}"#));
        let donnees = fiches_json(&[fiche_mairie()]);
        let conversation = [
            u("la mairie de Crest est-elle cliente ?"),
            Message::Assistant { texte: "Oui : CUST-2.".into(), donnees: Some(donnees) },
            u("fais-lui un devis de 10 repas"),
        ];
        discuter(&OpenWebUi::new(&faux, "m"), &[], &conversation, None, jour(), sans_recherche).expect("réponse");
        let messages = envoyes(&faux);
        assert_eq!(messages[2]["role"], "assistant");
        let passe: Value = serde_json::from_str(messages[2]["content"].as_str().unwrap_or("{}")).expect("JSON");
        assert_eq!(passe["fiches_dokos"][0]["id"], "CUST-2");
    }

    #[test]
    fn recherche_en_echec_signalee() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &reponse_ia(r#"{"recherche": {"type": "client", "texte": "Licorne"}}"#));
        faux.repondre_json(200, &reponse_ia(r#"{"reponse": "Je n'ai pas pu chercher dans Dokos."}"#));
        let r = discuter(&OpenWebUi::new(&faux, "m"), &[], &[u("Licorne est cliente ?")], None, jour(), |_, _| {
            Err(ErreurIa::Demande("profil Ventes non configuré".into()))
        })
        .expect("réponse");
        assert!(r.fiches.is_empty() && r.donnees.is_none());
        assert_eq!(r.avertissements.len(), 1);

        let lourdes: Vec<Fiche> = (0..200)
            .map(|i| Fiche {
                id: format!("CUST-{i}"),
                champs: vec![("Notes".into(), "x".repeat(300))],
                ..fiche_mairie()
            })
            .collect();
        assert!(fiches_json(&lourdes).chars().count() <= DONNEES_MAX);
    }
}
