use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};
use openopen_protocol::{EffectPermit, EvidenceKind, EvidenceRef, effect_permit_signing_bytes};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;

const NONCE_LENGTH: usize = 12;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CryptoError {
    #[error("encrypted value is malformed")]
    MalformedCiphertext,
    #[error("encrypted value authentication failed")]
    DecryptionFailed,
    #[error("value encryption failed")]
    EncryptionFailed,
    #[error("invalid signature encoding")]
    InvalidSignatureEncoding,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("evidence issuer is not the local trusted authority")]
    UntrustedIssuer,
    #[error("effect permit key is not the local effect authority")]
    UntrustedEffectKey,
    #[error("effect permit stable hash does not match its typed command")]
    InvalidEffectHash,
    #[error("serialized value error: {0}")]
    Serialization(String),
    #[error("secure random generation failed")]
    RandomGeneration,
}

#[derive(Clone)]
pub struct LocalAuthority {
    issuer_id: String,
    signing_key: SigningKey,
    effect_signing_key: SigningKey,
    cipher: Aes256Gcm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceClaims {
    pub id: String,
    pub mission_id: String,
    pub work_item_id: String,
    pub kind: EvidenceKind,
    pub source_id: String,
    pub sha256: Option<String>,
    pub observed_at_ms: i64,
}

impl LocalAuthority {
    /// Derives independent signing and encryption keys from Keychain-owned
    /// master material. The master is not retained after construction.
    #[must_use]
    pub fn from_master(issuer_id: impl Into<String>, master: [u8; 32]) -> Self {
        let signing_seed = derive_key(&master, b"openopen-signing-v1");
        let effect_signing_seed = derive_key(&master, b"openopen-effect-authorizer-v1");
        let encryption_key = derive_key(&master, b"openopen-encryption-v1");
        Self {
            issuer_id: issuer_id.into(),
            signing_key: SigningKey::from_bytes(&signing_seed),
            effect_signing_key: SigningKey::from_bytes(&effect_signing_seed),
            cipher: Aes256Gcm::new((&encryption_key).into()),
        }
    }

    #[must_use]
    pub fn issuer_id(&self) -> &str {
        &self.issuer_id
    }

    #[must_use]
    pub fn effect_verifying_key_hex(&self) -> String {
        hex::encode(self.effect_signing_key.verifying_key().to_bytes())
    }

    #[must_use]
    pub fn effect_key_id(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(self.effect_signing_key.verifying_key().to_bytes())
        )
    }

    #[must_use]
    pub fn sign_evidence(&self, claims: EvidenceClaims) -> EvidenceRef {
        let mut evidence = EvidenceRef {
            id: claims.id,
            mission_id: claims.mission_id,
            work_item_id: claims.work_item_id,
            kind: claims.kind,
            source_id: claims.source_id,
            sha256: claims.sha256,
            issuer_id: self.issuer_id.clone(),
            signature_hex: String::new(),
            observed_at_ms: claims.observed_at_ms,
        };
        evidence.signature_hex = self.sign_bytes(&evidence_bytes(&evidence));
        evidence
    }

    /// Verifies that evidence was emitted by this Rust authority and was not
    /// changed after observation.
    ///
    /// # Errors
    ///
    /// Returns an error for a different issuer, malformed signature, or any
    /// changed signed field.
    pub fn verify_evidence(&self, evidence: &EvidenceRef) -> Result<(), CryptoError> {
        if evidence.issuer_id != self.issuer_id {
            return Err(CryptoError::UntrustedIssuer);
        }
        self.verify_bytes(&evidence_bytes(evidence), &evidence.signature_hex)
    }

    pub(crate) fn sign_effect_permit(&self, permit: &mut EffectPermit) -> Result<(), CryptoError> {
        let bytes = effect_permit_signing_bytes(permit)
            .map_err(|error| CryptoError::Serialization(error.to_string()))?;
        permit.authorization_signature_hex =
            hex::encode(self.effect_signing_key.sign(&bytes).to_bytes());
        Ok(())
    }

    pub(crate) fn sign_effect_bytes(&self, bytes: &[u8]) -> String {
        hex::encode(self.effect_signing_key.sign(bytes).to_bytes())
    }

    pub(crate) fn verify_effect_bytes(
        &self,
        bytes: &[u8],
        signature_hex: &str,
    ) -> Result<(), CryptoError> {
        verify_with_key(
            &self.effect_signing_key.verifying_key(),
            bytes,
            signature_hex,
        )
    }

    /// Verifies a typed effect permit against the independently derived local
    /// effect-authority key.
    ///
    /// # Errors
    ///
    /// Returns an error for a changed key id, malformed permit, or signature.
    pub fn verify_effect_permit(&self, permit: &EffectPermit) -> Result<(), CryptoError> {
        if permit.core_key_id != self.effect_key_id() {
            return Err(CryptoError::UntrustedEffectKey);
        }
        if crate::effect::stable_effect_hash(&permit.command)
            .map_err(|error| CryptoError::Serialization(error.to_string()))?
            != permit.stable_effect_hash
        {
            return Err(CryptoError::InvalidEffectHash);
        }
        let bytes = effect_permit_signing_bytes(permit)
            .map_err(|error| CryptoError::Serialization(error.to_string()))?;
        verify_with_key(
            &self.effect_signing_key.verifying_key(),
            &bytes,
            &permit.authorization_signature_hex,
        )
    }

    pub(crate) fn sign_bytes(&self, bytes: &[u8]) -> String {
        hex::encode(self.signing_key.sign(bytes).to_bytes())
    }

    pub(crate) fn verify_bytes(
        &self,
        bytes: &[u8],
        signature_hex: &str,
    ) -> Result<(), CryptoError> {
        verify_with_key(&self.signing_key.verifying_key(), bytes, signature_hex)
    }

    pub(crate) fn encrypt_json<T: Serialize>(
        &self,
        value: &T,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let plaintext = serde_json::to_vec(value)
            .map_err(|error| CryptoError::Serialization(error.to_string()))?;
        let mut nonce_bytes = [0_u8; NONCE_LENGTH];
        getrandom::fill(&mut nonce_bytes).map_err(|_| CryptoError::RandomGeneration)?;
        let ciphertext = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: &plaintext,
                    aad: associated_data,
                },
            )
            .map_err(|_| CryptoError::EncryptionFailed)?;
        let mut sealed = nonce_bytes.to_vec();
        sealed.extend(ciphertext);
        Ok(sealed)
    }

    pub(crate) fn decrypt_json<T: DeserializeOwned>(
        &self,
        sealed: &[u8],
        associated_data: &[u8],
    ) -> Result<T, CryptoError> {
        let (nonce, ciphertext) = sealed
            .split_at_checked(NONCE_LENGTH)
            .ok_or(CryptoError::MalformedCiphertext)?;
        let plaintext = self
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: associated_data,
                },
            )
            .map_err(|_| CryptoError::DecryptionFailed)?;
        serde_json::from_slice(&plaintext)
            .map_err(|error| CryptoError::Serialization(error.to_string()))
    }
}

fn verify_with_key(
    verifying_key: &ed25519_dalek::VerifyingKey,
    bytes: &[u8],
    signature_hex: &str,
) -> Result<(), CryptoError> {
    let raw = hex::decode(signature_hex).map_err(|_| CryptoError::InvalidSignatureEncoding)?;
    let signature =
        Signature::from_slice(&raw).map_err(|_| CryptoError::InvalidSignatureEncoding)?;
    verifying_key
        .verify(bytes, &signature)
        .map_err(|_| CryptoError::InvalidSignature)
}

fn derive_key(master: &[u8; 32], domain: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(master);
    digest.finalize().into()
}

fn evidence_bytes(evidence: &EvidenceRef) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": evidence.id,
        "issuerId": evidence.issuer_id,
        "kind": evidence.kind,
        "missionId": evidence.mission_id,
        "observedAtMs": evidence.observed_at_ms,
        "sha256": evidence.sha256,
        "sourceId": evidence.source_id,
        "version": 1,
        "workItemId": evidence.work_item_id,
    }))
    .expect("EvidenceRef fields are infallibly serializable")
}
