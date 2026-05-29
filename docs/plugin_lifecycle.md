# Plugin Lifecycle

```
load → init → prerequisites → on_enter → [running] → on_exit → release → deinit → unload
```

## Required exports

- `plugin_required_api_major()` - returns `HOST_API_LEVEL_MAJOR`
- `plugin_required_api_minor()` - returns `HOST_API_LEVEL_MINOR`
- `plugin_init()` - called once after the WASM instance is created
- `plugin_deinit()` - called once before the WASM instance is destroyed
- `plugin_on_enter()` - called when the user opens the plugin
- `plugin_on_exit()` - called when the user leaves the plugin

The Rust `plugin_main!()` macro generates the API-level exports for you.

## Optional exports

| Export | When the host calls it |
|--------|------------------------|
| `plugin_on_action(action_id, idx, user_data)` | A UI view fires its callback. |
| `plugin_on_button(button_code)` | A keypad button is pressed while no host view is foreground. |
| `plugin_on_event(event_type, value)` | A subscribed EventBus event arrives. |
| `plugin_on_tick(uptime_ms)` | Periodic tick (~ once per second). |
| `plugin_on_cmd(len)` | The host forwarded a command string (e.g. `PLUGIN CMD <id> <args>`); pull it with `cmd::consume` / `host_cmd_consume`. |
| `plugin_on_prerequisite_failed(prereq_id, error_code)` | A prerequisite with `on_fail=callback` failed. |

## Prerequisite-controlled startup

Before calling `plugin_on_enter()`, the host walks the `prerequisites` list from the manifest in order. For each entry:

1. Check the condition (e.g. WiFi is connected).
2. If satisfied, mark it as "acquired by the plugin" and continue.
3. If not, apply `on_fail`:
   - `abort` - cancel start, show toast, fall back to the plugin list.
   - `warn` - ask the user with a confirm dialog.
   - `callback` - let the plugin decide via `plugin_on_prerequisite_failed`.

On exit the host releases acquired resources in reverse order. WiFi that was brought up by the host is disconnected again automatically.
