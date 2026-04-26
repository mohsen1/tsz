//! Integration tests for duplicate `extends` clause emit error recovery.
//!
//! When the parser encounters a class with a duplicate `extends` keyword
//! (e.g. `class D extends C extends C { ... }`), tsc reports TS1172 but
//! still preserves both heritage clauses in the AST so JS emit prints the
//! source verbatim (`class D extends C extends C { ... }`). The parser
//! ships both clauses to the emitter so this matches tsc's
//! `extendsClauseAlreadySeen` and `extendsClauseAlreadySeen2` baselines.
//!
//! See:
//! - `crates/tsz-parser/src/parser/state_statements_class.rs`
//!   (`parse_heritage_clause_extends` duplicate-keyword recovery)
//! - `crates/tsz-emitter/src/emitter/declarations/class/emit_es6.rs`
//!   (heritage-clause emission loop)
//! - `TypeScript/tests/baselines/reference/extendsClauseAlreadySeen.js`
//! - `TypeScript/tests/baselines/reference/extendsClauseAlreadySeen2.js`

use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

/// Source `class D extends C extends C { baz() { } }` (TypeScript test
/// `extendsClauseAlreadySeen.ts`) must emit both `extends C extends C`
/// clauses verbatim. The duplicate-clause TS1172 diagnostic is parser-side
/// and does not affect the emitted JS.
#[test]
fn class_duplicate_extends_emits_both_clauses_verbatim() {
    let source = "class C {\n}\nclass D extends C extends C {\n    baz() { }\n}\n";
    let output = print_es2015(source);
    assert!(
        output.contains("class D extends C extends C"),
        "expected duplicate `extends C extends C` to round-trip; got:\n{output}"
    );
}

/// `class D extends A extends B { ... }` must emit `class D extends A extends B`
/// even when the duplicate clause references a different base type. Each
/// duplicate `extends` keyword introduces its own heritage clause node so the
/// emitter prints them in source order.
#[test]
fn class_duplicate_extends_distinct_bases_emits_both_clauses() {
    let source = "class A {}\nclass B {}\nclass D extends A extends B { baz() {} }\n";
    let output = print_es2015(source);
    assert!(
        output.contains("class D extends A extends B"),
        "expected `extends A extends B` to round-trip; got:\n{output}"
    );
}

/// Duplicate `implements` clauses do NOT appear in JS output (interfaces are
/// type-only). Confirm we still strip `implements` cleanly even though
/// duplicate `extends` is now preserved.
#[test]
fn class_duplicate_implements_does_not_appear_in_js() {
    let source = "class C {\n}\nclass D implements C implements C {\n    baz() { }\n}\n";
    let output = print_es2015(source);
    assert!(
        !output.contains("implements"),
        "implements clauses must not appear in JS emit; got:\n{output}"
    );
    assert!(
        output.contains("class D"),
        "expected `class D` to be emitted; got:\n{output}"
    );
}
