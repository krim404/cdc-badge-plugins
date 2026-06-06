//! \file
//! \brief Matrix Client-Server JSON parsing and small encoding helpers.
//!
//! Parsing is intentionally tolerant: missing fields are skipped rather than
//! failing the whole response, so a malformed room never breaks the sync.

use crate::model::{EncInfo, Message};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde_json::Value;

/// Escape a string for embedding inside a JSON string literal.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u");
                let code = c as u32;
                for shift in [12, 8, 4, 0] {
                    let nibble = (code >> shift) & 0xf;
                    out.push(char::from_digit(nibble, 16).unwrap_or('0'));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Percent-encode one URL path segment (room ids contain `!`, `:`, `#`).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

/// Reduce `@local:server` to `local`; pass other shapes through unchanged.
pub fn short_sender(mxid: &str) -> String {
    let no_at = mxid.strip_prefix('@').unwrap_or(mxid);
    match no_at.find(':') {
        Some(i) => no_at[..i].to_string(),
        None => no_at.to_string(),
    }
}

/// Outcome of a successful `/login`.
pub struct LoginResult {
    pub user_id: String,
    pub device_id: String,
    pub token: String,
}

pub fn parse_login(body: &str) -> Option<LoginResult> {
    let v: Value = serde_json::from_str(body).ok()?;
    let token = v.get("access_token")?.as_str()?.to_string();
    Some(LoginResult {
        user_id: v.get("user_id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        device_id: v.get("device_id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        token,
    })
}

/// Per-room delta extracted from `/sync`.
pub struct RoomUpdate {
    pub id: String,
    /// New display name if this delta carried one.
    pub name: Option<String>,
    pub encrypted: bool,
    pub messages: Vec<Message>,
    /// Joined member MXIDs seen in this delta.
    pub members: Vec<String>,
    /// Joined `(mxid, displayname)` pairs seen in this delta; display name is
    /// empty when the member event carried none.
    pub member_names: Vec<(String, String)>,
    /// Latest `origin_server_ts` (ms) seen in this delta, 0 if none.
    pub last_ts: u64,
    /// True when this update is a pending invitation (rooms.invite), not a join.
    pub invited: bool,
}

pub struct SyncResult {
    pub next_batch: String,
    pub rooms: Vec<RoomUpdate>,
    pub to_device: Vec<ToDeviceOlm>,
    /// Server-side count of unclaimed signed_curve25519 one-time keys, from
    /// `device_one_time_keys_count`; drives count-based OTK replenishment.
    pub otk_count: Option<u32>,
    /// Room ids reported under `rooms.leave` (left/kicked), removed from the cache.
    pub left: Vec<String>,
}

/// One Olm-encrypted to-device payload addressed to a recipient curve25519 key.
pub struct ToDeviceOlm {
    pub sender_key: String,
    pub recipient_curve: String,
    pub msg_type: u8,
    pub body: String,
}

fn event_array<'a>(parent: Option<&'a Value>, section: &str) -> &'a [Value] {
    parent
        .and_then(|p| p.get(section))
        .and_then(|s| s.get("events"))
        .and_then(|e| e.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
}

fn name_from_events(events: &[Value]) -> Option<String> {
    let mut found = None;
    for e in events {
        let ty = e.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let content = e.get("content");
        if ty == "m.room.name" {
            if let Some(n) = content.and_then(|c| c.get("name")).and_then(|n| n.as_str()) {
                if !n.is_empty() {
                    found = Some(n.to_string());
                }
            }
        } else if ty == "m.room.canonical_alias" && found.is_none() {
            if let Some(a) = content.and_then(|c| c.get("alias")).and_then(|a| a.as_str()) {
                if !a.is_empty() {
                    found = Some(a.to_string());
                }
            }
        }
    }
    found
}

fn name_from_heroes(room: &Value, member_names: &[(String, String)]) -> Option<String> {
    let heroes = room.get("summary")?.get("m.heroes")?.as_array()?;
    if heroes.is_empty() {
        return None;
    }
    let names: Vec<String> = heroes
        .iter()
        .filter_map(|h| h.as_str())
        .map(|mxid| {
            member_names
                .iter()
                .find(|(u, _)| u == mxid)
                .map(|(_, dn)| dn.clone())
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| short_sender(mxid))
        })
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

fn parse_room(id: &str, room: &Value) -> RoomUpdate {
    let state = event_array(Some(room), "state");
    let timeline = event_array(Some(room), "timeline");

    let mut encrypted = false;
    let mut messages = Vec::new();
    let mut members = Vec::new();
    let mut member_names: Vec<(String, String)> = Vec::new();
    for e in state.iter().chain(timeline.iter()) {
        let ty = e.get("type").and_then(|t| t.as_str());
        if ty == Some("m.room.encryption") {
            encrypted = true;
        }
        if ty == Some("m.room.member") {
            let content = e.get("content");
            let joined =
                content.and_then(|c| c.get("membership")).and_then(|m| m.as_str()) == Some("join");
            if let Some(uid) = e.get("state_key").and_then(|s| s.as_str()) {
                if joined && !uid.is_empty() {
                    if !members.iter().any(|m| m == uid) {
                        members.push(uid.to_string());
                    }
                    let dn = content
                        .and_then(|c| c.get("displayname"))
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !member_names.iter().any(|(u, _)| u == uid) {
                        member_names.push((uid.to_string(), dn));
                    }
                }
            }
        }
    }

    // Name priority: explicit room name/alias, else derived from heroes (DMs).
    let name = name_from_events(timeline)
        .or_else(|| name_from_events(state))
        .or_else(|| name_from_heroes(room, &member_names));
    let mut last_ts = 0u64;
    for e in timeline {
        let ty = e.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let ts = e.get("origin_server_ts").and_then(|t| t.as_u64()).unwrap_or(0);
        if ty == "m.room.message" || ty == "m.room.encrypted" {
            last_ts = last_ts.max(ts);
        }
        if ty == "m.room.encrypted" {
            encrypted = true;
        }
        if let Some(m) = parse_message_event(e) {
            messages.push(m);
        }
    }

    RoomUpdate { id: id.to_string(), name, encrypted, messages, members, member_names, last_ts, invited: false }
}

/// Map a single timeline event to a display [`Message`], or `None` for events
/// that are not text/encrypted messages. Shared by sync parsing and the
/// `/messages` back-pagination chunk.
fn parse_message_event(e: &Value) -> Option<Message> {
    let ty = e.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let sender = short_sender(e.get("sender").and_then(|s| s.as_str()).unwrap_or(""));
    let content = e.get("content");
    match ty {
        "m.room.message" => {
            let msgtype = content.and_then(|c| c.get("msgtype")).and_then(|m| m.as_str());
            if msgtype != Some("m.text") {
                return None;
            }
            let body = content
                .and_then(|c| c.get("body"))
                .and_then(|b| b.as_str())
                .unwrap_or("")
                .to_string();
            Some(Message { sender, body, encrypted: false, enc: None })
        }
        "m.room.encrypted" => {
            let enc = match (
                content.and_then(|c| c.get("session_id")).and_then(|s| s.as_str()),
                content.and_then(|c| c.get("ciphertext")).and_then(|s| s.as_str()),
            ) {
                (Some(sid), Some(ct)) => Some(EncInfo {
                    session_id: sid.to_string(),
                    ciphertext: ct.to_string(),
                }),
                _ => None,
            };
            Some(Message { sender, body: String::new(), encrypted: true, enc })
        }
        _ => None,
    }
}

/// Parse a `GET /rooms/{id}/messages` response. Its `chunk` is newest-first
/// (dir=b); the returned messages are ordered oldest-first for display.
pub fn parse_messages_chunk(body: &str) -> Vec<Message> {
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    if let Some(chunk) = v.get("chunk").and_then(|c| c.as_array()) {
        for e in chunk {
            if let Some(m) = parse_message_event(e) {
                out.push(m);
            }
        }
    }
    out.reverse();
    out
}

/// Parse a `rooms.invite` entry from its stripped `invite_state` events: room
/// name (if any) and member identities, flagged as a pending invitation.
fn parse_invite(id: &str, room: &Value) -> RoomUpdate {
    let events = room
        .get("invite_state")
        .and_then(|s| s.get("events"))
        .and_then(|e| e.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    let mut members = Vec::new();
    let mut member_names: Vec<(String, String)> = Vec::new();
    let mut encrypted = false;
    for e in events {
        let ty = e.get("type").and_then(|t| t.as_str());
        if ty == Some("m.room.encryption") {
            encrypted = true;
        }
        if ty == Some("m.room.member") {
            if let Some(uid) = e.get("state_key").and_then(|s| s.as_str()) {
                if !uid.is_empty() {
                    let dn = e
                        .get("content")
                        .and_then(|c| c.get("displayname"))
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !members.iter().any(|m| m == uid) {
                        members.push(uid.to_string());
                    }
                    if !member_names.iter().any(|(u, _)| u == uid) {
                        member_names.push((uid.to_string(), dn));
                    }
                }
            }
        }
    }
    let name = name_from_events(events);
    RoomUpdate {
        id: id.to_string(),
        name,
        encrypted,
        messages: Vec::new(),
        members,
        member_names,
        last_ts: 0,
        invited: true,
    }
}

/// Extract every `(key_id, ciphertext, iv, mac)` entry from an
/// `m.secret_storage.v1.aes-hmac-sha2` account-data secret. A secret can hold
/// more than one entry (e.g. after a recovery-key rotation, keyed by SSSS key
/// id), so callers try each until one MAC-verifies. Empty when none are usable.
pub fn parse_ssss_secret(body: &str) -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let enc = match v.get("encrypted").and_then(|e| e.as_object()) {
        Some(e) => e,
        None => return out,
    };
    for (key_id, entry) in enc {
        let get = |k: &str| entry.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
        if let (Some(ct), Some(iv), Some(mac)) = (get("ciphertext"), get("iv"), get("mac")) {
            out.push((key_id.clone(), ct, iv, mac));
        }
    }
    out
}

/// A recipient device's identity keys from `/keys/query`.
pub struct DeviceKey {
    pub user_id: String,
    pub device_id: String,
    pub curve25519: String,
    pub ed25519: String,
}

pub fn parse_keys_query(body: &str) -> Vec<DeviceKey> {
    let mut out = Vec::new();
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let users = match v.get("device_keys").and_then(|d| d.as_object()) {
        Some(u) => u,
        None => return out,
    };
    for (user_id, devices) in users {
        let devices = match devices.as_object() {
            Some(d) => d,
            None => continue,
        };
        for (device_id, dk) in devices {
            let keys = dk.get("keys");
            let curve = keys
                .and_then(|k| k.get(format!("curve25519:{}", device_id).as_str()))
                .and_then(|x| x.as_str());
            let ed = keys
                .and_then(|k| k.get(format!("ed25519:{}", device_id).as_str()))
                .and_then(|x| x.as_str());
            if let (Some(curve), Some(ed)) = (curve, ed) {
                out.push(DeviceKey {
                    user_id: user_id.clone(),
                    device_id: device_id.clone(),
                    curve25519: curve.to_string(),
                    ed25519: ed.to_string(),
                });
            }
        }
    }
    out
}

/// A claimed one-time key from `/keys/claim`, addressed by user+device.
pub struct ClaimedOtk {
    pub user_id: String,
    pub device_id: String,
    pub key: String,
}

pub fn parse_keys_claim(body: &str) -> Vec<ClaimedOtk> {
    let mut out = Vec::new();
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let users = match v.get("one_time_keys").and_then(|d| d.as_object()) {
        Some(u) => u,
        None => return out,
    };
    for (user_id, devices) in users {
        let devices = match devices.as_object() {
            Some(d) => d,
            None => continue,
        };
        for (device_id, keys) in devices {
            // The single entry is keyed "signed_curve25519:<id>".
            if let Some(obj) = keys.as_object() {
                if let Some((_, entry)) = obj.iter().next() {
                    if let Some(key) = entry.get("key").and_then(|k| k.as_str()) {
                        out.push(ClaimedOtk {
                            user_id: user_id.clone(),
                            device_id: device_id.clone(),
                            key: key.to_string(),
                        });
                    }
                }
            }
        }
    }
    out
}

/// One encrypted Megolm session entry from `GET /room_keys/keys`.
pub struct BackupSession {
    pub session_id: String,
    pub ciphertext: String,
    pub mac: String,
    pub ephemeral: String,
}

pub fn parse_backup(body: &str) -> Vec<BackupSession> {
    let mut out = Vec::new();
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let rooms = match v.get("rooms").and_then(|r| r.as_object()) {
        Some(r) => r,
        None => return out,
    };
    for room in rooms.values() {
        let sessions = match room.get("sessions").and_then(|s| s.as_object()) {
            Some(s) => s,
            None => continue,
        };
        for (session_id, entry) in sessions {
            let data = match entry.get("session_data") {
                Some(d) => d,
                None => continue,
            };
            let get = |k: &str| data.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
            if let (Some(ciphertext), Some(mac), Some(ephemeral)) =
                (get("ciphertext"), get("mac"), get("ephemeral"))
            {
                out.push(BackupSession {
                    session_id: session_id.clone(),
                    ciphertext,
                    mac,
                    ephemeral,
                });
            }
        }
    }
    out
}

pub fn parse_sync(body: &str) -> Option<SyncResult> {
    let v: Value = serde_json::from_str(body).ok()?;
    let next_batch = v.get("next_batch").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let mut rooms = Vec::new();
    if let Some(join) = v
        .get("rooms")
        .and_then(|r| r.get("join"))
        .and_then(|j| j.as_object())
    {
        for (id, room) in join {
            rooms.push(parse_room(id, room));
        }
    }
    if let Some(invite) = v
        .get("rooms")
        .and_then(|r| r.get("invite"))
        .and_then(|i| i.as_object())
    {
        for (id, room) in invite {
            rooms.push(parse_invite(id, room));
        }
    }
    let to_device = parse_to_device(&v);
    let otk_count = v
        .get("device_one_time_keys_count")
        .and_then(|c| c.get("signed_curve25519"))
        .and_then(|n| n.as_u64())
        .map(|n| n as u32);
    let mut left = Vec::new();
    if let Some(leave) = v
        .get("rooms")
        .and_then(|r| r.get("leave"))
        .and_then(|l| l.as_object())
    {
        for (id, _) in leave {
            left.push(id.clone());
        }
    }
    Some(SyncResult { next_batch, rooms, to_device, otk_count, left })
}

/// Extract Olm-encrypted to-device payloads (`m.room.encrypted` /
/// `m.olm.v1.curve25519-aes-sha2`) from a `/sync` response, one entry per
/// recipient ciphertext slot.
fn parse_to_device(v: &Value) -> Vec<ToDeviceOlm> {
    let mut out = Vec::new();
    for e in event_array(Some(v), "to_device") {
        if e.get("type").and_then(|t| t.as_str()) != Some("m.room.encrypted") {
            continue;
        }
        let content = match e.get("content") {
            Some(c) => c,
            None => continue,
        };
        if content.get("algorithm").and_then(|a| a.as_str())
            != Some("m.olm.v1.curve25519-aes-sha2")
        {
            continue;
        }
        let sender_key = match content.get("sender_key").and_then(|s| s.as_str()) {
            Some(s) => s,
            None => continue,
        };
        if let Some(ct) = content.get("ciphertext").and_then(|c| c.as_object()) {
            for (recipient_curve, entry) in ct {
                let msg_type = entry.get("type").and_then(|t| t.as_u64()).unwrap_or(0) as u8;
                if let Some(body) = entry.get("body").and_then(|b| b.as_str()) {
                    out.push(ToDeviceOlm {
                        sender_key: sender_key.to_string(),
                        recipient_curve: recipient_curve.clone(),
                        msg_type,
                        body: body.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// Extract `(session_id, session_key)` from a decrypted `m.room_key` to-device
/// event, or `None` if it is not a Megolm room key.
pub fn parse_room_key(plain: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(plain).ok()?;
    if v.get("type")?.as_str()? != "m.room_key" {
        return None;
    }
    let content = v.get("content")?;
    if content.get("algorithm")?.as_str()? != "m.megolm.v1.aes-sha2" {
        return None;
    }
    let sid = content.get("session_id")?.as_str()?.to_string();
    let skey = content.get("session_key")?.as_str()?.to_string();
    Some((sid, skey))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_handles_quotes_and_controls() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("x\ny"), "x\\ny");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }

    #[test]
    fn urlencode_encodes_room_id() {
        assert_eq!(urlencode("!abc:srv.org"), "%21abc%3Asrv.org");
        assert_eq!(urlencode("plain-9_.~"), "plain-9_.~");
    }

    #[test]
    fn short_sender_strips() {
        assert_eq!(short_sender("@alice:example.org"), "alice");
        assert_eq!(short_sender("bob"), "bob");
    }

    #[test]
    fn parse_login_reads_fields() {
        let body = r#"{"user_id":"@a:s","device_id":"D1","access_token":"tok"}"#;
        let r = parse_login(body).unwrap();
        assert_eq!(r.user_id, "@a:s");
        assert_eq!(r.device_id, "D1");
        assert_eq!(r.token, "tok");
        assert!(parse_login(r#"{"device_id":"D1"}"#).is_none());
    }

    #[test]
    fn parse_sync_extracts_rooms_and_text() {
        let body = r#"{
          "next_batch":"s2",
          "rooms":{"join":{
            "!r:srv":{
              "state":{"events":[{"type":"m.room.name","content":{"name":"Lobby"}}]},
              "timeline":{"events":[
                {"type":"m.room.message","sender":"@bob:srv","content":{"msgtype":"m.text","body":"hi"}},
                {"type":"m.room.message","sender":"@bob:srv","content":{"msgtype":"m.image","body":"pic"}}
              ]}
            }
          }}
        }"#;
        let r = parse_sync(body).unwrap();
        assert_eq!(r.next_batch, "s2");
        assert_eq!(r.rooms.len(), 1);
        let room = &r.rooms[0];
        assert_eq!(room.id, "!r:srv");
        assert_eq!(room.name.as_deref(), Some("Lobby"));
        assert!(!room.encrypted);
        assert_eq!(room.messages.len(), 1); // only the m.text, not the image
        assert_eq!(room.messages[0].sender, "bob");
        assert_eq!(room.messages[0].body, "hi");
    }

    #[test]
    fn parse_sync_marks_encrypted_and_captures_payload() {
        let body = r#"{"next_batch":"s","rooms":{"join":{"!e:s":{
            "state":{"events":[{"type":"m.room.encryption","content":{}}]},
            "timeline":{"events":[{"type":"m.room.encrypted","sender":"@c:s","content":{"session_id":"SID","ciphertext":"CT"}}]}
        }}}}"#;
        let r = parse_sync(body).unwrap();
        let room = &r.rooms[0];
        assert!(room.encrypted);
        assert_eq!(room.messages.len(), 1);
        let m = &room.messages[0];
        assert!(m.encrypted);
        let enc = m.enc.as_ref().unwrap();
        assert_eq!(enc.session_id, "SID");
        assert_eq!(enc.ciphertext, "CT");
    }

    #[test]
    fn parse_sync_names_dm_from_heroes() {
        // No m.room.name -> derive from hero's display name.
        let body = r#"{"next_batch":"s","rooms":{"join":{"!dm:s":{
            "summary":{"m.heroes":["@krim:matrix.krim.dev"]},
            "state":{"events":[{"type":"m.room.member","state_key":"@krim:matrix.krim.dev","content":{"membership":"join","displayname":"Krim"}}]},
            "timeline":{"events":[]}
        }}}}"#;
        assert_eq!(parse_sync(body).unwrap().rooms[0].name.as_deref(), Some("Krim"));

        // No display name -> fall back to the localpart.
        let body2 = r#"{"next_batch":"s","rooms":{"join":{"!dm:s":{
            "summary":{"m.heroes":["@krim:matrix.krim.dev"]},
            "timeline":{"events":[]}
        }}}}"#;
        assert_eq!(parse_sync(body2).unwrap().rooms[0].name.as_deref(), Some("krim"));
    }

    #[test]
    fn parse_backup_extracts_sessions() {
        let body = r#"{"rooms":{"!r:s":{"sessions":{
            "SID1":{"first_message_index":0,"session_data":{"ciphertext":"C","mac":"M","ephemeral":"E"}}
        }}}}"#;
        let sessions = parse_backup(body);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "SID1");
        assert_eq!(sessions[0].ciphertext, "C");
        assert_eq!(sessions[0].mac, "M");
        assert_eq!(sessions[0].ephemeral, "E");
    }
}
