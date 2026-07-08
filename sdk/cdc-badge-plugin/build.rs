//! Generate Rust constants from sdk/host_api.h so the SDK never drifts from
//! the C-side definition. Run automatically by cargo before each build.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let header_path = PathBuf::from(&manifest_dir).join("../host_api.h");
    println!("cargo:rerun-if-changed={}", header_path.display());

    let header = fs::read_to_string(&header_path)
        .unwrap_or_else(|e| panic!("read {}: {}", header_path.display(), e));

    let prefixes = [
        "UI_ICON_",
        "HOST_OK",
        "HOST_ERR_",
        "LOG_LEVEL_",
        "HOST_EXT_FEATURE_",
        "HOST_MSG_",
        "HOST_VCARD_",
        "HOST_SURFACE_",
        "HOST_SPRITE_",
        "HOST_ANIM_",
        "HOST_EASE_",
        "HOST_CANVAS_ANIM_",
    ];

    let mut values: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for raw in header.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut parts = rest.splitn(2, |c: char| c.is_whitespace());
        let name = parts.next().unwrap_or("").trim().to_string();
        let Some(value_part) = parts.next() else {
            continue;
        };
        if !prefixes.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let value = value_part
            .split("/*")
            .next()
            .unwrap_or("")
            .split("//")
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();
        values.insert(name.clone(), value);
        order.push(name);
    }

    let mut out = String::new();
    out.push_str("// Auto-generated from sdk/host_api.h - do not edit by hand.\n");
    for name in &order {
        let raw = values.get(name).unwrap();
        let rendered = render_value(raw, &values, name).unwrap_or_else(|| raw.clone());
        let rust_type = if name.starts_with("UI_ICON_") || name.starts_with("LOG_LEVEL_") {
            "u8"
        } else {
            "core::ffi::c_int"
        };
        out.push_str(&format!(
            "pub const {}: {} = {};\n",
            name, rust_type, rendered
        ));
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = PathBuf::from(out_dir).join("host_api_consts.rs");
    fs::write(&dest, out).expect("write host_api_consts.rs");
}

fn render_value(value: &str, all: &HashMap<String, String>, _self_name: &str) -> Option<String> {
    let v = value.trim();
    if v.starts_with("0x") || v.starts_with("0X") {
        return Some(v.to_string());
    }
    if v.chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '+')
    {
        return Some(v.to_string());
    }
    if let Some(_other) = all.get(v) {
        return Some(v.to_string());
    }
    if v.starts_with('(') && v.ends_with(')') {
        return Some(v.to_string());
    }
    None
}
