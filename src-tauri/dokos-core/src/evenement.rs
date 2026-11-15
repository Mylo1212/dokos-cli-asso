//! Événements publics avec inscriptions (appli bookings de Dokos).
//!
//! Deux étapes séparées, comme brouillon puis validation :
//! 1. [`creer_evenement`] : formulaire web (réutilisé si identique) et événement,
//!    tous deux **non publiés** ;
//! 2. [`publier`] : rend l'événement et son formulaire visibles sur le site.

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Dokos, Filtre, Liste};
use crate::date::DateHeure;
use crate::doublon::SiDoublon;
use crate::erreur::DokosError;
use crate::formulaire::{FormulaireObtenu, NouveauFormulaire, obtenir_formulaire, publier_formulaire};
use crate::transport::Transport;
use crate::types::texte_ou_vide;

pub const DOCTYPE_EVENEMENT: &str = "Event";
pub const TITRE_MAX: usize = 140;
pub const PLACES_MAX: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurEvenement {
    TitreVide,
    TitreTropLong(usize),
    FinAvantDebut,
    PlacesInvalides(u32),
}

impl fmt::Display for ErreurEvenement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TitreVide => write!(f, "titre de l'événement manquant"),
            Self::TitreTropLong(n) => write!(f, "titre trop long : {n} caractères (maximum {TITRE_MAX})"),
            Self::FinAvantDebut => write!(f, "la fin de l'événement est avant son début"),
            Self::PlacesInvalides(n) => write!(f, "nombre de places invalide : {n} (entre 1 et {PLACES_MAX})"),
        }
    }
}

impl std::error::Error for ErreurEvenement {}

#[derive(Debug, Clone, PartialEq)]
pub struct NouvelEvenement {
    titre: String,
    debut: DateHeure,
    fin: Option<DateHeure>,
    journee_entiere: bool,
    lieu: Option<String>,
    description: Option<String>,
    places: Option<u32>,
    annulation_autorisee: bool,
}

impl NouvelEvenement {
    pub fn new(titre: &str, debut: DateHeure, fin: Option<DateHeure>) -> Result<Self, ErreurEvenement> {
        let titre = titre.split_whitespace().collect::<Vec<_>>().join(" ");
        if titre.is_empty() {
            return Err(ErreurEvenement::TitreVide);
        }
        let longueur = titre.chars().count();
        if longueur > TITRE_MAX {
            return Err(ErreurEvenement::TitreTropLong(longueur));
        }
        if fin.is_some_and(|fin| fin <= debut) {
            return Err(ErreurEvenement::FinAvantDebut);
        }
        Ok(Self {
            titre,
            debut,
            fin,
            journee_entiere: false,
            lieu: None,
            description: None,
            places: None,
            annulation_autorisee: false,
        })
    }

    pub fn journee_entiere(mut self, oui: bool) -> Self {
        self.journee_entiere = oui;
        self
    }

    pub fn avec_lieu(mut self, lieu: Option<&str>) -> Self {
        self.lieu = non_vide(lieu);
        self
    }

    /// Texte brut : les sauts de ligne sont conservés, le HTML est échappé.
    pub fn avec_description(mut self, description: Option<&str>) -> Self {
        self.description = non_vide(description);
        self
    }

    /// `None` : pas de limite.
    pub fn avec_places(mut self, places: Option<u32>) -> Result<Self, ErreurEvenement> {
        if let Some(n) = places
            && !(1..=PLACES_MAX).contains(&n)
        {
            return Err(ErreurEvenement::PlacesInvalides(n));
        }
        self.places = places;
        Ok(self)
    }

    pub fn avec_annulation(mut self, autorisee: bool) -> Self {
        self.annulation_autorisee = autorisee;
        self
    }

    pub fn titre(&self) -> &str {
        &self.titre
    }

    pub fn debut(&self) -> DateHeure {
        self.debut
    }

    pub fn fin(&self) -> Option<DateHeure> {
        self.fin
    }

    pub fn places(&self) -> Option<u32> {
        self.places
    }

    /// Corps JSON envoyé à Dokos. L'événement est créé **non publié**.
    pub fn corps(&self, formulaire: &str) -> Value {
        let mut corps = json!({
            "subject": self.titre,
            "starts_on": self.debut.to_string(),
            "all_day": u8::from(self.journee_entiere),
            "event_type": "Public",
            "event_category": "Event",
            "status": "Open",
            "published": 0,
            "allow_registrations": 1,
            "registration_form": formulaire,
            "max_number_of_registrations": self.places.unwrap_or(0),
            "allow_cancellations": u8::from(self.annulation_autorisee),
        });
        if let Some(fin) = self.fin {
            corps["ends_on"] = json!(fin.to_string());
        }
        if let Some(lieu) = &self.lieu {
            corps["location"] = json!(lieu);
        }
        if let Some(description) = &self.description {
            corps["description"] = json!(texte_en_html(description));
        }
        corps
    }
}

fn non_vide(texte: Option<&str>) -> Option<String> {
    texte.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Paragraphes `<p>` séparés par les lignes vides, `<br>` pour les sauts simples.
fn texte_en_html(texte: &str) -> String {
    let echappe = texte.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    echappe
        .replace("\r\n", "\n")
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", p.replace('\n', "<br>")))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvenementExistant {
    #[serde(rename = "name")]
    pub id: String,
    #[serde(rename = "subject", default, deserialize_with = "texte_ou_vide")]
    pub titre: String,
    #[serde(rename = "starts_on", default, deserialize_with = "texte_ou_vide")]
    pub debut: String,
    #[serde(default)]
    pub published: u8,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub route: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub registration_form: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreationEvenement {
    Cree {
        evenement: EvenementExistant,
        formulaire: FormulaireObtenu,
    },
    /// Rien n'a été écrit : un événement de même titre au même horaire existait.
    DejaExistant(EvenementExistant),
}

/// Événement non annulé de même titre commençant au même moment. Lecture seule.
pub fn chercher_evenement_identique<T: Transport>(
    dokos: &Dokos<T>,
    evenement: &NouvelEvenement,
) -> Result<Option<EvenementExistant>, DokosError> {
    #[derive(Deserialize)]
    struct Nom {
        name: String,
    }
    let liste = Liste::new(&["name"])
        .filtre(Filtre::egal("subject", evenement.titre.as_str()))
        .filtre(Filtre::egal("starts_on", evenement.debut.to_string()))
        .filtre(Filtre::different("status", "Cancelled"))
        .limite(1);
    let noms: Vec<Nom> = dokos.lister(DOCTYPE_EVENEMENT, &liste)?;
    match noms.first() {
        Some(nom) => Ok(Some(dokos.lire(DOCTYPE_EVENEMENT, &nom.name)?)),
        None => Ok(None),
    }
}

/// Crée le formulaire (ou réutilise un identique) puis l'événement, non publiés.
/// Le contrôle de doublon passe en premier : rien n'est écrit s'il échoue.
pub fn creer_evenement<T: Transport>(
    dokos: &Dokos<T>,
    evenement: &NouvelEvenement,
    formulaire: &NouveauFormulaire,
    si_doublon: SiDoublon,
) -> Result<CreationEvenement, DokosError> {
    if si_doublon == SiDoublon::Reutiliser
        && let Some(existant) = chercher_evenement_identique(dokos, evenement)?
    {
        return Ok(CreationEvenement::DejaExistant(existant));
    }
    let formulaire = obtenir_formulaire(dokos, formulaire)?;
    let cree: EvenementExistant = dokos.creer(DOCTYPE_EVENEMENT, evenement.corps(&formulaire.nom))?;
    if cree.published != 0 {
        return Err(DokosError::ReponseInvalide(format!("l'événement {} a été publié à la création", cree.id)));
    }
    Ok(CreationEvenement::Cree { evenement: cree, formulaire })
}

/// Adresses publiques, relatives à la racine du site Dokos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    pub evenement: String,
    pub route_evenement: String,
    pub route_inscription: String,
}

/// Publie le formulaire d'inscription puis l'événement.
pub fn publier<T: Transport>(dokos: &Dokos<T>, id: &str) -> Result<Publication, DokosError> {
    let evenement: EvenementExistant = dokos.lire(DOCTYPE_EVENEMENT, id)?;
    if evenement.registration_form.is_empty() {
        return Err(DokosError::Validation(format!("l'événement {id} n'a pas de formulaire d'inscription")));
    }
    let route_formulaire = publier_formulaire(dokos, &evenement.registration_form)?;
    let publie: EvenementExistant = if evenement.published == 1 {
        evenement
    } else {
        dokos.modifier(DOCTYPE_EVENEMENT, id, json!({"published": 1}))?
    };
    Ok(Publication {
        evenement: publie.id.clone(),
        route_evenement: publie.route.trim_start_matches('/').to_owned(),
        route_inscription: format!("{}/new?event={}", route_formulaire.trim_start_matches('/'), publie.id),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::formulaire::Question;
    use crate::formulaire::tests::meta_inscription;
    use crate::transport::Methode;
    use crate::transport::faux::FauxTransport;

    fn moment(texte: &str) -> DateHeure {
        texte.parse().expect("date-heure de test")
    }

    fn atelier() -> NouvelEvenement {
        NouvelEvenement::new("  Atelier   poterie ", moment("2026-12-14 14:00"), Some(moment("2026-12-14 17:00")))
            .expect("événement")
            .avec_lieu(Some("Grande salle"))
            .avec_description(Some("Venez <modeler> !\n\nMatériel fourni.\nTablier conseillé."))
            .avec_places(Some(12))
            .expect("places")
    }

    #[test]
    fn evenements_invalides() {
        let debut = moment("2026-12-14 14:00");
        assert_eq!(NouvelEvenement::new(" ", debut, None), Err(ErreurEvenement::TitreVide));
        assert_eq!(NouvelEvenement::new("A", debut, Some(debut)), Err(ErreurEvenement::FinAvantDebut));
        assert_eq!(
            NouvelEvenement::new(&"x".repeat(TITRE_MAX + 1), debut, None),
            Err(ErreurEvenement::TitreTropLong(TITRE_MAX + 1))
        );
        let evenement = NouvelEvenement::new("A", debut, None).expect("événement");
        assert_eq!(evenement.clone().avec_places(Some(0)), Err(ErreurEvenement::PlacesInvalides(0)));
        assert!(evenement.avec_places(None).is_ok());
    }

    #[test]
    fn corps_non_publie() {
        assert_eq!(
            atelier().corps("form-1"),
            json!({
                "subject": "Atelier poterie",
                "starts_on": "2026-12-14 14:00:00",
                "ends_on": "2026-12-14 17:00:00",
                "all_day": 0,
                "event_type": "Public",
                "event_category": "Event",
                "status": "Open",
                "published": 0,
                "allow_registrations": 1,
                "registration_form": "form-1",
                "max_number_of_registrations": 12,
                "allow_cancellations": 0,
                "location": "Grande salle",
                "description": "<p>Venez &lt;modeler&gt; !</p><p>Matériel fourni.<br>Tablier conseillé.</p>",
            })
        );
    }

    #[test]
    fn creation_complete() {
        let formulaire =
            NouveauFormulaire::new(&meta_inscription(), &[Question { champ: "age".into(), obligatoire: true }])
                .expect("formulaire");
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": []})); // pas d'événement identique
        faux.repondre_json(200, &json!({"data": []})); // route libre
        faux.repondre_json(200, &json!({"data": {"name": "form-age", "route": "inscription-age"}}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "EV00042", "subject": "Atelier poterie", "published": 0,
                                                 "registration_form": "form-age"}}),
        );
        let creation =
            creer_evenement(&Dokos::new(&faux), &atelier(), &formulaire, SiDoublon::Reutiliser).expect("création");
        let CreationEvenement::Cree { evenement, formulaire } = creation else { panic!("{creation:?}") };
        assert_eq!(evenement.id, "EV00042");
        assert_eq!(formulaire.nom, "form-age");

        let requetes = faux.requetes();
        assert_eq!(
            requetes[0].parametre("filters"),
            Some(
                r#"[["subject","=","Atelier poterie"],["starts_on","=","2026-12-14 14:00:00"],["status","!=","Cancelled"]]"#
            )
        );
        assert_eq!(requetes[3].methode, Methode::Post);
        assert_eq!(requetes[3].chemin, "/api/resource/Event");
        assert_eq!(requetes[3].corps.as_ref().map(|c| c["registration_form"].clone()), Some(json!("form-age")));
    }

    #[test]
    fn evenement_identique_rien_ecrit() {
        let formulaire = NouveauFormulaire::new(&meta_inscription(), &[]).expect("formulaire");
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": [{"name": "EV00042"}]}));
        faux.repondre_json(200, &json!({"data": {"name": "EV00042", "subject": "Atelier poterie", "published": 1}}));
        let creation =
            creer_evenement(&Dokos::new(&faux), &atelier(), &formulaire, SiDoublon::Reutiliser).expect("doublon");
        assert!(matches!(creation, CreationEvenement::DejaExistant(e) if e.id == "EV00042"));
        assert!(faux.requetes().iter().all(|r| r.methode == Methode::Get));
    }

    #[test]
    fn publication() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"name": "EV00042", "published": 0, "registration_form": "form-age"}}));
        faux.repondre_json(200, &json!({"data": {"name": "form-age", "route": "inscription-age", "published": 0}}));
        faux.repondre_json(200, &json!({"data": {"name": "form-age", "route": "inscription-age", "published": 1}}));
        faux.repondre_json(
            200,
            &json!({"data": {"name": "EV00042", "published": 1, "route": "events/EV00042-atelier-poterie",
                             "registration_form": "form-age"}}),
        );
        let publication = publier(&Dokos::new(&faux), "EV00042").expect("publication");
        assert_eq!(
            publication,
            Publication {
                evenement: "EV00042".into(),
                route_evenement: "events/EV00042-atelier-poterie".into(),
                route_inscription: "inscription-age/new?event=EV00042".into(),
            }
        );
        let ecritures: Vec<_> =
            faux.requetes().into_iter().filter(|r| r.methode != Methode::Get).map(|r| r.chemin).collect();
        assert_eq!(ecritures, ["/api/resource/Web%20Form/form-age", "/api/resource/Event/EV00042"]);
    }

    #[test]
    fn publication_sans_formulaire_refusee() {
        let faux = FauxTransport::new();
        faux.repondre_json(200, &json!({"data": {"name": "EV1", "published": 0, "registration_form": null}}));
        assert!(matches!(publier(&Dokos::new(&faux), "EV1"), Err(DokosError::Validation(_))));
        assert_eq!(faux.requetes().len(), 1);
    }
}
