//! Grand livre : écritures comptables (`GL Entry`) d'un journal comptable et
//! d'une période, mises au format d'import d'un logiciel de comptabilité.
//! **Lecture seule.**
//!
//! Les écritures sont lues directement, une ligne par écriture, plutôt que par
//! le rapport « Grand Livre » de Dokos : pas de lignes « Ouverture », « Total »
//! ni « Fermeture » à écarter, pas de libellés traduits à reconnaître. Les
//! réglages du rapport qu'on reproduit : écritures annulées et à-nouveaux
//! exclus, valeurs nettes dans les comptes de tiers.
//!
//! Format produit (une ligne par écriture) :
//!
//! | Colonne | Contenu |
//! |---|---|
//! | DATE | date de comptabilisation |
//! | JOURNAL | [`JournalFichier`] (`VEN` ou `REC`, au choix), quel que soit le journal lu |
//! | GENERAL | chiffres en tête du compte (`411 - Clients - SLA` → 411) |
//! | AUXILIAIRE | [`AUXILIAIRE_CLIENTS`] si le compte est exactement [`COMPTE_CLIENTS`] |
//! | DEBIT, CREDIT | montants (0 si vide) |
//! | LIBELLE | tiers |
//! | REFINTERNE | numéro de la pièce (document de référence) |

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer};

use crate::client::{Dokos, Filtre, Liste};
use crate::date::Date;
use crate::erreur::DokosError;
use crate::export::ErreurExport;
use crate::transport::Transport;
use crate::types::texte_ou_vide;

pub const DOCTYPE_JOURNAL: &str = "Accounting Journal";
pub const DOCTYPE_ECRITURE: &str = "GL Entry";
pub const DOCTYPE_COMPTE: &str = "Account";

/// Nombre maximal d'écritures d'un export : au-delà, il est refusé (jamais tronqué).
pub const ECRITURES_MAX: usize = 100_000;
/// Journaux comptables proposés au plus.
const JOURNAUX_MAX: usize = 500;
/// Comptes lus au plus (plan comptable).
const COMPTES_MAX: usize = 20_000;

/// Valeur de la colonne JOURNAL du fichier final : le journal du logiciel de
/// comptabilité qui reçoit l'import, au choix de l'utilisateur, quel que soit
/// le journal lu dans Dokos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JournalFichier {
    /// Journal des ventes.
    #[default]
    Ven,
    /// Journal des règlements (recettes).
    Rec,
}

impl JournalFichier {
    pub const TOUS: [JournalFichier; 2] = [Self::Ven, Self::Rec];

    /// Code écrit dans la colonne JOURNAL.
    pub fn code(self) -> &'static str {
        match self {
            Self::Ven => "VEN",
            Self::Rec => "REC",
        }
    }
}

impl std::fmt::Display for JournalFichier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

impl std::str::FromStr for JournalFichier {
    type Err = String;

    /// Casse et espaces ignorés ; texte vide : `VEN`.
    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        let texte = texte.trim();
        if texte.is_empty() {
            return Ok(Self::default());
        }
        Self::TOUS
            .into_iter()
            .find(|j| j.code().eq_ignore_ascii_case(texte))
            .ok_or_else(|| format!("journal du fichier « {texte} » inconnu (attendu : VEN ou REC)"))
    }
}
/// Code du compte clients, seul à porter un auxiliaire.
pub const COMPTE_CLIENTS: u64 = 411;
/// Valeur de la colonne AUXILIAIRE des lignes du compte clients.
pub const AUXILIAIRE_CLIENTS: &str = "ERP";
/// Types de compte dont les valeurs sont présentées nettes (un seul côté par écriture).
const TYPES_DE_TIERS: &[&str] = &["Receivable", "Payable"];

/// Journal comptable de Dokos.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Journal {
    /// Identifiant Dokos (`name`), par exemple « VEN - SLA » : code et abréviation de la société.
    #[serde(rename = "name")]
    pub id: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub journal_code: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub journal_name: String,
    /// `Sales`, `Purchase`, `Cash`, `Bank`, `Opening` ou `Miscellaneous`.
    #[serde(rename = "type", default, deserialize_with = "texte_ou_vide")]
    pub type_journal: String,
}

/// Journaux comptables actifs, par code.
pub fn lister_journaux<T: Transport>(dokos: &Dokos<T>) -> Result<Vec<Journal>, DokosError> {
    dokos.lister_tout(
        DOCTYPE_JOURNAL,
        &Liste::new(&["name", "journal_code", "journal_name", "type"])
            .filtre(Filtre::egal("disabled", 0))
            .ordre("journal_code asc, name asc"),
        JOURNAUX_MAX,
    )
}

fn nombre_ou_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(0.0))
}

/// Écriture comptable, telle que Dokos la renvoie.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Ecriture {
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub posting_date: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub account: String,
    #[serde(default, deserialize_with = "nombre_ou_zero")]
    pub debit: f64,
    #[serde(default, deserialize_with = "nombre_ou_zero")]
    pub credit: f64,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub party: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    pub voucher_no: String,
}

const CHAMPS_ECRITURE: &[&str] = &["posting_date", "account", "debit", "credit", "party", "voucher_no"];

/// Compte du plan comptable : numéro et type, pour les valeurs nettes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Compte {
    #[serde(rename = "name")]
    id: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    account_number: String,
    #[serde(default, deserialize_with = "texte_ou_vide")]
    account_type: String,
}

/// Une ligne du fichier final.
#[derive(Debug, Clone, PartialEq)]
pub struct LigneGrandLivre {
    pub date: Date,
    pub general: u64,
    /// Vide, sauf sur le compte clients.
    pub auxiliaire: &'static str,
    pub debit: f64,
    pub credit: f64,
    pub libelle: String,
    pub reference: String,
}

/// Grand livre prêt à écrire.
#[derive(Debug, Clone, PartialEq)]
pub struct GrandLivre {
    pub lignes: Vec<LigneGrandLivre>,
    /// Écritures écartées (compte sans numéro, date illisible), une phrase par cas.
    pub ecartees: Vec<String>,
    pub total_debit: f64,
    pub total_credit: f64,
}

/// Chiffres en tête d'un compte (« 411 - Clients - SLA » → 411) ; `None` s'il n'y en a pas.
pub fn code_compte(compte: &str) -> Option<u64> {
    let chiffres: String = compte.trim().chars().take_while(char::is_ascii_digit).collect();
    if chiffres.is_empty() { None } else { chiffres.parse().ok() }
}

fn arrondi(montant: f64) -> f64 {
    (montant * 100.0).round() / 100.0
}

/// Débit et crédit d'une écriture. Sur un compte de tiers, valeurs nettes : la
/// différence va du côté qui l'emporte, l'autre est à zéro.
fn montants(ecriture: &Ecriture, compte_de_tiers: bool) -> (f64, f64) {
    if compte_de_tiers {
        let net = arrondi(ecriture.debit - ecriture.credit);
        if net >= 0.0 { (net, 0.0) } else { (0.0, -net) }
    } else {
        (ecriture.debit, ecriture.credit)
    }
}

/// Met les écritures au format du fichier. `comptes` : plan comptable lu dans
/// Dokos (pour les types de compte et, à défaut de chiffres dans le nom du
/// compte, son numéro).
fn transformer(ecritures: &[Ecriture], comptes: &[Compte]) -> GrandLivre {
    let par_nom: BTreeMap<&str, &Compte> = comptes.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut lignes = Vec::with_capacity(ecritures.len());
    let mut ecartees = BTreeSet::new();
    let (mut total_debit, mut total_credit) = (0.0, 0.0);
    for e in ecritures {
        let compte = par_nom.get(e.account.as_str());
        let general = code_compte(&e.account).or_else(|| compte.and_then(|c| code_compte(&c.account_number)));
        let Some(general) = general else {
            ecartees.insert(format!("compte « {} » sans numéro (pièce {})", e.account, e.voucher_no));
            continue;
        };
        let Ok(date) = e.posting_date.parse::<Date>() else {
            ecartees.insert(format!("date « {} » illisible (pièce {})", e.posting_date, e.voucher_no));
            continue;
        };
        let de_tiers = compte.is_some_and(|c| TYPES_DE_TIERS.contains(&c.account_type.as_str()));
        let (debit, credit) = montants(e, de_tiers);
        total_debit += debit;
        total_credit += credit;
        lignes.push(LigneGrandLivre {
            date,
            general,
            auxiliaire: if general == COMPTE_CLIENTS { AUXILIAIRE_CLIENTS } else { "" },
            debit,
            credit,
            libelle: e.party.trim().to_owned(),
            reference: e.voucher_no.clone(),
        });
    }
    GrandLivre {
        lignes,
        ecartees: ecartees.into_iter().collect(),
        total_debit: arrondi(total_debit),
        total_credit: arrondi(total_credit),
    }
}

/// Lit les écritures du journal sur la période (bornes comprises) et les met
/// au format du fichier. `journal` est l'identifiant Dokos du journal
/// (« VEN - SLA »). Refusé au-delà de [`ECRITURES_MAX`] écritures.
pub fn lire_grand_livre<T: Transport>(
    dokos: &Dokos<T>,
    journal: &str,
    du: Date,
    au: Date,
) -> Result<GrandLivre, ErreurExport> {
    let journal = journal.trim();
    if journal.is_empty() {
        return Err(ErreurExport::Filtre("choisissez un journal comptable".to_owned()));
    }
    if du > au {
        return Err(ErreurExport::Filtre(format!("période inversée : du {du} au {au}")));
    }
    let comptes: Vec<Compte> = dokos.lister_tout(
        DOCTYPE_COMPTE,
        &Liste::new(&["name", "account_number", "account_type"]).ordre("name asc"),
        COMPTES_MAX,
    )?;
    let liste = Liste::new(CHAMPS_ECRITURE)
        .filtre(Filtre::egal("accounting_journal", journal))
        .filtre(Filtre::entre("posting_date", du.to_string(), au.to_string()))
        // Écritures annulées et à-nouveaux : exclus, comme dans le rapport.
        .filtre(Filtre::egal("is_cancelled", 0))
        .filtre(Filtre::different("is_opening", "Yes"))
        .ordre("posting_date asc, voucher_no asc, creation asc, name asc");
    // Une de plus que le plafond, pour savoir s'il est dépassé sans rien tronquer.
    let ecritures: Vec<Ecriture> = dokos.lister_tout(DOCTYPE_ECRITURE, &liste, ECRITURES_MAX + 1)?;
    if ecritures.len() > ECRITURES_MAX {
        return Err(ErreurExport::Filtre(format!("plus de {ECRITURES_MAX} écritures sur cette période : réduisez-la")));
    }
    Ok(transformer(&ecritures, &comptes))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transport::faux::FauxTransport;

    fn jour(texte: &str) -> Date {
        texte.parse().expect("date")
    }

    fn compte(id: &str, numero: &str, type_compte: &str) -> Compte {
        Compte { id: id.into(), account_number: numero.into(), account_type: type_compte.into() }
    }

    fn ecriture(date: &str, compte: &str, debit: f64, credit: f64, tiers: &str, piece: &str) -> Ecriture {
        Ecriture {
            posting_date: date.into(),
            account: compte.into(),
            debit,
            credit,
            party: tiers.into(),
            voucher_no: piece.into(),
        }
    }

    #[test]
    fn code_des_comptes() {
        assert_eq!(code_compte("411 - Clients - SLA"), Some(411));
        assert_eq!(code_compte(" 44571100 - TVA collectée - SLA"), Some(44_571_100));
        assert_eq!(code_compte("Clients - SLA"), None);
        assert_eq!(code_compte(""), None);
        assert_eq!(code_compte("99999999999999999999999 - trop long"), None);
    }

    #[test]
    fn format_du_fichier() {
        let comptes =
            [compte("411 - Clients - SLA", "411", "Receivable"), compte("706 - Ventes - SLA", "706", "Income Account")];
        let ecritures = [
            ecriture("2026-10-31", "411 - Clients - SLA", 120.0, 0.0, " Mairie de Crest ", "ACC-SINV-1"),
            ecriture("2026-10-31", "706 - Ventes - SLA", 0.0, 100.0, "", "ACC-SINV-1"),
            ecriture("2026-10-31", "44571100 - TVA collectée - SLA", 0.0, 20.0, "", "ACC-SINV-1"),
        ];
        let gl = transformer(&ecritures, &comptes);
        assert_eq!(gl.lignes.len(), 3);
        let clients = &gl.lignes[0];
        assert_eq!((clients.general, clients.auxiliaire), (411, "ERP"));
        assert_eq!(clients.libelle, "Mairie de Crest");
        assert_eq!(clients.reference, "ACC-SINV-1");
        assert_eq!(clients.date, jour("2026-10-31"));
        assert_eq!((gl.lignes[1].general, gl.lignes[1].auxiliaire, gl.lignes[1].credit), (706, "", 100.0));
        assert_eq!(gl.lignes[2].general, 44_571_100, "code tel quel, comptes inconnus du plan compris");
        assert_eq!((gl.total_debit, gl.total_credit), (120.0, 120.0));
        assert!(gl.ecartees.is_empty());
    }

    #[test]
    fn journal_du_fichier() {
        assert_eq!("VEN".parse(), Ok(JournalFichier::Ven));
        assert_eq!(" rec ".parse(), Ok(JournalFichier::Rec));
        assert_eq!("".parse(), Ok(JournalFichier::Ven), "vide : VEN");
        assert!("ACH".parse::<JournalFichier>().expect_err("inconnu").contains("VEN ou REC"));
        assert_eq!((JournalFichier::Ven.code(), JournalFichier::Rec.to_string().as_str()), ("VEN", "REC"));
    }

    #[test]
    fn auxiliaire_seulement_sur_le_compte_411_exact() {
        let ecritures = [
            ecriture("2026-10-01", "4110 - Clients douteux - SLA", 1.0, 0.0, "", "P"),
            ecriture("2026-10-01", "4191 - Avances - SLA", 0.0, 1.0, "", "P"),
        ];
        let gl = transformer(&ecritures, &[]);
        assert!(gl.lignes.iter().all(|l| l.auxiliaire.is_empty()));
    }

    #[test]
    fn valeurs_nettes_sur_les_comptes_de_tiers() {
        let comptes = [
            compte("411 - Clients - SLA", "411", "Receivable"),
            compte("658 - Charges - SLA", "658", "Expense Account"),
        ];
        let ecritures = [
            ecriture("2026-10-01", "411 - Clients - SLA", 150.0, 50.0, "A", "P1"),
            ecriture("2026-10-01", "411 - Clients - SLA", 10.0, 30.0, "A", "P2"),
            ecriture("2026-10-01", "658 - Charges - SLA", 10.0, 3.0, "", "P3"),
        ];
        let gl = transformer(&ecritures, &comptes);
        assert_eq!((gl.lignes[0].debit, gl.lignes[0].credit), (100.0, 0.0));
        assert_eq!((gl.lignes[1].debit, gl.lignes[1].credit), (0.0, 20.0));
        assert_eq!((gl.lignes[2].debit, gl.lignes[2].credit), (10.0, 3.0), "hors tiers : valeurs brutes");
    }

    #[test]
    fn numero_du_plan_comptable_a_defaut_de_chiffres_dans_le_nom() {
        let comptes = [compte("Clients", "411", "Receivable")];
        let gl = transformer(&[ecriture("2026-10-01", "Clients", 5.0, 0.0, "", "P")], &comptes);
        assert_eq!((gl.lignes[0].general, gl.lignes[0].auxiliaire), (411, "ERP"));
    }

    #[test]
    fn ecritures_ecartees_et_signalees() {
        let ecritures = [
            ecriture("2026-10-01", "Sans numéro", 1.0, 0.0, "", "P1"),
            ecriture("2026-10-01", "Sans numéro", 2.0, 0.0, "", "P1"),
            ecriture("hier", "706 - Ventes - SLA", 0.0, 1.0, "", "P2"),
            ecriture("2026-10-01", "706 - Ventes - SLA", 0.0, 1.0, "", "P3"),
        ];
        let gl = transformer(&ecritures, &[]);
        assert_eq!(gl.lignes.len(), 1);
        assert_eq!(gl.ecartees.len(), 2, "une phrase par cas, sans doublon : {:?}", gl.ecartees);
        assert!(gl.ecartees.iter().any(|e| e.contains("Sans numéro")));
        assert!(gl.ecartees.iter().any(|e| e.contains("hier")));
    }

    #[test]
    fn lecture_avec_filtres_de_dokos() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "411 - Clients - SLA", "account_number": "411", "account_type": "Receivable"}]}),
        );
        faux.repondre_json(
            200,
            &json!({"data": [
                {"posting_date": "2026-10-31", "account": "411 - Clients - SLA", "debit": 120.0, "credit": null,
                 "party": "Mairie", "voucher_no": "ACC-SINV-1"},
            ]}),
        );
        let gl = lire_grand_livre(&Dokos::new(&faux), " VEN - SLA ", jour("2026-10-01"), jour("2026-10-31"))
            .expect("grand livre");
        assert_eq!(gl.lignes.len(), 1);
        assert_eq!((gl.lignes[0].debit, gl.lignes[0].credit), (120.0, 0.0), "crédit nul lu comme zéro");

        let requetes = faux.requetes();
        assert_eq!(requetes[0].chemin, "/api/resource/Account");
        assert_eq!(requetes[1].chemin, "/api/resource/GL%20Entry");
        let filtres = requetes[1].parametre("filters").unwrap_or_default().to_owned();
        for attendu in [
            r#"["accounting_journal","=","VEN - SLA"]"#,
            r#"["posting_date","between",["2026-10-01","2026-10-31"]]"#,
            r#"["is_cancelled","=",0]"#,
            r#"["is_opening","!=","Yes"]"#,
        ] {
            assert!(filtres.contains(attendu), "{attendu} absent de {filtres}");
        }
        assert_eq!(requetes[1].parametre("order_by"), Some("posting_date asc, voucher_no asc, creation asc, name asc"));
        assert!(requetes.iter().all(|r| r.methode == crate::transport::Methode::Get));
    }

    #[test]
    fn periode_et_journal_verifies_avant_toute_lecture() {
        let faux = FauxTransport::new();
        let dokos = Dokos::new(&faux);
        assert!(matches!(
            lire_grand_livre(&dokos, "VEN - SLA", jour("2026-11-01"), jour("2026-10-01")),
            Err(ErreurExport::Filtre(m)) if m.contains("inversée")
        ));
        assert!(matches!(
            lire_grand_livre(&dokos, "  ", jour("2026-10-01"), jour("2026-10-31")),
            Err(ErreurExport::Filtre(m)) if m.contains("journal")
        ));
        assert!(faux.requetes().is_empty());
    }

    #[test]
    fn liste_des_journaux() {
        let faux = FauxTransport::new();
        faux.repondre_json(
            200,
            &json!({"data": [{"name": "VEN - SLA", "journal_code": "VEN", "journal_name": "Ventes", "type": "Sales"}]}),
        );
        let journaux = lister_journaux(&Dokos::new(&faux)).expect("journaux");
        assert_eq!(journaux[0].id, "VEN - SLA");
        assert_eq!((journaux[0].journal_code.as_str(), journaux[0].type_journal.as_str()), ("VEN", "Sales"));
        let requete = &faux.requetes()[0];
        assert_eq!(requete.chemin, "/api/resource/Accounting%20Journal");
        assert_eq!(requete.parametre("filters"), Some(r#"[["disabled","=",0]]"#));
    }
}
