//! DTS emit of the inferred yield type for unannotated generators, focused on
//! literal widening.
//!
//! tsc computes an unannotated generator's yield type as
//! `getWidenedType(getUnionType(<yielded operand types>))`: a *single* literal is
//! widened to its primitive base (`yield 1` -> `number`), but a multi-member
//! literal union is preserved (`yield 1; yield 2` -> `1 | 2`). tsz previously
//! widened every yield operand before the union, so a directly-`export`ed
//! generator — whose return type comes from the checker's computed signature —
//! collapsed `1 | 2` down to `number`. The fix keeps bare literal operands
//! unwidened until the union and widens the union only when it is a single
//! literal, matching tsc for declarations, methods, async generators, and the
//! directly-exported form. The rule keys on the yield operands' AST structure,
//! not the generator's name (verified with renamed binders).

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
        path.push(format!("tsz_yield_widening_dts_{name}_{nanos}"));
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
    emit_dts_with_lib(name, source, "es2015", "es6")
}

fn emit_dts_with_lib(name: &str, source: &str, target: &str, lib: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--target",
            target,
            "--lib",
            lib,
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

/// Primary repro: a directly-`export`ed generator that yields two distinct
/// number literals keeps the literal union `1 | 2` instead of widening to
/// `number`. The renamed binder proves the rule is not name-dependent.
#[test]
fn exported_generator_preserves_multi_literal_yield_union() {
    let Some(dts) = emit_dts(
        "multi_number",
        "export function* pair() { yield 1; yield 2; }\nexport function* renamed() { yield 1; yield 2; }\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("pair(): Generator<1 | 2, void, unknown>"),
        "multi-member literal yield union must be preserved, not widened to number:\n{dts}"
    );
    assert!(
        dts.contains("renamed(): Generator<1 | 2, void, unknown>"),
        "literal-union preservation must not depend on the generator name:\n{dts}"
    );
}

/// A mixed string/number literal union is preserved for a directly-exported
/// generator (`1 | "s"`).
#[test]
fn exported_generator_preserves_mixed_literal_yield_union() {
    let Some(dts) = emit_dts(
        "mixed_literal",
        "export function* mix() { yield 1; yield \"s\"; }\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("mix(): Generator<\"s\" | 1, void, unknown>"),
        "mixed literal yield union must be preserved (TS7 ranks string literals before number literals):\n{dts}"
    );
}

/// A single literal yield is still widened to its primitive base, matching tsc
/// (`getWidenedType` of a lone literal).
#[test]
fn exported_generator_widens_single_literal_yield() {
    let Some(dts) = emit_dts(
        "single",
        "export function* one() { yield 1; }\nexport function* dupe() { yield 1; yield 1; }\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("one(): Generator<number, void, unknown>"),
        "a single literal yield should widen to its base:\n{dts}"
    );
    assert!(
        dts.contains("dupe(): Generator<number, void, unknown>"),
        "repeated identical literals collapse to one and widen:\n{dts}"
    );
}

/// An async generator preserves its multi-literal yield union too.
#[test]
fn exported_async_generator_preserves_multi_literal_yield_union() {
    // `AsyncGenerator` lives in the es2018 lib.
    let Some(dts) = emit_dts_with_lib(
        "async_multi",
        "export async function* pair() { yield 1; yield 2; }\n",
        "es2018",
        "es2018",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("pair(): AsyncGenerator<1 | 2, void, unknown>"),
        "async generator multi-literal yield union must be preserved:\n{dts}"
    );
}

/// A class method generator (which routes through the body-driven emitter helper)
/// agrees with the directly-exported form.
#[test]
fn method_generator_preserves_multi_literal_yield_union() {
    let Some(dts) = emit_dts(
        "method",
        "export class C {\n  *m() { yield 1; yield 2; }\n}\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("m(): Generator<1 | 2, void, unknown>"),
        "class method generator must preserve the literal union:\n{dts}"
    );
}

/// Fresh object/array literal operands still widen their structure regardless of
/// union membership (`getWidenedType` reaches those leaves) — the fix must not
/// leak literal preservation into compound operands.
#[test]
fn exported_generator_widens_compound_literal_operands() {
    let Some(dts) = emit_dts(
        "compound",
        "export function* arr() { yield [1, 2]; yield [3, 4]; }\nexport function* obj() { yield { a: 1 }; }\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("arr(): Generator<number[], void, unknown>"),
        "fresh array literal operands must widen to number[]:\n{dts}"
    );
    assert!(
        dts.contains("a: number"),
        "fresh object literal operands must widen their property types:\n{dts}"
    );
}
