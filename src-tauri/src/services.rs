//! Logique des commandes de l'interface, testable sans Tauri ni réseau.

use dokos_core::date::{Date, DateHeure};
use dokos_core::doublon::{DocumentExistant, SiDoublon, chercher_doublons};
use dokos_core::evenement::{
    CreationEvenement, DOCTYPE_EVENEMENT, EvenementExistant, NouvelEvenement, chercher_evenement_identique,
    creer_evenement, publier,
};
use dokos_core::formulaire::{DOCTYPE_INSCRIPTION, NouveauFormulaire, Question, questions_possibles};
use dokos_core::meta::{Champ, lire_meta};
use dokos_core::recherche::{chercher_article_dans, chercher_client, lister_articles, lister_clients};
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

/// Tous les clients actifs (lecture complète, pour les listes déroulantes).
pub fn lister_les_clients<T: Transport>(dokos: &Dokos<T>) -> Resultat<Vec<ClientVue>> {
    let clients = lister_clients(dokos).map_err(texte)?;
    Ok(clients.into_iter().map(|c| ClientVue { id: c.id, nom: c.nom }).collect())
}

/// Tous les articles vendables, limités à un groupe (sous-groupes compris) si `groupe` est donné.
pub fn lister_les_articles<T: Transport>(dokos: &Dokos<T>, groupe: Option<&str>) -> Resultat<Vec<ArticleVue>> {
    let articles = lister_articles(dokos, groupe).map_err(texte)?;
    Ok(articles.into_iter().map(vue_article).collect())
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
        .map(|l| {
            LigneVente::new(&l.article, l.quantite, date_option(l.livraison.as_deref())?)
                .and_then(|ligne| ligne.avec_prix(l.prix))
                .map_err(texte)
        })
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
            lignes: vec![LigneDemande { article: "REPAS".into(), quantite: 25.0, livraison: None, prix: None }],
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
    fn prix_saisi_sur_une_ligne() {
        let mut demande = demande_vente("facture");
        demande.lignes[0].prix = Some(12.5);
        demande.lignes.push(LigneDemande { article: "SALLE-G".into(), quantite: 1.0, livraison: None, prix: None });
        let corps = document_vente(&demande, None).expect("facture").corps();
        assert_eq!(corps["items"][0]["rate"], 12.5);
        assert!(corps["items"][1].get("rate").is_none(), "sans prix saisi, Dokos garde sa liste de prix");
        assert_eq!(corps["ignore_pricing_rule"], 1);

        demande.lignes[0].prix = Some(-1.0);
        assert!(document_vente(&demande, None).is_err());
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
}
