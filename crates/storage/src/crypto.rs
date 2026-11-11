use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Result, StorageError};

const VERSION: u8 = 1;
const NONCE_BYTES: usize = 24;

#[derive(Clone)]
pub struct RecoveryKey([u8; 32]);

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryEnvelope {
    user_id: Uuid,
    email: String,
    token: String,
}

impl RecoveryKey {
    /// Accepts exactly 64 hexadecimal characters (32 bytes of key material).
    pub fn from_hex(value: &str) -> Result<Self> {
        let bytes = hex::decode(value).map_err(|_| StorageError::Crypto)?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| StorageError::Crypto)?;
        Ok(Self(key))
    }

    pub fn encrypt(&self, user_id: Uuid, email: &str, token: &str) -> Result<Vec<u8>> {
        let envelope = RecoveryEnvelope {
            user_id,
            email: email.to_owned(),
            token: token.to_owned(),
        };
        let plaintext = serde_json::to_vec(&envelope).map_err(|_| StorageError::Crypto)?;
        let mut nonce = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| StorageError::Crypto)?;
        let cipher = XChaCha20Poly1305::new((&self.0).into());
        let ciphertext = cipher
            .encrypt(&XNonce::from(nonce), plaintext.as_slice())
            .map_err(|_| StorageError::Crypto)?;
        let mut output = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        output.push(VERSION);
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Used by the isolated delivery worker and by local tests. Do not expose
    /// decrypted recovery material through an HTTP endpoint.
    pub fn decrypt(&self, payload: &[u8]) -> Result<(Uuid, String, String)> {
        if payload.len() < 1 + NONCE_BYTES + 16 || payload[0] != VERSION {
            return Err(StorageError::Crypto);
        }
        let nonce: [u8; NONCE_BYTES] = payload[1..1 + NONCE_BYTES]
            .try_into()
            .map_err(|_| StorageError::Crypto)?;
        let ciphertext = &payload[1 + NONCE_BYTES..];
        let cipher = XChaCha20Poly1305::new((&self.0).into());
        let plaintext = cipher
            .decrypt(&XNonce::from(nonce), ciphertext)
            .map_err(|_| StorageError::Crypto)?;
        let envelope: RecoveryEnvelope =
            serde_json::from_slice(&plaintext).map_err(|_| StorageError::Crypto)?;
        Ok((envelope.user_id, envelope.email, envelope.token))
    }
}
