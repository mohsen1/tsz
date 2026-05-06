//! Regression tests for paren preservation when stripping type-only syntax.
//!
//! tsc collapses `(expr as T)` / `(expr satisfies T)` / `(<T>expr)` parens
//! around simple expressions but preserves parens around an instantiation
//! expression (`Box<number>`). When two layers of parens wrap the type-only
//! form, the rule applies independently:
//!
//! - `((10 satisfies number))` → `10`           (both parens stripped)
//! - `((expr as I)).foo`        → `expr.foo`     (both parens stripped)
//! - `((Box<number>)) instanceof Object`         → `((Box)) instanceof Object`
//!
//! The discriminator is the kind of the type-only inner node:
//!   - `TYPE_ASSERTION` / `AS_EXPRESSION` / `SATISFIES_EXPRESSION` collapse.
//!   - `EXPRESSION_WITH_TYPE_ARGUMENTS` (instantiation expression) preserves
//!     the outer paren.
//!
//! Source: `crates/tsz-emitter/src/emitter/expressions/core/helpers.rs`
//! (`emit_parenthesized` — the double-paren branch).

use tsz_common::common::ScriptTarget;
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn parse_lower_print(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn double_paren_around_satisfies_collapses_both_layers() {
    let source = "const a = ((/*comm*/ 10 satisfies number));\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        output.contains("const a = /*comm*/ 10;"),
        "Double parens around satisfies should collapse to bare expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const a = (/*comm*/ 10);"),
        "Outer paren around already-stripped inner paren must be removed.\nOutput:\n{output}"
    );
}

#[test]
fn double_paren_around_as_expression_at_access_position_collapses() {
    let source = "interface I { always(): void; }\nfunction g(result: unknown) {\n    if (((result as I)).always) {\n        return result;\n    }\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        output.contains("if (result.always)"),
        "Double parens around `expr as T` at access position should fully strip.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("if ((result).always)") && !output.contains("if (((result)).always)"),
        "No paren residue should be left around the access base.\nOutput:\n{output}"
    );
}

#[test]
fn double_paren_around_instantiation_expression_preserves_outer_paren() {
    let source = "declare class Box<T> { value: T; }\ndeclare const maybeBox: unknown;\n((Box<number>)) instanceof Object;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        output.contains("((Box)) instanceof Object"),
        "Double parens around an instantiation expression must keep both source parens.\nOutput:\n{output}"
    );
}
