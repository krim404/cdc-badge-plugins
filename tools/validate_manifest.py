#!/usr/bin/env python3
"""Validate a plugin meta.json against the schema.

Run as: python tools/validate_manifest.py examples/hello_world/meta.json
"""

import argparse
import json
import re
import sys
from pathlib import Path


REQUIRED_FIELDS = ["id", "version", "host_api_level_min", "linear_memory_kb"]
ID_PATTERN = re.compile(r"^[a-z][a-z0-9_]{1,31}$")
VERSION_PATTERN = re.compile(r"^\d+\.\d+\.\d+$")
API_LEVEL_PATTERN = re.compile(r"^\d+\.\d+$")
KNOWN_PREREQUISITES = {
    "wifi_connected", "ble_active", "network_reachable", "time_synced",
    "secure_element_ready", "battery_min", "not_charging", "usb_connected",
    "min_free_psram", "min_free_dram", "unlocked", "module_loaded", "feature_flag",
}

KNOWN_CAPABILITIES = {
    "wifi", "ble", "http", "socket", "rmem", "ecc", "ble_service_uuids",
    "message_types", "nvs_namespace", "display_lowlevel", "ui_exclusive",
    "usb_cdc", "gpio_pins", "pwm_pins", "adc_pins", "i2c_bus", "sao", "grove",
    "pixel_strip", "background", "autoload", "prevent_sleep", "vfat",
}


def fail(path: Path, msg: str) -> None:
    print(f"{path}: ERROR: {msg}", file=sys.stderr)
    raise SystemExit(1)


def validate(path: Path) -> None:
    with path.open("r", encoding="utf-8") as f:
        manifest = json.load(f)

    for field in REQUIRED_FIELDS:
        if field not in manifest:
            fail(path, f"missing required field '{field}'")

    if not ID_PATTERN.match(manifest["id"]):
        fail(path, f"id '{manifest['id']}' must match {ID_PATTERN.pattern}")

    if not VERSION_PATTERN.match(manifest["version"]):
        fail(path, f"version must be semver MAJOR.MINOR.PATCH, got '{manifest['version']}'")

    if not API_LEVEL_PATTERN.match(manifest["host_api_level_min"]):
        fail(path, "host_api_level_min must be MAJOR.MINOR")

    lm = manifest["linear_memory_kb"]
    if not isinstance(lm, int) or lm < 16 or lm > 1024:
        fail(path, f"linear_memory_kb must be int in [16, 1024], got {lm}")

    for name in manifest.get("prerequisites", {}):
        if name not in KNOWN_PREREQUISITES:
            fail(path, f"unknown prerequisite '{name}'")

    for name in manifest.get("capabilities", {}):
        if name not in KNOWN_CAPABILITIES:
            fail(path, f"unknown capability '{name}'")

    ns = manifest.get("capabilities", {}).get("nvs_namespace")
    if ns is not None:
        if len(ns) > 15:
            fail(path, f"nvs_namespace '{ns}' exceeds 15 chars (NVS hard limit)")
        if not (ns.startswith("plg_") or ns.startswith("plugin_")):
            fail(path, f"nvs_namespace '{ns}' must start with 'plg_' or 'plugin_' "
                       f"to isolate plugin storage from system NVS (wifi creds, etc.)")
        if not all(c.islower() or c.isdigit() or c == "_" for c in ns):
            fail(path, f"nvs_namespace '{ns}' must be [a-z0-9_] only")

    i18n = manifest.get("i18n", {})
    meta = i18n.get("meta", {})
    if "name" not in meta:
        fail(path, "i18n.meta.name is required")

    print(f"{path}: OK")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifests", nargs="+", help="Paths to meta.json files")
    args = parser.parse_args()
    for m in args.manifests:
        validate(Path(m))
    return 0


if __name__ == "__main__":
    sys.exit(main())
