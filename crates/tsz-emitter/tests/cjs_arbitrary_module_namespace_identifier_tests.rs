//! Regression tests for CJS export emission with *arbitrary module namespace
//! identifiers* — string-literal export names like `"<X>"` that aren't valid
//! JS identifiers and must use bracket access at the runtime assignment.
//!
//! tsc emits `exports["<X>"] = someValue;` for `export { someValue as "<X>" }`
//! both in the void-0 preamble and in the inline `exports.X = X;` assignment
//! that follows the local declaration. The inline-after-declaration path in
//! `source_file::emit` was bypassing the bracket-access fallback and emitting
//! `exports.<X> = someValue;` — a syntax error that broke 8 of the 14
//! `arbitraryModuleNamespaceIdentifiers_module` module-variant baselines.
//!
//! Source: `crates/tsz-emitter/src/emitter/source_file/emit.rs`
//! (the `cjs_deferred_export_bindings` loop that emits inline exports
//! immediately after `const`/`let`/`var`/`class` declarations).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn cjs_inline_export_uses_bracket_access_for_non_identifier_export_name() {
    let source = "const someValue = \"someValue\";\nexport { someValue as \"<X>\" };\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("exports[\"<X>\"] = someValue;"),
        "Inline `exports.X = X;` must use bracket access for non-identifier export names.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.<X>"),
        "`exports.<X>` is a syntax error — must never appear.\nOutput:\n{output}"
    );
    // Sanity: void-0 preamble already used bracket access.
    assert!(
        output.contains("exports[\"<X>\"] = void 0;"),
        "void-0 preamble must keep bracket access for non-identifier export names.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_inline_export_keeps_dot_access_for_plain_identifier_export_name() {
    let source = "const someValue = \"someValue\";\nexport { someValue as renamed };\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("exports.renamed = someValue;"),
        "Plain identifier export names should keep dot access.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports[\"renamed\"]"),
        "Bracket access must not be used for valid-identifier export names.\nOutput:\n{output}"
    );
}
