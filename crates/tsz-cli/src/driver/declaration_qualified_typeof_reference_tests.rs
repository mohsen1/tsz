//! `.d.ts` emit coverage for `typeof` of a **qualified** value reference.
//!
//! When an inferred type position (a function return, here) resolves to a
//! value-space symbol, tsc's declaration emitter spells it `typeof Name`
//! rather than expanding the callable's structural shape. This held for bare
//! identifiers (`typeof bf`) and for a namespace **class** (`typeof Q.QC`) but
//! not for the *callable* qualified shapes — a static method (`M.sm`), a
//! namespace function (`P.pf`), or a nested-namespace function (`R.T.tf`) were
//! structurally expanded instead (issue #17281).
//!
//! The discriminator is the resolved symbol's kind and its reachability from
//! module scope (`value_reference_symbol_can_use_typeof`), not whether the
//! reference is dotted — so the qualified path must admit `FUNCTION`/`METHOD`
//! exactly as the bare path does, while a value-scope-local target still
//! expands. No emit-corpus row exercises a dotted `typeof` of a callable, so
//! this coverage is hand-written.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;

/// Compile a single-file source with declaration-only emit and return the text
/// of the emitted `dist/decl.d.ts`.
fn emit_declaration(source: &str) -> String {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "dist",
    "strict": false
  },
  "files": ["decl.ts"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("decl.ts"), source).expect("write source");

    let args = CliArgs::try_parse_from(["tsz", "-p", "tsconfig.json", "--pretty", "false"])
        .expect("parse args");
    let result = compile(&args, dir.path()).expect("compile succeeds");
    assert!(
        result.diagnostics.iter().all(|d| d.code != 5069),
        "declaration prerequisites should be satisfied, got: {:?}",
        result.diagnostics
    );
    fs::read_to_string(dir.path().join("dist/decl.d.ts")).expect("emitted decl.d.ts")
}

/// A static method referenced through its class name emits `typeof M.sm`, not
/// the expanded call signature.
#[test]
fn static_method_qualified_reference_emits_typeof() {
    let dts = emit_declaration(
        "export class M { static sm(x: number) { return x; } }\n\
         export function h2() { return M.sm; }\n",
    );
    assert!(
        dts.contains("function h2(): typeof M.sm;"),
        "static method should be spelled `typeof M.sm`, got:\n{dts}"
    );
}

/// A namespace function reference emits `typeof P.pf` (and does not restate the
/// overload set structurally).
#[test]
fn namespace_function_qualified_reference_emits_typeof() {
    let dts = emit_declaration(
        "export namespace P {\n\
         export function pf(x: number): number;\n\
         export function pf(x: string): string;\n\
         export function pf(x: any): any { return x; }\n\
         }\n\
         export function h() { return P.pf; }\n",
    );
    assert!(
        dts.contains("function h(): typeof P.pf;"),
        "namespace function should be spelled `typeof P.pf`, got:\n{dts}"
    );
}

/// A function nested two namespaces deep still resolves to a dotted `typeof`.
#[test]
fn nested_namespace_function_qualified_reference_emits_typeof() {
    let dts = emit_declaration(
        "export namespace R { export namespace T { export function tf(x: number) { return x; } } }\n\
         export function h3() { return R.T.tf; }\n",
    );
    assert!(
        dts.contains("function h3(): typeof R.T.tf;"),
        "nested-namespace function should be spelled `typeof R.T.tf`, got:\n{dts}"
    );
}

/// The pre-existing namespace-class case is unchanged.
#[test]
fn namespace_class_qualified_reference_still_emits_typeof() {
    let dts = emit_declaration(
        "export namespace Q { export class QC {} }\n\
         export function h4() { return Q.QC; }\n",
    );
    assert!(
        dts.contains("function h4(): typeof Q.QC;"),
        "namespace class should stay `typeof Q.QC`, got:\n{dts}"
    );
}

/// Anti-hardcoding: the rule is symbol-kind + scope, not the specific binder
/// names — renaming every identifier keeps the dotted `typeof`.
#[test]
fn qualified_typeof_is_binder_name_independent() {
    let dts = emit_declaration(
        "export class Widget { static build(n: number) { return n; } }\n\
         export function makeBuilder() { return Widget.build; }\n",
    );
    assert!(
        dts.contains("function makeBuilder(): typeof Widget.build;"),
        "renamed static method should be spelled `typeof Widget.build`, got:\n{dts}"
    );
}

/// Negative control: a static method of a class declared **inside a function
/// body** is not reachable from module scope, so its name cannot be spelled —
/// tsc expands the signature, and so must tsz.
#[test]
fn value_scope_local_static_method_expands_structurally() {
    let dts = emit_declaration(
        "export function outer() {\n\
         class Local { static sm(x: number) { return x; } }\n\
         return Local.sm;\n\
         }\n",
    );
    assert!(
        dts.contains("function outer(): (x: number) => number;"),
        "value-scope-local static method must expand structurally, got:\n{dts}"
    );
    assert!(
        !dts.contains("typeof"),
        "no `typeof` reference should survive for a function-local class, got:\n{dts}"
    );
}

/// Negative control: reading a method off an **instance value** is not a
/// value-space reference to the method symbol, so it expands (matching tsc).
#[test]
fn instance_value_method_access_expands_structurally() {
    let dts = emit_declaration(
        "class C { m(x: number) { return x; } }\n\
         declare const c: C;\n\
         export function g1() { return c.m; }\n",
    );
    assert!(
        dts.contains("function g1(): (x: number) => number;"),
        "instance-value method access must expand structurally, got:\n{dts}"
    );
}
