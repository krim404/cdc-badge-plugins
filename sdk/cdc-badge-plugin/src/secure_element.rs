//! \file
//! \brief TROPIC01 ECC keys (addressed by name) and chip identity.
//!
//! Plugin ECC keys are referenced by name, like [`crate::rmem`]; the host maps
//! each declared name (manifest `capabilities.ecc`) to a slot in a reserved
//! plugin ECC pool and persists the mapping. Private keys never leave the
//! chip; only public keys and signatures are returned.

use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// \brief Maximum ECC key-name length, excluding the trailing NUL.
pub const ECC_NAME_MAX: usize = 15;

/// \brief ECC curve selector for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    P256,
    Ed25519,
}

impl Curve {
    fn as_u8(self) -> u8 {
        match self {
            Curve::P256 => 0,
            Curve::Ed25519 => 1,
        }
    }

    /// \brief Raw public-key length in bytes for this curve.
    fn pubkey_len(self) -> usize {
        match self {
            Curve::P256 => 64,
            Curve::Ed25519 => 32,
        }
    }
}

/// \brief Generic secure-element error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct SeError;

/// \brief Generate a fresh ECC key under `name`, claiming a plugin pool slot.
///
/// The name must be declared in the manifest's `capabilities.ecc`.
/// \param name  Key name, 1-[`ECC_NAME_MAX`] chars.
/// \param curve Curve for the new key.
/// \return `Ok(())` on success, `Err(SeError)` on denial, a full pool, or
///         hardware failure.
pub fn generate(name: &str, curve: Curve) -> Result<(), SeError> {
    let c = CString::new(name).map_err(|_| SeError)?;
    rc(unsafe { host_ecc_generate(c.as_ptr(), curve.as_u8()) })
}

/// \brief Import an externally-generated private key under `name`.
///
/// Currently rejected by the firmware (private keys are generated on-chip).
pub fn import(name: &str, priv_key: &[u8], curve: Curve) -> Result<(), SeError> {
    let c = CString::new(name).map_err(|_| SeError)?;
    rc(unsafe { host_ecc_import(c.as_ptr(), priv_key.as_ptr(), curve.as_u8()) })
}

/// \brief Export the public key for `name`.
/// \param name  Key name declared in `capabilities.ecc`.
/// \param curve Curve of the stored key (selects the output length).
/// \return The raw public key, or `Err(SeError)` on failure.
pub fn pubkey(name: &str, curve: Curve) -> Result<Vec<u8>, SeError> {
    let c = CString::new(name).map_err(|_| SeError)?;
    let mut buf = Vec::<u8>::with_capacity(curve.pubkey_len());
    let r = unsafe {
        buf.set_len(curve.pubkey_len());
        host_ecc_pubkey(c.as_ptr(), buf.as_mut_ptr(), curve.as_u8())
    };
    rc(r)?;
    Ok(buf)
}

/// \brief Erase the named ECC key and free its pool slot.
/// \param name Key name declared in `capabilities.ecc`.
pub fn delete(name: &str) -> Result<(), SeError> {
    let c = CString::new(name).map_err(|_| SeError)?;
    rc(unsafe { host_ecc_delete(c.as_ptr()) })
}

/// \brief Whether the named ECC key currently holds key material.
/// \param name Key name declared in `capabilities.ecc`.
pub fn exists(name: &str) -> bool {
    let Ok(c) = CString::new(name) else { return false };
    unsafe { host_ecc_exists(c.as_ptr()) != 0 }
}

/// \brief ECDSA-sign `msg` with the P-256 key `name`.
/// \return The 64-byte raw signature, or `Err(SeError)` on failure.
pub fn ecdsa_sign(name: &str, msg: &[u8]) -> Result<[u8; 64], SeError> {
    let c = CString::new(name).map_err(|_| SeError)?;
    let mut sig = [0u8; 64];
    rc(unsafe { host_ecdsa_sign(c.as_ptr(), msg.as_ptr(), msg.len(), sig.as_mut_ptr()) })?;
    Ok(sig)
}

/// \brief Ed25519-sign `msg` with the key `name`.
/// \return The 64-byte signature, or `Err(SeError)` on failure.
pub fn eddsa_sign(name: &str, msg: &[u8]) -> Result<[u8; 64], SeError> {
    let c = CString::new(name).map_err(|_| SeError)?;
    let mut sig = [0u8; 64];
    rc(unsafe { host_eddsa_sign(c.as_ptr(), msg.as_ptr(), msg.len(), sig.as_mut_ptr()) })?;
    Ok(sig)
}

/// \brief Read the TROPIC01 chip serial / identity blob.
/// \param max Capacity to request from the chip.
/// \return The identity bytes, or `Err(SeError)` on failure.
pub fn chip_id(max: usize) -> Result<Vec<u8>, SeError> {
    let mut buf = Vec::<u8>::with_capacity(max);
    let mut len = max;
    let r = unsafe {
        buf.set_len(max);
        host_se_chip_id(buf.as_mut_ptr(), &mut len)
    };
    rc(r)?;
    Ok(buf)
}

/// \brief Read the TROPIC01 firmware versions.
/// \return `(riscv, spect)` 4-byte version tuples, or `Err(SeError)`.
pub fn fw_version() -> Result<([u8; 4], [u8; 4]), SeError> {
    let mut riscv = [0u8; 4];
    let mut spect = [0u8; 4];
    rc(unsafe { host_se_fw_version(riscv.as_mut_ptr(), spect.as_mut_ptr()) })?;
    Ok((riscv, spect))
}

fn rc(code: c_int) -> Result<(), SeError> {
    if code == 0 {
        Ok(())
    } else {
        Err(SeError)
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_ecc_generate(name: *const c_char, curve: u8) -> c_int;
    fn host_ecc_import(name: *const c_char, priv_key: *const u8, curve: u8) -> c_int;
    fn host_ecc_pubkey(name: *const c_char, pub_key: *mut u8, curve: u8) -> c_int;
    fn host_ecc_delete(name: *const c_char) -> c_int;
    fn host_ecc_exists(name: *const c_char) -> c_int;
    fn host_ecdsa_sign(name: *const c_char, msg: *const u8, len: usize, sig: *mut u8) -> c_int;
    fn host_eddsa_sign(name: *const c_char, msg: *const u8, len: usize, sig: *mut u8) -> c_int;
    fn host_se_chip_id(serial: *mut u8, len: *mut usize) -> c_int;
    fn host_se_fw_version(riscv: *mut u8, spect: *mut u8) -> c_int;
}
