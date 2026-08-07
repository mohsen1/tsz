//! DTS emit for `yield*` (delegating yield) in unannotated generators.
//!
//! tsc infers an unannotated generator's yield type from the values it yields,
//! including the *element* type of any iterable delegated to with `yield*`:
//! `function* g() { yield* [1, 2]; }` emits `Generator<number, void, unknown>`.
//!
//! tsz's declaration emitter previously bailed out the moment its body-driven
//! yield-type inference saw a `yield*` expression and fell back to
//! `Generator<any, void, unknown>`.  The fix routes the delegated operand
//! through the solver's iterator protocol so the element type participates in
//! that inference.  Resolution covers operands whose iterator info is available
//! structurally (arrays and tuples); other operands keep the conservative `any`
//! fallback rather than emitting a wrong type.
//!
//! These tests exercise the body-driven inference path — generator methods and
//! locally-declared generators surfaced through a re-export.  (Directly-exported
//! top-level `export function*` declarations are emitted through a separate path
//! that does not yet run body-driven yield inference even for plain `yield`; that
//! pre-existing gap is tracked separately.)

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
        path.push(format!("tsz_yield_star_dts_{name}_{nanos}"));
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
    emit_dts_with_args(name, source, &["--target", "es2015", "--lib", "es6"])
}

/// Emit with the default (es2018) lib so `AsyncGenerator` and a bare `Generator`
/// delegate resolve. Used by the delegated-`TNext` pins, whose delegate carries
/// `TNext = any` from the lib default — a shape the es6-only harness cannot name.
fn emit_dts_default_lib(name: &str, source: &str) -> Option<String> {
    emit_dts_with_args(name, source, &["--target", "es2018"])
}

fn emit_dts_with_args(name: &str, source: &str, extra_args: &[&str]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let mut args = vec!["repro.ts", "--declaration", "--emitDeclarationOnly"];
    args.extend_from_slice(extra_args);
    args.extend_from_slice(&["--pretty", "false"]);

    let output = Command::new(tsz_bin)
        .args(&args)
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

/// Primary repro: delegating to an array literal contributes the array element
/// type, not `any`.  Adjacent case: a differently-named generator proves the
/// rule is not binder-name dependent.
#[test]
fn yield_star_array_delegation_infers_element_type() {
    let Some(dts) = emit_dts(
        "array",
        r#"
function* numbers() { yield* [1, 2]; }
function* renamed() { yield* [10, 20, 30]; }
export { numbers, renamed };
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("numbers(): Generator<number, void, unknown>"),
        "yield* array element type should drive the inferred yield type:\n{dts}"
    );
    assert!(
        dts.contains("renamed(): Generator<number, void, unknown>"),
        "yield* inference must not depend on the generator name:\n{dts}"
    );
    assert!(
        !dts.contains("Generator<any"),
        "yield* delegation must not fall back to Generator<any>:\n{dts}"
    );
}

/// A plain `yield` and a `yield*` producing the same element type collapse to
/// that single yield type, matching tsc.
#[test]
fn yield_star_mixed_with_plain_yield_unifies_to_shared_type() {
    let Some(dts) = emit_dts(
        "mixed",
        "function* gen() { yield 1; yield* [2, 3]; }\nexport { gen };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("gen(): Generator<number, void, unknown>"),
        "mixed yield / yield* with matching element types should infer number:\n{dts}"
    );
}

/// Delegating to a tuple yields the union of its element types.
#[test]
fn yield_star_tuple_delegation_unions_element_types() {
    let Some(dts) = emit_dts(
        "tuple",
        "function* gen() { yield* [1, \"x\"] as [number, string]; }\nexport { gen };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("gen(): Generator<string | number, void, unknown>")
            || dts.contains("gen(): Generator<number | string, void, unknown>"),
        "yield* over a tuple should union element types:\n{dts}"
    );
}

/// Async generators delegating to an array infer `AsyncGenerator<element>`.
#[test]
fn async_yield_star_array_delegation_infers_element_type() {
    let Some(dts) = emit_dts(
        "async",
        "async function* gen() { yield* [1, 2]; }\nexport { gen };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("gen(): AsyncGenerator<number, void, unknown>"),
        "async yield* array element type should drive the inferred yield type:\n{dts}"
    );
}

/// Generator *method* declarations (here on an exported class) share the same
/// body-driven inference path.
#[test]
fn yield_star_array_delegation_in_method_infers_element_type() {
    let Some(dts) = emit_dts(
        "method",
        r#"
export class C {
    *gen() { yield* [1, 2]; }
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("gen(): Generator<number, void, unknown>"),
        "generator method yield* delegation should infer the element type:\n{dts}"
    );
}

/// Regression guard: a `yield*` whose element type is not structurally
/// resolvable keeps the conservative `Generator<any, …>` fallback rather than
/// emitting a wrong type or dropping the generator return type entirely.
#[test]
fn yield_star_unresolvable_operand_keeps_any_fallback() {
    let Some(dts) = emit_dts(
        "fallback",
        "function* gen(x: Iterable<unknown>) { yield* x; }\nexport { gen };\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    // The exact element type is not resolved structurally here; the emitter must
    // still produce a Generator return type (conservatively `any`), never panic
    // or drop the annotation.
    assert!(
        dts.contains("gen(x: Iterable<unknown>): Generator<"),
        "unresolvable yield* should still emit a Generator return type:\n{dts}"
    );
}

/// Delegating to a bare `Generator` (`Generator<unknown, any, any>`) contributes
/// the delegate's own `TNext` — `any` — to the enclosing generator's inferred
/// `TNext`. `tsc` renders a *computed* generator return type with all three type
/// arguments and never drops a trailing one that equals its default, so the
/// delegated `any` survives:
///
/// ```ts
/// declare function src(): Generator;
/// function* relay() { yield* src(); }
/// // tsc: relay(): Generator<unknown, void, any>
/// ```
///
/// tsz previously trimmed the trailing `any` because it equaled the lib default,
/// emitting `Generator<unknown, void>` — the divergence #15632 tracks. The
/// declaration function, the function expression, and a renamed binder all share
/// the same body-driven inference, so all three must keep the third argument.
#[test]
fn yield_star_bare_generator_delegation_keeps_any_next_type() {
    let Some(dts) = emit_dts_default_lib(
        "bare_next",
        r#"
declare function src(): Generator;
function* relay() { yield* src(); }
const relayExpr = function* () { yield* src(); };
function* zzz() { yield* src(); }
export { relay, relayExpr, zzz };
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("relay(): Generator<unknown, void, any>"),
        "a bare-`Generator` delegate contributes `TNext = any`; the trailing \
         argument must not be trimmed against the lib default:\n{dts}"
    );
    assert!(
        dts.contains("relayExpr: () => Generator<unknown, void, any>"),
        "the function-expression path shares the same delegated-`TNext`:\n{dts}"
    );
    assert!(
        dts.contains("zzz(): Generator<unknown, void, any>"),
        "the delegated-`TNext` rendering must not depend on the binder name:\n{dts}"
    );
    assert!(
        !dts.contains("Generator<unknown, void>"),
        "the trailing `any` must not be elided as a default:\n{dts}"
    );
}

/// The async arm carries the same delegated `TNext = any` into `AsyncGenerator`.
#[test]
fn async_yield_star_bare_generator_delegation_keeps_any_next_type() {
    let Some(dts) = emit_dts_default_lib(
        "bare_next_async",
        r#"
declare function asrc(): AsyncGenerator;
async function* arelay() { yield* asrc(); }
export { arelay };
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("arelay(): AsyncGenerator<unknown, void, any>"),
        "async delegation to a bare `AsyncGenerator` must keep the delegated \
         `TNext = any`:\n{dts}"
    );
}

/// The same all-arguments rule beyond generators: a *computed* re-export of a
/// value whose type instantiates a generic with trailing arguments equal to
/// their defaults keeps every argument, matching `tsc`
/// (`const r = w` where `w: Foo<boolean, string>` emits
/// `Foo<boolean, string, number>`, not `Foo<boolean>`). This guards the printer
/// change from being narrowed back to a generator-only special case.
#[test]
fn computed_reexport_keeps_all_default_equal_type_arguments() {
    let Some(dts) = emit_dts_default_lib(
        "computed_defaults",
        r#"
interface Foo<A, B = string, C = number> { a: A; b: B; c: C; }
declare const w: Foo<boolean, string>;
const r = w;
export { r };
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("r: Foo<boolean, string, number>"),
        "a computed reference renders its full argument list, including trailing \
         arguments that equal their defaults:\n{dts}"
    );
}
