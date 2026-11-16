//! Ed25519 signatures and SHA-256 checksums, all as lower-case hex text.
//!
//! The same format is used for the catalog (`catalog.json.sig`) and for
//! Freya TDE's skill packs (`pack.json.sig`): the signature covers the
//! exact bytes of the signed file.
//!
//! *Ed25519-Signaturen und SHA-256-Prüfsummen, als Hex-Text.*

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::Error;

/// Lower-case hex of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Bytes of a hex string (upper or lower case, surrounding white space
/// ignored).
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// A 32-byte key from hex.
pub fn key_from_hex(text: &str) -> Option<[u8; 32]> {
    from_hex(text).and_then(|b| b.try_into().ok())
}

/// SHA-256 of `bytes`, hex.
pub fn sha256(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// The public key belonging to a secret key.
pub fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

/// Signs `data` with the secret key; the signature as hex.
pub fn sign(data: &[u8], secret: &[u8; 32]) -> String {
    to_hex(&SigningKey::from_bytes(secret).sign(data).to_bytes())
}

/// Checks the hex `signature` of `data` against the public `key`.
pub fn verify(data: &[u8], signature: &str, key: &[u8; 32]) -> Result<(), Error> {
    let key = VerifyingKey::from_bytes(key).map_err(|e| {
        Error::new(
            crate::codes::BAD_KEY,
            format!("öffentlicher Schlüssel: {e}"),
        )
    })?;
    let signature = from_hex(signature)
        .and_then(|b| <[u8; 64]>::try_from(b).ok())
        .map(|b| Signature::from_bytes(&b))
        .ok_or_else(|| {
            Error::new(
                crate::codes::SIGNATURE,
                "Signatur ist kein Ed25519-Hex-Text",
            )
        })?;
    key.verify(data, &signature)
        .map_err(|_| Error::new(crate::codes::SIGNATURE, "Signatur passt nicht zu den Daten"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let secret = [3; 32];
        let public = public_key(&secret);
        let sig = sign(b"katalog", &secret);
        assert_eq!(sig.len(), 128);
        assert!(verify(b"katalog", &sig, &public).is_ok());
        assert_eq!(
            verify(b"katalog!", &sig, &public).unwrap_err().code,
            crate::codes::SIGNATURE
        );
        assert_eq!(
            verify(b"katalog", "zz", &public).unwrap_err().code,
            crate::codes::SIGNATURE
        );
        assert_eq!(
            verify(b"katalog", &sig, &public_key(&[4; 32]))
                .unwrap_err()
                .code,
            crate::codes::SIGNATURE
        );
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(from_hex(" 00ff10 ").unwrap(), [0, 255, 16]);
        assert_eq!(from_hex("ABcd").unwrap(), [0xab, 0xcd]);
        assert!(from_hex("abc").is_none() && from_hex("zz").is_none());
        assert_eq!(to_hex(&[1, 171]), "01ab");
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
