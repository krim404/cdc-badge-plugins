# Print Counter

A demo that chains four SDK basics into one flow: a persisted counter in NVS, a
setting edited with the T9 keyboard, an HTTP GET, and handing the result to
another plugin through the `thermo_print` external feature.

## Use

The view has two rows:

- **Print #&lt;n&gt;** - brings WiFi up if it is down, fetches the configured URL,
  sends the response text to the printer and advances the counter once the
  print is reported done.
- **Print URL** - T9 input for the URL, stored in NVS.

`;N` in the URL is replaced by the current counter value:

```text
https://www.krim.dev/print.php?id=abcd&num=;N
```

A URL without `;N` is fetched unchanged. The response body is printed as plain
text; word wrapping and pagination happen in the printer plugin.

## Prerequisites

`plugins/thermo_printer` must be installed - it provides `thermo_print`. Without
it the badge shows "no plugin provides this feature" and the counter stays put.

## Why `background: true`

`host_ext_feature_use` switches the provider to the foreground, which tears down
a caller that is not resident. The counter advances on the provider's status
action, so the plugin claims residency with `lifecycle::set_resident(true)`
right before handing over the job and releases it (together with its WiFi
reservation) when the status arrives.

The response body is cut at the 32 KiB job limit. The URL is stored in plain
NVS, so avoid putting secrets into it. `thermo_printer` 0.2.0 or newer is
required for text jobs.

## Building

```sh
cargo build -p print_counter --release --target wasm32-unknown-unknown
```

`tools/build_all.sh` builds it into `dist/` alongside the other plugins.
