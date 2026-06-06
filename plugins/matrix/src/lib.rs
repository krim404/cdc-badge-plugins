//! \file
//! \brief Matrix chat client plugin (Phase 1: plaintext, unencrypted rooms).
//!
//! UI: a room list with a `[3]` menu (New / Delete / System). Selecting a room
//! opens the chat as a plugin-drawn canvas (bottom-anchored, newest at the
//! bottom); `Y` opens the reply T9 and `Y` again sends, `N` returns to the list,
//! `2`/`8` scroll the history. The System screen holds username, password (rmem),
//! server URL and the E2EE recovery key.
//!
//! Key routing: the chat canvas owns its keys via the canvas key callback
//! (`ACT_CHAT_KEY`, `user_data` = ASCII key code), which keeps the chat visible across
//! presses. The room list uses the EventBus subscription
//! (`plugin_on_action(ACT_KEY_EVENT, 0, ascii_char)`) for the `[1]` sync.

// This crate links vodozemac, which needs std; hence no `#![no_std]` here.
// `extern crate alloc` is kept so the `alloc::` paths in the submodules stay
// valid unchanged.
extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;

use cdc_badge_plugin::{event, i18n, log, plugin_main, time, ui};

mod actions;
mod client;
mod crypto;
mod json;
mod model;
mod store;
mod ui_views;

#[cfg(target_arch = "wasm32")]
mod getrandom_shim;

use actions::*;
use model::{Room, Session, RETAIN};

plugin_main!();

const TAG: &str = "matrix";

// ASCII codes the EventBus reports in `user_data` for the keypad.
const KEY_Y: u32 = b'Y' as u32;
const KEY_N: u32 = b'N' as u32;
const KEY_SYNC: u32 = b'1' as u32;
const KEY_UP: u32 = b'2' as u32;
const KEY_DOWN: u32 = b'8' as u32;
const KEY_MENU: u32 = b'3' as u32;

// Single-threaded interior mutability: WAMR runs each plugin on one thread.
struct PluginCell<T>(RefCell<T>);
// SAFETY: WAMR runs every plugin on a single thread.
unsafe impl<T> Sync for PluginCell<T> {}
impl<T> PluginCell<T> {
    const fn new(v: T) -> Self {
        Self(RefCell::new(v))
    }
}
impl<T> core::ops::Deref for PluginCell<T> {
    type Target = RefCell<T>;
    fn deref(&self) -> &RefCell<T> {
        &self.0
    }
}

/// Which plugin screen is currently on top; drives key routing.
#[derive(Clone, Copy, PartialEq)]
enum View {
    RoomList,
    System,
    Chat,
    /// A T9 input is open (reply over the chat, or join/new room over the list);
    /// non-pollable so the auto-poll does not redraw underneath it.
    Composing,
    /// A System-config field editor (T9 / password) is open over the system list.
    EditingField,
}

static SESSION: PluginCell<Option<Session>> = PluginCell::new(None);
static ROOMS: PluginCell<Vec<Room>> = PluginCell::new(Vec::new());
static OPEN_ROOM: PluginCell<Option<String>> = PluginCell::new(None);
static MENU_TARGET: PluginCell<Option<String>> = PluginCell::new(None);
static PENDING_DELETE: PluginCell<Option<String>> = PluginCell::new(None);
static VIEW: PluginCell<View> = PluginCell::new(View::RoomList);
static TXN: PluginCell<u64> = PluginCell::new(0);
static CRYPTO: PluginCell<Option<crypto::Crypto>> = PluginCell::new(None);
static UPLOADED: PluginCell<bool> = PluginCell::new(false);
/// Last server-side unclaimed one-time-key count (from `/sync`), driving
/// count-based replenishment so we never re-upload keys the server still holds.
static OTK_COUNT: PluginCell<u32> = PluginCell::new(0);
/// Hash of the session snapshot last written to NVS, so unchanged state is never
/// rewritten (avoids flash wear from repeated opens/syncs).
static CRYPTO_HASH: PluginCell<u64> = PluginCell::new(0);
/// Uptime (ms) of the last auto-refresh poll; throttles the poll timer.
static LAST_POLL_MS: PluginCell<u64> = PluginCell::new(0);
/// Chat scroll offset in lines, counted up from the newest message (0 = bottom).
static SCROLL: PluginCell<usize> = PluginCell::new(0);
/// Pending background room-key distribution, processed in chunks across ticks.
static PENDING_DIST: PluginCell<Option<PendingDist>> = PluginCell::new(None);

/// Auto-refresh interval for the room list / open chat. Short enough that new
/// messages surface without a manual sync; `/sync` uses `timeout=0`, so each
/// poll is a quick snapshot that does not block key input.
const POLL_INTERVAL_MS: u64 = 6_000;
/// Replenish one-time keys when the server holds fewer than this many; one upload
/// adds 30, keeping the server-side pool under the 50-key cap.
const OTK_TARGET: u32 = 20;
/// Recipient devices processed per tick during room-key distribution. One fresh
/// Olm session (X25519 + ratchet + AES, all in wasm) per device costs tens of
/// millions of instructions; four stay safely under WAMR's per-call limit (500M)
/// and cut the tick count (~10 ticks for 39 devices). A trap would leak the held
/// RefCell borrows (wasm does not unwind), so staying under the limit is mandatory.
/// Each tick blocks input/GUI for its duration (plugin_on_tick shares the host
/// call_mutex with key dispatch), so this is a balance, not to be raised freely.
const DIST_CHUNK: usize = 4;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn set_view(v: View) {
    *VIEW.borrow_mut() = v;
}

/// Load (or create + persist) the Olm account, attach the backup decryptor if a
/// recovery key is set, then pull the key backup.
fn init_crypto() {
    let mut c = match store::load_account().and_then(|b| crypto::Crypto::from_account_pickle(&b)) {
        Some(c) => c,
        None => {
            if store::has_account() {
                // A sealed account exists but could not be unsealed/parsed.
                // Creating a fresh one would rotate the device identity and break
                // every existing session and the server's view of our keys. Refuse
                // and leave crypto uninitialised; the user can reset the module.
                log::error(TAG, "account present but unreadable; refusing to rotate identity");
                return;
            }
            let fresh = crypto::Crypto::create();
            store::save_account(&fresh.account_pickle());
            fresh
        }
    };
    // Restore persisted sessions so a reboot skips the backup fetch and reuses
    // existing inbound room keys.
    if let Some(state) = store::load_crypto_state() {
        c.import_state(&state);
        // Seed the dedup hash with the on-disk state so an unchanged session
        // snapshot is never rewritten on subsequent opens.
        *CRYPTO_HASH.borrow_mut() = fnv1a(&state);
    }
    let have_sessions = c.session_count() > 0;
    *CRYPTO.borrow_mut() = Some(c);
    // Derive the real backup key from the recovery key via SSSS, then pull the
    // backup the first time (nothing persisted yet); afterwards the user
    // refreshes explicitly with [1].
    setup_backup();
    if !have_sessions {
        refresh_backup();
    }
}

/// Recover the Megolm-backup private key from the stored recovery (SSSS) key:
/// fetch the `m.megolm_backup.v1` account-data secret and decrypt it with the
/// SSSS key, then arm the backup decryptor. No-op without a recovery key.
fn setup_backup() {
    let reckey = match store::load_recovery_key() {
        Some(k) if !k.is_empty() => k,
        _ => return,
    };
    let ssss = match crypto::parse_recovery_key(&reckey) {
        Some(k) => k,
        None => {
            log::warn(TAG, "recovery key malformed");
            return;
        }
    };
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return,
    };
    let body = match client::account_data(&sess, "m.megolm_backup.v1") {
        Some(b) => b,
        None => return,
    };
    // The secret may carry several entries (one per SSSS key id, e.g. after a
    // recovery-key rotation); only the matching recovery key MAC-verifies one.
    for (_, ct, iv, mac) in &json::parse_ssss_secret(&body) {
        if let Some(key) = crypto::ssss_decrypt_backup_key(&ssss, ct, iv, mac) {
            if let Some(c) = CRYPTO.borrow_mut().as_mut() {
                c.set_backup_key(&key);
            }
            log::info(TAG, "backup key ready");
            return;
        }
    }
    log::warn(TAG, "SSSS decrypt failed (wrong recovery key?)");
}

/// Persist the session snapshot (sealed) to NVS, but only when it actually
/// changed since the last write - keeps repeated opens/syncs from wearing flash.
fn persist_crypto() {
    let bytes = match CRYPTO.borrow().as_ref() {
        Some(c) => c.export_state(),
        None => return,
    };
    let hash = fnv1a(&bytes);
    if hash == *CRYPTO_HASH.borrow() {
        return; // unchanged -> skip the flash write
    }
    store::save_crypto_state(&bytes);
    *CRYPTO_HASH.borrow_mut() = hash;
}

/// Re-fetch the server-side key backup and import any new Megolm sessions.
fn refresh_backup() {
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return,
    };
    if !CRYPTO.borrow().as_ref().map(|c| c.has_backup()).unwrap_or(false) {
        return; // no recovery key -> nothing to fetch
    }
    let version = match client::backup_version(&sess) {
        Some(v) => v,
        None => return,
    };
    let body = match client::backup_keys(&sess, &version) {
        Some(b) => b,
        None => return,
    };
    let sessions = json::parse_backup(&body);
    {
        let mut guard = CRYPTO.borrow_mut();
        if let Some(c) = guard.as_mut() {
            for bs in &sessions {
                c.import_backup_session(&bs.session_id, &bs.ciphertext, &bs.mac, &bs.ephemeral);
            }
            log::info(TAG, &alloc::format!("backup: {} sessions", c.session_count()));
        }
    }
    persist_crypto();
}

/// Upload one freshly created room key to the server-side key backup so a device
/// that was offline at send time (or logs in later) can restore it. No-op when
/// no backup key is armed or the server has no backup version.
fn backup_room_key(sess: &Session, room_id: &str, session_id: &str) {
    let version = match client::backup_version(sess) {
        Some(v) => v,
        None => return,
    };
    let blob = CRYPTO.borrow().as_ref().and_then(|c| c.backup_session_blob(session_id));
    let (ciphertext, mac, ephemeral) = match blob {
        Some(b) => b,
        None => return,
    };
    let body = alloc::format!(
        "{{\"rooms\":{{\"{room}\":{{\"sessions\":{{\"{sid}\":{{\"first_message_index\":0,\"forwarded_count\":0,\"is_verified\":false,\"session_data\":{{\"ciphertext\":\"{ct}\",\"mac\":\"{mac}\",\"ephemeral\":\"{eph}\"}}}}}}}}}}}}",
        room = json::json_escape(room_id),
        sid = json::json_escape(session_id),
        ct = json::json_escape(&ciphertext),
        mac = json::json_escape(&mac),
        eph = json::json_escape(&ephemeral),
    );
    let _ = client::put_backup_keys(sess, &version, &body);
}

fn next_txn() -> u64 {
    let mut t = TXN.borrow_mut();
    *t += 1;
    time::uptime_ms().wrapping_mul(1000).wrapping_add(*t)
}

fn normalize_server(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');
    if s.is_empty() {
        String::new()
    } else if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        alloc::format!("https://{}", s)
    }
}

fn render_rooms() {
    set_view(View::RoomList);
    let logged_in = SESSION.borrow().is_some();
    // ROOMS stays in stable storage order; the list is sorted for display only,
    // with each item keyed by its storage index so a tap always maps back to the
    // same room (see render_room_list / open_chat).
    let rooms = ROOMS.borrow();
    ui_views::render_room_list(&rooms, logged_in);
}

/// Pull `/sync` and merge into [`ROOMS`]. `full` does a full state sync (room
/// names + recent timeline, used on enter) and does not mark rooms unread;
/// incremental syncs ([1] / auto-refresh) mark rooms with new messages unread.
fn sync_now(full: bool) {
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return,
    };
    let mut result = match client::sync(&sess, full) {
        Some(r) => r,
        None => return,
    };
    let mark_unread = !full;
    // Track the server-side one-time-key count so ensure_uploaded only replenishes
    // when the pool runs low (never re-uploading keys the server already holds).
    if let Some(n) = result.otk_count {
        *OTK_COUNT.borrow_mut() = n;
    }
    // Keep the sync token in RAM only; it is flushed to NVS on exit to avoid a
    // flash write on every sync.
    if !result.next_batch.is_empty() {
        if let Some(s) = SESSION.borrow_mut().as_mut() {
            s.next_batch = result.next_batch.clone();
        }
    }
    // Import room keys delivered as Olm to-device messages before decrypting the
    // timeline, so events from other senders become readable in this same pass.
    let imported_keys = {
        let mut guard = CRYPTO.borrow_mut();
        match guard.as_mut() {
            Some(c) => {
                let my_curve = c.identity_curve25519();
                let total = result.to_device.len();
                let mut matched = 0u32;
                let mut decrypted = 0u32;
                let mut imported = 0u32;
                for td in result.to_device.iter().filter(|t| t.recipient_curve == my_curve) {
                    matched += 1;
                    if let Some(plain) = c.olm_decrypt(&td.sender_key, td.msg_type, &td.body) {
                        decrypted += 1;
                        if let Some((sid, skey)) = json::parse_room_key(&plain) {
                            if c.import_room_key(&sid, &skey) {
                                imported += 1;
                            }
                        }
                    }
                }
                if total > 0 {
                    log::info(
                        TAG,
                        &alloc::format!(
                            "to_device: {} total, {} for us, {} decrypted, {} keys",
                            total, matched, decrypted, imported
                        ),
                    );
                }
                imported > 0
            }
            None => false,
        }
    };
    if imported_keys {
        // To-device events are one-shot (the sync token advances past them), so
        // persist freshly imported keys now rather than risk losing them.
        persist_crypto();
    }
    // Decrypt Megolm events for which we hold the room key. The inbound sessions
    // are already persisted at import time; their advancing replay state is not
    // worth a flash write per sync, so we do not persist here.
    {
        let mut guard = CRYPTO.borrow_mut();
        if let Some(c) = guard.as_mut() {
            for up in result.rooms.iter_mut() {
                for m in up.messages.iter_mut() {
                    if let Some(enc) = m.enc.clone() {
                        if let Some(body) = c.decrypt_megolm(&enc.session_id, &enc.ciphertext) {
                            m.body = body;
                            m.encrypted = false;
                            m.enc = None;
                        }
                    }
                }
            }
        }
    }
    let invite_count = result.rooms.iter().filter(|r| r.invited).count();
    if invite_count > 0 {
        log::info(TAG, &alloc::format!("sync: {} invite(s) full={}", invite_count, full));
    }
    let mut rooms = ROOMS.borrow_mut();
    for up in result.rooms {
        let idx = rooms.iter().position(|r| r.id == up.id);
        let room = match idx {
            Some(i) => &mut rooms[i],
            None => {
                rooms.push(Room::new(up.id.clone()));
                let last = rooms.len() - 1;
                &mut rooms[last]
            }
        };
        if let Some(name) = up.name {
            room.name = name;
            room.is_dm = false;
        }
        // DM fallback: a room with no name/alias/heroes keeps its id as the
        // display name (Room::new seeds name = id), so resolve it to the other
        // participant and flag it a direct chat for the list icon.
        if room.name == room.id {
            if let Some(label) = up
                .member_names
                .iter()
                .find(|(uid, _)| uid != &sess.user_id)
                .map(|(uid, dn)| if dn.is_empty() { json::short_sender(uid) } else { dn.clone() })
            {
                room.name = label;
                room.is_dm = true;
            }
        }
        if up.encrypted {
            room.encrypted = true;
        }
        // A room appears under rooms.invite while pending, then rooms.join once
        // accepted; track that so the list/menu can offer accept/reject.
        room.invited = up.invited;
        for member in up.members {
            if !room.members.contains(&member) {
                room.members.push(member);
            }
        }
        room.last_ts = room.last_ts.max(up.last_ts);
        // Mark unread only for incremental deltas (a full sync re-delivers
        // history and must not flag everything unread).
        let is_open = OPEN_ROOM.borrow().as_deref() == Some(room.id.as_str());
        if mark_unread && !up.messages.is_empty() && !is_open {
            room.unread = true;
        }
        // A full sync carries the recent timeline; replace rather than append so
        // re-entering does not duplicate the last messages.
        if full {
            room.messages.clear();
        }
        model::push_messages(&mut room.messages, up.messages, RETAIN);
    }
    // Drop rooms the server reports as left, so a stale copy cannot linger next
    // to a freshly created room of the same name (re-added by a timeout=0 sync
    // before the leave propagated).
    for id in &result.left {
        rooms.retain(|r| &r.id != id);
    }
    drop(rooms);
    if let Some(open) = OPEN_ROOM.borrow().clone() {
        if result.left.iter().any(|id| id == &open) {
            *OPEN_ROOM.borrow_mut() = None;
        }
    }
}

fn open_chat(index: u32) {
    let (id, invited) = match ROOMS.borrow().get(index as usize) {
        Some(r) => (r.id.clone(), r.invited),
        None => return,
    };
    // A pending invitation is not joined yet; opening it as a chat would show an
    // unreadable empty room. Prompt to accept the invitation instead.
    if invited {
        prompt_accept_invite(&id);
        return;
    }
    *OPEN_ROOM.borrow_mut() = Some(id.clone());
    set_view(View::Chat);
    *SCROLL.borrow_mut() = 0;
    {
        let mut rooms = ROOMS.borrow_mut();
        if let Some(r) = rooms.iter_mut().find(|r| r.id == id) {
            r.unread = false;
        }
    }
    // A freshly joined room has an empty /sync timeline (sync only carries
    // post-join events); pull recent history once so the chat is not blank.
    let empty = ROOMS
        .borrow()
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.messages.is_empty())
        .unwrap_or(false);
    if empty {
        backfill_history(&id);
    }
    let rooms = ROOMS.borrow();
    if let Some(r) = rooms.iter().find(|r| r.id == id) {
        ui_views::push_chat(r);
    }
}

/// One-shot history backfill for a room whose `/sync` timeline is empty (e.g.
/// just after joining): pull recent messages via back-pagination and decrypt the
/// ones we hold keys for.
fn backfill_history(id: &str) {
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return,
    };
    let body = match client::room_messages(&sess, id, 20) {
        Some(b) => b,
        None => return,
    };
    let mut msgs = json::parse_messages_chunk(&body);
    if msgs.is_empty() {
        return;
    }
    {
        let mut guard = CRYPTO.borrow_mut();
        if let Some(c) = guard.as_mut() {
            for m in msgs.iter_mut() {
                if let Some(enc) = m.enc.clone() {
                    if let Some(plain) = c.decrypt_megolm(&enc.session_id, &enc.ciphertext) {
                        m.body = plain;
                        m.encrypted = false;
                        m.enc = None;
                    }
                }
            }
        }
    }
    if let Some(r) = ROOMS.borrow_mut().iter_mut().find(|r| r.id == id) {
        if r.messages.is_empty() {
            model::push_messages(&mut r.messages, msgs, RETAIN);
        }
    }
}

/// Confirm dialog (showing the room name) to accept a pending invitation.
fn prompt_accept_invite(id: &str) {
    let name = ROOMS
        .borrow()
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.name.clone())
        .unwrap_or_else(|| id.to_string());
    *MENU_TARGET.borrow_mut() = Some(id.to_string());
    let msg = alloc::format!("{}: {}", name, i18n::tr_key("invite_confirm"));
    ui::push_confirm(&msg, ui::UI_ICON_ALERT, ACT_INVITE_CONFIRM);
}

/// Join the room in MENU_TARGET (accept its invitation) and refresh the list.
fn accept_invite() {
    // Clone out of the cells first so no RefCell borrow is held across sync_now
    // (which re-borrows SESSION/ROOMS/CRYPTO).
    let id = MENU_TARGET.borrow().clone();
    let sess = SESSION.borrow().clone();
    if let (Some(id), Some(sess)) = (id, sess) {
        if client::join(&sess, &id).is_some() {
            if let Some(r) = ROOMS.borrow_mut().iter_mut().find(|r| r.id == id) {
                r.invited = false;
            }
            sync_now(true);
            ui::push_toast(i18n::tr_key("joined"), ui::UI_ICON_SUCCESS, 1000);
        } else {
            ui::push_toast(i18n::tr_key("join_failed"), ui::UI_ICON_ERROR, 1500);
        }
    }
    render_rooms();
}

/// Invite a user (MXID) to the currently open room.
fn invite_user(mxid: &str) {
    let room = OPEN_ROOM.borrow().clone();
    let sess = SESSION.borrow().clone();
    if let (Some(room), Some(sess)) = (room, sess) {
        if client::invite(&sess, &room, mxid) {
            ui::push_toast(i18n::tr_key("invited"), ui::UI_ICON_SUCCESS, 1000);
        } else {
            ui::push_toast(i18n::tr_key("invite_failed"), ui::UI_ICON_ERROR, 1500);
        }
    }
}

fn ctx_pick(item: u32) {
    match item {
        CTX_JOIN => {
            // Suppress the auto-poll / list re-render while the T9 is open.
            set_view(View::Composing);
            ui::push_t9_input(i18n::tr_key("new_room_prompt"), None, 200, ACT_NEW_ROOM_DONE);
        }
        CTX_CREATE => {
            set_view(View::Composing);
            ui::push_t9_input(i18n::tr_key("create_prompt"), None, 200, ACT_CREATE_ROOM_DONE);
        }
        CTX_INVITE => {
            set_view(View::Composing);
            ui::push_t9_input(i18n::tr_key("invite_user_prompt"), None, 200, ACT_INVITE_USER_DONE);
        }
        CTX_ACCEPT => accept_invite(),
        CTX_REJECT => {
            let id = MENU_TARGET.borrow().clone();
            if let Some(id) = id {
                if let Some(sess) = SESSION.borrow().clone() {
                    let _ = client::leave(&sess, &id);
                }
                ROOMS.borrow_mut().retain(|r| r.id != id);
            }
            render_rooms();
        }
        CTX_DELETE => {
            if let Some(id) = MENU_TARGET.borrow().clone() {
                let name = ROOMS
                    .borrow()
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| id.clone());
                *PENDING_DELETE.borrow_mut() = Some(id);
                let msg = alloc::format!("{}: {}", name, i18n::tr_key("delete_confirm"));
                ui::push_confirm(&msg, ui::UI_ICON_ALERT, ACT_DEL_CONFIRM);
            }
        }
        CTX_SYSTEM => {
            let cfg = store::load_config();
            ui_views::render_system(&cfg, false);
            set_view(View::System);
        }
        _ => {}
    }
}

fn sys_select(item: u32) {
    let cfg = store::load_config();
    if ui_views::open_field_editor(item, &cfg) {
        // The field editor sits on top of the system list; mark the state so a
        // stray N during editing is not mistaken for "leave the system list".
        // The ACT_SYS_*_DONE handler restores View::System on confirm or cancel.
        set_view(View::EditingField);
        return;
    }
    match item {
        SYS_LOGIN => do_login(),
        SYS_RESET => {
            ui::push_confirm(i18n::tr_key("reset_confirm"), ui::UI_ICON_ALERT, ACT_SYS_RESET_CONFIRM);
        }
        _ => {}
    }
}

fn do_login() {
    let cfg = store::load_config();
    let pass = store::load_password().unwrap_or_default();
    if cfg.server.is_empty() || cfg.user.is_empty() || pass.is_empty() {
        ui::push_toast(i18n::tr_key("login_failed"), ui::UI_ICON_ERROR, 1500);
        return;
    }
    // Drawn synchronously by push_toast before the blocking login HTTP, so the
    // badge shows progress instead of looking frozen.
    ui::push_toast(i18n::tr_key("connecting"), ui::UI_ICON_NONE, 0);
    // An explicit login always registers a FRESH device (device_id = None) so the
    // new device publishes the current account identity; a re-login can never land
    // a reinitialised account on a stale device_id (which broke incoming Olm). The
    // boot/restore path keeps the stored device_id, so this is not proliferation.
    match client::login(&cfg.server, &cfg.user, &pass, None) {
        Some(r) => {
            store::save_login(&r.user_id, &r.device_id, &r.token);
            *SESSION.borrow_mut() = store::load_session();
            // Fresh device: must (re)publish device + one-time keys.
            *UPLOADED.borrow_mut() = false;
            ui::pop_to_plugin();
            set_view(View::RoomList);
            init_crypto();
            sync_now(true);
            if let Some(s) = SESSION.borrow().clone() {
                ensure_uploaded(&s);
            }
            render_rooms();
            ui::push_toast(i18n::tr_key("logged_in"), ui::UI_ICON_SUCCESS, 1000);
        }
        None => ui::push_toast(i18n::tr_key("login_failed"), ui::UI_ICON_ERROR, 1500),
    }
}

fn new_room(alias: &str) {
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return,
    };
    // A full MXID (@user:server) starts a direct chat (create room + invite);
    // anything else is a room id or alias to join.
    let result = if alias.starts_with('@') {
        client::create_dm(&sess, alias)
    } else {
        client::join(&sess, alias)
    };
    match result {
        Some(room_id) => {
            // Add the room right away so it shows even before the next sync
            // reflects the join (an immediate timeout=0 sync may not yet).
            {
                let mut rooms = ROOMS.borrow_mut();
                if !rooms.iter().any(|r| r.id == room_id) {
                    rooms.push(Room::new(room_id));
                }
            }
            sync_now(true);
            ui::push_toast(i18n::tr_key("joined"), ui::UI_ICON_SUCCESS, 1000);
        }
        None => ui::push_toast(i18n::tr_key("join_failed"), ui::UI_ICON_ERROR, 1500),
    }
}

/// Create a brand-new encrypted room named `name` and show it in the list.
fn create_room(name: &str) {
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return,
    };
    match client::create_room(&sess, name) {
        Some(room_id) => {
            {
                let mut rooms = ROOMS.borrow_mut();
                if !rooms.iter().any(|r| r.id == room_id) {
                    rooms.push(Room::new(room_id));
                }
            }
            sync_now(true);
            ui::push_toast(i18n::tr_key("created"), ui::UI_ICON_SUCCESS, 1000);
        }
        None => ui::push_toast(i18n::tr_key("join_failed"), ui::UI_ICON_ERROR, 1500),
    }
}

fn delete_confirmed() {
    if let Some(id) = PENDING_DELETE.borrow_mut().take() {
        if let Some(sess) = SESSION.borrow().clone() {
            let _ = client::leave(&sess, &id);
        }
        ROOMS.borrow_mut().retain(|r| r.id != id);
    }
}

/// Publish our device keys + one-time keys once per run.
fn ensure_uploaded(sess: &Session) {
    if *UPLOADED.borrow() {
        return;
    }
    // Replenish only when the server is low on our one-time keys. Re-uploading
    // keys it still holds is rejected with HTTP 400 ("already exists"); a fresh
    // device reports a low count here and uploads, and as keys are claimed the
    // count drops so a later open tops the pool back up. Must run after a sync,
    // which is what populates OTK_COUNT.
    if *OTK_COUNT.borrow() >= OTK_TARGET {
        *UPLOADED.borrow_mut() = true;
        return;
    }
    let payload = {
        let mut guard = CRYPTO.borrow_mut();
        guard.as_mut().map(|c| {
            (
                c.device_keys_json(&sess.user_id, &sess.device_id),
                c.one_time_keys_json(&sess.user_id, &sess.device_id, 30),
                c.account_pickle(),
            )
        })
    };
    if let Some((dk, otks, pickle)) = payload {
        let status = client::keys_upload(sess, &dk, &otks);
        let _ = store::save_account(&pickle);
        if status / 100 == 2 {
            *UPLOADED.borrow_mut() = true;
        }
    }
}

fn build_claim_json(devices: &[json::DeviceKey]) -> String {
    let mut by_user: alloc::collections::BTreeMap<&str, Vec<&str>> =
        alloc::collections::BTreeMap::new();
    for d in devices {
        by_user.entry(&d.user_id).or_default().push(&d.device_id);
    }
    let users: Vec<String> = by_user
        .iter()
        .map(|(u, devs)| {
            let inner: Vec<String> = devs
                .iter()
                .map(|dev| alloc::format!("\"{}\":\"signed_curve25519\"", json::json_escape(dev)))
                .collect();
            alloc::format!("\"{}\":{{{}}}", json::json_escape(u), inner.join(","))
        })
        .collect();
    alloc::format!("{{{}}}", users.join(","))
}

#[allow(clippy::too_many_arguments)]
fn room_key_plaintext(
    sess: &Session,
    recipient_user: &str,
    recipient_ed: &str,
    room_id: &str,
    session_id: &str,
    session_key: &str,
    my_ed: &str,
) -> String {
    alloc::format!(
        "{{\"type\":\"m.room_key\",\"content\":{{\"algorithm\":\"m.megolm.v1.aes-sha2\",\"room_id\":\"{room}\",\"session_id\":\"{sid}\",\"session_key\":\"{skey}\"}},\"sender\":\"{sender}\",\"sender_device\":\"{sdev}\",\"keys\":{{\"ed25519\":\"{myed}\"}},\"recipient\":\"{rcpt}\",\"recipient_keys\":{{\"ed25519\":\"{red}\"}}}}",
        room = json::json_escape(room_id),
        sid = json::json_escape(session_id),
        skey = json::json_escape(session_key),
        sender = json::json_escape(&sess.user_id),
        sdev = json::json_escape(&sess.device_id),
        myed = json::json_escape(my_ed),
        rcpt = json::json_escape(recipient_user),
        red = json::json_escape(recipient_ed),
    )
}

/// One recipient device awaiting an Olm-wrapped room key. `otk` is `Some` when no
/// Olm session exists yet (claim it, then establish); `None` reuses the session.
struct OlmTarget {
    user_id: String,
    device_id: String,
    curve25519: String,
    ed25519: String,
    otk: Option<String>,
}

/// Background room-key distribution job, drained `DIST_CHUNK` devices per tick.
/// To-device payloads accumulate in `msgs` across chunks and are sent as a single
/// `sendToDevice` request once all recipients are processed (one round-trip, not
/// one per chunk).
struct PendingDist {
    sess: Session,
    room_id: String,
    session_id: String,
    session_key: String,
    my_curve: String,
    my_ed: String,
    targets: Vec<OlmTarget>,
    next: usize,
    msgs: alloc::collections::BTreeMap<String, Vec<String>>,
}

/// Prepare a background room-key distribution for a freshly created outbound
/// session: query member devices, claim one-time keys for the ones without an
/// Olm session, and queue the recipients in [`PENDING_DIST`]. The per-device Olm
/// establish/encrypt runs later in [`process_distribution`], chunked across ticks
/// so a single host call never exceeds WAMR's instruction limit.
#[allow(clippy::too_many_arguments)]
fn enqueue_distribution(
    sess: &Session,
    room_id: &str,
    session_id: &str,
    session_key: &str,
    my_curve: &str,
    my_ed: &str,
) {
    let members: Vec<String> = ROOMS
        .borrow()
        .iter()
        .find(|r| r.id == room_id)
        .map(|r| r.members.clone())
        .unwrap_or_default();

    let mut devices = match client::keys_query(sess, &members) {
        Some(b) => json::parse_keys_query(&b),
        None => Vec::new(),
    };
    devices.retain(|d| d.curve25519 != my_curve);

    // Only devices without a cached Olm session need a freshly claimed one-time
    // key; devices with a persisted session reuse it (the cache that keeps repeat
    // distributions fast). Receiving never clobbers these send sessions.
    let need: Vec<json::DeviceKey> = devices
        .iter()
        .filter(|d| {
            !CRYPTO
                .borrow()
                .as_ref()
                .map(|c| c.has_olm_session(&d.curve25519))
                .unwrap_or(false)
        })
        .map(|d| json::DeviceKey {
            user_id: d.user_id.clone(),
            device_id: d.device_id.clone(),
            curve25519: d.curve25519.clone(),
            ed25519: d.ed25519.clone(),
        })
        .collect();
    let mut otk_by_curve: alloc::collections::BTreeMap<String, String> =
        alloc::collections::BTreeMap::new();
    if !need.is_empty() {
        match client::keys_claim(sess, &build_claim_json(&need)) {
            Some(body) => {
                let otks = json::parse_keys_claim(&body);
                log::info(TAG, &alloc::format!("claim: need={} got={}", need.len(), otks.len()));
                for otk in otks {
                    if let Some(d) = devices
                        .iter()
                        .find(|d| d.user_id == otk.user_id && d.device_id == otk.device_id)
                    {
                        otk_by_curve.insert(d.curve25519.clone(), otk.key);
                    }
                }
            }
            None => log::warn(TAG, "keys/claim failed"),
        }
    }

    // Reuse an existing Olm session (otk=None) or attach the freshly claimed
    // one-time key; drop devices we can neither reuse nor claim.
    let targets: Vec<OlmTarget> = devices
        .into_iter()
        .filter_map(|d| {
            let has_session = CRYPTO
                .borrow()
                .as_ref()
                .map(|c| c.has_olm_session(&d.curve25519))
                .unwrap_or(false);
            let otk = if has_session {
                None
            } else {
                Some(otk_by_curve.remove(&d.curve25519)?)
            };
            Some(OlmTarget {
                user_id: d.user_id,
                device_id: d.device_id,
                curve25519: d.curve25519,
                ed25519: d.ed25519,
                otk,
            })
        })
        .collect();

    log::info(
        TAG,
        &alloc::format!("distribute room={} members={} devices={} chunked", room_id, members.len(), targets.len()),
    );
    if targets.is_empty() {
        return;
    }
    *PENDING_DIST.borrow_mut() = Some(PendingDist {
        sess: sess.clone(),
        room_id: room_id.to_string(),
        session_id: session_id.to_string(),
        session_key: session_key.to_string(),
        my_curve: my_curve.to_string(),
        my_ed: my_ed.to_string(),
        targets,
        next: 0,
        msgs: alloc::collections::BTreeMap::new(),
    });
}

/// Drain up to [`DIST_CHUNK`] queued recipients: establish any missing Olm
/// session and Olm-wrap the room key into the accumulator. Once the last
/// recipient is processed, send everything in a single `sendToDevice` and clear
/// the job. No-op when nothing is queued.
fn process_distribution() {
    let mut guard = PENDING_DIST.borrow_mut();
    let pd = match guard.as_mut() {
        Some(p) => p,
        None => return,
    };
    let end = (pd.next + DIST_CHUNK).min(pd.targets.len());
    log::info(TAG, &alloc::format!("dist tick {}->{}/{} t={}ms", pd.next, end, pd.targets.len(), time::uptime_ms()));
    // Progress on a second line so it fits the toast box (count over total).
    ui::push_toast(
        &alloc::format!("{}\n{}/{}", i18n::tr_key("distributing"), pd.next, pd.targets.len()),
        ui::UI_ICON_NONE,
        0,
    );

    {
        let mut cg = CRYPTO.borrow_mut();
        let c = match cg.as_mut() {
            Some(c) => c,
            None => {
                *guard = None;
                return;
            }
        };
        for t in &pd.targets[pd.next..end] {
            if !c.has_olm_session(&t.curve25519) {
                match &t.otk {
                    Some(otk) => {
                        c.establish_olm(&t.curve25519, otk);
                    }
                    None => continue,
                }
                if !c.has_olm_session(&t.curve25519) {
                    continue;
                }
            }
            let plaintext = room_key_plaintext(
                &pd.sess,
                &t.user_id,
                &t.ed25519,
                &pd.room_id,
                &pd.session_id,
                &pd.session_key,
                &pd.my_ed,
            );
            if let Some((kind, body)) = c.olm_encrypt(&t.curve25519, &plaintext) {
                let content = alloc::format!(
                    "{{\"algorithm\":\"m.olm.v1.curve25519-aes-sha2\",\"sender_key\":\"{sk}\",\"ciphertext\":{{\"{dc}\":{{\"type\":{t2},\"body\":\"{b}\"}}}}}}",
                    sk = json::json_escape(&pd.my_curve),
                    dc = json::json_escape(&t.curve25519),
                    t2 = kind,
                    b = json::json_escape(&body),
                );
                pd.msgs
                    .entry(t.user_id.clone())
                    .or_default()
                    .push(alloc::format!("\"{}\":{}", json::json_escape(&t.device_id), content));
            }
        }
    }
    pd.next = end;

    // Send this chunk's to-device payloads now and drop them, so the accumulator
    // never holds more than one chunk's worth of Olm messages. Keeping the whole
    // room's payloads until the end spikes the guest heap on the final assembly
    // (the trap site); flushing per chunk caps that peak at DIST_CHUNK devices.
    if !pd.msgs.is_empty() {
        let messages = alloc::format!(
            "{{{}}}",
            pd.msgs
                .iter()
                .map(|(u, devs)| alloc::format!("\"{}\":{{{}}}", json::json_escape(u), devs.join(",")))
                .collect::<Vec<_>>()
                .join(",")
        );
        client::send_to_device(&pd.sess, &messages, next_txn());
        pd.msgs.clear();
    }

    if pd.next >= pd.targets.len() {
        log::info(TAG, "distribute done");
        *guard = None;
        // Persist the Olm sessions established this round so the next boot skips
        // the per-device X25519 handshake (need=0).
        persist_crypto();
        // Replaces the persistent progress toast with an auto-dismissing one.
        ui::push_toast(i18n::tr_key("distributed"), ui::UI_ICON_SUCCESS, 1500);
    }
}

/// Encrypt and send a text message into an E2EE room. The room key is
/// distributed only when the outbound session is newly created this boot;
/// outbound/Olm sessions are kept in RAM (not persisted), so sending never
/// writes to flash.
fn e2ee_send(room_id: &str, text: &str) -> bool {
    let sess = match SESSION.borrow().clone() {
        Some(s) => s,
        None => return false,
    };
    let (my_curve, my_ed) = match CRYPTO.borrow().as_ref() {
        Some(c) if c.has_backup() => (c.identity_curve25519(), c.identity_ed25519()),
        // No armed backup/recovery key: refuse before the heavy Olm distribution.
        _ => return false,
    };

    let (session_id, session_key, is_new) = match CRYPTO.borrow_mut().as_mut() {
        Some(c) => c.ensure_outbound(room_id),
        None => return false,
    };
    log::info(TAG, &alloc::format!("e2ee_send room={} new_session={}", room_id, is_new));

    // Encrypt and send the message now (fast); `session_key` was captured at
    // chain index 0 above, so the first message stays decryptable once the key
    // reaches recipients via the background distribution.
    let event = alloc::format!(
        "{{\"type\":\"m.room.message\",\"content\":{{\"msgtype\":\"m.text\",\"body\":\"{}\"}},\"room_id\":\"{}\"}}",
        json::json_escape(text),
        json::json_escape(room_id),
    );
    let ciphertext = match CRYPTO.borrow_mut().as_mut().and_then(|c| c.encrypt_megolm(room_id, &event)) {
        Some(ct) => ct,
        None => return false,
    };
    let content = alloc::format!(
        "{{\"algorithm\":\"m.megolm.v1.aes-sha2\",\"sender_key\":\"{sk}\",\"ciphertext\":\"{ct}\",\"session_id\":\"{sid}\",\"device_id\":\"{dev}\"}}",
        sk = json::json_escape(&my_curve),
        ct = json::json_escape(&ciphertext),
        sid = json::json_escape(&session_id),
        dev = json::json_escape(&sess.device_id),
    );
    let sent = client::send_encrypted(&sess, room_id, &content, next_txn());

    // Queue the room-key distribution for a freshly created outbound session;
    // `process_distribution` drains it in chunks from `plugin_on_tick`.
    if is_new {
        // Persist the freshly created inbound counterpart now so the sender can
        // still decrypt this message after a reboot even if distribution aborts.
        persist_crypto();
        ensure_uploaded(&sess);
        // Upload the room key to the server-side backup so devices that were
        // offline at send time (or log in later) can restore it.
        backup_room_key(&sess, room_id, &session_id);
        enqueue_distribution(&sess, room_id, &session_id, &session_key, &my_curve, &my_ed);
    }
    sent
}

fn reply_done() {
    let text = match ui::consume_input_text(400) {
        Some(t) => t,
        None => {
            set_view(View::Chat);
            return;
        }
    };
    let text = text.trim().to_string();
    let room_id = match OPEN_ROOM.borrow().clone() {
        Some(id) => id,
        None => {
            set_view(View::Chat);
            return;
        }
    };
    if !text.is_empty() {
        let encrypted = ROOMS
            .borrow()
            .iter()
            .find(|r| r.id == room_id)
            .map(|r| r.encrypted)
            .unwrap_or(false);
        // Sending blocks on HTTP (and, for E2EE rooms, key distribution); show a
        // toast first so the badge does not look frozen.
        ui::push_toast(i18n::tr_key("sending"), ui::UI_ICON_NONE, 1500);
        let sent = if encrypted {
            e2ee_send(&room_id, &text)
        } else if let Some(sess) = SESSION.borrow().clone() {
            client::send_text(&sess, &room_id, &text, next_txn())
        } else {
            false
        };
        if sent {
            // Refresh straight away so the just-sent message shows once, fetched
            // from the server (decryptable via our own inbound mirror), instead
            // of a local echo that the next sync would duplicate.
            sync_now(false);
        } else {
            ui::push_toast(i18n::tr_key("send_failed"), ui::UI_ICON_ERROR, 1500);
        }
    }
    // The T9 view popped itself, revealing the chat canvas underneath; redraw it
    // in place, anchored to the newest message.
    *SCROLL.borrow_mut() = 0;
    show_open_chat();
}

/// Redraw the chat canvas for the currently open room in place (the canvas view
/// stays on the stack across the reply T9, so this never pushes a new view).
fn show_open_chat() {
    let id = match OPEN_ROOM.borrow().clone() {
        Some(id) => id,
        None => return,
    };
    set_view(View::Chat);
    let scroll = *SCROLL.borrow();
    let rooms = ROOMS.borrow();
    if let Some(r) = rooms.iter().find(|r| r.id == id) {
        *SCROLL.borrow_mut() = ui_views::draw_chat(r, scroll);
    }
}

/// EventBus key handler (delivered for every key press). The chat canvas owns
/// its own keys via [`chat_key`], so this only drives the room list sync and the
/// host-popped System back transition. Input-view cancels (reply T9, system
/// field editors) arrive as their `*_DONE` action with `user_data = 0`, not here.
fn key_event(key: u32) {
    let view = *VIEW.borrow();
    match (key, view) {
        // The host pops the system list on N back to the room list.
        (KEY_N, View::System) => set_view(View::RoomList),
        (KEY_SYNC, View::RoomList) => {
            refresh_backup();
            sync_now(false);
            render_rooms();
        }
        _ => {}
    }
}

/// Canvas key callback while the chat is open (`user_data` = ASCII key code). The
/// canvas keeps the chat on screen, so Y/N/scroll are handled here directly.
/// True when the open room is encrypted and no backup/recovery key is armed.
/// In that state replying is blocked: the UI shows a hint instead of the reply
/// input and `e2ee_send` refuses early.
fn e2ee_reply_blocked() -> bool {
    let encrypted = OPEN_ROOM
        .borrow()
        .as_deref()
        .map(|id| {
            ROOMS
                .borrow()
                .iter()
                .find(|r| r.id.as_str() == id)
                .map(|r| r.encrypted)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !encrypted {
        return false;
    }
    !CRYPTO.borrow().as_ref().map(|c| c.has_backup()).unwrap_or(false)
}

fn chat_key(key: u32) {
    if *VIEW.borrow() != View::Chat {
        return;
    }
    match key {
        KEY_Y => {
            if e2ee_reply_blocked() {
                ui::push_toast(i18n::tr_key("e2ee_no_key"), ui::UI_ICON_ERROR, 2500);
                return;
            }
            ui::push_t9_input(i18n::tr_key("reply_prompt"), None, 400, ACT_CHAT_REPLY_DONE);
            set_view(View::Composing);
        }
        KEY_N => {
            // Pop only the chat canvas to reveal the room list underneath; a full
            // pop_to_plugin would also drop the list and the following render_rooms
            // would push a duplicate, leaving a stale ListView on the stack.
            ui::pop();
            *OPEN_ROOM.borrow_mut() = None;
            render_rooms();
        }
        KEY_UP | KEY_DOWN => {
            {
                let mut s = SCROLL.borrow_mut();
                *s = if key == KEY_UP { *s + 1 } else { s.saturating_sub(1) };
            }
            show_open_chat();
        }
        KEY_MENU => {
            // Target the open room so CTX_INVITE addresses it.
            *MENU_TARGET.borrow_mut() = OPEN_ROOM.borrow().clone();
            ui_views::open_chat_menu();
        }
        _ => {}
    }
}

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    // std crate: route panics to the host log so a trap is not just "unreachable".
    #[cfg(target_arch = "wasm32")]
    std::panic::set_hook(alloc::boxed::Box::new(|info| {
        log::error(TAG, &alloc::format!("{}", info));
    }));
    log::info(TAG, "init");
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    let _ = event::subscribe(event::KEY_PRESSED, ACT_KEY_EVENT);
    *SESSION.borrow_mut() = store::load_session();
    set_view(View::RoomList);
    if SESSION.borrow().is_some() {
        init_crypto();
        let dev = SESSION.borrow().as_ref().map(|s| s.device_id.clone()).unwrap_or_default();
        if let Some(c) = CRYPTO.borrow().as_ref() {
            log::info(TAG, &alloc::format!("identity curve={} device={}", c.identity_curve25519(), dev));
        }
        sync_now(true);
        // Publish our device + one-time keys (only when the server is low on
        // ours) so other devices can claim an OTK and Olm-encrypt their room keys
        // to us. Runs after the sync so OTK_COUNT reflects the server-side pool.
        if let Some(s) = SESSION.borrow().clone() {
            ensure_uploaded(&s);
        }
    }
    *LAST_POLL_MS.borrow_mut() = time::uptime_ms();
    render_rooms();
    0
}

/// Messages currently held for the open room (0 if none open).
fn open_room_len() -> usize {
    let id = match OPEN_ROOM.borrow().clone() {
        Some(i) => i,
        None => return 0,
    };
    ROOMS.borrow().iter().find(|r| r.id == id).map(|r| r.messages.len()).unwrap_or(0)
}

/// Signature of the visible room-list state (count, names, timestamps, unread /
/// invited / dm flags); the poll re-renders only when this changes, sparing the
/// e-paper a needless refresh every interval.
fn rooms_sig() -> u64 {
    let rooms = ROOMS.borrow();
    let mut h = fnv1a(&(rooms.len() as u64).to_le_bytes());
    for r in rooms.iter() {
        h ^= fnv1a(r.name.as_bytes());
        h = h.wrapping_mul(0x100_0000_01b3);
        h ^= r.last_ts;
        h = h.wrapping_mul(0x100_0000_01b3);
        h ^= (r.unread as u64) | ((r.invited as u64) << 1) | ((r.is_dm as u64) << 2);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Periodic host tick (every ~50 ms). Auto-refreshes the room list or the open
/// chat every [`POLL_INTERVAL_MS`]; serialized with `plugin_on_action` by the
/// host. The chat canvas is redrawn only when new messages actually arrived, so
/// an idle chat does not trigger a panel refresh every interval.
#[no_mangle]
pub extern "C" fn plugin_on_tick(lo: u32, hi: u32) -> i32 {
    // A queued room-key distribution gets this whole tick (chunked Olm work);
    // skip the sync poll so one host call stays under the instruction limit.
    if PENDING_DIST.borrow().is_some() {
        process_distribution();
        return 0;
    }
    let now = ((hi as u64) << 32) | (lo as u64);
    let view = *VIEW.borrow();
    let pollable = matches!(view, View::RoomList | View::Chat) && SESSION.borrow().is_some();
    if pollable && now.wrapping_sub(*LAST_POLL_MS.borrow()) >= POLL_INTERVAL_MS {
        *LAST_POLL_MS.borrow_mut() = now;
        let before = open_room_len();
        let before_sig = rooms_sig();
        sync_now(false);
        match view {
            View::Chat => {
                if open_room_len() != before {
                    show_open_chat();
                }
            }
            // Room list: only redraw when the visible list actually changed.
            _ => {
                if rooms_sig() != before_sig {
                    render_rooms();
                }
            }
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    // Flush the sync token once on leave instead of on every sync.
    if let Some(s) = SESSION.borrow().as_ref() {
        if !s.next_batch.is_empty() {
            store::save_sync_token(&s.next_batch);
        }
    }
    ROOMS.borrow_mut().clear();
    *OPEN_ROOM.borrow_mut() = None;
    *MENU_TARGET.borrow_mut() = None;
    *PENDING_DELETE.borrow_mut() = None;
    *PENDING_DIST.borrow_mut() = None;
    *CRYPTO.borrow_mut() = None;
    set_view(View::RoomList);
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _position: u32, user_data: u32) -> i32 {
    match action_id {
        // user_data is the selected item's id (its stable storage index in ROOMS,
        // set by render_room_list); idx is only the on-screen position, which
        // differs from storage order because the list is displayed sorted. Bind
        // the command to the item id so it always targets the selected room.
        ACT_ROOM_SELECT => open_chat(user_data),
        ACT_ROOM_MENU => {
            let invited = {
                let rooms = ROOMS.borrow();
                let target = rooms.get(user_data as usize);
                *MENU_TARGET.borrow_mut() = target.map(|r| r.id.clone());
                target.map(|r| r.invited).unwrap_or(false)
            };
            ui_views::open_room_menu(invited);
        }
        ACT_ROOM_CTX_PICK => ctx_pick(user_data),

        ACT_NEW_ROOM_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(200) {
                    let t = t.trim();
                    if !t.is_empty() {
                        new_room(t);
                    }
                }
            }
            render_rooms();
        }
        ACT_CREATE_ROOM_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(200) {
                    let t = t.trim();
                    if !t.is_empty() {
                        create_room(t);
                    }
                }
            }
            render_rooms();
        }
        ACT_DEL_CONFIRM => {
            if user_data == 1 {
                delete_confirmed();
            } else {
                PENDING_DELETE.borrow_mut().take();
            }
            render_rooms();
        }
        ACT_INVITE_CONFIRM => {
            if user_data == 1 {
                accept_invite();
            } else {
                render_rooms();
            }
        }
        ACT_INVITE_USER_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(200) {
                    let t = t.trim();
                    if !t.is_empty() {
                        invite_user(t);
                    }
                }
            }
            show_open_chat();
        }

        ACT_SYS_SELECT => sys_select(user_data),
        ACT_SYS_USER_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(128) {
                    store::save_user(t.trim());
                }
            }
            let cfg = store::load_config();
            ui_views::render_system(&cfg, true);
            set_view(View::System);
        }
        ACT_SYS_PASS_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(128) {
                    store::save_password(&t);
                }
            }
            let cfg = store::load_config();
            ui_views::render_system(&cfg, true);
            set_view(View::System);
        }
        ACT_SYS_SRV_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(200) {
                    store::save_server(&normalize_server(&t));
                }
            }
            let cfg = store::load_config();
            ui_views::render_system(&cfg, true);
            set_view(View::System);
        }
        ACT_SYS_KEY_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(120) {
                    store::save_recovery_key(t.trim());
                    init_crypto();
                }
            }
            let cfg = store::load_config();
            ui_views::render_system(&cfg, true);
            set_view(View::System);
        }
        ACT_SYS_RESET_CONFIRM if user_data == 1 => {
            store::reset_all();
            *SESSION.borrow_mut() = None;
            *CRYPTO.borrow_mut() = None;
            *UPLOADED.borrow_mut() = false;
            *CRYPTO_HASH.borrow_mut() = 0;
            ROOMS.borrow_mut().clear();
            ui::pop_to_plugin();
            set_view(View::RoomList);
            render_rooms();
            ui::push_toast(i18n::tr_key("reset_done"), ui::UI_ICON_SUCCESS, 1000);
        }

        ACT_CHAT_KEY => chat_key(user_data),
        ACT_CHAT_REPLY_DONE => {
            if user_data == 1 {
                reply_done();
            } else {
                show_open_chat();
            }
        }

        ACT_KEY_EVENT => key_event(user_data),
        _ => {}
    }
    0
}
