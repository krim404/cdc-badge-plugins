#!/usr/bin/env python3
"""Generate catalog.json from every plugin manifest in the repo.

Scans both `examples/*/meta.json` (demo plugins) and `plugins/*/meta.json`
(shipped plugins) and emits a single catalog consumed by the web flasher.
"""

import argparse
import json
import os
import sys
from pathlib import Path


def load_manifest(meta_path: Path) -> dict:
    with meta_path.open("r", encoding="utf-8") as f:
        return json.load(f)


def english(field) -> str:
    if isinstance(field, dict):
        return field.get("en") or next(iter(field.values()), "")
    return str(field)


def build_catalog(repo_root: Path, release_base_url: str, version: str,
                  dist_dir: Path) -> dict:
    plugin_dirs: list[Path] = []
    for top in ("examples", "plugins"):
        top_dir = repo_root / top
        if not top_dir.is_dir():
            continue
        plugin_dirs.extend(sorted(p for p in top_dir.iterdir() if p.is_dir()))

    seen_ids: set[str] = set()
    plugins = []
    for plugin_dir in plugin_dirs:
        meta_path = plugin_dir / "meta.json"
        if not meta_path.is_file():
            continue
        manifest = load_manifest(meta_path)
        plugin_id = manifest["id"]
        if plugin_id in seen_ids:
            raise SystemExit(f"duplicate plugin id '{plugin_id}' across examples/ and plugins/")
        seen_ids.add(plugin_id)
        i18n_meta = manifest.get("i18n", {}).get("meta", {})
        wasm_path = dist_dir / f"{plugin_id}.wasm"
        wasm_size_kb = None
        if wasm_path.is_file():
            wasm_size_kb = (wasm_path.stat().st_size + 1023) // 1024
        entry = {
            "id": plugin_id,
            "version": manifest.get("version", "0.0.0"),
            "author": manifest.get("author", ""),
            "name": english(i18n_meta.get("name", plugin_id)),
            "description": english(i18n_meta.get("description", "")),
            "icon": manifest.get("icon", "info"),
            "host_api_level_min": manifest.get("host_api_level_min", "0.8"),
            "linear_memory_kb": manifest.get("linear_memory_kb", 64),
            "capabilities": manifest.get("capabilities", {}),
            "prerequisites": manifest.get("prerequisites", {}),
            "wasm_url": f"{release_base_url}/{plugin_id}.wasm",
            "meta_url": f"{release_base_url}/{plugin_id}.meta.json",
        }
        if wasm_size_kb is not None:
            entry["wasm_size_kb"] = wasm_size_kb
        lang_path = plugin_dir / f"{plugin_id}.lang.json"
        if lang_path.is_file():
            entry["lang_url"] = f"{release_base_url}/{plugin_id}.lang.json"
        plugins.append(entry)

    return {
        "catalog_version": 1,
        "release_version": version,
        "plugins": plugins,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=".",
                        help="Path to the plugin repo root (default: cwd)")
    parser.add_argument("--release-base-url", required=True,
                        help="Base URL where the release assets are hosted")
    parser.add_argument("--version", default="latest",
                        help="Release version string written into the catalog")
    parser.add_argument("--out", default="catalog.json",
                        help="Output file path (default: catalog.json)")
    parser.add_argument("--dist", default="dist",
                        help="Directory holding built <id>.wasm files (default: dist)")
    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    dist_dir = (repo_root / args.dist).resolve() if not Path(args.dist).is_absolute() \
        else Path(args.dist).resolve()
    catalog = build_catalog(repo_root, args.release_base_url.rstrip("/"),
                            args.version, dist_dir)

    out_path = Path(args.out)
    out_path.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {out_path} with {len(catalog['plugins'])} plugins")
    return 0


if __name__ == "__main__":
    sys.exit(main())
