//! \file
//! \brief Action ids and context-menu item ids for `plugin_on_action` dispatch.
//!
//! The firmware delivers list/menu selects as `(action, position, item_id)`,
//! context-menu selects as `(action, position, item_id)`, T9/password confirm
//! as `(action, text_len, 1)`, confirm dialogs as `(action, 1|0, 0)`, and
//! EventBus key presses as `(action, 0, ascii_char)`.

// Room list (plugin root view).
pub const ACT_ROOM_SELECT: u32 = 10; // Y on a room -> open chat
pub const ACT_ROOM_MENU: u32 = 11; // [3] on the room list -> context menu
pub const ACT_ROOM_CTX_PICK: u32 = 12; // a context-menu entry was chosen

// Context-menu item ids (carried in user_data for ACT_ROOM_CTX_PICK).
pub const CTX_JOIN: u32 = 1; // join an existing room (id/alias) or start a DM (@user)
pub const CTX_DELETE: u32 = 2;
pub const CTX_SYSTEM: u32 = 3;
pub const CTX_CREATE: u32 = 4; // create a brand-new room
pub const CTX_ACCEPT: u32 = 5; // accept a room invitation (join)
pub const CTX_REJECT: u32 = 6; // reject a room invitation (leave)
pub const CTX_INVITE: u32 = 7; // invite a user to the currently open room

// Join / create / delete room.
pub const ACT_NEW_ROOM_DONE: u32 = 20; // T9 room id/alias/@user confirmed -> join/DM
pub const ACT_DEL_CONFIRM: u32 = 21; // delete confirm dialog
pub const ACT_CREATE_ROOM_DONE: u32 = 22; // T9 new-room name confirmed -> create
pub const ACT_INVITE_CONFIRM: u32 = 23; // accept-invitation confirm dialog
pub const ACT_INVITE_USER_DONE: u32 = 24; // T9 @user for invite-to-open-room confirmed

// System config screen (a list whose rows carry the SYS_* item ids).
pub const ACT_SYS_SELECT: u32 = 30; // a field row was picked
pub const ACT_SYS_USER_DONE: u32 = 31;
pub const ACT_SYS_PASS_DONE: u32 = 32;
pub const ACT_SYS_SRV_DONE: u32 = 33;
pub const ACT_SYS_KEY_DONE: u32 = 34; // recovery key (used by E2EE, stored now)
pub const ACT_SYS_RESET_CONFIRM: u32 = 35;

// System list row item ids (user_data for ACT_SYS_SELECT).
pub const SYS_USER: u32 = 1;
pub const SYS_PASS: u32 = 2;
pub const SYS_SRV: u32 = 3;
pub const SYS_KEY: u32 = 4;
pub const SYS_LOGIN: u32 = 5;
pub const SYS_RESET: u32 = 6;

// Chat view.
pub const ACT_CHAT_KEY: u32 = 40; // canvas key callback (user_data = ASCII key code)
pub const ACT_CHAT_REPLY_DONE: u32 = 41; // T9 reply confirmed -> send

// Background: EventBus KEY_PRESSED subscription.
pub const ACT_KEY_EVENT: u32 = 50;
