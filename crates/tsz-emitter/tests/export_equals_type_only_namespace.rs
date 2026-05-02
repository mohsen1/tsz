//! Regression tests for `export = X` where `X` is a type-only namespace.
//!
//! TypeScript erases `export = X;` when `X` is a non-instantiated namespace
//! (containing only `interface`, `type`, etc.). For CommonJS output, tsc
//! emits the `__esModule` marker but no `module.exports = X;` line because
//! there is no JS binding for `X` at runtime.
//!
//! Before the fix in `export_assignment_identifier_is_type_only`, the
//! emitter only set `matched_runtime` for instantiated namespaces and never
//! set `matched_type` for type-only ones. The function returned
//! `matched_type && !matched_runtime = false && true = false`, so the
//! caller treated `export = X` as a runtime export and emitted
//! `module.exports = X;`.
//!
//! Mirrors the conformance test
//! `tests/cases/compiler/exportNamespaceDeclarationRetainsVisibility.ts`.

use tsz_common::common::ModuleKind;
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print_with_opts;

fn cjs_opts() -> PrintOptions {
    PrintOptions {
        module: ModuleKind::CommonJS,
        ..PrintOptions::default()
    }
}

#[test]
fn export_equals_type_only_namespace_emits_only_es_module_marker() {
    let source = r#"namespace X {
    interface A {
        kind: 'a';
    }

    interface B {
        kind: 'b';
    }

    export type C = A | B;
}

export = X;
"#;

    let output = parse_and_print_with_opts(source, cjs_opts());

    assert!(
        output.contains("Object.defineProperty(exports, \"__esModule\""),
        "expected __esModule marker for CJS file with type-only export=, got:\n{output}"
    );
    assert!(
        !output.contains("module.exports = X"),
        "type-only namespace must not be assigned to module.exports, got:\n{output}"
    );
}

#[test]
fn export_equals_instantiated_namespace_still_emits_module_exports() {
    // Sanity: namespaces with values still produce module.exports = X.
    let source = r#"namespace X {
    export const value = 1;
}

export = X;
"#;

    let output = parse_and_print_with_opts(source, cjs_opts());

    assert!(
        output.contains("module.exports = X"),
        "instantiated namespace must still be exported, got:\n{output}"
    );
}

#[test]
fn export_equals_declare_namespace_does_not_emit_module_exports() {
    // `declare namespace X` that contains only type-level members (interfaces,
    // type aliases) is non-instantiated at the JS level and must not produce
    // `module.exports = X`. tsc erases such export-equals and emits the
    // `__esModule` marker only.
    //
    // Note: `declare namespace X { const value: number; }` IS considered
    // instantiated by tsc (VariableStatement → Instantiated in
    // getModuleInstanceStateWorker) and DOES emit `module.exports = X`.
    // Only namespaces whose bodies contain exclusively type-only declarations
    // (interface, type alias, const enum without preserveConstEnums, etc.)
    // are non-instantiated.
    let source = r#"declare namespace X {
    interface I {
        x: number;
    }
    type T = string;
}

export = X;
"#;

    let output = parse_and_print_with_opts(source, cjs_opts());

    assert!(
        !output.contains("module.exports = X"),
        "declare namespace with only type members must not appear in module.exports, got:\n{output}"
    );
}

#[test]
fn export_equals_declare_namespace_with_value_members_emits_module_exports() {
    let source = r#"declare namespace X {
    export var value: string;
    export function read(): string;
}

export = X;
"#;

    let output = parse_and_print_with_opts(source, cjs_opts());

    assert!(
        output.contains("module.exports = X"),
        "declare namespace with ambient value members must be assigned to module.exports, got:\n{output}"
    );
}

#[test]
fn export_equals_namespace_with_only_interfaces_is_type_only() {
    // Pure interface-only namespace.
    let source = r#"namespace X {
    export interface I {
        x: number;
    }
}

export = X;
"#;

    let output = parse_and_print_with_opts(source, cjs_opts());

    assert!(
        !output.contains("module.exports = X"),
        "interface-only namespace must not be runtime-exported, got:\n{output}"
    );
}
