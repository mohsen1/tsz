//! TS2550 "change your target library" parity tests.
//!
//! When a property is missing only because the configured `lib`/`target` is
//! older than the lib that first introduced it, tsc reports TS2550 with a
//! suggested lib (`Try changing the 'lib' compiler option to '<lib>' or
//! later.`) instead of the bare TS2339 / TS2551 ("did you mean") diagnostic.
//!
//! The lib for each `(type, property)` pair is driven by the table in
//! `tsz_checker::error_reporter::suggestions::get_lib_for_type_property`,
//! which mirrors tsc's `getScriptTargetFeatures`. These end-to-end tests load
//! the real lib at a low target so the property is genuinely absent, and lock
//! in the diagnostic code plus the exact suggested lib.

use std::path::{Path, PathBuf};
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
        path.push(format!("tsz_lib_target_ts2550_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write repro file");
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

/// Run `tsz --noEmit` on `source` at `lib` (used for both `--target` and
/// `--lib`) and return the combined stdout+stderr diagnostic text.
fn check_at_lib(source: &str, lib: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(lib).expect("temp dir");
    let file = temp.path.join("repro.ts");
    write_file(&file, source);
    let output = Command::new(tsz_bin)
        .args([
            "repro.ts", "--noEmit", "--pretty", "false", "--target", lib, "--lib", lib,
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// Assert that accessing `expr` at `lib` reports TS2550 suggesting
/// `expected_lib`.
fn assert_ts2550(expr: &str, lib: &str, expected_lib: &str) {
    let source = format!("const _r = ({expr});\n");
    let Some(text) = check_at_lib(&source, lib) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        text.contains("error TS2550:"),
        "expected TS2550 for `{expr}` at lib {lib}, got:\n{text}"
    );
    let needle = format!("Try changing the 'lib' compiler option to '{expected_lib}' or later.");
    assert!(
        text.contains(&needle),
        "expected suggested lib '{expected_lib}' for `{expr}` at lib {lib}, got:\n{text}"
    );
    // The bare "does not exist" code must not leak alongside the richer message.
    assert!(
        !text.contains("error TS2551:"),
        "TS2550 must take priority over the TS2551 spelling suggestion for `{expr}`:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// SymbolConstructor: dispose/asyncDispose/metadata (esnext), matchAll (es2020).
// These were previously absent from the table (the receiver resolves to
// `SymbolConstructor`, which had no entry), so tsz reported a bare TS2339.
// ---------------------------------------------------------------------------

#[test]
fn symbol_dispose_suggests_esnext() {
    assert_ts2550("Symbol.dispose", "es2017", "esnext");
}

#[test]
fn symbol_async_dispose_suggests_esnext() {
    assert_ts2550("Symbol.asyncDispose", "es2017", "esnext");
}

#[test]
fn symbol_metadata_suggests_esnext() {
    assert_ts2550("Symbol.metadata", "es2017", "esnext");
}

#[test]
fn symbol_match_all_suggests_es2020() {
    assert_ts2550("Symbol.matchAll", "es2019", "es2020");
}

// ---------------------------------------------------------------------------
// PromiseConstructor.withResolvers (es2024) — was missing entirely.
// ---------------------------------------------------------------------------

#[test]
fn promise_with_resolvers_suggests_es2024() {
    assert_ts2550("Promise.withResolvers()", "es2021", "es2024");
}

// ---------------------------------------------------------------------------
// ObjectConstructor.getOwnPropertyDescriptors (es2017) — was missing, so tsz
// fell through to the TS2551 "did you mean getOwnPropertyDescriptor" path.
// ---------------------------------------------------------------------------

#[test]
fn object_get_own_property_descriptors_suggests_es2017() {
    assert_ts2550("Object.getOwnPropertyDescriptors({})", "es2015", "es2017");
}

// ---------------------------------------------------------------------------
// Math.f16round (es2025) — was missing, fell through to TS2551.
// ---------------------------------------------------------------------------

#[test]
fn math_f16round_suggests_es2025() {
    assert_ts2550("Math.f16round(1)", "es2022", "es2025");
}

// ---------------------------------------------------------------------------
// Set set-operations (es2025) — the old blanket `Set => es2015` reported the
// wrong lib; tsc introduces these in es2025.
// ---------------------------------------------------------------------------

#[test]
fn set_union_suggests_es2025() {
    assert_ts2550(
        "new Set<number>().union(new Set<number>())",
        "es2022",
        "es2025",
    );
}

#[test]
fn set_difference_suggests_es2025() {
    assert_ts2550(
        "new Set<number>().difference(new Set<number>())",
        "es2022",
        "es2025",
    );
}

// ---------------------------------------------------------------------------
// Map.getOrInsert (esnext) — old blanket `Map => es2015` was wrong.
// ---------------------------------------------------------------------------

#[test]
fn map_get_or_insert_suggests_esnext() {
    assert_ts2550(
        "new Map<number, number>().getOrInsert(1, 2)",
        "es2022",
        "esnext",
    );
}

// ---------------------------------------------------------------------------
// Array mutation-copy helpers — the old table over-reported `esnext`; tsc
// introduces toReversed/toSorted/toSpliced/with in es2023.
// ---------------------------------------------------------------------------

#[test]
fn array_to_reversed_suggests_es2023() {
    assert_ts2550("[1, 2].toReversed()", "es2015", "es2023");
}

#[test]
fn array_with_suggests_es2023() {
    assert_ts2550("[1, 2].with(0, 9)", "es2015", "es2023");
}

// ---------------------------------------------------------------------------
// String.isWellFormed (es2024) — old table over-reported `esnext`.
// ---------------------------------------------------------------------------

#[test]
fn string_is_well_formed_suggests_es2024() {
    assert_ts2550("'a'.isWellFormed()", "es2022", "es2024");
}

// ---------------------------------------------------------------------------
// ArrayBuffer resize/transfer (es2024) — no ArrayBuffer entry existed.
// ---------------------------------------------------------------------------

#[test]
fn array_buffer_resize_suggests_es2024() {
    assert_ts2550("new ArrayBuffer(8).resize(16)", "es2022", "es2024");
}

#[test]
fn array_buffer_transfer_suggests_es2024() {
    assert_ts2550("new ArrayBuffer(8).transfer()", "es2022", "es2024");
}

// ---------------------------------------------------------------------------
// ErrorConstructor.isError (esnext) — kept SEPARATE from the `Error` instance
// entry so `(new Error()).isError` stays a bare TS2339 like tsc.
// ---------------------------------------------------------------------------

#[test]
fn error_is_error_constructor_suggests_esnext() {
    assert_ts2550("Error.isError({})", "es2022", "esnext");
}

#[test]
fn error_instance_is_error_stays_ts2339() {
    // `isError` lives on `ErrorConstructor`, not the `Error` instance, so an
    // instance access must NOT be promoted to TS2550 (parity with tsc).
    let Some(text) = check_at_lib("const _r = (new Error('x').isError);\n", "esnext") else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        text.contains("error TS2339:") && !text.contains("error TS2550:"),
        "instance Error.isError must stay TS2339, got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// DataView 64-bit accessors (es2020) and Date.toTemporalInstant (esnext) — no
// entry existed for either receiver type.
// ---------------------------------------------------------------------------

#[test]
fn data_view_get_big_int64_suggests_es2020() {
    assert_ts2550(
        "new DataView(new ArrayBuffer(8)).getBigInt64(0)",
        "es2019",
        "es2020",
    );
}

#[test]
fn date_to_temporal_instant_suggests_esnext() {
    assert_ts2550("new Date().toTemporalInstant()", "es2022", "esnext");
}

// ---------------------------------------------------------------------------
// Parity guard: a genuinely non-existent member is NOT in any lib, so it must
// remain a bare TS2339 — the table must not synthesize a false TS2550.
// ---------------------------------------------------------------------------

#[test]
fn unknown_member_stays_ts2339() {
    let Some(text) = check_at_lib("const _r = ([1, 2].definitelyNotAMethod());\n", "es2015") else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        text.contains("error TS2339:") && !text.contains("error TS2550:"),
        "unknown member must stay TS2339, got:\n{text}"
    );
}
