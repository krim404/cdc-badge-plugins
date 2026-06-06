//! \file
//! \brief Plain data types for the Matrix client and small pure helpers.

use alloc::string::String;
use alloc::vec::Vec;

/// Messages retained per room in RAM (newest kept, older dropped).
pub const RETAIN: usize = 30;

/// User-editable configuration entered on the System screen.
#[derive(Default, Clone)]
pub struct Config {
    /// Homeserver base URL, e.g. `https://matrix.org`.
    pub server: String,
    /// Username as typed (localpart or full MXID), used for login + prefill.
    pub user: String,
}

/// An authenticated session, rehydrated from storage on enter.
#[derive(Clone)]
pub struct Session {
    pub base_url: String,
    pub token: String,
    /// Full MXID returned by `/login` (`@user:server`), used for the self echo.
    pub user_id: String,
    /// Device id from `/login`; consumed by E2EE key upload in a later phase.
    #[allow(dead_code)]
    pub device_id: String,
    /// `next_batch` sync token; empty means "no sync yet" (initial sync).
    pub next_batch: String,
}

/// Megolm payload of an `m.room.encrypted` event, kept until it is decrypted.
#[derive(Clone)]
pub struct EncInfo {
    pub session_id: String,
    pub ciphertext: String,
}

/// One displayed timeline message.
#[derive(Clone)]
pub struct Message {
    /// Short sender (localpart of the MXID).
    pub sender: String,
    /// Plaintext body (UTF-8). Empty + `encrypted` means not yet decryptable.
    pub body: String,
    /// True while this is an undecrypted `m.room.encrypted` event.
    pub encrypted: bool,
    /// Megolm payload for decryption; `None` once decrypted or for plaintext.
    pub enc: Option<EncInfo>,
}

/// A joined room with its retained recent messages.
#[derive(Clone)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub encrypted: bool,
    pub messages: Vec<Message>,
    /// Joined member MXIDs, used to address E2EE room-key distribution.
    pub members: Vec<String>,
    /// `origin_server_ts` (ms) of the latest message; drives list ordering.
    pub last_ts: u64,
    /// Set when new messages arrived while the room was not open.
    pub unread: bool,
    /// True for a resolved 1:1 direct chat (the name came from the other member,
    /// not a room name), so the list can mark it as a person.
    pub is_dm: bool,
    /// True for a pending invitation (rooms.invite) not yet joined.
    pub invited: bool,
}

impl Room {
    pub fn new(id: String) -> Self {
        Room {
            name: id.clone(),
            id,
            encrypted: false,
            messages: Vec::new(),
            members: Vec::new(),
            last_ts: 0,
            unread: false,
            is_dm: false,
            invited: false,
        }
    }
}

/// Append messages and keep only the newest [`RETAIN`].
pub fn push_messages(buf: &mut Vec<Message>, mut incoming: Vec<Message>, retain: usize) {
    buf.append(&mut incoming);
    if buf.len() > retain {
        let drop = buf.len() - retain;
        buf.drain(0..drop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn msg(b: &str) -> Message {
        Message {
            sender: "a".to_string(),
            body: b.to_string(),
            encrypted: false,
            enc: None,
        }
    }

    #[test]
    fn push_messages_trims_to_retain() {
        let mut buf = Vec::new();
        for i in 0..5 {
            push_messages(&mut buf, alloc::vec![msg(&i.to_string())], 3);
        }
        assert_eq!(buf.len(), 3);
        assert_eq!(buf[0].body, "2");
        assert_eq!(buf[2].body, "4");
    }

    #[test]
    fn push_messages_below_retain_keeps_all() {
        let mut buf = Vec::new();
        push_messages(&mut buf, alloc::vec![msg("x"), msg("y")], 10);
        assert_eq!(buf.len(), 2);
    }
}
