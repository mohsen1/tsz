//! DTS emit: the synthesized `[x: string]` index-signature union of an object
//! literal whose computed members mix string- and number-keyed entries.
//!
//! Structural rule, oracled against `typescript@7.0.2`
//! (conformance/types/members/indexSignatures1.ts's `obj13`): a JS numeric
//! property key is also reachable through the string index (engines coerce
//! numeric keys to strings), so `tsc` folds every string- AND number-keyed
//! computed member's value type into one `[x: string]` union, in true source
//! declaration order across both kinds — not "every string-kind member, then
//! every number-kind member".
//!
//! Before the fix, `rewrite_object_literal_computed_index_signatures`
//! (`crates/tsz-emitter/src/declaration_emitter/helpers/
//! type_inference_object_rewrites.rs`) built the `[x: string]` union from two
//! separate buckets — "concrete-kind" members (regardless of string vs
//! number) and "dynamic-kind" members (regardless of string vs number) —
//! concatenated concrete-bucket-then-dynamic-bucket. That reorders
//! `'x': 0, 'a'+'b': 1, [1]: 2, [1+2]: 3` (declared in that order) into
//! `0 | 2 | 1 | 3` instead of `0 | 1 | 2 | 3`.
//!
//! These run the full checker pipeline (the emitter's unit-level harness uses
//! an empty type cache and cannot reach this rewrite path at all).

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
        path.push(format!("tsz_computed_index_order_dts_{name}_{nanos}"));
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

fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--strict",
            "--target",
            "esnext",
            "--lib",
            "esnext",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    let dts = std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_else(|_| {
        panic!(
            "expected repro.d.ts to be emitted.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Some(dts)
}

macro_rules! dts_or_skip {
    ($name:expr, $src:expr) => {
        match emit_dts($name, $src) {
            Some(dts) => dts,
            None => {
                println!("skipping: tsz binary not found");
                return;
            }
        }
    };
}

#[test]
fn string_and_number_keyed_members_stay_in_source_order() {
    // The exact shape from indexSignatures1.ts's obj13 (string, dynamic
    // string, concrete number, dynamic number, in that source order).
    let dts = dts_or_skip!(
        "obj13_shape",
        "export const sym = Symbol();\n\
         export const o = {\n\
         \x20   ['x']: 0 as const,\n\
         \x20   ['a' + 'b']: 1 as const,\n\
         \x20   [1]: 2 as const,\n\
         \x20   [1 + 2]: 3 as const,\n\
         \x20   [sym]: 4 as const,\n\
         \x20   [Symbol()]: 5 as const,\n\
         };\n"
    );
    assert!(
        dts.contains("[x: string]: 0 | 1 | 2 | 3;"),
        "string index must list every string/number member in source order: {dts}"
    );
    assert!(
        dts.contains("[x: number]: 2 | 3;"),
        "number index unaffected: {dts}"
    );
    assert!(
        dts.contains("[x: symbol]: 4 | 5;"),
        "symbol index unaffected: {dts}"
    );
}

#[test]
fn number_keyed_member_written_before_string_keyed_member() {
    // Adjacent case: reversing which kind is declared first still preserves
    // true source order — this cannot be "string bucket always first".
    let dts = dts_or_skip!(
        "number_before_string",
        "export const o = {\n\
         \x20   [1]: 0 as const,\n\
         \x20   ['x']: 1 as const,\n\
         \x20   [1 + 2]: 2 as const,\n\
         \x20   ['a' + 'b']: 3 as const,\n\
         };\n"
    );
    assert!(
        dts.contains("[x: string]: 0 | 1 | 2 | 3;"),
        "source order must survive even when a number-keyed member leads: {dts}"
    );
    assert!(
        dts.contains("[x: number]: 0 | 2;"),
        "number index keeps its own source order: {dts}"
    );
}

#[test]
fn only_dynamic_string_members_still_render_in_source_order() {
    // Negative/fallback case: no number-keyed member at all, so the
    // concrete/dynamic split degenerates to a single bucket — must still
    // work (this path already passed before the fix, guards no regression).
    let dts = dts_or_skip!(
        "string_only",
        "export const o = {\n\
         \x20   ['x']: 0 as const,\n\
         \x20   ['a' + 'b']: 1 as const,\n\
         \x20   ['c' + 'd']: 2 as const,\n\
         };\n"
    );
    assert!(
        dts.contains("[x: string]: 0 | 1 | 2;"),
        "string-only union must stay in source order: {dts}"
    );
}
