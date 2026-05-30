# BLE Scanner - Result Export

The scanner saves every run to a text file on the badge so the results can be
retrieved later. This requires the `vfat` capability (declared in `meta.json`).

## When it is written

The file is written **when you leave the scanner** (press `N` to go back). The
plugin records the start time when you open it and the end time when you leave,
so the file always reflects one complete scan session.

Nothing is written while scanning - only once, on close - so the flash is not
hammered. If the real-time clock is not set, no file is written (the timestamp
would be meaningless).

## Where it is stored

Files live in the plugin's private vFAT folder:

```
/plugins/data/ble_scanner/<DDMM-HHMM>.txt
```

The name is the local end time, e.g. `0305-2213.txt` for 03 May, 22:13.

## File format

Plain text, one value per line. A short header gives the scan window
(start / end), then one line per discovered device:

```
Start: 03.05 22:01
End:   03.05 22:13
Devices: 4

AA:BB:CC:DD:EE:FF  -67dBm x42 Eve Energy
11:22:33:44:55:66  -80dBm x7
...
```

Each device line is `MAC  RSSI  xSightings  Name` (the name is empty when the
device never advertised one).

## How to read it back

Use the **vFAT module** on the badge (`Tools -> Expert -> vFAT`), or the
serial shell:

```
VFAT CD data/ble_scanner
VFAT LIST
VFAT GET 0305-2213.txt
```

In the GUI explorer, open the `data` folder, then `ble_scanner`, and select the
file to view it on screen.
