# news_feed (example)

Headline reader for any Atom RSS feed. Default feed is
[Golem.de](https://rss.golem.de/rss.php?feed=ATOM1.0); the URL is stored
in NVS and editable on the badge.

## What it does

- On enter: GET the configured feed, extract the first 10 entry titles,
  show them as a scrollable list.
- Y on a headline: show the full title in an info modal (handy when the
  list view truncates long titles).
- Menu key on the list: open a T9 editor **pre-filled with the current
  URL**. Confirming saves the new URL to NVS and re-fetches; an empty
  input leaves the stored value untouched.

## Capabilities used

```json
"capabilities": {
  "wifi":           true,
  "http":           true,
  "ui_exclusive":   true,
  "nvs_namespace":  "plugin_news_feed"
}
```

`prerequisites.wifi_connected` makes the host bring WiFi up before
`plugin_on_enter`.

## Build

```bash
cargo build --release --target wasm32-unknown-unknown -p news_feed
wasm-opt -Oz target/wasm32-unknown-unknown/release/news_feed.wasm -o news_feed.wasm
```

## Install

```bash
python tools/upload_plugin.py \
  --wasm news_feed.wasm \
  --meta examples/news_feed/meta.json \
  --lang examples/news_feed/news_feed.lang.json
```

Open **Tools → Plugins → News Feed** on the badge.
