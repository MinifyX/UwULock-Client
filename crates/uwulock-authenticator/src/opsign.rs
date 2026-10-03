//! Windows' request signatures for a plugin passkey manager.
//!
//! When a plugin is added (`WebAuthNPluginAddAuthenticator`), Windows hands
//! it the public half of an "operation signing" key; every request it sends
//! later carries a signature over the encoded request made with the private
//! half, which only Windows' WebAuthn service holds. A request without a
//! valid one didn't come from Windows — another program of the same user
//! talks to UwULock's COM class directly — and is refused.
//!
//! The key comes as a CNG public key blob (`BCRYPT_ECCPUBLIC_BLOB` for
//! P-256, `BCRYPT_RSAPUBLIC_BLOB`) or as DER `SubjectPublicKeyInfo`; the
//! signature is ECDSA over SHA-256 (CNG's `r‖s` or DER) or RSA PKCS #1 v1.5 /
//! PSS with SHA-256, as `BCryptVerifySignature` checks them. Anything else
//! is refused: an unknown key verifies nothing. Pure Rust, so it is tested
//! on every system.

use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use rsa::{BigUint, Pkcs1v15Sign, Pss, RsaPublicKey};
use sha2::{Digest, Sha256};

/// `BCRYPT_ECDSA_PUBLIC_P256_MAGIC`, "ECS1".
const ECDSA_P256: u32 = 0x3153_4345;
/// `BCRYPT_ECDSA_PUBLIC_GENERIC_MAGIC`, "ECDP".
const ECDSA_GENERIC: u32 = 0x5044_4345;
/// `BCRYPT_RSAPUBLIC_MAGIC`, "RSA1".
const RSA_PUBLIC: u32 = 0x3141_5352;

/// The operation signing key, read once.
#[derive(Debug, Clone)]
pub enum OpSignKey {
    P256(VerifyingKey),
    Rsa(RsaPublicKey),
}

fn u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

impl OpSignKey {
    /// Reads the key as Windows gave it. `None`: no key UwULock knows.
    pub fn parse(bytes: &[u8]) -> Option<OpSignKey> {
        match u32_le(bytes, 0)? {
            ECDSA_P256 | ECDSA_GENERIC => {
                let size = usize::try_from(u32_le(bytes, 4)?).ok()?;
                if size != 32 || bytes.len() != 8 + 2 * size {
                    return None;
                }
                let mut point = vec![0x04];
                point.extend_from_slice(&bytes[8..]);
                VerifyingKey::from_sec1_bytes(&point)
                    .ok()
                    .map(OpSignKey::P256)
            }
            RSA_PUBLIC => {
                let exponent = usize::try_from(u32_le(bytes, 8)?).ok()?;
                let modulus = usize::try_from(u32_le(bytes, 12)?).ok()?;
                let body = bytes.get(24..)?;
                if body.len() < exponent + modulus || !(128..=1024).contains(&modulus) {
                    return None;
                }
                let e = BigUint::from_bytes_be(&body[..exponent]);
                let n = BigUint::from_bytes_be(&body[exponent..exponent + modulus]);
                RsaPublicKey::new(n, e).ok().map(OpSignKey::Rsa)
            }
            // DER SubjectPublicKeyInfo starts with a SEQUENCE.
            _ if bytes.first() == Some(&0x30) => {
                use p256::pkcs8::DecodePublicKey as _;
                if let Ok(key) = VerifyingKey::from_public_key_der(bytes) {
                    return Some(OpSignKey::P256(key));
                }
                RsaPublicKey::from_public_key_der(bytes)
                    .ok()
                    .map(OpSignKey::Rsa)
            }
            _ => None,
        }
    }

    /// Whether `signature` is this key's over `message`.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        match self {
            OpSignKey::P256(key) => {
                let signature =
                    Signature::from_slice(signature).or_else(|_| Signature::from_der(signature));
                signature.is_ok_and(|signature| key.verify(message, &signature).is_ok())
            }
            OpSignKey::Rsa(key) => {
                let hash = Sha256::digest(message);
                key.verify(Pkcs1v15Sign::new::<Sha256>(), &hash, signature)
                    .is_ok()
                    || key.verify(Pss::new::<Sha256>(), &hash, signature).is_ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use rand::SeedableRng;

    fn ecc_blob(key: &VerifyingKey) -> Vec<u8> {
        let point = key.to_encoded_point(false);
        let mut blob = ECDSA_P256.to_le_bytes().to_vec();
        blob.extend_from_slice(&32u32.to_le_bytes());
        blob.extend_from_slice(&point.as_bytes()[1..]);
        blob
    }

    #[test]
    fn p256_requests() {
        let signing = SigningKey::random(&mut rand::thread_rng());
        let key = OpSignKey::parse(&ecc_blob(signing.verifying_key())).unwrap();
        let request = b"\x02\xa1\x01\x6bexample.com";
        let signature: Signature = signing.sign(request);
        assert!(key.verify(request, &signature.to_bytes()));
        assert!(key.verify(request, signature.to_der().as_bytes()));
        // Another request, another key, nothing: refused.
        assert!(!key.verify(b"\x02\xa1\x01\x6bexample.org", &signature.to_bytes()));
        let other = SigningKey::random(&mut rand::thread_rng());
        let forged: Signature = other.sign(request);
        assert!(!key.verify(request, &forged.to_bytes()));
        assert!(!key.verify(request, &[]));
        assert!(!key.verify(request, &[0; 64]));
        // As DER SubjectPublicKeyInfo too.
        use p256::pkcs8::EncodePublicKey as _;
        let der = signing.verifying_key().to_public_key_der().unwrap();
        let key = OpSignKey::parse(der.as_bytes()).unwrap();
        assert!(key.verify(request, &signature.to_bytes()));
    }

    #[test]
    fn rsa_requests() {
        use rsa::traits::PublicKeyParts as _;
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let private = rsa::RsaPrivateKey::new(&mut rng, 1024).unwrap();
        let public = private.to_public_key();
        let (e, n) = (public.e().to_bytes_be(), public.n().to_bytes_be());
        let mut blob = RSA_PUBLIC.to_le_bytes().to_vec();
        for value in [1024u32, e.len() as u32, n.len() as u32, 0, 0] {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        blob.extend_from_slice(&e);
        blob.extend_from_slice(&n);
        let key = OpSignKey::parse(&blob).unwrap();
        let request = b"\x01\xa0";
        let hash = Sha256::digest(request);
        let signature = private.sign(Pkcs1v15Sign::new::<Sha256>(), &hash).unwrap();
        assert!(key.verify(request, &signature));
        assert!(!key.verify(b"\x01\xa1", &signature));
        let pss = private
            .sign_with_rng(&mut rng, Pss::new::<Sha256>(), &hash)
            .unwrap();
        assert!(key.verify(request, &pss));
    }

    #[test]
    fn unknown_keys_verify_nothing() {
        assert!(OpSignKey::parse(&[]).is_none());
        assert!(OpSignKey::parse(&[1, 2, 3, 4, 5, 6, 7, 8]).is_none());
        let mut short = ECDSA_P256.to_le_bytes().to_vec();
        short.extend_from_slice(&32u32.to_le_bytes());
        short.extend_from_slice(&[4; 10]);
        assert!(OpSignKey::parse(&short).is_none());
        // Not a point on the curve.
        let mut off = ECDSA_P256.to_le_bytes().to_vec();
        off.extend_from_slice(&32u32.to_le_bytes());
        off.extend_from_slice(&[0xff; 64]);
        assert!(OpSignKey::parse(&off).is_none());
    }
}
