//! Client Open WebUI : `POST /api/chat/completions` (format OpenAI), avec
//! les bases de connaissance jointes en `files: [{type: "collection", id}]`.
//!
//! Le modèle doit répondre par un objet JSON. La réponse est extraite (blocs
//! `<think>`, balises ```json, texte autour) puis désérialisée dans un type
//! strict ; en cas d'échec, une seule nouvelle demande est faite.

use std::fmt;

use dokos_core::DokosError;
use dokos_core::transport::{Requete, Transport};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// Modèle par défaut, tel que nommé dans Open WebUI.
pub const MODELE_PAR_DEFAUT: &str = "qwen3.6-35b-a3b";
/// Taille maximale d'un texte envoyé par l'utilisateur.
pub const TEXTE_MAX: usize = 8_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurIa {
    /// Serveur injoignable : Mac éteint, Tailscale déconnecté…
    Injoignable(String),
    CleRefusee,
    Service {
        statut: u16,
        message: String,
    },
    /// Le modèle n'a pas renvoyé de JSON exploitable, même après relance.
    ReponseIllisible(String),
    Dokos(DokosError),
    Demande(String),
}

impl fmt::Display for ErreurIa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Injoignable(m) => write!(
                f,
                "assistant IA injoignable ({m}) : vérifiez que le Mac est allumé et connecté à Tailscale. Le formulaire reste utilisable à la main."
            ),
            Self::CleRefusee => write!(f, "clé API Open WebUI refusée : vérifiez-la dans Réglages"),
            Self::Service { statut, message } => write!(f, "Open WebUI a répondu HTTP {statut} : {message}"),
            Self::ReponseIllisible(m) => {
                write!(f, "réponse de l'IA inexploitable ({m}) : remplissez le formulaire à la main ou reformulez")
            }
            Self::Dokos(e) => e.fmt(f),
            Self::Demande(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ErreurIa {}

impl From<DokosError> for ErreurIa {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

/// Tour d'une conversation envoyée au modèle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tour {
    Utilisateur(String),
    Assistant(String),
}

/// Réponse JSON de l'IA, avec les remarques sur son obtention.
#[derive(Debug, Clone, PartialEq)]
pub struct Reponse<D> {
    pub donnees: D,
    /// Par exemple : base de connaissance introuvable, réponse relancée.
    pub avertissements: Vec<String>,
}

pub struct OpenWebUi<T> {
    transport: T,
    modele: String,
}

impl<T: Transport> OpenWebUi<T> {
    pub fn new(transport: T, modele: &str) -> Self {
        let modele = modele.trim();
        Self { transport, modele: if modele.is_empty() { MODELE_PAR_DEFAUT } else { modele }.to_owned() }
    }

    pub fn modele(&self) -> &str {
        &self.modele
    }

    /// Modèles disponibles ; sert à tester la connexion.
    pub fn modeles(&self) -> Result<Vec<String>, ErreurIa> {
        #[derive(Deserialize)]
        struct Liste {
            data: Vec<Modele>,
        }
        #[derive(Deserialize)]
        struct Modele {
            id: String,
        }
        let reponse = self.transport.envoyer(&Requete::get("/api/models")).map_err(injoignable)?;
        verifier_statut(reponse.statut, &reponse.texte())?;
        let liste: Liste = serde_json::from_slice(&reponse.corps)
            .map_err(|e| ErreurIa::Service { statut: reponse.statut, message: e.to_string() })?;
        Ok(liste.data.into_iter().map(|m| m.id).collect())
    }

    /// Texte brut de la réponse du modèle.
    fn completer(&self, messages: &[Value], bases: &[String]) -> Result<String, ErreurIa> {
        let mut corps = json!({
            "model": self.modele,
            "stream": false,
            "temperature": 0,
            "messages": messages,
        });
        if !bases.is_empty() {
            let fichiers: Vec<Value> = bases.iter().map(|id| json!({"type": "collection", "id": id})).collect();
            corps["files"] = Value::Array(fichiers);
        }
        let reponse = self.transport.envoyer(&Requete::post("/api/chat/completions", corps)).map_err(injoignable)?;
        verifier_statut(reponse.statut, &reponse.texte())?;
        let valeur: Value = serde_json::from_slice(&reponse.corps)
            .map_err(|e| ErreurIa::Service { statut: reponse.statut, message: e.to_string() })?;
        let contenu = valeur
            .pointer("/choices/0/message/content")
            .or_else(|| valeur.pointer("/message/content"))
            .and_then(Value::as_str)
            .ok_or_else(|| ErreurIa::Service { statut: reponse.statut, message: "réponse sans contenu".into() })?;
        Ok(contenu.to_owned())
    }

    /// Demande un objet JSON. Si les bases de connaissance font échouer
    /// l'appel (identifiant inconnu…), relance sans elles et le signale.
    pub fn demander_json<D: DeserializeOwned>(
        &self,
        systeme: &str,
        utilisateur: &str,
        bases: &[String],
    ) -> Result<Reponse<D>, ErreurIa> {
        self.demander_json_conversation(systeme, &[Tour::Utilisateur(utilisateur.to_owned())], bases)
    }

    /// Comme [`Self::demander_json`], avec une conversation : tours de
    /// l'utilisateur et réponses précédentes de l'assistant, dans l'ordre.
    pub fn demander_json_conversation<D: DeserializeOwned>(
        &self,
        systeme: &str,
        conversation: &[Tour],
        bases: &[String],
    ) -> Result<Reponse<D>, ErreurIa> {
        let mut avertissements = Vec::new();
        let mut messages = vec![json!({"role": "system", "content": systeme})];
        messages.extend(conversation.iter().map(|tour| match tour {
            Tour::Utilisateur(texte) => json!({"role": "user", "content": texte}),
            Tour::Assistant(texte) => json!({"role": "assistant", "content": texte}),
        }));

        let mut bases_utilisees = bases.to_vec();
        let premier = match self.completer(&messages, &bases_utilisees) {
            Err(ErreurIa::Service { .. }) if !bases_utilisees.is_empty() => {
                avertissements.push(format!(
                    "bases de connaissance ignorées ({}) : introuvables ou refusées par Open WebUI, vérifiez leurs identifiants dans Réglages",
                    bases_utilisees.join(", ")
                ));
                bases_utilisees.clear();
                self.completer(&messages, &bases_utilisees)?
            }
            autre => autre?,
        };

        match decoder::<D>(&premier) {
            Ok(donnees) => Ok(Reponse { donnees, avertissements }),
            Err(erreur) => {
                messages.push(json!({"role": "assistant", "content": premier}));
                messages.push(json!({"role": "user", "content": format!(
                    "Ta réponse n'est pas exploitable ({erreur}). Réponds uniquement par l'objet JSON demandé, sans aucun texte autour."
                )}));
                let second = self.completer(&messages, &bases_utilisees)?;
                let donnees = decoder::<D>(&second).map_err(ErreurIa::ReponseIllisible)?;
                avertissements.push("réponse de l'IA relancée une fois (premier essai illisible)".to_owned());
                Ok(Reponse { donnees, avertissements })
            }
        }
    }
}

fn injoignable(e: DokosError) -> ErreurIa {
    match e {
        DokosError::Reseau(m) => ErreurIa::Injoignable(m),
        autre => ErreurIa::Dokos(autre),
    }
}

fn verifier_statut(statut: u16, corps: &str) -> Result<(), ErreurIa> {
    match statut {
        200..=299 => Ok(()),
        401 | 403 => Err(ErreurIa::CleRefusee),
        _ => {
            let message = serde_json::from_str::<Value>(corps)
                .ok()
                .and_then(|v| v.get("detail").or_else(|| v.get("error")).map(|d| d.to_string()))
                .unwrap_or_else(|| corps.chars().take(200).collect());
            Err(ErreurIa::Service { statut, message })
        }
    }
}

fn decoder<D: DeserializeOwned>(texte: &str) -> Result<D, String> {
    let json = extraire_json(texte).ok_or("aucun objet JSON trouvé")?;
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// Premier objet JSON équilibré du texte, hors raisonnement `<think>…</think>`.
pub fn extraire_json(texte: &str) -> Option<&str> {
    let sans_reflexion = match texte.rfind("</think>") {
        Some(fin) => &texte[fin + "</think>".len()..],
        None => texte,
    };
    let debut = sans_reflexion.find('{')?;
    let octets = sans_reflexion.as_bytes();
    let (mut profondeur, mut dans_chaine, mut echappe) = (0_usize, false, false);
    for (i, &c) in octets.iter().enumerate().skip(debut) {
        if dans_chaine {
            match c {
                _ if echappe => echappe = false,
                b'\\' => echappe = true,
                b'"' => dans_chaine = false,
                _ => {}
            }
            continue;
        }
        match c {
            b'"' => dans_chaine = true,
            b'{' => profondeur += 1,
            b'}' => {
                profondeur -= 1;
                if profondeur == 0 {
                    // Délimiteurs ASCII : `debut` et `i` sont des frontières de caractère.
                    return Some(&sans_reflexion[debut..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Texte de l'utilisateur borné, pour ne pas envoyer un document entier par erreur.
pub fn borner(texte: &str) -> Result<String, ErreurIa> {
    let texte = texte.trim();
    if texte.is_empty() {
        return Err(ErreurIa::Demande("décrivez d'abord la demande".to_owned()));
    }
    if texte.chars().count() > TEXTE_MAX {
        return Err(ErreurIa::Demande(format!(
            "texte trop long (plus de {TEXTE_MAX} caractères) : gardez l'essentiel"
        )));
    }
    Ok(texte.to_owned())
}

#[cfg(test)]
pub(crate) mod tests {
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;

    use super::*;

    /// Réponse `chat/completions` au format OpenAI.
    pub(crate) fn reponse_ia(contenu: &str) -> Value {
        json!({"id": "chatcmpl-1", "choices": [{"index": 0, "message": {"role": "assistant", "content": contenu}}]})
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Exemple {
        client: String,
    }

    #[test]
    fn extraction_du_json() {
        for (texte, attendu) in [
            (r#"{"client": "Mairie"}"#, Some(r#"{"client": "Mairie"}"#)),
            ("<think>Je réfléchis {pas ça}</think>\n```json\n{\"a\": {\"b\": 1}}\n```", Some(r#"{"a": {"b": 1}}"#)),
            (
                r#"Voici : {"t": "accolade } dans \" une chaîne"} fin"#,
                Some(r#"{"t": "accolade } dans \" une chaîne"}"#),
            ),
            ("aucun json", None),
            ("{ jamais fermé", None),
        ] {
            assert_eq!(extraire_json(texte), attendu, "{texte}");
        }
    }

    #[test]
    fn requete_et_bases() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &reponse_ia(r#"{"client": "Mairie de Crest"}"#));
        let ia = OpenWebUi::new(&faux, "");
        let reponse: Reponse<Exemple> =
            ia.demander_json("système", "demande", &["kb-ventes".to_owned()]).expect("réponse");
        assert_eq!(reponse.donnees, Exemple { client: "Mairie de Crest".into() });
        assert!(reponse.avertissements.is_empty());

        let requete = &faux.requetes()[0];
        assert_eq!(requete.methode, Methode::Post);
        assert_eq!(requete.chemin, "/api/chat/completions");
        let corps = requete.corps.as_ref().expect("corps");
        assert_eq!(corps["model"], MODELE_PAR_DEFAUT);
        assert_eq!(corps["stream"], false);
        assert_eq!(corps["files"], json!([{"type": "collection", "id": "kb-ventes"}]));
        assert_eq!(corps["messages"][0]["role"], "system");
    }

    #[test]
    fn base_inconnue_ignoree_avec_avertissement() {
        let faux = FauxTransport::new();
        faux.repondre(400, r#"{"detail": "Collection not found"}"#);
        faux.repondre_json(200, &reponse_ia(r#"{"client": "X"}"#));
        let ia = OpenWebUi::new(&faux, "modele");
        let reponse: Reponse<Exemple> = ia.demander_json("s", "u", &["inconnue".to_owned()]).expect("réponse");
        assert_eq!(reponse.avertissements.len(), 1);
        assert!(faux.requetes()[1].corps.as_ref().is_some_and(|c| c.get("files").is_none()));
    }

    #[test]
    fn relance_unique_si_illisible() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &reponse_ia("Bien sûr ! Le client est la mairie."));
        faux.repondre_json(200, &reponse_ia(r#"{"client": "Mairie"}"#));
        let ia = OpenWebUi::new(&faux, "m");
        let reponse: Reponse<Exemple> = ia.demander_json("s", "u", &[]).expect("réponse");
        assert_eq!(reponse.donnees.client, "Mairie");
        let requetes = faux.requetes();
        let messages = &requetes[1].corps.as_ref().expect("corps")["messages"];
        assert_eq!(messages.as_array().map(Vec::len), Some(4));

        let faux = FauxTransport::new();
        faux.repondre_json(200, &reponse_ia("non"));
        faux.repondre_json(200, &reponse_ia("toujours pas"));
        let erreur = OpenWebUi::new(&faux, "m").demander_json::<Exemple>("s", "u", &[]).expect_err("illisible");
        assert!(matches!(erreur, ErreurIa::ReponseIllisible(_)));
        assert_eq!(faux.requetes().len(), 2);
    }

    #[test]
    fn erreurs_de_service() {
        let faux = FauxTransport::new();
        faux.repondre(401, "{}");
        faux.echouer(DokosError::Reseau("connexion refusée".into()));
        let ia = OpenWebUi::new(&faux, "m");
        assert_eq!(ia.demander_json::<Exemple>("s", "u", &[]).expect_err("clé"), ErreurIa::CleRefusee);
        assert!(matches!(ia.modeles(), Err(ErreurIa::Injoignable(_))));
    }

    #[test]
    fn liste_des_modeles() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"id": "qwen3.6-35b-a3b", "name": "Qwen"}]}));
        assert_eq!(OpenWebUi::new(&faux, "m").modeles().expect("modèles"), ["qwen3.6-35b-a3b"]);
        assert_eq!(faux.requetes()[0].chemin, "/api/models");
    }

    #[test]
    fn textes_bornes() {
        assert!(borner("  ").is_err());
        assert!(borner(&"x".repeat(TEXTE_MAX + 1)).is_err());
        assert_eq!(borner(" 25 repas ").as_deref(), Ok("25 repas"));
    }
}
