//! DTS emit: an object-rest binding (`const { a, ...rest } = obj`) declares
//! `rest` as the source type with the sibling-bound keys **omitted**, never as
//! the full source object.
//!
//! Structural rule: tsc types an object-rest element as `Omit<Source, K>` where
//! `K` is the union of every non-rest sibling key bound in the same pattern
//! (its `getRestType`). So `const { a, ...rest } = { a; b; c }` gives
//! `rest: { b; c }`, and the emitted `.d.ts` must reflect that. Before the fix
//! the declaration emitter handed the rest element the *entire* source object
//! type, re-surfacing the already-bound `a` in the `.d.ts` (a wrong, wider
//! type). The emitter now defers to the checker's already-computed rest type
//! rather than substituting the raw source type.
//!
//! These run the full checker pipeline (the unit-level declaration-emit harness
//! uses an empty type cache and cannot infer the omitted rest types). Each rule
//! is exercised with more than one spelling / binder name so a regression keyed
//! on a particular property name rather than the structural shape would fail.

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
        path.push(format!("tsz_object_rest_binding_dts_{name}_{nanos}"));
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
            "es2020",
            "--lib",
            "es2020",
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

/// Compact whitespace so multi-line object type literals can be asserted with a
/// single canonical spelling regardless of the emitter's line wrapping.
fn squash(dts: &str) -> String {
    dts.split_whitespace().collect::<Vec<_>>().join(" ")
}

// =============================================================================
// Core rule: the rest element omits every sibling-bound key.
// =============================================================================

#[test]
fn object_rest_omits_single_bound_sibling() {
    let dts = dts_or_skip!(
        "single",
        "declare const o: { a: number; b: string; c: boolean };\n\
         export const { a, ...rest } = o;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("rest: { b: string; c: boolean; }"),
        "rest must omit the bound `a`, keeping only `b`/`c`:\n{dts}"
    );
    assert!(
        !s.contains("rest: { a: number;"),
        "rest must not re-surface the already-bound `a`:\n{dts}"
    );
}

#[test]
fn object_rest_omits_multiple_bound_siblings() {
    // Different binder names than the first case so a fixture-name fast path
    // would not satisfy both.
    let dts = dts_or_skip!(
        "multi",
        "declare const src: { one: number; two: string; three: boolean; four: null };\n\
         export const { one, two, ...remaining } = src;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("remaining: { three: boolean; four: null; }"),
        "rest must omit both bound siblings `one`/`two`:\n{dts}"
    );
    assert!(
        !s.contains("one:") || !s.contains("remaining: { one:"),
        "rest must not contain the bound `one`:\n{dts}"
    );
}

#[test]
fn object_rest_with_renamed_binding_omits_source_key() {
    // `{ a: aa, ...rest }` binds the *source* key `a`; the rest omits `a`,
    // not the local alias `aa`.
    let dts = dts_or_skip!(
        "renamed",
        "declare const o: { a: number; b: string; c: boolean };\n\
         export const { a: aa, ...rest } = o;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("rest: { b: string; c: boolean; }"),
        "renamed binding must still omit the source key `a`:\n{dts}"
    );
}

#[test]
fn object_rest_with_computed_string_literal_key_omits_it() {
    let dts = dts_or_skip!(
        "computed",
        "declare const o: { a: number; b: string; c: boolean };\n\
         export const { ['a']: x, ...rest } = o;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("rest: { b: string; c: boolean; }"),
        "computed string-literal key must be omitted from the rest:\n{dts}"
    );
}

#[test]
fn object_rest_clone_without_siblings_keeps_all_keys() {
    // No sibling bindings: the rest is the full source object (nothing omitted).
    let dts = dts_or_skip!(
        "clone",
        "declare const o: { a: number; b: string };\n\
         export const { ...clone } = o;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("clone: { a: number; b: string; }"),
        "a sibling-free object rest is the whole source object:\n{dts}"
    );
}

#[test]
fn object_rest_over_generic_source_uses_omit() {
    // A rest over a still-generic source is declared as `Omit<T, "id">`,
    // matching tsc's deferred rest type.
    let dts = dts_or_skip!(
        "generic",
        "export function f<T extends { id: number; name: string }>(o: T) {\n\
         const { id, ...rest } = o;\n\
         return rest;\n\
         }\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("): Omit<T, \"id\">;"),
        "a rest over a generic source must be declared as `Omit<T, \"id\">`:\n{dts}"
    );
}

#[test]
fn nested_object_rest_omits_at_each_level() {
    let dts = dts_or_skip!(
        "nested",
        "declare const o: { inner: { p: number; q: string; r: boolean }; top: number };\n\
         export const { inner: { p, ...innerRest }, ...outerRest } = o;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("innerRest: { q: string; r: boolean; }"),
        "inner rest must omit the bound `p`:\n{dts}"
    );
    assert!(
        s.contains("outerRest: { top: number; }"),
        "outer rest must omit the destructured `inner`:\n{dts}"
    );
}

// =============================================================================
// Counter-regression: array rest is unaffected by the object-rest change.
// =============================================================================

#[test]
fn array_rest_binding_still_emits_tail_array() {
    let dts = dts_or_skip!(
        "array",
        "declare const t: [number, string, boolean];\n\
         export const [first, ...tail] = t;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("first: number") && s.contains("tail: [string, boolean]"),
        "array rest must keep the tail tuple type:\n{dts}"
    );
}
