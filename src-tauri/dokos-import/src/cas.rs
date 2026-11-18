//! Cas d'usage d'import et colonnes de leur modèle.

use std::fmt;

use crate::referentiel::{
    LISTE_ARTICLES, LISTE_CLIENTS, LISTE_FACTURES, LISTE_GROUPES_ARTICLES, LISTE_GROUPES_CLIENTS, LISTE_MODES_PAIEMENT,
    LISTE_TERRITOIRES, LISTE_UNITES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cas {
    /// Factures saisies ligne par ligne, prix facultatif.
    Factures,
    /// Écritures de paiement (brouillons) en règlement de factures validées.
    Paiements,
    Articles,
    Clients,
}

impl Cas {
    pub const TOUS: [Cas; 4] = [Cas::Factures, Cas::Paiements, Cas::Articles, Cas::Clients];

    /// Nom technique, stable (ligne de commande, marque du modèle).
    pub fn nom(self) -> &'static str {
        match self {
            Self::Factures => "factures",
            Self::Paiements => "paiements",
            Self::Articles => "articles",
            Self::Clients => "clients",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Factures => "Factures",
            Self::Paiements => "Paiements",
            Self::Articles => "Articles",
            Self::Clients => "Clients",
        }
    }

    /// Ce qui est créé dans Dokos, et dans quel état.
    pub fn resultat(self) -> &'static str {
        match self {
            Self::Factures => "factures en brouillon, prix de Dokos ou saisis, taxes calculées par Dokos",
            Self::Paiements => {
                "écritures de paiement en brouillon, rattachées à la facture réglée (à valider dans Dokos)"
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

    pub fn colonnes(self) -> Vec<Colonne> {
        let c = Colonne::new;
        let lot = c(
            "Lot",
            false,
            Genre::Texte,
            "Facultatif. Même valeur sur toutes les lignes d'une même facture, même éloignées. Vide : les lignes qui se suivent avec le même client, la même date et la même échéance forment une seule facture ; mettez un lot pour séparer deux factures du même client à la même date.",
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
            Self::Paiements => vec![
                c(
                    "Lot",
                    false,
                    Genre::Texte,
                    "Facultatif. Même valeur sur toutes les lignes d'un même paiement, même éloignées. Vide : les lignes qui se suivent avec le même client, la même date, le même mode de paiement et la même référence forment un seul paiement ; mettez un lot pour séparer deux paiements identiques.",
                ),
                c(
                    "Facture",
                    true,
                    Genre::Texte,
                    "Numéro Dokos d'une facture validée dont le solde n'est pas nul, voir la feuille Listes.",
                )
                .liste(LISTE_FACTURES),
                c("Date du paiement", true, Genre::Date, "Date d'encaissement au format JJ/MM/AAAA ou AAAA-MM-JJ."),
                c(
                    "Montant",
                    false,
                    Genre::Nombre,
                    "Facultatif : montant du paiement affecté à cette facture. Vide : tout son reste dû. Jamais plus que le reste dû.",
                ),
                c(
                    "Mode de paiement",
                    true,
                    Genre::Texte,
                    "Mode de paiement actif de Dokos, voir la feuille Listes. Il détermine le compte d'encaissement.",
                )
                .liste(LISTE_MODES_PAIEMENT),
                c(
                    "Référence",
                    false,
                    Genre::Texte,
                    "Numéro de chèque, de virement… Obligatoire si le mode de paiement encaisse sur un compte bancaire.",
                ),
            ],
        }
    }

    /// Ligne d'exemple du modèle, dans l'ordre des colonnes. Ignorée à l'import.
    pub fn exemple(self) -> Vec<String> {
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
            Self::Paiements => {
                vec![v(""), v("ACC-SINV-2026-00012"), v("05/11/2026"), v(""), v("Virement"), v("VIR-2026-118")]
            }
        }
    }

    /// Paragraphes du mode d'emploi, en plus des règles communes.
    pub fn consignes(self) -> &'static str {
        match self {
            Self::Factures => {
                "Une ligne par article ; une facture peut avoir autant d'articles que de lignes. Sans Lot, les lignes qui se suivent avec le même client, la même date de facture et la même échéance (et la même Référence, si elle est renseignée) forment une seule facture. Avec un Lot, les lignes qui le partagent forment une seule facture, même éloignées : le client et les dates sont lus sur la première ligne du lot et doivent être identiques sur les autres. Pour deux factures du même client à la même date, indiquez deux Lots différents. Le prix unitaire HT est facultatif : laissé vide, Dokos applique ses listes de prix ; les taxes sont toujours calculées par Dokos. « Tarif du client » à Oui enregistre en plus ce prix dans la liste de prix de Dokos, pour cet article et ce client : c'est la seule écriture de l'import qui ne soit pas un brouillon. Chaque document est créé en brouillon, à relire et valider dans Dokos."
            }
            Self::Articles => {
                "Une ligne par article. Les articles sont créés désactivés : vous les activez depuis le catalogue de l'appli ou dans Dokos après relecture. Un code ou un libellé déjà présent n'est pas recréé."
            }
            Self::Clients => {
                "Une ligne par client. Un client de même nom n'est pas recréé. Les contacts et adresses détaillés se saisissent ensuite dans Dokos."
            }
            Self::Paiements => {
                "Une ligne par facture réglée, avec le montant qui lui est affecté ; un paiement peut régler plusieurs factures d'un même client. Sans Lot, les lignes qui se suivent avec le même client, la même date, le même mode de paiement et la même référence forment un seul paiement, réparti entre leurs factures. Avec un Lot, les lignes qui le partagent forment un seul paiement, même éloignées : date, mode, référence et client doivent être les mêmes que sur la première ligne du lot. Pour deux paiements identiques qui se suivent, indiquez deux Lots différents. Dokos prépare l'écriture de paiement (client, comptes, lien vers les factures) ; le fichier donne la date, les montants, le mode de paiement et la référence. Le montant vide règle tout le reste dû de la facture ; les montants d'une même facture sur tout le fichier (règlement en plusieurs fois) ne peuvent pas dépasser son reste dû. La référence est obligatoire quand le mode de paiement encaisse sur un compte bancaire. Chaque paiement est créé en brouillon : la facture n'est soldée qu'une fois le paiement validé dans Dokos, après relecture. Le journal comptable est attribué par Dokos selon ses règles."
            }
        }
    }

    pub fn depuis_nom(nom: &str) -> Option<Self> {
        let nom = nom.trim().to_lowercase();
        Self::TOUS.into_iter().find(|c| c.nom() == nom || c.libelle().to_lowercase() == nom)
    }
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
}

impl Colonne {
    pub fn new(nom: &str, obligatoire: bool, genre: Genre, aide: &str) -> Self {
        Self { nom: nom.to_owned(), obligatoire, genre, aide: aide.to_owned(), liste: None }
    }

    fn liste(mut self, liste: &'static str) -> Self {
        self.liste = Some(liste);
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
        for cas in Cas::TOUS {
            let colonnes = cas.colonnes();
            assert_eq!(colonnes.len(), cas.exemple().len(), "{cas}");
            assert!(colonnes.iter().any(|c| c.obligatoire), "{cas}");
            assert_eq!(Cas::depuis_nom(cas.nom()), Some(cas));
            assert_eq!(Cas::depuis_nom(&cas.libelle().to_uppercase()), Some(cas));
        }
        assert_eq!(Cas::depuis_nom("avoirs"), None);
        assert_eq!(Cas::depuis_nom("devis"), None);
        assert_eq!(Cas::depuis_nom("factures-modele"), None, "cas retiré");
        assert_eq!(Cas::depuis_nom("Paiements"), Some(Cas::Paiements));
    }
}
