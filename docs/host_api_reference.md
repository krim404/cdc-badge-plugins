# Host API Reference

The canonical source of truth for the host API is [`sdk/host_api.h`](../sdk/host_api.h). This file walks the same API in a more readable form.

## API Level

```c
#define HOST_API_LEVEL_MAJOR  0
#define HOST_API_LEVEL_MINOR  6
```

A plugin declares the minimum it needs in `meta.json`:

```json
"host_api_level_min": "0.5"
```

The host loads the plugin only if `plugin_major == host_major && plugin_minor <= host_minor`. Pre-1.0, treat any minor bump as potentially breaking - rebuild your plugin against the new SDK.

## Function families

- **Logging** - `host_log`, `host_log_hex`
- **Time** - `host_uptime_ms`, `host_unix_time`, `host_local_time`, `host_timezone_offset`
- **Power** - `host_battery_mv/pct`, `host_charge_status`, `host_is_*`
- **Crypto** - SHA256, HMAC-SHA256, AES-GCM, Base32/64/Hex, Random
- **SecureElement** - R-Memory, ECC keys, ECDSA / EdDSA signing
- **HTTP** - streamed open/perform/read/close
- **WiFi** - request/release, info, scan
- **BLE** - GATT server registration, GATT client read/write/notify
- **NVS** - typed key/value, per-plugin namespace
- **UI** - push pre-built views (toast, list, confirm, T9 input, ...)
- **Low-level GFX** - opt-in via capability
- **I18n** - manifest-based lookups, runtime registration
- **EventBus** - subscribe, publish module events
- **Keypad** - poll/consume, primary input comes through `plugin_on_button`
- **System info** - feature flags, firmware version
- **Command channel** - `host_cmd_consume` pulls a host-forwarded command string (paired with the optional `plugin_on_cmd` export)

## Return codes

```c
#define HOST_OK                  0
#define HOST_ERR_GENERIC        -1
#define HOST_ERR_INVALID_ARG    -2
#define HOST_ERR_NO_CAPABILITY  -3
#define HOST_ERR_NOT_FOUND      -4
#define HOST_ERR_TIMEOUT        -5
#define HOST_ERR_NO_MEMORY      -6
#define HOST_ERR_BUSY           -7
#define HOST_ERR_NOT_SUPPORTED  -8
```

## Capability vs prerequisite

Both come from `meta.json`. Capabilities are **what the plugin may do** (checked on every call). Prerequisites are **what the host establishes before the plugin runs** (WiFi, BLE, time sync, battery level). See [capabilities.md](capabilities.md) and [plugin_lifecycle.md](plugin_lifecycle.md).
