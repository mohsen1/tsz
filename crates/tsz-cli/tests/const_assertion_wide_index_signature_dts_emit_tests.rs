//! DTS emit: the synthesized index-signature union of a `const`-asserted
//! object literal that also has a wide (non-literal-key) computed member.
//!
//! Structural rule, oracled against `typescript@7.0.2`
//! (conformance/expressions/typeAssertions/constAssertions.ts): when a
//! `const`-asserted object literal has a computed member whose key cannot be
//! resolved to a literal type, tsc folds every member's own value type into
//! the synthesized string index signature — not in plain source order, but in
//! two tiers, each internally in source order: every literal/primitive-ish
//! type first, then every structural (object shape, array/tuple, or
//! function/constructor) type. `null` sorts after the structural tier,
//! `undefined` after `null`, matching the printer's own nullable-tail
//! convention.
//!
//! Before the fix, the declaration emitter's source-text recovery for this
//! union (`source_ordered_object_literal_index_value_union_text`,
//! `crates/tsz-emitter/src/declaration_emitter/helpers/
//! type_inference_object_rewrites.rs`) used plain source order, so a method
//! declared ahead of the wide computed key printed ahead of later primitive
//! members instead of after all of them.
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
        path.push(format!("tsz_const_wide_index_dts_{name}_{nanos}"));
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
fn method_sorts_after_every_primitive_regardless_of_source_position() {
    let dts = dts_or_skip!(
        "method_after_primitives",
        "export const o = { a: 1, b: 2, c: 3, d() {}, ['e' + '']: 4 } as const;\n"
    );
    assert!(
        dts.contains("1 | 2 | 3 | 4 | (() => void)"),
        "method-valued member must sort after every primitive: {dts}"
    );
}

#[test]
fn method_written_before_primitives_still_sorts_last() {
    // Adjacent case: the method is now the FIRST member — tsc still renders
    // it last, so this cannot be a first/last-written heuristic.
    let dts = dts_or_skip!(
        "method_first_still_last",
        "export const o = { d() {}, a: 1, b: 2, ['e' + '']: 4 } as const;\n"
    );
    assert!(
        dts.contains("1 | 2 | 4 | (() => void)"),
        "method position in source must not change its tail placement: {dts}"
    );
}

#[test]
fn object_shape_sorts_before_tuple_after_primitives() {
    // Adjacent case: two structural kinds keep their own relative order after
    // the primitive tier, and that relative order is by kind (object before
    // tuple), not by which was written first in source.
    let dts = dts_or_skip!(
        "object_before_tuple",
        "export const o = { a: 1, arr: [1, 2], obj: { x: 1 }, ['e' + '']: 4 } as const;\n"
    );
    let idx_primitive = dts.find("1 | 4").expect("primitive tier rendered first");
    let idx_obj = dts.find("readonly x: 1").expect("object member rendered");
    let idx_tuple = dts.rfind("readonly [1, 2]").expect("tuple member rendered");
    assert!(
        idx_primitive < idx_obj && idx_obj < idx_tuple,
        "expected primitive tier, then object shape, then tuple, in that order: {dts}"
    );
}

#[test]
fn structural_type_sorts_before_null_and_undefined() {
    // Adjacent case: null/undefined sort after the structural tier, null
    // before undefined, matching the printer's own nullable-tail convention.
    let dts = dts_or_skip!(
        "structural_before_nullish",
        "export const o = { a: 1, n: null, u: undefined, d() {}, ['e' + '']: 4 } as const;\n"
    );
    assert!(
        dts.contains("1 | 4 | (() => void) | null | undefined"),
        "expected primitives, then the method, then null, then undefined: {dts}"
    );
}

#[test]
fn two_structural_members_of_the_same_kind_keep_source_order() {
    // Negative/fallback case: within the structural tier itself, two members
    // of the SAME kind (both functions) are deduped to one union member (the
    // printed function type text is identical), so this must not regress to
    // printing the type twice.
    let dts = dts_or_skip!(
        "same_kind_structural_dedup",
        "export const o = { a: 1, d() {}, f() {}, ['e' + '']: 4 } as const;\n"
    );
    assert!(
        dts.contains("1 | 4 | (() => void);"),
        "two same-shaped function members must collapse to one union entry: {dts}"
    );
}
