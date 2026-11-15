//! Client de l'API `/api/resource` de Frappe.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::erreur::DokosError;
use crate::meta::Meta;
use crate::parallele;
use crate::transport::{Reponse, Requete, Transport};

/// Durée de vie des réponses mémorisées (réglages de vente). La structure
/// des DocTypes a sa propre mémoire, plus courte : [`DUREE_STRUCTURE`].
const DUREE_MEMOIRE: Duration = Duration::from_secs(300);

/// Nombre de résultats par défaut d'une liste.
const LIMITE_DEFAUT: u32 = 20;
/// Plafond d'une liste. Frappe interprète 0 comme « tout », d'où le minimum de 1.
const LIMITE_MAX: u32 = 500;
/// Durée pendant laquelle la structure d'un DocType lue dans Dokos est
/// reprise sans nouvelle requête. Courte : un champ ajouté dans Dokos doit
/// être vu vite, mais « Vérifier » puis « Créer » ne relisent pas tout.
const DUREE_STRUCTURE: Duration = Duration::from_secs(120);

/// Condition `[champ, opérateur, valeur]` au format des filtres Frappe,
/// ou `[table enfant, champ, opérateur, valeur]` pour une table enfant.
#[derive(Debug, Clone, PartialEq)]
pub struct Filtre {
    table: Option<String>,
    champ: String,
    operateur: &'static str,
    valeur: Value,
}

impl Filtre {
    fn new(champ: &str, operateur: &'static str, valeur: Value) -> Self {
        Self { table: None, champ: champ.to_owned(), operateur, valeur }
    }

    pub fn egal(champ: &str, valeur: impl Into<Value>) -> Self {
        Self::new(champ, "=", valeur.into())
    }

    pub fn different(champ: &str, valeur: impl Into<Value>) -> Self {
        Self::new(champ, "!=", valeur.into())
    }

    pub fn superieur(champ: &str, valeur: impl Into<Value>) -> Self {
        Self::new(champ, ">", valeur.into())
    }

    pub fn inferieur(champ: &str, valeur: impl Into<Value>) -> Self {
        Self::new(champ, "<", valeur.into())
    }

    /// `champ >= valeur`.
    pub fn a_partir_de(champ: &str, valeur: impl Into<Value>) -> Self {
        Self::new(champ, ">=", valeur.into())
    }

    /// `champ <= valeur`.
    pub fn jusqua(champ: &str, valeur: impl Into<Value>) -> Self {
        Self::new(champ, "<=", valeur.into())
    }

    /// `debut <= champ <= fin`, bornes incluses.
    pub fn entre(champ: &str, debut: impl Into<Value>, fin: impl Into<Value>) -> Self {
        Self::new(champ, "between", json!([debut.into(), fin.into()]))
    }

    /// `champ IN (valeurs)`.
    pub fn parmi(champ: &str, valeurs: &[&str]) -> Self {
        Self::new(champ, "in", json!(valeurs))
    }

    /// Champ vide ou nul (`is not set`).
    pub fn non_renseigne(champ: &str) -> Self {
        Self::new(champ, "is", json!("not set"))
    }

    /// Lien vers un DocType arborescent : le nœud donné et toute sa descendance.
    pub fn descendant_de(champ: &str, noeud: &str) -> Self {
        Self::new(champ, "descendants of (inclusive)", json!(noeud))
    }

    /// `champ LIKE %texte%`. Les `%` du texte sont retirés pour qu'il reste littéral.
    pub fn contient(champ: &str, texte: &str) -> Self {
        let motif = format!("%{}%", texte.replace('%', ""));
        Self::new(champ, "like", Value::String(motif))
    }

    /// `champ NOT LIKE %texte%` (un champ vide passe : Frappe compare `ifnull(champ, '')`).
    pub fn ne_contient_pas(champ: &str, texte: &str) -> Self {
        let motif = format!("%{}%", texte.replace('%', ""));
        Self::new(champ, "not like", Value::String(motif))
    }

    /// Condition sur un champ d'une table enfant (`Sales Order Item`…).
    pub fn enfant(table: &str, champ: &str, valeur: impl Into<Value>) -> Self {
        Self { table: Some(table.to_owned()), champ: champ.to_owned(), operateur: "=", valeur: valeur.into() }
    }

    pub(crate) fn en_json(&self) -> Value {
        match &self.table {
            Some(table) => json!([table, self.champ, self.operateur, self.valeur]),
            None => json!([self.champ, self.operateur, self.valeur]),
        }
    }
}

/// Paramètres d'un `GET /api/resource/<DocType>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Liste {
    champs: Vec<String>,
    filtres: Vec<Filtre>,
    ou_filtres: Vec<Filtre>,
    ordre: Option<String>,
    limite: u32,
    debut: u32,
}

impl Liste {
    pub fn new(champs: &[&str]) -> Self {
        Self {
            champs: champs.iter().map(|&c| c.to_owned()).collect(),
            filtres: Vec::new(),
            ou_filtres: Vec::new(),
            ordre: None,
            limite: LIMITE_DEFAUT,
            debut: 0,
        }
    }

    /// Condition obligatoire (ET).
    pub fn filtre(mut self, filtre: Filtre) -> Self {
        self.filtres.push(filtre);
        self
    }

    /// Condition alternative (OU entre toutes les conditions ajoutées ainsi).
    pub fn ou(mut self, filtre: Filtre) -> Self {
        self.ou_filtres.push(filtre);
        self
    }

    /// Tri SQL, par exemple `customer_name asc`.
    pub fn ordre(mut self, ordre: &str) -> Self {
        self.ordre = Some(ordre.to_owned());
        self
    }

    pub fn limite(mut self, limite: u32) -> Self {
        self.limite = limite.clamp(1, LIMITE_MAX);
        self
    }

    /// Saute les `debut` premiers résultats (pagination).
    pub fn debut(mut self, debut: u32) -> Self {
        self.debut = debut;
        self
    }

    /// Nombre maximal de résultats d'une page.
    pub fn limite_max() -> u32 {
        LIMITE_MAX
    }

    fn parametres(&self) -> Vec<(String, String)> {
        let liste_json = |filtres: &[Filtre]| Value::Array(filtres.iter().map(Filtre::en_json).collect()).to_string();
        let mut parametres = vec![("fields".to_owned(), json!(self.champs).to_string())];
        if !self.filtres.is_empty() {
            parametres.push(("filters".to_owned(), liste_json(&self.filtres)));
        }
        if !self.ou_filtres.is_empty() {
            parametres.push(("or_filters".to_owned(), liste_json(&self.ou_filtres)));
        }
        if let Some(ordre) = &self.ordre {
            parametres.push(("order_by".to_owned(), ordre.clone()));
        }
        parametres.push(("limit_page_length".to_owned(), self.limite.to_string()));
        if self.debut > 0 {
            parametres.push(("limit_start".to_owned(), self.debut.to_string()));
        }
        parametres
    }
}

/// Enveloppe `{"data": …}` des réponses `/api/resource`.
#[derive(Deserialize)]
struct Donnees<D> {
    data: D,
}

/// Enveloppe `{"message": …}` des réponses `/api/method`.
#[derive(Deserialize)]
struct Message<D> {
    message: D,
}

/// Corps des réponses mémorisées et date de lecture, par requête.
type Reponses = HashMap<String, (Instant, Vec<u8>)>;

/// Ce que le client retient d'une requête à l'autre, tant qu'il vit : qui
/// est connecté, et la structure des DocTypes déjà lus.
#[derive(Debug, Default)]
struct Memoire {
    utilisateur: Mutex<Option<String>>,
    structures: Mutex<BTreeMap<String, (Instant, Meta)>>,
}

#[derive(Debug)]
pub struct Dokos<T> {
    transport: T,
    /// Réponses des lectures « mémorisables », par requête. `None` : mémoire
    /// désactivée (par défaut, et toujours dans les tests).
    reponses: Option<Mutex<Reponses>>,
    memoire: Memoire,
}

impl<T: Transport> Dokos<T> {
    pub fn new(transport: T) -> Self {
        Self { transport, reponses: None, memoire: Memoire::default() }
    }

    /// Garde quelques minutes les lectures qui ne changent presque jamais
    /// (réglages de vente) au lieu de les redemander à
    /// chaque action. Seules les réponses réussies sont gardées.
    pub fn avec_memoire(mut self) -> Self {
        self.reponses = Some(Mutex::new(HashMap::new()));
        self
    }

    /// Applique `f` à chaque élément, avec quelques requêtes simultanées si le
    /// transport le permet (sinon dans l'ordre). Résultats dans l'ordre des
    /// éléments ; la première erreur, dans cet ordre, est renvoyée.
    pub fn pour_chacun<E, R, Er, F>(&self, elements: &[E], f: F) -> Result<Vec<R>, Er>
    where
        E: Sync,
        R: Send,
        Er: Send,
        F: Fn(&E) -> Result<R, Er> + Sync,
    {
        parallele::pour_chacun(self.transport.parallele(), elements, f)
    }

    /// Calcule deux lectures indépendantes en même temps si le transport le permet.
    pub fn joindre<A: Send, B: Send>(&self, a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B) {
        parallele::joindre(self.transport.parallele(), a, b)
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Identifiant de l'utilisateur API, demandé à Dokos à chaque appel.
    /// Sert à tester la connexion.
    pub fn utilisateur_connecte(&self) -> Result<String, DokosError> {
        let reponse: Message<String> = self.executer(&Requete::get("/api/method/frappe.auth.get_logged_user"))?;
        if reponse.message == "Guest" {
            return Err(DokosError::Authentification);
        }
        if let Ok(mut connu) = self.memoire.utilisateur.lock() {
            *connu = Some(reponse.message.clone());
        }
        Ok(reponse.message)
    }

    /// Identifiant de l'utilisateur API, demandé à Dokos une seule fois : il
    /// ne change pas tant que la clé reste la même, et ce client ne lui survit pas.
    pub fn utilisateur(&self) -> Result<String, DokosError> {
        if let Ok(connu) = self.memoire.utilisateur.lock()
            && let Some(utilisateur) = connu.as_ref()
        {
            return Ok(utilisateur.clone());
        }
        self.utilisateur_connecte()
    }

    /// Structure d'un DocType encore en mémoire, si elle a été lue récemment.
    pub(crate) fn structure_en_memoire(&self, doctype: &str) -> Option<Meta> {
        let structures = self.memoire.structures.lock().ok()?;
        let (lue, meta) = structures.get(doctype)?;
        (lue.elapsed() < DUREE_STRUCTURE).then(|| meta.clone())
    }

    pub(crate) fn retenir_structure(&self, meta: &Meta) {
        if let Ok(mut structures) = self.memoire.structures.lock() {
            structures.insert(meta.doctype.clone(), (Instant::now(), meta.clone()));
        }
    }

    /// Oublie les structures de DocTypes gardées en mémoire : la prochaine
    /// lecture interroge Dokos. À appeler quand l'utilisateur demande
    /// explicitement un état frais (rechargement, « Vérifier »).
    pub fn oublier_structures(&self) {
        if let Ok(mut structures) = self.memoire.structures.lock() {
            structures.clear();
        }
    }

    pub fn lister<D: DeserializeOwned>(&self, doctype: &str, liste: &Liste) -> Result<Vec<D>, DokosError> {
        let mut requete = Requete::get(chemin_ressource(doctype, None)?);
        requete.parametres = liste.parametres();
        let reponse: Donnees<Vec<D>> = self.executer(&requete)?;
        Ok(reponse.data)
    }

    /// Toutes les pages d'une liste, par pages de [`LIMITE_MAX`], dans la limite
    /// de `plafond` éléments (la limite et le début de `liste` sont ignorés).
    pub fn lister_tout<D: DeserializeOwned>(
        &self,
        doctype: &str,
        liste: &Liste,
        plafond: usize,
    ) -> Result<Vec<D>, DokosError> {
        let mut tout: Vec<D> = Vec::new();
        while tout.len() < plafond {
            let debut = u32::try_from(tout.len()).unwrap_or(u32::MAX);
            let page: Vec<D> = self.lister(doctype, &liste.clone().limite(LIMITE_MAX).debut(debut))?;
            let complete = page.len() as u32 == LIMITE_MAX;
            tout.extend(page);
            if !complete {
                break;
            }
        }
        tout.truncate(plafond);
        Ok(tout)
    }

    pub fn lire<D: DeserializeOwned>(&self, doctype: &str, nom: &str) -> Result<D, DokosError> {
        let reponse: Donnees<D> = self.executer(&Requete::get(chemin_ressource(doctype, Some(nom))?))?;
        Ok(reponse.data)
    }

    /// Comme [`Self::lire`], réponse gardée quelques minutes si la mémoire est
    /// active (voir [`Self::avec_memoire`]). Pour les réglages, pas pour les documents.
    pub fn lire_memorise<D: DeserializeOwned>(&self, doctype: &str, nom: &str) -> Result<D, DokosError> {
        let reponse: Donnees<D> = self.executer_memorise(&Requete::get(chemin_ressource(doctype, Some(nom))?))?;
        Ok(reponse.data)
    }

    /// Plusieurs listes en un lot ([`Transport::envoyer_lot`]) : les résultats
    /// dans l'ordre des demandes, ou la première erreur.
    pub fn lister_lot<D: DeserializeOwned>(&self, demandes: &[(&str, &Liste)]) -> Result<Vec<Vec<D>>, DokosError> {
        let requetes = demandes
            .iter()
            .map(|(doctype, liste)| {
                let mut requete = Requete::get(chemin_ressource(doctype, None)?);
                requete.parametres = liste.parametres();
                Ok(requete)
            })
            .collect::<Result<Vec<_>, DokosError>>()?;
        let reponses = self.transport.envoyer_lot(&requetes)?;
        reponses.iter().map(|r| decoder::<Donnees<Vec<D>>>(r).map(|d| d.data)).collect()
    }

    /// Plusieurs documents du même DocType en un lot : dans l'ordre des noms,
    /// ou la première erreur.
    pub fn lire_lot<D: DeserializeOwned>(&self, doctype: &str, noms: &[&str]) -> Result<Vec<D>, DokosError> {
        let requetes = noms
            .iter()
            .map(|nom| Ok(Requete::get(chemin_ressource(doctype, Some(nom))?)))
            .collect::<Result<Vec<_>, DokosError>>()?;
        let reponses = self.transport.envoyer_lot(&requetes)?;
        reponses.iter().map(|r| decoder::<Donnees<D>>(r).map(|d| d.data)).collect()
    }

    /// `POST` générique, réservé aux opérations typées de ce crate.
    pub(crate) fn creer<D: DeserializeOwned>(&self, doctype: &str, corps: Value) -> Result<D, DokosError> {
        let reponse: Donnees<D> = self.executer(&Requete::post(chemin_ressource(doctype, None)?, corps))?;
        Ok(reponse.data)
    }

    /// `PUT` générique (modification partielle), réservé aux opérations typées de ce crate.
    pub(crate) fn modifier<D: DeserializeOwned>(
        &self,
        doctype: &str,
        nom: &str,
        corps: Value,
    ) -> Result<D, DokosError> {
        let reponse: Donnees<D> = self.executer(&Requete::put(chemin_ressource(doctype, Some(nom))?, corps))?;
        Ok(reponse.data)
    }

    /// Supprime un document **en brouillon**, après l'avoir relu : un document
    /// validé n'est jamais supprimé. Sert aux tests réels, pour effacer ce
    /// qu'ils ont créé.
    pub fn supprimer_brouillon(&self, doctype: &str, nom: &str) -> Result<(), DokosError> {
        #[derive(Deserialize)]
        struct Etat {
            #[serde(default)]
            docstatus: u8,
        }
        let etat: Etat = self.lire(doctype, nom)?;
        if etat.docstatus != 0 {
            return Err(DokosError::Validation(format!("{doctype} {nom} n'est pas en brouillon : non supprimé")));
        }
        let reponse = self.transport.envoyer(&Requete::delete(chemin_ressource(doctype, Some(nom))?))?;
        if !(200..300).contains(&reponse.statut) {
            return Err(DokosError::depuis_reponse(reponse.statut, &reponse.texte()));
        }
        Ok(())
    }

    /// Appel d'une méthode `/api/method/…` en `POST` (corps JSON), réponse `message` décodée.
    pub(crate) fn methode_post<D: DeserializeOwned>(&self, methode: &str, corps: Value) -> Result<D, DokosError> {
        let reponse: Message<D> = self.executer(&Requete::post(format!("/api/method/{methode}"), corps).longue())?;
        Ok(reponse.message)
    }

    /// Appel en lecture d'une méthode `/api/method/…`, réponse brute décodée.
    /// Jamais mémorisée : la structure des DocTypes a sa propre mémoire
    /// ([`Self::structure_en_memoire`]), que l'utilisateur peut vider.
    pub(crate) fn methode<D: DeserializeOwned>(
        &self,
        methode: &str,
        parametres: &[(&str, &str)],
    ) -> Result<D, DokosError> {
        self.executer(&Requete::get(format!("/api/method/{methode}")).avec_parametres(parametres))
    }

    /// Appel en lecture d'une méthode qui renvoie un fichier (PDF…).
    pub(crate) fn telecharger(&self, methode: &str, parametres: &[(&str, &str)]) -> Result<Vec<u8>, DokosError> {
        let requete = Requete::get(format!("/api/method/{methode}")).avec_parametres(parametres).longue();
        let reponse = self.transport.envoyer(&requete)?;
        if !(200..300).contains(&reponse.statut) {
            return Err(DokosError::depuis_reponse(reponse.statut, &reponse.texte()));
        }
        Ok(reponse.corps)
    }

    fn executer<D: DeserializeOwned>(&self, requete: &Requete) -> Result<D, DokosError> {
        decoder(&self.transport.envoyer(requete)?)
    }

    /// Lecture (`GET`) dont la réponse réussie est gardée [`DUREE_MEMOIRE`].
    fn executer_memorise<D: DeserializeOwned>(&self, requete: &Requete) -> Result<D, DokosError> {
        let Some(memoire) = &self.reponses else { return self.executer(requete) };
        let cle = format!("{}?{:?}", requete.chemin, requete.parametres);
        if let Ok(m) = memoire.lock()
            && let Some((quand, corps)) = m.get(&cle)
            && quand.elapsed() < DUREE_MEMOIRE
        {
            tracing::debug!(chemin = %requete.chemin, "réponse mémorisée");
            return serde_json::from_slice(corps).map_err(|e| DokosError::ReponseInvalide(e.to_string()));
        }
        let reponse = self.transport.envoyer(requete)?;
        let valeur = decoder(&reponse)?;
        if let Ok(mut m) = memoire.lock() {
            m.retain(|_, (quand, _)| quand.elapsed() < DUREE_MEMOIRE);
            m.insert(cle, (Instant::now(), reponse.corps));
        }
        Ok(valeur)
    }
}

/// Corps JSON d'une réponse réussie, ou l'erreur que Dokos y décrit.
fn decoder<D: DeserializeOwned>(reponse: &Reponse) -> Result<D, DokosError> {
    if !(200..300).contains(&reponse.statut) {
        return Err(DokosError::depuis_reponse(reponse.statut, &reponse.texte()));
    }
    serde_json::from_slice(&reponse.corps).map_err(|e| DokosError::ReponseInvalide(e.to_string()))
}

/// Noms (`name`) des documents d'un DocType de référence : genres, postes, types de contrat…
pub fn lister_noms<T: Transport>(
    dokos: &Dokos<T>,
    doctype: &str,
    filtres: Vec<Filtre>,
    limite: u32,
) -> Result<Vec<String>, DokosError> {
    #[derive(Deserialize)]
    struct Nom {
        name: String,
    }
    let mut liste = Liste::new(&["name"]).ordre("name asc").limite(limite);
    for filtre in filtres {
        liste = liste.filtre(filtre);
    }
    let noms: Vec<Nom> = dokos.lister(doctype, &liste)?;
    Ok(noms.into_iter().map(|n| n.name).collect())
}

fn chemin_ressource(doctype: &str, nom: Option<&str>) -> Result<String, DokosError> {
    if doctype.trim().is_empty() {
        return Err(DokosError::Configuration("DocType vide".to_owned()));
    }
    let mut chemin = format!("/api/resource/{}", encoder_segment(doctype));
    if let Some(nom) = nom {
        if nom.trim().is_empty() {
            return Err(DokosError::Configuration("nom de document vide".to_owned()));
        }
        chemin.push('/');
        chemin.push_str(&encoder_segment(nom));
    }
    Ok(chemin)
}

/// Encode un segment de chemin ou d'URL : tout sauf les caractères non réservés de la RFC 3986.
pub fn encoder_segment(segment: &str) -> String {
    let mut encode = String::with_capacity(segment.len());
    for octet in segment.bytes() {
        if octet.is_ascii_alphanumeric() || matches!(octet, b'-' | b'.' | b'_' | b'~') {
            encode.push(char::from(octet));
        } else {
            encode.push_str(&format!("%{octet:02X}"));
        }
    }
    encode
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Nom {
        name: String,
    }

    #[test]
    fn encodage_des_segments() {
        assert_eq!(encoder_segment("Sales Order"), "Sales%20Order");
        assert_eq!(encoder_segment("SAL-ORD-2026-00001"), "SAL-ORD-2026-00001");
        assert_eq!(encoder_segment("A/B?c#d"), "A%2FB%3Fc%23d");
        assert_eq!(encoder_segment("Crêpe"), "Cr%C3%AApe");
        assert_eq!(encoder_segment(".."), "..");
    }

    #[test]
    fn chemins() {
        assert_eq!(chemin_ressource("Customer", None).as_deref(), Ok("/api/resource/Customer"));
        assert_eq!(chemin_ressource("Sales Order", Some("SO 1")).as_deref(), Ok("/api/resource/Sales%20Order/SO%201"));
        assert!(chemin_ressource(" ", None).is_err());
        assert!(chemin_ressource("Customer", Some("")).is_err());
    }

    #[test]
    fn parametres_de_liste() {
        let liste = Liste::new(&["name", "customer_name"])
            .filtre(Filtre::egal("disabled", 0))
            .filtre(Filtre::different("docstatus", 2))
            .filtre(Filtre::entre("delivery_date", "2026-11-12", "2026-11-13"))
            .ou(Filtre::contient("name", "crest"))
            .ou(Filtre::contient("customer_name", "100%crest"))
            .ordre("customer_name asc")
            .limite(10);
        let p = liste.parametres();
        let valeur = |nom: &str| p.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str());
        assert_eq!(valeur("fields"), Some(r#"["name","customer_name"]"#));
        assert_eq!(
            valeur("filters"),
            Some(
                r#"[["disabled","=",0],["docstatus","!=",2],["delivery_date","between",["2026-11-12","2026-11-13"]]]"#
            )
        );
        assert_eq!(valeur("or_filters"), Some(r#"[["name","like","%crest%"],["customer_name","like","%100crest%"]]"#));
        let enfant = Liste::new(&["name"])
            .filtre(Filtre::enfant("Sales Order Item", "prevdoc_docname", "SAL-QTN-1"))
            .filtre(Filtre::parmi("status", &["Draft", "To Deliver"]))
            .filtre(Filtre::non_renseigne("relieving_date"))
            .filtre(Filtre::descendant_de("item_group", "Restauration"))
            .filtre(Filtre::superieur("outstanding_amount", 0))
            .filtre(Filtre::inferieur("due_date", "2026-11-03"))
            .parametres();
        let filtres = enfant.iter().find(|(n, _)| n == "filters").map(|(_, v)| v.as_str()).unwrap_or_default();
        assert_eq!(
            filtres,
            r#"[["Sales Order Item","prevdoc_docname","=","SAL-QTN-1"],["status","in",["Draft","To Deliver"]],["relieving_date","is","not set"],["item_group","descendants of (inclusive)","Restauration"],["outstanding_amount",">",0],["due_date","<","2026-11-03"]]"#
        );
        assert_eq!(valeur("order_by"), Some("customer_name asc"));
        assert_eq!(valeur("limit_page_length"), Some("10"));
    }

    #[test]
    fn liste_minimale_et_limites() {
        let p = Liste::new(&["name"]).limite(0).parametres();
        assert!(p.iter().all(|(n, _)| n != "filters" && n != "or_filters" && n != "order_by"));
        assert!(p.contains(&("limit_page_length".to_owned(), "1".to_owned())));
        let p = Liste::new(&["name"]).limite(100_000).parametres();
        assert!(p.contains(&("limit_page_length".to_owned(), LIMITE_MAX.to_string())));
    }

    #[test]
    fn lister_decode_data() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "A"}, {"name": "B"}]}));
        let dokos = Dokos::new(&faux);
        let noms: Vec<Nom> = dokos.lister("Customer", &Liste::new(&["name"])).expect("liste");
        assert_eq!(noms, vec![Nom { name: "A".into() }, Nom { name: "B".into() }]);
        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 1);
        assert_eq!(requetes[0].methode, Methode::Get);
        assert_eq!(requetes[0].chemin, "/api/resource/Customer");
        assert!(requetes[0].corps.is_none());
    }

    #[test]
    fn lister_tout_parcourt_les_pages() {
        let page = |debut: u32, n: u32| json!({"data": (debut..debut + n).map(|i| json!({"name": format!("C{i}")})).collect::<Vec<_>>()});
        let faux = FauxTransport::new();
        faux.repondre_json(200, &page(0, LIMITE_MAX));
        faux.repondre_json(200, &page(LIMITE_MAX, 3));
        let noms: Vec<Nom> = Dokos::new(&faux).lister_tout("Customer", &Liste::new(&["name"]), 10_000).expect("liste");
        assert_eq!(noms.len(), LIMITE_MAX as usize + 3);
        let requetes = faux.requetes();
        assert_eq!(requetes.len(), 2);
        assert_eq!(requetes[0].parametre("limit_start"), None);
        assert_eq!(requetes[1].parametre("limit_start"), Some(LIMITE_MAX.to_string().as_str()));
        assert_eq!(requetes[1].parametre("limit_page_length"), Some(LIMITE_MAX.to_string().as_str()));
    }

    #[test]
    fn lister_tout_respecte_le_plafond() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": (0..LIMITE_MAX).map(|i| json!({"name": format!("C{i}")})).collect::<Vec<_>>()}),
        );
        let noms: Vec<Nom> = Dokos::new(&faux).lister_tout("Customer", &Liste::new(&["name"]), 10).expect("liste");
        assert_eq!(noms.len(), 10);
        assert_eq!(faux.requetes().len(), 1);
    }

    #[test]
    fn lire_un_document() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"name": "CUST-1", "autre": 1}}));
        let nom: Nom = Dokos::new(&faux).lire("Customer", "CUST-1").expect("lecture");
        assert_eq!(nom.name, "CUST-1");
        assert_eq!(faux.requetes()[0].chemin, "/api/resource/Customer/CUST-1");
    }

    #[test]
    fn erreur_http_convertie() {
        let faux = FauxTransport::new();
        faux.repondre(
            403,
            r#"{"exc_type": "PermissionError", "_server_messages": "[\"{\\\"message\\\": \\\"Accès refusé\\\"}\"]"}"#,
        );
        let resultat: Result<Vec<Nom>, _> = Dokos::new(&faux).lister("Customer", &Liste::new(&["name"]));
        assert_eq!(resultat, Err(DokosError::Permission("Accès refusé".into())));
    }

    #[test]
    fn reponse_illisible() {
        let faux = FauxTransport::new();
        faux.repondre(200, "<html>page de connexion</html>");
        let resultat: Result<Vec<Nom>, _> = Dokos::new(&faux).lister("Customer", &Liste::new(&["name"]));
        assert!(matches!(resultat, Err(DokosError::ReponseInvalide(_))));
    }

    #[test]
    fn erreur_reseau_propagee() {
        let faux = FauxTransport::new();
        faux.echouer(DokosError::Reseau("délai dépassé".into()));
        assert_eq!(Dokos::new(&faux).utilisateur_connecte(), Err(DokosError::Reseau("délai dépassé".into())));
    }

    #[test]
    fn suppression_de_brouillon_seulement() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"name": "Q-1", "docstatus": 0}}));
        faux.repondre_json(202, &json!({"message": "ok"}));
        faux.repondre_json(200, &json!({"data": {"name": "Q-2", "docstatus": 1}}));
        let dokos = Dokos::new(&faux);
        dokos.supprimer_brouillon("Quotation", "Q-1").expect("suppression");
        assert!(matches!(dokos.supprimer_brouillon("Quotation", "Q-2"), Err(DokosError::Validation(_))));
        let requetes = faux.requetes();
        assert_eq!(requetes[1].methode, Methode::Delete);
        assert_eq!(requetes[1].chemin, "/api/resource/Quotation/Q-1");
        assert_eq!(requetes.len(), 3);
    }

    #[test]
    fn utilisateur_connecte() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"message": "api@exemple.fr"}));
        faux.repondre_json(200, &json!({"message": "Guest"}));
        let dokos = Dokos::new(&faux);
        assert_eq!(dokos.utilisateur_connecte().as_deref(), Ok("api@exemple.fr"));
        assert_eq!(dokos.utilisateur_connecte(), Err(DokosError::Authentification));
        assert_eq!(faux.requetes()[0].chemin, "/api/method/frappe.auth.get_logged_user");
    }

    #[test]
    fn memoire_des_lectures_reussies() {
        let faux = FauxTransport::new();
        faux.repondre(500, "{}")
            .repondre_json(200, &json!({"data": {"name": "Réglages"}}))
            .repondre_json(200, &json!({"data": {"name": "Autre"}}));
        let dokos = Dokos::new(&faux).avec_memoire();
        // Une erreur n'est jamais gardée : la lecture suivante repart vers Dokos.
        assert!(dokos.lire_memorise::<Nom>("Selling Settings", "Selling Settings").is_err());
        for _ in 0..3 {
            let nom: Nom = dokos.lire_memorise("Selling Settings", "Selling Settings").expect("réglages");
            assert_eq!(nom.name, "Réglages");
        }
        // Un autre document n'est pas confondu avec le premier.
        let autre: Nom = dokos.lire_memorise("Selling Settings", "Autre").expect("autre");
        assert_eq!(autre.name, "Autre");
        assert_eq!(faux.requetes().len(), 3);
    }

    #[test]
    fn sans_memoire_par_defaut() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"name": "A"}})).repondre_json(200, &json!({"data": {"name": "B"}}));
        let dokos = Dokos::new(&faux);
        let a: Nom = dokos.lire_memorise("Selling Settings", "Selling Settings").expect("a");
        let b: Nom = dokos.lire_memorise("Selling Settings", "Selling Settings").expect("b");
        assert_eq!((a.name.as_str(), b.name.as_str()), ("A", "B"));
    }
}
