//! Logique des commandes de l'interface, testable sans Tauri ni réseau.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use conformite::Regles;
use conformite::dokos::{VerificationContrat, verifier_format, verifier_nouveau_contrat};
use conformite::donnees::{Gravite, Probleme, a_des_erreurs};
use conformite::proposition::{LigneDiff, difference};
use conformite::rapport::{EtatMention, RapportGabarit};
use conformite::regles::Contexte;
use dokos_core::contrat::{
    CreationContrat, DOCTYPE_CONTRAT, NouveauContrat, champs_libres, creer_contrat, lister_types_contrat,
};
use dokos_core::date::{Date, DateHeure};
use dokos_core::doublon::{DocumentExistant, SiDoublon, chercher_doublons};
use dokos_core::employe::{
    CreationEmploye, DOCTYPE_EMPLOYE, EmployeExistant, NouvelEmploye, chercher_employes, creer_employe, lister_genres,
    lister_postes,
};
use dokos_core::evenement::{
    CreationEvenement, DOCTYPE_EVENEMENT, EvenementExistant, NouvelEvenement, chercher_evenement_identique,
    creer_evenement, publier,
};
use dokos_core::formulaire::{DOCTYPE_INSCRIPTION, NouveauFormulaire, Question, questions_possibles};
use dokos_core::impression::{lister_formats, telecharger_pdf};
use dokos_core::meta::{Champ, lire_meta};
use dokos_core::recherche::{chercher_article_dans, chercher_client};
use dokos_core::vente::{Creation, LigneVente, NouveauDocumentVente, TypeVente, creer_brouillon};
use dokos_core::{Dokos, Transport};

use crate::vues::*;

pub use crate::erreurs::{ErreurVue, erreur as texte};

/// Erreur structurée (genre + message) affichée par l'interface.
pub type Resultat<T> = Result<T, ErreurVue>;

/// Lien vers un document dans le bureau Dokos : `/app/sales-order/SAL-ORD-…`.
pub fn lien_desk(base: &str, doctype: &str, id: &str) -> String {
    let route = doctype.to_lowercase().replace(' ', "-");
    format!("{base}/app/{route}/{}", dokos_core::client::encoder_segment(id))
}

fn date(texte_date: &str) -> Resultat<Date> {
    texte_date.parse().map_err(texte)
}

fn date_option(texte_date: Option<&str>) -> Resultat<Option<Date>> {
    texte_date.map(str::trim).filter(|d| !d.is_empty()).map(date).transpose()
}

fn non_vide(valeur: Option<&str>) -> Option<&str> {
    valeur.map(str::trim).filter(|v| !v.is_empty())
}

// --- Ventes -------------------------------------------------------------

pub fn chercher_clients<T: Transport>(dokos: &Dokos<T>, recherche: &str) -> Resultat<Vec<ClientVue>> {
    let clients = chercher_client(dokos, recherche).map_err(texte)?;
    Ok(clients.into_iter().map(|c| ClientVue { id: c.id, nom: c.nom }).collect())
}

pub fn vue_article(a: dokos_core::types::Article) -> ArticleVue {
    ArticleVue { code: a.code, libelle: a.libelle, unite: a.unite, groupe: a.groupe }
}

/// Recherche d'articles, limitée à un groupe d'articles si `groupe` est donné.
pub fn chercher_articles<T: Transport>(
    dokos: &Dokos<T>,
    recherche: &str,
    groupe: Option<&str>,
) -> Resultat<Vec<ArticleVue>> {
    let articles = chercher_article_dans(dokos, recherche, groupe).map_err(texte)?;
    Ok(articles.into_iter().map(vue_article).collect())
}

pub fn type_vente(nom: &str) -> Resultat<TypeVente> {
    match nom {
        "devis" => Ok(TypeVente::Devis),
        "commande" => Ok(TypeVente::Commande),
        "facture" => Ok(TypeVente::Facture),
        autre => Err(ErreurVue::validation(format!("type de document inconnu : {autre}"))),
    }
}

pub fn document_vente(demande: &DemandeVente, societe: Option<&str>) -> Resultat<NouveauDocumentVente> {
    let type_vente = type_vente(&demande.type_vente)?;
    let lignes = demande
        .lignes
        .iter()
        .map(|l| LigneVente::new(&l.article, l.quantite, date_option(l.livraison.as_deref())?).map_err(texte))
        .collect::<Resultat<Vec<_>>>()?;
    NouveauDocumentVente::new(
        type_vente,
        &demande.client,
        date(&demande.date)?,
        date_option(demande.echeance.as_deref())?,
        lignes,
    )
    .map_err(texte)?
    .avec_reference(demande.reference.as_deref())
    .map(|d| d.avec_societe(societe))
    .map_err(texte)
}

pub(crate) fn vue_document(base: &str, type_vente: TypeVente, d: &DocumentExistant) -> DocumentVue {
    DocumentVue {
        id: d.id.clone(),
        statut: d.statut().to_owned(),
        date: d.date_document.clone(),
        total: d.total,
        devise: d.devise.clone(),
        lien: lien_desk(base, type_vente.doctype(), &d.id),
    }
}

pub fn apercu_vente<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeVente,
    societe: Option<&str>,
) -> Resultat<ApercuVente> {
    let document = document_vente(demande, societe)?;
    let t = document.type_vente();
    let doublons = chercher_doublons(dokos, &document).map_err(texte)?;
    Ok(ApercuVente {
        identiques: doublons.identiques.iter().map(|d| vue_document(base, t, d)).collect(),
        similaires: doublons.similaires.iter().map(|d| vue_document(base, t, d)).collect(),
    })
}

pub fn creer_vente<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeVente,
    societe: Option<&str>,
    si_doublon: SiDoublon,
) -> Resultat<ResultatVente> {
    let document = document_vente(demande, societe)?;
    let t = document.type_vente();
    match creer_brouillon(dokos, &document, si_doublon).map_err(texte)? {
        Creation::DejaExistant(existant) => {
            Ok(ResultatVente { cree: false, document: vue_document(base, t, &existant), autres: Vec::new() })
        }
        Creation::Cree { document: cree, autres } => Ok(ResultatVente {
            cree: true,
            document: DocumentVue {
                id: cree.id.clone(),
                statut: "brouillon".to_owned(),
                date: document.date().to_string(),
                total: cree.total,
                devise: cree.devise,
                lien: lien_desk(base, t.doctype(), &cree.id),
            },
            autres: autres.iter().map(|d| vue_document(base, t, d)).collect(),
        }),
    }
}

// --- Événements et formulaires ------------------------------------------

fn vue_champ(champ: &Champ, obligatoire: bool) -> ChampVue {
    ChampVue {
        champ: champ.fieldname.clone(),
        libelle: champ.libelle().to_owned(),
        type_champ: champ.fieldtype.clone(),
        obligatoire,
    }
}

pub fn questions_formulaire<T: Transport>(dokos: &Dokos<T>) -> Resultat<Vec<ChampVue>> {
    let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION).map_err(texte)?;
    Ok(questions_possibles(&meta).iter().map(|c| vue_champ(c, false)).collect())
}

fn evenement_et_formulaire<T: Transport>(
    dokos: &Dokos<T>,
    demande: &DemandeEvenement,
) -> Resultat<(NouvelEvenement, NouveauFormulaire)> {
    let debut: DateHeure = demande.debut.parse().map_err(texte)?;
    let fin = non_vide(demande.fin.as_deref()).map(str::parse::<DateHeure>).transpose().map_err(texte)?;
    let evenement = NouvelEvenement::new(&demande.titre, debut, fin)
        .map_err(texte)?
        .journee_entiere(demande.journee)
        .avec_lieu(demande.lieu.as_deref())
        .avec_description(demande.description.as_deref())
        .avec_places(demande.places)
        .map_err(texte)?
        .avec_annulation(demande.annulation);
    let questions: Vec<Question> =
        demande.questions.iter().map(|q| Question { champ: q.champ.clone(), obligatoire: q.obligatoire }).collect();
    let meta = lire_meta(dokos, DOCTYPE_INSCRIPTION).map_err(texte)?;
    let formulaire = NouveauFormulaire::new(&meta, &questions).map_err(texte)?;
    Ok((evenement, formulaire))
}

fn vue_evenement(base: &str, e: &EvenementExistant) -> DocumentVue {
    DocumentVue {
        id: e.id.clone(),
        statut: if e.published == 1 { "publié" } else { "non publié" }.to_owned(),
        date: e.debut.clone(),
        total: 0.0,
        devise: String::new(),
        lien: lien_desk(base, DOCTYPE_EVENEMENT, &e.id),
    }
}

pub fn apercu_evenement<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeEvenement,
) -> Resultat<ApercuEvenement> {
    let (evenement, formulaire) = evenement_et_formulaire(dokos, demande)?;
    let existant = chercher_evenement_identique(dokos, &evenement).map_err(texte)?;
    Ok(ApercuEvenement {
        formulaire_titre: formulaire.titre().to_owned(),
        formulaire_route: formulaire.route().to_owned(),
        champs: formulaire
            .champs()
            .iter()
            .map(|c| ChampVue {
                champ: c.fieldname.clone(),
                libelle: c.label.clone(),
                type_champ: c.fieldtype.clone(),
                obligatoire: c.obligatoire,
            })
            .collect(),
        existant: existant.as_ref().map(|e| vue_evenement(base, e)),
    })
}

pub fn creer_un_evenement<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeEvenement,
    si_doublon: SiDoublon,
) -> Resultat<ResultatEvenement> {
    let (evenement, formulaire) = evenement_et_formulaire(dokos, demande)?;
    match creer_evenement(dokos, &evenement, &formulaire, si_doublon).map_err(texte)? {
        CreationEvenement::DejaExistant(e) => Ok(ResultatEvenement {
            cree: false,
            evenement: vue_evenement(base, &e),
            formulaire: e.registration_form.clone(),
            formulaire_reutilise: true,
        }),
        CreationEvenement::Cree { evenement, formulaire } => Ok(ResultatEvenement {
            cree: true,
            evenement: vue_evenement(base, &evenement),
            formulaire: formulaire.nom,
            formulaire_reutilise: formulaire.reutilise,
        }),
    }
}

pub fn publier_un_evenement<T: Transport>(dokos: &Dokos<T>, base: &str, id: &str) -> Resultat<PublicationVue> {
    let publication = publier(dokos, id).map_err(texte)?;
    Ok(PublicationVue {
        page: format!("{base}/{}", publication.route_evenement),
        inscription: format!("{base}/{}", publication.route_inscription),
    })
}

// --- RH -------------------------------------------------------------------

pub fn listes_rh<T: Transport>(dokos: &Dokos<T>, regles: &Regles, societe: Option<&str>) -> Resultat<ListesRh> {
    let meta = lire_meta(dokos, DOCTYPE_CONTRAT).map_err(texte)?;
    Ok(ListesRh {
        genres: lister_genres(dokos).map_err(texte)?,
        postes: lister_postes(dokos).map_err(texte)?,
        types_contrat: lister_types_contrat(dokos).map_err(texte)?,
        formats: lister_formats(dokos, DOCTYPE_CONTRAT).map_err(texte)?,
        champs_libres: champs_libres(&meta).iter().map(|c| vue_champ(c, c.reqd == 1)).collect(),
        societe: societe.unwrap_or_default().to_owned(),
        regles_version: regles.version.clone(),
        regles_validees: regles.est_validee(),
        regles_a_relire: regles_a_relire(regles, Date::aujourdhui()),
    })
}

/// Vrai si la liste a été validée il y a plus d'un an (ou si sa date est illisible).
pub fn regles_a_relire(regles: &Regles, aujourdhui: Date) -> bool {
    if !regles.est_validee() {
        return false;
    }
    regles.valide_le.trim().parse::<Date>().map(|d| d.jours_jusqua(aujourdhui) > 366).unwrap_or(true)
}

fn vue_employe(base: &str, e: &EmployeExistant) -> EmployeVue {
    EmployeVue {
        id: e.id.clone(),
        nom: e.nom_complet.clone(),
        entree: e.entree.clone(),
        lien: lien_desk(base, DOCTYPE_EMPLOYE, &e.id),
    }
}

pub fn chercher_des_employes<T: Transport>(dokos: &Dokos<T>, base: &str, recherche: &str) -> Resultat<Vec<EmployeVue>> {
    let employes = chercher_employes(dokos, recherche).map_err(texte)?;
    Ok(employes.iter().map(|e| vue_employe(base, e)).collect())
}

pub fn creer_un_employe<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    demande: &DemandeEmploye,
    societe: Option<&str>,
) -> Resultat<ResultatEmploye> {
    let societe =
        non_vide(societe).ok_or_else(|| ErreurVue::configuration("société manquante : renseignez-la dans Réglages"))?;
    let employe = NouvelEmploye::new(
        &demande.prenom,
        &demande.nom,
        &demande.genre,
        date(&demande.naissance)?,
        date(&demande.entree)?,
        societe,
    )
    .map_err(texte)?
    .avec_email(demande.email.as_deref())
    .map_err(texte)?
    .avec_poste(demande.poste.as_deref());
    match creer_employe(dokos, &employe, SiDoublon::Reutiliser).map_err(texte)? {
        CreationEmploye::DejaExistant(e) => Ok(ResultatEmploye { cree: false, employe: vue_employe(base, &e) }),
        CreationEmploye::Cree(e) => Ok(ResultatEmploye { cree: true, employe: vue_employe(base, &e) }),
    }
}

fn nouveau_contrat<T: Transport>(
    dokos: &Dokos<T>,
    demande: &DemandeContrat,
    societe: Option<&str>,
) -> Resultat<NouveauContrat> {
    let meta = lire_meta(dokos, DOCTYPE_CONTRAT).map_err(texte)?;
    let champs: BTreeMap<String, String> = demande.champs.clone();
    NouveauContrat::new(
        &demande.employe,
        &demande.type_contrat,
        &demande.poste,
        date(&demande.entree)?,
        date_option(demande.fin.as_deref())?,
    )
    .and_then(|c| c.avec_heures(demande.heures, demande.repartition))
    .and_then(|c| c.avec_societe(societe).avec_champs(&meta, champs))
    .map_err(texte)
}

pub fn vue_problemes(problemes: &[Probleme]) -> Vec<ProblemeVue> {
    problemes
        .iter()
        .map(|p| ProblemeVue {
            gravite: match p.gravite {
                Gravite::Erreur => "erreur",
                Gravite::Avertissement => "avertissement",
            }
            .to_owned(),
            message: p.message.clone(),
            reference: p.reference.clone(),
        })
        .collect()
}

pub fn vue_rapport(rapport: &RapportGabarit) -> RapportVue {
    RapportVue {
        conforme: rapport.conforme(),
        regles_validees: rapport.regles_validees,
        version_regles: rapport.version_regles.clone(),
        mentions: rapport
            .mentions
            .iter()
            .map(|m| {
                let (etat, detail) = match &m.etat {
                    EtatMention::Presente => ("presente", String::new()),
                    EtatMention::Manquante => ("manquante", String::new()),
                    EtatMention::ChampsAbsentsDeDokos(champs) => {
                        ("champ_absent", format!("champ à créer dans Dokos : {}", champs.join(", ")))
                    }
                };
                MentionVue { libelle: m.libelle.clone(), reference: m.reference.clone(), etat: etat.to_owned(), detail }
            })
            .collect(),
        interdits: rapport.interdits.clone(),
        non_verifies: rapport.non_verifies.clone(),
        a_completer: rapport.a_completer,
        champs_inconnus: rapport.champs_inconnus.clone(),
    }
}

fn vue_verification(verification: &VerificationContrat) -> VerificationContratVue {
    VerificationContratVue {
        problemes: vue_problemes(&verification.problemes),
        bloquant: a_des_erreurs(&verification.problemes),
        gabarit: verification.gabarit.as_ref().map(vue_rapport),
    }
}

pub fn verifier_un_contrat<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    demande: &DemandeContrat,
    societe: Option<&str>,
) -> Resultat<VerificationContratVue> {
    let contrat = nouveau_contrat(dokos, demande, societe)?;
    let verification =
        verifier_nouveau_contrat(dokos, regles, &contrat, non_vide(demande.format.as_deref())).map_err(texte)?;
    Ok(vue_verification(&verification))
}

/// Vérifie de nouveau puis crée : un contrat avec des erreurs n'est jamais créé,
/// même si l'interface n'a pas lancé la vérification.
pub fn creer_un_contrat<T: Transport>(
    dokos: &Dokos<T>,
    base: &str,
    regles: &Regles,
    demande: &DemandeContrat,
    societe: Option<&str>,
    si_doublon: SiDoublon,
) -> Resultat<ResultatContrat> {
    let contrat = nouveau_contrat(dokos, demande, societe)?;
    let verification =
        verifier_nouveau_contrat(dokos, regles, &contrat, non_vide(demande.format.as_deref())).map_err(texte)?;
    if a_des_erreurs(&verification.problemes) {
        return Err(ErreurVue::validation(
            "le contrat a des erreurs : rien n'a été créé. Lancez « Vérifier » pour les voir.",
        ));
    }
    let (cree, contrat) = match creer_contrat(dokos, &contrat, si_doublon).map_err(texte)? {
        CreationContrat::DejaExistant(c) => (false, c),
        CreationContrat::Cree(c) => (true, c),
    };
    Ok(ResultatContrat { cree, lien: lien_desk(base, DOCTYPE_CONTRAT, &contrat.id), id: contrat.id })
}

pub fn contexte_format(regles: &Regles, type_contrat: &str, temps_partiel: bool) -> Contexte {
    let mut contexte = regles.contexte_contrat(type_contrat, None);
    contexte.temps_partiel = temps_partiel;
    contexte
}

/// Lignes communes au-delà de ce nombre autour d'un changement : masquées.
const CONTEXTE_DIFF: usize = 2;

pub fn compacter_diff(lignes: Vec<LigneDiff>) -> Vec<LigneDiffVue> {
    let change = |l: &LigneDiff| !matches!(l, LigneDiff::Commune(_));
    let garder: Vec<bool> = (0..lignes.len())
        .map(|i| {
            let debut = i.saturating_sub(CONTEXTE_DIFF);
            let fin = (i + CONTEXTE_DIFF + 1).min(lignes.len());
            lignes[debut..fin].iter().any(change)
        })
        .collect();
    let mut vues = Vec::new();
    let mut masquees = 0;
    for (ligne, garder) in lignes.into_iter().zip(garder) {
        if !garder {
            masquees += 1;
            continue;
        }
        if masquees > 0 {
            vues.push(LigneDiffVue {
                genre: "saut".to_owned(), texte: format!("… {masquees} lignes inchangées …")
            });
            masquees = 0;
        }
        let (genre, texte) = match ligne {
            LigneDiff::Commune(t) => ("commune", t),
            LigneDiff::Ajoutee(t) => ("ajout", t),
            LigneDiff::Retiree(t) => ("retrait", t),
        };
        vues.push(LigneDiffVue { genre: genre.to_owned(), texte });
    }
    if masquees > 0 {
        vues.push(LigneDiffVue { genre: "saut".to_owned(), texte: format!("… {masquees} lignes inchangées …") });
    }
    vues
}

pub fn verifier_un_format<T: Transport>(
    dokos: &Dokos<T>,
    regles: &Regles,
    format: &str,
    contexte: &Contexte,
) -> Resultat<(VerificationFormatVue, Option<String>)> {
    let verification = verifier_format(dokos, regles, format, contexte).map_err(texte)?;
    let proposition = verification.proposition.as_ref().map(|p| PropositionVue {
        ajouts: p.ajouts.clone(),
        champs_a_creer: p.champs_a_creer.clone(),
        a_completer: verification.rapport_propose.as_ref().map_or(0, |r| r.a_completer),
        diff: compacter_diff(difference(&verification.gabarit, &p.gabarit)),
    });
    let gabarit_propose = verification.proposition.map(|p| p.gabarit);
    Ok((VerificationFormatVue { rapport: vue_rapport(&verification.rapport), proposition }, gabarit_propose))
}

pub fn chemin_proposition(dossier: &Path, format: &str) -> PathBuf {
    dossier.join(format!("{}.proposition.html", dokos_config::nom_de_fichier(format)))
}

pub fn pdf_contrat<T: Transport>(dokos: &Dokos<T>, dossier: &Path, id: &str, format: &str) -> Resultat<PathBuf> {
    let pdf = telecharger_pdf(dokos, DOCTYPE_CONTRAT, id, format).map_err(texte)?;
    let chemin = dossier.join(format!("{}.pdf", dokos_config::nom_de_fichier(id)));
    std::fs::write(&chemin, pdf).map_err(|e| ErreurVue::fichier(format!("{} : {e}", chemin.display())))?;
    Ok(chemin)
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    const BASE: &str = "https://dokos.exemple.fr";

    fn demande_vente(type_vente: &str) -> DemandeVente {
        DemandeVente {
            type_vente: type_vente.into(),
            client: "CUST-2".into(),
            date: "2026-11-12".into(),
            echeance: Some(String::new()),
            lignes: vec![LigneDemande { article: "REPAS".into(), quantite: 25.0, livraison: None }],
            reference: None,
        }
    }

    #[test]
    fn liens_vers_dokos() {
        assert_eq!(
            lien_desk(BASE, "Sales Order", "SAL-ORD-2026-00012"),
            "https://dokos.exemple.fr/app/sales-order/SAL-ORD-2026-00012"
        );
        assert_eq!(lien_desk(BASE, "Event", "EV 1/2"), "https://dokos.exemple.fr/app/event/EV%201%2F2");
    }

    #[test]
    fn demandes_de_vente() {
        for (nom, champ_date) in
            [("devis", "transaction_date"), ("commande", "delivery_date"), ("facture", "posting_date")]
        {
            let document = document_vente(&demande_vente(nom), Some("Les Amanins")).expect(nom);
            assert_eq!(document.corps()[champ_date], "2026-11-12", "{nom}");
            assert!(document.corps().get("valid_till").is_none() && document.corps().get("due_date").is_none());
        }
        assert!(document_vente(&demande_vente("avoir"), None).is_err());
        let mut sans_ligne = demande_vente("devis");
        sans_ligne.lignes.clear();
        assert_eq!(
            document_vente(&sans_ligne, None).err().map(|e| e.message),
            Some("le document n'a aucune ligne".to_owned())
        );
        let mut avec_reference = demande_vente("facture");
        avec_reference.reference = Some("LOT-1".into());
        assert_eq!(document_vente(&avec_reference, None).expect("facture").corps()["po_no"], "LOT-1");
    }

    #[test]
    fn vente_creee_avec_lien() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "SAL-QTN-1", "docstatus": 0, "grand_total": 300.0, "currency": "EUR"}}),
        );
        let resultat =
            creer_vente(&Dokos::new(&faux), BASE, &demande_vente("devis"), None, SiDoublon::Reutiliser).expect("devis");
        assert!(resultat.cree);
        assert_eq!(resultat.document.lien, "https://dokos.exemple.fr/app/quotation/SAL-QTN-1");
    }

    fn meta_contrat() -> serde_json::Value {
        let champs: Vec<_> = [
            "employee",
            "contract_type",
            "designation",
            "date_of_joining",
            "relieving_date",
            "weekly_working_hours",
            "company",
            "employee_name",
            "remuneration_brute",
            "lieu_travail",
            "motif_cdd",
        ]
        .into_iter()
        .map(|c| json!({"fieldname": c, "fieldtype": "Data"}))
        .collect();
        json!({"docs": [{"name": DOCTYPE_CONTRAT, "fields": champs}]})
    }

    #[test]
    fn contrat_avec_erreurs_jamais_cree() {
        let faux = FauxTransport::new();
        // Structure lue une fois : la vérification reprend celle de la construction.
        faux.repondre_json(200, &meta_contrat());
        faux.repondre_json(200, &json!({"data": {"name": "HR-EMP-00012", "date_of_joining": "2026-12-01"}}));
        let demande = DemandeContrat {
            employe: "HR-EMP-00012".into(),
            type_contrat: "CDD".into(),
            poste: "Animatrice".into(),
            entree: "2026-12-01".into(),
            fin: None,
            heures: Some(35.0),
            repartition: None,
            champs: BTreeMap::new(),
            format: None,
        };
        let erreur =
            creer_un_contrat(&Dokos::new(&faux), BASE, &Regles::par_defaut(), &demande, None, SiDoublon::Reutiliser)
                .expect_err("bloqué");
        assert!(erreur.message.contains("rien n'a été créé"), "{erreur}");
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));
        assert_eq!(faux.requetes().iter().filter(|r| r.chemin.ends_with("getdoctype")).count(), 1);
        assert_eq!(faux.reponses_restantes(), 0);
    }

    fn meta_inscription() -> serde_json::Value {
        let champs: Vec<_> = [
            ("event", "Link"),
            ("first_name", "Data"),
            ("last_name", "Data"),
            ("email", "Data"),
            ("mobile_number", "Data"),
            ("age", "Int"),
        ]
        .into_iter()
        .map(|(c, t)| json!({"fieldname": c, "fieldtype": t, "label": c}))
        .collect();
        json!({"docs": [{"name": DOCTYPE_INSCRIPTION, "fields": champs}]})
    }

    fn demande_evenement() -> DemandeEvenement {
        DemandeEvenement {
            titre: "Atelier poterie".into(),
            debut: "2026-12-14T14:00".into(),
            fin: Some("2026-12-14T17:00".into()),
            journee: false,
            lieu: Some("Grande salle".into()),
            description: None,
            places: Some(12),
            annulation: true,
            questions: vec![QuestionDemande { champ: "age".into(), obligatoire: true }],
        }
    }

    #[test]
    fn apercu_d_evenement_avec_existant() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &meta_inscription());
        faux.repondre_json(200, &json!({"data": [{"name": "EV-1"}]}));
        faux.repondre_json(200, &json!({"data": {"name": "EV-1", "subject": "Atelier poterie", "published": 1}}));
        let apercu = apercu_evenement(&Dokos::new(&faux), BASE, &demande_evenement()).expect("aperçu");
        assert_eq!(apercu.formulaire_route, "inscription-age");
        assert_eq!(apercu.champs.len(), 6);
        assert!(apercu.champs[5].obligatoire);
        let existant = apercu.existant.expect("existant");
        assert_eq!(existant.statut, "publié");
        assert_eq!(existant.lien, "https://dokos.exemple.fr/app/event/EV-1");
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));
    }

    #[test]
    fn evenement_cree_puis_publie() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &meta_inscription());
        faux.repondre_json(200, &json!({"data": []})); // doublon
        faux.repondre_json(200, &json!({"data": []})); // formulaire sur la route
        faux.repondre_json(200, &json!({"data": {"name": "WF-1", "route": "inscription-age"}}));
        faux.repondre_json(200, &json!({"data": {"name": "EV-2", "subject": "Atelier poterie", "published": 0, "registration_form": "WF-1"}}));
        let dokos = Dokos::new(&faux);
        let r = creer_un_evenement(&dokos, BASE, &demande_evenement(), SiDoublon::Reutiliser).expect("création");
        assert!(r.cree && !r.formulaire_reutilise);
        assert_eq!(r.formulaire, "WF-1");
        assert_eq!(r.evenement.statut, "non publié");

        faux.repondre_json(
            200,
            &json!({"data": {"name": "EV-2", "published": 0, "registration_form": "WF-1", "route": "events/ev-2"}}),
        );
        faux.repondre_json(200, &json!({"data": {"name": "WF-1", "route": "inscription-age", "published": 0}}));
        faux.repondre_json(200, &json!({"data": {"name": "WF-1", "route": "inscription-age", "published": 1}}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "EV-2", "published": 1, "route": "events/ev-2", "registration_form": "WF-1"}}),
        );
        let publication = publier_un_evenement(&dokos, BASE, "EV-2").expect("publication");
        assert_eq!(publication.page, "https://dokos.exemple.fr/events/ev-2");
        assert_eq!(publication.inscription, "https://dokos.exemple.fr/inscription-age/new?event=EV-2");
        assert_eq!(faux.requetes().iter().filter(|r| r.methode == Methode::Put).count(), 2);
    }

    #[test]
    fn employe_cree_ou_refuse() {
        let demande = DemandeEmploye {
            prenom: "Marie".into(),
            nom: "Dupont".into(),
            genre: "Femme".into(),
            naissance: "1995-04-02".into(),
            entree: "2026-12-01".into(),
            email: Some("marie@exemple.fr".into()),
            poste: None,
        };
        let faux = FauxTransport::new();
        let sans_societe = creer_un_employe(&Dokos::new(&faux), BASE, &demande, None).expect_err("société");
        assert_eq!(sans_societe.genre, "configuration");
        assert!(faux.requetes().is_empty());

        faux.repondre_json(200, &json!({"data": []}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "HR-EMP-1", "employee_name": "Marie Dupont", "date_of_joining": "2026-12-01"}}),
        );
        let r = creer_un_employe(&Dokos::new(&faux), BASE, &demande, Some("Les Amanins")).expect("employé");
        assert!(r.cree);
        assert_eq!(r.employe.lien, "https://dokos.exemple.fr/app/employee/HR-EMP-1");
        let corps = faux.requetes()[1].corps.clone().expect("corps");
        assert_eq!(corps["company"], "Les Amanins");
        assert_eq!(corps["personal_email"], "marie@exemple.fr");

        let trop_jeune = DemandeEmploye { naissance: "2020-01-01".into(), ..demande };
        let erreur = creer_un_employe(&Dokos::new(&faux), BASE, &trop_jeune, Some("Les Amanins")).expect_err("âge");
        assert_eq!(erreur.genre, "validation");
    }

    #[test]
    fn regles_anciennes() {
        let aujourdhui: Date = "2026-11-03".parse().expect("date");
        let mut regles = Regles::par_defaut();
        assert!(!regles_a_relire(&regles, aujourdhui), "non validée : pas de rappel");
        regles.valide_par = "Juriste".into();
        regles.valide_le = "2026-01-15".into();
        assert!(!regles_a_relire(&regles, aujourdhui));
        regles.valide_le = "2025-01-15".into();
        assert!(regles_a_relire(&regles, aujourdhui));
        regles.valide_le = "bientôt".into();
        assert!(regles_a_relire(&regles, aujourdhui));
    }

    #[test]
    fn diff_compacte() {
        let mut lignes: Vec<LigneDiff> = (0..10).map(|i| LigneDiff::Commune(format!("l{i}"))).collect();
        lignes.insert(8, LigneDiff::Ajoutee("nouveau".into()));
        let vues = compacter_diff(lignes);
        let genres: Vec<&str> = vues.iter().map(|v| v.genre.as_str()).collect();
        assert_eq!(genres, ["saut", "commune", "commune", "ajout", "commune", "commune"]);
        assert_eq!(vues[0].texte, "… 6 lignes inchangées …");
    }
}
