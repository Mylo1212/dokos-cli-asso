//! Logique des commandes de l'interface, testable sans Tauri ni réseau.

use dokos_core::date::Date;
use dokos_core::doublon::{DocumentExistant, SiDoublon, chercher_doublons};
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

#[cfg(test)]
mod tests {
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
}
