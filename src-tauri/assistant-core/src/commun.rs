//! Éléments communs aux extractions : consignes, résolution dans Dokos, dates.

use dokos_core::DokosError;
use dokos_core::date::{Date, DateHeure};
use dokos_core::recherche::Correspondance;

/// Nombre maximal de questions renvoyées à l'utilisateur.
const QUESTIONS_MAX: usize = 8;
const QUESTION_LONGUEUR_MAX: usize = 200;

/// Consignes communes à toutes les demandes.
pub fn consignes(aujourdhui: Date, specifique: &str, schema: &str) -> String {
    format!(
        "Tu aides à saisir des documents dans Dokos, un logiciel de gestion. Tu ne fais qu'extraire \
les informations présentes dans la demande de l'utilisateur.\n\
Règles :\n\
- Réponds uniquement par un objet JSON conforme au schéma ci-dessous, sans aucun texte autour.\n\
- N'invente rien. Si une information manque ou est ambiguë, mets null et pose une question courte dans \"questions\".\n\
- N'indique jamais de prix, de remise ni de taxe : Dokos les calcule.\n\
- Les bases de connaissance jointes servent à comprendre les prestations et les règles, pas à compléter la demande.\n\
- Dates au format AAAA-MM-JJ, heures au format HH:MM. Aujourd'hui, nous sommes le {} {aujourdhui}.\n\
{specifique}\n\
Schéma :\n{schema}",
        aujourdhui.jour_semaine()
    )
}

/// Résultat de la recherche d'un élément nommé par l'IA.
#[derive(Debug, Clone, PartialEq)]
pub enum Choix<C> {
    /// La demande n'en parle pas.
    NonPrecise,
    Trouve(C),
    /// Plusieurs candidats : l'utilisateur choisit.
    APreciser {
        texte: String,
        candidats: Vec<C>,
    },
    /// Rien dans Dokos : jamais créé en silence.
    Introuvable(String),
}

impl<C> Choix<C> {
    pub fn trouve(&self) -> Option<&C> {
        match self {
            Self::Trouve(c) => Some(c),
            _ => None,
        }
    }
}

/// Cherche dans Dokos le texte donné par l'IA (lecture seule).
pub fn choisir<C>(
    texte: Option<&str>,
    chercher: impl FnOnce(&str) -> Result<Correspondance<C>, DokosError>,
) -> Result<Choix<C>, DokosError> {
    let Some(texte) = texte.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(Choix::NonPrecise);
    };
    Ok(match chercher(texte)? {
        Correspondance::Unique(c) => Choix::Trouve(c),
        Correspondance::Plusieurs(candidats) => Choix::APreciser { texte: texte.to_owned(), candidats },
        Correspondance::Aucune => Choix::Introuvable(texte.to_owned()),
    })
}

/// Questions courtes et dédoublonnées.
pub fn nettoyer_questions(questions: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut propres: Vec<String> = Vec::new();
    for question in questions {
        let question: String = question.split_whitespace().collect::<Vec<_>>().join(" ");
        if question.is_empty() || propres.contains(&question) {
            continue;
        }
        propres.push(question.chars().take(QUESTION_LONGUEUR_MAX).collect());
        if propres.len() == QUESTIONS_MAX {
            break;
        }
    }
    propres
}

pub fn texte(valeur: Option<String>) -> Option<String> {
    valeur.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty() && v != "null")
}

/// Date donnée par l'IA ; une date illisible devient une question.
pub fn date(valeur: Option<String>, libelle: &str, questions: &mut Vec<String>) -> Option<Date> {
    let valeur = texte(valeur)?;
    match valeur.parse() {
        Ok(date) => Some(date),
        Err(_) => {
            questions.push(format!("{libelle} « {valeur} » illisible : quelle date ?"));
            None
        }
    }
}

pub fn date_heure(valeur: Option<String>, libelle: &str, questions: &mut Vec<String>) -> Option<DateHeure> {
    let valeur = texte(valeur)?;
    match valeur.parse() {
        Ok(d) => Some(d),
        Err(_) => {
            questions.push(format!("{libelle} « {valeur} » illisible : quelle date et quelle heure ?"));
            None
        }
    }
}

/// Valeur exacte d'une liste de Dokos, sans tenir compte de la casse.
pub fn dans_liste(valeur: Option<&str>, liste: &[String]) -> Option<String> {
    let valeur = valeur?.trim();
    liste.iter().find(|v| v.trim().eq_ignore_ascii_case(valeur)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consignes_datees() {
        let texte = consignes("2026-11-03".parse().expect("date"), "Spécifique.", "{}");
        assert!(texte.contains("nous sommes le mardi 2026-11-03"));
        assert!(texte.contains("Spécifique."));
    }

    #[test]
    fn questions() {
        let longue = "x".repeat(500);
        let propres = nettoyer_questions(["  Quel  jour ? ".into(), "Quel jour ?".into(), String::new(), longue]);
        assert_eq!(propres.len(), 2);
        assert_eq!(propres[0], "Quel jour ?");
        assert_eq!(propres[1].chars().count(), QUESTION_LONGUEUR_MAX);
        assert_eq!(nettoyer_questions((0..20).map(|i| format!("q{i}"))).len(), QUESTIONS_MAX);
    }

    #[test]
    fn dates() {
        let mut questions = Vec::new();
        assert_eq!(
            date(Some("2026-11-12".into()), "Date", &mut questions).map(|d| d.to_string()).as_deref(),
            Some("2026-11-12")
        );
        assert_eq!(date(Some("null".into()), "Date", &mut questions), None);
        assert_eq!(date(Some("le 12".into()), "Date", &mut questions), None);
        assert_eq!(questions, ["Date « le 12 » illisible : quelle date ?"]);
        assert!(date_heure(Some("2026-12-14 14:00".into()), "Début", &mut questions).is_some());
    }

    #[test]
    fn listes() {
        let types = vec!["CDD".to_owned(), "CDI".to_owned()];
        assert_eq!(dans_liste(Some(" cdd "), &types).as_deref(), Some("CDD"));
        assert_eq!(dans_liste(Some("intérim"), &types), None);
    }
}
