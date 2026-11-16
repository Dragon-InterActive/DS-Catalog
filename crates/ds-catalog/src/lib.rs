//! The signed program catalog of Dragon Software: schema and validation
//! ([`catalog`]), Ed25519 signatures and SHA-256 checksums ([`sign`]),
//! platform keys ([`platform`]). Used by the DS-Launcher (reading) and by
//! `ds-sign` (writing); Freya TDE's skill packs use the same signatures.
//!
//! Errors carry a stable `snake_case` code ([`codes`]).
//!
//! *Signierter Programmkatalog: Schema, Prüfung, Signaturen.*

pub mod catalog;
pub mod platform;
pub mod sign;

use serde::Serialize;

/// Stable error codes.
pub mod codes {
    /// No public key is built in yet.
    pub const NO_KEY: &str = "no_key";
    /// A public key is not a valid Ed25519 key.
    pub const BAD_KEY: &str = "bad_key";
    /// A signature is missing, malformed or does not match.
    pub const SIGNATURE: &str = "signature";
    /// The catalog is not valid JSON or breaks the schema.
    pub const CATALOG_INVALID: &str = "catalog_invalid";
    /// A downloaded file has another SHA-256 than the catalog says.
    pub const DOWNLOAD_HASH: &str = "download_hash";
}

/// An error with a stable code and a readable detail (German).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct Error {
    pub code: &'static str,
    pub detail: String,
}

impl Error {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}
