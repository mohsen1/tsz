//! ES5 namespace emit: references inside a nested namespace to a `var`/`let`/
//! `const` exported by an *enclosing* namespace must be qualified by the
//! *declaring* namespace, not the innermost one (issue #14680).
//!
//! Full-pipeline (`Printer`) tests: the divergence only manifests when the
//! dispatch layer seeds `prior_exported_vars` from the enclosing namespace's
//! exported-variable set, which the nested-body recursion previously re-applied
//! under the inner namespace name.

use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_common::common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new_with_language_version(
        "test.ts".to_string(),
        source.to_string(),
        ScriptTarget::ES5,
    );
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            always_strict: true,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

/// w1: a nested-namespace function referencing an enclosing `const` export must
/// qualify it by the declaring namespace (`N.a`), never the inner one (`M.a`).
#[test]
fn nested_fn_reference_to_enclosing_const_qualified_by_declaring_namespace() {
    let output = emit_es5(
        "namespace N {\n  export const a = 1;\n  export namespace M { export function f() { return a; } }\n}",
    );
    assert!(
        output.contains("return N.a;"),
        "reference to enclosing export must be `N.a`. Got:\n{output}"
    );
    assert!(
        !output.contains("return M.a;"),
        "must not qualify enclosing export by the inner namespace. Got:\n{output}"
    );
}

/// w2: two nesting levels deep — `Outer.v`, not `Inner.v`.
#[test]
fn doubly_nested_fn_reference_to_enclosing_let_qualified_by_declaring_namespace() {
    let output = emit_es5(
        "namespace Outer {\n  export let v = 10;\n  export namespace Mid { export namespace Inner { export function read() { return v; } } }\n}",
    );
    assert!(
        output.contains("return Outer.v;"),
        "two-level nested reference must be `Outer.v`. Got:\n{output}"
    );
    assert!(
        !output.contains("Inner.v") && !output.contains("Mid.v"),
        "must not qualify by an inner namespace. Got:\n{output}"
    );
}

/// w3: nested+sibling exports — each reference qualifies by its declaring level
/// (`V.b = U.a + 1; W.c = U.a + V.b;`).
#[test]
fn nested_sibling_const_exports_qualify_by_declaring_level() {
    let output = emit_es5(
        "namespace U {\n  export const a = 1;\n  export namespace V { export const b = a + 1; export namespace W { export const c = a + b; } }\n}",
    );
    assert!(
        output.contains("V.b = U.a + 1;"),
        "`b`'s initializer must read `U.a`. Got:\n{output}"
    );
    assert!(
        output.contains("W.c = U.a + V.b;"),
        "`c`'s initializer must read `U.a` and `V.b`. Got:\n{output}"
    );
}

/// w4 (shadow): a nested namespace that re-declares the name locally as a
/// non-exported `var` shadows the enclosing export, so the reference stays
/// unqualified (bare `a`).
#[test]
fn nested_local_redeclaration_shadows_enclosing_export() {
    let output = emit_es5(
        "namespace N {\n  export const a = 1;\n  export namespace M { const a = 2; export function f() { return a; } }\n}",
    );
    assert!(
        output.contains("return a;"),
        "shadowed reference must stay bare `a`. Got:\n{output}"
    );
    assert!(
        !output.contains("return N.a;") && !output.contains("return M.a;"),
        "a locally shadowed name must not be qualified. Got:\n{output}"
    );
}

/// Renamed binders take the identical structural path (anti-hardcoding).
#[test]
fn nested_enclosing_var_qualification_is_binder_name_agnostic() {
    let output = emit_es5(
        "namespace Box {\n  export const seed = 42;\n  export namespace Lid { export function peek() { return seed; } }\n}",
    );
    assert!(
        output.contains("return Box.seed;"),
        "renamed binders must qualify by the declaring namespace. Got:\n{output}"
    );
    assert!(
        !output.contains("Lid.seed"),
        "renamed binders must not qualify by the inner namespace. Got:\n{output}"
    );
}
