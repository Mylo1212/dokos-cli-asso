//! « La liste des clients du groupe Collectivités », « les factures impayées
//! d'octobre » → liste et filtres proposés pour un export Excel.
//!
//! En deux temps, car la connexion Dokos dépend de la liste (profil Ventes
//! ou RH) : [`comprendre`] interroge l'IA seule, puis [`completer`] retrouve
//! chaque valeur dans Dokos (lecture seule). L'IA ne choisit ni DocType ni
//! champ : seulement une liste et des filtres de [`dokos_core::export`].

use dokos_core::date::Date;
use dokos_core::export::{FiltresExport, Reference, TEXTE_MAX, TypeListe, choisir_valeur, valeurs_possibles};
use dokos_core::recherche::{Correspondance, normaliser, trouver_client};
use dokos_core::types::Client;
use dokos_core::{Dokos, Transport};
use serde::Deserialize;

use crate::commun::{Choix, choisir, consignes, date, nettoyer_questions, texte};
use crate::owui::{ErreurIa, OpenWebUi, borner};

const SCHEMA: &str = r#"{
  "liste": "clients" | "articles" | "devis" | "commandes" | "factures" | "employes" | null,
  "client": "nom du client tel qu'il est écrit" | null,
  "groupe": "groupe de clients ou d'articles tel qu'il est écrit" | null,
  "territoire": "territoire tel qu'il est écrit" | null,
  "du": "AAAA-MM-JJ" | null,
  "au": "AAAA-MM-JJ" | null,
  "statut": "clé de statut de la liste" | null,
  "texte": "mot cherché dans le nom" | null,
  "inclure_inactifs": true | false,
  "vendables_seulement": true | false,
  "questions": ["question courte"]
}"#;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Extraction {
    liste: Option<String>,
    client: Option<String>,
    groupe: Option<String>,
    territoire: Option<String>,
    du: Option<String>,
    au: Option<String>,
    statut: Option<String>,
    texte: Option<String>,
    inclure_inactifs: Option<bool>,
    vendables_seulement: Option<bool>,
    questions: Vec<String>,
}

/// Réponse de l'IA, pas encore confrontée à Dokos.
#[derive(Debug, Clone)]
pub struct DemandeComprise {
    pub liste: Option<TypeListe>,
    extraction: Extraction,
    avertissements: Vec<String>,
}

/// Export proposé : rien n'est lu au-delà des vérifications.
#[derive(Debug, Clone, PartialEq)]
pub struct PropositionListe {
    pub liste: Option<TypeListe>,
    /// Filtres retenus, tous vérifiés.
    pub filtres: FiltresExport,
    pub client: Choix<Client>,
    /// Vrai si la liste et chaque filtre demandé ont été retrouvés : le
    /// fichier peut être produit sans autre question.
    pub pret: bool,
    pub questions: Vec<String>,
    pub avertissements: Vec<String>,
}

/// Filtres possibles et statuts de chaque liste, pour les consignes.
fn specifique() -> String {
    let mut texte =
        String::from("Il s'agit d'exporter une liste de Dokos dans un fichier Excel. Listes et filtres possibles :\n");
    for liste in TypeListe::TOUS {
        let mut filtres = Vec::new();
        if liste.champ_client().is_some() {
            filtres.push("client".to_owned());
        }
        if liste.groupe().is_some() {
            filtres.push(format!(
                "groupe (groupe d'{})",
                if liste == TypeListe::Clients { "clients" } else { "articles" }
            ));
        }
        if liste.a_territoire() {
            filtres.push("territoire".to_owned());
        }
        if let Some((_, libelle)) = liste.champ_date() {
            filtres.push(format!("du / au ({libelle})"));
        }
        if !liste.statuts().is_empty() {
            let cles: Vec<String> = liste.statuts().iter().map(|s| format!("{} = {}", s.cle, s.libelle)).collect();
            filtres.push(format!("statut ({})", cles.join(", ")));
        }
        filtres.push("texte".to_owned());
        if liste == TypeListe::Articles {
            filtres.push("vendables_seulement".to_owned());
        }
        filtres.push(format!("inclure_inactifs ({})", liste.libelle_inactifs()));
        texte.push_str(&format!("- {} : {}.\n", liste.nom(), filtres.join(", ")));
    }
    texte.push_str(
        "- \"liste\" : la clé exacte d'une liste ci-dessus, ou null si la demande ne correspond à aucune.\n\
- Un filtre que la demande ne mentionne pas reste null (ou false) : ne pose pas de question pour un filtre absent.\n\
- Période (« en octobre », « ce mois-ci », « l'an dernier ») : donne \"du\" et \"au\".\n\
- \"statut\" : seulement une clé de la liste choisie.\n\
- \"inclure_inactifs\" : true seulement si la demande veut aussi les éléments désactivés, annulés ou partis.",
    );
    texte
}

/// Interroge l'IA. Aucun appel à Dokos.
pub fn comprendre<I: Transport>(
    ia: &OpenWebUi<I>,
    bases: &[String],
    demande: &str,
    aujourdhui: Date,
) -> Result<DemandeComprise, ErreurIa> {
    let demande = borner(demande)?;
    let reponse = ia.demander_json::<Extraction>(&consignes(aujourdhui, &specifique(), SCHEMA), &demande, bases)?;
    let liste = texte(reponse.donnees.liste.clone()).and_then(|l| l.parse().ok());
    Ok(DemandeComprise { liste, extraction: reponse.donnees, avertissements: reponse.avertissements })
}

/// Proposition quand l'IA n'a reconnu aucune liste : une question, sans Dokos.
pub fn sans_liste(comprise: DemandeComprise) -> PropositionListe {
    let mut questions = vec![match texte(comprise.extraction.liste) {
        Some(l) => {
            format!("Liste « {l} » non disponible : clients, articles, devis, commandes, factures ou employés ?")
        }
        None => "Quelle liste : clients, articles, devis, commandes, factures ou employés ?".to_owned(),
    }];
    questions.extend(comprise.extraction.questions);
    PropositionListe {
        liste: None,
        filtres: FiltresExport::default(),
        client: Choix::NonPrecise,
        pret: false,
        questions: nettoyer_questions(questions),
        avertissements: comprise.avertissements,
    }
}

/// Retrouve dans Dokos chaque valeur citée (lecture seule). Sans liste
/// reconnue, revient à [`sans_liste`] et n'appelle pas Dokos.
pub fn completer<D: Transport>(dokos: &Dokos<D>, comprise: DemandeComprise) -> Result<PropositionListe, ErreurIa> {
    let Some(liste) = comprise.liste else {
        return Ok(sans_liste(comprise));
    };
    let DemandeComprise { extraction: e, mut avertissements, .. } = comprise;
    let mut questions = Vec::new();
    let mut filtres = FiltresExport::default();
    let mut pret = true;
    let ignorer = |filtre: &str, avertissements: &mut Vec<String>| {
        avertissements.push(format!(
            "filtre « {filtre} » ignoré : il ne s'applique pas à la liste des {}",
            liste.libelle().to_lowercase()
        ));
    };

    let client_demande = texte(e.client);
    let client = if liste.champ_client().is_some() {
        choisir(client_demande.as_deref(), |t| trouver_client(dokos, t))?
    } else {
        if client_demande.is_some() {
            ignorer("client", &mut avertissements);
        }
        Choix::NonPrecise
    };
    match &client {
        Choix::Trouve(c) => filtres.client = Some(c.id.clone()),
        Choix::Introuvable(t) => {
            pret = false;
            questions.push(format!("Aucun client actif « {t} » dans Dokos : lequel ?"));
        }
        Choix::APreciser { texte, .. } => {
            pret = false;
            questions.push(format!("Plusieurs clients correspondent à « {texte} » : lequel ?"));
        }
        Choix::NonPrecise => {}
    }

    for (reference, demande, nom) in
        [(Reference::Groupe, texte(e.groupe), "groupe"), (Reference::Territoire, texte(e.territoire), "territoire")]
    {
        let Some(demande) = demande else { continue };
        let possible = match reference {
            Reference::Groupe => liste.groupe().is_some(),
            Reference::Territoire => liste.a_territoire(),
        };
        if !possible {
            ignorer(nom, &mut avertissements);
            continue;
        }
        let valeurs = valeurs_possibles(dokos, liste, reference)?;
        match choisir_valeur(&demande, &valeurs) {
            Correspondance::Unique(v) => match reference {
                Reference::Groupe => filtres.groupe = Some(v),
                Reference::Territoire => filtres.territoire = Some(v),
            },
            Correspondance::Plusieurs(v) => {
                pret = false;
                questions.push(format!("Quel {nom} pour « {demande} » : {} ?", v.join(", ")));
            }
            Correspondance::Aucune => {
                pret = false;
                questions.push(format!("Aucun {nom} « {demande} » dans Dokos : lequel ?"));
            }
        }
    }

    let (du, au) = (texte(e.du), texte(e.au));
    if liste.champ_date().is_some() {
        let nb_questions = questions.len();
        filtres.du = date(du, "Début de période", &mut questions);
        filtres.au = date(au, "Fin de période", &mut questions);
        if questions.len() > nb_questions {
            pret = false;
        }
        if let (Some(d), Some(a)) = (filtres.du, filtres.au)
            && d > a
        {
            pret = false;
            (filtres.du, filtres.au) = (None, None);
            questions.push(format!("Période inversée (du {d} au {a}) : quelles dates ?"));
        }
    } else if du.is_some() || au.is_some() {
        ignorer("dates", &mut avertissements);
    }

    if let Some(statut) = texte(e.statut) {
        let cible = normaliser(&statut);
        match liste.statuts().iter().find(|s| normaliser(s.cle) == cible || normaliser(s.libelle) == cible) {
            Some(s) => filtres.statut = Some(s.cle.to_owned()),
            None if liste.statuts().is_empty() => ignorer("statut", &mut avertissements),
            None => {
                pret = false;
                let libelles: Vec<&str> = liste.statuts().iter().map(|s| s.libelle).collect();
                questions.push(format!("Statut « {statut} » inconnu : {} ?", libelles.join(", ")));
            }
        }
    }

    filtres.texte = texte(e.texte).map(|t| t.chars().take(TEXTE_MAX).collect());
    filtres.inclure_inactifs = e.inclure_inactifs.unwrap_or(false);
    if e.vendables_seulement.unwrap_or(false) {
        if liste == TypeListe::Articles {
            filtres.vendables_seulement = true;
        } else {
            ignorer("vendables seulement", &mut avertissements);
        }
    }
    // Par construction, les filtres retenus s'appliquent à la liste.
    if let Err(erreur) = filtres.verifier(liste) {
        pret = false;
        questions.push(erreur.to_string());
    }
    // Les questions de l'IA elle-même ne bloquent pas : la liste est complète.
    questions.extend(e.questions);
    Ok(PropositionListe {
        liste: Some(liste),
        filtres,
        client,
        pret,
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

    fn jour() -> Date {
        "2026-11-04".parse().expect("date")
    }

    fn comprise(json_ia: &str) -> (DemandeComprise, FauxTransport) {
        let ia = FauxTransport::new();
        ia.repondre_json(200, &reponse_ia(json_ia));
        let comprise = comprendre(&OpenWebUi::new(&ia, "m"), &[], "…", jour()).expect("comprise");
        (comprise, ia)
    }

    #[test]
    fn clients_d_un_groupe() {
        let (c, ia) = comprise(r#"{"liste": "clients", "groupe": "collectivites", "questions": []}"#);
        assert_eq!(c.liste, Some(TypeListe::Clients));
        let consigne =
            ia.requetes()[0].corps.as_ref().map(|c| c["messages"][0]["content"].to_string()).unwrap_or_default();
        assert!(consigne.contains("en_retard = en retard"), "{consigne}");
        assert!(consigne.contains("mercredi 2026-11-04"));

        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": [{"name": "Associations"}, {"name": "Collectivités"}]}));
        let p = completer(&Dokos::new(&dokos), c).expect("proposition");
        assert!(p.pret);
        assert_eq!(p.filtres.groupe.as_deref(), Some("Collectivités"));
        assert!(!p.filtres.inclure_inactifs);
        assert!(p.questions.is_empty());
        assert_eq!(dokos.requetes()[0].chemin, "/api/resource/Customer%20Group");
    }

    #[test]
    fn factures_impayees_d_un_client() {
        let (c, _) = comprise(
            r#"{"liste": "factures", "client": "mairie de crest", "du": "2026-10-01", "au": "2026-10-31",
                "statut": "impayées", "groupe": "Collectivités", "questions": ["Avec les avoirs ?"]}"#,
        );
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        let p = completer(&Dokos::new(&dokos), c).expect("proposition");
        assert!(p.pret, "{p:?}");
        assert_eq!(p.filtres.client.as_deref(), Some("CUST-2"));
        assert_eq!(p.filtres.statut.as_deref(), Some("impayee"));
        assert_eq!(p.filtres.du.map(|d| d.to_string()).as_deref(), Some("2026-10-01"));
        // Le groupe ne s'applique pas aux factures : ignoré, sans appel à Dokos.
        assert_eq!(p.avertissements.len(), 1);
        assert_eq!(p.questions, ["Avec les avoirs ?"]);
        assert!(dokos.requetes().iter().all(|r| r.methode == Methode::Get));
        assert_eq!(dokos.requetes().len(), 1);
    }

    #[test]
    fn valeurs_introuvables_en_questions() {
        let (c, _) =
            comprise(r#"{"liste": "commandes", "client": "Licorne", "statut": "perdue", "du": "le 3", "au": null}"#);
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": []}));
        let p = completer(&Dokos::new(&dokos), c).expect("proposition");
        assert!(!p.pret);
        assert_eq!(p.client, Choix::Introuvable("Licorne".into()));
        assert_eq!(p.filtres, FiltresExport::default());
        assert_eq!(p.questions.len(), 3, "{:?}", p.questions);
        assert!(p.questions[2].starts_with("Statut « perdue » inconnu"));
    }

    #[test]
    fn groupe_ambigu_et_vendables() {
        let (c, _) = comprise(r#"{"liste": "articles", "groupe": "repas", "vendables_seulement": true}"#);
        let dokos = FauxTransport::new();
        dokos.repondre_json(200, &json!({"data": [{"name": "Repas chauds"}, {"name": "Repas froids"}]}));
        let p = completer(&Dokos::new(&dokos), c).expect("proposition");
        assert!(!p.pret);
        assert!(p.filtres.vendables_seulement);
        assert_eq!(p.questions, ["Quel groupe pour « repas » : Repas chauds, Repas froids ?"]);
    }

    #[test]
    fn liste_inconnue() {
        let (c, _) = comprise(r#"{"liste": "fournisseurs"}"#);
        assert_eq!(c.liste, None);
        let dokos = FauxTransport::new();
        let p = completer(&Dokos::new(&dokos), c).expect("proposition");
        assert!(!p.pret);
        assert!(p.questions[0].starts_with("Liste « fournisseurs » non disponible"));
        assert!(dokos.requetes().is_empty());
    }
}
