//! Assistance par IA via Open WebUI.
//!
//! L'IA propose, Rust vérifie, l'utilisateur valide, Dokos enregistre :
//! - l'IA ne reçoit que le texte de la demande et les listes utiles (types de
//!   contrat, champs disponibles…), jamais la clé API de Dokos ;
//! - elle répond par un JSON désérialisé dans des types stricts, puis chaque
//!   élément est confronté à Dokos (lecture seule) ;
//! - le résultat ne fait que **pré-remplir** un formulaire (ou choisir une
//!   liste à exporter, en lecture seule) : aucune écriture
//!   dans Dokos ne part d'ici.

pub mod adresse;
pub mod commun;
pub mod conversation;
pub mod listes;
pub mod owui;
pub mod ventes;

pub use commun::Choix;
pub use owui::{ErreurIa, OpenWebUi, Reponse, Tour};
