//! Regression tests for `export import X = NS;` under ES-module output.
//!
//! For an `export import` alias to a local namespace (or nested namespace
//! member / namespaced class) under ES-module output
//! (`--module es2015|es2020|es2022|esnext`), tsz used to emit a syntactically
//! invalid `export ;` statement instead of `export var X = NS;`. The `export`
//! keyword sits on the outer `EXPORT_DECLARATION`, not on the inner
//! import-equals clause, so forcing `force_exported = false` let the
//! namespace-alias elision gate drop the assignment while the caller had
//! already written a bare `export `.
//!
//! The fix routes the ES-module path through
//! `emit_exported_import_equals_declaration` (the same handler the CommonJS
//! path already uses), so the clause is treated as exported and emits its own
//! `export var X = NS;` prefix.
//!
//! Source: `crates/tsz-emitter/src/emitter/module_emission/core/mod.rs`
//! (`emit_export_declaration_es6` — the `IMPORT_EQUALS_DECLARATION` arm).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as parse_lower_emit;

fn esnext_opts() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2020,
        module: ModuleKind::ESNext,
        ..Default::default()
    }
}

#[test]
fn esm_export_import_alias_to_namespace_emits_export_var() {
    let source = "namespace A { export const x = 1; }\nexport import AA = A;\n";
    let output = parse_lower_emit(source, esnext_opts());

    assert!(
        output.contains("export var AA = A;"),
        "exported namespace alias should emit `export var AA = A;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export ;"),
        "must not strand a bare `export ;`.\nOutput:\n{output}"
    );
}

#[test]
fn esm_export_import_alias_to_nested_namespace_member_emits_export_var() {
    let source =
        "namespace A { export namespace B { export const x = 1; } }\nexport import AB = A.B;\n";
    let output = parse_lower_emit(source, esnext_opts());

    assert!(
        output.contains("export var AB = A.B;"),
        "exported nested-namespace alias should emit `export var AB = A.B;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export ;"),
        "must not strand a bare `export ;`.\nOutput:\n{output}"
    );
}

#[test]
fn esm_export_import_alias_to_namespaced_class_emits_export_var() {
    let source = "namespace A { export class K {} }\nexport import AK = A.K;\n";
    let output = parse_lower_emit(source, esnext_opts());

    assert!(
        output.contains("export var AK = A.K;"),
        "exported namespaced-class alias should emit `export var AK = A.K;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export ;"),
        "must not strand a bare `export ;`.\nOutput:\n{output}"
    );
}

/// Non-exported `import X = A;` under ES module is already correct and must
/// stay a plain `var` (no `export`).
#[test]
fn esm_non_exported_import_alias_stays_plain_var() {
    let source = "namespace A { export const x = 1; }\nimport AA = A;\nAA;\n";
    let output = parse_lower_emit(source, esnext_opts());

    assert!(
        output.contains("var AA = A;"),
        "non-exported alias should stay `var AA = A;`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export var AA"),
        "non-exported alias must not become an export.\nOutput:\n{output}"
    );
}

/// An exported alias whose qualified target resolves to an *exported*
/// interface (type-only) has no runtime value — tsc emits neither the alias
/// nor a stray `export ;`.
#[test]
fn esm_export_import_alias_to_exported_interface_elides_cleanly() {
    let source = "namespace A { export interface I {} }\nexport import AI = A.I;\n";
    let output = parse_lower_emit(source, esnext_opts());

    assert!(
        !output.contains("export var AI"),
        "type-only alias must not emit a runtime export.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export ;"),
        "type-only alias must not strand a bare `export ;`.\nOutput:\n{output}"
    );
}
