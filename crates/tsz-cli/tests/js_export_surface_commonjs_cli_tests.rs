//! `module.exports = { default }` + sibling `exports.configs` export-surface
//! row, migrated from the two-binder in-process
//! `inspect_commonjs_two_file_consumer_symbol` harness to the real `tsz` binary.
//!
//! The manual two-binder inspector (two independently-built arenas + one shared
//! `TypeInterner` + manual `module_exports` insertion) diverges on cross-interner
//! atom identity, so the consumer symbol shape flips in-harness even though tsc
//! 7.0.2 and the real binary agree: the illegal export-assignment/sibling combo
//! is `TS2309`, the require() surface is exactly `{ default }`, and the late
//! `configs` sibling is dropped (reading it is `TS2339`). Verified standalone
//! against tsc 7.0.2 before migration.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_js_export_surface_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

/// Write `files` (relative path, contents) into a temp dir and run `tsz` over
/// them in checked-JS CommonJS mode, returning combined stdout+stderr.
fn run_tsz_files(name: &str, files: &[(&str, &str)]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let mut args: Vec<String> = Vec::new();
    for (rel, contents) in files {
        let path = temp.path.join(rel);
        std::fs::write(&path, contents).expect("write fixture");
        args.push((*rel).to_string());
    }
    args.extend(
        [
            "--allowJs",
            "--checkJs",
            "--strict",
            "--module",
            "commonjs",
            "--esModuleInterop",
            "--noEmit",
            "--pretty",
            "false",
        ]
        .map(String::from),
    );
    let output = Command::new(tsz_bin)
        .args(&args)
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

const LIB_JS: &str = r#"
const defaultConfig = { parser: "babel" };
module.exports = { default: defaultConfig };
exports.configs = { "stage-0": defaultConfig };
"#;

/// TS7: `module.exports = { default }` mixed with a sibling `exports.configs`
/// write is an illegal export-assignment combination (`TS2309`). The require()
/// consumer surface is exactly `{ default }`: `default` resolves, while the
/// dropped `configs` sibling is `TS2339`. tsc 7.0.2 and the real `tsz` binary
/// agree; only the stripped two-binder in-process harness diverged.
#[test]
fn require_consumer_keeps_default_surface_and_drops_illegal_configs_sibling() {
    let consumer = r#"
import lib = require("./lib.js");
const value = lib;
const onSurface = value.default;
const dropped = value.configs;
"#;
    let Some(out) = run_tsz_files(
        "default_before_late_exports",
        &[("lib.js", LIB_JS), ("consumer.ts", consumer)],
    ) else {
        println!("tsz binary not found; skipping");
        return;
    };

    // Illegal export-assignment + sibling `exports.*` combination.
    assert!(
        out.contains("TS2309"),
        "lib.js must report TS2309 (export assignment with other exported elements); got:\n{out}"
    );
    // The late `exports.configs` sibling is dropped from the require() surface,
    // so reading it is a missing-property error.
    assert!(
        out.contains("Property 'configs' does not exist"),
        "the `configs` sibling must be dropped from the require() surface (TS2339 on read); got:\n{out}"
    );
    // The direct `default` export IS on the surface, so reading it must not error.
    assert!(
        !out.contains("Property 'default' does not exist"),
        "the direct `default` export must remain on the require() surface; got:\n{out}"
    );
}
