//! Cas d'usage d'import et colonnes de leur modèle.

use std::fmt;

use dokos_core::vente::TypeVente;

use crate::referentiel::{
    LISTE_ARTICLES, LISTE_CLIENTS, LISTE_EVENEMENTS, LISTE_GENRES, LISTE_GROUPES_ARTICLES, LISTE_GROUPES_CLIENTS,
    LISTE_POSTES, LISTE_QUESTIONS, LISTE_TERRITOIRES, LISTE_UNITES, Referentiel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cas {
    Factures,
    Commandes,
    Devis,
    Inscriptions,
    Articles,
    Employes,
    Clients,
    Evenements,
    /// Copies d'un devis, d'une commande ou d'une facture « Template ».
    DevisModele,
    CommandesModele,
    FacturesModele,
}

impl Cas {
    pub const TOUS: [Cas; 11] = [
        Cas::Factures,
        Cas::Commandes,
        Cas::Devis,
        Cas::FacturesModele,
        Cas::CommandesModele,
        Cas::DevisModele,
        Cas::Inscriptions,
        Cas::Articles,
        Cas::Employes,
        Cas::Clients,
        Cas::Evenements,
    ];

    /// Nom technique, stable (ligne de commande, marque du modèle).
    pub fn nom(self) -> &'static str {
        match self {
            Self::Factures => "factures",
            Self::Commandes => "commandes",
            Self::Devis => "devis",
            Self::Inscriptions => "inscriptions",
            Self::Articles => "articles",
            Self::Employes => "employes",
            Self::Clients => "clients",
            Self::Evenements => "evenements",
            Self::DevisModele => "devis-modele",
            Self::CommandesModele => "commandes-modele",
            Self::FacturesModele => "factures-modele",
        }
    }

    /// Type de vente d'un cas « à partir d'un modèle ».
    pub fn type_modele(self) -> Option<TypeVente> {
        match self {
            Self::DevisModele => Some(TypeVente::Devis),
            Self::CommandesModele => Some(TypeVente::Commande),
            Self::FacturesModele => Some(TypeVente::Facture),
            _ => None,
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Factures => "Factures",
            Self::Commandes => "Commandes",
            Self::Devis => "Devis",
            Self::Inscriptions => "Inscriptions à un événement",
            Self::Articles => "Articles",
            Self::Employes => "Employés",
            Self::Clients => "Clients",
            Self::Evenements => "Programme d'événements",
            Self::DevisModele => "Devis à partir d'un modèle",
            Self::CommandesModele => "Commandes à partir d'un modèle",
            Self::FacturesModele => "Factures à partir d'un modèle",
        }
    }

    /// Ce qui est créé dans Dokos, et dans quel état.
    pub fn resultat(self) -> &'static str {
        match self {
            Self::Factures => "factures en brouillon, prix de Dokos ou saisis, taxes calculées par Dokos",
            Self::Commandes => "commandes en brouillon, prix de Dokos ou saisis, taxes calculées par Dokos",
            Self::Devis => "devis en brouillon, prix de Dokos ou saisis, taxes calculées par Dokos",
            Self::Inscriptions => "inscriptions à l'événement choisi",
            Self::Articles => "articles désactivés, à activer depuis le catalogue",
            Self::Employes => "fiches employé",
            Self::Clients => "fiches client (contacts et adresses restent à saisir dans Dokos)",
            Self::Evenements => "événements et formulaires d'inscription non publiés",
            Self::DevisModele => "devis en brouillon, copies du modèle choisi (prix, taxes et conditions du modèle)",
            Self::CommandesModele => {
                "commandes en brouillon, copies du modèle choisi (prix, taxes et conditions du modèle)"
            }
            Self::FacturesModele => {
                "factures en brouillon, copies du modèle choisi (prix, taxes et conditions du modèle)"
            }
        }
    }

    /// Profil de connexion Dokos utilisé.
    pub fn profil(self) -> &'static str {
        match self {
            Self::Factures
            | Self::Commandes
            | Self::Devis
            | Self::Articles
            | Self::Clients
            | Self::DevisModele
            | Self::CommandesModele
            | Self::FacturesModele => "ventes",
            Self::Inscriptions | Self::Evenements => "evenements",
            Self::Employes => "rh",
        }
    }

    /// Nom de la colonne qui regroupe plusieurs lignes en un même document, s'il y en a une.
    pub fn colonne_lot(self) -> Option<&'static str> {
        match self {
            Self::Factures | Self::Commandes | Self::Devis => Some("Lot"),
            _ => None,
        }
    }

    pub fn colonnes(self, referentiel: &Referentiel) -> Vec<Colonne> {
        let c = Colonne::new;
        let lot = c(
            "Lot",
            false,
            Genre::Texte,
            "Même valeur sur toutes les lignes d'un même document. Vide : une ligne = un document.",
        );
        let client =
            c("Client", true, Genre::Texte, "Identifiant Dokos (CUST-…) ou nom exact du client, voir l'onglet Listes.")
                .liste(LISTE_CLIENTS);
        let article = c("Article", true, Genre::Texte, "Code article ou libellé exact, voir l'onglet Listes.")
            .liste(LISTE_ARTICLES);
        let quantite = c("Quantité", true, Genre::Nombre, "Nombre supérieur à zéro. Virgule ou point.");
        let prix = c(
            "Prix unitaire HT",
            false,
            Genre::Nombre,
            "Facultatif : prix unitaire hors taxes de la ligne. Vide : prix de la liste de prix de Dokos. Si un prix est saisi dans un document, les règles de prix de Dokos (remises) ne s'appliquent à aucune de ses lignes.",
        );
        let tarif = c(
            "Tarif du client",
            false,
            Genre::OuiNon,
            "Facultatif. Oui : le prix unitaire saisi est aussi enregistré dans la liste de prix de Dokos comme prix de cet article pour ce client seulement (un prix déjà enregistré pour ce client est remplacé) ; les prochains documents de ce client le reprendront. Non ou vide : le prix ne vaut que pour ce document.",
        );
        let reference = c(
            "Référence",
            false,
            Genre::Texte,
            "Facultatif : référence externe reportée dans « N° de commande client ».",
        );
        match self {
            Self::Factures => vec![
                lot,
                client,
                c("Date de facture", true, Genre::Date, "Date au format JJ/MM/AAAA ou AAAA-MM-JJ."),
                c("Échéance", false, Genre::Date, "Facultatif : date d'échéance, après la date de facture."),
                article,
                quantite,
                prix,
                tarif,
                reference,
            ],
            Self::Commandes => vec![
                lot,
                client,
                c("Date de livraison", true, Genre::Date, "Date de la prestation ou de la livraison."),
                article,
                quantite,
                prix,
                tarif,
                c(
                    "Livraison de la ligne",
                    false,
                    Genre::Date,
                    "Facultatif : si cette ligne est livrée à une autre date.",
                ),
                reference,
            ],
            Self::Devis => vec![
                lot,
                client,
                c("Date du devis", true, Genre::Date, "Date au format JJ/MM/AAAA ou AAAA-MM-JJ."),
                c("Valable jusqu'au", false, Genre::Date, "Facultatif : fin de validité du devis."),
                article,
                quantite,
                prix,
                tarif,
            ],
            Self::Inscriptions => {
                let mut colonnes = vec![
                    c("Événement", true, Genre::Texte, "Identifiant de l'événement, voir l'onglet Listes.")
                        .liste(LISTE_EVENEMENTS),
                    c("Prénom", true, Genre::Texte, ""),
                    c("Nom", true, Genre::Texte, ""),
                    c("E-mail", true, Genre::Texte, "Une inscription par e-mail et par événement."),
                    c("Téléphone", false, Genre::Texte, ""),
                ];
                for (champ, libelle) in referentiel.liste(LISTE_QUESTIONS) {
                    colonnes.push(
                        Colonne::new(
                            libelle,
                            false,
                            Genre::Texte,
                            &format!("Réponse à la question « {libelle} » (champ {champ})."),
                        )
                        .champ(champ),
                    );
                }
                colonnes
            }
            Self::Articles => vec![
                c("Code", true, Genre::Texte, "Code unique de l'article (sans / % < >)."),
                c("Libellé", true, Genre::Texte, "Nom affiché sur les documents."),
                c("Groupe", true, Genre::Texte, "Groupe d'articles existant, voir l'onglet Listes.")
                    .liste(LISTE_GROUPES_ARTICLES),
                c("Unité", true, Genre::Texte, "Unité de mesure existante, voir l'onglet Listes.").liste(LISTE_UNITES),
                c("Vendable", false, Genre::OuiNon, "Oui (par défaut) ou Non."),
                c("Description", false, Genre::Texte, ""),
            ],
            Self::Employes => vec![
                c("Prénom", true, Genre::Texte, ""),
                c("Nom", true, Genre::Texte, ""),
                c("Genre", true, Genre::Texte, "Valeur de la liste des genres de Dokos.").liste(LISTE_GENRES),
                c("Date de naissance", true, Genre::Date, "JJ/MM/AAAA."),
                c("Date d'entrée", true, Genre::Date, "JJ/MM/AAAA."),
                c("E-mail", false, Genre::Texte, "E-mail personnel, facultatif."),
                c("Poste", false, Genre::Texte, "Poste existant dans Dokos, facultatif.").liste(LISTE_POSTES),
            ],
            Self::Clients => vec![
                c("Nom", true, Genre::Texte, "Nom du client tel qu'il apparaîtra sur les documents."),
                c("Type", false, Genre::Texte, "Organisation (par défaut) ou Particulier."),
                c("Groupe de clients", false, Genre::Texte, "Facultatif, voir l'onglet Listes.")
                    .liste(LISTE_GROUPES_CLIENTS),
                c("Territoire", false, Genre::Texte, "Facultatif, voir l'onglet Listes.").liste(LISTE_TERRITOIRES),
                c("E-mail", false, Genre::Texte, ""),
                c("Téléphone", false, Genre::Texte, ""),
            ],
            Self::Evenements => vec![
                c("Titre", true, Genre::Texte, ""),
                c("Début", true, Genre::DateHeure, "JJ/MM/AAAA HH:MM."),
                c("Fin", false, Genre::DateHeure, "Facultatif, après le début."),
                c("Journée entière", false, Genre::OuiNon, "Oui ou Non (par défaut)."),
                c("Lieu", false, Genre::Texte, ""),
                c("Places", false, Genre::Entier, "Nombre de places ; vide : sans limite."),
                c(
                    "Questions",
                    false,
                    Genre::Texte,
                    "Codes des questions du formulaire séparés par des virgules ; « ! » après un code le rend obligatoire (age!,allergies). Voir l'onglet Listes.",
                ),
                c("Annulation possible", false, Genre::OuiNon, "Oui ou Non (par défaut)."),
                c("Description", false, Genre::Texte, "Texte de la page publique."),
            ],
            Self::DevisModele | Self::CommandesModele | Self::FacturesModele => {
                let mut colonnes = vec![
                    client,
                    c("Date d'arrivée", true, Genre::Date, "Reportée dans « custom_date_arrivee ». JJ/MM/AAAA."),
                    c(
                        "Date de départ",
                        true,
                        Genre::Date,
                        "Reportée dans « custom_date_depart » ; c'est aussi la date de livraison d'une commande.",
                    ),
                ];
                if let Some(modele) = &referentiel.modele {
                    let designations: Vec<String> = modele.lignes.iter().map(|l| l.designation()).collect();
                    for ligne in &modele.lignes {
                        let designation = ligne.designation();
                        // Un même article sur plusieurs lignes : la ligne départage les colonnes.
                        let nom = if designations.iter().filter(|d| **d == designation).count() > 1 {
                            format!("Qté {designation} — ligne {}", ligne.index + 1)
                        } else {
                            format!("Qté {designation}")
                        };
                        colonnes.push(
                            Colonne::new(
                                &nom,
                                false,
                                Genre::Nombre,
                                &format!(
                                    "Quantité ; vide ou 0 : article retiré. Dans le modèle : {} {} à {:.2} {}.",
                                    nombre_lisible(ligne.quantite),
                                    ligne.unite,
                                    ligne.prix,
                                    modele.devise
                                ),
                            )
                            .champ(&ligne.index.to_string()),
                        );
                    }
                }
                colonnes
            }
        }
    }

    /// Ligne d'exemple du modèle, dans l'ordre des colonnes. Ignorée à l'import.
    pub fn exemple(self, referentiel: &Referentiel) -> Vec<String> {
        let v = |s: &str| s.to_owned();
        match self {
            Self::Factures => vec![
                v("OCT-ECOLE"),
                v("Mairie de Crest"),
                v("31/10/2026"),
                v("30/11/2026"),
                v("REPAS"),
                v("25"),
                v(""),
                v(""),
                v("Bon 2026-118"),
            ],
            Self::Commandes => {
                vec![
                    v("SEJOUR-12"),
                    v("Mairie de Crest"),
                    v("12/11/2026"),
                    v("REPAS"),
                    v("25"),
                    v(""),
                    v(""),
                    v(""),
                    v(""),
                ]
            }
            Self::Devis => {
                vec![v(""), v("Mairie de Crest"), v("03/11/2026"), v("03/12/2026"), v("REPAS"), v("25"), v(""), v("")]
            }
            Self::Inscriptions => {
                let mut exemple =
                    vec![v("EV00042"), v("Marie"), v("Dupont"), v("marie.dupont@exemple.fr"), v("06 00 00 00 00")];
                exemple.extend(referentiel.liste(LISTE_QUESTIONS).iter().map(|_| String::new()));
                exemple
            }
            Self::Articles => vec![
                v("GOUTER"),
                v("Goûter bio"),
                v("Restauration"),
                v("Unité"),
                v("Oui"),
                v("Goûter servi en fin d'atelier"),
            ],
            Self::Employes => vec![
                v("Marie"),
                v("Dupont"),
                v("Femme"),
                v("02/04/1995"),
                v("01/12/2026"),
                v("marie.dupont@exemple.fr"),
                v("Animatrice"),
            ],
            Self::Clients => {
                vec![v("Mairie de Crest"), v("Organisation"), v(""), v(""), v("mairie@crest.fr"), v("04 75 00 00 00")]
            }
            Self::Evenements => vec![
                v("Atelier poterie"),
                v("14/12/2026 14:00"),
                v("14/12/2026 17:00"),
                v("Non"),
                v("Grande salle"),
                v("12"),
                v("age!,allergies"),
                v("Oui"),
                v("Atelier d'initiation, à partir de 8 ans."),
            ],
            Self::DevisModele | Self::CommandesModele | Self::FacturesModele => {
                let mut exemple = vec![v("Mairie de Crest"), v("01/12/2026"), v("03/12/2026")];
                if let Some(modele) = &referentiel.modele {
                    exemple.extend(modele.lignes.iter().map(|l| nombre_lisible(l.quantite)));
                }
                exemple
            }
        }
    }

    /// Paragraphes du mode d'emploi, en plus des règles communes.
    pub fn consignes(self) -> &'static str {
        match self {
            Self::Factures | Self::Commandes | Self::Devis => {
                "Une ligne par article. Les lignes qui partagent le même Lot forment un seul document : le client et les dates sont lus sur la première ligne du lot et doivent être identiques sur les autres. Le prix unitaire HT est facultatif : laissé vide, Dokos applique ses listes de prix ; les taxes sont toujours calculées par Dokos. « Tarif du client » à Oui enregistre en plus ce prix dans la liste de prix de Dokos, pour cet article et ce client : c'est la seule écriture de l'import qui ne soit pas un brouillon. Chaque document est créé en brouillon, à relire et valider dans Dokos."
            }
            Self::Inscriptions => {
                "Une ligne par participant. L'événement est désigné par son identifiant (onglet Listes). Une inscription déjà présente (même e-mail, même événement) n'est pas recréée. Les colonnes de questions correspondent aux champs du formulaire d'inscription de Dokos."
            }
            Self::Articles => {
                "Une ligne par article. Les articles sont créés désactivés : vous les activez depuis le catalogue de l'appli ou dans Dokos après relecture. Un code ou un libellé déjà présent n'est pas recréé."
            }
            Self::Employes => {
                "Une ligne par salarié. La société est celle des Réglages. Un employé de mêmes prénom, nom et date de naissance n'est pas recréé. Les contrats de travail se font ensuite dans l'onglet Contrats, avec leurs vérifications."
            }
            Self::Clients => {
                "Une ligne par client. Un client de même nom n'est pas recréé. Les contacts et adresses détaillés se saisissent ensuite dans Dokos."
            }
            Self::Evenements => {
                "Une ligne par événement. Chaque événement et son formulaire d'inscription sont créés non publiés ; la publication se fait depuis l'onglet Formulaires ou « Mes brouillons ». Les questions sont les codes des champs du formulaire (onglet Listes)."
            }
            Self::DevisModele | Self::CommandesModele | Self::FacturesModele => {
                "Une ligne par document. Chaque document est une copie du modèle choisi à la génération de ce fichier : prix, remises, taxes, conditions de paiement, conditions générales et champs du modèle sont repris tels quels. Seuls le client, les dates d'arrivée et de départ et les quantités sont lus dans le fichier. Une quantité vide ou à 0 retire l'article ; une ligne sans aucune quantité est refusée. La date du document est celle du jour de l'import ; la date de livraison d'une commande est la date de départ. Si le modèle est modifié dans Dokos, régénérez ce fichier."
            }
        }
    }

    pub fn depuis_nom(nom: &str) -> Option<Self> {
        let nom = nom.trim().to_lowercase();
        Self::TOUS.into_iter().find(|c| c.nom() == nom || c.libelle().to_lowercase() == nom)
    }
}

/// `25` plutôt que `25.0`.
pub fn nombre_lisible(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { n.to_string() }
}

impl fmt::Display for Cas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    Texte,
    Nombre,
    Entier,
    Date,
    DateHeure,
    OuiNon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colonne {
    pub nom: String,
    pub obligatoire: bool,
    pub genre: Genre,
    pub aide: String,
    /// Nom de la liste de l'onglet Listes qui alimente la liste déroulante.
    pub liste: Option<&'static str>,
    /// Nom technique associé (question du formulaire), s'il y en a un.
    pub champ: Option<String>,
}

impl Colonne {
    pub fn new(nom: &str, obligatoire: bool, genre: Genre, aide: &str) -> Self {
        Self { nom: nom.to_owned(), obligatoire, genre, aide: aide.to_owned(), liste: None, champ: None }
    }

    fn liste(mut self, liste: &'static str) -> Self {
        self.liste = Some(liste);
        self
    }

    fn champ(mut self, champ: &str) -> Self {
        self.champ = Some(champ.to_owned());
        self
    }

    /// En-tête affiché : le nom, avec `*` si la colonne est obligatoire.
    pub fn entete(&self) -> String {
        if self.obligatoire { format!("{} *", self.nom) } else { self.nom.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colonnes_et_exemples_alignes() {
        let referentiel = Referentiel::default();
        for cas in Cas::TOUS {
            let colonnes = cas.colonnes(&referentiel);
            assert_eq!(colonnes.len(), cas.exemple(&referentiel).len(), "{cas}");
            assert!(colonnes.iter().any(|c| c.obligatoire), "{cas}");
            assert_eq!(Cas::depuis_nom(cas.nom()), Some(cas));
            assert_eq!(Cas::depuis_nom(&cas.libelle().to_uppercase()), Some(cas));
        }
        assert_eq!(Cas::depuis_nom("avoirs"), None);
    }

    #[test]
    fn questions_dynamiques() {
        let mut referentiel = Referentiel::default();
        referentiel
            .ajouter(LISTE_QUESTIONS, vec![("age".into(), "Âge".into()), ("allergies".into(), "Allergies".into())]);
        let colonnes = Cas::Inscriptions.colonnes(&referentiel);
        assert_eq!(colonnes.len(), 7);
        assert_eq!(colonnes[5].champ.as_deref(), Some("age"));
        assert_eq!(colonnes[5].entete(), "Âge");
        assert_eq!(Cas::Inscriptions.exemple(&referentiel).len(), 7);
    }
}
