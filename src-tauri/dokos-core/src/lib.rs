//! Client de l'API REST Frappe utilisée par Dokos.
//!
//! - Lecture : [`Dokos::lister`], [`Dokos::lire`], les recherches de [`recherche`]
//!   et les listes exportables de [`export`].
//! - Écriture : uniquement des opérations typées et validées, comme
//!   [`vente::creer_brouillon`], précédée d'un contrôle de doublon
//!   ([`doublon`]). Il n'existe pas de `POST` générique public.
//!
//! Le réseau passe par le trait [`Transport`] : [`HttpTransport`] en vrai,
//! `transport::faux::FauxTransport` dans les tests.

pub mod article;
pub mod brouillons;
pub mod client;
pub mod clientele;
pub mod contrat;
pub mod date;
pub mod doublon;
pub mod employe;
pub mod erreur;
pub mod evenement;
pub mod export;
pub mod fiche;
pub mod formulaire;
pub mod http;
pub mod impression;
pub mod inscription;
pub mod meta;
pub mod modele_vente;
mod parallele;
pub mod recherche;
pub mod suivi;
pub mod transformation;
pub mod transport;
pub mod types;
pub mod vente;

pub use client::{Dokos, Filtre, Liste};
pub use erreur::DokosError;
pub use http::{HttpTransport, Secret};
pub use transport::Transport;
