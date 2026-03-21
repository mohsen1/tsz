//! Regression tests for stable-identity DefId helpers.
//!
//! These tests verify that the refactored stable-identity helpers
//! (`resolve_def_id_for_lowering`, `ensure_def_id_with_alias`,
//! `resolve_symbol_as_lazy_type`) produce correct behavior for:
//! - Generic type references
//! - Namespace-qualified type references
//! - Class/type query lowering
//! - Recursive type aliases

use tsz_binder::BinderState;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// =========================================================================
// Generic type reference resolution via stable-identity helpers
// =========================================================================

#[test]
fn generic_type_ref_without_args_no_false_error() {
    // A generic interface used without type arguments (when all params have defaults)
    // should not produce spurious errors after the stable-identity refactoring.
    let source = r#"
interface Box<T = any> {
    value: T;
}
const b: Box = { value: 42 };
"#;
    let diags = check_and_get_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|(code, _)| *code != 0).collect();
    assert!(
        errors.is_empty(),
        "Expected no errors for generic with defaults, got: {errors:?}"
    );
}

#[test]
fn generic_type_ref_with_args_correct_checking() {
    // Generic type with explicit args should still produce correct TS2322
    let source = r#"
interface Container<T> {
    value: T;
}
const c: Container<number> = { value: "string" };
"#;
    let diags = check_and_get_diagnostics(source);
    let has_ts2322 = diags.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for string→number assignment, got: {diags:?}"
    );
}

#[test]
fn recursive_type_alias_no_stack_overflow() {
    // Recursive type aliases resolved via ensure_def_id_with_alias
    // should not cause infinite loops or stack overflows.
    let source = r#"
type JsonValue = string | number | boolean | null | JsonArray | JsonObject;
type JsonArray = JsonValue[];
interface JsonObject {
    [key: string]: JsonValue;
}
const x: JsonValue = [1, "hello", [true, null]];
"#;
    let diags = check_and_get_diagnostics(source);
    // Should not panic or produce TS2589 (excessive depth)
    let has_ts2589 = diags.iter().any(|(code, _)| *code == 2589);
    assert!(
        !has_ts2589,
        "Should not produce TS2589 for valid recursive type: {diags:?}"
    );
}

// =========================================================================
// Namespace-qualified type references
// =========================================================================

#[test]
fn namespace_qualified_type_ref_resolves() {
    // Qualified name resolution uses ensure_def_id_with_alias internally.
    // Verify it still works correctly after refactoring.
    let source = r#"
namespace Animals {
    export interface Cat {
        name: string;
        meow(): void;
    }
}
const cat: Animals.Cat = { name: "Whiskers", meow() {} };
"#;
    let diags = check_and_get_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|(code, _)| *code != 0).collect();
    assert!(
        errors.is_empty(),
        "Expected no errors for namespace-qualified type, got: {errors:?}"
    );
}

#[test]
fn namespace_qualified_generic_type_ref() {
    // Qualified generic type reference should work via the refactored helpers.
    let source = r#"
namespace Collections {
    export interface List<T> {
        items: T[];
        add(item: T): void;
    }
}
const list: Collections.List<number> = {
    items: [1, 2, 3],
    add(item: number) {}
};
"#;
    let diags = check_and_get_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|(code, _)| *code != 0).collect();
    assert!(
        errors.is_empty(),
        "Expected no errors for namespace-qualified generic type, got: {errors:?}"
    );
}

// =========================================================================
// Class return type and Lazy→TypeQuery conversion
// =========================================================================

#[test]
fn class_self_reference_in_static_method() {
    // The resolve_lazy_class_to_constructor path (using def_to_symbol_id_with_fallback)
    // should correctly convert Lazy(DefId) to TypeQuery for class self-references.
    let source = r#"
class Factory {
    value: number;
    constructor(v: number) { this.value = v; }
    static create(): Factory {
        return new Factory(42);
    }
}
const f: Factory = Factory.create();
"#;
    let diags = check_and_get_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|(code, _)| *code != 0).collect();
    assert!(
        errors.is_empty(),
        "Expected no errors for class static factory, got: {errors:?}"
    );
}

// =========================================================================
// Type literal resolution with Lazy(DefId)
// =========================================================================

#[test]
fn type_literal_with_lazy_ref_no_false_ts2339() {
    // Type literals that reference external types via resolve_symbol_as_lazy_type
    // should not produce false TS2339 errors.
    let source = r#"
interface Point { x: number; y: number; }
type Shape = {
    origin: Point;
    area(): number;
};
declare const s: Shape;
const x: number = s.origin.x;
"#;
    let diags = check_and_get_diagnostics(source);
    let has_ts2339 = diags.iter().any(|(code, _)| *code == 2339);
    assert!(
        !has_ts2339,
        "Expected no TS2339 for type literal member access, got: {diags:?}"
    );
}

#[test]
fn type_literal_generic_ref_in_namespace() {
    // Type literals inside namespaces using resolve_symbol_as_lazy_type
    // should resolve correctly.
    let source = r#"
namespace NS {
    export interface Wrapper<T> { value: T; }
    export type Config = {
        data: Wrapper<string>;
    };
}
declare const cfg: NS.Config;
const v: string = cfg.data.value;
"#;
    let diags = check_and_get_diagnostics(source);
    let has_ts2339 = diags.iter().any(|(code, _)| *code == 2339);
    assert!(
        !has_ts2339,
        "Expected no TS2339 for namespace type literal generic ref, got: {diags:?}"
    );
}

// =========================================================================
// Mapped type template resolution
// =========================================================================

#[test]
fn mapped_type_with_alias_body_resolves() {
    // Mapped types reference type aliases via the def_id_resolver closure.
    // After refactoring to resolve_def_id_for_lowering, this should still work.
    let source = r#"
type Readonly2<T> = {
    readonly [P in keyof T]: T[P];
};
interface Original {
    name: string;
    age: number;
}
const ro: Readonly2<Original> = { name: "test", age: 5 };
"#;
    let diags = check_and_get_diagnostics(source);
    let errors: Vec<_> = diags.iter().filter(|(code, _)| *code != 0).collect();
    assert!(
        errors.is_empty(),
        "Expected no errors for mapped type alias, got: {errors:?}"
    );
}
