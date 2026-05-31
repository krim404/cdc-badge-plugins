# grove_blink

GPIO demo plugin: toggles the LED on **Grove pin 0** (GPIO 2) when the user presses **Y**.

Showcases:
- The `grove` capability shortcut (which unlocks GPIO 2 and 3 without listing them in `gpio_pins`).
- `gpio::set_direction` / `gpio::write` from the Rust SDK.
- EventBus subscription to `KEY_PRESSED` via an action id.
- Localised UI strings (`press_y_to_toggle`, `led_on`, `led_off`).

Wire any LED + resistor between Grove SIG0 (the white wire on the Grove connector) and GND.

Build and install: see [Getting Started](../../docs/getting_started.md).

Open **Plugins -> Grove Blink** on the badge.
