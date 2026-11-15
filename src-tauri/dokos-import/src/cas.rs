//! Cas d'usage d'import et colonnes de leur modèle.

use std::fmt;

use dokos_core::vente::TypeVente;

use crate::referentiel::{
    LISTE_ARTICLES, LISTE_CLIENTS, LISTE_GROUPES_ARTICLES, LISTE_GROUPES_CLIENTS, LISTE_TERRITOIRES, LISTE_UNITES,
    Referentiel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cas {
    /// Factures saisies ligne par ligne, prix facultatif.
    Factures,
    /// Copies d'une facture « Template » de Dokos.
    FacturesModele,
    Articles,
    Clients,
}

impl Cas {
    pub const TOUS: [Cas; 4] = [Cas::Factures, Cas::FacturesModele, Cas::Articles, Cas::Clients];

    /// Nom technique, stable (ligne de commande, marque du modèle).
    pub fn nom(self) -> &'static str {
        match self {
            Self::Factures => "factures",
            Self::FacturesModele => "factures-modele",
            Self::Articles => "articles",
            Self::Clients => "clients",
        }
    }

    /// Type de vente d'un cas « à partir d'un modèle ».
    pub fn type_modele(self) -> Option<TypeVente> {
        match self {
            Self::FacturesModele => Some(TypeVente::Facture),
            _ => None,
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Factures => "Factures",
            Self::FacturesModele => "Factures à partir d'un modèle",
            Self::Articles => "Articles",
            Self::Clients => "Clients",
        }
    }

    /// Ce qui est créé dans Dokos, et dans quel état.
    pub fn resultat(self) -> &'static str {
        match self {
            Self::Factures => "factures en brouillon, prix de Dokos ou saisis, taxes calculées par Dokos",
            Self::FacturesModele => {
                "factures en brouillon, copies du modèle choisi (prix, taxes et conditions du modèle)"
            }
            Self::Articles => "articles désactivés, à activer depuis le catalogue",
            Self::Clients => "fiches client (contacts et adresses restent à saisir dans Dokos)",
        }
    }

    /// Profil de connexion Dokos utilisé.
    pub fn profil(self) -> &'static str {
        "ventes"
    }

    /// Nom de la colonne qui regroupe plusieurs lignes en un même document, s'il y en a une.
    pub fn colonne_lot(self) -> Option<&'static str> {
        match self {
            Self::Factures => Some("Lot"),
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
        let client = c(
            "Client",
            true,
            Genre::Texte,
            "Identifiant Dokos (CUST-…) ou nom exact du client, voir la feuille Listes.",
        )
        .liste(LISTE_CLIENTS);
        let article = c("Article", true, Genre::Texte, "Code article ou libellé exact, voir la feuille Listes.")
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
            Self::Articles => vec![
                c("Code", true, Genre::Texte, "Code unique de l'article (sans / % < >)."),
                c("Libellé", true, Genre::Texte, "Nom affiché sur les documents."),
                c("Groupe", true, Genre::Texte, "Groupe d'articles existant, voir la feuille Listes.")
                    .liste(LISTE_GROUPES_ARTICLES),
                c("Unité", true, Genre::Texte, "Unité de mesure existante, voir la feuille Listes.")
                    .liste(LISTE_UNITES),
                c("Vendable", false, Genre::OuiNon, "Oui (par défaut) ou Non."),
                c("Description", false, Genre::Texte, ""),
            ],
            Self::Clients => vec![
                c("Nom", true, Genre::Texte, "Nom du client tel qu'il apparaîtra sur les documents."),
                c("Type", false, Genre::Texte, "Organisation (par défaut) ou Particulier."),
                c("Groupe de clients", false, Genre::Texte, "Facultatif, voir la feuille Listes.")
                    .liste(LISTE_GROUPES_CLIENTS),
                c("Territoire", false, Genre::Texte, "Facultatif, voir la feuille Listes.").liste(LISTE_TERRITOIRES),
                c("E-mail", false, Genre::Texte, ""),
                c("Téléphone", false, Genre::Texte, ""),
            ],
            Self::FacturesModele => {
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
            Self::Articles => vec![
                v("GOUTER"),
                v("Goûter bio"),
                v("Restauration"),
                v("Unité"),
                v("Oui"),
                v("Goûter servi en fin d'atelier"),
            ],
            Self::Clients => {
                vec![v("Mairie de Crest"), v("Organisation"), v(""), v(""), v("mairie@crest.fr"), v("04 75 00 00 00")]
            }
            Self::FacturesModele => {
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
            Self::Factures => {
                "Une ligne par article. Les lignes qui partagent le même Lot forment un seul document : le client et les dates sont lus sur la première ligne du lot et doivent être identiques sur les autres. Le prix unitaire HT est facultatif : laissé vide, Dokos applique ses listes de prix ; les taxes sont toujours calculées par Dokos. « Tarif du client » à Oui enregistre en plus ce prix dans la liste de prix de Dokos, pour cet article et ce client : c'est la seule écriture de l'import qui ne soit pas un brouillon. Chaque document est créé en brouillon, à relire et valider dans Dokos."
            }
            Self::Articles => {
                "Une ligne par article. Les articles sont créés désactivés : vous les activez depuis le catalogue de l'appli ou dans Dokos après relecture. Un code ou un libellé déjà présent n'est pas recréé."
            }
            Self::Clients => {
                "Une ligne par client. Un client de même nom n'est pas recréé. Les contacts et adresses détaillés se saisissent ensuite dans Dokos."
            }
            Self::FacturesModele => {
                "Une ligne par document. Chaque document est une copie du modèle choisi à la génération de ce fichier : prix, remises, taxes, conditions de paiement, conditions générales et champs du modèle sont repris tels quels. Seuls le client, les dates d'arrivée et de départ et les quantités sont lus dans le fichier. Une quantité vide ou à 0 retire l'article ; une ligne sans aucune quantité est refusée. La date de la facture est celle du jour de l'import. Si le modèle est modifié dans Dokos, régénérez ce fichier."
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
    Date,
    OuiNon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colonne {
    pub nom: String,
    pub obligatoire: bool,
    pub genre: Genre,
    pub aide: String,
    /// Nom de la liste de la feuille Listes du classeur qui alimente la liste déroulante.
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
        assert_eq!(Cas::depuis_nom("devis"), None);
    }
}
