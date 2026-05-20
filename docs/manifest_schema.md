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
  "ecc_slots": [],
  "ble_service_uuids": [],
  "nvs_namespace": "plugin_ha",
  "http": true,
  "display_lowlevel": false,
  "ui_exclusive": true
}
```

`rmem` is a list of slot names (1-15 chars each). The host allocates a
physical slot from the plugin pool on first write and persists the
association in the slot header. Two plugins declaring the same name share
the same physical slot (intentional, common scope). Calls to capabilities
that were not declared return `HOST_ERR_NO_CAPABILITY`.

## prerequisites

```json
"prerequisites": {
  "wifi_connected": { "timeout_ms": 15000, "on_fail": "abort" },
  "time_synced":    { "on_fail": "warn" },
  "battery_min":    { "min_pct": 10, "on_fail": "abort" }
}
```

`on_fail` is one of `abort`, `warn`, `callback`. See [Plugin Lifecycle](plugin_lifecycle.md) for the order in which the host evaluates them.
