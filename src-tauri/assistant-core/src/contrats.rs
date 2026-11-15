//! « Embauche de Marie Dupont en CDD du 1/12 au 31/3, 24 h, animatrice » →
//! employé et contrat pré-remplis. La vérification de conformité reste celle
//! du crate `conformite`, faite avant toute création.

use std::collections::BTreeMap;

use dokos_core::contrat::{DOCTYPE_CONTRAT, champs_libres, lister_types_contrat};
use dokos_core::date::Date;
use dokos_core::employe::{EmployeExistant, lister_genres, lister_postes, trouver_employe};
use dokos_core::meta::lire_meta;
use dokos_core::{Dokos, Transport};
use serde::Deserialize;
use serde_json::Value;

use crate::commun::{Choix, choisir, consignes, dans_liste, date, nettoyer_questions, texte};
use crate::owui::{ErreurIa, OpenWebUi, borner};

/// Postes envoyés à l'IA au plus (le reste est vérifié par Rust).
const POSTES_MAX: usize = 150;

const SCHEMA: &str = r#"{
  "prenom": "prénom du salarié" | null,
  "nom": "nom de famille du salarié" | null,
  "naissance": "AAAA-MM-JJ" | null,
  "genre": "valeur exacte de la liste des genres" | null,
  "type_contrat": "valeur exacte de la liste des types" | null,
  "poste": "intitulé du poste" | null,
  "entree": "AAAA-MM-JJ" | null,
  "fin": "AAAA-MM-JJ" | null,
  "heures_hebdo": nombre | null,
  "repartition": [lundi, mardi, mercredi, jeudi, vendredi, samedi, dimanche] | null,
  "champs": {"nom_technique": "valeur"},
  "questions": ["question courte"]
}"#;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Extraction {
    prenom: Option<String>,
    nom: Option<String>,
    naissance: Option<String>,
    genre: Option<String>,
    type_contrat: Option<String>,
    poste: Option<String>,
    entree: Option<String>,
    fin: Option<String>,
    heures_hebdo: Option<f64>,
    repartition: Option<Vec<f64>>,
    champs: BTreeMap<String, Value>,
    questions: Vec<String>,
}

/// Employé à créer, si aucun employé actif ne correspond.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmployePropose {
    pub prenom: String,
    pub nom: String,
    pub naissance: Option<Date>,
    pub genre: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropositionContrat {
    pub employe: Choix<EmployeExistant>,
    pub nouvel_employe: Option<EmployePropose>,
    /// Type de contrat de Dokos (valeur exacte de la liste).
    pub type_contrat: Option<String>,
    pub poste: Option<String>,
    /// Faux si le poste n'existe pas encore dans Dokos.
    pub poste_connu: bool,
    pub entree: Option<Date>,
    pub fin: Option<Date>,
    pub heures: Option<f64>,
    pub repartition: Option<[f64; 7]>,
    /// Champs complémentaires existants dans Dokos uniquement.
    pub champs: BTreeMap<String, String>,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

pub fn proposer_contrat<D: Transport, I: Transport>(
    dokos: &Dokos<D>,
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    aujourdhui: Date,
) -> Result<PropositionContrat, ErreurIa> {
    let demande = borner(demande)?;
    let types = lister_types_contrat(dokos)?;
    let postes = lister_postes(dokos)?;
    let genres = lister_genres(dokos)?;
    let meta = lire_meta(dokos, DOCTYPE_CONTRAT)?;
    let libres = champs_libres(&meta);

    let liste_champs = if libres.is_empty() {
        "(aucun : laisse \"champs\" vide)".to_owned()
    } else {
        libres
            .iter()
            .map(|c| format!("- {} : {} ({})", c.fieldname, c.libelle(), c.fieldtype))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let specifique = format!(
        "Il s'agit de l'embauche d'un salarié et de son contrat de travail.\n\
Types de contrat possibles (valeur exacte) : {}.\n\
Genres possibles (valeur exacte) : {}.\n\
Postes existants : {}.\n\
\"champs\" : uniquement ces champs complémentaires (nom technique avant les deux-points) :\n{liste_champs}\n\
\"repartition\" : heures par jour du lundi au dimanche, seulement si elles sont données.",
        types.join(", "),
        genres.join(", "),
        postes.iter().take(POSTES_MAX).cloned().collect::<Vec<_>>().join(", "),
    );
    let reponse = ia.demander_json::<Extraction>(&consignes(aujourdhui, &specifique, SCHEMA), &demande, bases)?;
    let x = reponse.donnees;
    let mut questions = Vec::new();
    let mut avertissements = reponse.avertissements;

    let prenom = texte(x.prenom);
    let nom = texte(x.nom);
    let nom_complet = match (&prenom, &nom) {
        (Some(p), Some(n)) => Some(format!("{p} {n}")),
        (Some(t), None) | (None, Some(t)) => Some(t.clone()),
        (None, None) => None,
    };
    let employe = choisir(nom_complet.as_deref(), |t| trouver_employe(dokos, t))?;
    let naissance = date(x.naissance, "Date de naissance", &mut questions);
    let genre = dans_liste(texte(x.genre).as_deref(), &genres);
    let nouvel_employe = match (&employe, prenom, nom) {
        (Choix::Introuvable(_), Some(prenom), Some(nom)) => {
            if naissance.is_none() {
                questions.push(format!("Date de naissance de {prenom} {nom} ?"));
            }
            if genre.is_none() {
                questions.push(format!("Genre de {prenom} {nom} (obligatoire dans Dokos) ?"));
            }
            Some(EmployePropose { prenom, nom, naissance, genre })
        }
        _ => None,
    };
    match &employe {
        Choix::NonPrecise => questions.push("Pour quel salarié ?".to_owned()),
        Choix::APreciser { texte, .. } => {
            questions.push(format!("Plusieurs employés correspondent à « {texte} » : lequel ?"))
        }
        Choix::Introuvable(t) if nouvel_employe.is_none() => {
            questions.push(format!("Aucun employé actif « {t} » : prénom et nom pour le créer ?"));
        }
        Choix::Introuvable(_) | Choix::Trouve(_) => {}
    }

    let type_demande = texte(x.type_contrat);
    let type_contrat = dans_liste(type_demande.as_deref(), &types);
    match (&type_demande, &type_contrat) {
        (None, _) => questions.push(format!("Quel type de contrat ({}) ?", types.join(", "))),
        (Some(t), None) => {
            questions.push(format!("Type « {t} » absent de Dokos : lequel parmi {} ?", types.join(", ")))
        }
        _ => {}
    }

    let poste_demande = texte(x.poste);
    let poste_existant = dans_liste(poste_demande.as_deref(), &postes);
    let poste_connu = poste_existant.is_some();
    if let Some(p) = poste_demande.as_ref().filter(|_| !poste_connu) {
        questions.push(format!("Poste « {p} » absent des postes de Dokos : à créer, ou lequel choisir ?"));
    }
    let poste = poste_existant.or(poste_demande);

    let entree = date(x.entree, "Date d'entrée", &mut questions);
    if entree.is_none() {
        questions.push("Date d'entrée ?".to_owned());
    }
    let fin = date(x.fin, "Date de fin", &mut questions).filter(|f| entree.is_none_or(|e| *f >= e));

    let heures = x.heures_hebdo.filter(|h| h.is_finite() && *h > 0.0 && *h <= 60.0);
    let repartition = match x.repartition {
        Some(jours) if jours.len() == 7 && jours.iter().all(|h| h.is_finite() && (0.0..=24.0).contains(h)) => {
            let jours: [f64; 7] = jours.try_into().unwrap_or([0.0; 7]);
            jours.iter().any(|h| *h > 0.0).then_some(jours)
        }
        Some(_) => {
            questions.push("Répartition des heures illisible : combien d'heures chaque jour ?".to_owned());
            None
        }
        None => None,
    };

    let mut champs = BTreeMap::new();
    for (nom, valeur) in x.champs {
        let valeur = match valeur {
            Value::String(s) => s.trim().to_owned(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => u8::from(b).to_string(),
            _ => continue,
        };
        if valeur.is_empty() || valeur == "null" {
            continue;
        }
        if libres.iter().any(|c| c.fieldname == nom) {
            champs.insert(nom, valeur);
        } else {
            avertissements.push(format!("champ « {nom} » proposé par l'IA ignoré : il n'existe pas dans Dokos"));
        }
    }

    questions.extend(x.questions);
    Ok(PropositionContrat {
        employe,
        nouvel_employe,
        type_contrat,
        poste,
        poste_connu,
        entree,
        fin,
        heures,
        repartition,
        champs,
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

    fn dokos_rh(employes: serde_json::Value) -> FauxTransport {
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": [{"name": "CDD"}, {"name": "CDI"}]}));
        dokos.repondre_json(200, &json!({"data": [{"name": "Animatrice"}, {"name": "Cuisinier"}]}));
        dokos.repondre_json(200, &json!({"data": [{"name": "Femme"}, {"name": "Homme"}]}));
        dokos.repondre_json(
            200,
            &json!({"docs": [{"name": DOCTYPE_CONTRAT, "fields": [
                {"fieldname": "employee", "fieldtype": "Link"},
                {"fieldname": "motif_cdd", "fieldtype": "Small Text", "label": "Motif du CDD"},
                {"fieldname": "remuneration_brute", "fieldtype": "Currency", "label": "Rémunération"},
            ]}]}),
        );
        dokos.repondre_json(200, &employes);
        dokos
    }

    fn jour() -> Date {
        "2026-11-03".parse().expect("date")
    }

    #[test]
    fn nouvelle_embauche() {
        let dokos = dokos_rh(json!({"data": []}));
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &reponse_ia(
                r#"{"prenom": "Marie", "nom": "Dupont", "naissance": "1995-04-02", "genre": "femme",
                "type_contrat": "cdd", "poste": "Animatrice", "entree": "2026-12-01", "fin": "2027-03-31",
                "heures_hebdo": 24, "repartition": [6, 6, 6, 6, 0, 0, 0],
                "champs": {"motif_cdd": "Accroissement temporaire d'activité", "remuneration_brute": 1500, "salaire": "2000"},
                "questions": []}"#,
            ),
        );
        let p =
            proposer_contrat(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "…", jour()).expect("proposition");
        assert_eq!(p.employe, Choix::Introuvable("Marie Dupont".into()));
        assert_eq!(
            p.nouvel_employe,
            Some(EmployePropose {
                prenom: "Marie".into(),
                nom: "Dupont".into(),
                naissance: "1995-04-02".parse().ok(),
                genre: Some("Femme".into())
            })
        );
        assert_eq!(p.type_contrat.as_deref(), Some("CDD"));
        assert!(p.poste_connu);
        assert_eq!(p.repartition, Some([6.0, 6.0, 6.0, 6.0, 0.0, 0.0, 0.0]));
        assert_eq!(p.champs.get("remuneration_brute").map(String::as_str), Some("1500"));
        assert!(!p.champs.contains_key("salaire"));
        assert_eq!(p.avertissements, ["champ « salaire » proposé par l'IA ignoré : il n'existe pas dans Dokos"]);
        assert!(p.questions.is_empty(), "{:?}", p.questions);
        assert!(dokos.requetes().iter().all(|r| r.methode == Methode::Get));
    }

    #[test]
    fn employe_existant_et_valeurs_inconnues() {
        let dokos = dokos_rh(json!({"data": [{"name": "HR-EMP-00012", "employee_name": "Marie Dupont"}]}));
        let ia = FauxTransport::new();
        ia.repondre_json(
            200,
            &reponse_ia(
                r#"{"prenom": "Marie", "nom": "Dupont", "type_contrat": "intérim", "poste": "Cheffe de projet",
                         "entree": "2026-12-01", "repartition": [6, 6]}"#,
            ),
        );
        let p =
            proposer_contrat(&Dokos::new(&dokos), &OpenWebUi::new(&ia, "m"), &[], "…", jour()).expect("proposition");
        assert_eq!(p.employe.trouve().map(|e| e.id.as_str()), Some("HR-EMP-00012"));
        assert_eq!(p.nouvel_employe, None);
        assert_eq!(p.type_contrat, None);
        assert_eq!(p.poste.as_deref(), Some("Cheffe de projet"));
        assert!(!p.poste_connu);
        assert_eq!(p.repartition, None);
        assert_eq!(
            p.questions,
            [
                "Type « intérim » absent de Dokos : lequel parmi CDD, CDI ?",
                "Poste « Cheffe de projet » absent des postes de Dokos : à créer, ou lequel choisir ?",
                "Répartition des heures illisible : combien d'heures chaque jour ?",
            ]
        );
    }
}
