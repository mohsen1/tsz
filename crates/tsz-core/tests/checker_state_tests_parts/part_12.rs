/// Test that computed properties with identifier keys emit TS2564
#[test]
fn test_ts2564_computed_property_emits_error() {
    let source = r#"
const key1 = "computedKey";
class Foo {
    [key1]: number;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 for computed property without initialization
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for computed property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that computed properties initialized in constructor pass TS2564 check
#[test]
fn test_ts2564_computed_property_initialized_passes() {
    let source = r#"
const key2 = "initInConstructor";
class Foo {
    [key2]: number;
    constructor() {
        this[key2] = 42;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 for property initialized in constructor
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for initialized computed property, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_recursive_mapped_type_stack_guard() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
type Obj = { a: number };
declare let foo: Circular<Obj>;
foo.a;
"#;

    let (parser, root) = parse_test_source(source);
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
}

#[test]
fn test_recursive_mapped_type_list_widget_guard() {
    let source = r#"
type NonOptionalKeys<T> = { [P in keyof T]: undefined extends T[P] ? never : P }[keyof T];
type Child<T> = { [P in NonOptionalKeys<T>]: T[P] };

interface ListWidget {
    "type": "list",
    "minimum_count": number,
    "maximum_count": number,
    "collapsable"?: boolean,
    "each": Child<ListWidget>;
}

type ListChild = Child<ListWidget>;

declare let x: ListChild;
x.type;
"#;

    let (parser, root) = parse_test_source(source);
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
}

#[test]
fn test_abstract_constructor_type_parses() {
    // Test that abstract constructor types parse correctly (no TS1005/TS1109 errors)
    let source = r#"
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass) {
    return baseClass;
}

type AbstractConstructor<T> = abstract new (...args: any[]) => T;
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
export {};
declare global {
  var augmented: number;
}
augmented;
"#;

    let (parser, root) = parse_test_source(source);
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

    let source = r#"
namespace Utils { export const x = 1; }
namespace Utils { export const y = x; }
const z = Utils.y;
"#;

    let (parser, root) = parse_test_source(source);
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

    let source = r#"
declare module "pkg" {
  export const x: number;
}
declare module "pkg" {
  export const y: typeof x;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let source = r#"
// Direct circular reference - should emit TS2456
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
};

// Usage to trigger resolution
declare let x: Recurse;
"#;

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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
fn test_type_predicate_param_type_no_ts2304() {
    let source = r#"
class Wat {
    set p1(x: this is string) {}
    set p2(x: asserts this is string) {}
}
"#;

    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_type_predicate_return_no_ts2304() {
    let source = r#"
declare function isString(value: unknown): value is string;
declare function assertIsString(value: unknown): asserts value is string;
declare function assertDefined<T>(value: T): asserts value;
const assertFn: (value: unknown) => asserts value = value => {};
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for type predicate returns, got: {codes:?}"
    );
}

#[test]
fn test_type_predicate_this_return_no_ts2304() {
    let source = r#"
interface Foo {
    ok: boolean;
}

const obj = {
    m(): this is Foo {
        return this.ok;
    }
};
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for `this is` return type, got: {codes:?}"
    );
}

#[test]
fn test_exports_reference_no_ts2304() {
    let source = r#"
exports.foo = 1;
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for exports reference, got: {codes:?}"
    );
}

#[test]
fn test_mapped_type_param_no_ts2304() {
    let source = r#"
type Types = "boolean" | "string";
type Properties<T extends { [key: string]: Types }> = {
    readonly [key in keyof T]: T[key] extends "boolean" ? boolean : string
};
"#;

    let (parser, root) = parse_test_source(source);

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
        "Unexpected TS2304 for mapped type parameter, got: {codes:?}"
    );
}

#[test]
fn test_accessor_modifier_declaration_no_ts2304() {
    let source = r#"
interface I1 {
    accessor a: number;
}

accessor class C3 {}
accessor var V1: any;
accessor export default V1;
"#;

    let (parser, root) = parse_test_source(source);

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
        "Unexpected TS2304 for accessor modifier recovery, got: {codes:?}"
    );
}

#[test]
fn test_namespace_sibling_export_resolves() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
namespace Utils {
    export const x = 1;
}

namespace Utils {
    export const y = x;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for namespace sibling export, got: {codes:?}"
    );
}

#[test]
fn test_namespace_type_literal_resolves_members() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
namespace A {
    class Point { x: number = 0; y: number = 0; }
    export type Square = {
        top: { left: Point; right: Point };
        bottom: { left: Point; right: Point };
    };
}
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for namespace type literal members, got: {codes:?}"
    );
}

#[test]
fn test_namespace_type_query_resolves_alias() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
namespace A {
    export class Point {}
}

namespace C {
    import a = A;
    type AliasType = typeof a;
    type PointType = a.Point;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for namespace import alias type query, got: {codes:?}"
    );
}

#[test]
fn test_declare_global_merges_into_global_scope() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
export {};

declare global {
    interface GlobalThing { value: number; }
    var globalValue: GlobalThing;
}

const x = globalValue;
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for declare global, got: {codes:?}"
    );
}

#[test]
fn test_ambient_module_declaration_resolves_import() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare module "foo" {
    export interface Options { value: number; }
}

import { Options } from "foo";
const opts: Options = { value: 1 };
"#;

    let (parser, root) = parse_test_source(source);
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
        "Unexpected TS2304 for ambient module import, got: {codes:?}"
    );
}

#[test]
fn test_extends_expression_with_type_args_instantiates_base() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Base<T, U> {
    x: T;
    y: U;
}

interface BaseConstructor {
    new (x: string, y: string): Base<string, string>;
    new <T>(x: T): Base<T, T>;
    new <T>(x: T, y: T): Base<T, T>;
    new <T, U>(x: T, y: U): Base<T, U>;
}

declare function getBase(): BaseConstructor;

class D2 extends getBase() <number> {
    constructor() {
        super(10);
        super(10, 20);
        this.x = 1;
        this.y = 2;
    }
}

class D3 extends getBase() <string, number> {
    constructor() {
        super("abc", 42);
        this.x = "x";
        this.y = 2;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for extends instantiation expression, got: {codes:?}"
    );
}

#[test]
fn test_contextual_array_literal_uses_element_type() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Base { foo: string = ""; }
class Derived { foo: string = ""; bar: number = 0; }
class Derived2 extends Base { bar: string = ""; }

declare const d1: Derived;
declare const d2: Derived2;

const r: Base[] = [d1, d2];
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for contextual array literal, got: {codes:?}"
    );
}

#[test]
fn test_indexed_access_resolves_class_property_type() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class C {
    foo = 3;
    #bar = 3;
    constructor() {
        const ok: C["foo"] = 3;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for indexed access property type, got: {codes:?}"
    );
}

#[test]
fn test_static_private_fields_ignored_in_constructor_assignability() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class A {
    static #foo: number;
    static #bar: number;
}

const willErrorSomeDay: typeof A = class {};
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for typeof class assignment, got: {codes:?}"
    );
}

#[test]
fn test_assignment_expression_condition_narrows_discriminant() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
type D = { done: true, value: 1 } | { done: false, value: 2 };
declare function fn(): D;
let o: D;
if ((o = fn()).done) {
    const y: 1 = o.value;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for assignment expression narrowing, got: {codes:?}"
    );
}

/// Test destructuring assignment default value narrowing with complex patterns
#[test]
fn test_destructuring_assignment_default_order_narrows() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let a: 0 | 1 = 0;
let b: 0 | 1 | 9;
[{ [(a = 1)]: b } = [9, a] as const] = [];
const bb: 0 = b;
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for destructuring assignment, got: {codes:?}"
    );
}

#[test]
fn test_in_operator_const_name_narrows_union() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const a = "a";
type A = { a: number };
type B = { b: string };
declare const c: A | B;
if (a in c) {
    const x: number = c[a];
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for in-operator narrowing, got: {codes:?}"
    );
}

#[test]
fn test_instanceof_type_param_narrows_to_intersection() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class C { prop: string = ""; }
function f<T>(x: T) {
    if (x instanceof C) {
        const y: C = x;
        x.prop;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for instanceof narrowing, got: {codes:?}"
    );
}

#[test]
fn test_optional_chain_discriminant_narrows_union() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare const o: { x: 1, y: string } | { x: 2, y: number } | undefined;
if (o?.x === 1) {
    const x: 1 = o.x;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for optional-chain discriminant narrowing, got: {codes:?}"
    );
}

// =============================================================================
// TS2339 Inheritance Traversal Tests
// =============================================================================

#[test]
fn test_class_inheritance_property_access() {
    // Tests that accessing inherited instance properties doesn't produce TS2339
    let source = r#"
class Base {
    baseProp: number = 1;
}
class Derived extends Base {
    method() { return this.baseProp; }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&2339),
        "Should not emit TS2339 for inherited class property, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_mixin_inheritance_property_access() {
    // This test is related to test_abstract_mixin_intersection_ts2339 and requires
    // fixing type parameter scope handling for nested classes in generic functions.
    let source = r#"
interface Mixin {
    mixinMethod(): void;
}

function Mixin<TBaseClass extends abstract new (...args: any) => any>(
    baseClass: TBaseClass
): TBaseClass & (abstract new (...args: any) => Mixin) {
    abstract class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}

class Base {
    baseMethod() {}
}

class Derived extends Mixin(Base) {}

const d = new Derived();
d.baseMethod();
d.mixinMethod();
"#;

    let (parser, root) = parse_test_source(source);
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
    // Previously a known limitation, now resolved: mixin-based inheritance correctly
    // resolves intersection types, so no TS2339 is emitted.
    assert!(
        !codes.contains(&2339),
        "Mixin-based inheritance should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_mixin_return_type_preserves_base_properties() {
    let source = r#"
type Constructor<T> = new (...args: any[]) => T;

class Base {
    constructor(public x: number, public y: number) {}
}

const Printable = <T extends Constructor<Base>>(superClass: T) => class extends superClass {
    static message = "hello";
    print() {
        this.x;
    }
}

function Tagged<T extends Constructor<{}>>(superClass: T) {
    class C extends superClass {
        _tag: string;
        constructor(...args: any[]) {
            super(...args);
            this._tag = "hello";
        }
    }
    return C;
}

const Thing2 = Tagged(Printable(Base));
Thing2.message;

function f() {
    const thing = new Thing2(1, 2);
    thing.x;
    thing._tag;
    thing.print();
}

class Thing3 extends Thing2 {
    test() {
        this.print();
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
    // Previously a known limitation, now resolved: mixin constructor/instance property
    // resolution through generic class expressions works correctly.
    assert!(
        !codes.contains(&2339),
        "Mixin constructor/instance properties should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_class_extends_class_like_constructor_properties() {
    let source = r#"
interface Base<T, U> {
    x: T;
    y: U;
}

interface BaseConstructor {
    new (x: string, y: string): Base<string, string>;
    new <T>(x: T): Base<T, T>;
    new <T, U>(x: T, y: U): Base<T, U>;
}

declare function getBase(): BaseConstructor;

class D1 extends getBase() {
    constructor() {
        super("abc", "def");
        this.x;
        this.y;
    }
}

class D2 extends getBase() <number> {
    constructor() {
        super(10);
        super(10, 20);
        this.x;
        this.y;
    }
}

class D3 extends getBase() <string, number> {
    constructor() {
        super("abc", 42);
        this.x;
        this.y;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&2339),
        "Should not emit TS2339 for class-like constructor inheritance, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

