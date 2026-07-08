# Capabilities

Capabilities answer the question **"what is this plugin allowed to do?"**. They are checked once at load time (slot conflicts, reserved resources) and then enforced at runtime on every host API call.

## Static checks at load

- `host_api_level_min` matches the firmware's level
- `linear_memory_kb` is within `[16, 4096]`
- `rmem` names are 1-15 chars (the on-chip name field holds 16 bytes incl. NUL); declaring `rmem` requires also declaring `nvs_namespace`
- `ecc` names are 1-15 chars; the host maps each to a slot in a reserved plugin ECC pool (plugins never name a physical slot)
- each `ble_service_uuids` entry is a 128-bit lowercase UUID (`8-4-4-4-12` hex with dashes)
- `nvs_namespace` starts with `plg_` or `plugin_`, is `[a-z0-9_]` only, and is at most 15 chars. The prefix is mandatory: it isolates plugin NVS from system namespaces such as `nvs.net80211` (WiFi credentials), `wifi` or `display` so a plugin cannot read or overwrite them

## Runtime enforcement

When a plugin calls a host function whose access is gated by a capability, the host checks the declared list before performing the action. A denial returns `HOST_ERR_NO_CAPABILITY` and writes an entry to the badge log. The plugin keeps running.

Examples:

| Call | Required capability |
|------|---------------------|
| `host_rmem_{read,write,erase}_named(name, ...)` | `rmem` must contain `name` |
| `host_ecc_*(name, ...)` (`generate`/`pubkey`/`delete`/`ecdsa_sign`/`eddsa_sign`/...) | `ecc` must contain `name` |
| `host_wifi_request()` | `wifi: true` |
| `host_http_open(...)` | `http: true` |
| `host_socket_*` | `socket: true` |
| `host_display_*` low-level GFX | `display_lowlevel: true` |
| `host_ble_*` (GATT server + central) | `ble: true` |
| `host_vcard_*` (own + received vCard store) | `vcard: true` |
| `host_ext_feature_register_handler(name, ...)` | `provides` must contain `name` |
| `host_net_listen/accept/close` (inbound TCP server) | `net_listen: true` |
| `host_msg_*` (badge-to-badge message transfer) | `ble: true` **and** a non-empty `message_types` |
| `host_usb_cdc_*` | `usb_cdc: true` |
| vFAT file storage (`host_fs_*`) | `vfat: true` |
| `host_gpio_*` | pin in `gpio_pins`, or `grove: true` for GPIO 2/3, or `sao: true` for GPIO 15/16 |
| `host_gpio_pwm_*` | pin in `pwm_pins` |
| `host_adc_read(pin, ...)` | pin in `adc_pins` |
| `host_i2c_*` | bus in `i2c_bus` (bus 0 is reserved) |
| `host_sao_eeprom_*` | `sao: true` (uses I2C1 0x50 transparently) |
| `host_nvs_*` | always permitted, but isolated to the declared `nvs_namespace` |

## Behavioral capabilities

These capabilities change host behavior while the plugin is loaded instead of
gating a specific call:

| Capability | Effect |
|------------|--------|
| `background` | **Permission** to stay loaded and keep receiving `plugin_on_tick` after the user leaves it. The plugin must call `lifecycle::set_resident(true)` to actually stay resident; otherwise it is torn down on leave. |
| `autoload` | **Permission** to be loaded at boot (headless, runs `plugin_init`). The plugin must call `lifecycle::set_resident(true)` in `plugin_init` to remain resident afterward; otherwise it is unloaded right after init. Orthogonal to `background`. See [Plugin Lifecycle](plugin_lifecycle.md). |
| `net_listen` | Permission to run an inbound TCP listener with `net::listen(port, action)`; accepted connections use the normal `socket` read/write/close. |
| `prevent_sleep` | While the plugin is loaded (foreground or background), the badge skips the lock-screen light sleep. The host registers a sleep inhibitor for the plugin on load and releases it on unload, so the caffeinated icon shows on the lock screen. Deep sleep triggered by the user is unaffected. |

**Breaking change (host API 0.8):** `background` and `autoload` are now permission
only. A plugin that relied on the capability alone to stay resident must add
`lifecycle::set_resident(true)`. A resident background plugin can be stopped from
the plugin list's `[3]` context menu ("Stop background").

## Provided external features

`provides: ["thermo_print", ...]` declares that this plugin implements the
named features for other plugins. Names are `[a-z][a-z0-9_]*`, at most 32
chars, at most 4 entries. Provider duties:

- Register a handler for every entry in `plugin_init` via
  `feature::register_provider(name, action_id)` - the firmware starts the
  plugin into the foreground when a job arrives, so `plugin_init` always ran.
- Pull the job in the handler action with `feature::consume_job` and call
  `feature::report_result` exactly once per job.

Caller side needs **no** capability: `feature::use_ext_feature(name, payload,
status_action)` hands at most 32 KiB to the provider, which then runs in the
foreground ("Open with" semantics). A caller without `background: true` is
unloaded by that foreground switch and its status action is silently dropped;
declare `background: true` when the result matters. If no installed plugin
provides the feature, the firmware shows a "no plugin for this feature" modal
and the call returns `NotFound`.

The `thermo_print` feature (provided by the `thermo_printer` plugin) accepts the
raw raster job format documented in
[plugins/thermo_printer](../plugins/thermo_printer/README.md).

## Hardware shortcuts

The shortcuts `grove` and `sao` unlock all pins of the corresponding port without listing them individually:

| Shortcut | Unlocks |
|----------|---------|
| `grove: true` | GPIO 2 (SIG0), GPIO 3 (SIG1) |
| `sao: true`   | GPIO 15, GPIO 16, I2C1 access at address `0x50` (SAO EEPROM) |

Loading two plugins that both want the same physical pin is rejected at load time - the second plugin sees a "GPIO N already held by `<other_id>`" error and is not started.

## Forbidden APIs

A small set of APIs are simply not exported to plugins, regardless of capability declaration: anything that touches the lock-screen / PIN system, BLE bond management, charger control, deep sleep, and direct HID report emission. The full list lives in the [host API reference](host_api_reference.md).
