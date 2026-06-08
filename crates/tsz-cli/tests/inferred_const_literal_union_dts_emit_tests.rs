//! DTS emit: widening of a `const` declaration whose inferred type is a
//! heterogeneous literal union produced by a fresh conditional initializer.
//!
//! Structural rule: tsc's `getWidenedType` widens each *fresh* literal member
//! of an inferred `const` type to its primitive base for declaration emit,
//! while keeping non-literal members. So `const v = cond ? 1 : "x"` emits
//! `string | number`, `const v = cond ? 1 : null` emits `number | null`, and
//! `const v = cond ? 1 : obj` emits `number | { ... }`.
//!
//! Before the fix, declaration emit only collapsed *homogeneous* literal unions
//! (`1 | 2` -> `number`) and left heterogeneous / literal-with-non-literal
//! unions un-widened (`1 | "x"`), diverging from `tsc`. `as const` unions (whose
//! literals are non-fresh) are preserved either way.
//!
//! These run the full checker pipeline (the unit-level declaration-emit harness
//! uses an empty type cache and cannot infer conditional-expression types).
//! Each behavioural case is exercised with at least two distinct member
//! spellings so a regression keyed on a particular literal value rather than the
//! structural shape would fail.

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
        path.push(format!("tsz_const_lit_union_dts_{name}_{nanos}"));
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
            "es2015",
            "--lib",
            "es6",
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

// =============================================================================
// Heterogeneous fresh literal unions widen per member (the fixed behaviour)
// =============================================================================

/// Primary repro: `cond ? 1 : "x"` -> `string | number` (not `1 | "x"`).
#[test]
fn mixed_literal_kinds_widen_to_primitive_union() {
    let Some(dts) = emit_dts(
        "mixed",
        "declare const k: boolean;\nexport const v = k ? 1 : \"x\";\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const v: string | number;"),
        "fresh mixed-kind literal union must widen per member:\n{dts}"
    );
}

/// Adjacent case: different literal members (`true | 2` -> `number | boolean`)
/// prove the rule is structural, not tied to the `1 | "x"` spelling.
#[test]
fn mixed_literal_kinds_widen_renamed() {
    let Some(dts) = emit_dts(
        "mixed_renamed",
        "declare const flag: boolean;\nexport const w = flag ? true : 2;\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const w: number | boolean;"),
        "fresh mixed-kind literal union must widen regardless of member spelling:\n{dts}"
    );
}

/// A union mixing a fresh literal with `null` widens only the literal member:
/// `1 | null` -> `number | null`.
#[test]
fn literal_with_null_widens_literal_keeps_null() {
    let Some(dts) = emit_dts(
        "null_branch",
        "declare const k: boolean;\nexport const v = k ? 1 : null;\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const v: number | null;"),
        "fresh literal-with-null union must widen only the literal member:\n{dts}"
    );
}

/// A union mixing a fresh literal with an object widens only the literal member:
/// `1 | { p: number }` -> `number | { p: number }`.
#[test]
fn literal_with_object_widens_literal_keeps_object() {
    let Some(dts) = emit_dts(
        "object_branch",
        "declare const k: boolean;\ndeclare const o: { p: number };\nexport const v = k ? 1 : o;\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("number") && dts.contains("p: number"),
        "fresh literal-with-object union must widen only the literal member:\n{dts}"
    );
    assert!(
        !dts.contains(": 1 |"),
        "the literal member must not survive un-widened:\n{dts}"
    );
}

/// The fresh-literal recursion descends through nested conditional branches:
/// `1 | (cond ? "a" : 2)` -> `string | number`.
#[test]
fn nested_conditional_branches_widen() {
    let Some(dts) = emit_dts(
        "nested",
        "declare const k: boolean;\nexport const v = k ? 1 : (k ? \"a\" : 2);\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const v: string | number;"),
        "nested fresh conditional literal union must widen:\n{dts}"
    );
}

// =============================================================================
// Unchanged behaviour: homogeneous collapse and `as const` preservation
// =============================================================================

/// Same-kind literal unions still collapse to the single primitive
/// (`1 | 2` -> `number`, `"a" | "b"` -> `string`).
#[test]
fn homogeneous_literal_union_still_collapses() {
    let Some(num) = emit_dts(
        "homo_num",
        "declare const k: boolean;\nexport const v = k ? 1 : 2;\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        num.contains("export declare const v: number;"),
        "homogeneous number-literal union must still collapse to number:\n{num}"
    );

    let Some(str_out) = emit_dts(
        "homo_str",
        "declare const k: boolean;\nexport const v = k ? \"a\" : \"b\";\n",
    ) else {
        return;
    };
    assert!(
        str_out.contains("export declare const v: string;"),
        "homogeneous string-literal union must still collapse to string:\n{str_out}"
    );
}

/// `as const` produces non-widening literal types: a heterogeneous `as const`
/// conditional union keeps its literal members instead of widening.
#[test]
fn as_const_conditional_union_is_not_widened() {
    let Some(dts) = emit_dts(
        "as_const",
        "declare const k: boolean;\nexport const v = k ? (1 as const) : (\"x\" as const);\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        !dts.contains("string | number"),
        "as const literal union must not widen to a primitive union:\n{dts}"
    );
    assert!(
        dts.contains('1') && dts.contains("\"x\""),
        "as const literal members must be preserved:\n{dts}"
    );
}
