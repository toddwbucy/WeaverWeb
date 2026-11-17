//! conforms: web-a-certificate-is-signed-only-over-a-verified-p256-request
//!
//! **A box's certificate request, verified** (Spec 8.2, the exchange's step
//! 2): PKCS #10 in PEM, whose own signature proves the box holds the key,
//! taken as ECDSA P-256 with SHA-256 alone, the kind `mint_client` makes.
//! The `enrollment` feature carries the parser, so the connectors' build,
//! which never verifies a request, holds none (Spec 8.1).
//!
//! rcgen's parser verifies the request's signature under the curve the
//! request's key names, and names the request's algorithm by its signature
//! algorithm alone. **So the curve is checked by the key's own length**:
//! under ecdsa-with-SHA256 the parser verifies a P-256 key or a P-384 key
//! and no other, and a P-256 public half is 65 bytes, uncompressed, where a
//! P-384 half is 97.

use rcgen::{CertificateSigningRequestParams, PKCS_ECDSA_P256_SHA256, PublicKeyData};

/// The bytes of an uncompressed P-256 point: the form byte, then two
/// 32-byte coordinates.
const P256_POINT: usize = 65;

/// A request's public half, verified: the key the box holds, for a
/// certificate the authority signs, a P-256 point by its type.
pub struct Verified {
    /// The uncompressed point, checked by `verify`.
    point: Vec<u8>,
}

impl PublicKeyData for Verified {
    fn der_bytes(&self) -> &[u8] {
        &self.point
    }

    fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
        &PKCS_ECDSA_P256_SHA256
    }
}

/// **Why a request is refused**, for the console's log and never for the
/// box, which reads `request_refused` alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Not a request: not PEM, not PKCS #10, or an extension rcgen does
    /// not read.
    Malformed,
    /// The request's own signature does not verify under its key.
    Signature,
    /// Not ECDSA P-256 with SHA-256.
    Kind,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refused::Malformed => "the request is not a PKCS #10 request rcgen reads",
            Refused::Signature => "the request's signature does not verify under its key",
            Refused::Kind => "the request is not ECDSA P-256 with SHA-256",
        })
    }
}

/// **Verify one request** (Spec 8.2, step 2): its signature under its own
/// key, then its kind.
pub fn verify(pem: &str) -> Result<Verified, Refused> {
    let request = CertificateSigningRequestParams::from_pem(pem).map_err(|e| match e {
        rcgen::Error::InvalidCertificationRequestSignature => Refused::Signature,
        rcgen::Error::UnsupportedSignatureAlgorithm => Refused::Kind,
        _ => Refused::Malformed,
    })?;
    let public_key = request.public_key;
    let point = public_key.der_bytes();
    if public_key.algorithm() != &PKCS_ECDSA_P256_SHA256
        || point.len() != P256_POINT
        || point.first() != Some(&0x04)
    {
        return Err(Refused::Kind);
    }
    Ok(Verified {
        point: point.to_vec(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rcgen::{CertificateParams, KeyPair};

    fn request_der(key: &KeyPair) -> Vec<u8> {
        CertificateParams::new(Vec::<String>::new())
            .unwrap()
            .serialize_request(key)
            .unwrap()
            .der()
            .to_vec()
    }

    pub(crate) fn request_of(key: &KeyPair) -> String {
        pem_of(&request_der(key))
    }

    fn pem_of(der: &[u8]) -> String {
        use base64::Engine;
        let body = base64::engine::general_purpose::STANDARD.encode(der);
        let lines: Vec<&str> = body
            .as_bytes()
            .chunks(64)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect();
        format!(
            "-----BEGIN CERTIFICATE REQUEST-----\n{}\n-----END CERTIFICATE REQUEST-----\n",
            lines.join("\n")
        )
    }

    /// A request naming `named`'s public half and carrying another key's
    /// signature.
    pub(crate) fn signed_by_another_key(named: &KeyPair) -> String {
        let signer = KeyPair::generate().unwrap();
        let mut request = request_der(&signer);
        let theirs = signer.public_key_raw();
        let at = request
            .windows(theirs.len())
            .position(|w| w == theirs)
            .expect("the request carries its key's point");
        request[at..at + theirs.len()].copy_from_slice(named.public_key_raw());
        pem_of(&request)
    }

    #[test]
    fn a_p256_request_verifies() {
        let key = KeyPair::generate().unwrap();
        let verified = verify(&request_of(&key)).unwrap();
        assert_eq!(verified.der_bytes(), key.public_key_raw());
        assert_eq!(verified.algorithm(), &PKCS_ECDSA_P256_SHA256);
    }

    /// **A request signed by another key is refused** (Spec 9): the request
    /// names one key's public half and carries another key's signature, so
    /// it proves nothing of the key it names.
    ///
    /// Perturbation: skip the signature's verification, and it is signed.
    #[test]
    fn a_request_signed_by_another_key_is_refused() {
        let named = KeyPair::generate().unwrap();
        assert_eq!(
            verify(&signed_by_another_key(&named)).err(),
            Some(Refused::Signature),
            "a request whose signature is another key's was taken"
        );
    }

    #[test]
    fn a_p384_request_is_refused() {
        let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap();
        assert_eq!(verify(&request_of(&key)).err(), Some(Refused::Kind));
    }

    #[test]
    fn an_ed25519_request_is_refused() {
        let key = KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        assert_eq!(verify(&request_of(&key)).err(), Some(Refused::Kind));
    }

    #[test]
    fn what_is_not_a_request_is_malformed() {
        assert_eq!(verify("").err(), Some(Refused::Malformed));
        assert_eq!(
            verify(
                "-----BEGIN CERTIFICATE REQUEST-----\nAAAA\n-----END CERTIFICATE REQUEST-----\n"
            )
            .err(),
            Some(Refused::Malformed)
        );
    }
}
