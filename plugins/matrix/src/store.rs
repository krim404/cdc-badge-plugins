//! \file
//! \brief Persistence: NVS for non-secrets, rmem for secrets.
//!
//! Secrets (access token, password, recovery key, pickle key) live only in
//! named rmem slots. A write the secure element rejects is reported to the
//! caller; nothing secret is ever written to NVS. Values that an earlier
//! version left in NVS are moved into rmem on first read and erased.

use crate::model::{Config, Session};
use alloc::string::String;
use alloc::vec::Vec;
use cdc_badge_plugin::{crypto, log, nvs, random, rmem, Result};

const TAG: &str = "matrix";

const K_SRV: &str = "srv";
const K_USER: &str = "user";
const K_UID: &str = "uid";
const K_DEVICE: &str = "device";
const K_SYNCTOK: &str = "synctok";
const K_TOKEN_FB: &str = "tok_fb";
const K_PASS_FB: &str = "pass_fb";
const K_RECKEY_FB: &str = "reckey_fb";

const R_TOKEN: &str = "mtx_token";
const R_PASS: &str = "mtx_pass";
const R_RECKEY: &str = "mtx_reckey";
const R_PICKLE: &str = "mtx_pickle";

const K_PICKLE_FB: &str = "pickle_fb";
const K_ACCOUNT: &str = "acct";
const K_ACCOUNT_LEN: &str = "acct_len";
const K_MG_STATE: &str = "mg_state";
const K_MG_LEN: &str = "mg_len";

const STR_CAP: usize = 1024;
const PICKLE_KEY_LEN: usize = 32;
/// Read caps for blobs written before their length key existed.
const ACCOUNT_CAP: usize = 65536;
const MG_STATE_CAP: usize = 262144;
/// Sealed blob layout: `iv(12) || tag(16) || ciphertext`.
const SEAL_IV_LEN: usize = 12;
const SEAL_TAG_LEN: usize = 16;
const SEAL_HEADER_LEN: usize = SEAL_IV_LEN + SEAL_TAG_LEN;

fn secret_write(slot: &str, fallback_key: &str, value: &str) -> Result<()> {
    rmem::write(slot, value.as_bytes())?;
    let _ = nvs::erase(fallback_key);
    Ok(())
}

/// Read a secret from rmem; a value still in the legacy NVS key is moved into
/// rmem and erased from NVS.
fn secret_read(slot: &str, fallback_key: &str) -> Option<String> {
    if let Ok(bytes) = rmem::read(slot, STR_CAP) {
        if !bytes.is_empty() {
            return String::from_utf8(bytes).ok();
        }
    }
    let legacy = nvs::get_str(fallback_key, STR_CAP).filter(|s| !s.is_empty())?;
    if rmem::write(slot, legacy.as_bytes()).is_ok() {
        let _ = nvs::erase(fallback_key);
    }
    Some(legacy)
}

fn secret_erase(slot: &str, fallback_key: &str) {
    let _ = rmem::erase(slot);
    let _ = nvs::erase(fallback_key);
}

pub fn load_config() -> Config {
    Config {
        server: nvs::get_str(K_SRV, 256).unwrap_or_default(),
        user: nvs::get_str(K_USER, 256).unwrap_or_default(),
    }
}

pub fn save_server(value: &str) {
    let _ = nvs::set_str(K_SRV, value);
}

pub fn save_user(value: &str) {
    let _ = nvs::set_str(K_USER, value);
}

pub fn save_password(value: &str) -> Result<()> {
    secret_write(R_PASS, K_PASS_FB, value)
}

pub fn load_password() -> Option<String> {
    secret_read(R_PASS, K_PASS_FB)
}

pub fn save_recovery_key(value: &str) -> Result<()> {
    secret_write(R_RECKEY, K_RECKEY_FB, value)
}

pub fn load_recovery_key() -> Option<String> {
    secret_read(R_RECKEY, K_RECKEY_FB)
}

pub fn save_login(user_id: &str, device_id: &str, token: &str) -> Result<()> {
    let _ = nvs::set_str(K_UID, user_id);
    let _ = nvs::set_str(K_DEVICE, device_id);
    secret_write(R_TOKEN, K_TOKEN_FB, token)
}

pub fn load_device_id() -> Option<String> {
    nvs::get_str(K_DEVICE, 256).filter(|s| !s.is_empty())
}

pub fn save_sync_token(value: &str) {
    let _ = nvs::set_str(K_SYNCTOK, value);
}

/// Build a session from storage, or `None` when not logged in.
pub fn load_session() -> Option<Session> {
    let base_url = nvs::get_str(K_SRV, 256).filter(|s| !s.is_empty())?;
    let token = secret_read(R_TOKEN, K_TOKEN_FB)?;
    Some(Session {
        base_url,
        token,
        user_id: nvs::get_str(K_UID, 256).unwrap_or_default(),
        device_id: load_device_id().unwrap_or_default(),
        next_batch: nvs::get_str(K_SYNCTOK, 256).unwrap_or_default(),
    })
}

/// Wipe everything this plugin owns (NVS namespace + secret slots).
pub fn reset_all() {
    secret_erase(R_TOKEN, K_TOKEN_FB);
    secret_erase(R_PASS, K_PASS_FB);
    secret_erase(R_RECKEY, K_RECKEY_FB);
    let _ = rmem::erase(R_PICKLE);
    let _ = nvs::erase_all();
}

// --- Sealed Olm account persistence -----------------------------------------

fn key_from(bytes: &[u8]) -> Option<[u8; PICKLE_KEY_LEN]> {
    let mut k = [0u8; PICKLE_KEY_LEN];
    if bytes.len() != PICKLE_KEY_LEN {
        return None;
    }
    k.copy_from_slice(bytes);
    Some(k)
}

/// 32-byte key for at-rest sealing, held in rmem only. A stable value is
/// critical: if it ever changes, the sealed account no longer unseals and the
/// device identity is lost, so a new key is minted only when none exists. A
/// copy an earlier version mirrored into NVS is moved into rmem and erased.
/// `None` when no key is readable and none can be created.
fn pickle_key() -> Option<[u8; PICKLE_KEY_LEN]> {
    match rmem::read(R_PICKLE, PICKLE_KEY_LEN) {
        Ok(b) => {
            let k = key_from(&b);
            if k.is_some() {
                let _ = nvs::erase(K_PICKLE_FB);
            } else {
                log::error(TAG, "pickle key: slot has an unexpected size");
            }
            return k;
        }
        Err(cdc_badge_plugin::Error::NotFound) => {}
        Err(e) => {
            // A transient read error must never mint a replacement key.
            log::error(
                TAG,
                &alloc::format!("pickle key: rmem read failed: {:?}", e),
            );
            return None;
        }
    }
    if let Some(k) = nvs::get_blob(K_PICKLE_FB, PICKLE_KEY_LEN).and_then(|b| key_from(&b)) {
        if rmem::write(R_PICKLE, &k).is_ok() {
            let _ = nvs::erase(K_PICKLE_FB);
        }
        return Some(k);
    }
    let mut k = [0u8; PICKLE_KEY_LEN];
    if random::fill(&mut k).is_err() {
        log::error(TAG, "pickle key: rng failed");
        return None;
    }
    if rmem::write(R_PICKLE, &k).is_err() {
        log::error(TAG, "pickle key: rmem write failed");
        return None;
    }
    Some(k)
}

/// Seal `plain` as `iv(12) || tag(16) || ciphertext` with AES-256-GCM.
fn seal(plain: &[u8]) -> Option<Vec<u8>> {
    let key = crypto::sha256(&pickle_key()?).ok()?;
    let mut iv = [0u8; SEAL_IV_LEN];
    random::fill(&mut iv).ok()?;
    let sealed = crypto::aes_gcm_encrypt(&key, &iv, &[], plain).ok()?;
    let mut out = Vec::with_capacity(SEAL_HEADER_LEN + sealed.ciphertext.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&sealed.tag);
    out.extend_from_slice(&sealed.ciphertext);
    Some(out)
}

fn unseal(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < SEAL_HEADER_LEN {
        return None;
    }
    let key = crypto::sha256(&pickle_key()?).ok()?;
    crypto::aes_gcm_decrypt(
        &key,
        &data[..SEAL_IV_LEN],
        &[],
        &data[SEAL_HEADER_LEN..],
        &data[SEAL_IV_LEN..SEAL_HEADER_LEN],
    )
    .ok()
}

/// Store a blob together with its length so the read allocates exactly that.
fn set_sized_blob(key: &str, len_key: &str, value: &[u8]) -> Result<()> {
    nvs::set_blob(key, value)?;
    nvs::set_u32(len_key, value.len() as u32)
}

fn get_sized_blob(key: &str, len_key: &str, legacy_cap: usize) -> Option<Vec<u8>> {
    let cap = nvs::get_u32(len_key)
        .map(|n| n as usize)
        .unwrap_or(legacy_cap);
    nvs::get_blob(key, cap)
}

pub fn save_account(pickle: &[u8]) -> bool {
    let sealed = match seal(pickle) {
        Some(s) => s,
        None => {
            log::error(TAG, "save_account: seal failed");
            return false;
        }
    };
    if set_sized_blob(K_ACCOUNT, K_ACCOUNT_LEN, &sealed).is_err() {
        log::error(TAG, "save_account: nvs set_blob failed");
        return false;
    }
    true
}

pub fn load_account() -> Option<Vec<u8>> {
    let sealed = get_sized_blob(K_ACCOUNT, K_ACCOUNT_LEN, ACCOUNT_CAP)?;
    unseal(&sealed)
}

/// True when a sealed account blob exists on disk (regardless of whether it can
/// be unsealed). Lets the caller distinguish a true first run from a corrupted
/// or unreadable account so it never silently rotates the device identity.
pub fn has_account() -> bool {
    nvs::get_u32(K_ACCOUNT_LEN).is_some() || nvs::get_blob(K_ACCOUNT, ACCOUNT_CAP).is_some()
}

/// Persist the sealed Megolm/Olm session snapshot.
pub fn save_crypto_state(state: &[u8]) {
    if let Some(sealed) = seal(state) {
        let _ = set_sized_blob(K_MG_STATE, K_MG_LEN, &sealed);
    }
}

pub fn load_crypto_state() -> Option<Vec<u8>> {
    let sealed = get_sized_blob(K_MG_STATE, K_MG_LEN, MG_STATE_CAP)?;
    unseal(&sealed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_test_stubs::{nvs_has, reset, rmem_set};

    #[test]
    fn legacy_nvs_secret_moves_into_rmem() {
        reset();
        nvs::set_str(K_PASS_FB, "hunter2").unwrap();
        assert_eq!(load_password().as_deref(), Some("hunter2"));
        assert!(!nvs_has(K_PASS_FB));
        assert_eq!(rmem::read(R_PASS, STR_CAP).unwrap(), b"hunter2");
        assert_eq!(load_password().as_deref(), Some("hunter2"));
    }

    #[test]
    fn secret_write_never_falls_back_to_nvs() {
        reset();
        let long = "x".repeat(STR_CAP + 1);
        assert!(save_password(&long).is_err());
        assert!(!nvs_has(K_PASS_FB));
        assert_eq!(load_password(), None);
    }

    #[test]
    fn pickle_key_mirror_moves_into_rmem_and_is_stable() {
        reset();
        let mirrored = [7u8; PICKLE_KEY_LEN];
        nvs::set_blob(K_PICKLE_FB, &mirrored).unwrap();
        assert_eq!(pickle_key(), Some(mirrored));
        assert!(!nvs_has(K_PICKLE_FB));
        assert_eq!(pickle_key(), Some(mirrored));

        reset();
        let fresh = pickle_key().unwrap();
        assert_ne!(fresh, [0u8; PICKLE_KEY_LEN]);
        assert_eq!(pickle_key(), Some(fresh));
        assert!(!nvs_has(K_PICKLE_FB));

        reset();
        rmem_set(R_PICKLE, &mirrored);
        nvs::set_blob(K_PICKLE_FB, &[9u8; PICKLE_KEY_LEN]).unwrap();
        assert_eq!(pickle_key(), Some(mirrored));
        assert!(!nvs_has(K_PICKLE_FB));
    }

    #[test]
    fn sized_blob_round_trips_and_reads_legacy_without_length() {
        reset();
        let data = alloc::vec![1u8; 300];
        set_sized_blob(K_MG_STATE, K_MG_LEN, &data).unwrap();
        assert_eq!(get_sized_blob(K_MG_STATE, K_MG_LEN, 16), Some(data.clone()));
        nvs::erase(K_MG_LEN).unwrap();
        assert_eq!(get_sized_blob(K_MG_STATE, K_MG_LEN, 1024), Some(data));
    }
}
