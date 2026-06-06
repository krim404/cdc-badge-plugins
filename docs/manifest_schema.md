# Manifest Schema (`meta.json`)

Every plugin ships a JSON manifest next to its `.wasm`. The host parses it at load time, registers the i18n strings, validates the capabilities, then satisfies the prerequisites before running the plugin.

## Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Used as filename stem. By convention lowercase `[a-z][a-z0-9_]*`; the parser only requires it to be non-empty. |
| `version` | string | yes | SemVer `MAJOR.MINOR.PATCH`. |
| `author` | string | no | Free text. |
| `icon` | string | no | Built-in icon identifier (`info`, `battery`, `ha`, ...). |
| `host_api_level_min` | string | yes | Minimum host API level required, `MAJOR.MINOR`. |
| `linear_memory_kb` | int | yes | WAMR linear memory size, 16-4096 KB. |
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

The `capabilities` object is a flat map. See [Capabilities](capabilities.md)
for what each one allows, how it is enforced, and the behavioral flags
(`background`, `autoload`, `prevent_sleep`).

```json
"capabilities": {
  "wifi": true,
  "ble": false,
  "http": true,
  "socket": false,
  "ui_exclusive": true,
  "display_lowlevel": false,
  "usb_cdc": false,
  "vfat": false,
  "pixel_strip": false,
  "background": true,
  "autoload": false,
  "prevent_sleep": true,
  "grove": false,
  "sao": false,
  "rmem": ["ha_token"],
  "ecc": ["signing_key"],
  "ble_service_uuids": [],
  "gpio_pins": [],
  "pwm_pins": [],
  "adc_pins": [],
  "i2c_bus": [],
  "nvs_namespace": "plugin_ha"
}
```

| Field | Type | Meaning |
|-------|------|---------|
| `wifi`, `ble`, `http`, `socket`, `display_lowlevel`, `usb_cdc`, `vfat`, `pixel_strip`, `ui_exclusive` | bool | Gate the matching host API family. |
| `background`, `autoload`, `prevent_sleep` | bool | Behavioral flags. |
| `grove`, `sao` | bool | Hardware port shortcuts. |
| `rmem` | string[] | Named secure-memory slots (1-15 chars). Requires `nvs_namespace`. |
| `ecc` | string[] | Named ECC key slots (1-15 chars). |
| `ble_service_uuids` | string[] | 128-bit lowercase UUIDs for GATT services. |
| `gpio_pins`, `pwm_pins`, `adc_pins`, `i2c_bus` | int[] | Hardware pins / buses the plugin may use. |
| `nvs_namespace` | string | Must start with `plg_` or `plugin_`; `[a-z0-9_]` only; max 15 chars. |

## prerequisites

```json
"prerequisites": {
  "wifi_connected": { "timeout_ms": 15000, "on_fail": "abort" },
  "time_synced":    { "on_fail": "warn" },
  "battery_min":    { "min_pct": 10, "on_fail": "abort" }
}
```

`on_fail` is one of `abort`, `warn`, `callback`. See [Plugin Lifecycle](plugin_lifecycle.md) for the order in which the host evaluates them.
