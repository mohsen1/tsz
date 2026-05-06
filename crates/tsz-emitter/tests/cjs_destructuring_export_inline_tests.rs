//! Regression test for inline `exports.X = X;` emission after a
//! destructuring `const`/`let`/`var` declaration whose names appear in a
//! later `export { ... }` clause.
//!
//! `get_declaration_export_names` was extracting names only from
//! identifier-shaped binding names, so `const [a, , b] = [1, 2, 3];
//! export { a, b };` emitted the destructuring but dropped both
//! `exports.a = a;` and `exports.b = b;`. Use the existing
//! `collect_binding_names` helper so destructuring patterns yield every
//! bound name.
//!
//! Source: `crates/tsz-emitter/src/emitter/source_file/const_enums.rs`
//! (`collect_variable_names_with_initializers`).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn cjs_inline_export_handles_array_destructuring_binding() {
    let source = "const [a, , b] = [1, 2, 3];\nexport { a, b };\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("const [a, , b] = [1, 2, 3];"),
        "destructuring declaration should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.a = a;"),
        "Inline `exports.a = a;` must follow a destructuring declaration that binds `a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.b = b;"),
        "Inline `exports.b = b;` must follow a destructuring declaration that binds `b`.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_inline_export_handles_object_destructuring_binding() {
    let source = "const { x, y } = obj as { x: number; y: number };\nexport { x, y };\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("exports.x = x;"),
        "Inline `exports.x = x;` must follow an object-destructuring declaration that binds `x`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.y = y;"),
        "Inline `exports.y = y;` must follow an object-destructuring declaration that binds `y`.\nOutput:\n{output}"
    );
}
