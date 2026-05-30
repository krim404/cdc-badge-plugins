# Manifest Schema (`meta.json`)

Every plugin ships a JSON manifest next to its `.wasm`. The host parses it at load time, registers the i18n strings, validates the capabilities, then satisfies the prerequisites before running the plugin.

## Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Lowercase, `[a-z][a-z0-9_]{1,31}`. Used as filename stem and NVS namespace. |
| `version` | string | yes | SemVer `MAJOR.MINOR.PATCH`. |
| `author` | string | no | Free text. |
| `icon` | string | no | Built-in icon identifier (`info`, `battery`, `ha`, ...). |
| `host_api_level_min` | string | yes | Minimum host API level required, `MAJOR.MINOR`. |
| `linear_memory_kb` | int | yes | WAMR linear memory size, 16-1024 KB. |
| `i18n` | object | no | Localised strings (see below). |
| `capabilities` | object | no | What the plugin is allowed to do. |
| `prerequisites` | object | no | What the host must establish before running the plugin. |

## i18n

```json
"i18n": {
  "default_language": "en",
  "meta": {
    "name":        { "en": "My Plugin", "de": "Mein Plugin" },
    "description": { "en": "...", "de": "..." }
  },
  "strings": {
    "save":   { "en": "Save",   "de": "Speichern" },
    "cancel": { "en": "Cancel", "de": "Abbrechen" }
  }
}
```

Plugin code looks them up with `host_i18n_tr_key("save")` or `host_i18n_tr_meta("name")`.

## capabilities

```json
"capabilities": {
  "wifi": true,
  "ble": false,
  "rmem": ["ha_token"],
  "ecc": ["signing_key"],
  "ble_service_uuids": [],
  "nvs_namespace": "plugin_ha",
  "http": true,
  "display_lowlevel": false,
  "ui_exclusive": true,
  "background": true,
  "prevent_sleep": true
}
```

`background` keeps the plugin loaded and ticking (`plugin_on_tick`) after the
user leaves it. `prevent_sleep` stops the badge from entering the lock-screen
light sleep while the plugin is loaded; see [Capabilities](capabilities.md).

`rmem` is a list of slot names (1-15 chars each). The host allocates a
physical slot from the plugin pool (TROPIC01 R-Mem slots 501-511) on first
write and persists the association in the slot header. Two plugins
declaring the same name share the same physical slot (intentional, common
scope). Calls to capabilities that were not declared return
`HOST_ERR_NO_CAPABILITY`. System slots (PIN hashes, FIDO2 keys, GPG keys,
TOTP secrets, password vault) live outside the plugin pool and are not
addressable from a plugin under any circumstance.

`ecc` is a list of ECC key names (1-15 chars each), addressed by name exactly
like `rmem`. The host maps each declared name to a slot in a small reserved
plugin ECC pool and persists the mapping in NVS, so a key keeps its slot across
reboot and reinstall; plugins never reference a physical slot number. The pool
is intentionally tiny (firmware features such as attestation and WebAuthn own
the scarce TROPIC01 slots), so only a limited number of plugin ECC keys can be
live at once.

`nvs_namespace` **must start with `plg_` or `plugin_`** (lowercase, digits
and underscore only, 15-char NVS hard limit). The prefix is enforced both
by the manifest validator and at runtime by the host so a plugin cannot
declare a namespace like `nvs.net80211` and read the WiFi-credential
`sta.pswd` out of system NVS, nor stomp on `wifi` / `display` / other
firmware-owned namespaces. Pick `plg_<short>` when `plugin_<id>` overflows
the 15-char budget (e.g. `plg_grove_led` instead of `plugin_grove_led`).

## prerequisites

```json
"prerequisites": {
  "wifi_connected": { "timeout_ms": 15000, "on_fail": "abort" },
  "time_synced":    { "on_fail": "warn" },
  "battery_min":    { "min_pct": 10, "on_fail": "abort" }
}
```

`on_fail` is one of `abort`, `warn`, `callback`. See [Plugin Lifecycle](plugin_lifecycle.md) for the order in which the host evaluates them.
