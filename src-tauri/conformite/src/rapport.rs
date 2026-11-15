//! Conformité d'un gabarit aux mentions obligatoires.

use std::collections::BTreeSet;

use crate::gabarit::Analyse;
use crate::normalisation::{contient_mots, normaliser};
use crate::regles::{Contexte, Regles};

/// Propriétés présentes sur tout document Frappe, sans être des champs du DocType.
const PROPRIETES_STANDARD: &[&str] =
    &["name", "owner", "creation", "modified", "modified_by", "docstatus", "idx", "doctype"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EtatMention {
    Presente,
    Manquante,
    /// Le gabarit affiche ces champs, mais ils n'existent pas dans Dokos :
    /// ils s'imprimeront vides tant qu'ils ne sont pas créés.
    ChampsAbsentsDeDokos(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultatMention {
    pub id: String,
    pub libelle: String,
    pub reference: String,
    pub etat: EtatMention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RapportGabarit {
    pub regles_validees: bool,
    pub version_regles: String,
    pub mentions: Vec<ResultatMention>,
    pub interdits: Vec<String>,
    pub non_verifies: Vec<String>,
    pub a_completer: usize,
    /// Champs `doc.x` du gabarit qui n'existent pas dans Dokos.
    pub champs_inconnus: Vec<String>,
}

impl RapportGabarit {
    /// Toutes les mentions présentes, rien d'interdit, rien à compléter.
    /// Ne vaut pas validation juridique, surtout si la liste n'est pas validée.
    pub fn conforme(&self) -> bool {
        self.mentions.iter().all(|m| m.etat == EtatMention::Presente)
            && self.interdits.is_empty()
            && self.a_completer == 0
    }

    pub fn non_presentes(&self) -> impl Iterator<Item = &ResultatMention> {
        self.mentions.iter().filter(|m| m.etat != EtatMention::Presente)
    }
}

pub fn existe_dans_dokos(champ: &str, champs_dokos: &BTreeSet<String>) -> bool {
    champs_dokos.contains(champ) || PROPRIETES_STANDARD.contains(&champ)
}

/// `champs_dokos` : champs du DocType imprimé (`Employment Contract`), lus dans Dokos.
pub fn verifier_gabarit(
    regles: &Regles,
    analyse: &Analyse,
    contexte: &Contexte,
    champs_dokos: &BTreeSet<String>,
) -> RapportGabarit {
    let mentions = regles
        .applicables(contexte)
        .map(|mention| {
            let par_texte = mention.textes.iter().any(|t| contient_mots(&analyse.texte, &normaliser(t)));
            let affiches: Vec<&String> = mention.champs.iter().filter(|c| analyse.champs.contains(*c)).collect();
            let par_champ = affiches.iter().any(|c| existe_dans_dokos(c, champs_dokos));
            let etat = if par_texte || par_champ {
                EtatMention::Presente
            } else if affiches.is_empty() {
                EtatMention::Manquante
            } else {
                EtatMention::ChampsAbsentsDeDokos(affiches.into_iter().cloned().collect())
            };
            ResultatMention {
                id: mention.id.clone(),
                libelle: mention.libelle.clone(),
                reference: mention.reference.clone(),
                etat,
            }
        })
        .collect();
    RapportGabarit {
        regles_validees: regles.est_validee(),
        version_regles: regles.version.clone(),
        mentions,
        interdits: analyse.interdits.clone(),
        non_verifies: analyse.non_verifies.clone(),
        a_completer: analyse.a_completer,
        champs_inconnus: analyse.champs.iter().filter(|c| !existe_dans_dokos(c, champs_dokos)).cloned().collect(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::gabarit::analyser;

    pub(crate) fn champs_dokos() -> BTreeSet<String> {
        [
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
        .map(str::to_owned)
        .collect()
    }

    /// Gabarit de CDI à temps plein complet.
    pub(crate) const CDI_COMPLET: &str = r#"
        <h1>Contrat de travail à durée indéterminée</h1>
        <p>Entre {{ doc.company }} et {{ doc.employee_name }}.</p>
        <p>Poste : {{ doc.designation }}, à {{ doc.lieu_travail }}, à compter du {{ doc.get_formatted("date_of_joining") }}.</p>
        <p>Période d'essai : {{ doc.periode_essai }}.</p>
        <p>Rémunération : {{ doc.get_formatted("remuneration_brute") }} ; durée : {{ doc.weekly_working_hours }} h.</p>
        <p>Congés payés selon la loi. Convention collective ÉCLAT (IDCC 1518).</p>
        <p>Retraite complémentaire : Malakoff Humanis.</p>
    "#;

    fn contexte_cdi() -> Contexte {
        Contexte { cdi: true, cdd: false, temps_partiel: false }
    }

    #[test]
    fn gabarit_complet() {
        let rapport = verifier_gabarit(&Regles::par_defaut(), &analyser(CDI_COMPLET), &contexte_cdi(), &champs_dokos());
        let manquantes: Vec<_> = rapport.non_presentes().map(|m| m.id.as_str()).collect();
        assert!(manquantes.is_empty(), "{manquantes:?}");
        assert!(rapport.conforme());
        assert!(!rapport.regles_validees);
    }

    #[test]
    fn mentions_cdd_manquantes() {
        let contexte = Contexte { cdi: false, cdd: true, temps_partiel: false };
        let rapport = verifier_gabarit(&Regles::par_defaut(), &analyser(CDI_COMPLET), &contexte, &champs_dokos());
        let manquantes: Vec<_> = rapport.non_presentes().map(|m| m.id.as_str()).collect();
        assert_eq!(manquantes, ["motif_cdd", "terme_cdd"]);
        assert!(!rapport.conforme());
    }

    #[test]
    fn champ_affiche_mais_absent_de_dokos() {
        let gabarit =
            format!("{CDI_COMPLET}<p>Motif : {{{{ doc.motif_cdd }}}}. Fin : {{{{ doc.relieving_date }}}}</p>");
        let contexte = Contexte { cdi: false, cdd: true, temps_partiel: false };
        let rapport = verifier_gabarit(&Regles::par_defaut(), &analyser(&gabarit), &contexte, &champs_dokos());
        let motif = rapport.mentions.iter().find(|m| m.id == "motif_cdd").expect("mention");
        assert_eq!(motif.etat, EtatMention::ChampsAbsentsDeDokos(vec!["motif_cdd".into()]));
        assert_eq!(rapport.champs_inconnus, ["motif_cdd"]);
        let terme = rapport.mentions.iter().find(|m| m.id == "terme_cdd").expect("mention");
        assert_eq!(terme.etat, EtatMention::Presente);
    }

    #[test]
    fn texte_a_completer_ou_acces_interdit() {
        let rapport = verifier_gabarit(
            &Regles::par_defaut(),
            &analyser(&format!("{CDI_COMPLET}<p>[à compléter]</p>")),
            &contexte_cdi(),
            &champs_dokos(),
        );
        assert_eq!(rapport.a_completer, 1);
        assert!(!rapport.conforme());

        let rapport = verifier_gabarit(
            &Regles::par_defaut(),
            &analyser(&format!("{CDI_COMPLET}{{{{ frappe.db.get_value('Employee', 'X', 'ctc') }}}}")),
            &contexte_cdi(),
            &champs_dokos(),
        );
        assert!(!rapport.conforme());
        assert_eq!(rapport.interdits.len(), 1);
    }
}
