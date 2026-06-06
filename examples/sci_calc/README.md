# sci_calc

A scientific calculator example plugin for the CDC Badge. It demonstrates the
canvas long-press API (delivered for any view via the keypad's deferred
short-press mode), a categorized function menu, `libm` math in a `no_std`
plugin, and exporting data to a `.txt` file on the vFAT partition.

## Display

The screen is a calculation tape. The top line is the live input/result; on
`=` the completed calculation drops into the history list below and the result
becomes the new starting value. The status row shows the angle mode (DEG/RAD),
a memory flag (`M`), and the pending operator. The number font auto-shrinks to
fit. The full scrollable history is available from the menu and can be exported.

## Shortcuts

Short press = tap; long press = hold (~0.8 s). A long press never also sends the
tap.

| Key | Short press | Long press |
|-----|-------------|------------|
| 0   | digit 0     | decimal point `.` |
| 1   | digit 1     | `+` |
| 2   | digit 2     | `-` |
| 3   | digit 3     | `x` (multiply) |
| 4   | digit 4     | `/` (divide) |
| 5   | digit 5     | `^` (power) |
| 6   | digit 6     | `sqrt` |
| 7   | digit 7     | `x^2` |
| 8   | digit 8     | `1/x` |
| 9   | digit 9     | `%` |
| Y   | `=` (equals/enter) | open the function menu |
| N   | backspace   | clear all |

`N + Y` together is the system exit chord (handled by the OS).

## Menu

Long-press `Y` opens the categorized, scrollable menu:

- **Arithmetic**: `+ - x / x^y mod %`, negate, decimal, backspace, clear entry, clear all
- **Trigonometry**: `sin cos tan asin acos atan` (respect the DEG/RAD setting)
- **Log & Exp**: `ln log10 e^x 10^x x^2 sqrt cbrt 1/x x!`
- **Constants**: `pi e`
- **Memory**: `MS MR M+ M- MC`
- **Angle**: toggle DEG/RAD (persisted in NVS)
- **History**: open the scrollable tape in the on-screen text viewer
- **Export**: write the tape to `DDMM-HHMM.txt` on the vFAT partition
- **Quit**: leave the plugin

Calculations use immediate left-to-right execution (no operator precedence),
like a classic pocket calculator: `2 + 3 x 4 =` yields `20`.

## Exporting the tape

Files are written to the plugin's vFAT folder `/plugins/data/sci_calc/`.
Retrieve them over the serial console:

```
VFAT CD data/sci_calc
VFAT LIST
VFAT GET DDMM-HHMM.txt
```

or via Tools -> Expert -> vFAT in the on-device UI.

## Build

```
cargo build --release --target wasm32-unknown-unknown -p sci_calc
cargo test -p sci_calc        # host-side engine unit tests
```
