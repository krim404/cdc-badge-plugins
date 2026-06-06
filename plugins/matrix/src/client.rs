//! \file
//! \brief Matrix Client-Server API calls over the host HTTP transport.
//!
//! Phase 1 covers login, sync, sending plaintext `m.text`, and join/leave.
//! All calls are synchronous; `/sync` uses `timeout=0` so it returns the
//! current snapshot without blocking the UI thread.

use crate::json::{self, LoginResult, SyncResult};
use crate::model::Session;
use cdc_badge_plugin::{http, log};
use alloc::format;
use alloc::string::String;

const TAG: &str = "matrix";

/// Limit the sync payload: only the last few timeline events, no presence or
/// account data. Keeps the buffered response small on the badge.
// Full member state is kept (no lazy loading) so E2EE key distribution can
// address every joined device.
const SYNC_FILTER: &str = "{\"room\":{\"timeline\":{\"limit\":10}},\"presence\":{\"types\":[]},\"account_data\":{\"types\":[]}}";

fn auth(req: &http::Request, token: &str) {
    let _ = req.header("Authorization", &format!("Bearer {}", token));
}

/// `POST /login` with `m.login.password`.
pub fn login(base_url: &str, user: &str, password: &str, device_id: Option<&str>) -> Option<LoginResult> {
    let url = format!("{}/_matrix/client/v3/login", base_url);
    // Reuse a prior device_id so the server does not register a fresh device on
    // every login (which accumulates dead devices and drifts the account's
    // one-time-key bookkeeping from the server's per-device view).
    let dev = device_id
        .filter(|d| !d.is_empty())
        .map(|d| format!("\"device_id\":\"{}\",", json::json_escape(d)))
        .unwrap_or_default();
    let body = format!(
        "{{\"type\":\"m.login.password\",{dev}\"identifier\":{{\"type\":\"m.id.user\",\"user\":\"{user}\"}},\"password\":\"{pass}\",\"initial_device_display_name\":\"CDC Badge\"}}",
        dev = dev,
        user = json::json_escape(user),
        pass = json::json_escape(password),
    );
    let req = http::Request::open(http::POST, &url, 15000).ok()?;
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status / 100 != 2 {
        log::warn(TAG, &format!("login HTTP {}", status));
        return None;
    }
    json::parse_login(&text)
}

/// `GET /sync` (`timeout=0`, immediate). `full` (or no token yet) does a full
/// state sync so room names + recent timeline are present; otherwise it resumes
/// incrementally from `next_batch` (deltas only - no room state).
pub fn sync(s: &Session, full: bool) -> Option<SyncResult> {
    let url = if full || s.next_batch.is_empty() {
        format!(
            "{}/_matrix/client/v3/sync?timeout=0&filter={}",
            s.base_url,
            json::urlencode(SYNC_FILTER)
        )
    } else {
        format!(
            "{}/_matrix/client/v3/sync?since={}&timeout=0&filter={}",
            s.base_url,
            json::urlencode(&s.next_batch),
            json::urlencode(SYNC_FILTER)
        )
    };
    let req = http::Request::open(http::GET, &url, 20000).ok()?;
    auth(&req, &s.token);
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("sync HTTP {}", status));
        return None;
    }
    json::parse_sync(&text)
}

/// `GET /rooms/{id}/messages?dir=b&limit=N` -> recent history (back-pagination),
/// used to populate a freshly joined room whose `/sync` timeline is empty.
pub fn room_messages(s: &Session, room_id: &str, limit: u32) -> Option<String> {
    let url = format!(
        "{}/_matrix/client/v3/rooms/{}/messages?dir=b&limit={}",
        s.base_url,
        json::urlencode(room_id),
        limit
    );
    let req = http::Request::open(http::GET, &url, 20000).ok()?;
    auth(&req, &s.token);
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("messages HTTP {}", status));
        return None;
    }
    Some(text)
}

/// `POST /rooms/{id}/invite` with `{"user_id": "@user:server"}`.
pub fn invite(s: &Session, room_id: &str, mxid: &str) -> bool {
    let url = format!(
        "{}/_matrix/client/v3/rooms/{}/invite",
        s.base_url,
        json::urlencode(room_id)
    );
    let body = format!("{{\"user_id\":\"{}\"}}", json::json_escape(mxid));
    match request_json(http::POST, s, &url, &body) {
        Some((status, _)) => status / 100 == 2,
        None => false,
    }
}

/// `PUT /rooms/{id}/send/m.room.message/{txn}` with an `m.text` body.
pub fn send_text(s: &Session, room_id: &str, text: &str, txn: u64) -> bool {
    let url = format!(
        "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
        s.base_url,
        json::urlencode(room_id),
        txn
    );
    let body = format!("{{\"msgtype\":\"m.text\",\"body\":\"{}\"}}", json::json_escape(text));
    let req = match http::Request::open(http::PUT, &url, 10000) {
        Ok(r) => r,
        Err(_) => return false,
    };
    auth(&req, &s.token);
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    match req.perform() {
        Ok(status) => status / 100 == 2,
        Err(_) => false,
    }
}

/// `POST /join/{idOrAlias}`. Returns the joined room id on success.
pub fn join(s: &Session, id_or_alias: &str) -> Option<String> {
    let url = format!("{}/_matrix/client/v3/join/{}", s.base_url, json::urlencode(id_or_alias));
    let req = http::Request::open(http::POST, &url, 15000).ok()?;
    auth(&req, &s.token);
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(b"{}");
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status / 100 != 2 {
        log::warn(TAG, &format!("join HTTP {}", status));
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(v.get("room_id").and_then(|r| r.as_str()).unwrap_or(id_or_alias).into())
}

/// `POST /createRoom` for a direct chat: invites `mxid`, flags `is_direct`, and
/// enables Megolm encryption. Returns the new room id.
pub fn create_dm(s: &Session, mxid: &str) -> Option<String> {
    let url = format!("{}/_matrix/client/v3/createRoom", s.base_url);
    let body = format!(
        "{{\"is_direct\":true,\"preset\":\"trusted_private_chat\",\"invite\":[\"{}\"],\"initial_state\":[{{\"type\":\"m.room.encryption\",\"state_key\":\"\",\"content\":{{\"algorithm\":\"m.megolm.v1.aes-sha2\"}}}}]}}",
        json::json_escape(mxid)
    );
    let req = http::Request::open(http::POST, &url, 15000).ok()?;
    auth(&req, &s.token);
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status / 100 != 2 {
        log::warn(TAG, &format!("createRoom HTTP {}", status));
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(v.get("room_id").and_then(|r| r.as_str())?.to_string())
}

/// `POST /createRoom` for a brand-new private, Megolm-encrypted room with the
/// given display name. Returns the new room id.
pub fn create_room(s: &Session, name: &str) -> Option<String> {
    let url = format!("{}/_matrix/client/v3/createRoom", s.base_url);
    let body = format!(
        "{{\"name\":\"{}\",\"preset\":\"private_chat\",\"initial_state\":[{{\"type\":\"m.room.encryption\",\"state_key\":\"\",\"content\":{{\"algorithm\":\"m.megolm.v1.aes-sha2\"}}}}]}}",
        json::json_escape(name)
    );
    let req = http::Request::open(http::POST, &url, 15000).ok()?;
    auth(&req, &s.token);
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status / 100 != 2 {
        log::warn(TAG, &format!("createRoom HTTP {}", status));
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(v.get("room_id").and_then(|r| r.as_str())?.to_string())
}

/// `POST /rooms/{id}/leave` then `/forget`.
pub fn leave(s: &Session, room_id: &str) -> bool {
    let enc = json::urlencode(room_id);
    let leave_url = format!("{}/_matrix/client/v3/rooms/{}/leave", s.base_url, enc);
    let ok = post_empty(s, &leave_url);
    if ok {
        let forget_url = format!("{}/_matrix/client/v3/rooms/{}/forget", s.base_url, enc);
        let _ = post_empty(s, &forget_url);
    }
    ok
}

/// `GET /user/{id}/account_data/{type}` -> the raw account-data content.
pub fn account_data(s: &Session, event_type: &str) -> Option<String> {
    let url = format!(
        "{}/_matrix/client/v3/user/{}/account_data/{}",
        s.base_url,
        json::urlencode(&s.user_id),
        json::urlencode(event_type)
    );
    let req = http::Request::open(http::GET, &url, 10000).ok()?;
    auth(&req, &s.token);
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("account_data {} HTTP {}", event_type, status));
        return None;
    }
    Some(text)
}

/// `GET /room_keys/version` -> the current backup version string.
pub fn backup_version(s: &Session) -> Option<String> {
    let url = format!("{}/_matrix/client/v3/room_keys/version", s.base_url);
    let req = http::Request::open(http::GET, &url, 10000).ok()?;
    auth(&req, &s.token);
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("backup version HTTP {}", status));
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(v.get("version")?.as_str()?.to_string())
}

/// `GET /room_keys/keys?version=N` -> the full encrypted backup body.
pub fn backup_keys(s: &Session, version: &str) -> Option<String> {
    let url = format!(
        "{}/_matrix/client/v3/room_keys/keys?version={}",
        s.base_url,
        json::urlencode(version)
    );
    let req = http::Request::open(http::GET, &url, 25000).ok()?;
    auth(&req, &s.token);
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("backup keys HTTP {}", status));
        return None;
    }
    Some(text)
}

/// `PUT /room_keys/keys?version=N` with a `{"rooms":...}` backup body. Stores
/// outbound room keys so a device that was offline at send time can restore them.
pub fn put_backup_keys(s: &Session, version: &str, body: &str) -> bool {
    let url = format!(
        "{}/_matrix/client/v3/room_keys/keys?version={}",
        s.base_url,
        json::urlencode(version)
    );
    match request_json(http::PUT, s, &url, body) {
        Some((status, b)) => {
            log::info(TAG, &format!("room_keys PUT HTTP {}", status));
            if status / 100 != 2 {
                log::warn(TAG, &format!("room_keys PUT body: {}", b));
            }
            status / 100 == 2
        }
        None => {
            log::warn(TAG, "room_keys PUT transport error");
            false
        }
    }
}

fn request_json(method: u8, s: &Session, url: &str, body: &str) -> Option<(i32, String)> {
    let req = http::Request::open(method, url, 15000).ok()?;
    auth(&req, &s.token);
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    let status = req.perform().ok()?;
    let text = req.read_to_string().ok()?;
    Some((status, text))
}

/// `POST /keys/upload` with our device keys and signed one-time keys.
/// Returns the HTTP status (or -1 on a transport error).
pub fn keys_upload(s: &Session, device_keys_json: &str, one_time_keys_json: &str) -> i32 {
    let url = format!("{}/_matrix/client/v3/keys/upload", s.base_url);
    let body = format!(
        "{{\"device_keys\":{},\"one_time_keys\":{}}}",
        device_keys_json, one_time_keys_json
    );
    match request_json(http::POST, s, &url, &body) {
        Some((status, body)) => {
            log::info(TAG, &format!("keys/upload HTTP {}", status));
            if status / 100 != 2 {
                log::warn(TAG, &format!("keys/upload body: {}", body));
            }
            status
        }
        None => {
            log::warn(TAG, "keys/upload transport error");
            -1
        }
    }
}

/// `POST /keys/query` for a set of users; returns the raw response body.
pub fn keys_query(s: &Session, user_ids: &[String]) -> Option<String> {
    let entries: alloc::vec::Vec<String> = user_ids
        .iter()
        .map(|u| format!("\"{}\":[]", json::json_escape(u)))
        .collect();
    let url = format!("{}/_matrix/client/v3/keys/query", s.base_url);
    let body = format!("{{\"device_keys\":{{{}}}}}", entries.join(","));
    match request_json(http::POST, s, &url, &body)? {
        (200, text) => Some(text),
        (status, _) => {
            log::warn(TAG, &format!("keys/query HTTP {}", status));
            None
        }
    }
}

/// `POST /keys/claim`; `claims` is the `one_time_keys` request object body.
pub fn keys_claim(s: &Session, claims_json: &str) -> Option<String> {
    let url = format!("{}/_matrix/client/v3/keys/claim", s.base_url);
    let body = format!("{{\"one_time_keys\":{}}}", claims_json);
    match request_json(http::POST, s, &url, &body)? {
        (200, text) => Some(text),
        (status, _) => {
            log::warn(TAG, &format!("keys/claim HTTP {}", status));
            None
        }
    }
}

/// `PUT /sendToDevice/m.room.encrypted/{txn}`; `messages_json` is the
/// `{user:{device:content}}` map.
pub fn send_to_device(s: &Session, messages_json: &str, txn: u64) -> bool {
    let url = format!(
        "{}/_matrix/client/v3/sendToDevice/m.room.encrypted/{}",
        s.base_url, txn
    );
    let body = format!("{{\"messages\":{}}}", messages_json);
    match request_json(http::PUT, s, &url, &body) {
        Some((status, b)) => {
            log::info(TAG, &format!("sendToDevice HTTP {}", status));
            if status / 100 != 2 {
                log::warn(TAG, &format!("sendToDevice body: {}", b));
            }
            status / 100 == 2
        }
        None => {
            log::warn(TAG, "sendToDevice transport error");
            false
        }
    }
}

/// `PUT /rooms/{id}/send/m.room.encrypted/{txn}` with a Megolm event content.
pub fn send_encrypted(s: &Session, room_id: &str, content_json: &str, txn: u64) -> bool {
    let url = format!(
        "{}/_matrix/client/v3/rooms/{}/send/m.room.encrypted/{}",
        s.base_url,
        json::urlencode(room_id),
        txn
    );
    match request_json(http::PUT, s, &url, content_json) {
        Some((status, b)) => {
            log::info(TAG, &format!("rooms/send.encrypted HTTP {}", status));
            if status / 100 != 2 {
                log::warn(TAG, &format!("rooms/send.encrypted body: {}", b));
            }
            status / 100 == 2
        }
        None => {
            log::warn(TAG, "rooms/send.encrypted transport error");
            false
        }
    }
}

fn post_empty(s: &Session, url: &str) -> bool {
    let req = match http::Request::open(http::POST, url, 10000) {
        Ok(r) => r,
        Err(_) => return false,
    };
    auth(&req, &s.token);
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(b"{}");
    match req.perform() {
        Ok(status) => status / 100 == 2,
        Err(_) => false,
    }
}
