use crate::context::CheckerOptions;
use crate::query_boundaries::assignability::{
    AssignabilityQueryInputs, is_assignable_with_overrides, is_fresh_subtype_of,
};
use crate::query_boundaries::common::{
    TypeInterner, function_shape_for_type, object_shape_for_type,
};
use crate::state::{CheckerOverrideProvider, CheckerState};
use crate::test_utils::{check_js_source_diagnostics, check_source};
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

fn diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

#[test]
fn typed_array_cross_assignment_preserves_generic_display() {
    let diagnostics = diagnostics_for(
        r#"
interface ArrayBuffer {}
interface ArrayBufferLike {}
interface Int8Array<TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
    readonly tag: "Int8Array";
}
interface Uint8Array<TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
    readonly tag: "Uint8Array";
}
declare var Int8Array: {
    new (length: number): Int8Array<ArrayBuffer>;
};
declare var Uint8Array: {
    new (length: number): Uint8Array<ArrayBuffer>;
};

let arr_Int8Array = new Int8Array(1);
let arr_Uint8Array = new Uint8Array(1);

arr_Int8Array = arr_Uint8Array;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for incompatible typed array assignment");
    assert!(
        diag.message_text.contains(
            "Type 'Uint8Array<ArrayBuffer>' is not assignable to type 'Int8Array<ArrayBuffer>'."
        ),
        "typed array TS2322 should preserve generic type arguments, got: {diag:?}"
    );
}

#[test]
fn homomorphic_remap_missing_property_uses_specialized_source_display() {
    let diagnostics = diagnostics_for(
        r#"
type Exclude<T, U> = T extends U ? never : T;

interface Box<T> {
    length: number;
    find(value: T): T;
    end(value: T): T;
}

declare let tgt2: Box<number>;
declare let src2: { [K in keyof Box<number> as Exclude<K, "length">]: Box<number>[K] };

tgt2 = src2;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing 'length'");
    assert!(
        diag.message_text
            .contains("find: (value: number) => number")
            && diag.message_text.contains("end: (value: number) => number"),
        "TS2741 should use the specialized mapped source display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("value: T"),
        "TS2741 should not leak unspecialized Array<T> members into the source display, got: {diag:?}"
    );
}

fn strict_diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    )
}

fn function_shapes_for_named_bindings(
    source: &str,
    names: &[&str],
) -> Vec<Option<tsz_solver::FunctionShape>> {
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
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    names
        .iter()
        .map(|name| {
            binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .and_then(|type_id| function_shape_for_type(checker.ctx.types, type_id))
                .map(|shape| shape.as_ref().clone())
        })
        .collect()
}

fn normalized_function_shapes_for_named_bindings(
    source: &str,
    names: &[&str],
) -> Vec<Option<tsz_solver::FunctionShape>> {
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
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    names
        .iter()
        .map(|name| {
            binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .map(|type_id| checker.evaluate_type_for_assignability(type_id))
                .and_then(|type_id| function_shape_for_type(checker.ctx.types, type_id))
                .map(|shape| shape.as_ref().clone())
        })
        .collect()
}

fn normalized_type_kinds_for_named_bindings(source: &str, names: &[&str]) -> Vec<&'static str> {
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
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    names
        .iter()
        .map(|name| {
            let type_id = binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .map(|type_id| checker.evaluate_type_for_assignability(type_id))
                .expect("expected binding type");
            if function_shape_for_type(checker.ctx.types, type_id).is_some() {
                "Function"
            } else if object_shape_for_type(checker.ctx.types, type_id).is_some() {
                "Object"
            } else {
                "Other"
            }
        })
        .collect()
}

#[test]
fn conditional_type_intersection_assignment_ts2322() {
    // tsc emits TS2322 for both assignments because Something<A> contains
    // a deferred conditional type in its intersection.
    let source = r#"
            type Something<T> = { test: string } & (T extends object ? {
                arg: T
            } : {
                arg?: undefined
            });

            function testFunc2<A extends object>(a: A, sa: Something<A>) {
                sa = { test: 'hi', arg: a };
                sa = { test: 'bye', arg: a, arr: a };
            }
        "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322_count >= 2,
        "expected at least 2 TS2322 for assigning to intersection with deferred conditional, got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn constructor_accessibility_assignment_error_targets_lhs() {
    let source = r#"
            class Foo {
                constructor(public x: number) {}
            }
            class Bar {
                protected constructor(public x: number) {}
            }
            let a = Foo;
            a = Bar;
        "#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    let expected_start = source.find("a = Bar").expect("expected assignment span") as u32;

    assert_eq!(
        diag.start, expected_start,
        "TS2322 should be anchored to LHS"
    );
    assert_eq!(
        diag.length, 1,
        "TS2322 should target only the assignment target"
    );
}

#[test]
fn variable_declaration_initializer_ts2322_anchors_decl_name() {
    let source = r#"
let value: string = 42;
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    let value_start = source.find("value").expect("expected declaration name") as u32;
    assert_eq!(
        diag.start, value_start,
        "TS2322 should anchor at the variable declaration name"
    );
    assert_eq!(
        diag.length, 5,
        "TS2322 should cover only the declaration name"
    );
}

#[test]
fn nested_object_literal_excess_property_anchors_offending_property() {
    let source = r#"
type Inner = { ok: number };
type Outer = { inner: Inner };
const value: Outer = { inner: { ok: 1, nope: 2 } };
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2353)
        .expect("expected TS2353");

    let nope_start = source.find("nope").expect("expected excess property") as u32;
    assert_eq!(
        diag.start, nope_start,
        "TS2353 should anchor at the offending nested property name"
    );
    assert_eq!(
        diag.length, 4,
        "TS2353 should cover only the offending property"
    );
}

#[test]
fn commonjs_module_exports_assignment_does_not_contextually_type_rhs_object_literal() {
    let diagnostics = check_js_source_diagnostics(
        r#"
/** @typedef {{ id: string, label: string, traceEventNames: string[] }} TaskGroup */

/** @type {Object<string, TaskGroup>} */
const taskNameToGroup = {};

module.exports = {
    taskNameToGroup,
};
"#,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2353),
        "module.exports object literals in JS should not pick up contextual TS2353 diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn js_mapped_type_object_literal_accepts_finite_literal_keys() {
    let diagnostics = check_js_source_diagnostics(
        r#"
/** @typedef {'parseHTML'|'styleLayout'} TaskGroupIds */

/**
 * @type {{[P in TaskGroupIds]: {id: P, label: string}}}
 */
const taskGroups = {
    parseHTML: {
        id: 'parseHTML',
        label: 'Parse HTML & CSS'
    },
    styleLayout: {
        id: 'styleLayout',
        label: 'Style & Layout'
    },
};
"#,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2353),
        "finite mapped-type keys in JS object literals should not be treated as excess properties, got: {diagnostics:?}"
    );
}

#[test]
fn contextual_assignment_conditional_callback_branches_keep_parameter_context() {
    let diagnostics = diagnostics_for(
        r#"
declare const cond: boolean;
type Handler = { cb: (value: string) => string };
let handler!: Handler;
handler = cond
    ? { cb: value => value }
    : { cb: value => value };
"#,
    );

    let ts7006: Vec<_> = diagnostics.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected conditional assignment contextual retry to preserve callback parameter types, got: {diagnostics:?}"
    );
}

#[test]
fn destructuring_assignment_contextually_types_literal_rhs_for_ts2488() {
    let diagnostics = diagnostics_for(
        r#"
var a: string, b: boolean[];
[a, ...b] = { 0: "", 1: true };
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2488)
        .expect("expected TS2488 for non-iterable destructuring assignment");

    assert!(
        diag.message_text
            .contains("Type '{ 0: string; 1: true; }' must have a '[Symbol.iterator]()' method that returns an iterator."),
        "Expected TS2488 to preserve the partially contextualized RHS shape, got: {diag:?}"
    );
}

/// TS2488 in variable declarations widens boolean literals, matching tsc.
/// Regression test for conformance/es6/destructuring/iterableArrayPattern21.ts
#[test]
fn destructuring_declaration_widens_boolean_literals_for_ts2488() {
    let diagnostics = diagnostics_for(
        r#"
// @target: es2015
var [a, b] = { 0: "", 1: true };
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2488)
        .expect("expected TS2488 for non-iterable destructuring declaration");

    assert!(
        diag.message_text.contains("1: boolean"),
        "Expected TS2488 to widen boolean in var declaration, got: {:?}",
        diag.message_text
    );
}

/// TS2488 in assignment expressions preserves boolean literals, matching tsc.
/// Regression test for conformance/es6/destructuring/iterableArrayPattern23.ts
#[test]
fn destructuring_assignment_preserves_boolean_literals_for_ts2488() {
    let diagnostics = diagnostics_for(
        r#"
// @target: es2015
var a: string, b: boolean;
[a, b] = { 0: "", 1: true };
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2488)
        .expect("expected TS2488 for non-iterable destructuring assignment");

    assert!(
        diag.message_text.contains("1: true"),
        "Expected TS2488 to preserve boolean literal 'true' in assignment, got: {:?}",
        diag.message_text
    );
}

#[test]
fn for_of_object_destructuring_default_reports_leaf_mismatch() {
    let source = r#"
// @target: ES6
var x: string, y: number;
var array = [{ x: "", y: true }]
enum E { x }
for ({x, y = E.x} of array) {
    x;
    y;
}
"#;

    let diagnostics = diagnostics_for(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {diagnostics:?}"
    );

    let diag = ts2322[0];
    let y_start = source.find("y = E.x").expect("expected defaulted binding") as u32;
    assert_eq!(
        diag.start, y_start,
        "TS2322 should anchor at the defaulted binding name, not the whole pattern"
    );
    assert_eq!(
        diag.length, 1,
        "TS2322 should cover only the binding name token"
    );
    assert!(
        diag.message_text
            .contains("Type 'boolean' is not assignable to type 'number'"),
        "TS2322 should report the source property mismatch, got: {diag:?}"
    );
}

#[test]
fn nested_object_literal_assignability_keeps_exact_property_anchor() {
    let source = r#"
type Inner = { ok: string };
type Outer = { inner: Inner };
const value: Outer = { inner: { ok: 42 } };
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    let ok_start = source.rfind("ok").expect("expected nested property name") as u32;
    assert_eq!(
        diag.start, ok_start,
        "TS2322 should stay anchored at the offending nested property name"
    );
    assert_eq!(
        diag.length, 2,
        "TS2322 should cover only the offending property token"
    );
}

#[test]
fn discriminated_union_object_literal_reports_matching_member_property_mismatch() {
    let source = r#"
type A = {
    type: 'a',
    data: { a: string }
};

type B = {
    type: 'b',
    data: null
};

type C = {
    type: 'c',
    payload: string
};

type Union = A | B | C;

const foo: Union = {
    type: 'a',
    data: null
};
"#;

    let diagnostics = strict_diagnostics_for(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {diagnostics:?}"
    );

    let diag = ts2322[0];
    let data_start = source.rfind("data: null").expect("expected data property") as u32;
    assert_eq!(
        diag.start, data_start,
        "TS2322 should anchor at the discriminant-selected property, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("Type 'null' is not assignable to type '{ a: string; }'."),
        "TS2322 should report the matching union member property mismatch, got: {diag:?}"
    );
}

#[test]
fn numeric_property_assignment_reports_nested_value_mismatch() {
    let source = r#"
interface A {
    0: string;
}
var x: A = {
    0: 3
};
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    let prop_start = source.rfind("0: 3").expect("expected numeric property") as u32;
    assert_eq!(
        diag.start, prop_start,
        "TS2322 should anchor at the offending numeric property token"
    );
    assert_eq!(
        diag.length, 1,
        "TS2322 should cover only the numeric property token"
    );
    assert!(
        diag.message_text
            .contains("Type 'number' is not assignable to type 'string'"),
        "TS2322 should report the nested numeric property mismatch, got: {diag:?}"
    );
}

#[test]
fn missing_property_message_uses_contextual_function_parameter_types() {
    let source = r#"
let value: {
    f(n: number): number;
    g(s: string): number;
    m: number;
} = {
    f: (n) => 0,
    g: (s) => 0,
};
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2741)
        .expect("expected TS2741");

    assert!(
        diag.message_text
            .contains("{ f: (n: number) => number; g: (s: string) => number; }"),
        "TS2741 should preserve contextual function parameter types in the source display, got: {diag:?}"
    );
}

#[test]
fn mapped_enum_key_missing_property_uses_enum_member_display() {
    let diagnostics = diagnostics_for(
        r#"
type Record<K extends string | number, T> = { [P in K]: T };
enum E { A }
let foo: Record<E, any> = {};
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing enum mapped key");
    assert!(
        diag.message_text.contains("Property '[E.A]' is missing"),
        "TS2741 should render the enum member key, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Property '0' is missing"),
        "TS2741 should not render the erased numeric key, got: {diag:?}"
    );
}

#[test]
fn function_expression_assignment_reports_outer_signature_mismatch() {
    let source = r#"
interface T {
    (x: number): string;
}
declare let t: T;
t = (x: number) => 1;
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322 && d.message_text.contains("Type '(x: number) => number'"))
        .expect("expected outer TS2322");

    let assignment_start = source.find("t = (x: number)").expect("expected assignment") as u32;
    assert_eq!(
        diag.start, assignment_start,
        "TS2322 should anchor at the assignment target for function-expression signature mismatches"
    );
    assert!(
        diag.message_text.contains("Type '(x: number) => number'"),
        "TS2322 should report the outer signature mismatch, got: {diag:?}"
    );
    assert!(
        !diag
            .message_text
            .contains("Type 'number' is not assignable to type 'string'"),
        "The outer TS2322 should not collapse to the nested return-type leaf, got: {diag:?}"
    );
}

#[test]
fn tuple_arity_assignment_reports_outer_tuple_mismatch() {
    let source = r#"
let tup: [number, number, number] = [1, 2, 3, "string"];
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    let tup_start = source.find("tup").expect("expected variable name") as u32;
    assert_eq!(
        diag.start, tup_start,
        "TS2322 should anchor at the variable name for tuple arity mismatch"
    );
    assert!(
        diag.message_text.contains(
            "Type '[number, number, number, string]' is not assignable to type '[number, number, number]'"
        ),
        "TS2322 should report the outer tuple assignment mismatch, got: {diag:?}"
    );
    assert!(
        !diag
            .message_text
            .contains("Type 'string' is not assignable to type 'number'"),
        "tuple arity mismatch should not collapse to the extra element mismatch, got: {diag:?}"
    );
}

#[test]
fn generic_default_initializer_widens_numeric_literals() {
    let source = r#"
function foo3<T extends Number>(x: T = 1) { }
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    assert!(
        diag.message_text
            .contains("Type 'number' is not assignable to type 'T'"),
        "TS2322 should widen the initializer to number, got: {diag:?}"
    );
    assert!(
        !diag
            .message_text
            .contains("Type '1' is not assignable to type 'T'"),
        "TS2322 should not preserve the numeric literal in this generic initializer case, got: {diag:?}"
    );
}

#[test]
fn non_distributive_conditional_with_any_evaluates_to_true_branch() {
    // `[any] extends [number] ? 1 : 0` should evaluate to `1` (non-distributive).
    // `any extends number ? 1 : 0` should evaluate to `0 | 1` (distributive, picks both).
    // Assigning `0` to `U` (= 1) should emit TS2322, with message "...to type '1'",
    // NOT "...to type '[any] extends [number] ? 1 : 0'".
    let source = r#"
            type T = any extends number ? 1 : 0;
            let x: T;
            x = 1;
            x = 0;

            type U = [any] extends [number] ? 1 : 0;
            let y: U;
            y = 1;
            y = 0;
        "#;

    let diagnostics = diagnostics_for(source);
    // `x = 0` should NOT error: T = 0 | 1, and 0 is assignable to 0 | 1
    let x_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code == 2322 && d.message_text.contains("'0'") && d.message_text.contains("'0 | 1'")
        })
        .collect();
    assert!(
        x_errors.is_empty(),
        "x = 0 should not error since T = 0 | 1"
    );

    // `y = 0` should error: U = 1, and 0 is not assignable to 1
    let y_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code == 2322 && d.message_text.contains("'0'") && d.message_text.contains("'1'")
        })
        .collect();
    assert_eq!(
        y_errors.len(),
        1,
        "y = 0 should emit TS2322 with type '1', got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // The error message should reference the evaluated type '1', not the deferred conditional
    assert!(
        !y_errors[0].message_text.contains("extends"),
        "Error message should use evaluated type '1', not deferred conditional. Got: {}",
        y_errors[0].message_text
    );
}

#[test]
fn union_keyed_index_write_type_is_intersection() {
    // When writing to obj[k] where k is a union key, the write type is the
    // intersection of all property types. For `{ a: string, b: number }` with
    // key `'a' | 'b'`, write type = string & number = never.
    // tsc emits TS2322: Type 'any' is not assignable to type 'never'.
    let source = r#"
            const x1 = { a: 'foo', b: 42 };
            declare let k: 'a' | 'b';
            x1[k] = 'bar' as any;
        "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        1,
        "expected 1 TS2322 for assigning any to never (intersection of string & number), got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn any_not_assignable_to_never() {
    // tsc: Type 'any' is not assignable to type 'never'. (TS2322)
    // `any` bypasses most type checks but cannot be assigned to `never`.
    let source = r#"
            declare let x: never;
            x = 'bar' as any;
        "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        1,
        "expected 1 TS2322 for assigning any to never, got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn generic_conditional_type_alias_stays_deferred() {
    // Generic type aliases should NOT be eagerly evaluated — they stay deferred
    // until instantiated. This ensures we don't break generic conditional types.
    let source = r#"
            type IsString<T> = T extends string ? true : false;
            let a: IsString<string> = true;
            let b: IsString<number> = false;
            let c: IsString<string> = false;
        "#;

    let diagnostics = diagnostics_for(source);
    // `c = false` should error: IsString<string> = true, and false is not assignable to true
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        1,
        "expected 1 TS2322 for `c = false`, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn private_setter_only_no_false_ts2322() {
    // A class with a set-only private accessor should not emit TS2322
    // when assigning to it. The write type (setter param) is `number`,
    // not the read type (`undefined`).
    let source = r#"
            class C {
                set #foo(a: number) {}
                bar() {
                    let x = (this.#foo = 42 * 2);
                }
            }
        "#;

    let diagnostics = diagnostics_for(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "setter-only private accessor should not produce TS2322, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn private_setter_only_read_emits_ts2806() {
    // Reading from a private setter-only accessor should emit TS2806
    // ("Private accessor was defined without a getter"), not cascade
    // into TS2532/TS2488 from the `undefined` read type.
    let source = r#"
            class C {
                set #foo(a: number) {}
                bar() {
                    const value = this.#foo;
                }
            }
        "#;

    let diagnostics = diagnostics_for(source);
    let ts2806 = diagnostics.iter().filter(|d| d.code == 2806).count();
    assert_eq!(
        ts2806,
        1,
        "expected 1 TS2806 for reading setter-only private accessor, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    // Should NOT produce cascading TS2532 (possibly undefined)
    let ts2532 = diagnostics.iter().filter(|d| d.code == 2532).count();
    assert_eq!(ts2532, 0, "should not cascade into TS2532");
}

#[test]
fn private_setter_only_compound_assignment_emits_ts2806() {
    // Compound assignments (`+=`) read the LHS, so setter-only accessors
    // should trigger TS2806 for the read part.
    let source = r#"
            class C {
                set #val(a: number) {}
                bar() {
                    this.#val += 3;
                }
            }
        "#;

    let diagnostics = diagnostics_for(source);
    let ts2806 = diagnostics.iter().filter(|d| d.code == 2806).count();
    assert_eq!(
        ts2806,
        1,
        "expected 1 TS2806 for compound assignment to setter-only private accessor, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn inner_assignment_in_variable_decl_anchors_at_assignment_target() {
    let source = r#"interface A { x: number; }
interface B { y: string; }
declare let b: B;
declare let a: A;
const x = a = b;"#;

    let diagnostics = diagnostics_for(source);

    let ts2741: Vec<_> = diagnostics.iter().filter(|d| d.code == 2741).collect();
    assert!(
        !ts2741.is_empty(),
        "expected TS2741 for inner assignment in variable decl, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );

    // The diagnostic should anchor at `a` (the inner assignment target),
    // NOT at `const` (the variable statement start).
    let diag = ts2741[0];
    let a_offset = source.find("const x = a = b;").unwrap() + "const x = ".len();
    assert_eq!(
        diag.start as usize, a_offset,
        "TS2741 should point to inner assignment target 'a' (offset {}), not offset {}",
        a_offset, diag.start
    );
}

#[test]
fn generic_construct_signature_return_type_mismatch_ts2322() {
    // Generic construct signatures must not suppress TS2322 when comparing
    // incompatible return types. The `has_own_signature_type_params` check
    // must include construct_signatures, not just call_signatures.
    let source = r#"
        declare var a: new <T>(x: T) => void;
        declare var b: new <T>(x: T) => T;
        b = a;
    "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        1,
        "expected 1 TS2322 for assigning new <T>(x: T) => void to new <T>(x: T) => T, got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn generic_function_to_void_assignment_anchor_rhs() {
    let source = r#"
var x: void;
function f<T>(a: T) {}
x = f;
"#;

    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for function-to-void assignment");

    let expected_f_offset = source
        .find("x = f;")
        .expect("expected rhs function reference") as u32
        + 4;
    assert_eq!(
        diag.start, expected_f_offset,
        "TS2322 should anchor at the rhs function identifier"
    );
    assert_eq!(diag.length, 1, "TS2322 should cover only `f`");
}

#[test]
fn generic_construct_signature_different_arity_ts2322() {
    // When source has fewer type params than target (1 vs 2), the source is
    // more restrictive and should not be assignable.
    let source = r#"
        declare var a: new <T>(x: { a: T; b: T }) => T[];
        declare var b: new <U, V>(x: { a: U; b: V }) => U[];
        b = a;
    "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        1,
        "expected 1 TS2322 for different type param arity construct signatures, got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn generic_rest_types_callback_contravariance_ts2322() {
    // When a function type with outer-scope generic rest parameters (T extends any[])
    // is assigned to a concrete function type with never rest params in callback position,
    // the assignability check must NOT be suppressed — the types are genuinely incompatible.
    let source = r#"
        function assignmentWithComplexRest2<T extends any[]>() {
            const fn1: (cb: (x: string, ...rest: T) => void) => void = (cb) => {};
            const fn2: (cb: (...args: never) => void) => void = fn1;
        }
    "#;

    let diagnostics = strict_diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322_count >= 1,
        "expected at least 1 TS2322 for callback contravariance with never rest params, got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn generic_rest_types_direct_assignment_no_error() {
    // Direct assignment of a function with generic rest params to (...args: never) => void
    // should NOT produce an error.
    let source = r#"
        function assignmentWithComplexRest<T extends any[]>() {
            const fn1: (x: string, ...rest: T) => void = (x, ..._) => x;
            const fn2: (...args: never) => void = fn1;
        }
    "#;

    let diagnostics = strict_diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        0,
        "expected no TS2322 for direct assignment with generic rest params, got {ts2322_count}. Diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn generic_function_stricter_constraints_emit_ts2322() {
    let source = r#"
        var f = function <T, S extends T>(x: T, y: S): void {
            x = y
        };

        var g = function <T, S>(x: T, y: S): void { };

        g = f;
    "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        1,
        "expected TS2322 for assigning a stricter generic callback to a looser one, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn declared_generic_function_stricter_constraints_emit_ts2322() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;

        g = f;
    "#;

    let diagnostics = diagnostics_for(source);
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        1,
        "expected TS2322 for declared generic signatures with stricter source constraints, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn constrained_generic_signature_preserves_type_param_constraint_metadata() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

    let shapes = function_shapes_for_named_bindings(source, &["f", "g"]);
    let f_shape = shapes[0].as_ref().expect("expected function shape for f");
    let g_shape = shapes[1].as_ref().expect("expected function shape for g");

    assert_eq!(
        f_shape.type_params.len(),
        2,
        "f shape lost generic params: {f_shape:?}"
    );
    assert_eq!(
        g_shape.type_params.len(),
        2,
        "g shape lost generic params: {g_shape:?}"
    );
    assert!(
        f_shape.type_params[1].constraint.is_some(),
        "expected constrained source type param metadata to be preserved: {f_shape:?}"
    );
    assert!(
        g_shape.type_params[1].constraint.is_none(),
        "expected unconstrained target type param metadata to stay unconstrained: {g_shape:?}"
    );
}

#[test]
fn assignability_normalization_preserves_generic_constraint_metadata() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

    let shapes = normalized_function_shapes_for_named_bindings(source, &["f", "g"]);
    let f_shape = shapes[0]
        .as_ref()
        .expect("expected normalized function shape for f");
    let g_shape = shapes[1]
        .as_ref()
        .expect("expected normalized function shape for g");

    assert_eq!(
        f_shape.type_params.len(),
        2,
        "normalized source shape lost generic params: {f_shape:?}"
    );
    assert_eq!(
        g_shape.type_params.len(),
        2,
        "normalized target shape lost generic params: {g_shape:?}"
    );
    assert!(
        f_shape.type_params[1].constraint.is_some(),
        "normalized source shape lost the S extends T constraint: {f_shape:?}"
    );
    assert!(
        g_shape.type_params[1].constraint.is_none(),
        "normalized target shape unexpectedly gained a constraint: {g_shape:?}"
    );
}

#[test]
fn assignability_normalization_keeps_generic_functions_callable_not_plain_objects() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

    let kinds = normalized_type_kinds_for_named_bindings(source, &["f", "g"]);
    assert_eq!(
        kinds[0], "Function",
        "expected normalized source to stay a function, got {kinds:?}"
    );
    assert_eq!(
        kinds[1], "Function",
        "expected normalized target to stay a function, got {kinds:?}"
    );
}

#[test]
fn solver_subtype_rejects_stricter_generic_constraints_directly() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

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
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let ids: Vec<_> = ["f", "g"]
        .iter()
        .map(|name| {
            binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .map(|type_id| checker.evaluate_type_for_assignability(type_id))
                .expect("expected binding type")
        })
        .collect();

    assert!(
        !is_fresh_subtype_of(checker.ctx.types, ids[0], ids[1]),
        "boundary subtype unexpectedly accepts stricter generic constraints"
    );
}

#[test]
fn boundary_assignability_rejects_stricter_generic_constraints() {
    let source = r#"
        declare let f: <T, S extends T>(x: T, y: S) => void;
        declare let g: <T, S>(x: T, y: S) => void;
    "#;

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
        CheckerOptions::default(),
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let ids: Vec<_> = ["f", "g"]
        .iter()
        .map(|name| {
            binder
                .file_locals
                .get(name)
                .map(|sym_id| checker.get_type_of_symbol(sym_id))
                .map(|type_id| checker.evaluate_type_for_assignability(type_id))
                .expect("expected binding type")
        })
        .collect();

    let overrides = CheckerOverrideProvider::new(&checker, None);
    let relation_result = is_assignable_with_overrides(
        &AssignabilityQueryInputs {
            db: checker.ctx.types,
            resolver: &checker.ctx,
            source: ids[0],
            target: ids[1],
            flags: checker.ctx.pack_relation_flags(),
            inheritance_graph: &checker.ctx.inheritance_graph,
            sound_mode: checker.ctx.sound_mode(),
        },
        &overrides,
    );
    assert!(
        !relation_result.is_related(),
        "assignability boundary unexpectedly accepts stricter generic constraints"
    );
}

#[test]
fn js_constructor_property_with_logical_or_is_declaration() {
    // Pattern: `X.Y = X.Y || function() {}` — tsc treats this as a
    // declaration (AssignmentDeclarationKind.Property), not a regular
    // assignment. No TS2322 should be emitted.
    let diagnostics = check_js_source_diagnostics(
        r#"
var test = {};
test.K = test.K ||
    function () {}

test.K.prototype = {
    add() {}
};
"#,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "JS lazy constructor initialization `X.Y = X.Y || function() {{}}` \
         should be treated as a declaration and not produce TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn js_constructor_property_with_nullish_coalescing_is_declaration() {
    // Pattern: `X.Y = X.Y ?? function() {}` — same as above but with `??`.
    let diagnostics = check_js_source_diagnostics(
        r#"
var test = {};
test.K = test.K ??
    function () {}
"#,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "JS lazy constructor initialization `X.Y = X.Y ?? function() {{}}` \
         should be treated as a declaration and not produce TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn import_meta_assignment_emits_ts2364() {
    // import.meta is parsed as PROPERTY_ACCESS_EXPRESSION in tsz, but assigning to
    // import.meta directly should emit TS2364 (not a valid assignment target), matching tsc.
    let diags = diagnostics_for("import.meta = {};");
    assert!(
        diags.iter().any(|d| d.code == 2364),
        "Expected TS2364 for `import.meta = {{}}` but got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn import_meta_property_assignment_is_valid() {
    // import.meta.foo is a regular property access, so the assignment target is valid.
    // It may still emit a property error, but not TS2364.
    let diags = diagnostics_for("import.meta.foo = 42;");
    assert!(
        !diags.iter().any(|d| d.code == 2364),
        "Should NOT emit TS2364 for `import.meta.foo = 42` but got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `({ } = { x: 0, y: 0 })` is a destructuring assignment with an empty
/// pattern. tsc treats every property on the RHS as excess and emits TS2353
/// for each, even though the empty `{}` target is normally treated as wide
/// for assignability. The variable-declaration form `var { } = { x: 0, y: 0 };`
/// stays silent — only the assignment-expression shape gets the strict check.
#[test]
fn destructuring_assignment_empty_pattern_emits_ts2353_for_each_excess_property() {
    let diags = diagnostics_for(
        r#"
function f() {
    ({ } = { x: 0, y: 0 });
}
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert_eq!(
        ts2353.len(),
        2,
        "expected exactly two TS2353 (one per RHS property) for empty destructuring pattern; got: {ts2353:?}"
    );
    assert!(
        ts2353.iter().any(|d| d.message_text.contains("'x'")),
        "expected TS2353 for property 'x', got: {ts2353:?}"
    );
    assert!(
        ts2353.iter().any(|d| d.message_text.contains("'y'")),
        "expected TS2353 for property 'y', got: {ts2353:?}"
    );
}

/// `var { } = { x: 0, y: 0 };` (declaration form) must NOT emit TS2353 for
/// excess properties. tsc only applies the strict empty-pattern check to
/// destructuring assignments, not declarations — verifying the new check is
/// scoped correctly to the assignment path.
#[test]
fn destructuring_declaration_empty_pattern_does_not_emit_ts2353() {
    let diags = diagnostics_for(
        r#"
function f() {
    var { } = { x: 0, y: 0 };
}
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "destructuring declaration with empty pattern must not emit TS2353; got: {ts2353:?}"
    );
}

/// Regression: TS2322 must fire when a bare type parameter is assigned to a
/// template-literal pattern referencing the same type parameter.
///
/// `tsc` reports TS2322 here because `\`${T}\`` is an opaque pattern type;
/// `T`'s instantiation could be a literal subtype that does not structurally
/// match the template, so the assignment is not statically sound. Without the
/// template-literal carve-out in `should_suppress_assignability_diagnostic`,
/// the generic "complex type" suppression would silently accept it because
/// `\`${T}\`` "contains" T but is not itself a type parameter.
///
/// Repros the missing fingerprint at
/// `templateLiteralTypes5.ts(14,11)`.
#[test]
fn type_parameter_to_template_literal_of_self_emits_ts2322() {
    let source = r#"
function f<T extends "a" | "b">(x: T) {
    const test1: `${T}` = x;
}
"#;
    let diags = diagnostics_for(source);
    let ts2322s: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322s.is_empty(),
        "expected TS2322 for `T -> \\`${{T}}\\`` assignment; diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let lhs_diag = ts2322s
        .iter()
        .find(|d| d.message_text.contains("'T'") && d.message_text.contains("`${T}`"))
        .expect("expected TS2322 message naming T and `${T}`");
    let test1_start = source.find("test1").expect("expected variable name") as u32;
    assert_eq!(
        lhs_diag.start, test1_start,
        "TS2322 should anchor at the variable declaration name (test1)"
    );
}

/// Companion check: template-literal vs template-literal assignments where
/// both sides share a type parameter (e.g. `\`${Uppercase<T>}\``) must keep
/// their existing suppression. This locks in the narrowness of the
/// template-literal carve-out so it does not regress
/// `templateLiteralTypes3.ts` (where tsc accepts the spread of values typed
/// `Uppercase<\`1.${T}.4\`>` against an inferred `Uppercase<\`1.${T}.3\`>`).
#[test]
fn template_literal_to_template_literal_with_generic_intrinsic_does_not_emit_ts2345() {
    let source = r#"
type DotString = `${string}.${string}.${string}`;
declare function spread<P extends DotString>(...args: P[]): P;
function ft1<T extends string>(
    u1: Uppercase<`1.${T}.3`>,
    u2: Uppercase<`1.${T}.4`>,
) {
    spread(u1, u2);
}
"#;
    let diags = diagnostics_for(source);
    let ts2345s: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345s.is_empty(),
        "template-vs-template generic intrinsic spread must stay suppressed; \
         got TS2345 diagnostics: {:?}",
        ts2345s.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// `function h({ prop = "baz" }: StringUnion)` — when a binding-element default
/// is a non-elaboratable expression (e.g. a string literal that doesn't fit a
/// literal-union target), tsc anchors TS2322 on the binding name (`prop`)
/// rather than the initializer expression (`"baz"`).
///
/// Regression test for
/// `conformance/types/contextualTypes/methodDeclarations/contextuallyTypedBindingInitializerNegative.ts`.
#[test]
fn binding_default_string_lit_anchors_at_binding_name() {
    let source = r#"
interface StringUnion { prop: "foo" | "bar"; }
function h({ prop = "baz" }: StringUnion) {}
"#;
    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for non-fitting binding default");

    // Locate the binding name `prop` and the initializer `"baz"` in the
    // source so the assertion stays robust if surrounding text changes.
    let prop_offset = source.find("prop = ").expect("expected `prop = `") as u32;
    let baz_offset = source.find("\"baz\"").expect("expected `\"baz\"`") as u32;

    assert_eq!(
        diag.start, prop_offset,
        "TS2322 should anchor at the binding name `prop` (offset {prop_offset}), \
         not the initializer `\"baz\"` (offset {baz_offset}); got: {diag:?}"
    );
    assert!(
        diag.message_text.contains("\"baz\"")
            && diag.message_text.contains("\"foo\" | \"bar\""),
        "TS2322 message should still describe the actual mismatch (\"baz\" vs literal union), \
         got: {:?}",
        diag.message_text
    );
}

/// Even though the binding-default anchor walks to the binding name, an arrow
/// function default with a body return-type mismatch (e.g.
/// `function f({ show: x = v => v }: Show)` where `Show.show` returns `string`)
/// should still elaborate to the body expression — the elaboration path
/// (`try_elaborate_function_arg_return_error`) overrides the binding-name
/// anchor with its own body anchor. This test pins that contract.
#[test]
fn binding_default_arrow_body_return_mismatch_still_elaborates_to_body() {
    let source = r#"
interface Show { show: (x: number) => string; }
function f({ show: showRename = v => v }: Show) {}
"#;
    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for arrow body return type mismatch");

    // The error must anchor at the second `v` (the body), not at `show:`,
    // `showRename`, or the whole arrow `v => v`.
    let body_offset = {
        let arrow_idx = source.find("v => v").expect("expected `v => v`");
        let body_start = arrow_idx + "v => ".len();
        body_start as u32
    };
    assert_eq!(
        diag.start, body_offset,
        "TS2322 for arrow body return mismatch should anchor at the body expression \
         (offset {body_offset}); got: {diag:?}"
    );
    assert!(
        diag.message_text.contains("'number'") && diag.message_text.contains("'string'"),
        "TS2322 should describe the body return-type mismatch (number vs string), got: {:?}",
        diag.message_text
    );
}
