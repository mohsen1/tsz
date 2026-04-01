use crate::context::CheckerOptions;
use crate::test_utils::{check_js_source_diagnostics, check_source};

fn diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
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
