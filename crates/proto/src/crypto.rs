use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};

type CbcEnc = cbc::Encryptor<aes::Aes128>;
type CbcDec = cbc::Decryptor<aes::Aes128>;

/// Additional data used by Prime devices for AES-GCM.
pub const GCM_AAD: [u8; 16] = [
    0x33, 0x22, 0x11, 0x00, 0x77, 0x66, 0x55, 0x44, 0xbb, 0xaa, 0x99, 0x88, 0xff, 0xee, 0xdd, 0xcc,
];

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CryptoError {
    #[error("ciphertext could not be decrypted")]
    Decrypt,
    #[error("device public key is invalid")]
    PublicKey,
    #[error("private key is invalid")]
    PrivateKey,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Cipher {
    Cbc { key: [u8; 16], iv: [u8; 16] },
    Gcm { key: [u8; 16], nonce: [u8; 12] },
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cipher::Cbc { .. } => f.write_str("Cipher::Cbc"),
            Cipher::Gcm { .. } => f.write_str("Cipher::Gcm"),
        }
    }
}

impl Cipher {
    /// AES-128-CBC with key and IV taken from the ECDH shared secret.
    pub fn cbc_from_secret(secret: &[u8; 32]) -> Self {
        let mut key = [0; 16];
        let mut iv = [0; 16];
        key.copy_from_slice(&secret[..16]);
        iv.copy_from_slice(&secret[16..]);
        Cipher::Cbc { key, iv }
    }

    /// AES-128-GCM with key and nonce taken from the ECDH shared secret.
    pub fn gcm_from_secret(secret: &[u8; 32]) -> Self {
        let mut key = [0; 16];
        let mut nonce = [0; 12];
        key.copy_from_slice(&secret[..16]);
        nonce.copy_from_slice(&secret[16..28]);
        Cipher::Gcm { key, nonce }
    }

    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        match self {
            Cipher::Cbc { key, iv } => {
                CbcEnc::new(&(*key).into(), &(*iv).into()).encrypt_padded_vec::<Pkcs7>(plain)
            }
            Cipher::Gcm { key, nonce } => aes_gcm::Aes128Gcm::new(&(*key).into())
                .encrypt(
                    &(*nonce).into(),
                    Payload {
                        msg: plain,
                        aad: &GCM_AAD,
                    },
                )
                .unwrap_or_default(),
        }
    }

    pub fn open(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match self {
            Cipher::Cbc { key, iv } => CbcDec::new(&(*key).into(), &(*iv).into())
                .decrypt_padded_vec::<Pkcs7>(ciphertext)
                .map_err(|_| CryptoError::Decrypt),
            Cipher::Gcm { key, nonce } => aes_gcm::Aes128Gcm::new(&(*key).into())
                .decrypt(
                    &(*nonce).into(),
                    Payload {
                        msg: ciphertext,
                        aad: &GCM_AAD,
                    },
                )
                .map_err(|_| CryptoError::Decrypt),
        }
    }
}

/// A P-256 key pair for the ECDH step of the handshake.
#[derive(Clone)]
pub struct KeyPair {
    secret: SecretKey,
}

impl KeyPair {
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, CryptoError> {
        SecretKey::from_slice(bytes)
            .map(|secret| Self { secret })
            .map_err(|_| CryptoError::PrivateKey)
    }

    /// Our public key as X‖Y, 64 bytes.
    pub fn public_xy(&self) -> [u8; 64] {
        let point = self.secret.public_key().to_sec1_point(false);
        let mut out = [0; 64];
        out.copy_from_slice(&point.as_bytes()[1..65]);
        out
    }

    /// ECDH with the device's X‖Y public key; returns the 32-byte x coordinate.
    pub fn shared_secret(&self, device_xy: &[u8]) -> Result<[u8; 32], CryptoError> {
        if device_xy.len() != 64 {
            return Err(CryptoError::PublicKey);
        }
        let mut sec1 = Vec::with_capacity(65);
        sec1.push(0x04);
        sec1.extend_from_slice(device_xy);
        let public = PublicKey::from_sec1_bytes(&sec1).map_err(|_| CryptoError::PublicKey)?;
        let shared =
            p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), public.as_affine());
        let mut out = [0; 32];
        out.copy_from_slice(shared.raw_secret_bytes());
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbc_round_trip() {
        let c = Cipher::cbc_from_secret(&[7; 32]);
        let ct = c.seal(b"hello");
        assert_eq!(ct.len(), 16);
        assert_eq!(c.open(&ct).expect("open"), b"hello");
    }

    #[test]
    fn gcm_round_trip() {
        let c = Cipher::gcm_from_secret(&[9; 32]);
        let ct = c.seal(b"hello");
        assert_eq!(ct.len(), 5 + 16);
        assert_eq!(c.open(&ct).expect("open"), b"hello");
        assert!(c.open(&ct[..10]).is_err());
    }
}
