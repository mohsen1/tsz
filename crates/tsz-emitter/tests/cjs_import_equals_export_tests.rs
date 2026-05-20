//! Regression tests for CJS inline export of namespace-alias `import = ` aliases.
//!
//! When `import a = M.x;` is followed by `export { a };` (or any export clause
//! that includes `a`), tsc lowers the alias as `var a = M.x;` and emits the
//! inline `exports.a = a;` immediately after — the same treatment as a regular
//! `var a = ...;` declaration.
//!
//! tsz was missing the `IMPORT_EQUALS_DECLARATION` arm in
//! `get_declaration_export_names`, so the inline-after-declaration emission
//! path skipped the alias. The deferred-export bookkeeping had already removed
//! `a` from the iteration of the export clause (expecting the inline path to
//! handle it), and the result was that `exports.a = a;` was emitted from
//! neither path — the alias was declared but never exported.
//!
//! Source: `crates/tsz-emitter/src/emitter/source_file/const_enums.rs`
//! (`get_declaration_export_names` — the `IMPORT_EQUALS_DECLARATION` arm).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

#[test]
fn cjs_inline_export_emits_assignment_for_namespace_alias_import_equals() {
    let source = "namespace M { export var x; }\nimport a = M.x;\nexport { a };\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("var a = M.x;"),
        "import-equals to namespace value should lower to var.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.a = a;"),
        "Inline `exports.a = a;` should follow the namespace-alias declaration.\nOutput:\n{output}"
    );
}

/// Counter-regression: type-only import-equals chains (e.g. `import b = a.I`
/// where `I` is an exported interface) must not produce a runtime export at
/// all. The `IMPORT_EQUALS_DECLARATION` arm only emits when the alias actually
/// has runtime value — `import_decl_has_runtime_value` is false for
/// type-only chains, so `get_declaration_export_names` returns empty and no
/// `exports.b = b;` is added.
#[test]
fn cjs_inline_export_skips_type_only_import_equals() {
    let source =
        "namespace a { export interface I {} }\nexport import b = a.I;\nexport var x: b;\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        !output.contains("exports.b = "),
        "Type-only import-equals alias should not produce a runtime export.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_export_import_equals_missing_trailing_entity_identifier_emits_assignment() {
    let source = "export import x = N.A.\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("exports.x = N.A.;"),
        "Exported recovered import-equals entity name should still emit the CJS export assignment.\nOutput:\n{output}"
    );
}
