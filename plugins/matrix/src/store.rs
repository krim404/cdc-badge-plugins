//! \file
//! \brief Persistence: NVS for non-secrets, rmem for secrets.
//!
//! Secrets (access token, password, recovery key) go to named rmem slots.
//! Access tokens can exceed a small rmem slot, so writes that the secure
//! element rejects fall back to NVS and reads check both.

use crate::model::{Config, Session};
use cdc_badge_plugin::{crypto, log, nvs, random, rmem};
use alloc::string::String;
use alloc::vec::Vec;

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
const K_MG_STATE: &str = "mg_state";

const STR_CAP: usize = 1024;

fn secret_write(slot: &str, fallback_key: &str, value: &str) {
    if rmem::write(slot, value.as_bytes()).is_err() {
        let _ = nvs::set_str(fallback_key, value);
    } else {
        let _ = nvs::erase(fallback_key);
    }
}

fn secret_read(slot: &str, fallback_key: &str) -> Option<String> {
    if let Ok(bytes) = rmem::read(slot, STR_CAP) {
        if !bytes.is_empty() {
            return String::from_utf8(bytes).ok();
        }
    }
    nvs::get_str(fallback_key, STR_CAP).filter(|s| !s.is_empty())
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

pub fn save_password(value: &str) {
    secret_write(R_PASS, K_PASS_FB, value);
}

pub fn load_password() -> Option<String> {
    secret_read(R_PASS, K_PASS_FB)
}

pub fn save_recovery_key(value: &str) {
    secret_write(R_RECKEY, K_RECKEY_FB, value);
}

pub fn load_recovery_key() -> Option<String> {
    secret_read(R_RECKEY, K_RECKEY_FB)
}

pub fn save_login(user_id: &str, device_id: &str, token: &str) {
    let _ = nvs::set_str(K_UID, user_id);
    let _ = nvs::set_str(K_DEVICE, device_id);
    secret_write(R_TOKEN, K_TOKEN_FB, token);
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
        device_id: nvs::get_str(K_DEVICE, 256).unwrap_or_default(),
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

/// 32-byte key for at-rest sealing. A stable value is critical: if it ever
/// changes, the sealed account no longer unseals and the device identity is
/// lost. The key is mirrored in both rmem and NVS, and a new one is minted only
/// when BOTH copies are absent - a transient rmem read failure must never rotate
/// it.
fn pickle_key() -> [u8; 32] {
    if let Ok(b) = rmem::read(R_PICKLE, 32) {
        if b.len() == 32 {
            let mut k = [0u8; 32];
            k.copy_from_slice(&b);
            // Backfill the NVS mirror so a later transient rmem read cannot leave
            // the key with no readable copy.
            if nvs::get_blob(K_PICKLE_FB, 32).map(|v| v.len() != 32).unwrap_or(true) {
                let _ = nvs::set_blob(K_PICKLE_FB, &k);
            }
            return k;
        }
    }
    if let Some(b) = nvs::get_blob(K_PICKLE_FB, 32) {
        if b.len() == 32 {
            let mut k = [0u8; 32];
            k.copy_from_slice(&b);
            let _ = rmem::write(R_PICKLE, &k);
            return k;
        }
    }
    let mut k = [0u8; 32];
    let _ = random::fill(&mut k);
    let _ = rmem::write(R_PICKLE, &k);
    let _ = nvs::set_blob(K_PICKLE_FB, &k);
    k
}

/// Seal `plain` as `iv(12) || tag(16) || ciphertext` with AES-256-GCM.
fn seal(plain: &[u8]) -> Option<Vec<u8>> {
    let key = crypto::sha256(&pickle_key()).ok()?;
    let mut iv = [0u8; 12];
    random::fill(&mut iv).ok()?;
    let sealed = crypto::aes_gcm_encrypt(&key, &iv, &[], plain).ok()?;
    let mut out = Vec::with_capacity(28 + sealed.ciphertext.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&sealed.tag);
    out.extend_from_slice(&sealed.ciphertext);
    Some(out)
}

fn unseal(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 28 {
        return None;
    }
    let key = crypto::sha256(&pickle_key()).ok()?;
    crypto::aes_gcm_decrypt(&key, &data[0..12], &[], &data[28..], &data[12..28]).ok()
}

pub fn save_account(pickle: &[u8]) -> bool {
    let sealed = match seal(pickle) {
        Some(s) => s,
        None => {
            log::error(TAG, "save_account: seal failed");
            return false;
        }
    };
    if nvs::set_blob(K_ACCOUNT, &sealed).is_err() {
        log::error(TAG, "save_account: nvs set_blob failed");
        return false;
    }
    true
}

pub fn load_account() -> Option<Vec<u8>> {
    // The account pickle grows with published one-time keys (up to ~100), so the
    // read cap must exceed that; too small a cap makes the blob read fail, the
    // account look absent, and a fresh identity get minted - which rotates the
    // device's curve25519 and breaks every existing session.
    let sealed = nvs::get_blob(K_ACCOUNT, 65536)?;
    unseal(&sealed)
}

/// True when a sealed account blob exists on disk (regardless of whether it can
/// be unsealed). Lets the caller distinguish a true first run from a corrupted
/// or unreadable account so it never silently rotates the device identity.
pub fn has_account() -> bool {
    nvs::get_blob(K_ACCOUNT, 65536).is_some()
}

/// Persist the sealed Megolm/Olm session snapshot.
pub fn save_crypto_state(state: &[u8]) {
    if let Some(sealed) = seal(state) {
        let _ = nvs::set_blob(K_MG_STATE, &sealed);
    }
}

pub fn load_crypto_state() -> Option<Vec<u8>> {
    // Must exceed the sealed snapshot of all inbound room keys + per-device Olm
    // sessions; too small a cap makes the read fail, so `have_sessions` is always
    // false and the heavy key-backup re-import runs on every open (flash wear +
    // UI stalls).
    let sealed = nvs::get_blob(K_MG_STATE, 262144)?;
    unseal(&sealed)
}
