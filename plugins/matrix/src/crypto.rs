//! \file
//! \brief E2EE via vodozemac.
//!
//! Receive (Phase 2): decrypt the server-side key backup
//! (`m.megolm_backup.v1.curve25519-aes-sha2`) with the Matrix recovery key,
//! import the Megolm sessions, and decrypt room timeline events.
//!
//! Send (Phase 3): a persisted Olm [`Account`] provides the device identity and
//! one-time keys; per-room Megolm [`GroupSession`]s encrypt outgoing messages,
//! and per-device Olm [`Session`]s wrap the `m.room_key` for to-device delivery.
//! Trust-on-first-use: no device verification or cross-signing.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use vodozemac::megolm::{
    ExportedSessionKey, GroupSession, InboundGroupSession, InboundGroupSessionPickle, MegolmMessage,
    SessionConfig, SessionKey,
};
use vodozemac::olm::{
    Account, AccountPickle, OlmMessage, Session, SessionConfig as OlmSessionConfig, SessionPickle,
};
use vodozemac::pk_encryption::{Message, PkDecryption, PkEncryption};
use vodozemac::{Curve25519PublicKey, Curve25519SecretKey};

use crate::json::json_escape;

/// Sealed-NVS snapshot. Persists the inbound (received) room keys plus the
/// outbound Megolm inbound-counterparts (so the sender can decrypt its own
/// messages after a reboot) and the per-device Olm sessions (so a reboot does
/// not re-run an X25519 handshake per device). Outbound Megolm sessions stay in
/// RAM and rotate on reboot.
#[derive(Serialize, Deserialize, Default)]
struct CryptoState {
    inbound: BTreeMap<String, InboundGroupSessionPickle>,
    #[serde(default)]
    olm: BTreeMap<String, Vec<SessionPickle>>,
}

/// Bitcoin base58 alphabet, used by the Matrix recovery/security key encoding.
const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Decode a base58 string (no checksum) into bytes.
pub fn base58_decode(s: &str) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new(); // little-endian during accumulation
    for c in s.bytes() {
        let val = B58.iter().position(|&a| a == c)? as u32;
        let mut carry = val;
        for b in out.iter_mut() {
            carry += (*b as u32) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            out.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    for c in s.bytes() {
        if c == b'1' {
            out.push(0);
        } else {
            break;
        }
    }
    out.reverse();
    Some(out)
}

/// Decode a Matrix recovery key (base58, `0x8B 0x01` prefix + 32-byte key +
/// XOR parity byte) into the raw 32-byte Curve25519 backup secret.
pub fn parse_recovery_key(s: &str) -> Option<[u8; 32]> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base58_decode(&cleaned)?;
    if bytes.len() != 35 || bytes[0] != 0x8b || bytes[1] != 0x01 {
        return None;
    }
    if bytes.iter().fold(0u8, |acc, &b| acc ^ b) != 0 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[2..34]);
    Some(key)
}

/// HKDF-SHA256 producing 64 bytes (AES key || MAC key), built from the host
/// HMAC primitive. Used by the SSSS secret-storage scheme.
fn hkdf_sha256_64(ikm: &[u8], salt: &[u8; 32], info: &[u8]) -> Option<[u8; 64]> {
    use cdc_badge_plugin::crypto::hmac_sha256;
    let prk = hmac_sha256(salt, ikm).ok()?; // extract
    let mut t1_in = Vec::with_capacity(info.len() + 1);
    t1_in.extend_from_slice(info);
    t1_in.push(0x01);
    let t1 = hmac_sha256(&prk, &t1_in).ok()?;
    let mut t2_in = Vec::with_capacity(32 + info.len() + 1);
    t2_in.extend_from_slice(&t1);
    t2_in.extend_from_slice(info);
    t2_in.push(0x02);
    let t2 = hmac_sha256(&prk, &t2_in).ok()?;
    let mut okm = [0u8; 64];
    okm[..32].copy_from_slice(&t1);
    okm[32..].copy_from_slice(&t2);
    Some(okm)
}

/// Decrypt the `m.megolm_backup.v1` SSSS secret to recover the 32-byte backup
/// curve25519 private key.
///
/// `ssss_key` is the raw 32 bytes from the Element recovery/security key. The
/// secret uses `m.secret_storage.v1.aes-hmac-sha2`: HKDF-SHA256 (info = secret
/// name) -> AES-256-CTR key + HMAC-SHA256 key; the MAC covers the ciphertext.
pub fn ssss_decrypt_backup_key(
    ssss_key: &[u8; 32],
    ciphertext_b64: &str,
    iv_b64: &str,
    mac_b64: &str,
) -> Option<[u8; 32]> {
    use aes::Aes256;
    use cdc_badge_plugin::crypto::{base64_decode, hmac_sha256};
    use ctr::cipher::{KeyIvInit, StreamCipher};

    let okm = hkdf_sha256_64(ssss_key, &[0u8; 32], b"m.megolm_backup.v1")?;
    let aes_key = &okm[..32];
    let mac_key = &okm[32..];

    let mut ct = base64_decode(ciphertext_b64).ok()?;
    let iv = base64_decode(iv_b64).ok()?;
    let expected_mac = base64_decode(mac_b64).ok()?;
    if iv.len() != 16 {
        return None;
    }

    // Encrypt-then-MAC: the MAC covers the ciphertext bytes.
    let mac = hmac_sha256(mac_key, &ct).ok()?;
    if mac.len() != expected_mac.len() || mac != expected_mac.as_slice() {
        return None;
    }

    let mut cipher = ctr::Ctr128BE::<Aes256>::new(aes_key.into(), iv.as_slice().into());
    cipher.apply_keystream(&mut ct);

    // Plaintext is the base64 of the 32-byte backup private key.
    let key_b64 = String::from_utf8(ct).ok()?;
    let key_bytes = base64_decode(key_b64.trim()).ok()?;
    if key_bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&key_bytes);
    Some(out)
}

/// Cap on retained Olm sessions per peer device: enough to decrypt recent
/// messages and keep the send session valid, while bounding the persisted state.
const MAX_OLM_SESSIONS_PER_PEER: usize = 3;

/// Olm/Megolm state: device account, backup decryptor, and live sessions.
pub struct Crypto {
    account: Account,
    backup: Option<PkDecryption>,
    inbound: BTreeMap<String, InboundGroupSession>, // session_id -> inbound megolm
    outbound: BTreeMap<String, GroupSession>,       // room_id -> outbound megolm
    olm: BTreeMap<String, Vec<Session>>,            // device curve25519 -> olm sessions
}

impl Crypto {
    /// Create a brand-new device account.
    pub fn create() -> Self {
        Self::new(Account::new())
    }

    pub fn new(account: Account) -> Self {
        Self {
            account,
            backup: None,
            inbound: BTreeMap::new(),
            outbound: BTreeMap::new(),
            olm: BTreeMap::new(),
        }
    }

    /// Restore a persisted account, or `None` if the pickle is unreadable.
    pub fn from_account_pickle(bytes: &[u8]) -> Option<Self> {
        let pickle: AccountPickle = serde_json::from_slice(bytes).ok()?;
        Some(Self::new(Account::from_pickle(pickle)))
    }

    /// Serialize the account for sealed persistence.
    pub fn account_pickle(&self) -> Vec<u8> {
        serde_json::to_vec(&self.account.pickle()).unwrap_or_default()
    }

    /// Serialize the inbound room keys for sealed persistence, so a reboot does
    /// not have to re-fetch the whole key backup.
    pub fn export_state(&self) -> Vec<u8> {
        let state = CryptoState {
            inbound: self.inbound.iter().map(|(k, v)| (k.clone(), v.pickle())).collect(),
            olm: self
                .olm
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().map(|s| s.pickle()).collect()))
                .collect(),
        };
        serde_json::to_vec(&state).unwrap_or_default()
    }

    /// Restore inbound room keys and Olm sessions from a previously
    /// [`export_state`]ed snapshot.
    pub fn import_state(&mut self, bytes: &[u8]) {
        if let Ok(state) = serde_json::from_slice::<CryptoState>(bytes) {
            for (id, pickle) in state.inbound {
                self.inbound.insert(id, InboundGroupSession::from_pickle(pickle));
            }
            for (curve, pickles) in state.olm {
                self.olm
                    .insert(curve, pickles.into_iter().map(Session::from_pickle).collect());
            }
        }
    }

    /// Enable the backup receive path from the raw 32-byte Megolm backup
    /// curve25519 private key (obtained via [`ssss_decrypt_backup_key`]).
    pub fn set_backup_key(&mut self, key: &[u8; 32]) {
        self.backup = Some(PkDecryption::from_key(Curve25519SecretKey::from_slice(key)));
    }

    pub fn has_backup(&self) -> bool {
        self.backup.is_some()
    }

    /// Encrypt one inbound session for the server-side key backup
    /// (`m.megolm_backup.v1.curve25519-aes-sha2`). Returns the
    /// `(ciphertext, mac, ephemeral)` triplet (unpadded base64) for the backup
    /// `session_data`, or `None` when the session is unknown or no backup key is
    /// armed. Lets a device that was offline at send time restore the room key.
    pub fn backup_session_blob(&self, session_id: &str) -> Option<(String, String, String)> {
        let backup = self.backup.as_ref()?;
        let inbound = self.inbound.get(session_id)?;
        let exported = inbound.export_at_first_known_index().to_base64();
        let session_data = format!(
            "{{\"algorithm\":\"m.megolm.v1.aes-sha2\",\"forwarding_curve25519_key_chain\":[],\"sender_claimed_keys\":{{\"ed25519\":\"{ed}\"}},\"sender_key\":\"{ck}\",\"session_key\":\"{sk}\"}}",
            ed = json_escape(&self.identity_ed25519()),
            ck = json_escape(&self.identity_curve25519()),
            sk = json_escape(&exported),
        );
        let message = PkEncryption::from(backup).encrypt(session_data.as_bytes()).ok()?;
        Some((
            cdc_badge_plugin::crypto::base64_encode(&message.ciphertext).ok()?,
            cdc_badge_plugin::crypto::base64_encode(&message.mac).ok()?,
            message.ephemeral_key.to_base64(),
        ))
    }

    pub fn session_count(&self) -> usize {
        self.inbound.len()
    }

    pub fn identity_curve25519(&self) -> String {
        self.account.curve25519_key().to_base64()
    }

    pub fn identity_ed25519(&self) -> String {
        self.account.ed25519_key().to_base64()
    }

    // --- Receive (backup) -------------------------------------------------

    /// Decrypt one backed-up session blob and import its Megolm session.
    pub fn import_backup_session(
        &mut self,
        session_id: &str,
        ciphertext: &str,
        mac: &str,
        ephemeral: &str,
    ) -> bool {
        let backup = match self.backup.as_ref() {
            Some(b) => b,
            None => return false,
        };
        let message = match Message::from_base64(ciphertext, mac, ephemeral) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let plain = match backup.decrypt(&message) {
            Ok(p) => p,
            Err(_) => return false,
        };
        let key_b64 = match session_key_from_backup(&plain) {
            Some(k) => k,
            None => return false,
        };
        let exported = match ExportedSessionKey::from_base64(&key_b64) {
            Ok(e) => e,
            Err(_) => return false,
        };
        self.inbound.insert(
            session_id.to_string(),
            InboundGroupSession::import(&exported, SessionConfig::version_1()),
        );
        true
    }

    /// Decrypt a Megolm timeline event; returns the plaintext message body.
    pub fn decrypt_megolm(&mut self, session_id: &str, ciphertext_b64: &str) -> Option<String> {
        let session = self.inbound.get_mut(session_id)?;
        let message = MegolmMessage::from_base64(ciphertext_b64).ok()?;
        let decrypted = session.decrypt(&message).ok()?;
        body_from_event(&decrypted.plaintext)
    }

    // --- Send -------------------------------------------------------------

    /// Canonical-JSON `device_keys` object, signed with our Ed25519 key.
    pub fn device_keys_json(&self, user_id: &str, device_id: &str) -> String {
        let curve = self.identity_curve25519();
        let ed = self.identity_ed25519();
        // Keys must be in canonical (lexicographic) order for the signature.
        let signed = format!(
            "{{\"algorithms\":[\"m.olm.v1.curve25519-aes-sha2\",\"m.megolm.v1.aes-sha2\"],\"device_id\":\"{dev}\",\"keys\":{{\"curve25519:{dev}\":\"{curve}\",\"ed25519:{dev}\":\"{ed}\"}},\"user_id\":\"{user}\"}}",
            dev = json_escape(device_id),
            curve = json_escape(&curve),
            ed = json_escape(&ed),
            user = json_escape(user_id),
        );
        let sig = self.account.sign(signed.as_bytes()).to_base64();
        // Re-emit with the signatures block appended.
        format!(
            "{{\"algorithms\":[\"m.olm.v1.curve25519-aes-sha2\",\"m.megolm.v1.aes-sha2\"],\"device_id\":\"{dev}\",\"keys\":{{\"curve25519:{dev}\":\"{curve}\",\"ed25519:{dev}\":\"{ed}\"}},\"signatures\":{{\"{user}\":{{\"ed25519:{dev}\":\"{sig}\"}}}},\"user_id\":\"{user}\"}}",
            dev = json_escape(device_id),
            curve = json_escape(&curve),
            ed = json_escape(&ed),
            user = json_escape(user_id),
            sig = json_escape(&sig),
        )
    }

    /// Generate `count` one-time keys and return the signed `one_time_keys`
    /// object for `/keys/upload`, marking them published.
    pub fn one_time_keys_json(&mut self, user_id: &str, device_id: &str, count: usize) -> String {
        self.account.generate_one_time_keys(count);
        let mut entries: Vec<String> = Vec::new();
        for (key_id, key) in self.account.one_time_keys() {
            let key_b64 = key.to_base64();
            let signed = format!("{{\"key\":\"{}\"}}", json_escape(&key_b64));
            let sig = self.account.sign(signed.as_bytes()).to_base64();
            entries.push(format!(
                "\"signed_curve25519:{}\":{{\"key\":\"{}\",\"signatures\":{{\"{}\":{{\"ed25519:{}\":\"{}\"}}}}}}",
                json_escape(&key_id.to_base64()),
                json_escape(&key_b64),
                json_escape(user_id),
                json_escape(device_id),
                json_escape(&sig),
            ));
        }
        self.account.mark_keys_as_published();
        format!("{{{}}}", entries.join(","))
    }

    /// True when at least one Olm session with this device exists.
    pub fn has_olm_session(&self, device_curve25519: &str) -> bool {
        self.olm.get(device_curve25519).map_or(false, |v| !v.is_empty())
    }

    /// Establish an outbound Olm session to a device from its claimed OTK,
    /// appending it to that device's session list (never replacing sessions we
    /// may still need to decrypt incoming messages with).
    pub fn establish_olm(&mut self, device_curve25519: &str, one_time_key: &str) -> bool {
        let identity = match Curve25519PublicKey::from_base64(device_curve25519) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let otk = match Curve25519PublicKey::from_base64(one_time_key) {
            Ok(k) => k,
            Err(_) => return false,
        };
        match self
            .account
            .create_outbound_session(OlmSessionConfig::version_1(), identity, otk)
        {
            Ok(session) => {
                let v = self.olm.entry(device_curve25519.to_string()).or_default();
                v.push(session);
                while v.len() > MAX_OLM_SESSIONS_PER_PEER {
                    v.remove(0);
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Olm-encrypt `plaintext` for a device; returns `(message_type, body_b64)`.
    /// The base64 of the ciphertext is done by the host (offloads it from wasm).
    pub fn olm_encrypt(&mut self, device_curve25519: &str, plaintext: &str) -> Option<(u8, String)> {
        // Reuse this device's current send session (the most recently established).
        let session = self.olm.get_mut(device_curve25519)?.last_mut()?;
        let message = session.encrypt(plaintext.as_bytes()).ok()?;
        let (kind, bytes) = message.to_parts();
        let body = cdc_badge_plugin::crypto::base64_encode(&bytes).ok()?;
        Some((kind as u8, body))
    }

    /// Decrypt an incoming Olm to-device message from `sender_curve25519`. A
    /// pre-key message (`msg_type == 0`) creates a fresh inbound Olm session; a
    /// normal message (`msg_type == 1`) advances the existing one. Returns the
    /// decrypted to-device event JSON, or `None` on any failure.
    pub fn olm_decrypt(&mut self, sender_curve25519: &str, msg_type: u8, body_b64: &str) -> Option<String> {
        let sender = Curve25519PublicKey::from_base64(sender_curve25519).ok()?;
        let bytes = cdc_badge_plugin::crypto::base64_decode(body_b64).ok()?;
        let message = OlmMessage::from_parts(msg_type as usize, &bytes).ok()?;
        // Try every existing session for this peer first, so a received message
        // never clobbers the session we use for sending. Only a pre-key that
        // matches no existing session creates a new one, appended to the list.
        if let Some(sessions) = self.olm.get_mut(sender_curve25519) {
            for s in sessions.iter_mut() {
                if let Ok(pt) = s.decrypt(&message) {
                    return String::from_utf8(pt).ok();
                }
            }
        }
        if let OlmMessage::PreKey(pre) = &message {
            let result = self
                .account
                .create_inbound_session(OlmSessionConfig::version_1(), sender, pre)
                .ok()?;
            let plaintext = result.plaintext;
            let v = self.olm.entry(sender_curve25519.to_string()).or_default();
            v.push(result.session);
            while v.len() > MAX_OLM_SESSIONS_PER_PEER {
                v.remove(0);
            }
            return String::from_utf8(plaintext).ok();
        }
        None
    }

    /// Import a Megolm room key (received via to-device) under its `session_id`
    /// so timeline events encrypted with it become decryptable. Returns false on
    /// a malformed key.
    pub fn import_room_key(&mut self, session_id: &str, session_key_b64: &str) -> bool {
        let key = match SessionKey::from_base64(session_key_b64) {
            Ok(k) => k,
            Err(_) => return false,
        };
        self.inbound
            .insert(session_id.to_string(), InboundGroupSession::new(&key, SessionConfig::version_1()));
        true
    }

    /// Get or create the outbound Megolm session for a room; returns
    /// `(session_id, session_key_b64, is_new)`. When `is_new` is false the room
    /// key was already distributed this boot and need not be sent again.
    pub fn ensure_outbound(&mut self, room_id: &str) -> (String, String, bool) {
        let is_new = !self.outbound.contains_key(room_id);
        let session = self
            .outbound
            .entry(room_id.to_string())
            .or_insert_with(|| GroupSession::new(SessionConfig::version_1()));
        let session_id = session.session_id();
        let session_key = session.session_key();
        if is_new {
            // Keep the matching inbound session locally so the sender can decrypt
            // (and persist/back up) its own messages; recipients import the same
            // key via the to-device room-key distribution.
            let inbound = InboundGroupSession::new(&session_key, SessionConfig::version_1());
            self.inbound.insert(session_id.clone(), inbound);
        }
        (session_id, session_key.to_base64(), is_new)
    }

    /// Megolm-encrypt an event for a room; returns the ciphertext base64.
    pub fn encrypt_megolm(&mut self, room_id: &str, plaintext: &str) -> Option<String> {
        let session = self.outbound.get_mut(room_id)?;
        Some(session.encrypt(plaintext.as_bytes()).to_base64())
    }
}

fn session_key_from_backup(plain: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(plain).ok()?;
    Some(v.get("session_key")?.as_str()?.to_string())
}

fn body_from_event(plain: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(plain).ok()?;
    Some(v.get("content")?.get("body")?.as_str()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base58_encode(data: &[u8]) -> String {
        let mut digits: Vec<u8> = Vec::new();
        for &b in data {
            let mut carry = b as u32;
            for d in digits.iter_mut() {
                carry += (*d as u32) << 8;
                *d = (carry % 58) as u8;
                carry /= 58;
            }
            while carry > 0 {
                digits.push((carry % 58) as u8);
                carry /= 58;
            }
        }
        let mut out = String::new();
        for &b in data {
            if b == 0 {
                out.push('1');
            } else {
                break;
            }
        }
        for &d in digits.iter().rev() {
            out.push(B58[d as usize] as char);
        }
        out
    }

    fn encode_recovery_key(key: &[u8; 32]) -> String {
        let mut bytes = alloc::vec![0x8bu8, 0x01];
        bytes.extend_from_slice(key);
        let parity = bytes.iter().fold(0u8, |acc, &b| acc ^ b);
        bytes.push(parity);
        base58_encode(&bytes)
    }

    #[test]
    fn base58_roundtrip() {
        let data = b"\x00\x00hello base58";
        assert_eq!(base58_decode(&base58_encode(data)).unwrap(), data);
    }

    #[test]
    fn recovery_key_roundtrips() {
        let key = [
            1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32,
        ];
        let encoded = encode_recovery_key(&key);
        assert_eq!(parse_recovery_key(&encoded), Some(key));
        let spaced: String = encoded
            .as_bytes()
            .chunks(4)
            .map(|c| core::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(parse_recovery_key(&spaced), Some(key));
    }

    #[test]
    fn recovery_key_rejects_bad_parity() {
        let key = [7u8; 32];
        let mut encoded = encode_recovery_key(&key).into_bytes();
        let last = encoded.len() - 1;
        encoded[last] = if encoded[last] == b'A' { b'B' } else { b'A' };
        let s = String::from_utf8(encoded).unwrap();
        assert_eq!(parse_recovery_key(&s), None);
    }

    #[test]
    fn megolm_import_then_decrypt() {
        let mut group = GroupSession::new(SessionConfig::version_1());
        let session_id = group.session_id();
        let exported = InboundGroupSession::new(&group.session_key(), SessionConfig::version_1())
            .export_at_first_known_index();
        let event = br#"{"type":"m.room.message","content":{"msgtype":"m.text","body":"hi badge"}}"#;
        let ciphertext = group.encrypt(event).to_base64();

        let mut sessions: BTreeMap<String, InboundGroupSession> = BTreeMap::new();
        sessions.insert(
            session_id.clone(),
            InboundGroupSession::import(&exported, SessionConfig::version_1()),
        );
        let message = MegolmMessage::from_base64(&ciphertext).unwrap();
        let plain = sessions.get_mut(&session_id).unwrap().decrypt(&message).unwrap();
        assert_eq!(body_from_event(&plain.plaintext).as_deref(), Some("hi badge"));
    }

    #[test]
    fn account_pickle_roundtrip_and_device_keys() {
        let c = Crypto::new(Account::new());
        let pickle = c.account_pickle();
        let restored = Crypto::from_account_pickle(&pickle).unwrap();
        assert_eq!(restored.identity_curve25519(), c.identity_curve25519());
        let dk = c.device_keys_json("@me:srv", "DEV");
        assert!(dk.contains("\"user_id\":\"@me:srv\""));
        assert!(dk.contains("\"signatures\""));
        assert!(dk.contains("curve25519:DEV"));
    }

    #[test]
    fn inbound_session_pickle_roundtrip() {
        // The persistence mechanism: an inbound session survives pickle ->
        // JSON -> from_pickle and still decrypts.
        let mut group = GroupSession::new(SessionConfig::version_1());
        let inbound = InboundGroupSession::new(&group.session_key(), SessionConfig::version_1());
        let event = br#"{"type":"m.room.message","content":{"msgtype":"m.text","body":"persisted"}}"#;
        let ct = group.encrypt(event).to_base64();

        let bytes = serde_json::to_vec(&inbound.pickle()).unwrap();
        let pickle: InboundGroupSessionPickle = serde_json::from_slice(&bytes).unwrap();
        let mut restored = InboundGroupSession::from_pickle(pickle);

        let msg = MegolmMessage::from_base64(&ct).unwrap();
        let dec = restored.decrypt(&msg).unwrap();
        assert_eq!(body_from_event(&dec.plaintext).as_deref(), Some("persisted"));
    }

    #[test]
    fn megolm_send_then_self_decrypt() {
        // Encrypt with an outbound session, decrypt by importing its key.
        let mut c = Crypto::new(Account::new());
        let (sid, key_b64, is_new) = c.ensure_outbound("!r:s");
        assert!(is_new);
        let event = r#"{"type":"m.room.message","content":{"msgtype":"m.text","body":"yo"}}"#;
        let ct = c.encrypt_megolm("!r:s", event).unwrap();

        let exported = ExportedSessionKey::from_base64(
            &vodozemac::megolm::SessionKey::from_base64(&key_b64)
                .map(|sk| {
                    InboundGroupSession::new(&sk, SessionConfig::version_1())
                        .export_at_first_known_index()
                        .to_base64()
                })
                .unwrap(),
        )
        .unwrap();
        let mut inbound = InboundGroupSession::import(&exported, SessionConfig::version_1());
        let _ = sid;
        let msg = MegolmMessage::from_base64(&ct).unwrap();
        let dec = inbound.decrypt(&msg).unwrap();
        assert_eq!(body_from_event(&dec.plaintext).as_deref(), Some("yo"));
    }
}
