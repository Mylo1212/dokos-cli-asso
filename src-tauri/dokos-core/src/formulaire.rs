//! Formulaires web d'inscription aux événements (appli bookings de Dokos).
//!
//! Un formulaire web enregistre ses réponses dans `Event Registration` : il ne
//! peut poser que des questions qui existent comme champs de ce DocType
//! (standard ou personnalisés). L'outil ne crée jamais de champ personnalisé.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Dokos, Filtre, Liste};
use crate::erreur::DokosError;
use crate::meta::{Champ, Meta};
use crate::transport::Transport;
use crate::types::texte_ou_vide;

pub const DOCTYPE_FORMULAIRE: &str = "Web Form";
pub const DOCTYPE_INSCRIPTION: &str = "Event Registration";

/// Champs toujours présents, dans cet ordre, avec leur caractère obligatoire.
pub const CHAMPS_DE_BASE: &[(&str, bool)] =
    &[("event", true), ("first_name", true), ("last_name", true), ("email", true), ("mobile_number", false)];
/// Champs gérés par Dokos (paiement, rattachements, suivi) : jamais proposés.
const CHAMPS_RESERVES: &[&str] = &[
    "user",
    "contact",
    "customer",
    "amount",
    "company",
    "payment_gateway",
    "payment_status",
    "reference_no",
    "attended",
    "amended_from",
    "naming_series",
];
/// Types de champ qu'un visiteur anonyme peut remplir sans risque.
const TYPES_QUESTION: &[&str] =
    &["Data", "Small Text", "Text", "Long Text", "Int", "Float", "Check", "Select", "Date", "Phone"];
pub const QUESTIONS_MAX: usize = 20;
/// Essais de route (`-2`, `-3`…) si la route voulue est prise par un autre formulaire.
const ESSAIS_ROUTE: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurFormulaire {
    MauvaisDocType(String),
    /// L'appli bookings n'est sans doute pas installée.
    ChampDeBaseAbsent(String),
    QuestionInconnue(String),
    QuestionNonAutorisee(String),
    QuestionEnDouble(String),
    TropDeQuestions(usize),
}

impl fmt::Display for ErreurFormulaire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MauvaisDocType(d) => write!(f, "structure reçue pour {d} au lieu de {DOCTYPE_INSCRIPTION}"),
            Self::ChampDeBaseAbsent(c) => {
                write!(f, "champ « {c} » absent d'{DOCTYPE_INSCRIPTION} : l'appli bookings est-elle installée ?")
            }
            Self::QuestionInconnue(c) => write!(
                f,
                "« {c} » n'existe pas dans {DOCTYPE_INSCRIPTION} : créez d'abord le champ dans Dokos (Personnaliser le formulaire)"
            ),
            Self::QuestionNonAutorisee(c) => write!(f, "« {c} » ne peut pas être demandé dans un formulaire public"),
            Self::QuestionEnDouble(c) => write!(f, "question « {c} » en double"),
            Self::TropDeQuestions(n) => write!(f, "trop de questions : {n} (maximum {QUESTIONS_MAX})"),
        }
    }
}

impl std::error::Error for ErreurFormulaire {}

/// Questions que l'on peut ajouter à un formulaire d'inscription.
pub fn questions_possibles(meta: &Meta) -> Vec<Champ> {
    meta.champs.iter().filter(|c| est_question_possible(c)).cloned().collect()
}

fn est_question_possible(champ: &Champ) -> bool {
    champ.saisissable()
        && TYPES_QUESTION.contains(&champ.fieldtype.as_str())
        && !CHAMPS_RESERVES.contains(&champ.fieldname.as_str())
        && !CHAMPS_DE_BASE.iter().any(|(nom, _)| *nom == champ.fieldname)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// Nom technique du champ dans `Event Registration`.
    pub champ: String,
    pub obligatoire: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChampFormulaire {
    pub fieldname: String,
    pub fieldtype: String,
    pub label: String,
    pub options: String,
    pub obligatoire: bool,
    pub lecture_seule: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NouveauFormulaire {
    titre: String,
    route: String,
    champs: Vec<ChampFormulaire>,
}

impl NouveauFormulaire {
    /// `meta` est la structure d'`Event Registration` lue dans Dokos.
    pub fn new(meta: &Meta, questions: &[Question]) -> Result<Self, ErreurFormulaire> {
        if meta.doctype != DOCTYPE_INSCRIPTION {
            return Err(ErreurFormulaire::MauvaisDocType(meta.doctype.clone()));
        }
        if questions.len() > QUESTIONS_MAX {
            return Err(ErreurFormulaire::TropDeQuestions(questions.len()));
        }
        let vers_champ = |champ: &Champ, obligatoire: bool| ChampFormulaire {
            fieldname: champ.fieldname.clone(),
            fieldtype: champ.fieldtype.clone(),
            label: champ.libelle().to_owned(),
            options: champ.options.clone(),
            obligatoire,
            // L'événement est rempli par le lien « S'inscrire » (`?event=…`).
            lecture_seule: champ.fieldname == "event",
        };

        let mut champs = Vec::with_capacity(CHAMPS_DE_BASE.len() + questions.len());
        for (nom, obligatoire) in CHAMPS_DE_BASE {
            let champ = meta.champ(nom).ok_or_else(|| ErreurFormulaire::ChampDeBaseAbsent((*nom).to_owned()))?;
            champs.push(vers_champ(champ, *obligatoire));
        }
        let mut vus = BTreeSet::new();
        for question in questions {
            let nom = question.champ.trim();
            if !vus.insert(nom.to_owned()) {
                return Err(ErreurFormulaire::QuestionEnDouble(nom.to_owned()));
            }
            let champ = meta.champ(nom).ok_or_else(|| ErreurFormulaire::QuestionInconnue(nom.to_owned()))?;
            if !est_question_possible(champ) {
                return Err(ErreurFormulaire::QuestionNonAutorisee(nom.to_owned()));
            }
            champs.push(vers_champ(champ, question.obligatoire));
        }

        let questions_posees = &champs[CHAMPS_DE_BASE.len()..];
        let titre = if questions_posees.is_empty() {
            "Inscription".to_owned()
        } else {
            let libelles: Vec<&str> = questions_posees.iter().map(|c| c.label.as_str()).collect();
            let titre = format!("Inscription — {}", libelles.join(", "));
            titre.chars().take(140).collect()
        };
        let route = route_pour(questions_posees);
        Ok(Self { titre, route, champs })
    }

    pub fn titre(&self) -> &str {
        &self.titre
    }

    /// Route publique voulue, sans `/` initial.
    pub fn route(&self) -> &str {
        &self.route
    }

    pub fn champs(&self) -> &[ChampFormulaire] {
        &self.champs
    }

    fn signature(&self) -> BTreeSet<(String, bool)> {
        self.champs.iter().map(|c| (c.fieldname.clone(), c.obligatoire)).collect()
    }

    /// Corps JSON envoyé à Dokos. Le formulaire est créé **non publié**.
    pub fn corps(&self, route: &str) -> Value {
        let champs: Vec<Value> = self
            .champs
            .iter()
            .map(|c| {
                json!({
                    "fieldname": c.fieldname,
                    "fieldtype": c.fieldtype,
                    "label": c.label,
                    "options": c.options,
                    "reqd": u8::from(c.obligatoire),
                    "read_only": u8::from(c.lecture_seule),
                })
            })
            .collect();
        json!({
            "title": self.titre,
            "route": route,
            "doc_type": DOCTYPE_INSCRIPTION,
            "is_standard": 0,
            "published": 0,
            "login_required": 0,
            "anonymous": 0,
            "allow_multiple": 0,
            "allow_edit": 0,
            "allow_incomplete": 0,
            "button_label": "S'inscrire",
            "success_title": "Inscription enregistrée",
            "success_message": "Merci ! Votre inscription est bien enregistrée.",
            "web_form_fields": champs,
        })
    }
}

/// `inscription-age-allergies`, ou `inscription-<empreinte>` si c'est trop long.
fn route_pour(questions: &[ChampFormulaire]) -> String {
    if questions.is_empty() {
        return "inscription".to_owned();
    }
    let mut noms: Vec<&str> = questions.iter().map(|c| c.fieldname.as_str()).collect();
    noms.sort_unstable();
    let lisible = format!("inscription-{}", noms.join("-")).replace('_', "-").to_lowercase();
    let propre = lisible.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if propre && lisible.len() <= 80 {
        return lisible;
    }
    // FNV-1a : stable d'une version à l'autre, contrairement au hachage de la bibliothèque standard.
    let empreinte = noms
        .join(",")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, o| (h ^ u64::from(o)).wrapping_mul(0x0100_0000_01b3));
    format!("inscription-{:08x}", empreinte & 0xffff_ffff)
}

/// Formulaire utilisé pour un événement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaireObtenu {
    /// Identifiant Dokos du Web Form.
    pub nom: String,
    pub route: String,
    /// Vrai si un formulaire identique existait déjà.
    pub reutilise: bool,
}

#[derive(Debug, Deserialize)]
struct FormulaireExistant {
    name: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    route: String,
    #[serde(default)]
    published: u8,
    #[serde(default)]
    web_form_fields: Vec<ChampExistant>,
}

#[derive(Debug, Deserialize)]
struct ChampExistant {
    #[serde(default, deserialize_with = "texte_ou_vide")]
    fieldname: String,
    #[serde(default)]
    reqd: u8,
}

impl FormulaireExistant {
    fn signature(&self) -> BTreeSet<(String, bool)> {
        self.web_form_fields
            .iter()
            .filter(|c| !c.fieldname.is_empty())
            .map(|c| (c.fieldname.clone(), c.reqd == 1))
            .collect()
    }
}

fn lire_par_route<T: Transport>(dokos: &Dokos<T>, route: &str) -> Result<Option<FormulaireExistant>, DokosError> {
    #[derive(Deserialize)]
    struct Nom {
        name: String,
    }
    let liste = Liste::new(&["name"]).filtre(Filtre::egal("route", route)).limite(1);
    let noms: Vec<Nom> = dokos.lister(DOCTYPE_FORMULAIRE, &liste)?;
    match noms.first() {
        Some(nom) => Ok(Some(dokos.lire(DOCTYPE_FORMULAIRE, &nom.name)?)),
        None => Ok(None),
    }
}

/// Réutilise le formulaire identique s'il existe (même route, mêmes champs),
/// sinon en crée un, non publié, sur la première route libre.
pub fn obtenir_formulaire<T: Transport>(
    dokos: &Dokos<T>,
    formulaire: &NouveauFormulaire,
) -> Result<FormulaireObtenu, DokosError> {
    let signature = formulaire.signature();
    for essai in 1..=ESSAIS_ROUTE {
        let route = if essai == 1 { formulaire.route.clone() } else { format!("{}-{essai}", formulaire.route) };
        match lire_par_route(dokos, &route)? {
            Some(existant) if existant.signature() == signature => {
                return Ok(FormulaireObtenu { nom: existant.name, route: existant.route, reutilise: true });
            }
            Some(_) => continue,
            None => {
                let cree: FormulaireExistant = dokos.creer(DOCTYPE_FORMULAIRE, formulaire.corps(&route))?;
                let route = if cree.route.is_empty() { route } else { cree.route };
                return Ok(FormulaireObtenu { nom: cree.name, route, reutilise: false });
            }
        }
    }
    Err(DokosError::Validation(format!(
        "les routes {}…-{ESSAIS_ROUTE} sont toutes prises par d'autres formulaires",
        formulaire.route
    )))
}

/// Publie le formulaire s'il ne l'est pas. Renvoie sa route.
pub(crate) fn publier_formulaire<T: Transport>(dokos: &Dokos<T>, nom: &str) -> Result<String, DokosError> {
    let existant: FormulaireExistant = dokos.lire(DOCTYPE_FORMULAIRE, nom)?;
    if existant.published == 1 {
        return Ok(existant.route);
    }
    let publie: FormulaireExistant = dokos.modifier(DOCTYPE_FORMULAIRE, nom, json!({"published": 1}))?;
    Ok(if publie.route.is_empty() { existant.route } else { publie.route })
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    pub(crate) fn meta_inscription() -> Meta {
        let champ = |fieldname: &str, fieldtype: &str, label: &str| Champ {
            fieldname: fieldname.into(),
            fieldtype: fieldtype.into(),
            label: label.into(),
            options: String::new(),
            reqd: 0,
            hidden: 0,
            read_only: 0,
            permlevel: 0,
        };
        let mut payant = champ("payment_status", "Select", "Payment Status");
        payant.options = "\nUnpaid\nPaid".into();
        let mut cache = champ("interne", "Data", "Interne");
        cache.hidden = 1;
        Meta {
            doctype: DOCTYPE_INSCRIPTION.into(),
            champs: vec![
                champ("first_name", "Data", "First Name"),
                champ("last_name", "Data", "Last Name"),
                champ("email", "Data", "Email Address"),
                champ("mobile_number", "Data", "Mobile Number"),
                champ("event", "Link", "Event"),
                champ("qty", "Int", "Number of slots booked"),
                payant,
                champ("customer", "Link", "Customer"),
                champ("age", "Int", "Âge"),
                champ("allergies", "Small Text", "Allergies"),
                champ("justificatif", "Attach", "Justificatif"),
                cache,
            ],
        }
    }

    fn question(champ: &str, obligatoire: bool) -> Question {
        Question { champ: champ.into(), obligatoire }
    }

    #[test]
    fn questions_proposees() {
        let noms: Vec<String> = questions_possibles(&meta_inscription()).into_iter().map(|c| c.fieldname).collect();
        assert_eq!(noms, ["qty", "age", "allergies"]);
    }

    #[test]
    fn formulaire_valide() {
        let formulaire =
            NouveauFormulaire::new(&meta_inscription(), &[question("age", true), question("allergies", false)])
                .expect("formulaire");
        assert_eq!(formulaire.titre(), "Inscription — Âge, Allergies");
        assert_eq!(formulaire.route(), "inscription-age-allergies");
        let noms: Vec<&str> = formulaire.champs().iter().map(|c| c.fieldname.as_str()).collect();
        assert_eq!(noms, ["event", "first_name", "last_name", "email", "mobile_number", "age", "allergies"]);

        let corps = formulaire.corps("inscription-age-allergies");
        assert_eq!(corps["doc_type"], "Event Registration");
        assert_eq!(corps["published"], 0);
        assert_eq!(corps["is_standard"], 0);
        assert_eq!(
            corps["web_form_fields"][0],
            json!({"fieldname": "event", "fieldtype": "Link", "label": "Event",
            "options": "", "reqd": 1, "read_only": 1})
        );
        assert_eq!(corps["web_form_fields"][5]["reqd"], 1);
        assert_eq!(corps["web_form_fields"][6]["reqd"], 0);
    }

    #[test]
    fn formulaire_sans_question() {
        let formulaire = NouveauFormulaire::new(&meta_inscription(), &[]).expect("formulaire");
        assert_eq!((formulaire.titre(), formulaire.route()), ("Inscription", "inscription"));
    }

    #[test]
    fn questions_refusees() {
        let meta = meta_inscription();
        let erreur = |questions: &[Question]| NouveauFormulaire::new(&meta, questions).expect_err("refus");
        assert_eq!(erreur(&[question("regime", true)]), ErreurFormulaire::QuestionInconnue("regime".into()));
        for interdit in ["payment_status", "customer", "justificatif", "interne", "email"] {
            assert_eq!(
                erreur(&[question(interdit, false)]),
                ErreurFormulaire::QuestionNonAutorisee(interdit.into()),
                "{interdit}"
            );
        }
        assert_eq!(
            erreur(&[question("age", true), question("age", false)]),
            ErreurFormulaire::QuestionEnDouble("age".into())
        );
        let trop: Vec<Question> = (0..=QUESTIONS_MAX).map(|i| question(&format!("q{i}"), false)).collect();
        assert_eq!(erreur(&trop), ErreurFormulaire::TropDeQuestions(QUESTIONS_MAX + 1));
    }

    #[test]
    fn bookings_absent() {
        let mut meta = meta_inscription();
        meta.champs.retain(|c| c.fieldname != "event");
        assert_eq!(NouveauFormulaire::new(&meta, &[]), Err(ErreurFormulaire::ChampDeBaseAbsent("event".into())));
        meta.doctype = "Lead".into();
        assert_eq!(NouveauFormulaire::new(&meta, &[]), Err(ErreurFormulaire::MauvaisDocType("Lead".into())));
    }

    #[test]
    fn route_longue_raccourcie() {
        let mut meta = meta_inscription();
        let noms: Vec<String> = (0..12).map(|i| format!("question_supplementaire_{i}")).collect();
        for nom in &noms {
            meta.champs.push(Champ {
                fieldname: nom.clone(),
                fieldtype: "Data".into(),
                label: nom.clone(),
                options: String::new(),
                reqd: 0,
                hidden: 0,
                read_only: 0,
                permlevel: 0,
            });
        }
        let questions: Vec<Question> = noms.iter().map(|n| question(n, false)).collect();
        let formulaire = NouveauFormulaire::new(&meta, &questions).expect("formulaire");
        assert!(formulaire.route().starts_with("inscription-"));
        assert_eq!(formulaire.route().len(), "inscription-".len() + 8);
        let encore = NouveauFormulaire::new(&meta, &questions).expect("formulaire");
        assert_eq!(formulaire.route(), encore.route());
    }

    fn existant(route: &str, champs: &[(&str, u8)]) -> serde_json::Value {
        let champs: Vec<_> = champs.iter().map(|(f, r)| json!({"fieldname": f, "reqd": r})).collect();
        json!({"data": {"name": format!("form-{route}"), "route": route, "published": 0, "web_form_fields": champs}})
    }

    const BASE: [(&str, u8); 5] =
        [("event", 1), ("first_name", 1), ("last_name", 1), ("email", 1), ("mobile_number", 0)];

    #[test]
    fn formulaire_identique_reutilise() {
        let formulaire = NouveauFormulaire::new(&meta_inscription(), &[question("age", true)]).expect("formulaire");
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "form-inscription-age"}]}));
        let mut champs = BASE.to_vec();
        champs.push(("age", 1));
        faux.repondre_json(200, &existant("inscription-age", &champs));
        let obtenu = obtenir_formulaire(&Dokos::new(&faux), &formulaire).expect("formulaire");
        assert_eq!(
            obtenu,
            FormulaireObtenu { nom: "form-inscription-age".into(), route: "inscription-age".into(), reutilise: true }
        );
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));
        assert_eq!(faux.requetes()[0].parametre("filters"), Some(r#"[["route","=","inscription-age"]]"#));
    }

    #[test]
    fn route_prise_par_un_autre_formulaire() {
        let formulaire = NouveauFormulaire::new(&meta_inscription(), &[question("age", true)]).expect("formulaire");
        let faux = FauxTransport::new();
        // « inscription-age » existe mais avec l'âge facultatif : on passe à « -2 », libre.
        faux.repondre_json(200, &json!({"data": [{"name": "form-inscription-age"}]}));
        let mut champs = BASE.to_vec();
        champs.push(("age", 0));
        faux.repondre_json(200, &existant("inscription-age", &champs));
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(200, &json!({"data": {"name": "Inscription — Âge", "route": "inscription-age-2"}}));
        let obtenu = obtenir_formulaire(&Dokos::new(&faux), &formulaire).expect("formulaire");
        assert_eq!(obtenu.route, "inscription-age-2");
        assert!(!obtenu.reutilise);
        let requetes = faux.requetes();
        assert_eq!(requetes[2].parametre("filters"), Some(r#"[["route","=","inscription-age-2"]]"#));
        assert_eq!(requetes[3].methode, Methode::Post);
        assert_eq!(requetes[3].chemin, "/api/resource/Web%20Form");
        assert_eq!(requetes[3].corps.as_ref().map(|c| c["route"].clone()), Some(json!("inscription-age-2")));
    }

    #[test]
    fn publication_du_formulaire() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &existant("inscription-age", &BASE));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "form-inscription-age", "route": "inscription-age", "published": 1}}),
        );
        let route = publier_formulaire(&Dokos::new(&faux), "form-inscription-age").expect("publication");
        assert_eq!(route, "inscription-age");
        let requetes = faux.requetes();
        assert_eq!(requetes[1].methode, Methode::Put);
        assert_eq!(requetes[1].corps, Some(json!({"published": 1})));
    }
}
