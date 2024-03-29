/*!
Crypto things
*/
#![allow(dead_code)]
use ring::aead::BoundKey;
use ring::pbkdf2;

use crate::CONFIG;
use std::num::NonZeroU32;

const URL_SAFE_ENGINE: base64::engine::fast_portable::FastPortable =
    base64::engine::fast_portable::FastPortable::from(
        &base64::alphabet::URL_SAFE,
        base64::engine::fast_portable::NO_PAD,
    );

pub fn b64_encode<S: AsRef<[u8]>>(s: S) -> String {
    base64::encode_engine(s, &URL_SAFE_ENGINE)
}
pub fn b64_decode<S: AsRef<[u8]>>(s: S) -> crate::Result<Vec<u8>> {
    Ok(base64::decode_engine(s, &URL_SAFE_ENGINE).map_err(|e| {
        tracing::error!("error decoding base64 string {:?}", e);
        e
    })?)
}

/// Return a `Vec` of secure random bytes of size `n`
pub fn rand_bytes(n: usize) -> crate::Result<Vec<u8>> {
    use ring::rand::SecureRandom;
    let mut buf = vec![0; n];
    let sysrand = ring::rand::SystemRandom::new();
    sysrand
        .fill(&mut buf)
        .map_err(|_| "Error getting random bytes")?;
    Ok(buf)
}

pub fn new_pw_salt() -> crate::Result<Vec<u8>> {
    rand_bytes(128)
}

pub fn derive_password_hash(pw: &[u8], salt: &[u8]) -> [u8; ring::digest::SHA512_OUTPUT_LEN] {
    let mut out = [0; ring::digest::SHA512_OUTPUT_LEN];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA512,
        NonZeroU32::new(100_000u32).unwrap(),
        salt,
        pw,
        &mut out,
    );
    out
}

pub fn hmac_sign(s: &str) -> String {
    hmac_sign_with_key(s, &crate::CONFIG.signing_key)
}
pub fn hmac_sign_with_key(s: &str, key: &str) -> String {
    // using a 32 byte key
    let s_key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key.as_bytes());
    let tag = ring::hmac::sign(&s_key, s.as_bytes());
    hex::encode(tag)
}

pub fn hmac_verify(text: &str, sig: &str) -> bool {
    hmac_verify_with_key(text, sig, &crate::CONFIG.signing_key)
}

pub fn hmac_verify_with_key(text: &str, sig: &str, key: &str) -> bool {
    let sig = hex::decode(sig);
    let sig = if let Ok(sig) = sig {
        sig
    } else {
        return false;
    };
    // using a 32 byte key
    let s_key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key.as_bytes());
    ring::hmac::verify(&s_key, text.as_bytes(), &sig).is_ok()
}

/// ring requires an implementor of `NonceSequence`,
/// which if a wrapping trait around `ring::aead::Nonce`.
/// We have to make a wrapper that can pass ownership
/// of the nonce exactly once.
struct OneNonceSequence {
    inner: Option<ring::aead::Nonce>,
}
impl OneNonceSequence {
    fn new(inner: ring::aead::Nonce) -> Self {
        Self { inner: Some(inner) }
    }
}

impl ring::aead::NonceSequence for OneNonceSequence {
    fn advance(&mut self) -> std::result::Result<ring::aead::Nonce, ring::error::Unspecified> {
        self.inner.take().ok_or(ring::error::Unspecified)
    }
}

pub fn new_gcm_nonce() -> crate::Result<Vec<u8>> {
    rand_bytes(12)
}

/// Return the SHA256 hash of `bytes`
pub fn hash(bytes: &[u8]) -> Vec<u8> {
    let alg = &ring::digest::SHA256;
    let digest = ring::digest::digest(alg, bytes);
    Vec::from(digest.as_ref())
}

/// Encrypt `bytes` with the given `nonce` and `pass`
///
/// `bytes` are encrypted using AES_256_GCM, `nonce` is expected to be
/// 12-bytes, and `pass` 32-bytes
pub fn encrypt_bytes(
    bytes: &[u8],
    nonce: &[u8],
    pass: &[u8],
    salt: &[u8],
) -> crate::Result<Vec<u8>> {
    let alg = &ring::aead::AES_256_GCM;
    let nonce = ring::aead::Nonce::try_assume_unique_for_key(nonce)
        .map_err(|_| "Encryption nonce not unique")?;
    let nonce = OneNonceSequence::new(nonce);

    // The resulting stretched key must be the same length as the
    // encryption algorithm's key-length. For us, the alg is
    // AES_256_GCM whose key-length is 32-bytes
    let stretched = derive_password_hash(pass, salt);
    let key = ring::aead::UnboundKey::new(alg, &stretched[0..32])
        .map_err(|_| "Error building sealing key")?;
    let mut key = ring::aead::SealingKey::new(key, nonce);
    let mut in_out = bytes.to_vec();
    key.seal_in_place_append_tag(ring::aead::Aad::empty(), &mut in_out)
        .map_err(|_| "Failed encrypting bytes")?;
    Ok(in_out)
}

/// Decrypt `bytes` with the given `nonce` and `pass`
///
/// `bytes` are decrypted using AES_256_GCM, `nonce` is expected to be
/// 12-bytes, and `pass` 32-bytes
pub fn decrypt_bytes<'a>(
    bytes: &'a mut [u8],
    nonce: &[u8],
    pass: &[u8],
    salt: &[u8],
) -> crate::Result<&'a [u8]> {
    let alg = &ring::aead::AES_256_GCM;
    let nonce = ring::aead::Nonce::try_assume_unique_for_key(nonce)
        .map_err(|_| "Decryption nonce not unique")?;
    let nonce = OneNonceSequence::new(nonce);

    // The resulting stretched key must be the same length as the
    // encryption algorithm's key-length. For us, the alg is
    // AES_256_GCM whose key-length is 32-bytes
    let stretched = derive_password_hash(pass, salt);
    let key = ring::aead::UnboundKey::new(alg, &stretched[0..32])
        .map_err(|_| "Error build opening key")?;
    let mut key = ring::aead::OpeningKey::new(key, nonce);
    let out_slice = key
        .open_in_place(ring::aead::Aad::empty(), bytes)
        .map_err(|_| "Failed decrypting bytes")?;
    Ok(out_slice)
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Enc {
    pub value: String,
    pub nonce: String,
    pub salt: String,
}

pub fn encrypt(s: &str) -> crate::Result<Enc> {
    encrypt_with_key(s, &CONFIG.encryption_key)
}

pub fn encrypt_with_key(s: &str, key: &str) -> crate::Result<Enc> {
    let nonce = new_gcm_nonce().map_err(|_| "error generating nonce")?;
    let salt = new_pw_salt().map_err(|_| "error generating salt")?;
    let b = encrypt_bytes(s.as_bytes(), &nonce, key.as_bytes(), &salt)
        .map_err(|_| "encryption error")?;

    let value = b64_encode(b);
    let nonce = b64_encode(&nonce);
    let salt = b64_encode(&salt);
    Ok(Enc { value, nonce, salt })
}

pub fn decrypt(enc: &Enc) -> crate::Result<String> {
    decrypt_with_key(enc, &CONFIG.encryption_key)
}

pub fn decrypt_with_key(enc: &Enc, key: &str) -> crate::Result<String> {
    let nonce = b64_decode(&enc.nonce).map_err(|_| "nonce base64 decode error")?;
    let salt = b64_decode(&enc.salt).map_err(|_| "salt base64 decode error")?;
    let mut value = b64_decode(&enc.value).map_err(|_| "value base64 decode error")?;
    let bytes = decrypt_bytes(value.as_mut_slice(), &nonce, key.as_bytes(), &salt)
        .map_err(|_| "encryption error")?;
    let s = String::from_utf8(bytes.to_owned()).map_err(|_| "error decrypting bytes")?;
    Ok(s)
}

#[test]
fn test_encrypt_decrypt() {
    assert_eq!(decrypt(&encrypt("test").unwrap()).unwrap(), "test");
}
