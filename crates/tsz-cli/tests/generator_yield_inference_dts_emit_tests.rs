//! Declaration-emit inference of an *unannotated* generator's yield type.
//!
//! tsc computes the inferred yield type as
//! `getWidenedType(getUnionType(<yield operand types>))`. The union regularizes
//! the operand literal types, so a union of two or more **distinct** literals is
//! preserved verbatim (`yield "x"; yield "y"` -> `Generator<"x" | "y">`), while a
//! **single** fresh literal widens to its base (`yield "x"` -> `Generator<string>`).
//!
//! tsz previously widened **each** yield operand individually and could only keep
//! one unique yield type, so it over-widened `yield "x"; yield "y"` to
//! `Generator<string>` and collapsed mixed operands (`yield "x"; yield 1`) all the
//! way to `Generator<any>`. These guards pin the corrected behavior against
//! `tsc` 6.0.2.
//!
//! Every case uses a distinct generator/binder name so the assertions track the
//! structural rule, not any identifier (anti-hardcoding gate).

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
        path.push(format!("tsz_yield_infer_dts_{name}_{nanos}"));
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
            "--target",
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

/// A union of two distinct string literals is preserved, not widened to `string`.
#[test]
fn distinct_string_literal_yields_preserve_the_union() {
    let Some(dts) = emit_dts(
        "distinct_string",
        "function* greetings() { yield \"hi\"; yield \"bye\"; }\nexport { greetings };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("greetings(): Generator<\"hi\" | \"bye\", void, unknown>"),
        "two distinct string-literal yields must stay a literal union:\n{dts}"
    );
}

/// A union of two distinct number literals is preserved, not widened to `number`.
#[test]
fn distinct_number_literal_yields_preserve_the_union() {
    let Some(dts) = emit_dts(
        "distinct_number",
        "function* counts() { yield 1; yield 2; }\nexport { counts };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("counts(): Generator<1 | 2, void, unknown>"),
        "two distinct number-literal yields must stay a literal union:\n{dts}"
    );
}

/// Mixed-kind literal operands must not collapse to `any` (the prior bug) nor
/// widen to `string | number` — both literals survive.
#[test]
fn mixed_kind_literal_yields_do_not_collapse_to_any() {
    let Some(dts) = emit_dts(
        "mixed_kind",
        "function* feed() { yield \"go\"; yield 7; }\nexport { feed };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        !dts.contains("Generator<any"),
        "mixed-kind literal yields must not fall back to Generator<any>:\n{dts}"
    );
    assert!(
        dts.contains("\"go\"") && dts.contains('7'),
        "both literal operands must survive the union:\n{dts}"
    );
    assert!(
        !dts.contains("Generator<string")
            && !dts.contains("string | number")
            && !dts.contains("number | string"),
        "mixed-kind literals must stay literals, not widen to primitives:\n{dts}"
    );
}

/// A single fresh string-literal yield widens to its base type.
#[test]
fn single_string_literal_yield_widens_to_base() {
    let Some(dts) = emit_dts(
        "single_string",
        "function* once() { yield \"only\"; }\nexport { once };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("once(): Generator<string, void, unknown>"),
        "a single fresh literal yield widens to its base:\n{dts}"
    );
}

/// Repeated identical literals collapse to one value and then widen, exactly
/// like a single yield.
#[test]
fn repeated_identical_literal_yields_widen_to_base() {
    let Some(dts) = emit_dts(
        "repeated",
        "function* twice() { yield 5; yield 5; }\nexport { twice };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("twice(): Generator<number, void, unknown>"),
        "repeated identical literals dedup to one fresh value and widen:\n{dts}"
    );
}

/// A `const`-asserted literal operand is non-fresh and keeps its literal type
/// even when it is the only yield.
#[test]
fn const_asserted_literal_yield_is_preserved() {
    let Some(dts) = emit_dts(
        "const_assert",
        "function* tagged() { yield \"kind\" as const; }\nexport { tagged };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("tagged(): Generator<\"kind\", void, unknown>"),
        "a const-asserted literal yield keeps its literal type:\n{dts}"
    );
}

/// A literal subsumed by a non-literal operand reduces away (`"x" | string` ->
/// `string`).
#[test]
fn literal_subsumed_by_primitive_operand_reduces() {
    let Some(dts) = emit_dts(
        "subsumed",
        "function* mix(label: string) { yield \"first\"; yield label; }\nexport { mix };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("mix(label: string): Generator<string, void, unknown>"),
        "a string-literal subsumed by `string` reduces to `string`:\n{dts}"
    );
}

/// A bare `yield;` contributes `undefined`; mixed with a literal it stays a
/// literal-plus-`undefined` union rather than widening the literal.
#[test]
fn bare_yield_mixed_with_literal_keeps_literal() {
    let Some(dts) = emit_dts(
        "bare_mixed",
        "function* maybe() { yield \"value\"; yield; }\nexport { maybe };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("\"value\"") && dts.contains("undefined"),
        "a literal mixed with a bare `yield;` keeps the literal and undefined:\n{dts}"
    );
    assert!(
        !dts.contains("Generator<string"),
        "the literal must not widen when mixed with a bare yield:\n{dts}"
    );
}

/// An empty generator (no `yield` at all) infers a `never` yield type.
#[test]
fn empty_generator_infers_never_yield() {
    let Some(dts) = emit_dts("empty", "function* idle() {}\nexport { idle };\n") else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("idle(): Generator<never, void, unknown>"),
        "an empty generator yields `never`:\n{dts}"
    );
}

/// A fresh object-literal operand widens its member positions.
#[test]
fn object_literal_yield_widens_members() {
    let Some(dts) = emit_dts(
        "object",
        "function* records() { yield { id: 1 }; }\nexport { records };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("id: number"),
        "a fresh object-literal yield widens its members:\n{dts}"
    );
    assert!(
        !dts.contains("id: 1"),
        "the object member literal must widen, not stay `1`:\n{dts}"
    );
}

/// Async generators follow the same rule and keep the distinct-literal union.
#[test]
fn async_generator_preserves_distinct_literal_union() {
    let Some(dts) = emit_dts(
        "async",
        "async function* stream() { yield \"a\"; yield \"b\"; }\nexport { stream };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("stream(): AsyncGenerator<\"a\" | \"b\", void, unknown>"),
        "an async generator keeps the distinct-literal yield union:\n{dts}"
    );
}
