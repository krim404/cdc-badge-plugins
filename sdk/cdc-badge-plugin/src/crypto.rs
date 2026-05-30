//! \file
//! \brief Software crypto primitives and binary-to-text codecs.
//!
//! Hashing, AES-256-GCM AEAD and base32/base64/hex codecs backed by the
//! firmware's mbedTLS. Random bytes live in the [`crate::random`] module;
//! asymmetric key operations live in [`crate::secure_element`].

use crate::{check, Error, Result};
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// \brief SHA-256 digest of `data`.
/// \return The 32-byte hash, or `Err` on failure.
pub fn sha256(data: &[u8]) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    check(unsafe { host_sha256(data.as_ptr(), data.len(), out.as_mut_ptr()) })?;
    Ok(out)
}

/// \brief HMAC-SHA-256 of `data` under `key`.
/// \return The 32-byte MAC, or `Err` on failure.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    check(unsafe {
        host_hmac_sha256(key.as_ptr(), key.len(), data.as_ptr(), data.len(), out.as_mut_ptr())
    })?;
    Ok(out)
}

/// \brief Ciphertext plus authentication tag from an AES-256-GCM seal.
pub struct GcmSealed {
    pub ciphertext: Vec<u8>,
    pub tag: [u8; 16],
}

/// \brief AES-256-GCM encrypt.
/// \param key       32-byte key.
/// \param iv        12-byte nonce.
/// \param aad       Additional authenticated data (may be empty).
/// \param plaintext Data to encrypt.
/// \return The ciphertext and 16-byte tag, or `Err` on bad key/iv length or
///         failure.
pub fn aes_gcm_encrypt(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<GcmSealed> {
    if key.len() != 32 || iv.len() != 12 {
        return Err(Error::InvalidArg);
    }
    let mut ct = Vec::<u8>::with_capacity(plaintext.len());
    let mut tag = [0u8; 16];
    let rc = unsafe {
        ct.set_len(plaintext.len());
        host_aes_gcm_encrypt(
            key.as_ptr(),
            iv.as_ptr(),
            aad.as_ptr(),
            aad.len(),
            plaintext.as_ptr(),
            plaintext.len(),
            ct.as_mut_ptr(),
            tag.as_mut_ptr(),
        )
    };
    check(rc)?;
    Ok(GcmSealed { ciphertext: ct, tag })
}

/// \brief AES-256-GCM decrypt and verify.
/// \param key        32-byte key.
/// \param iv         12-byte nonce.
/// \param aad        Additional authenticated data (may be empty).
/// \param ciphertext Data to decrypt.
/// \param tag        16-byte tag to verify.
/// \return The plaintext, or `Err` when the tag fails to verify or on bad
///         key/iv/tag length.
pub fn aes_gcm_decrypt(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8],
) -> Result<Vec<u8>> {
    if key.len() != 32 || iv.len() != 12 || tag.len() != 16 {
        return Err(Error::InvalidArg);
    }
    let mut pt = Vec::<u8>::with_capacity(ciphertext.len());
    let rc = unsafe {
        pt.set_len(ciphertext.len());
        host_aes_gcm_decrypt(
            key.as_ptr(),
            iv.as_ptr(),
            aad.as_ptr(),
            aad.len(),
            ciphertext.as_ptr(),
            ciphertext.len(),
            tag.as_ptr(),
            pt.as_mut_ptr(),
        )
    };
    check(rc)?;
    Ok(pt)
}

fn encode_with(
    data: &[u8],
    out_size: usize,
    f: unsafe extern "C" fn(*const u8, usize, *mut c_char, usize) -> c_int,
) -> Result<String> {
    let mut buf = Vec::<u8>::with_capacity(out_size);
    let rc = unsafe {
        buf.set_len(out_size);
        f(data.as_ptr(), data.len(), buf.as_mut_ptr() as *mut c_char, out_size)
    };
    check(rc)?;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    String::from_utf8(buf).map_err(|_| Error::Generic)
}

fn decode_with(
    text: &str,
    out_size: usize,
    f: unsafe extern "C" fn(*const c_char, usize, *mut u8, usize) -> c_int,
) -> Result<Vec<u8>> {
    let c = CString::new(text).map_err(|_| Error::InvalidArg)?;
    let mut buf = Vec::<u8>::with_capacity(out_size);
    let n = unsafe {
        buf.set_len(out_size);
        f(c.as_ptr(), text.len(), buf.as_mut_ptr(), out_size)
    };
    if n < 0 {
        return Err(Error::from_code(n));
    }
    buf.truncate(n as usize);
    Ok(buf)
}

/// \brief Base32-encode `data` (RFC 4648 alphabet, no padding).
pub fn base32_encode(data: &[u8]) -> Result<String> {
    encode_with(data, (data.len() + 4) / 5 * 8 + 1, host_base32_encode)
}

/// \brief Base32-decode `text` into raw bytes.
pub fn base32_decode(text: &str) -> Result<Vec<u8>> {
    decode_with(text, text.len(), host_base32_decode)
}

/// \brief Base64-encode `data` (standard alphabet with padding).
pub fn base64_encode(data: &[u8]) -> Result<String> {
    encode_with(data, (data.len() + 2) / 3 * 4 + 1, host_base64_encode)
}

/// \brief Base64-decode `text` into raw bytes.
pub fn base64_decode(text: &str) -> Result<Vec<u8>> {
    decode_with(text, text.len(), host_base64_decode)
}

/// \brief Lowercase-hex-encode `data`.
pub fn hex_encode(data: &[u8]) -> Result<String> {
    encode_with(data, data.len() * 2 + 1, host_hex_encode)
}

/// \brief Hex-decode `text` (case-insensitive) into raw bytes.
pub fn hex_decode(text: &str) -> Result<Vec<u8>> {
    decode_with(text, text.len() / 2, host_hex_decode)
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_sha256(data: *const u8, len: usize, out: *mut u8) -> c_int;
    fn host_hmac_sha256(key: *const u8, klen: usize, data: *const u8, dlen: usize,
                        out: *mut u8) -> c_int;
    fn host_aes_gcm_encrypt(key: *const u8, iv: *const u8, aad: *const u8, aad_len: usize,
                            pt: *const u8, pt_len: usize, ct: *mut u8, tag: *mut u8) -> c_int;
    fn host_aes_gcm_decrypt(key: *const u8, iv: *const u8, aad: *const u8, aad_len: usize,
                            ct: *const u8, ct_len: usize, tag: *const u8, pt: *mut u8) -> c_int;
    fn host_base32_encode(input: *const u8, in_len: usize, out: *mut c_char, out_size: usize) -> c_int;
    fn host_base32_decode(input: *const c_char, in_len: usize, out: *mut u8, out_size: usize) -> c_int;
    fn host_base64_encode(input: *const u8, in_len: usize, out: *mut c_char, out_size: usize) -> c_int;
    fn host_base64_decode(input: *const c_char, in_len: usize, out: *mut u8, out_size: usize) -> c_int;
    fn host_hex_encode(input: *const u8, in_len: usize, out: *mut c_char, out_size: usize) -> c_int;
    fn host_hex_decode(input: *const c_char, in_len: usize, out: *mut u8, out_size: usize) -> c_int;
}
