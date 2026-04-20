//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[test]
fn test_abstract_constructor_type_parses() {
    use crate::parser::ParserState;

    // Test that abstract constructor types parse correctly (no TS1005/TS1109 errors)
    let source = r#"
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass) {
    return baseClass;
}

type AbstractConstructor<T> = abstract new (...args: any[]) => T;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Check for parser errors (TS1005 = ';' expected, TS1109 = Expression expected)
    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005 || d.code == 1109)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Should not have parse errors for abstract new syntax: {parse_errors:?}"
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
}

/// Test that unterminated template expressions produce TS1005 parse error.
/// Note: tsc does NOT report TS2304 for names inside unterminated templates —
/// only TS1005 for the missing '}'. We match that behavior.
#[test]
fn test_unterminated_template_expression_reports_parse_error() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = "var v = `foo ${ a ";

    let mut parser = ParserState::new("TemplateExpression1.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parse_codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        parse_codes.contains(&diagnostic_codes::EXPECTED),
        "Expected TS1005 for unterminated template expression, got: {parse_codes:?}"
    );
}

#[test]
fn test_global_augmentation_binds_to_file_scope() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
export {};
declare global {
  var augmented: number;
}
augmented;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for global augmentation: {codes:?}"
    );
}

#[test]
fn test_namespace_merging_resolves_prior_exports() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
namespace Utils { export const x = 1; }
namespace Utils { export const y = x; }
const z = Utils.y;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for merged namespace export lookup: {codes:?}"
    );
}

#[test]
fn test_module_augmentation_merges_exports() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
declare module "pkg" {
  export const x: number;
}
declare module "pkg" {
  export const y: typeof x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for module augmentation export lookup: {codes:?}"
    );
}

/// Test TS2456: Circular type alias detection
///
/// TODO: Circular type alias detection (TS2456) is not yet implemented.
/// TS2456 should fire for circular type alias references.
#[test]
fn test_circular_type_alias_ts2456() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
// Direct circular reference - should emit TS2456
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
};

// Usage to trigger resolution
declare let x: Recurse;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // TS2456 is now implemented - verify it fires for circular type alias
    let has_ts2456 = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == diagnostic_codes::TYPE_ALIAS_CIRCULARLY_REFERENCES_ITSELF);
    assert!(
        has_ts2456,
        "Expected TS2456 for circular type alias. Got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_builtin_type_references_only_emit_ts2304_for_missing_dom_globals() {
    // Regression test: Global types like Promise, Array, Map should not cause
    // TS2304 "Cannot find name" errors when lib.d.ts is not loaded.
    use crate::parser::ParserState;

    let source = r#"
// Type references with type arguments
declare const promise: Promise<string>;
declare const promiseLike: PromiseLike<number>;
declare const map: Map<string, number>;
declare const set: Set<string>;
declare const array: Array<number>;
declare const readonlyArray: ReadonlyArray<string>;
declare const partial: Partial<{x: number}>;
declare const required: Required<{x?: number}>;
declare const readonly: Readonly<{x: number}>;
declare const record: Record<string, number>;
declare const iterator: Iterator<number>;
declare const element: Element;
declare const htmlElement: HTMLElement;
declare const doc: Document;
declare const win: Window;
declare const event: Event;
declare const nodes: NodeList;
declare const date: Date;
declare const regex: RegExp;
declare const regexExec: RegExpExecArray;
declare const key: PropertyKey;
declare const desc: PropertyDescriptor;

type NN = NonNullable<string | null>;
type Ex = Extract<string | number, string>;
type Th = ThisType<{ x: number }>;

// Type alias with builtin generic
type MyPromise<T> = Promise<T>;
declare const myPromise: MyPromise<boolean>;

// typeof with global constructor
declare const PromiseConstructor: typeof Promise;
declare const ArrayConstructor: typeof Array;
declare const MapConstructor: typeof Map;

// Interface extending builtin
interface MyError extends Error {
    customField: string;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2304_messages: Vec<String> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| d.message_text.clone())
        .collect();

    let missing_dom_globals = [
        "Element",
        "HTMLElement",
        "Document",
        "Window",
        "Event",
        "NodeList",
    ];
    for name in missing_dom_globals {
        assert!(
            ts2304_messages
                .iter()
                .any(|message| message.contains(&format!("'{name}'"))),
            "expected TS2304 for missing DOM global {name}, got: {ts2304_messages:?}"
        );
    }

    let builtin_non_dom_types = [
        "Promise",
        "PromiseLike",
        "Map",
        "Set",
        "Array",
        "ReadonlyArray",
        "Partial",
        "Required",
        "Readonly",
        "Record",
        "Iterator",
        "Date",
        "RegExp",
        "RegExpExecArray",
        "PropertyKey",
        "PropertyDescriptor",
        "NonNullable",
        "Extract",
        "ThisType",
        "Error",
    ];
    for name in builtin_non_dom_types {
        assert!(
            !ts2304_messages
                .iter()
                .any(|message| message.contains(&format!("'{name}'"))),
            "did not expect TS2304 for builtin lib type {name}, got: {ts2304_messages:?}"
        );
    }

    assert!(
        ts2304_messages.len() == 6,
        "expected TS2304 only for missing DOM globals, got: {ts2304_messages:?}"
    );
}

#[test]
fn test_builtin_types_in_type_literal_only_emit_ts2304_for_missing_dom_globals() {
    // Ensure true lib types still resolve in type literals while missing DOM globals
    // continue to route through plain TS2304.
    use crate::parser::ParserState;

    let source = r#"
type Box<T> = { value: T };
type Foo = {
  promise: Promise<string>;
  map: Map<string, number>;
  list: ReadonlyArray<number>;
  partial: Partial<{ x: number }>;
  node: NodeList;
  doc: Document;
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2304_messages: Vec<String> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| d.message_text.clone())
        .collect();

    assert!(
        ts2304_messages
            .iter()
            .any(|message| message.contains("'NodeList'")),
        "expected TS2304 for missing NodeList, got: {ts2304_messages:?}"
    );
    assert!(
        ts2304_messages
            .iter()
            .any(|message| message.contains("'Document'")),
        "expected TS2304 for missing Document, got: {ts2304_messages:?}"
    );
    for name in ["Promise", "Map", "ReadonlyArray", "Partial"] {
        assert!(
            !ts2304_messages
                .iter()
                .any(|message| message.contains(&format!("'{name}'"))),
            "did not expect TS2304 for builtin type literal member {name}, got: {ts2304_messages:?}"
        );
    }
    assert!(
        ts2304_messages.len() == 2,
        "expected only missing DOM globals to produce TS2304 in type literals, got: {ts2304_messages:?}"
    );
}

#[test]
fn test_switch_case_param_reference_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
function area(s: { kind: "square"; size: number } | { kind: "circle"; radius: number }) {
    switch (s.kind) {
        case "square":
            return s.size * s.size;
        case "circle":
            return s.radius * s.radius;
        default:
            return 0;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for switch case param references, got: {codes:?}"
    );
}

#[test]
#[ignore = "checker emits spurious TS2304 for type predicates in setter params — needs type_node_resolution fix"]
fn test_type_predicate_param_type_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
class Wat {
    set p1(x: this is string) {}
    set p2(x: asserts this is string) {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    // Parser correctly rejects type predicates in setter parameter position
    // (same as tsc which emits TS1005), so we only check that the checker
    // doesn't add a spurious TS2304 on top of the parse errors.

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for type predicate parameter types, got: {codes:?}"
    );
}

