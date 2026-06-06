# Matrix plugin

A minimal [Matrix](https://matrix.org) chat client for the CDC Badge: send and
receive plain-text messages, including in end-to-end-encrypted rooms.

## What it does

- Room list with a `[3]` context menu: **New room** / **Delete room** / **System**.
- Open a room to read its last 10 messages in a scrolling info view.
- Reply: press **Y** in the chat view to open T9, then **Y** again to send.
- E2EE: decrypts incoming encrypted messages and sends encrypted messages in
  encrypted rooms (see below). Trust-on-first-use; no device verification.

Text messages (`m.text`) only. No media, edits, or message deletion.

## Setup

1. Open the plugin, press `[3]` and choose **System**.
2. Fill in the fields (press **Y** on a row to edit):
   - **User** - your Matrix user (localpart or full `@user:server`).
   - **Password** - stored in the secure element (rmem).
   - **Server URL** - your homeserver, e.g. `https://matrix.org`.
   - **E2EE key** - your Matrix **recovery key** (security key). See below.
3. Choose **Login & save**.

Press `[1]` on the room list to sync. New rooms are joined by id or alias via
the `[3] -> New room` entry.

## E2EE key (recovery key)

The recovery key is the Matrix **security key** from your account's key backup
(in Element: *Settings -> Security & Privacy -> Secure Backup*). It looks like
`EsT? ???? ???? ???? ...` (base58, grouped in fours).

It is long and awkward to type on the T9 keypad. **Enter it via serial paste:**
open the **System -> E2EE key** field so the T9 input is active, then send

```
PASTE <your recovery key>
```

over the badge's USB serial console. The pasted text lands in the open T9 field;
press **Y** to confirm. The key is stored in the secure element (rmem slot
`mtx_reckey`).

### How E2EE works here

- **Receiving:** the plugin reads the server-side key backup
  (`m.megolm_backup.v1.curve25519-aes-sha2`), decrypts the Megolm room keys with
  the recovery key, and decrypts the room timeline. Requires that key backup is
  enabled on your account and that the sending devices have uploaded their keys
  to it. Messages whose keys are not (yet) in the backup show as `(encrypted)`.
- **Sending:** the plugin uploads its device keys, creates a Megolm session per
  room, distributes the room key to member devices over Olm to-device messages,
  and sends `m.room.encrypted` events.

## Storage

- Secure element (rmem): `mtx_token`, `mtx_pass`, `mtx_reckey`, `mtx_pickle`
  (the Olm account and the Megolm/Olm session snapshot are sealed with a key
  derived from `mtx_pickle` and kept in NVS).
- NVS (`plg_matrix`): server URL, user, device id, sync token (flushed on exit),
  room cache, the sealed account (`acct`) and inbound-key snapshot (`mg_state`).

**Reset module** in the System menu wipes all of the above.

## Building

The badge plugins are built with `tools/build_all.sh`. This plugin links
[vodozemac](https://github.com/matrix-org/vodozemac) and is therefore a `std`
crate, unlike the other (`no_std`) plugins; the build script builds it in its own
`cargo` invocation so feature unification does not leak `std` into the others.

```
tools/build_all.sh        # produces dist/matrix.wasm + dist/matrix.meta.json
cargo test -p matrix      # host-side unit tests (parsing, crypto round-trips)
```

## Limitations

- Recovery-key receive relies on the server-side key backup; live messages whose
  keys are not yet backed up may not decrypt until the next backup refresh.
- No cross-signing, device verification, or key rotation handling.
- Received room keys are persisted (sealed) in NVS, so the backup is only fetched
  on first run; press `[1]` to refresh keys from the backup later. Outbound and
  Olm sessions are not persisted (kept in RAM, rotated on reboot) to avoid a
  flash write per sent message - after a reboot the first message to a room
  re-shares its key once.
