//! Vérifications à partir des données lues dans Dokos (lecture seule).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use dokos_core::DokosError;
use dokos_core::contrat::{DOCTYPE_CONTRAT, NouveauContrat, lire_valeurs_contrat};
use dokos_core::employe::DOCTYPE_EMPLOYE;
use dokos_core::impression::{FormatImpression, lire_format};
use dokos_core::meta::lire_meta;
use dokos_core::{Dokos, Transport};
use serde::Deserialize;

use crate::donnees::{Probleme, verifier_donnees};
use crate::gabarit::{Analyse, analyser};
use crate::proposition::{Proposition, proposer};
use crate::rapport::{RapportGabarit, verifier_gabarit};
use crate::regles::{Contexte, Regles};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurVerification {
    Dokos(DokosError),
    /// Format du constructeur visuel ou non HTML : pas de gabarit à analyser.
    FormatNonAnalysable(String),
    /// Le format n'imprime pas des contrats de travail.
    MauvaisDocType {
        format: String,
        doc_type: String,
    },
}

impl fmt::Display for ErreurVerification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dokos(e) => e.fmt(f),
            Self::FormatNonAnalysable(nom) => write!(
                f,
                "le format « {nom} » n'est pas un gabarit HTML/Jinja personnalisé : il ne peut pas être analysé"
            ),
            Self::MauvaisDocType { format, doc_type } => {
                write!(f, "le format « {format} » imprime des {doc_type}, pas des {DOCTYPE_CONTRAT}")
            }
        }
    }
}

impl std::error::Error for ErreurVerification {}

impl From<DokosError> for ErreurVerification {
    fn from(e: DokosError) -> Self {
        Self::Dokos(e)
    }
}

/// Champs d'`Employment Contract` dans Dokos, personnalisés compris.
pub fn champs_contrat<T: Transport>(dokos: &Dokos<T>) -> Result<BTreeSet<String>, DokosError> {
    let meta = lire_meta(dokos, DOCTYPE_CONTRAT)?;
    Ok(meta.champs.into_iter().filter(|c| c.a_une_valeur()).map(|c| c.fieldname).collect())
}

fn format_analysable<T: Transport>(dokos: &Dokos<T>, nom: &str) -> Result<FormatImpression, ErreurVerification> {
    let format = lire_format(dokos, nom)?;
    if format.doc_type != DOCTYPE_CONTRAT {
        return Err(ErreurVerification::MauvaisDocType { format: format.nom, doc_type: format.doc_type });
    }
    if !format.est_jinja() {
        return Err(ErreurVerification::FormatNonAnalysable(format.nom));
    }
    Ok(format)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationFormat {
    pub format: String,
    pub gabarit: String,
    pub rapport: RapportGabarit,
    /// Gabarit avec les mentions manquantes ajoutées, s'il en manque.
    pub proposition: Option<Proposition>,
    /// Rapport du gabarit proposé, une fois les champs manquants créés.
    pub rapport_propose: Option<RapportGabarit>,
}

/// Vérifie un format d'impression de contrat et propose les ajouts nécessaires.
/// Rien n'est écrit dans Dokos.
pub fn verifier_format<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    nom_format: &str,
    contexte: &Contexte,
) -> Result<VerificationFormat, ErreurVerification> {
    let format = format_analysable(dokos, nom_format)?;
    let champs = champs_contrat(dokos)?;
    let rapport = verifier_gabarit(regles, &analyser(&format.html), contexte, &champs);
    let proposition = proposer(&format.html, &rapport, regles, &champs);
    let rapport_propose = proposition.as_ref().map(|p| {
        let mut champs_apres = champs.clone();
        champs_apres.extend(p.champs_a_creer.iter().cloned());
        verifier_gabarit(regles, &p.analyse, contexte, &champs_apres)
    });
    Ok(VerificationFormat { format: format.nom, gabarit: format.html, rapport, proposition, rapport_propose })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationContrat {
    pub problemes: Vec<Probleme>,
    /// Conformité du format d'impression choisi, pour ce contrat.
    pub gabarit: Option<RapportGabarit>,
}

fn entree_employe<T: Transport>(dokos: &Dokos<T>, employe: &str) -> Result<Option<String>, DokosError> {
    #[derive(Deserialize)]
    struct Employe {
        #[serde(default)]
        date_of_joining: Option<String>,
    }
    if employe.trim().is_empty() {
        return Ok(None);
    }
    let employe: Employe = dokos.lire(DOCTYPE_EMPLOYE, employe)?;
    Ok(employe.date_of_joining)
}

fn verifier_valeurs<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    valeurs: &BTreeMap<String, String>,
    format: Option<&str>,
) -> Result<VerificationContrat, ErreurVerification> {
    let champs = champs_contrat(dokos)?;
    let employe = valeurs.get("employee").map(String::as_str).unwrap_or_default();
    let entree = entree_employe(dokos, employe)?;
    let heures = valeurs.get("weekly_working_hours").and_then(|h| h.replace(',', ".").parse().ok());
    let contexte =
        regles.contexte_contrat(valeurs.get("contract_type").map(String::as_str).unwrap_or_default(), heures);

    let analyse: Option<Analyse> = match format {
        Some(nom) => Some(analyser(&format_analysable(dokos, nom)?.html)),
        None => None,
    };
    let problemes = verifier_donnees(regles, valeurs, &champs, entree.as_deref(), analyse.as_ref().map(|a| &a.champs));
    let gabarit = analyse.map(|a| verifier_gabarit(regles, &a, &contexte, &champs));
    Ok(VerificationContrat { problemes, gabarit })
}

/// Vérifie un contrat avant sa création (rien n'est écrit).
pub fn verifier_nouveau_contrat<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    contrat: &NouveauContrat,
    format: Option<&str>,
) -> Result<VerificationContrat, ErreurVerification> {
    verifier_valeurs(dokos, regles, &contrat.valeurs(), format)
}

/// Vérifie un contrat déjà dans Dokos.
pub fn verifier_contrat_existant<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    id: &str,
    format: Option<&str>,
) -> Result<VerificationContrat, ErreurVerification> {
    let valeurs = lire_valeurs_contrat(dokos, id)?;
    verifier_valeurs(dokos, regles, &valeurs, format)
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::{Value, json};

    use super::*;
    use crate::donnees::a_des_erreurs;
    use crate::rapport::tests::CDI_COMPLET;

    fn meta_contrat(avec_motif: bool) -> Value {
        let mut champs: Vec<Value> = [
            "employee",
            "employee_name",
            "contract_type",
            "designation",
            "date_of_joining",
            "relieving_date",
            "weekly_working_hours",
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
            "company",
            "remuneration_brute",
            "lieu_travail",
            "periode_essai",
        ]
        .into_iter()
        .map(|c| json!({"fieldname": c, "fieldtype": "Data"}))
        .collect();
        if avec_motif {
            champs.push(json!({"fieldname": "motif_cdd", "fieldtype": "Small Text"}));
        }
        json!({"docs": [{"name": DOCTYPE_CONTRAT, "fields": champs}]})
    }

    fn format(html: &str, doc_type: &str, custom: u8) -> Value {
        json!({"data": {"name": "Contrat CDD", "doc_type": doc_type, "html": html, "custom_format": custom,
                        "print_format_type": "Jinja", "disabled": 0}})
    }

    #[test]
    fn format_avec_proposition() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &format(CDI_COMPLET, DOCTYPE_CONTRAT, 1));
        faux.repondre_json(200, &meta_contrat(false));
        let contexte = Contexte { cdi: false, cdd: true, temps_partiel: false };
        let verification =
            verifier_format(&Dokos::new(&faux), &Regles::par_defaut(), "Contrat CDD", &contexte).expect("vérification");
        assert!(!verification.rapport.conforme());
        let proposition = verification.proposition.expect("proposition");
        assert_eq!(proposition.champs_a_creer, ["motif_cdd"]);
        assert!(verification.rapport_propose.expect("rapport").conforme());
        assert_eq!(faux.requetes().len(), 2);
        assert!(faux.requetes().iter().all(|r| r.methode == dokos_core::transport::Methode::Get));
    }

    #[test]
    fn formats_refuses() {
        let contexte = Contexte::default();
        let faux = FauxTransport::new();
        faux.repondre_json(200, &format("", DOCTYPE_CONTRAT, 0));
        let erreur = verifier_format(&Dokos::new(&faux), &Regles::par_defaut(), "Contrat CDD", &contexte);
        assert_eq!(erreur, Err(ErreurVerification::FormatNonAnalysable("Contrat CDD".into())));

        let faux = FauxTransport::new();
        faux.repondre_json(200, &format("<p></p>", "Sales Invoice", 1));
        let erreur = verifier_format(&Dokos::new(&faux), &Regles::par_defaut(), "Contrat CDD", &contexte);
        assert!(matches!(erreur, Err(ErreurVerification::MauvaisDocType { .. })));
    }

    #[test]
    fn contrat_existant_avec_format() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": {"name": "HR-EMP-CONT-00003", "employee": "HR-EMP-00012", "contract_type": "CDD",
                             "designation": "Animatrice", "date_of_joining": "2026-12-01", "relieving_date": null,
                             "weekly_working_hours": 35.0, "remuneration_brute": 2000, "lieu_travail": "Crest",
                             "motif_cdd": ""}}),
        );
        faux.repondre_json(200, &meta_contrat(true));
        faux.repondre_json(200, &json!({"data": {"name": "HR-EMP-00012", "date_of_joining": "2026-12-01"}}));
        faux.repondre_json(200, &format(CDI_COMPLET, DOCTYPE_CONTRAT, 1));
        let verification = verifier_contrat_existant(
            &Dokos::new(&faux),
            &Regles::par_defaut(),
            "HR-EMP-CONT-00003",
            Some("Contrat CDD"),
        )
        .expect("vérification");
        assert!(a_des_erreurs(&verification.problemes));
        let texte: Vec<String> = verification.problemes.iter().map(ToString::to_string).collect();
        assert!(texte.iter().any(|t| t.contains("un CDD doit avoir une date de fin")), "{texte:?}");
        assert!(texte.iter().any(|t| t.contains("Motif précis")), "{texte:?}");
        let gabarit = verification.gabarit.expect("rapport du format");
        assert!(gabarit.non_presentes().any(|m| m.id == "motif_cdd"));
    }
}
