//! DTS emit for object-rest destructuring bindings (`const { a, ...rest } = obj`).
//!
//! Structural rule: when an object binding pattern has a rest element, `tsc`
//! types the rest as `Omit<Source, K>` where `K` is the union of every non-rest
//! sibling key bound in the same pattern (its `getRestType`). The emitted
//! declaration must therefore drop those sibling keys — it must not re-surface
//! the whole source object. The checker already computes that omission via
//! `omit_properties_from_type` and caches it on the rest symbol; declaration
//! emit now defers to that computed type instead of re-deriving the source
//! object.
//!
//! Cases vary binder names, sibling counts, renamed/computed keys, generic
//! sources (which stay the deferred `Omit`), and nested patterns so the coverage
//! proves the structural rule rather than a single spelling. Array rest
//! (`[first, ...tail]`) is included as a negative case: it is unaffected and
//! still binds the residual tuple/array slice. Tests invoke the full `tsz`
//! binary so the checker's symbol-type cache (which the fix consults) is
//! populated.

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
        path.push(format!("tsz_obj_rest_dts_{name}_{nanos}"));
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

/// Collapse whitespace so assertions can match the emitted member set without
/// depending on the exact line breaks / indentation of the printed object type.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// =============================================================================
// Single and multiple bound siblings are omitted from the rest type
// =============================================================================

/// Primary repro: one bound sibling (`a`) is omitted; `rest` keeps `b`, `c`.
#[test]
fn single_bound_sibling_is_omitted_from_rest() {
    let Some(dts) = emit_dts(
        "single_sibling",
        r#"
declare const o: { a: number; b: number; c: number };
export const { a, ...rest } = o;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("rest: { b: number; c: number; }"),
        "rest must omit the bound sibling `a`:\n{dts}"
    );
    assert!(
        !flat.contains("rest: { a: number;"),
        "rest must not re-surface the bound sibling `a`:\n{dts}"
    );
}

/// Two bound siblings (`p`, `q`) are both omitted; renamed binders prove the
/// rule keys on the pattern, not on the names `a`/`rest`.
#[test]
fn multiple_bound_siblings_are_omitted_from_rest() {
    let Some(dts) = emit_dts(
        "multi_sibling",
        r#"
declare const source: { p: number; q: string; r: boolean; s: number };
export const { p, q, ...leftover } = source;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("leftover: { r: boolean; s: number; }"),
        "rest must omit both bound siblings `p` and `q`:\n{dts}"
    );
    assert!(
        !flat.contains("p: number") || !flat.contains("leftover: { p"),
        "rest must not re-surface `p`:\n{dts}"
    );
}

// =============================================================================
// Renamed and computed sibling keys omit the SOURCE key
// =============================================================================

/// A renamed binding (`{ a: aa, ...rest }`) omits the *source* key `a`, not the
/// local binder `aa`.
#[test]
fn renamed_sibling_omits_source_key() {
    let Some(dts) = emit_dts(
        "renamed_sibling",
        r#"
declare const o: { a: number; b: number; c: number };
export const { a: aa, ...others } = o;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("aa: number"),
        "the renamed local binder `aa` keeps its type:\n{dts}"
    );
    assert!(
        flat.contains("others: { b: number; c: number; }"),
        "rest must omit the SOURCE key `a` (not `aa`):\n{dts}"
    );
    assert!(
        !flat.contains("others: { a:"),
        "rest must not keep the omitted source key `a`:\n{dts}"
    );
}

/// A computed string-literal sibling key (`{ ['a']: v, ...rest }`) is omitted.
#[test]
fn computed_string_literal_sibling_is_omitted() {
    let Some(dts) = emit_dts(
        "computed_key",
        r#"
declare const o: { a: number; b: number; c: number };
export const { ['a']: aval, ...crest } = o;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("crest: { b: number; c: number; }"),
        "rest must omit the computed string-literal key `a`:\n{dts}"
    );
}

// =============================================================================
// Sibling-free rest keeps every key
// =============================================================================

/// A rest with no bound siblings (`{ ...clone }`) keeps the full source shape.
#[test]
fn sibling_free_rest_keeps_all_keys() {
    let Some(dts) = emit_dts(
        "clone_all",
        r#"
declare const src: { a: number; b: number; c: number };
export const { ...clone } = src;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("clone: { a: number; b: number; c: number; }"),
        "sibling-free rest must keep every source key:\n{dts}"
    );
}

// =============================================================================
// Generic source stays the deferred `Omit<T, K>`
// =============================================================================

/// When the source is still a type parameter, the rest stays the deferred
/// `Omit<T, "a">` — declaration emit must render that application, not the raw
/// constraint shape.
#[test]
fn generic_source_rest_is_deferred_omit() {
    let Some(dts) = emit_dts(
        "generic_omit",
        r#"
export function pick<T extends { a: number; b: string; c: boolean }>(t: T) {
  const { a, ...rest } = t;
  return rest;
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains(r#"): Omit<T, "a">"#),
        "generic-source rest must render as the deferred `Omit<T, \"a\">`:\n{dts}"
    );
}

// =============================================================================
// Nested object rest omits at each level
// =============================================================================

/// A nested rest (`{ p: { m, ...pr } }`) omits `m` from the *inner* object.
#[test]
fn nested_object_rest_omits_at_inner_level() {
    let Some(dts) = emit_dts(
        "nested_rest",
        r#"
interface Nested { p: { m: number; n: number; q: string } }
declare const nn: Nested;
export const { p: { m, ...pr } } = nn;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("m: number"),
        "inner bound key `m` keeps its type:\n{dts}"
    );
    assert!(
        flat.contains("pr: { n: number; q: string; }"),
        "inner rest must omit `m`:\n{dts}"
    );
    assert!(
        !flat.contains("pr: { m:"),
        "inner rest must not re-surface `m`:\n{dts}"
    );
}

// =============================================================================
// Negative: array rest is unaffected (residual tuple / array slice)
// =============================================================================

/// Array rest from a tuple source keeps producing the residual tuple slice.
#[test]
fn array_rest_from_tuple_is_unaffected() {
    let Some(dts) = emit_dts(
        "tuple_rest",
        r#"
declare const tup: [number, string, boolean];
export const [first, ...tail] = tup;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("first: number"),
        "leading tuple element keeps its type:\n{dts}"
    );
    assert!(
        flat.contains("tail: [string, boolean]"),
        "array rest from a tuple must keep the residual tuple slice:\n{dts}"
    );
}

/// Array rest from a plain array source keeps producing the array element type.
#[test]
fn array_rest_from_array_is_unaffected() {
    let Some(dts) = emit_dts(
        "array_rest",
        r#"
declare const arr: number[];
export const [head, ...atail] = arr;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    let flat = flatten(&dts);
    assert!(
        flat.contains("head: number"),
        "leading array element keeps its type:\n{dts}"
    );
    assert!(
        flat.contains("atail: number[]"),
        "array rest from an array must keep the array element type:\n{dts}"
    );
}
