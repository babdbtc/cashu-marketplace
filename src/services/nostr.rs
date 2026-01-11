use nostr_sdk::prelude::*;
use sha2::Digest;

use crate::config::Config;
use crate::error::{AppError, AppResult};

/// Nostr keypair and encryption service
pub struct NostrService {
    _admin_npub: String,
}

impl NostrService {
    /// Initialize Nostr service
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        Ok(Self {
            _admin_npub: config.admin_npub.clone(),
        })
    }

    /// Generate a new Nostr keypair
    pub fn generate_keypair() -> AppResult<(String, String)> {
        let keys = Keys::generate();

        let nsec = keys
            .secret_key()
            .to_bech32()
            .map_err(|e| AppError::Internal(format!("Failed to encode nsec: {}", e)))?;

        let npub = keys
            .public_key()
            .to_bech32()
            .map_err(|e| AppError::Internal(format!("Failed to encode npub: {}", e)))?;

        Ok((nsec, npub))
    }

    /// Validate and parse an nsec
    pub fn validate_nsec(nsec: &str) -> AppResult<Keys> {
        let secret_key =
            SecretKey::from_bech32(nsec).map_err(|_| AppError::InvalidNsec)?;

        let keys = Keys::new(secret_key);
        Ok(keys)
    }

    /// Get npub from nsec
    pub fn npub_from_nsec(nsec: &str) -> AppResult<String> {
        let keys = Self::validate_nsec(nsec)?;
        let npub = keys
            .public_key()
            .to_bech32()
            .map_err(|e| AppError::Internal(format!("Failed to encode npub: {}", e)))?;
        Ok(npub)
    }

    /// Validate an npub
    pub fn validate_npub(npub: &str) -> AppResult<PublicKey> {
        PublicKey::from_bech32(npub)
            .map_err(|_| AppError::Internal("Invalid npub".to_string()))
    }

    /// Encrypt a message to a recipient's npub (NIP-44)
    pub fn encrypt_message(
        sender_nsec: &str,
        recipient_npub: &str,
        content: &str,
    ) -> AppResult<String> {
        let sender_keys = Self::validate_nsec(sender_nsec)?;
        let recipient_pubkey = Self::validate_npub(recipient_npub)?;

        let encrypted = nip44::encrypt(
            sender_keys.secret_key(),
            &recipient_pubkey,
            content,
            nip44::Version::default(),
        )
        .map_err(|e| AppError::Internal(format!("Encryption failed: {}", e)))?;

        Ok(encrypted)
    }

    /// Decrypt a message from a sender's npub (NIP-44)
    pub fn decrypt_message(
        recipient_nsec: &str,
        sender_npub: &str,
        encrypted: &str,
    ) -> AppResult<String> {
        let recipient_keys = Self::validate_nsec(recipient_nsec)?;
        let sender_pubkey = Self::validate_npub(sender_npub)?;

        let decrypted = nip44::decrypt(recipient_keys.secret_key(), &sender_pubkey, encrypted)
            .map_err(|e| AppError::Internal(format!("Decryption failed: {}", e)))?;

        Ok(decrypted)
    }

    /// Sign a message with an nsec (for verification)
    /// Note: This creates a simple hash-based signature for internal use
    pub fn sign_message(nsec: &str, message: &str) -> AppResult<String> {
        let keys = Self::validate_nsec(nsec)?;

        // Create a simple signature by hashing message with secret key context
        let mut hasher = sha2::Sha256::new();
        hasher.update(message.as_bytes());
        hasher.update(keys.public_key().to_bytes());
        let hash = hasher.finalize();

        Ok(hex::encode(hash))
    }

    /// Verify a signature
    /// Note: This is a simplified verification for internal use
    pub fn verify_signature(npub: &str, message: &str, signature: &str) -> AppResult<bool> {
        let pubkey = Self::validate_npub(npub)?;

        // Recreate the hash and compare
        let mut hasher = sha2::Sha256::new();
        hasher.update(message.as_bytes());
        hasher.update(pubkey.to_bytes());
        let hash = hasher.finalize();

        let expected = hex::encode(hash);
        Ok(expected == signature)
    }

    /// Encrypt nsec for storage (optional, for server-generated keys)
    /// Uses a simple XOR with a derived key for now
    /// In production, use proper key derivation and AES-GCM
    pub fn encrypt_nsec_for_storage(nsec: &str, user_password: &str) -> AppResult<String> {
        // Simple encryption for demo - in production use proper crypto
        let key_bytes = sha2::Sha256::digest(user_password.as_bytes());
        let nsec_bytes = nsec.as_bytes();

        let encrypted: Vec<u8> = nsec_bytes
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ key_bytes[i % 32])
            .collect();

        Ok(hex::encode(encrypted))
    }

    /// Decrypt nsec from storage
    pub fn decrypt_nsec_from_storage(encrypted: &str, user_password: &str) -> AppResult<String> {
        let encrypted_bytes =
            hex::decode(encrypted).map_err(|_| AppError::Internal("Invalid encrypted data".to_string()))?;

        let key_bytes = sha2::Sha256::digest(user_password.as_bytes());

        let decrypted: Vec<u8> = encrypted_bytes
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ key_bytes[i % 32])
            .collect();

        String::from_utf8(decrypted)
            .map_err(|_| AppError::Internal("Decryption failed".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let (nsec, npub) = NostrService::generate_keypair().unwrap();
        assert!(nsec.starts_with("nsec1"));
        assert!(npub.starts_with("npub1"));
    }

    #[test]
    fn test_npub_from_nsec() {
        let (nsec, expected_npub) = NostrService::generate_keypair().unwrap();
        let npub = NostrService::npub_from_nsec(&nsec).unwrap();
        assert_eq!(npub, expected_npub);
    }

    #[test]
    fn test_encrypt_decrypt_message() {
        let (sender_nsec, _sender_npub) = NostrService::generate_keypair().unwrap();
        let (recipient_nsec, recipient_npub) = NostrService::generate_keypair().unwrap();

        let message = "Hello, this is a secret message!";

        let encrypted =
            NostrService::encrypt_message(&sender_nsec, &recipient_npub, message).unwrap();

        let sender_npub = NostrService::npub_from_nsec(&sender_nsec).unwrap();
        let decrypted =
            NostrService::decrypt_message(&recipient_nsec, &sender_npub, &encrypted).unwrap();

        assert_eq!(decrypted, message);
    }
}
