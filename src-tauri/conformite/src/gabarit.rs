//! Analyse d'un gabarit d'impression Jinja (Frappe).
//!
//! - Champs affichés : `doc.x`, `doc.get("x")`, `doc.get_formatted("x")`, `doc["x"]`.
//! - Texte visible : HTML et balises Jinja retirés, normalisé.
//! - Sécurité : un gabarit est du code exécuté par Dokos, qui peut lire d'autres
//!   données (`frappe.db.get_value`, `frappe.get_list`…) ou modifier le document
//!   (`doc.db_set`). Seuls `doc.<champ>`, quelques fonctions de mise en forme
//!   et des filtres connus sont acceptés sans réserve.

use std::collections::BTreeSet;

use crate::normalisation::{compter_mots, normaliser, texte_visible};

/// Appels `frappe.…` de mise en forme, sans accès aux données.
const FRAPPE_AUTORISES: &[&str] = &[
    "frappe.format",
    "frappe.format_value",
    "frappe.utils.add_days",
    "frappe.utils.cint",
    "frappe.utils.date_diff",
    "frappe.utils.flt",
    "frappe.utils.fmt_money",
    "frappe.utils.format_date",
    "frappe.utils.format_datetime",
    "frappe.utils.format_time",
    "frappe.utils.formatdate",
    "frappe.utils.getdate",
    "frappe.utils.money_in_words",
    "frappe.utils.nowdate",
    "frappe.utils.today",
];
/// Méthodes du document autorisées : lecture d'un champ.
const METHODES_DOC: &[&str] = &["get", "get_formatted"];
/// Noms qui donnent accès à la base, à la session ou à des écritures.
const NOMS_INTERDITS: &[&str] = &[
    "attach_print",
    "call",
    "cancel",
    "conf",
    "db",
    "db_get",
    "db_set",
    "delete",
    "enqueue",
    "form_dict",
    "get_all",
    "get_cached_doc",
    "get_cached_value",
    "get_doc",
    "get_hooks",
    "get_list",
    "get_meta",
    "get_print",
    "get_single_value",
    "get_value",
    "insert",
    "local",
    "render_template",
    "request",
    "run_method",
    "run_script",
    "save",
    "sendmail",
    "session",
    "set_value",
    "sql",
    "submit",
];
/// Filtres Jinja sans effet de bord.
const FILTRES_SURS: &[&str] = &[
    "abs",
    "capitalize",
    "center",
    "d",
    "default",
    "e",
    "escape",
    "first",
    "float",
    "format",
    "global_date_format",
    "int",
    "join",
    "last",
    "length",
    "lower",
    "replace",
    "round",
    "safe",
    "string",
    "striptags",
    "title",
    "trim",
    "truncate",
    "upper",
    "wordcount",
];
/// Fonctions globales sans accès aux données.
const FONCTIONS_SURES: &[&str] = &["_", "range", "loop"];
/// Instructions qui chargent un autre gabarit, non vérifié.
const INSTRUCTIONS_INTERDITES: &[&str] = &["import", "include", "extends", "from"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analyse {
    /// Champs du document affichés ou testés par le gabarit.
    pub champs: BTreeSet<String>,
    /// Texte visible normalisé (voir [`normaliser`]).
    pub texte: String,
    /// Constructions refusées : accès aux données, écritures, gabarits externes.
    pub interdits: Vec<String>,
    /// Appels ou filtres inconnus : sans doute sûrs, mais non vérifiés.
    pub non_verifies: Vec<String>,
    /// Nombre de « [à compléter …] » restants.
    pub a_completer: usize,
}

pub fn analyser(gabarit: &str) -> Analyse {
    let mut analyse = Analyse::default();
    let mut texte = String::with_capacity(gabarit.len());
    let mut reste = gabarit;
    while !reste.is_empty() {
        let Some(debut) = reste.find('{') else {
            texte.push_str(reste);
            break;
        };
        texte.push_str(&reste[..debut]);
        let apres = &reste[debut..];
        if let Some(commentaire) = apres.strip_prefix("{#") {
            reste = commentaire.find("#}").map_or("", |fin| &commentaire[fin + 2..]);
            texte.push(' ');
        } else if let Some(expression) = apres.strip_prefix("{{") {
            let (interieur, suite) = jusqua_fermeture(expression, "}}");
            analyser_expression(interieur, false, &mut analyse);
            texte.push(' ');
            reste = suite;
        } else if let Some(instruction) = apres.strip_prefix("{%") {
            let (interieur, suite) = jusqua_fermeture(instruction, "%}");
            analyser_expression(interieur, true, &mut analyse);
            texte.push(' ');
            reste = suite;
        } else {
            texte.push('{');
            reste = &apres[1..];
        }
    }
    analyse.texte = normaliser(&texte_visible(&texte));
    analyse.a_completer = compter_mots(&analyse.texte, "a completer");
    analyse.interdits.dedup();
    analyse.non_verifies.sort();
    analyse.non_verifies.dedup();
    analyse
}

/// Contenu jusqu'au délimiteur de fermeture (ASCII), en sautant les chaînes.
fn jusqua_fermeture<'a>(texte: &'a str, fermeture: &str) -> (&'a str, &'a str) {
    let octets = texte.as_bytes();
    let mut i = 0;
    let mut guillemet: Option<u8> = None;
    while i < octets.len() {
        let c = octets[i];
        match guillemet {
            Some(_) if c == b'\\' => i += 1,
            Some(q) if c == q => guillemet = None,
            Some(_) => {}
            None if c == b'\'' || c == b'"' => guillemet = Some(c),
            // Délimiteur ASCII : `i` est forcément une frontière de caractère.
            None if octets[i..].starts_with(fermeture.as_bytes()) => {
                return (&texte[..i], &texte[i + fermeture.len()..]);
            }
            None => {}
        }
        i += 1;
    }
    (texte, "")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Jeton {
    Nom(String),
    Chaine(String),
    Point,
    Parenthese,
    Crochet,
    Barre,
    Autre,
}

fn decouper(expression: &str) -> Vec<Jeton> {
    let mut jetons = Vec::new();
    let caracteres: Vec<char> = expression.chars().collect();
    let mut i = 0;
    while i < caracteres.len() {
        let c = caracteres[i];
        if c.is_alphabetic() || c == '_' {
            let debut = i;
            while i < caracteres.len() && (caracteres[i].is_alphanumeric() || caracteres[i] == '_') {
                i += 1;
            }
            jetons.push(Jeton::Nom(caracteres[debut..i].iter().collect()));
            continue;
        }
        match c {
            '\'' | '"' => {
                let mut contenu = String::new();
                i += 1;
                while i < caracteres.len() && caracteres[i] != c {
                    if caracteres[i] == '\\' && i + 1 < caracteres.len() {
                        i += 1;
                    }
                    contenu.push(caracteres[i]);
                    i += 1;
                }
                jetons.push(Jeton::Chaine(contenu));
            }
            '.' => jetons.push(Jeton::Point),
            '(' => jetons.push(Jeton::Parenthese),
            '[' => jetons.push(Jeton::Crochet),
            '|' => jetons.push(Jeton::Barre),
            c if c.is_whitespace() => {}
            _ => jetons.push(Jeton::Autre),
        }
        i += 1;
    }
    jetons
}

fn analyser_expression(expression: &str, instruction: bool, analyse: &mut Analyse) {
    let jetons = decouper(expression);
    if instruction
        && let Some(Jeton::Nom(mot)) = jetons.first()
        && INSTRUCTIONS_INTERDITES.contains(&mot.as_str())
    {
        analyse.interdits.push(format!("instruction « {mot} » : charge un autre gabarit, non vérifié"));
    }

    let mut i = 0;
    while i < jetons.len() {
        let Jeton::Nom(premier) = &jetons[i] else {
            i += 1;
            continue;
        };
        // Chaîne pointée `a.b.c` ; un nom qui suit `).` démarre une nouvelle chaîne.
        let filtre = i > 0 && jetons[i - 1] == Jeton::Barre;
        let mut chaine = vec![premier.clone()];
        let mut j = i + 1;
        while j + 1 < jetons.len()
            && jetons[j] == Jeton::Point
            && let Jeton::Nom(suivant) = &jetons[j + 1]
        {
            chaine.push(suivant.clone());
            j += 2;
        }
        let appel = jetons.get(j) == Some(&Jeton::Parenthese);
        let argument = match (jetons.get(j), jetons.get(j + 1)) {
            (Some(Jeton::Parenthese | Jeton::Crochet), Some(Jeton::Chaine(s))) => Some(s.clone()),
            _ => None,
        };
        verifier_chaine(&chaine, filtre, appel, argument, analyse);
        i = j;
    }
}

fn verifier_chaine(chaine: &[String], filtre: bool, appel: bool, argument: Option<String>, analyse: &mut Analyse) {
    let complet = chaine.join(".");
    if chaine.iter().any(|partie| partie.starts_with("__")) {
        analyse.interdits.push(format!("« {complet} » : attribut interne"));
        return;
    }
    if filtre {
        if !FILTRES_SURS.contains(&complet.as_str()) {
            analyse.non_verifies.push(format!("filtre « {complet} »"));
        }
        return;
    }
    let racine = chaine[0].as_str();
    match racine {
        "doc" => match chaine.get(1).map(String::as_str) {
            None => {
                // `doc["x"]`
                if let Some(champ) = argument {
                    analyse.champs.insert(champ);
                }
            }
            Some(methode) if appel && chaine.len() == 2 => {
                if METHODES_DOC.contains(&methode) {
                    if let Some(champ) = argument {
                        analyse.champs.insert(champ);
                    }
                } else {
                    analyse
                        .interdits
                        .push(format!("« {complet}() » : seules doc.get() et doc.get_formatted() sont permises"));
                }
            }
            Some(champ) => {
                if NOMS_INTERDITS.contains(&champ) {
                    analyse.interdits.push(format!("« {complet} » : accès interdit"));
                } else {
                    analyse.champs.insert(champ.to_owned());
                }
            }
        },
        "frappe" => {
            if !FRAPPE_AUTORISES.contains(&complet.as_str()) {
                analyse.interdits.push(format!("« {complet} » : accès aux données ou au serveur interdit"));
            }
        }
        _ if chaine.iter().any(|partie| NOMS_INTERDITS.contains(&partie.as_str())) => {
            analyse.interdits.push(format!("« {complet} » : accès interdit"));
        }
        _ if appel && !FONCTIONS_SURES.contains(&racine) => {
            analyse.non_verifies.push(format!("appel « {complet}() »"));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn champs(gabarit: &str) -> Vec<String> {
        analyser(gabarit).champs.into_iter().collect()
    }

    #[test]
    fn champs_affiches() {
        let gabarit = r#"
            <p>{{ doc.employee_name }} — {{ doc.get_formatted("remuneration_brute") }}</p>
            {% if doc.relieving_date %}Fin : {{ doc.get('relieving_date') }}{% endif %}
            {{ doc["lieu_travail"] | e }} {{ frappe.utils.formatdate(doc.date_of_joining) }}
            {% for l in doc.leave_types %}{{ l.leave_type }}{% endfor %}
        "#;
        assert_eq!(
            champs(gabarit),
            ["date_of_joining", "employee_name", "leave_types", "lieu_travail", "relieving_date", "remuneration_brute"]
        );
        let analyse = analyser(gabarit);
        assert!(analyse.interdits.is_empty(), "{:?}", analyse.interdits);
        assert!(analyse.non_verifies.is_empty(), "{:?}", analyse.non_verifies);
    }

    #[test]
    fn chaines_et_commentaires_ignores() {
        let gabarit = r#"{# {{ doc.secret }} #}{{ _("doc.faux") }}{{ "}}" }}<p>Texte {{ doc.vrai }}</p>"#;
        assert_eq!(champs(gabarit), ["vrai"]);
        assert_eq!(analyser(gabarit).texte, "texte");
    }

    #[test]
    fn texte_visible_et_a_completer() {
        let analyse = analyser("<p>Période d'<b>essai</b> : {{ doc.periode_essai }}</p><p>[à compléter : IDCC]</p>");
        assert_eq!(analyse.texte, "periode d essai a completer idcc");
        assert_eq!(analyse.a_completer, 1);
    }

    #[test]
    fn acces_interdits() {
        for gabarit in [
            "{{ frappe.db.get_value('Employee', 'X', 'ctc') }}",
            "{{ frappe.get_list('Salary Slip') }}",
            "{{ frappe.session.user }}",
            "{% set x = frappe.get_doc('User', 'Administrator') %}",
            "{{ doc.db_set('status', 'Left') }}",
            "{{ doc.run_method('cancel') }}",
            "{{ doc.__class__ }}",
            "{{ ''.__class__.__mro__ }}",
            "{% include 'autre.html' %}",
            "{% import 'macros.html' as m %}",
            "{{ get_list('Employee') }}",
            "{{ x.db.sql('select 1') }}",
        ] {
            let analyse = analyser(gabarit);
            assert!(!analyse.interdits.is_empty(), "accepté à tort : {gabarit}");
        }
    }

    #[test]
    fn non_verifies() {
        let analyse = analyser("{{ doc.x | markdown }} {{ fonction_maison(doc.y) }} {{ range(3) }}");
        assert_eq!(analyse.non_verifies, ["appel « fonction_maison() »", "filtre « markdown »"]);
        assert!(analyse.interdits.is_empty());
    }

    #[test]
    fn gabarit_tronque_sans_panique() {
        for gabarit in
            ["{{ doc.a", "{% if doc.b", "{# jamais fermé", "{ seul", "{{ 'chaîne non fermée }}", "é{{doc.é}}"]
        {
            let _ = analyser(gabarit);
        }
        assert_eq!(champs("{{ doc.a"), ["a"]);
    }
}
