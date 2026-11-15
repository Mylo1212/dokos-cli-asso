//! `dokos devis|commande|facture brouillon` : résolution du client et des articles, puis document.

use dokos_core::date::Date;
use dokos_core::doublon::{DocumentExistant, chercher_doublons};
use dokos_core::recherche::{Candidat, Correspondance, trouver_article, trouver_client};
use dokos_core::types::{Article, Client};
use dokos_core::vente::{Creation, LigneVente, NouveauDocumentVente, SiDoublon, TypeVente, creer_brouillon};
use dokos_core::{Dokos, Transport};

use crate::erreur::Erreur;
use crate::saisie::{self, LigneSaisie};

/// Document prêt à envoyer, avec ce qui a été résolu pour l'afficher.
#[derive(Debug)]
pub struct Preparation {
    pub client: Client,
    pub articles: Vec<Article>,
    pub document: NouveauDocumentVente,
}

/// Résout le client et chaque article dans Dokos (lecture seule).
/// Rien d'ambigu ou d'introuvable n'est choisi à la place de l'utilisateur.
pub fn preparer<T: Transport>(
    dokos: &Dokos<T>,
    type_vente: TypeVente,
    client: &str,
    date: Date,
    echeance: Option<Date>,
    lignes: &[LigneSaisie],
    societe: Option<&str>,
) -> Result<Preparation, Erreur> {
    let client = unique("client", client, trouver_client(dokos, client)?)?;
    let mut articles = Vec::with_capacity(lignes.len());
    let mut lignes_vente = Vec::with_capacity(lignes.len());
    for saisie in lignes {
        let article = unique("article", &saisie.article, trouver_article(dokos, &saisie.article)?)?;
        lignes_vente.push(LigneVente::new(&article.code, saisie.quantite, saisie.livraison)?.avec_prix(saisie.prix)?);
        articles.push(article);
    }
    let document =
        NouveauDocumentVente::new(type_vente, &client.id, date, echeance, lignes_vente)?.avec_societe(societe);
    Ok(Preparation { client, articles, document })
}

fn unique<C: Candidat>(genre: &str, texte: &str, correspondance: Correspondance<C>) -> Result<C, Erreur> {
    match correspondance {
        Correspondance::Unique(c) => Ok(c),
        Correspondance::Aucune => Err(Erreur::Resolution(format!("aucun {genre} actif ne correspond à « {texte} »"))),
        Correspondance::Plusieurs(candidats) => {
            let mut message = format!("plusieurs {genre}s correspondent à « {texte} » ; précisez avec l'identifiant :");
            for c in &candidats {
                message.push_str(&format!("\n  {}  {}", c.identifiant(), c.nom()));
            }
            Err(Erreur::Resolution(message))
        }
    }
}

pub struct Demande<'a> {
    pub type_vente: TypeVente,
    pub client: &'a str,
    pub date: Date,
    pub echeance: Option<Date>,
    pub lignes: &'a [LigneSaisie],
    pub simulation: bool,
    pub si_doublon: SiDoublon,
}

pub fn brouillon<T: Transport>(dokos: &Dokos<T>, societe: Option<&str>, demande: &Demande<'_>) -> Result<(), Erreur> {
    let type_vente = demande.type_vente;
    let preparation =
        preparer(dokos, type_vente, demande.client, demande.date, demande.echeance, demande.lignes, societe)?;
    let document = &preparation.document;

    println!("Client     : {} ({})", preparation.client.nom, preparation.client.id);
    println!("{:<11}: {}", majuscule(type_vente.libelle_date()), demande.date);
    if let (Some(echeance), Some(libelle)) = (demande.echeance, type_vente.libelle_echeance()) {
        println!("{:<11}: {echeance}", majuscule(libelle));
    }
    println!("Lignes :");
    for (ligne, article) in document.lignes().iter().zip(&preparation.articles) {
        let unite = if article.unite.is_empty() { String::new() } else { format!(" {}", article.unite) };
        let livraison = match type_vente {
            TypeVente::Commande => format!(" — {}", ligne.livraison().unwrap_or(demande.date)),
            TypeVente::Devis | TypeVente::Facture => String::new(),
        };
        let prix = ligne.prix().map_or(String::new(), |p| format!(" à {p:.2} HT"));
        println!(
            "  {:>6}{unite} × {} ({}){prix}{livraison}",
            saisie::quantite(ligne.quantite()),
            article.libelle,
            article.code
        );
    }
    if document.lignes().iter().any(|l| l.prix().is_some()) {
        println!(
            "Prix saisis : les règles de prix de Dokos (remises) ne s'appliquent à aucune ligne ; \
             les lignes sans prix gardent celui de la liste de prix. Taxes : calculées par Dokos."
        );
    } else {
        println!("Prix et taxes : calculés par Dokos.");
    }

    if demande.simulation {
        let doublons = chercher_doublons(dokos, document)?;
        if let Some(existant) = doublons.identiques.first()
            && demande.si_doublon == SiDoublon::Reutiliser
        {
            println!("\n{} identique déjà dans Dokos : {}", majuscule(type_vente.libelle()), decrire(existant));
            println!("Simulation : sans --simulation, rien ne serait créé.");
            return Ok(());
        }
        afficher_autres(type_vente, doublons.identiques.iter().chain(&doublons.similaires));
        let corps = serde_json::to_string_pretty(&document.corps()).map_err(|e| Erreur::Saisie(e.to_string()))?;
        println!("\nSimulation : rien n'a été écrit dans Dokos. Corps qui serait envoyé :\n{corps}");
        return Ok(());
    }

    match creer_brouillon(dokos, document, demande.si_doublon)? {
        Creation::DejaExistant(existant) => {
            println!("\n{} identique déjà dans Dokos : {}", majuscule(type_vente.libelle()), decrire(&existant));
            println!("Rien n'a été créé. Pour en créer un autre quand même : --autoriser-doublon");
        }
        Creation::Cree { document, autres } => {
            afficher_autres(type_vente, autres.iter());
            println!(
                "\n{} créé(e) en brouillon : {} (total {:.2} {})",
                majuscule(type_vente.libelle()),
                document.id,
                document.total,
                document.devise
            );
        }
    }
    Ok(())
}

fn majuscule(texte: &str) -> String {
    let mut caracteres = texte.chars();
    caracteres.next().map(|c| c.to_uppercase().chain(caracteres).collect()).unwrap_or_default()
}

fn decrire(document: &DocumentExistant) -> String {
    let mut description = format!("{} ({}", document.id, document.statut());
    if !document.date_document.is_empty() {
        description.push_str(&format!(", du {}", document.date_document));
    }
    description.push_str(&format!(", total {:.2} {})", document.total, document.devise));
    description
}

fn afficher_autres<'a>(type_vente: TypeVente, autres: impl Iterator<Item = &'a DocumentExistant>) {
    let autres: Vec<_> = autres.collect();
    if autres.is_empty() {
        return;
    }
    println!("\nAttention, autres {}s de ce client aux mêmes dates :", type_vente.libelle());
    for document in autres {
        println!("  {}", decrire(document));
    }
}

#[cfg(test)]
mod tests {
    use dokos_core::transport::Methode;
    use dokos_core::transport::faux::FauxTransport;
    use serde_json::json;

    use super::*;

    fn saisie(article: &str, quantite: f64) -> LigneSaisie {
        LigneSaisie { article: article.into(), quantite, livraison: None, prix: None }
    }

    fn jour() -> Date {
        "2026-11-12".parse().expect("date")
    }

    fn preparer_commande<T: Transport>(
        dokos: &Dokos<T>,
        client: &str,
        lignes: &[LigneSaisie],
        societe: Option<&str>,
    ) -> Result<Preparation, Erreur> {
        preparer(dokos, TypeVente::Commande, client, jour(), None, lignes, societe)
    }

    #[test]
    fn preparation_complete_sans_ecriture() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [
                {"name": "CUST-1", "customer_name": "CCAS de Crest"},
                {"name": "CUST-2", "customer_name": "Mairie de Crest"},
            ]}),
        );
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "REPAS", "item_name": "Repas complet", "stock_uom": "Unité"}]}),
        );
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "SALLE-G", "item_name": "Grande salle", "stock_uom": "Jour"}]}),
        );

        let lignes = [saisie("repas", 25.0), saisie("grande salle", 1.0)];
        let preparation = preparer_commande(&Dokos::new(&faux), "Mairie de Crest", &lignes, Some("Les Amanins"))
            .expect("préparation");

        assert_eq!(preparation.client.id, "CUST-2");
        assert_eq!(preparation.articles.len(), 2);
        let corps = preparation.document.corps();
        assert_eq!(corps["customer"], "CUST-2");
        assert_eq!(corps["company"], "Les Amanins");
        assert_eq!(corps["items"][1]["item_code"], "SALLE-G");

        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 3);
        assert!(requetes.iter().all(|r| r.methode == Methode::Get));
    }

    #[test]
    fn prix_saisi_envoye_a_dokos() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        faux.repondre_json(200, &json!({"data": [{"name": "REPAS", "item_name": "Repas"}]}));
        faux.repondre_json(200, &json!({"data": [{"name": "SALLE-G", "item_name": "Grande salle"}]}));
        let lignes = [LigneSaisie { prix: Some(12.5), ..saisie("repas", 25.0) }, saisie("grande salle", 1.0)];
        let preparation =
            preparer(&Dokos::new(&faux), TypeVente::Facture, "mairie", jour(), None, &lignes, None).expect("facture");
        let corps = preparation.document.corps();
        assert_eq!(corps["items"][0]["rate"], 12.5);
        assert!(corps["items"][1].get("rate").is_none());
        assert_eq!(corps["ignore_pricing_rule"], 1);
    }

    #[test]
    fn devis_avec_echeance() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        faux.repondre_json(200, &json!({"data": [{"name": "REPAS", "item_name": "Repas"}]}));
        let echeance = "2026-12-12".parse().ok();
        let preparation =
            preparer(&Dokos::new(&faux), TypeVente::Devis, "mairie", jour(), echeance, &[saisie("repas", 2.0)], None)
                .expect("devis");
        assert_eq!(preparation.document.corps()["valid_till"], "2026-12-12");
    }

    #[test]
    fn client_ambigu_arrete_tout() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [
                {"name": "CUST-1", "customer_name": "CCAS de Crest"},
                {"name": "CUST-2", "customer_name": "Mairie de Crest"},
            ]}),
        );
        let erreur = preparer_commande(&Dokos::new(&faux), "crest", &[saisie("repas", 1.0)], None).expect_err("ambigu");
        let message = erreur.to_string();
        assert!(message.contains("CUST-1  CCAS de Crest"), "{message}");
        assert!(message.contains("CUST-2  Mairie de Crest"), "{message}");
        // Les articles ne sont même pas cherchés.
        assert_eq!(faux.requetes().len(), 1);
    }

    #[test]
    fn article_introuvable() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie de Crest"}]}));
        faux.repondre_json(200, &json!({"data": []}));
        let erreur =
            preparer_commande(&Dokos::new(&faux), "mairie", &[saisie("licorne", 1.0)], None).expect_err("introuvable");
        assert_eq!(erreur.to_string(), "aucun article actif ne correspond à « licorne »");
    }

    #[test]
    fn quantite_invalide_apres_resolution() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "CUST-2", "customer_name": "Mairie"}]}));
        faux.repondre_json(200, &json!({"data": [{"name": "REPAS", "item_name": "Repas"}]}));
        let resultat = preparer_commande(&Dokos::new(&faux), "mairie", &[saisie("repas", 0.0)], None);
        assert!(matches!(resultat, Err(Erreur::Saisie(_))));
    }

    #[test]
    fn majuscules() {
        assert_eq!(majuscule("échéance"), "Échéance");
        assert_eq!(majuscule(""), "");
    }
}
