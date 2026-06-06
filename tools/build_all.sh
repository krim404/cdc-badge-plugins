#!/usr/bin/env bash
# Build every plugin in the workspace and stage the artefacts in dist/.
#
# Sources: examples/ (demo plugins) + plugins/ (shipped plugins).
# Output:  dist/<plugin_id>.wasm + dist/<plugin_id>.meta.json
#          dist/<plugin_id>.lang.json (when a translation file is present)
# Requires: rustup with the wasm32-unknown-unknown target, optional wasm-opt.

set -euo pipefail

# rustup is Homebrew-installed without ~/.cargo/bin shims, so cargo is absent
# from PATH in non-interactive shells. Resolve it through rustup, which honours
# the rust-toolchain.toml override.
if ! command -v cargo >/dev/null 2>&1; then
  PATH="$(dirname "$(rustup which cargo)"):${PATH}"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="${REPO_ROOT}/target/wasm32-unknown-unknown/release"
DIST_DIR="${REPO_ROOT}/dist"

mkdir -p "${DIST_DIR}"

cd "${REPO_ROOT}"
# The matrix plugin is a std crate (it links vodozemac for E2EE), while every
# other plugin is no_std. Building them in one cargo invocation unifies features
# across the shared cdc-badge-plugin / serde_json deps and leaks std into the
# no_std plugins (duplicate panic_impl). Build the no_std plugins together, then
# matrix on its own.
cargo build --release --target wasm32-unknown-unknown --workspace --exclude matrix
cargo build --release --target wasm32-unknown-unknown -p matrix

WASM_OPT="$(command -v wasm-opt || true)"

for dir in "${REPO_ROOT}"/examples/*/ "${REPO_ROOT}"/plugins/*/; do
  [[ -d "${dir}" ]] || continue
  plugin_id="$(basename "${dir}")"
  meta_file="${dir}meta.json"
  lang_file="${dir}${plugin_id}.lang.json"
  wasm_file="${TARGET_DIR}/${plugin_id}.wasm"

  if [[ ! -f "${meta_file}" ]]; then
    echo "skip ${plugin_id}: no meta.json"
    continue
  fi

  if [[ ! -f "${wasm_file}" ]]; then
    echo "skip ${plugin_id}: no built wasm artifact"
    continue
  fi

  manifest_id="$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['id'])" "${meta_file}")"
  if [[ "${manifest_id}" != "${plugin_id}" ]]; then
    echo "error: ${meta_file} declares id '${manifest_id}' but the directory is '${plugin_id}'." >&2
    echo "       Catalog URLs follow the manifest id, build output follows the directory name -" >&2
    echo "       any drift breaks plugin downloads in the web installer." >&2
    exit 1
  fi

  out_wasm="${DIST_DIR}/${plugin_id}.wasm"
  out_meta="${DIST_DIR}/${plugin_id}.meta.json"
  out_lang="${DIST_DIR}/${plugin_id}.lang.json"

  if [[ -n "${WASM_OPT}" ]]; then
    "${WASM_OPT}" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int "${wasm_file}" -o "${out_wasm}"
  else
    cp "${wasm_file}" "${out_wasm}"
  fi
  cp "${meta_file}" "${out_meta}"
  if [[ -f "${lang_file}" ]]; then
    cp "${lang_file}" "${out_lang}"
  fi

  size_kb=$(( $(wc -c < "${out_wasm}") / 1024 ))
  echo "built ${plugin_id}: ${size_kb} KB"

  python3 "${REPO_ROOT}/tools/validate_manifest.py" "${out_meta}"
done

echo "All built into ${DIST_DIR}"
