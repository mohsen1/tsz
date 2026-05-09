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
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

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

/// Regression for `declarationEmitSimpleComputedNames1`: a computed property
/// name on an *object-literal* method is a runtime expression and must pick
/// up the inline `exports.X` rewrite for CJS-exported names. The same
/// rewrite must NOT apply to class member names — those are key declarations
/// and stay as the bare identifier.
#[test]
fn cjs_inline_export_rewrites_computed_method_name_in_object_literal() {
    let source = "export const fieldName = \"f1\";\nexport const c = {\n    [fieldName]() { return \"r\"; }\n};\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("[exports.fieldName]"),
        "Computed method-name inside an object literal must qualify `fieldName` to `exports.fieldName`.\nOutput:\n{output}"
    );
}

/// Same behavior for a different binding name — guards against accidental
/// hardcoding of the identifier `fieldName` while fixing the original bug.
#[test]
fn cjs_inline_export_rewrites_computed_property_name_with_alternate_identifier() {
    let source = "export const k = \"a\";\nexport const o = {\n    [k]: 1\n};\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("[exports.k]"),
        "Computed property-name inside an object literal must qualify `k` to `exports.k`.\nOutput:\n{output}"
    );
}
