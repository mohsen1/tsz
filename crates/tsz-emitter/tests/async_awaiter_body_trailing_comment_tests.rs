//! Regression tests: same-line trailing comments on statements inside a
//! downleveled async body (the `__awaiter(this, ..., function* () { ... })`
//! wrapper) must be preserved, matching tsc.
//!
//! Rule: when an async function/method/arrow body is lowered into an
//! `__awaiter` generator wrapper (target ES2015+ with a module transform),
//! every wrapped body statement that carries a same-line trailing comment in
//! source keeps that comment, exactly as the canonical block emit loop does.
//! Previously the awaiter-body loops emitted statements naively
//! (`emit(stmt); write_line();`) and dropped the trailing comments.
//!
//! These assertions intentionally vary the comment text and the wrapped
//! statement kind so they assert the structural behavior, not a single
//! fingerprint.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn emit_es2015_commonjs(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        remove_comments: false,
        ..PrintOptions::default()
    };
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn async_function_body_statement_keeps_same_line_trailing_comment() {
    let output = emit_es2015_commonjs(
        "export async function fn() {\n    const req = await Promise.resolve(1); // ALPHA\n}\n",
    );
    assert!(
        output.contains("function* ()"),
        "Async function should downlevel to an __awaiter generator wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("// ALPHA"),
        "Trailing comment on the wrapped body statement must be preserved.\nOutput:\n{output}"
    );
    // The comment must stay on the same line as the lowered statement, not
    // leak onto its own line.
    assert!(
        output
            .lines()
            .any(|line| line.contains("yield") && line.contains("// ALPHA")),
        "Trailing comment must stay on the lowered yield statement line.\nOutput:\n{output}"
    );
}

#[test]
fn async_method_body_statement_keeps_same_line_trailing_comment() {
    let output = emit_es2015_commonjs(
        "export class C {\n    async m() {\n        let v = await Promise.resolve(2); // BETA\n    }\n}\n",
    );
    assert!(
        output.contains("// BETA"),
        "Trailing comment on an async method body statement must be preserved.\nOutput:\n{output}"
    );
    assert!(
        output
            .lines()
            .any(|line| line.contains("yield") && line.contains("// BETA")),
        "Async method trailing comment must stay on the lowered statement line.\nOutput:\n{output}"
    );
}

#[test]
fn async_arrow_body_statement_keeps_same_line_trailing_comment() {
    // Top-level async arrow path (no `this` capture).
    let output = emit_es2015_commonjs(
        "export const run = async () => {\n    const n = await Promise.resolve(3); // GAMMA\n};\n",
    );
    assert!(
        output.contains("// GAMMA"),
        "Trailing comment on an async arrow body statement must be preserved.\nOutput:\n{output}"
    );
    assert!(
        output
            .lines()
            .any(|line| line.contains("yield") && line.contains("// GAMMA")),
        "Async arrow trailing comment must stay on the lowered statement line.\nOutput:\n{output}"
    );
}

#[test]
fn async_body_preserves_trailing_comments_on_multiple_statement_kinds() {
    // Vary statement kind (expression statement, variable statement, return)
    // and comment text so the test exercises the rule, not one spelling.
    let output = emit_es2015_commonjs(
        "export async function multi() {\n    await Promise.resolve(); // FIRST\n    const x = 1; // SECOND\n    return x; // THIRD\n}\n",
    );
    for marker in ["// FIRST", "// SECOND", "// THIRD"] {
        assert!(
            output.contains(marker),
            "Trailing comment {marker} must be preserved on its body statement.\nOutput:\n{output}"
        );
    }
}
