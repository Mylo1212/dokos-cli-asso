//! Conformité des contrats de travail.
//!
//! Deux niveaux, sans IA :
//! - **le gabarit** (format d'impression) contient-il toutes les mentions
//!   obligatoires du type de contrat ? ([`rapport::verifier_gabarit`]) ;
//! - **chaque contrat** a-t-il des données complètes et cohérentes ?
//!   ([`donnees::verifier_donnees`]).
//!
//! La liste des mentions ([`regles`]) est un fichier relu par un humain.
//! L'outil vérifie la présence et la cohérence des mentions ; il ne garantit
//! pas la validité juridique d'un contrat.

#[cfg(feature = "dokos")]
pub mod dokos;
pub mod donnees;
pub mod gabarit;
pub mod normalisation;
pub mod proposition;
pub mod rapport;
pub mod regles;

pub use regles::{Contexte, Regles};
