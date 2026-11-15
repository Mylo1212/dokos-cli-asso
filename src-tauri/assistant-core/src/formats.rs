//! Rédaction par l'IA des mentions manquantes d'un format d'impression.
//!
//! L'IA ne renvoie que des **blocs** à ajouter, un par mention manquante,
//! jamais le gabarit entier : elle ne peut donc rien retirer. Chaque bloc est
//! vérifié par Rust ; un bloc refusé est remplacé par celui du fichier des
//! mentions (relu par un humain), avec la raison du refus.

use std::collections::BTreeSet;

use conformite::Regles;
use conformite::gabarit::{Analyse, analyser};
use conformite::proposition::inserer_blocs;
use conformite::rapport::{EtatMention, RapportGabarit, existe_dans_dokos, verifier_gabarit};
use conformite::regles::{Contexte, Mention};
use dokos_core::Transport;
use dokos_core::date::Date;
use serde::Deserialize;

use crate::commun::consignes;
use crate::owui::{ErreurIa, OpenWebUi};

/// Longueur maximale d'un bloc proposé.
const BLOC_MAX: usize = 4_000;
/// Partie du gabarit actuel envoyée pour le style.
const GABARIT_ENVOYE_MAX: usize = 20_000;
/// Balises ou attributs refusés dans un bloc.
const HTML_INTERDIT: &[&str] =
    &["<script", "<iframe", "<object", "<embed", "<style", "<link", "javascript:", "onerror", "onload"];

const SCHEMA: &str = r#"{
  "ajouts": [{"mention": "identifiant de la mention", "html": "bloc HTML de la clause"}],
  "explications": "texte court : sources utilisées (convention collective, articles) et points à faire vérifier"
}"#;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Reponse {
    ajouts: Vec<AjoutIa>,
    explications: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AjoutIa {
    mention: String,
    html: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AjoutRetenu {
    pub mention: String,
    pub libelle: String,
    /// Vrai si le bloc de l'IA a été retenu, faux si c'est celui du fichier.
    pub par_ia: bool,
    /// Pourquoi le bloc de l'IA a été écarté.
    pub raison: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropositionFormatIa {
    pub gabarit: String,
    pub ajouts: Vec<AjoutRetenu>,
    pub explications: String,
    pub champs_a_creer: Vec<String>,
    pub analyse: Analyse,
    pub avertissements: Vec<String>,
}

/// `None` si aucune mention ne manque.
pub fn proposer_format<I: Transport>(
    ia: &OpenWebUi<I>,
    bases: &[String],
    regles: &Regles,
    gabarit: &str,
    rapport: &RapportGabarit,
    champs_dokos: &BTreeSet<String>,
    aujourdhui: Date,
) -> Result<Option<PropositionFormatIa>, ErreurIa> {
    let manquantes: Vec<&Mention> = rapport
        .mentions
        .iter()
        .filter(|r| r.etat == EtatMention::Manquante)
        .filter_map(|r| regles.mentions.iter().find(|m| m.id == r.id))
        .collect();
    if manquantes.is_empty() {
        return Ok(None);
    }

    let description: Vec<String> = manquantes
        .iter()
        .map(|m| {
            let champs = if m.champs.is_empty() { "aucun".to_owned() } else { m.champs.join(", ") };
            format!(
                "- id « {} » : {} ({}). Champs Dokos à afficher : {champs}. Exemple validé : {}",
                m.id, m.libelle, m.reference, m.proposition
            )
        })
        .collect();
    let extrait: String = gabarit.chars().take(GABARIT_ENVOYE_MAX).collect();
    let specifique = format!(
        "Il s'agit de compléter un modèle de contrat de travail (gabarit HTML/Jinja de Dokos) avec les mentions obligatoires manquantes.\n\
Pour chaque mention ci-dessous, rédige un bloc HTML court, en français, dans le style du modèle :\n{}\n\
Contraintes strictes :\n\
- Variables : uniquement {{{{ doc.<champ> }}}} ou {{{{ doc.get_formatted(\"<champ>\") }}}} avec les champs indiqués, et {{% if doc.<champ> %}}…{{% endif %}}.\n\
- Aucune autre fonction, aucun filtre, aucun script, aucun style.\n\
- Appuie-toi sur la convention collective et les documents des bases de connaissance s'ils précisent la clause, et cite-les dans \"explications\".\n\
- Ce que tu ne sais pas : écris « [à compléter : …] ».\n\
Début du modèle actuel, pour le style :\n{extrait}",
        description.join("\n")
    );

    let reponse = ia.demander_json::<Reponse>(
        &consignes(aujourdhui, &specifique, SCHEMA),
        "Rédige les mentions manquantes.",
        bases,
    )?;

    let mut blocs = Vec::new();
    let mut ajouts = Vec::new();
    for mention in &manquantes {
        let propose = reponse.donnees.ajouts.iter().find(|a| a.mention.trim() == mention.id);
        let verdict = match propose {
            None => Err("l'IA n'a pas rédigé cette mention".to_owned()),
            Some(ajout) => verifier_bloc(regles, mention, &ajout.html, champs_dokos),
        };
        match verdict {
            Ok(()) => {
                blocs.push(propose.map(|a| a.html.trim().to_owned()).unwrap_or_default());
                ajouts.push(AjoutRetenu {
                    mention: mention.id.clone(),
                    libelle: mention.libelle.clone(),
                    par_ia: true,
                    raison: None,
                });
            }
            Err(raison) => {
                blocs.push(mention.proposition.trim().to_owned());
                ajouts.push(AjoutRetenu {
                    mention: mention.id.clone(),
                    libelle: mention.libelle.clone(),
                    par_ia: false,
                    raison: Some(raison),
                });
            }
        }
    }

    let refs: Vec<&str> = blocs.iter().map(String::as_str).collect();
    let nouveau = inserer_blocs(gabarit, &refs);
    let analyse = analyser(&nouveau);
    let champs_a_creer = analyse.champs.iter().filter(|c| !existe_dans_dokos(c, champs_dokos)).cloned().collect();
    Ok(Some(PropositionFormatIa {
        gabarit: nouveau,
        ajouts,
        explications: reponse.donnees.explications.trim().chars().take(2_000).collect(),
        champs_a_creer,
        analyse,
        avertissements: reponse.avertissements,
    }))
}

/// Un bloc de l'IA est retenu s'il est sûr, n'affiche que des champs connus
/// et satisfait à lui seul la mention.
fn verifier_bloc(
    regles: &Regles,
    mention: &Mention,
    html: &str,
    champs_dokos: &BTreeSet<String>,
) -> Result<(), String> {
    let html = html.trim();
    if html.is_empty() {
        return Err("bloc vide".to_owned());
    }
    if html.chars().count() > BLOC_MAX {
        return Err("bloc trop long".to_owned());
    }
    let minuscules = html.to_lowercase();
    if let Some(interdit) = HTML_INTERDIT.iter().find(|i| minuscules.contains(*i)) {
        return Err(format!("contenu refusé : « {interdit} »"));
    }
    let analyse = analyser(html);
    if let Some(interdit) = analyse.interdits.first() {
        return Err(format!("construction interdite : {interdit}"));
    }
    if let Some(non_verifie) = analyse.non_verifies.first() {
        return Err(format!("construction non vérifiée : {non_verifie}"));
    }
    let mut champs_permis = champs_dokos.clone();
    champs_permis.extend(mention.champs.iter().cloned());
    if let Some(inconnu) = analyse.champs.iter().find(|c| !existe_dans_dokos(c, &champs_permis)) {
        return Err(format!("champ inconnu : {inconnu}"));
    }
    let seule = Regles { mentions: vec![mention.clone()], ..regles.clone() };
    let tout = Contexte { cdi: true, cdd: true, temps_partiel: true };
    let rapport = verifier_gabarit(&seule, &analyse, &tout, &champs_permis);
    if rapport.mentions.iter().any(|m| m.etat != EtatMention::Presente) {
        return Err("le bloc ne contient pas la mention demandée".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::faux::FauxTransport;

    use super::*;
    use crate::owui::tests::reponse_ia;

    fn champs() -> BTreeSet<String> {
        [
            "company",
            "employee_name",
            "designation",
            "date_of_joining",
            "relieving_date",
            "weekly_working_hours",
            "lieu_travail",
            "remuneration_brute",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    const GABARIT: &str = "<html><body>\
<p>Entre {{ doc.company }} et {{ doc.employee_name }}, poste {{ doc.designation }} à {{ doc.lieu_travail }},\
 à compter du {{ doc.date_of_joining }}.</p>\
<p>Rémunération : {{ doc.remuneration_brute }} ; durée : {{ doc.weekly_working_hours }} h.</p>\
<p>Congés payés légaux. Période d'essai : un mois. Convention collective ÉCLAT. Retraite complémentaire : X.</p>\
</body></html>";

    #[test]
    fn blocs_verifies_et_repli() {
        let regles = Regles::par_defaut();
        let contexte = Contexte { cdi: false, cdd: true, temps_partiel: false };
        let rapport = verifier_gabarit(&regles, &analyser(GABARIT), &contexte, &champs());
        let ids: Vec<&str> = rapport.non_presentes().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["motif_cdd", "terme_cdd"]);

        let ia = FauxTransport::new();
        // Motif : bloc correct. Terme : bloc qui lit la base de données → refusé, repli sur le fichier.
        ia.repondre_json(
            200,
            &reponse_ia(
                r#"{"ajouts": [
                    {"mention": "motif_cdd", "html": "<p>Ce contrat est conclu pour : {{ doc.motif_cdd }} (art. 3 de la convention).</p>"},
                    {"mention": "terme_cdd", "html": "<p>Fin le {{ frappe.db.get_value('Employee', doc.employee, 'relieving_date') }}</p>"},
                    {"mention": "inutile", "html": "<p>{{ doc.ctc }}</p>"}
                 ],
                 "explications": "Motif rédigé selon l'article 3."}"#,
            ),
        );
        let jour: Date = "2026-11-03".parse().expect("date");
        let proposition = proposer_format(&OpenWebUi::new(&ia, "m"), &[], &regles, GABARIT, &rapport, &champs(), jour)
            .expect("proposition")
            .expect("des mentions manquent");

        assert!(proposition.ajouts[0].par_ia);
        assert!(!proposition.ajouts[1].par_ia);
        assert!(proposition.ajouts[1].raison.as_deref().is_some_and(|r| r.starts_with("construction interdite")));
        assert!(proposition.gabarit.contains("(art. 3 de la convention)"));
        assert!(!proposition.gabarit.contains("frappe.db"));
        assert!(!proposition.gabarit.contains("doc.ctc"), "un ajout hors des mentions manquantes est ignoré");
        assert!(proposition.analyse.interdits.is_empty());
        assert_eq!(proposition.champs_a_creer, ["motif_cdd"]);
        assert_eq!(proposition.explications, "Motif rédigé selon l'article 3.");

        let consigne =
            ia.requetes()[0].corps.as_ref().map(|c| c["messages"][0]["content"].to_string()).unwrap_or_default();
        assert!(consigne.contains("id « motif_cdd »"), "{consigne}");
    }

    #[test]
    fn blocs_refuses() {
        let regles = Regles::par_defaut();
        let motif = regles.mentions.iter().find(|m| m.id == "motif_cdd").expect("mention");
        for (html, raison) in [
            ("", "bloc vide"),
            ("<p>Motif : {{ doc.motif_cdd }}</p><script>alert(1)</script>", "contenu refusé"),
            ("<p onload=\"x\">{{ doc.motif_cdd }}</p>", "contenu refusé"),
            ("<p>{{ doc.motif_cdd | markdown }}</p>", "non vérifiée"),
            ("<p>{{ doc.motif_cdd }} {{ doc.ctc }}</p>", "champ inconnu : ctc"),
            ("<p>Contrat à durée déterminée.</p>", "ne contient pas la mention"),
        ] {
            let erreur = verifier_bloc(&regles, motif, html, &champs()).expect_err(html);
            assert!(erreur.contains(raison), "{html} → {erreur}");
        }
        assert!(verifier_bloc(&regles, motif, "<p>Motif : {{ doc.motif_cdd }}</p>", &champs()).is_ok());
    }

    #[test]
    fn rien_a_rediger() {
        let regles = Regles::par_defaut();
        let contexte = Contexte { cdi: true, cdd: false, temps_partiel: false };
        let rapport = verifier_gabarit(&regles, &analyser(GABARIT), &contexte, &champs());
        let ia = FauxTransport::new();
        let jour: Date = "2026-11-03".parse().expect("date");
        let resultat = proposer_format(&OpenWebUi::new(&ia, "m"), &[], &regles, GABARIT, &rapport, &champs(), jour);
        assert_eq!(resultat, Ok(None));
        assert!(ia.requetes().is_empty());
    }
}
