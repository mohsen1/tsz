//! Value-position "Cannot find name" spelling-suggestion parity (TS2552).
//!
//! `undefined` is a global *value* symbol in tsc (its synthetic
//! `undefinedSymbol`), while the intrinsic *type* keywords (`number`,
//! `boolean`, ...) are type-meaning symbols. Because tsz's `VALUE` and `TYPE`
//! symbol-flag masks overlap (both include `CLASS`/`ENUM`/`ENUM_MEMBER`),
//! gating the built-in keyword candidates on `meaning & TYPE` used to leak the
//! lowercase type keywords into value position (`numbr` → `number`) and never
//! offered `undefined`. tsc instead suggests the value *constructor*
//! (`numbr` → `Number`) and `undefined` (`udefined` → `undefined`).
//!
//! These end-to-end tests drive the real `tsz` binary — the in-process checker
//! harness does not report value-position `cannot find name`, so this is the
//! canonical place to lock the value/type split to tsc's behavior.

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
        path.push(format!("tsz_cannot_find_name_value_{name}_{nanos}"));
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

/// Run `tsz --strict --noEmit` on `source` and return combined stdout+stderr.
fn run_tsz(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let file = temp.path.join("repro.ts");
    std::fs::write(&file, source).expect("write repro file");
    let output = Command::new(tsz_bin)
        .args([
            "repro.ts", "--strict", "--noEmit", "--pretty", "false", "--target", "esnext", "--lib",
            "esnext",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// Assert `source` reports TS2552 for `typo` suggesting `expected`.
fn assert_suggests(name: &str, source: &str, typo: &str, expected: &str) {
    let Some(out) = run_tsz(name, source) else {
        println!("tsz binary not found; skipping {name}");
        return;
    };
    let needle = format!("Cannot find name '{typo}'. Did you mean '{expected}'?");
    assert!(
        out.contains("TS2552") && out.contains(&needle),
        "expected TS2552 \"{needle}\" for {name}, got:\n{out}"
    );
}

/// Assert `source` reports a bare TS2304 for `typo` and offers no TS2552
/// suggestion for it.
fn assert_bare_not_found(name: &str, source: &str, typo: &str) {
    let Some(out) = run_tsz(name, source) else {
        println!("tsz binary not found; skipping {name}");
        return;
    };
    assert!(
        out.contains(&format!("Cannot find name '{typo}'.")) && !out.contains("TS2552"),
        "expected bare TS2304 for '{typo}' in {name}, got:\n{out}"
    );
}

// --- Bug A: type keywords must not leak into value position; the value
// constructor is suggested instead. ---

#[test]
fn value_numbr_suggests_number_constructor() {
    assert_suggests("numbr", "const x = numbr;\n", "numbr", "Number");
}

#[test]
fn value_objet_suggests_object_constructor() {
    assert_suggests("objet", "const x = objet;\n", "objet", "Object");
}

#[test]
fn value_booleen_suggests_boolean_constructor() {
    assert_suggests("booleen", "const x = booleen;\n", "booleen", "Boolean");
}

#[test]
fn value_symbl_suggests_symbol_constructor() {
    assert_suggests("symbl", "const x = symbl;\n", "symbl", "Symbol");
}

// --- Bug B: `undefined` is a value symbol and is suggested in value position. ---

#[test]
fn value_udefined_suggests_undefined() {
    assert_suggests("udefined", "const x = udefined;\n", "udefined", "undefined");
}

#[test]
fn value_typeof_undefined_typo_suggests_undefined() {
    assert_suggests(
        "typeof_undef",
        "const x = typeof udnefined;\n",
        "udnefined",
        "undefined",
    );
}

// --- Negatives: keyword literals with no symbol entry are never suggested. ---

#[test]
fn value_viod_has_no_suggestion() {
    assert_bare_not_found("viod", "const x = viod;\n", "viod");
}

#[test]
fn value_nulll_has_no_suggestion() {
    assert_bare_not_found("nulll", "const x = nulll;\n", "nulll");
}

// --- Type position is unchanged: `undefined` is not a type symbol. ---

#[test]
fn type_undefined_typo_has_no_suggestion() {
    assert_bare_not_found("t_undef", "let x: undefindd;\n", "undefindd");
}
