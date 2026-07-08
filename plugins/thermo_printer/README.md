# Thermo Printer

Prints vCards, QR codes and files on BLE thermal "cat printers", and provides
the `thermo_print` external feature so any other installed plugin can print
through it. Print width is fixed at 384 px (48 bytes per row).

## Supported printers

Matched by the advertised name prefix during the BLE scan:

| Model | Model | Model |
|-------|-------|-------|
| GB01  | GT01  | MX06  |
| GB02  | YT01  | MX08  |
| GB03  | MX05  | MX10  |
|       |       | MXTP  |

All share the same GATT service (`0000ae00-…`) and the packet protocol below,
so any printer whose name begins with one of these prefixes works. To add a
model, extend `MODELS` in `src/proto.rs`.

## Menu

A multi-level menu: a top menu opens the **Print**, **Printer** and **Settings**
submenus (back = the plugin list key).

- **Print**
  - **Own vCard** / **Received vCard** — as Text, QR or Text + QR (the QR carries
    the full vCard).
  - **QR code** — free T9 text or URL (max 128 chars). `http(s)://` inputs stay
    unchanged, a bare `example.org/path` becomes `https://example.org/path`,
    anything else is plain text. The composed image can also be saved to vFAT as
    JPG or sent to another badge (`image/jpeg`, ≤ 4 KiB).
  - **File** — PNG/JPG (host-decoded, scaled to 384 px, dithered) and .txt/.md
    (word-wrapped with the on-device fonts, printed page by page) from the
    plugin's vFAT folder.
- **Printer**
  - **Scan printers** — see [Discovery](#discovery) below.
  - **Print server** — toggle the network print server (opt-in background; shows
    the badge URL and port when enabled). See [Print server](#print-server).
- **Settings** — thermal energy, BLE chunk size, paper feed, server port, body
  font (FreeMonoBold 9pt or compact 6x8), forget printer.

## Print server

Enable **Printer → Print server** to accept print jobs over the network (TCP,
default port 9100, configurable in Settings) and over USB serial. WiFi is
optional — the listener binds regardless; connect WiFi for network clients.
Enabling it opts the plugin into background residency (`host_set_resident`), so
it keeps serving after you leave the view; disabling it (or "Stop background" in
the plugin list `[3]` menu) frees it. With the server enabled the plugin also
autoloads after a reboot (`autoload` capability) and brings the listener back
up. The badge does all rendering.

Print jobs use the **PJ wire frame** (both network and the raw serial path):

```text
"PJ" | u8 type | u8 flags | u32 LE len | payload
type 0 = text/plain UTF-8   1 = image PNG/JPEG (scaled+dithered)   2 = raster (TP body)
```

The server answers `OK\n` once the job is parsed and queued — it does not mean
the print finished — and `ERR\n` on a malformed/oversized job or while another
print is running. Jobs are capped at 128 KiB.

Serial (line-based `PLUGIN CMD`): `PLUGIN CMD thermo_printer text <your text>` or,
after staging a file via `VFAT RECEIVE`, `PLUGIN CMD thermo_printer file <name>`.

The companion client is [`tools/print_cmd.py`](../../tools/print_cmd.py):

```sh
echo "hello" | tools/print_cmd.py --host <badge-ip>
tools/print_cmd.py --host <badge-ip> --image photo.jpg      # always scaled to 384 px
tools/print_cmd.py --serial --text "Ticket 42" --pin 0000
```

## Discovery

No BLE bonding or pairing — the cat printers expose an open GATT service. A
printer is found in one of two ways:

- **Scan printers** runs a repeating background scan while the screen is open:
  the list starts empty ("scanning…") and grows as printers are discovered,
  re-scanning every few seconds. Selecting one stores its address in NVS
  (`plg_printer`); the scan stops on selection, on leaving the screen, or when
  another menu action is chosen.
- **Printing without a saved printer** runs a one-shot scan and automatically
  connects to the strongest known printer, saving it.

Later prints connect straight to the saved address (no re-scan). After
connecting, the plugin discovers the service, resolves the TX (`0000ae01`,
write) and RX (`0000ae02`, notify) characteristics, subscribes to RX, then
streams the job. **Forget printer** in Settings clears the saved address.

## The `thermo_print` feature (for other plugins)

```rust
use cdc_badge_plugin::feature;

// raster: packed 1-bpp rows, MSB-first, a set bit prints black.
let payload = build_job_payload(384, height, stride, &rows);
feature::use_ext_feature("thermo_print", &payload, ACT_PRINT_STATUS)?;
```

Job payload format v1 (max 32 KiB total):

| Offset | Type   | Value                          |
|--------|--------|--------------------------------|
| 0      | u16 LE | magic `0x5450` ("TP")          |
| 2      | u8     | version = 1                    |
| 3      | u8     | flags = 0                      |
| 4      | u16 LE | width_px (≤ 384)               |
| 6      | u16 LE | height_px                      |
| 8      | u16 LE | stride_bytes (≥ (w+7)/8)       |
| 10     | u16 LE | reserved = 0                   |
| 12     | bytes  | rows[height][stride], MSB-first, set bit = black |

The status action fires with `HOST_EXT_FEATURE_STATUS_DONE` (0) on success or
an error code ≥ 1. Callers that want that status must declare
`background: true` (the print switches this plugin to the foreground). Compose
rasters with the SDK `surface` module and `qr::render_bitmap` /
`image::render`, then `surface.export()`.

## Protocol

Clean-room implementation of the cat-printer BLE protocol
(https://github.com/NaitLee/Cat-Printer served as behaviour/UUID reference):
service `0000ae00-…`, TX write `0000ae01`, RX notify `0000ae02`, frames
`51 78 <cmd> 00 <len_lo> <len_hi> <payload> <crc8> ff` (CRC8 poly `0x07`,
init `0x00`), rows as `0xA2` plain or `0xBF` RLE (whichever is smaller), paced
in MTU-sized chunks with a with-response write every 8 chunks for flow control.

## Capabilities

`ble`, `vcard`, `vfat`, `wifi`, `net_listen`, `background`, `autoload`,
`provides: ["thermo_print"]`, `message_types: ["image/jpeg"]`,
`nvs_namespace: "plg_printer"`. Requires host API level 0.8.

## Building

```sh
cargo build -p thermo_printer --release --target wasm32-unknown-unknown
cargo test -p thermo_printer          # CRC8, frame encoding, RLE, URL, layout, job format
```

`tools/build_all.sh` builds it into `dist/` alongside the other plugins.
