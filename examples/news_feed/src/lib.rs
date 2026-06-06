//! \file
//! \brief News feed example plugin.
//!
//! Fetches an Atom RSS feed via HTTP, extracts the latest 10 entry titles,
//! and shows them as a scrollable list. The feed URL is stored in NVS
//! (`plugin_news_feed:url`) and editable on the badge via T9: pressing
//! the menu key opens the editor pre-filled with the current URL.
//!
//! This is the most advanced example. New ideas, in order:
//!   - HTTP requests via the `http` SDK module,
//!   - persisting user-editable settings in NVS (the badge key/value
//!     store) under the `nvs` SDK module,
//!   - safer mutable plugin state via `RefCell` instead of `static mut`,
//!   - a small hand-written XML/Atom parser,
//!   - opening the T9 text input dialog and consuming its result,
//!   - unit tests that run on the host with `cargo test`.

// Plugins run in the WAMR sandbox without the standard library.
#![no_std]

// Heap types (`String`, `Vec`, `format!`) live in `alloc`.
extern crate alloc;

// Pull in the heap types we need by name. Importing them this way keeps
// the rest of the file free of `alloc::` prefixes.
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

// `RefCell<T>` gives us interior mutability: a `&` shared reference can
// still mutate the wrapped value at runtime, as long as no two borrows
// overlap. We use it to hide the rough edges of `static mut`.
use core::cell::RefCell;

// SDK modules we touch:
//   - `http` issues HTTP requests through the host,
//   - `i18n` resolves translation keys to strings,
//   - `log` writes to the serial log,
//   - `nvs` reads/writes persistent key/value pairs,
//   - `plugin_main` macro for the FFI plumbing,
//   - `ui` builds list views, modals and the T9 input dialog.
use cdc_badge_plugin::{http, i18n, log, nvs, plugin_main, ui};

// FFI plumbing. Always exactly once.
plugin_main!();

// Log tag used by every `log::info`/`log::error` call below.
const TAG: &str = "news";

// Used the very first time the plugin starts (or when the user clears
// the saved value): a public Atom feed that is unlikely to disappear.
const DEFAULT_URL: &str = "https://rss.golem.de/rss.php?feed=ATOM1.0";

// The NVS key under which we persist the chosen URL. NVS keys are
// namespaced per plugin automatically, so two plugins can share a short
// name like "url" without colliding.
const URL_KEY: &str = "url";

// How many headlines we keep and show. Picking a small cap keeps memory
// usage predictable and avoids overrunning the badge screen.
const MAX_HEADLINES: usize = 10;

// Maximum characters accepted by the T9 input dialog. Must be tight
// enough to fit on screen but generous enough for real-world URLs.
const URL_MAX_LEN: u16 = 200;

// Network timeout in milliseconds. The badge has a small TCP stack, so
// leave generous headroom: better one slow fetch than a misleading
// "fetch failed" toast on a flaky link.
const FETCH_TIMEOUT_MS: u32 = 30_000;

// Action IDs we attach to UI elements (lists, T9 dialog) so that the
// single `plugin_on_action` entry point can tell them apart later.
// Named constants beat magic numbers in any non-trivial plugin.
const ACTION_VIEW_TITLE: u32 = 1;     // user opened one of the headlines
const ACTION_EDIT_URL: u32 = 2;       // user pressed the menu key
const ACTION_EDIT_URL_DONE: u32 = 3;  // T9 dialog confirmed/cancelled
const ACTION_KEY_EVENT: u32 = 4;      // key press dispatched via event bus

// ASCII for the digit keys; user_data of plugin_on_action carries the key
// code when fired via the KEY_PRESSED event subscription (idx is the event
// ordinal, 0 for KEY_PRESSED).
const KEY_RELOAD: u32 = b'1' as u32;

// ---------------------------------------------------------------------
// Plugin-wide mutable state.
//
// We need two mutable values that outlive a single callback: the list
// of headlines we are showing and the URL we last fetched. The simple
// `static mut` trick used in `grove_blink` does not scale to non-`Copy`
// types like `Vec<String>` without extra boilerplate, so we wrap them
// in a small helper that lets us write `static FOO: PluginCell<...>`
// and then borrow them ergonomically.
//
// SAFETY: this is sound because WAMR runs every plugin on exactly one
// thread, so the `Sync` requirement on `static` items can be satisfied
// trivially. Do not copy this pattern into multi-threaded code without
// adding real synchronisation.
// ---------------------------------------------------------------------
struct PluginCell<T>(RefCell<T>);
// SAFETY: WAMR runs every plugin on a single thread.
unsafe impl<T> Sync for PluginCell<T> {}
impl<T> PluginCell<T> {
    // `const fn` lets us initialise the cell in a `static`, which only
    // accepts compile-time-constant expressions.
    const fn new(v: T) -> Self {
        Self(RefCell::new(v))
    }
}
// Deref-through to `RefCell<T>` so `HEADLINES.borrow_mut()` works
// without an explicit `.0`.
impl<T> core::ops::Deref for PluginCell<T> {
    type Target = RefCell<T>;
    fn deref(&self) -> &RefCell<T> {
        &self.0
    }
}

// The two pieces of state. `HEADLINES` is the cached list shown on
// screen; `CURRENT_URL` is what we last fetched, used to prefill the
// T9 editor when the user wants to change it.
struct Entry {
    title: String,
    summary: String,
}

static HEADLINES: PluginCell<Vec<Entry>> = PluginCell::new(Vec::new());
static CURRENT_URL: PluginCell<String> = PluginCell::new(String::new());

/// \brief Read the user-configured feed URL from NVS, or fall back to the
///        hardcoded Golem default.
/// \return The URL to fetch.
//
// Reading NVS returns `Option<String>`: `None` if the key is missing.
// `.filter(|s| !s.is_empty())` collapses an empty saved string into
// `None` so it also falls through to the default - otherwise an
// accidental empty entry would wedge the plugin.
fn current_url() -> String {
    nvs::get_str(URL_KEY, 256)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string())
}

/// \brief Strip simple inline tags (`<p>`, `<br>`, `<a href=..>...`) from a
///        snippet so the body fits the badge info view.
fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut last_space = true;
    for ch in input.chars() {
        if in_tag {
            if ch == '>' { in_tag = false; }
            continue;
        }
        if ch == '<' { in_tag = true; continue; }
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// \brief Pull the first `<tag>...</tag>` payload (CDATA-aware, attribute-tolerant)
///        from \p src. Returns `None` when not found.
fn extract_tag<'a>(src: &'a str, tag: &str) -> Option<&'a str> {
    let open = src.find(&format!("<{}", tag))?;
    let after_tag = src[open..].find('>')? + open + 1;
    let close = src[after_tag..].find(&format!("</{}>", tag))? + after_tag;
    Some(src[after_tag..close].trim())
}

/// \brief Strip a CDATA wrapper if present.
fn strip_cdata(s: &str) -> &str {
    if s.starts_with("<![CDATA[") && s.ends_with("]]>") {
        &s["<![CDATA[".len()..s.len() - 3]
    } else {
        s
    }
}

/// \brief Extract title + summary from one Atom `<entry>` or RSS `<item>` block.
/// \return `None` if the title is missing or empty.
fn extract_first_entry(entry: &str) -> Option<Entry> {
    let title_raw = strip_cdata(extract_tag(entry, "title")?).trim();
    if title_raw.is_empty() { return None; }
    let summary_raw = extract_tag(entry, "summary")
        .or_else(|| extract_tag(entry, "description"))
        .or_else(|| extract_tag(entry, "content"))
        .map(strip_cdata)
        .map(|s| s.trim())
        .unwrap_or("");
    let title_dec = title_raw.to_string();
    let summary_dec = strip_html(summary_raw);
    Some(Entry { title: title_dec, summary: summary_dec })
}

/// \brief Walk an Atom feed body and collect up to `max` entry titles.
///
/// The feed-level `<title>` outside of any `<entry>` is intentionally
/// skipped.
/// \param body Atom XML response body.
/// \param max  Maximum number of titles to return.
/// \return The collected titles in document order.
//
// Algorithm: repeatedly slice the body to "the next `<entry>...</entry>`
// block", extract its title, advance the cursor past the closing tag,
// and stop when we hit `max` or run out of entries.
fn parse_feed_entries(body: &str, max: usize) -> Vec<Entry> {
    let mut entries = Vec::new();
    let (open_tag, close_tag) = if body.contains("<entry") {
        ("<entry", "</entry>")
    } else {
        ("<item", "</item>")
    };
    let mut cursor = 0;
    while entries.len() < max {
        let entry_start = match body[cursor..].find(open_tag) {
            Some(i) => cursor + i,
            None => break,
        };
        let entry_end = match body[entry_start..].find(close_tag) {
            Some(i) => entry_start + i,
            None => break,
        };
        if let Some(e) = extract_first_entry(&body[entry_start..entry_end]) {
            entries.push(e);
        }
        cursor = entry_end + close_tag.len();
    }
    entries
}

/// \brief Issue a GET request and return the full response body.
/// \param url Target URL.
/// \return The response body on `2xx`, or a short error tag describing
///         which step failed (`"open"`, `"perform"`, `"http"`, `"read"`).
//
// Returning a `&'static str` error keeps the function tiny (no heap
// allocation in the error path) and is enough for our purposes: we only
// want to know *what* broke so we can log it. For richer errors, return
// an enum here.
fn fetch_body(url: &str) -> Result<String, &'static str> {
    log::info(TAG, &format!("GET {}", url));

    // `Request::open` allocates a request handle in the host. The
    // timeout applies to the whole request lifetime.
    let req = http::Request::open(http::GET, url, FETCH_TIMEOUT_MS).map_err(|_| "open")?;

    // Best-effort header set. Atom feeds usually advertise themselves
    // as `application/atom+xml`; the `*/*` fallback keeps servers that
    // ignore the `Accept` header happy. We ignore the result because a
    // failed header set is not worth aborting on.
    let _ = req.header("Accept", "application/atom+xml, application/xml, */*");

    // Actually send the request and receive the headers. The status
    // code is returned synchronously; the body is fetched separately
    // by `read_to_string` below.
    let status = req.perform().map_err(|_| "perform")?;
    log::info(TAG, &format!("HTTP {}", status));

    // Reject anything outside the success range. Following redirects
    // is the host's job.
    if status < 200 || status >= 300 {
        return Err("http");
    }

    // Read the body. This buffers the whole response in RAM - safe
    // because the firmware imposes a hard cap on response size, but
    // keep this in mind if you ever need to handle multi-MB feeds.
    req.read_to_string().map_err(|_| "read")
}

/// \brief Fetch the feed, parse it and render the headline list.
///
/// Always pushes a list view so the menu key (URL editor) stays reachable
/// even on error.
//
// This is the heart of the plugin. The structure to remember:
//   1. read URL (NVS or default),
//   2. cache it for the editor,
//   3. fetch + parse,
//   4. build a list with the menu key wired up,
//   5. fill the list with titles, "no entries" or "fetch failed",
//   6. push the list and set a footer hint.
fn fetch_and_render() {
    let url = current_url();

    // Save the URL we are actually using right now into the editor
    // cache. `borrow_mut()` panics if there is an outstanding borrow;
    // because we never hold a long borrow on either cell, this is safe
    // in practice. Keep `RefCell` borrows short.
    *CURRENT_URL.borrow_mut() = url.clone();

    // Drop the previous batch of headlines before we fetch new ones so
    // a fetch error leaves the cache empty (better than stale data).
    HEADLINES.borrow_mut().clear();

    // Chain the fetch with the parse. `.map(...)` only runs the closure
    // if the previous step succeeded, so we end up with
    // `Result<Vec<String>, &str>` here.
    let result = fetch_body(&url).map(|body| parse_feed_entries(&body, MAX_HEADLINES));

    // Start a list view. `.on_select(...)` says "when the user picks an
    // item, fire this action ID". `.on_menu(...)` says "when the user
    // presses the menu key on this list, fire this *other* action ID";
    // that is how we hook up the URL editor.
    let mut builder = ui::ListBuilder::new(i18n::tr_meta("name"))
        .on_select(ACTION_VIEW_TITLE)
        .on_menu(ACTION_EDIT_URL);

    // Three cases: titles present, empty feed, fetch failure. The `match`
    // makes it impossible to accidentally render the wrong UI for one of
    // them.
    match result {
        Ok(entries) if !entries.is_empty() => {
            for (i, e) in entries.iter().enumerate() {
                builder = builder.item(&e.title, i as u32, ui::UI_ICON_NONE);
            }
            *HEADLINES.borrow_mut() = entries;
        }
        Ok(_) => {
            // Feed parsed but contained no entries. Show a single info
            // row so the user understands the state.
            builder = builder.item(i18n::tr_key("no_entries"), 0, ui::UI_ICON_INFO);
            log::info(TAG, "feed parsed, but no entries");
        }
        Err(reason) => {
            // Fetch failed at some stage; show an error row in the list
            // *and* pop a short toast for immediate feedback. We keep
            // the list so the menu key (URL editor) remains reachable.
            builder = builder.item(i18n::tr_key("fetch_failed"), 0, ui::UI_ICON_ERROR);
            log::error(TAG, reason);
            ui::push_toast(i18n::tr_key("fetch_failed"), ui::UI_ICON_ERROR, 1500);
        }
    }

    // Finalise the list view. `push` adds it to the UI stack as a new
    // screen (unlike `replace` in `grove_blink` which swaps in place).
    builder.push();

    // The footer is the small hint line at the bottom. It is a great
    // place to remind the user about the menu key shortcut.
    ui::set_footer(i18n::tr_key("hint_main"));
}

/// \brief Lifecycle hook fired once when the plugin is loaded.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    log::info(TAG, "init");
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    log::info(TAG, "deinit");
    0
}

/// \brief Lifecycle hook fired every time the user opens the plugin.
///
/// Triggers the initial fetch and renders the headline list.
/// \return `0` on success.
//
// The fetch happens on the WASM thread and blocks `on_enter` until it
// finishes (or times out). For long fetches you would normally show a
// "loading" screen first and kick the fetch off in the background, but
// this example keeps things linear to stay readable.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    let _ = cdc_badge_plugin::event::subscribe(
        cdc_badge_plugin::event::KEY_PRESSED,
        ACTION_KEY_EVENT,
    );
    fetch_and_render();
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
///
/// Clears cached headlines and the current URL so a re-enter starts
/// fresh.
/// \return `0` on success.
//
// Clearing on exit keeps RAM usage tight and guarantees that the user
// always sees a fresh fetch when they re-enter the plugin.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    HEADLINES.borrow_mut().clear();
    CURRENT_URL.borrow_mut().clear();
    0
}

/// \brief Action dispatch for list selects, menu key and T9 confirm.
/// \param action_id Identifier set when pushing the originating view.
/// \param idx       For list selects: item index. For a KEY_PRESSED event:
///                  the event ordinal. For a T9 confirm: the entered text
///                  length (0 on cancel).
/// \param user_data For a KEY_PRESSED event: the ASCII key code. For a T9
///                  result: 1 on confirm, 0 on cancel.
/// \return `0` on success.
//
// `plugin_on_action` is the single entry point for *all* user input. We
// rely on `action_id` to figure out which view sent the action. Compare
// with `grove_blink` (one ID) to see how this scales.
#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACTION_VIEW_TITLE => {
            let entries = HEADLINES.borrow();
            if let Some(e) = entries.get(idx as usize) {
                let body: String = if e.summary.is_empty() {
                    e.title.clone()
                } else {
                    format!("{}\n\n{}", e.title, e.summary)
                };
                let title = i18n::tr_meta("name");
                drop(entries);
                ui::push_info(title, &body);
            }
        }
        ACTION_EDIT_URL => {
            // User pressed the menu key. Open the T9 text editor with
            // the current URL prefilled, capped at `URL_MAX_LEN` chars.
            // The last argument is the action ID we want fired when
            // the dialog closes.
            let initial = CURRENT_URL.borrow().clone();
            ui::push_t9_input(
                i18n::tr_key("feed_url"),
                Some(&initial),
                URL_MAX_LEN,
                ACTION_EDIT_URL_DONE,
            );
        }
        ACTION_KEY_EVENT => {
            // KEY_PRESSED dispatched from EventBus: `idx` is the event
            // type (always 0 here), `user_data` is the ASCII key code.
            let _ = idx;
            if user_data == KEY_RELOAD {
                fetch_and_render();
            }
        }
        ACTION_EDIT_URL_DONE => {
            // The T9 closed. `user_data == 1` marks a confirm, `0` a cancel;
            // only persist on confirm. Either way we re-fetch below.
            if user_data == 1 {
                // `consume_input_text` pulls the user-entered string
                // out of the host. It returns `None` if there is no
                // pending input (defensive - the host normally has it
                // ready when the action fires).
                if let Some(value) = ui::consume_input_text(URL_MAX_LEN as usize) {
                    let trimmed = value.trim();
                    if !trimmed.is_empty() {
                        // Persist the new URL so it survives a reboot.
                        // We do not error-check: a failed NVS write is
                        // surfaced via the log inside the SDK.
                        let _ = nvs::set_str(URL_KEY, trimmed);
                    }
                }
            }
            // The T9 view already popped itself before this fired, so do
            // not pop again here. Re-fetch with the (possibly new) URL.
            fetch_and_render();
        }
        // Unknown action IDs are ignored. This is friendlier than
        // returning an error: the firmware can add new generic actions
        // without breaking older plugins.
        _ => {}
    }
    0
}

// ---------------------------------------------------------------------
// Unit tests.
//
// These compile to a native binary via `cargo test`, *not* to WASM, so
// they cover the pure-logic functions only (`parse_atom_titles` and the
// helpers it calls). UI / HTTP / NVS code cannot be tested this way
// because it needs the host. Keeping pure functions easily testable is
// a deliberate design choice you can copy in your own plugins.
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tag_basic() {
        assert_eq!(extract_tag("<entry><title>hi</title></entry>", "title"), Some("hi"));
        assert_eq!(extract_tag("<title type=\"text\">attr</title>", "title"), Some("attr"));
        assert_eq!(extract_tag("<entry></entry>", "title"), None);
    }

    #[test]
    fn strip_cdata_unwraps() {
        assert_eq!(strip_cdata("<![CDATA[hello]]>"), "hello");
        assert_eq!(strip_cdata("plain"), "plain");
    }

    #[test]
    fn strip_html_removes_tags_and_collapses_whitespace() {
        let out = strip_html("<p>hello <b>world</b>\n   foo</p>");
        assert_eq!(out, "hello world foo");
    }
}
