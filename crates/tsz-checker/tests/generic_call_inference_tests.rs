//! Focused tests for generic call inference and contextual instantiation.
//!
//! These exercise the `call_inference.rs` module:
//! - Round-2 contextual typing for callback parameters
//! - Return-context substitution collection
//! - Generic function argument refinement against targets
//! - Widening/literal-preservation in type parameter substitutions
//! - Binding-pattern sanitization during inference
//! - Contextual constraint with self-referential type parameters
//! - Application shape preservation through union/intersection
//! - Anyish inference detection across composite types
//! - Return context substitution through tuples, arrays, and generics

use tsz_checker::context::CheckerOptions;

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn compile_and_get_raw_diagnostics(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

fn load_es5_lib_files_for_test() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

fn compile_with_es5_lib_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_es5_lib_files_for_test();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
}

fn relevant_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_with_es5_lib_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn compile_strict_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn compile_js_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

fn relevant_js_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_js_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

fn relevant_strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_strict_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

fn relevant_default_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = tsz_checker::test_utils::load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn relevant_strict_default_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = tsz_checker::test_utils::load_default_lib_files();
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|(code, _)| *code != 2318)
    .collect()
}

#[test]
fn readonly_const_tuple_spread_into_fixed_arity_generic_call_no_ts2554() {
    let diagnostics = relevant_default_lib_diagnostics(
        r#"
function infer<T, U, V>(a: T, b: U, c: V): [T, U, V] {
    return [a, b, c];
}

const args = [1, 'hello', true] as const;
const result = infer(...args);
const check: [1, 'hello', true] = result;
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected readonly const tuple spread to satisfy fixed-arity generic call; got: {diagnostics:#?}"
    );
}

#[test]
fn variadic_rest_tuple_satisfies_array_rest_constraint() {
    let source = r#"
export {};

export interface Option<T> {
  zip<O extends Array<Option<any>>>(...others: O): Option<[T, ...UnzipOptionArray<O>]>;
}

type UnzipOption<T> = T extends Option<infer V> ? V : never;
type UnzipOptionArray<T> = { [k in keyof T]: T[k] extends Option<any> ? UnzipOption<T[k]> : never };

declare const opt1: Option<number>;
declare const opt2: Option<string>;
declare const opt3: Option<boolean>;

opt1.zip(opt2, opt3);
"#;

    let diagnostics = relevant_lib_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "rest tuple should satisfy Array<Option<any>> constraint: {diagnostics:#?}"
    );
}

#[test]
fn generic_rest_parameter_infers_literal_tuple_under_primitive_array_constraint() {
    let diagnostics = relevant_default_lib_diagnostics(
        r#"
function typed<T extends string[]>(...args: T): T {
    return args;
}

const t1 = typed("a", "b", "c");
const check1: ["a", "b", "c"] = t1;
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "generic rest parameter should infer a tuple of string literal types; got: {diagnostics:#?}"
    );
}

#[test]
fn conditional_parameter_infers_through_branches_before_assignability() {
    let source = r#"
interface Iterable<T> {}
interface Array<T> extends Iterable<T> {}

type NonStringIterable<T> =
  T extends string ? never : T extends Iterable<any> ? T : never;

declare function doSomething<T>(value: NonStringIterable<T>): T;

doSomething('value'); // error: T = string, parameter reduces to never
doSomething(['v']); // ok: T = string[], parameter reduces to string[]
doSomething([{ foo() {} }]); // ok: T = { foo(): void }[]
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Only the string argument should fail after branch inference. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2345
            .iter()
            .any(|(_, msg)| msg.contains("not assignable to parameter of type 'never'")),
        "The rejected string branch should reduce the parameter to never. Diagnostics: {diags:#?}"
    );
}

#[test]
fn user_defined_comparable_diagnostic_is_not_rewritten_from_source_text() {
    // Regression test for issue #3057: when a user defines their own
    // `Comparable<T>` and calls a function whose declared parameter type is
    // `Comparable<number>`, the diagnostic must reflect the declared type.
    // It must never be rewritten to `Comparable<1 | 2>` (or any other generic
    // instantiation) by scanning numeric literals at the call site, because
    // nothing in the program defines or expects that instantiation.
    let source = r#"
interface Comparable<T> {
    value: T;
}

declare function acceptsComparable(value: Comparable<number>, ...rest: number[]): void;

acceptsComparable(1, 2);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for acceptsComparable(1, 2); got: {diags:#?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("parameter of type 'Comparable<number>'"),
        "diagnostic must report the declared parameter type, not a synthesized \
         instantiation derived from numeric call-site literals. Got: {msg}"
    );
    assert!(
        !msg.contains("Comparable<1 | 2>"),
        "the source-text rewrite must not synthesize a `Comparable<1 | 2>` type \
         that the program never declares. Got: {msg}"
    );
}

#[test]
fn user_defined_comparable_with_three_literals_is_not_rewritten() {
    // Same rule, different literal arity — verifies the fix is structural and
    // not just dropping the special case for "two literals".
    let source = r#"
interface Comparable<T> {
    value: T;
}

declare function acceptsComparable(value: Comparable<number>, ...rest: number[]): void;

acceptsComparable(1, 2, 3);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for acceptsComparable(1, 2, 3); got: {diags:#?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("parameter of type 'Comparable<number>'"),
        "diagnostic must report the declared parameter type. Got: {msg}"
    );
    assert!(
        !msg.contains("Comparable<1 | 2 | 3>"),
        "the source-text rewrite must not synthesize a literal-union \
         instantiation of `Comparable`. Got: {msg}"
    );
}

#[test]
fn self_referential_constraint_fallback_displays_literal_union_candidates() {
    // The call is invalid because number primitives do not satisfy
    // Comparable<T>. Inference still observes both literal candidates before
    // widening them for assignability, and tsc uses that candidate union in the
    // constraint display.
    let source = r#"
interface Comparable<T> {
    compareTo(other: T): number;
}
interface Comparer {
    <T extends Comparable<T>>(x: T, y: T): T;
}
declare const max2: Comparer;
max2(1, 2);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for max2(1, 2); got: {diags:#?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Argument of type 'number'"),
        "source should still be the widened primitive. Got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'Comparable<1 | 2>'"),
        "constraint fallback should display the literal candidate union. Got: {msg}"
    );
    assert!(
        !msg.contains("Comparable<number>"),
        "constraint fallback must not lose literal candidate provenance. Got: {msg}"
    );
}

#[test]
fn self_referential_constraint_fallback_preserves_literal_union_after_contextual_assignment() {
    let source = r#"
interface Comparable<T> {
    compareTo(other: T): number;
}
interface Comparer {
    <T extends Comparable<T>>(x: T, y: T): T;
}
var max2: Comparer = (x, y) => { return (x.compareTo(y) > 0) ? x : y };
var maxResult = max2(1, 2);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for max2(1, 2); got: {diags:#?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Argument of type 'number'"),
        "source should still be widened for primitive literal candidates. Got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'Comparable<1 | 2>'"),
        "contextual function assignment should not erase literal candidate display. Got: {msg}"
    );
    assert!(
        !msg.contains("Comparable<number>"),
        "constraint fallback must not display the widened primitive candidate. Got: {msg}"
    );
}

#[test]
fn self_referential_constraint_fallback_anchors_first_argument_after_contextual_assignment() {
    let source = r#"
interface Comparable<T> {
    compareTo(other: T): number;
}
interface Comparer {
    <T extends Comparable<T>>(x: T, y: T): T;
}
var max2: Comparer = (x, y) => { return (x.compareTo(y) > 0) ? x : y };
    var maxResult = max2(1, 2);
"#;
    let diagnostics = compile_and_get_raw_diagnostics(source);
    let first_arg_start = source.find("max2(1, 2)").expect("expected call") + "max2(".len();
    let matching_ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2345 && diag.start == first_arg_start as u32)
        .collect();
    assert_eq!(
        matching_ts2345.len(),
        1,
        "expected exactly one TS2345 anchored at max2's first argument. Got: {diagnostics:#?}"
    );
    let ts2345 = matching_ts2345[0];

    assert_eq!(
        ts2345.start, first_arg_start as u32,
        "TS2345 should anchor the first failing argument. Got: {ts2345:#?}"
    );
    assert_eq!(ts2345.length, 1, "TS2345 should cover only `1`");
    assert!(
        ts2345
            .message_text
            .contains("parameter of type 'Comparable<1 | 2>'"),
        "expected literal candidate display at the conformance anchor. Got: {ts2345:#?}"
    );
}

#[test]
fn self_referential_constraint_fallback_display_scales_beyond_two_candidates() {
    let source = r#"
interface Wrapped<T> {
    value: T;
}
interface UseWrapped {
    <T extends Wrapped<T>>(a: T, b: T, c: T): T;
}
declare const useWrapped: UseWrapped;
useWrapped("a", "b", "c");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for useWrapped literals; got: {diags:#?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Argument of type 'string'"),
        "source should still be widened for primitive literal candidates. Got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'Wrapped<\"a\" | \"b\" | \"c\">'"),
        "display provenance should preserve all literal candidates. Got: {msg}"
    );
}

#[test]
fn self_referential_constraint_fallback_displays_canonical_boolean_candidate() {
    let source = r#"
interface Wrapped<T> {
    value: T;
}
interface UseWrapped {
    <T extends Wrapped<T>>(a: T, b: T): T;
}
declare const useWrapped: UseWrapped;
useWrapped(true, false);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for boolean literal candidates; got: {diags:#?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("Argument of type 'boolean'"),
        "source should still be widened for primitive boolean candidates. Got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'Wrapped<boolean>'"),
        "true/false candidate unions should display through the canonical boolean type. Got: {msg}"
    );
    assert!(
        !msg.contains("Wrapped<true") && !msg.contains("Wrapped<false"),
        "boolean constraint fallback should not spell true/false as a literal union. Got: {msg}"
    );
}

// ─── Overloaded function arguments ───────────────────────────────────

#[test]
fn overloaded_function_argument_uses_last_signature_for_generic_inference() {
    let source = r#"
interface Promise<T> {
    then<U>(cb: (x: T) => Promise<U>): Promise<U>;
}

declare function testFunction(n: number): Promise<number>;
declare function testFunction(s: string): Promise<string>;

declare var numPromise: Promise<number>;
var newPromise = numPromise.then(testFunction);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "Overloaded function argument should infer U from the last overload and reject the number callback. Diagnostics: {diags:#?}"
    );
}

#[test]
fn overloaded_method_reports_no_overload_when_callback_inference_uses_last_signature() {
    let source = r#"
interface Promise<T> {
    then<U>(cb: (x: T) => Promise<U>): Promise<U>;
    then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => Promise<U>): Promise<U>;
}

declare function testFunction(n: number): Promise<number>;
declare function testFunction(s: string): Promise<string>;

declare var numPromise: Promise<number>;
var newPromise = numPromise.then(testFunction);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2769),
        "Overloaded method should reject every candidate and report TS2769. Diagnostics: {diags:#?}"
    );
}

// ─── Round-2 contextual typing for callbacks ─────────────────────────

#[test]
fn callback_parameter_gets_contextual_type_from_generic_call() {
    // The callback `x => x` should infer `x: string` from the generic call
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map(["a", "b"], x => x.length);
"#;
    let diags = relevant_diagnostics(source);
    // x should be contextually typed as string; x.length should work
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Callback parameter should be contextually typed. Diagnostics: {diags:#?}"
    );
}

#[test]
fn round2_contextual_type_for_multi_param_generic() {
    // Both T and U should be inferred in a two-type-parameter scenario
    let source = r#"
declare function zip<T, U>(a: T[], b: U[], fn: (x: T, y: U) => [T, U]): [T, U][];
const result = zip([1, 2], ["a", "b"], (x, y) => [x, y]);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Multi-param generic should contextually type all callback params. Diagnostics: {diags:#?}"
    );
}

#[test]
fn mapped_object_key_inference_is_lower_priority_than_direct_key_argument() {
    let source = r#"
type Lower<T> = { [K in keyof T]: T[K] };

declare function appendToOptionalArray<
  K extends string | number | symbol,
  T
>(
  object: { [x in K]?: Lower<T>[] },
  key: K,
  value: T
): void;

const foo: { x?: number[]; y?: string[] } = {};
appendToOptionalArray(foo, "x", 123);
appendToOptionalArray(foo, "y", "bar");
appendToOptionalArray(foo, "y", 12);
appendToOptionalArray(foo, "x", "no");
"#;
    let diags = relevant_strict_diagnostics(source);
    let ts2345_messages: Vec<_> = diags
        .iter()
        .filter_map(|(code, message)| (*code == 2345).then_some(message.as_str()))
        .collect();

    assert_eq!(
        ts2345_messages.len(),
        2,
        "only the two mismatched key/value calls should report TS2345. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2345_messages
            .iter()
            .any(|message| message.contains("{ y?: 12[]")),
        "the numeric value passed with key 'y' should check the object against only the y slot. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2345_messages
            .iter()
            .any(|message| message.contains("{ x?: string[]")),
        "the string value passed with key 'x' should check the object against only the x slot. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2345_messages
            .iter()
            .all(|message| !message.contains("Lower<")),
        "the final TS2345 surface should use the instantiated mapped property type, not the alias wrapper. Diagnostics: {diags:#?}"
    );
}

// ─── Return-context substitution ─────────────────────────────────────

#[test]
fn return_context_infers_type_argument_from_variable_annotation() {
    let source = r#"
declare function identity<T>(x: T): T;
const x: string = identity("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should help infer T=string. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_identity_preserves_single_literal_argument() {
    let source = r#"
function identity<T>(x: T): T { return x; }
const result = identity("test");
const check: "test" = result;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, message)| {
            *code != 2322 || !message.contains("Type 'string' is not assignable to type '\"test\"'")
        }),
        "generic identity should preserve a single direct literal candidate. Diagnostics: {diags:#?}"
    );
}

#[test]
fn return_context_detects_mismatch() {
    let source = r#"
declare function identity<T>(x: T): T;
const x: string = identity(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Should detect type mismatch: number not assignable to string. Diagnostics: {diags:#?}"
    );
}

// ─── Generic function argument refinement ────────────────────────────

#[test]
fn generic_callback_refined_against_target_params() {
    // A generic callback passed as argument should get instantiated
    // against the target parameter types
    let source = r#"
declare function apply<T>(fn: (x: T) => T, value: T): T;
const result = apply(x => x, 42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Generic callback should be refined. Diagnostics: {diags:#?}"
    );
}

#[test]
fn direct_generic_argument_mismatch_is_not_recovered_to_success() {
    let source = r#"
declare function bar<T>(item1: T, item2: T): T;
bar(1, "");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diags.iter().find(|(code, _)| *code == 2345);
    assert!(
        ts2345.is_some(),
        "Expected TS2345 for conflicting direct inference candidates. Diagnostics: {diags:#?}"
    );
    let msg = &ts2345.unwrap().1;
    assert!(
        msg.contains("Argument of type '\"\"' is not assignable to parameter of type '1'."),
        "TS2345 should preserve direct literal candidates. Got: {msg:?}"
    );
}

#[test]
fn direct_generic_argument_mismatch_survives_context_sensitive_callback() {
    let source = r#"
declare function g<T>(a: T, b: T, c: (t: T) => T): T;
g("", 3, a => a);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diags.iter().find(|(code, _)| *code == 2345);
    assert!(
        ts2345.is_some(),
        "Expected TS2345 for conflicting direct candidates before callback inference. Diagnostics: {diags:#?}"
    );
    let msg = &ts2345.unwrap().1;
    assert!(
        msg.contains("Argument of type '3' is not assignable to parameter of type '\"\"'."),
        "TS2345 should preserve the first direct literal inference candidate in the diagnostic. Got: {msg:?}"
    );
}

#[test]
fn rest_generic_argument_mismatch_displays_primitive_bases() {
    let source = r#"
declare function rest<T>(...items: T[]): T;
rest(1, "");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diags.iter().find(|(code, _)| *code == 2345);
    assert!(
        ts2345.is_some(),
        "Expected TS2345 for conflicting rest inference candidates. Diagnostics: {diags:#?}"
    );
    let msg = &ts2345.unwrap().1;
    assert!(
        msg.contains("Argument of type 'string' is not assignable to parameter of type 'number'."),
        "TS2345 should display primitive bases for conflicting rest generic candidates. Got: {msg:?}"
    );
}

#[test]
fn contextual_signature_instantiation_rejects_conflicting_generic_params() {
    let source = r#"
declare function foo<T>(cb: (x: number, y: string) => T): T;
declare function bar<T, U, V>(x: T, y: U, cb: (x: T, y: U) => V): V;
declare function g<T>(x: T, y: T): T;

var b: number | string;
var b = foo(g);
var b = bar(1, "one", g);
var b = bar("one", 1, g);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345_count = diags.iter().filter(|(code, _)| *code == 2345).count();
    let ts2403_count = diags.iter().filter(|(code, _)| *code == 2403).count();
    assert!(
        ts2345_count >= 3,
        "Expected TS2345 for each incompatible contextual generic callback. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2403_count >= 3,
        "Expected downstream TS2403 from each failed call's unknown return. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_class_function_member_annotated_callback_keeps_outer_type_param_in_ts2345() {
    let source = r#"
namespace WithCandidates {
    class C<T> {
        foo2<T, U>(x: T, cb: (a: T) => U) {
            return cb(x);
        }
    }
    declare var c: C<number>;

    class C3<T, U> {
        foo3<T, U>(x: T, cb: (a: T) => U, y: U) {
            return cb(x);
        }
    }
    declare var c3: C3<number, string>;

    function other<T, U>(t: T, u: U) {
        var r10 = c.foo2(1, (x: T) => '');
        var r11 = c3.foo3(1, (x: T) => '', '');
        var r11b = c3.foo3(1, (x: T) => '', 1);
    }
}
"#;
    let diags = relevant_diagnostics(source);
    let ts2345_messages: Vec<_> = diags
        .iter()
        .filter_map(|(code, msg)| (*code == 2345).then_some(msg.as_str()))
        .collect();
    assert_eq!(
        ts2345_messages.len(),
        3,
        "Expected exactly three TS2345 diagnostics. Diagnostics: {diags:#?}"
    );
    for msg in &ts2345_messages {
        assert!(
            msg.contains("Argument of type 'number' is not assignable to parameter of type 'T'."),
            "Expected the diagnostic to keep the annotated outer type parameter. Got: {msg:?}"
        );
        assert!(
            !msg.contains("parameter of type '1'"),
            "Fresh literal inference should not replace the annotated outer type parameter. Got: {msg:?}"
        );
    }
}

#[test]
fn overloaded_function_argument_uses_last_signature_for_generic_callback_inference() {
    let source = r#"
interface PromiseLike<T> {
    then<U>(cb: (x: T) => PromiseLike<U>): PromiseLike<U>;
}

declare function testFunction(n: number): PromiseLike<number>;
declare function testFunction(s: string): PromiseLike<string>;

declare var numPromise: PromiseLike<number>;
var newPromise = numPromise.then(testFunction);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "Expected TS2345 when the overloaded argument's last signature fixes U=string. Diagnostics: {diags:#?}"
    );
}

// TODO: higher-order generic inference for compose/map/filter chains doesn't
// correctly propagate type parameters to emit TS2339 for the invalid pipeline.
#[test]
fn higher_order_generic_return_mismatch_preserves_followup_ts2339() {
    let source = r#"
class SetOf<A> {
  _store: A[];

  add(a: A) {
    this._store.push(a);
  }

  transform<B>(transformer: (a: SetOf<A>) => SetOf<B>): SetOf<B> {
    return transformer(this);
  }

  forEach(fn: (a: A, index: number) => void) {
      this._store.forEach((a, i) => fn(a, i));
  }
}

declare function compose<A, B, C, D, E>(
  fnA: (a: SetOf<A>) => SetOf<B>,
  fnB: (b: SetOf<B>) => SetOf<C>,
  fnC: (c: SetOf<C>) => SetOf<D>,
  fnD: (c: SetOf<D>) => SetOf<E>,
): (x: SetOf<A>) => SetOf<E>;
function compose<T>(...fns: ((x: T) => T)[]): (x: T) => T {
  return (x: T) => fns.reduce((prev, fn) => fn(prev), x);
}

function map<A, B>(fn: (a: A) => B): (s: SetOf<A>) => SetOf<B> {
  return (a: SetOf<A>) => {
    const b: SetOf<B> = new SetOf();
    a.forEach(x => b.add(fn(x)));
    return b;
  }
}

function filter<A>(predicate: (a: A) => boolean): (s: SetOf<A>) => SetOf<A> {
  return (a: SetOf<A>) => {
    const result = new SetOf<A>();
    a.forEach(x => {
      if (predicate(x)) result.add(x);
    });
   return result;
  }
}

const testSet = new SetOf<number>();
testSet.add(1);
testSet.add(2);
testSet.add(3);

testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    map(x => x + x),
    map(x => x + "!!!"),
    map(x => x.toUpperCase())
  )
);

testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    map(x => x + x),
    map(x => 123),
    map(x => x.toUpperCase())
  )
);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2339),
        "Expected TS2339 after higher-order generic inference mismatch. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_return_context_preserves_undefined_in_callback_parameter() {
    let source = r#"
declare function match<T>(cb: (value: T) => boolean): T;
const z: number | undefined = match(y => y > 0);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 18048),
        "Expected TS18048 when callback parameter inherits number | undefined. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_return_context_preserves_undefined_through_optional_wrappers() {
    let source = r#"
declare function match<T>(cb: (value: T) => boolean): T;

declare function foo(pos: { x?: number; y?: number }): boolean;
foo({ y: match(y => y > 0) });

declare function foo2(point: [number?]): boolean;
foo2([match(y => y > 0)]);
"#;
    let diags = relevant_diagnostics(source);
    // tsc's `getTypeOfPropertyOfContextualType` returns `T | undefined` for
    // optional properties, so the contextual return type seen by `match<T>`
    // when used as a property value is `number | undefined`. With T inferred
    // to `number | undefined`, the callback parameter `y` is possibly
    // undefined and TS18048 fires inside the body. The tuple `[number?]`
    // exhibits the same behavior at element 0.
    //
    // See `contextuallyTypedOptionalProperty.ts` in the TypeScript conformance
    // suite (issue #55164) for the canonical baseline expecting both errors.
    let ts18048_count = diags.iter().filter(|(code, _)| *code == 18048).count();
    assert_eq!(
        ts18048_count, 2,
        "Expected TS18048 for both optional-wrapper callback sites. Diagnostics: {diags:#?}"
    );
}

#[test]
fn optional_tuple_generic_param_accepts_required_undefined_union_tuple() {
    let source = r#"
declare let tx2: [string | undefined];
declare function f12<T>(x: [T?]): T;
declare function f13<T>(x: Partial<T>): T;
f12(tx2);
f13(tx2);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "Expected no TS2345 for [string | undefined] against [T?]-based generics. Diagnostics: {diags:#?}"
    );
}

#[test]
fn speculative_callback_recheck_drops_stale_property_errors_after_instantiation() {
    let source = r#"
type Mapper<T, U> = (x: T) => U;

declare function wrap<T, U>(cb: Mapper<T, U>): Mapper<T, U>;
declare function combine<A, B, C>(f: (x: A) => B, g: (x: B) => C): (x: A) => C;
declare function useMapper<T, U>(value: T[], cb: Mapper<T, U>): U[];

useMapper(["a", "b"], combine(wrap(s => s.length), wrap(n => n > 10)));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Expected no stale TS2339 after callback recheck narrows `n` to number. Diagnostics: {diags:#?}"
    );
}

#[test]
fn class_tag_contextual_generator_callback_does_not_force_stale_ts2345() {
    let source = r#"
type Covariant<A> = (_: never) => A;

interface Effect<out A, out E = never, out R = never> {
  readonly _A: Covariant<A>;
  readonly _E: Covariant<E>;
  readonly _R: Covariant<R>;
}

declare function effectGen<Eff extends Effect<any, any, any>, AEff>(
  f: () => Generator<Eff, AEff, never>
): Effect<
  AEff,
  [Eff] extends [never]
    ? never
    : [Eff] extends [Effect<infer _A, infer E, infer _R>]
    ? E
    : never,
  [Eff] extends [never]
    ? never
    : [Eff] extends [Effect<infer _A, infer _E, infer R>]
    ? R
    : never
>;

declare function effectFn<
  Eff extends Effect<any, any, any>,
  AEff,
  Args extends Array<any>
>(
  body: (...args: Args) => Generator<Eff, AEff, never>
): (
  ...args: Args
) => Effect<
  AEff,
  [Eff] extends [never]
    ? never
    : [Eff] extends [Effect<infer _A, infer E, infer _R>]
    ? E
    : never,
  [Eff] extends [never]
    ? never
    : [Eff] extends [Effect<infer _A, infer _E, infer R>]
    ? R
    : never
>;

interface Tag<in out Id, in out Value> {
  readonly _op: "Tag";
  readonly Service: Value;
  readonly Identifier: Id;
}

interface TagClassShape<Id, Shape> {
  readonly Type: Shape;
  readonly Id: Id;
}

interface TagClass<Self, Id extends string, Type> extends Tag<Self, Type> {
  new (_: never): TagClassShape<Id, Type>;
  readonly key: Id;
}

declare function layerEffect<I, S, E, R>(
  tag: Tag<I, S>,
  effect: Effect<S, E, R>
): unknown;

declare function Tag<const Id extends string>(
  id: Id
): <Self, Shape>() => TagClass<Self, Id, Shape>;

class Foo extends Tag("Foo")<
  Foo,
  {
    fn: (a: string) => Effect<void>;
  }
>() {}

layerEffect(
  Foo,
  effectGen(function* () {
    return {
      fn: effectFn(function* (a) {
        a;
      }),
    };
  })
);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "Expected no stale TS2345 for class-tag contextual generator callback. Diagnostics: {diags:#?}"
    );
}

#[test]
fn contextual_nested_generator_return_inference_drops_stale_ts2345() {
    let source = r#"
type Covariant<A> = (_: never) => A;

interface Effect<out A, out E = never, out R = never> {
  readonly _A: Covariant<A>;
  readonly _E: Covariant<E>;
  readonly _R: Covariant<R>;
}

declare function effectGen<A, E, R, AEff>(
  f: () => Generator<Effect<A, E, R>, AEff, never>,
): Effect<AEff, E, R>;

declare function effectFn<A, E, R, AEff, Args extends Array<any>>(
  body: (...args: Args) => Generator<Effect<A, E, R>, AEff, never>,
): (...args: Args) => Effect<AEff, E, R>;

interface Tag<in out Id, in out Value> {
  readonly _op: "Tag";
  readonly Service: Value;
  readonly Identifier: Id;
}

interface TagClassShape<Id, Shape> {
  readonly Type: Shape;
  readonly Id: Id;
}

interface TagClass<Self, Id extends string, Type> extends Tag<Self, Type> {
  new (_: never): TagClassShape<Id, Type>;
  readonly key: Id;
}

declare function layerEffect<I, S, E, R>(
  tag: Tag<I, S>,
  effect: Effect<S, E, R>,
): unknown;

declare function Tag<const Id extends string>(
  id: Id,
): <Self, Shape>() => TagClass<Self, Id, Shape>;

class Foo extends Tag("Foo")<
  Foo,
  {
    fn: (a: string) => Effect<void>;
  }
>() {}

layerEffect(
  Foo,
  effectGen(function* () {
    return {
      fn: effectFn(function* (a) {
        a;
      }),
    };
  }),
);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "Expected return-context retry to discard stale TS2345 for the generator callback. Diagnostics: {diags:#?}"
    );
}

#[test]
fn speculative_tuple_listener_recheck_drops_stale_property_errors() {
    let source = r#"
interface CloseEvent {
    code: number;
    wasClean: boolean;
    reason: string;
}

interface ClientEvents {
    warn: [message: string];
    shardDisconnect: [closeEvent: CloseEvent, shardId: number];
}

declare class Client {
    on<K extends keyof ClientEvents>(event: K, listener: (...args: ClientEvents[K]) => void): void;
}

const bot = new Client();
bot.on("shardDisconnect", (event, shard) => {
    event.code;
    event.wasClean;
    event.reason;
    void shard;
});
bot.on("shardDisconnect", event => {
    event.code;
    event.wasClean;
    event.reason;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Expected no stale TS2339 for tuple listener recheck. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_binding_pattern_mismatch_preserves_structured_display() {
    let source = r#"
declare function trans<T>(f: (x: T) => string): number;
trans(({a}) => a);
"#;
    let diags = relevant_diagnostics(source);
    let ts2345_messages: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == 2345)
        .map(|(_, message)| message.clone())
        .collect();
    assert_eq!(
        ts2345_messages.len(),
        1,
        "Expected one TS2345 for binding-pattern mismatch. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2345_messages[0].contains("({ a }: { a: any; }) => any"),
        "Expected structured binding-pattern display. Diagnostics: {diags:#?}"
    );
}

// ─── Constraint-based literal preservation ───────────────────────────

#[test]
fn literal_preserved_when_constraint_is_primitive() {
    // When T extends string, the inferred type should be the literal "hello"
    // (not widened to string)
    let source = r#"
declare function literal<T extends string>(x: T): T;
const result = literal("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Literal should be preserved with primitive constraint. Diagnostics: {diags:#?}"
    );
}

#[test]
fn literal_widened_without_constraint() {
    // Without a constraint, literals should be widened in some contexts
    let source = r#"
declare function id<T>(x: T): T;
const result = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Should compile without errors. Diagnostics: {diags:#?}"
    );
}

// ─── Binding pattern sanitization ────────────────────────────────────

#[test]
fn binding_pattern_param_does_not_pollute_inference() {
    // Object destructuring in callback params should not break inference
    let source = r#"
declare function process<T extends { x: number }>(items: T[], fn: (item: T) => void): void;
process([{ x: 1, y: 2 }], ({ x }) => { const _n: number = x; });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345 && *code != 7031),
        "Binding patterns should not pollute inference. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation with applications ──────────────────────

#[test]
fn application_shape_preserved_in_contextual_type() {
    // When the contextual type is a generic application (e.g., Box<T>),
    // the inferred type should match. Currently Box<T> vs Box<number>
    // mismatch is a known conformance gap — verify it doesn't crash
    // and produces a stable diagnostic.
    let source = r#"
interface Box<T> { value: T; }
declare function wrap<T>(x: T): Box<T>;
const b: Box<number> = wrap(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should instantiate T=number through Box<T>. Diagnostics: {diags:#?}"
    );
}

// ─── Anyish inference detection ──────────────────────────────────────

#[test]
fn any_inferred_type_does_not_suppress_errors() {
    // When inference produces `any`, subsequent type errors should not be suppressed
    let source = r#"
declare function first<T>(arr: T[]): T;
declare const arr: any[];
const result = first(arr);
const _n: number = result;
"#;
    let diags = relevant_diagnostics(source);
    // `result` is `any` from `any[]` input, so assigning to `number` is fine
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "any-inferred result should be assignable to number. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with rest parameters ───────────────────────────────

#[test]
fn rest_parameter_inference_in_generic_call() {
    let source = r#"
declare function concat<T>(...args: T[]): T[];
const result = concat(1, 2, 3);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Rest parameter inference should work. Diagnostics: {diags:#?}"
    );
}

// ─── Enum argument namespace resolution ──────────────────────────────

#[test]
fn enum_as_argument_resolves_to_namespace_for_inference() {
    let source = r#"
enum Direction { Up, Down, Left, Right }
declare function keys<T extends object>(obj: T): (keyof T)[];
const k = keys(Direction);
"#;
    let diags = relevant_diagnostics(source);
    // Should not produce TS2345 for enum passed as object
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "Enum should be usable as object argument. Diagnostics: {diags:#?}"
    );
}

// ─── Callback conditional branch ─────────────────────────────────────

#[test]
fn callback_conditional_branch_used_for_contextual_type() {
    // An unannotated callback whose body is a conditional expression should use
    // the true branch for contextual typing, even when it has contextual params.
    let source = r#"
declare function lazy<T>(fn: (flag: boolean) => T): T;
const result = lazy(flag => flag ? 42 : "hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Zero-param callback conditional should work. Diagnostics: {diags:#?}"
    );
}

// ─── Return-context substitution through structured types ─────────────

#[test]
fn return_context_substitution_through_array() {
    // Return-context collection should walk through array element types
    // to match T[] in the return position against a concrete target.
    let source = r#"
declare function wrap<T>(x: T): T[];
const result: string[] = wrap("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Return context should infer T=string through array type. Diagnostics: {diags:#?}"
    );
}

#[test]
fn return_context_substitution_through_tuple() {
    // Return-context collection should walk through tuple element types
    let source = r#"
declare function pair<T, U>(a: T, b: U): [T, U];
const result: [number, string] = pair(1, "a");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Return context should infer through tuple types. Diagnostics: {diags:#?}"
    );
}

#[test]
fn return_context_substitution_through_generic_application() {
    // Return-context should match Application<T> against Application<concrete>
    // by comparing type arguments when base types match.
    let source = r#"
interface Wrapper<T> { value: T; }
declare function make<T>(x: T): Wrapper<T>;
declare function consume(w: Wrapper<number>): void;
consume(make(42));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match Application<T> against Application<concrete>. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual constraint with self-referential type parameters ──────

#[test]
fn self_referential_constraint_does_not_produce_any_contextual_type() {
    // When T extends Foo<T>, the self-reference should be broken (T → unknown)
    // so the constraint evaluates to a usable contextual type rather than `any`.
    let source = r#"
interface Base<T> { value: T; }
declare function create<T extends Base<T>>(init: (x: T) => void): T;
const result = create((x) => { const _v = x.value; });
"#;
    let diags = relevant_diagnostics(source);
    // The callback param `x` should get a usable contextual type, not `any`
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Self-referential constraint should resolve to usable contextual type. Diagnostics: {diags:#?}"
    );
}

// ─── Widening behavior with const type parameters ─────────────────────

#[test]
fn const_type_parameter_preserves_literal() {
    // `const T` type parameters should always preserve literal types
    // (skip widening even without a primitive constraint).
    let source = r#"
declare function literal<const T>(x: T): T;
const result = literal("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "const type parameter should compile cleanly. Diagnostics: {diags:#?}"
    );
}

#[test]
fn const_type_parameter_unions_across_object_properties() {
    // When `const T` appears in multiple positions within an object type parameter,
    // inference should create a union of all candidates rather than picking the first.
    // Regression: previously the object-property fallback collapsed the union.
    let source = r#"
declare function f5<const T>(obj: { x: T, y: T }): T;
const r1 = f5({ x: 1, y: 2 });
const r2 = f5({ x: { a: 1, b: "x" }, y: { a: 2, b: "y" } });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "const T in multiple object positions should union candidates. Diagnostics: {diags:#?}"
    );
}

#[test]
fn const_type_parameter_unions_across_tuple_positions() {
    // Same as above but with tuple positions.
    let source = r#"
declare function f4<const T>(arr: [T, T]): T;
const r1 = f4([1, 2]);
const r2 = f4([{ a: 1 }, { a: 2 }]);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "const T in multiple tuple positions should union candidates. Diagnostics: {diags:#?}"
    );
}

#[test]
fn const_type_parameter_infers_deep_readonly_literals() {
    let source = r#"
declare function keep<const T>(value: T): T;

const tupleViaConstParam = keep(["a", "b"]);
tupleViaConstParam[0] = "a";
tupleViaConstParam[1] = "z";

const objectViaConstParam = keep({ tag: "ok", nested: { value: 1 } });
objectViaConstParam.tag = "ok";
objectViaConstParam.nested.value = 1;

declare function keepMutable<T extends string[]>(value: T): T;
const mutable = keepMutable(["a", "b"]);
mutable[0] = "z";
"#;
    let diags = relevant_diagnostics(source);
    let ts2540 = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540, 4,
        "const T should infer readonly tuple/object literals while mutable T remains writable. Diagnostics: {diags:#?}"
    );
}

#[test]
fn const_type_parameter_readonly_tuple_rejects_mutable_array_assignment() {
    let source = r#"
declare function readonlyConstraint<const T extends readonly string[]>(value: T): T;
const fromReadonlyConstraint = readonlyConstraint(["a", "b"]);
let mutableArray: string[] = fromReadonlyConstraint;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 4104),
        "const T inferred readonly tuple should not assign to mutable array. Diagnostics: {diags:#?}"
    );
}

#[test]
fn const_type_parameter_mutable_array_constraint_does_not_reject_array_literal() {
    let source = r#"
declare function mutableConstraint<const T extends unknown[]>(value: T): T;
declare function mutableRest<const T extends unknown[]>(...args: T): T;

mutableConstraint(["hello", 42]);
mutableRest("hello", 42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "mutable array constraints on const T should not force readonly argument types. Diagnostics: {diags:#?}"
    );
}

#[test]
fn const_type_parameter_mixed_mutable_readonly_array_constraint_accepts_literals() {
    let source = r#"
declare function mixed<const T extends string[] | readonly number[]>(value: T): T;

mixed(["hello", "world"]);
mixed([1, 2, 3]);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "mixed mutable/readonly array constraints on const T should accept matching array literals. Diagnostics: {diags:#?}"
    );
}

#[test]
fn jsdoc_const_template_infers_deep_readonly_literals() {
    let source = r#"
/**
 * @template const T
 * @param {T} value
 * @returns {T}
 */
function keep(value) { return value; }

const tupleViaConstParam = keep(["a", "b"]);
tupleViaConstParam[0] = "a";

const objectViaConstParam = keep({ tag: "ok", nested: { value: 1 } });
objectViaConstParam.tag = "ok";
objectViaConstParam.nested.value = 1;
"#;
    let diags = relevant_js_diagnostics(source);
    let ts2540 = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540, 3,
        "JSDoc @template const should infer readonly tuple/object literals. Diagnostics: {diags:#?}"
    );
}

// ─── Multiple overload-like generic signatures ────────────────────────

#[test]
fn generic_call_with_union_constraint_infers_correctly() {
    // T extends string | number should allow both string and number arguments
    let source = r#"
declare function coerce<T extends string | number>(x: T): T;
const a = coerce("hello");
const b = coerce(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Union constraint should accept both string and number. Diagnostics: {diags:#?}"
    );
}

// ─── Callable param specificity comparison ────────────────────────────

#[test]
fn more_specific_contextual_type_wins_for_callback() {
    // When two candidate contextual types exist (e.g., from overloads),
    // the one with more non-any parameter types should be preferred.
    let source = r#"
declare function apply<T>(fn: (x: T) => T, value: T): T;
const result = apply((x) => x, 42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Callback should get contextual typing from most specific candidate. Diagnostics: {diags:#?}"
    );
}

// ─── Rest parameter in generic call ───────────────────────────────────

#[test]
fn rest_parameter_contextual_typing_in_callback() {
    // Rest parameters in generic calls should provide correct contextual types
    let source = r#"
declare function apply<T extends any[]>(fn: (...args: T) => void, ...args: T): void;
apply((a, b) => {}, 1, "hello");
"#;
    let diags = relevant_diagnostics(source);
    // Should not produce TS7006 for callback params when rest provides context
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Rest parameter should provide contextual typing for callback. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with return context from union target ───────────────

#[test]
fn return_context_strips_null_undefined_for_substitution() {
    // When the return context target is `T | null | undefined`, the
    // substitution collector should skip null/undefined members and
    // use the non-nullable part for inference.
    let source = r#"
declare function id<T>(x: T): T;
const result: string | null = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Return context should handle nullable union targets. Diagnostics: {diags:#?}"
    );
}

// ─── Iterator info matching in return context ─────────────────────────

#[test]
fn return_context_array_matches_iterable_target() {
    // When source returns T[] but target expects Iterable<concrete>,
    // the return context should extract yield_type from the iterable
    // and match it against the array element type.
    let source = r#"
declare function wrap<T>(x: T): T[];
declare function consume(iter: Iterable<number>): void;
consume(wrap(42));
"#;
    let diags = relevant_diagnostics(source);
    // May or may not produce errors depending on Iterable availability,
    // but should not crash or produce internal errors.
    assert!(
        diags.iter().all(|(code, _)| *code != 0),
        "Array-to-iterable matching should not crash. Diagnostics: {diags:#?}"
    );
}

// ─── Object structural matching in return context ─────────────────────

#[test]
fn return_context_matches_structurally_through_object_properties() {
    // When source returns an application type that evaluates to an object
    // and the target is an already-evaluated object, property types should
    // be matched structurally for return context substitution.
    let source = r#"
interface Config<T> { value: T; label: string; }
declare function config<T>(v: T): Config<T>;
const c: { value: number; label: string } = config(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match structurally through object properties. Diagnostics: {diags:#?}"
    );
}

// ─── Readonly/NoInfer wrapper unwrapping ──────────────────────────────

#[test]
fn application_shape_preserved_through_readonly() {
    // should_preserve_contextual_application_shape should recurse
    // through Readonly<T> wrappers to find application shapes.
    let source = r#"
interface Box<T> { value: T; }
declare function make<T>(x: T): Readonly<Box<T>>;
const result = make(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Readonly wrapper should not break inference. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_blocks_inferred_generic_call_candidates() {
    let source = r#"
declare function choose<T>(value: T, fallback: NoInfer<T>): T;
choose("a", "b");
choose("a", "a");

type NI<T> = NoInfer<T>;
declare function chooseAlias<T>(value: T, fallback: NI<T>): T;
chooseAlias("a", "b");

declare function choosePlain<T>(value: T, fallback: T): T;
choosePlain("a", "b");

choose<"a">("a", "b");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        3,
        "NoInfer fallback positions and explicit type args should reject \"b\", while plain T should infer from both arguments. Diagnostics: {diags:#?}"
    );
}

#[test]
fn explicit_boolean_literal_type_arguments_stay_literal() {
    let source = r#"
declare function id<T>(value: T): T;
id<true>(true);
id<true>(false);
id<false>(false);
id<false>(true);

declare let zero: { <T>(): T };
const zeroTrue: true = zero<true>(true);
const zeroFalse: false = zero<false>(false);

declare let f: { <T>(): T, g<U>(): U };
const inferred = f<true>(true);
const keepTrue: true = inferred;
const rejectFalse: false = inferred;
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert_eq!(
        ts2345.len(),
        2,
        "Explicit true/false type arguments should remain boolean literal types. Diagnostics: {diags:#?}"
    );
    assert!(
        ts2322.len() == 1,
        "Instantiation expression call results should not widen boolean literals. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_blocks_candidates_nested_in_object_properties() {
    let source = r#"
declare function chooseProp<T extends string>(value: T, fallback: { x: NoInfer<T> }): void;
chooseProp("a", { x: "b" });
chooseProp("a", { x: "a" });
"#;
    let diags = relevant_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "NoInfer nested in an object property should block fallback inference and reject only the \"b\" NoInfer property. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_blocks_candidates_nested_in_object_properties_with_lib_intrinsic() {
    let source = r#"
declare function chooseProp<T extends string>(value: T, fallback: { x: NoInfer<T> }): void;
chooseProp("a", { x: "b" });
chooseProp("a", { x: "a" });
"#;
    let diags = relevant_lib_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Lib intrinsic NoInfer nested in an object property should reject only the \"b\" property. Diagnostics: {diags:#?}"
    );
}

// ─── NoInfer with array arguments (issue #6363) ──────────────────────

#[test]
fn noinfer_array_argument_widens_to_primitive() {
    let source = r#"
declare function choose<T>(options: T[], fallback: NoInfer<T>): T;
choose(["a", "b", "c"], "d");
choose(["a", "b", "c"], "a");
choose([1, 2, 3], 4);
choose([true, false], true);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "array literal widens T to primitive so NoInfer fallback passes. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_array_single_element_widens_to_primitive() {
    let source = r#"
declare function choose<T>(options: T[], fallback: NoInfer<T>): T;
choose(["a"], "b");
choose([1], 2);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "single-element array widens to primitive for NoInfer fallback. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_array_renamed_type_param_widens_to_primitive() {
    let source = r#"
declare function pick<U>(candidates: U[], default_value: NoInfer<U>): U;
pick(["x", "y", "z"], "w");
pick([10, 20], 30);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "widening is not name-sensitive: different type-param name. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_scalar_literal_still_preserved() {
    let source = r#"
declare function choose<T>(value: T, fallback: NoInfer<T>): T;
choose("a", "b");
choose("a", "a");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "scalar direct argument keeps literal narrow; NoInfer fallback rejects mismatch. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_array_alias_widens_to_primitive() {
    let source = r#"
type NI<T> = NoInfer<T>;
declare function choose<T>(options: T[], fallback: NI<T>): T;
choose(["foo", "bar"], "baz");
choose([1, 2], 3);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "NoInfer via type alias still widens array-inferred T to primitive. Diagnostics: {diags:#?}"
    );
}

// ─── Inference with multiple callbacks ────────────────────────────────

#[test]
fn multiple_callback_params_all_get_contextual_types() {
    // When a generic function has multiple callback parameters,
    // all should receive contextual types from the inferred type arguments.
    let source = r#"
declare function bimap<T, U, V>(
    arr: T[],
    first: (x: T) => U,
    second: (x: T) => V
): [U[], V[]];
const result = bimap([1, 2, 3], x => x + 1, x => String(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "All callbacks should receive contextual types. Diagnostics: {diags:#?}"
    );
}

// ─── Explicit type arguments bypass inference ─────────────────────────

#[test]
fn explicit_type_arguments_provide_callback_context() {
    // When type arguments are explicitly provided, callback params
    // should be contextually typed from those explicit types.
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map<number, string>([1, 2], x => String(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006 && *code != 2345),
        "Explicit type args should provide callback context. Diagnostics: {diags:#?}"
    );
}

// ─── Nested generic calls ─────────────────────────────────────────────

#[test]
fn nested_generic_calls_propagate_inference() {
    // Generic inference should work through nested generic calls
    let source = r#"
declare function id<T>(x: T): T;
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map([1, 2, 3], x => id(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Nested generic calls should propagate inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic inference with default type parameters ───────────────────

#[test]
fn default_type_parameter_used_when_not_inferable() {
    // When a type parameter has a default and cannot be inferred,
    // the default should be used.
    let source = r#"
declare function create<T = string>(value?: T): T;
const result = create();
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Default type parameter should be used when not inferable. Diagnostics: {diags:#?}"
    );
}

#[test]
fn default_type_parameter_substitutes_inside_conditional_constraint() {
    // Regression for issue #6559: using Chainable without explicit type
    // arguments must substitute Config = {} inside the nested conditional
    // constraint for option's key parameter.
    let source = r#"
type Chainable<Config = {}> = {
  option<K extends string>(
    key: K extends keyof Config ? never : K,
    value: number
  ): void;
};

declare const explicit: Chainable<{}>;
explicit.option('foo', 123);

declare const defaulted: Chainable;
defaulted.option('foo', 123);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Default type parameter should substitute into conditional constraint. Diagnostics: {diags:#?}"
    );
}

#[test]
fn contextual_return_instantiates_defaulted_generic_call_result() {
    let source = r#"
interface Box<T> { value: T | undefined }
declare function make<O>(p: { value?: O }): Box<O>
const x: Box<string> = make({})
function f<T>(): Box<T> { return make({}) }
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Contextual generic return inference should instantiate the call result. Diagnostics: {diags:#?}"
    );
}

#[test]
fn result_union_false_branch_infers_never_and_error_type() {
    let source = r#"
type Result<T, E = unknown> =
  | { ok: true; value: T }
  | { ok: false; error: E };

function failure<E = unknown>(error: E): Result<never, E> {
  return { ok: false, error };
}

function handle<T, E>(result: Result<T, E>): T | E {
  return result.ok ? result.value : result.error;
}

const viaInline: never | string = handle(failure("error"));
const result = failure("error");
const viaAlias: never | string = handle(result);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Result-like false branch should infer T=never and E=string. Diagnostics: {diags:#?}"
    );
}

#[test]
fn renamed_result_union_false_branch_inference_is_structural() {
    let source = r#"
type Outcome<A, B> =
  | { tag: "some"; data: A }
  | { tag: "none"; problem: B };

function miss<B>(problem: B): Outcome<never, B> {
  return { tag: "none", problem };
}

function unwrap<A, B>(outcome: Outcome<A, B>): A | B {
  return outcome.tag === "some" ? outcome.data : outcome.problem;
}

const outcome = miss("missing");
const value: never | string = unwrap(outcome);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Branch inference must not depend on Result/ok/error spellings. Diagnostics: {diags:#?}"
    );
}

// ─── Callable shape sanitization with overloads ───────────────────────

#[test]
fn callable_binding_pattern_param_sanitization_single_signature() {
    // A callable argument with a destructured param should not break inference.
    // The callable shape's call signature params at binding-pattern positions
    // are replaced with `unknown` to avoid polluting the inference constraint.
    let source = r#"
declare function apply<T extends { a: number; b: string }>(
    items: T[],
    fn: (item: T) => void
): void;
apply([{ a: 1, b: "x" }], ({ a, b }) => {
    const _n: number = a;
    const _s: string = b;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345 && *code != 7031),
        "Callable shape sanitization should not break inference. Diagnostics: {diags:#?}"
    );
}

#[test]
fn callable_binding_pattern_does_not_leak_unknown_into_inferred_type() {
    // When a callback destructures its parameter, the inferred type for the
    // generic should still be correct (not unknown).
    let source = r#"
declare function first<T>(arr: T[], fn: (item: T) => boolean): T;
const result = first([1, 2, 3], (item) => item > 0);
const check: number = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Inferred T should still be number. Diagnostics: {diags:#?}"
    );
}

#[test]
fn conditional_alias_first_arg_context_types_binding_pattern_callback() {
    let source = r#"
interface TypeLambda {
    readonly In: unknown;
    readonly Out: unknown;
}
type Kind<F extends TypeLambda, In, Target> = F extends { readonly type: unknown }
    ? (F & { readonly In: In; readonly Target: Target })["type"]
    : { readonly F: F; readonly In: (_: In) => void; readonly Target: (_: Target) => Target };

declare const map: <F extends TypeLambda, R, A, B>(
    self: Kind<F, R, A>,
    f: (a: A) => B
) => Kind<F, R, B>;

declare const pair: <F extends TypeLambda, R, A, B>(
    left: Kind<F, R, A>,
    right: Kind<F, R, B>
) => Kind<F, R, [A, B]>;

function use<F extends TypeLambda, R, A, B>(
    left: Kind<F, R, A>,
    right: Kind<F, R, B>,
    f: (a: A, b: B) => string
): Kind<F, R, string> {
    return map(pair(left, right), ([a, b]) => f(a, b));
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| matches!(*code, 2345 | 7031)),
        "conditional alias inference should type destructured callback params. Diagnostics: {diags:#?}"
    );
}

#[test]
fn overloaded_conditional_alias_first_arg_context_types_binding_pattern_callback() {
    let source = r#"
interface TypeLambda {
    readonly In: unknown;
    readonly Out: unknown;
}
type Kind<F extends TypeLambda, In, Target> = F extends { readonly type: unknown }
    ? (F & { readonly In: In; readonly Target: Target })["type"]
    : { readonly F: F; readonly In: (_: In) => void; readonly Target: (_: Target) => Target };

interface Covariant<F extends TypeLambda> {
    readonly map: {
        <A, B>(f: (a: A) => B): <R>(self: Kind<F, R, A>) => Kind<F, R, B>;
        <R, A, B>(self: Kind<F, R, A>, f: (a: A) => B): Kind<F, R, B>;
    };
}
interface Product<F extends TypeLambda> extends Covariant<F> {
    readonly pair: <R, A, B>(
        left: Kind<F, R, A>,
        right: Kind<F, R, B>
    ) => Kind<F, R, [A, B]>;
}

function use<F extends TypeLambda, R, A, B>(
    F: Product<F>,
    left: Kind<F, R, A>,
    right: Kind<F, R, B>,
    f: (a: A, b: B) => string
): Kind<F, R, string> {
    return F.map(F.pair(left, right), ([a, b]) => f(a, b));
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| matches!(*code, 2345 | 7031)),
        "overloaded conditional alias inference should type destructured callback params. Diagnostics: {diags:#?}"
    );
}

#[test]
fn overloaded_higher_order_rest_any_constraint_accepts_generic_body() {
    let source = r#"
type Parameters<T extends (...args: any[]) => any> =
    T extends (...args: infer P) => any ? P : never;
interface IArguments {}

declare const dual: {
    <DataLast extends (...args: any[]) => any, DataFirst extends (...args: any[]) => any>(
        arity: Parameters<DataFirst>["length"],
        body: DataFirst
    ): DataLast & DataFirst;
    <DataLast extends (...args: any[]) => any, DataFirst extends (...args: any[]) => any>(
        isDataFirst: (args: IArguments) => boolean,
        body: DataFirst
    ): DataLast & DataFirst;
};

const make = (): {
    <A, B, C>(a: A, b: B, f: (a: A, b: B) => C): C;
} =>
    dual(3, <A, B, C>(a: A, b: B, f: (a: A, b: B) => C): C => f(a, b));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2769),
        "higher-order generic body should satisfy the rest-any function constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation edge cases ──────────────────────────────

#[test]
fn contextual_instantiation_through_readonly() {
    // Return context substitution should unwrap Readonly<T> when matching
    let source = r#"
declare function wrap<T>(value: T): Readonly<T>;
const result: Readonly<string> = wrap("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match through Readonly. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_call_with_union_return_context() {
    // Return context substitution should handle union target types
    let source = r#"
declare function id<T>(x: T): T;
const result: string | number = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should work with union target. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_call_application_matching_in_return_context() {
    // When source and target are both applications of the same generic,
    // their type arguments should be matched structurally.
    let source = r#"
interface Container<T> { value: T; }
declare function box_it<T>(value: T): Container<T>;
const result: Container<number> = box_it(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Application matching should work in return context. Diagnostics: {diags:#?}"
    );
}

#[test]
fn returned_function_parameters_keep_same_application_return_context() {
    let source = r#"
type Mapper<T, U> = (x: T) => U;
declare function wrap<T, U>(cb: Mapper<T, U>): Mapper<T, U>;
declare function arrayize<T, U>(cb: Mapper<T, U>): Mapper<T, U[]>;
declare function combine<A, B, C>(f: (x: A) => B, g: (x: B) => C): (x: A) => C;
declare function foo(f: Mapper<string, number>): void;
declare const strings: { map<U>(cb: (x: string, index: number, array: string[]) => U): U[] };
declare function identity<T>(x: T): T;

let f3: Mapper<string, number[]> = arrayize(wrap(s => s.length));
let f4: Mapper<string, boolean> = combine(wrap(s => s.length), wrap(n => n >= 10));
foo(wrap(s => s.length));
let a4 = strings.map(combine(wrap(s => s.length), wrap(n => n > 10)));
let a5 = strings.map(combine(identity, wrap(s => s.length)));
let a6 = strings.map(combine(wrap(s => s.length), identity));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Returned function parameter type should be preserved for same-application return context. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation through intersections ────────────────────

#[test]
fn generic_callback_in_intersection_parameter() {
    // When a generic parameter type is an intersection involving a callback,
    // inference should still provide contextual types for callback params.
    let source = r#"
declare function register<T>(value: T, handler: (x: T) => void): void;
register(42, (x) => {
    const _n: number = x;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Intersection callback should get contextual type. Diagnostics: {diags:#?}"
    );
}

// ─── Generic inference with mapped type return ──────────────────────────

#[test]
fn generic_inference_with_conditional_return() {
    // Generic inference should work when return type involves a conditional
    let source = r#"
declare function check<T>(x: T): T extends string ? true : false;
const result = check("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Conditional return type should not break inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with spread arguments ─────────────────────────────────

#[test]
fn generic_call_infers_from_spread_args() {
    // Spread arguments should participate in generic inference
    let source = r#"
declare function first<T>(arr: T[]): T;
const arr = [1, 2, 3];
const result = first(arr);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Spread args should participate in inference. Diagnostics: {diags:#?}"
    );
}

// ─── Widening with number literal constraint ────────────────────────────

#[test]
fn number_literal_constraint_preserves_literal_type() {
    // T extends number should preserve the literal 42, not widen to number
    let source = r#"
declare function num<T extends number>(x: T): T;
const result = num(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Number literal should be preserved with number constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Boolean literal constraint ──────────────────────────────────────────

#[test]
fn boolean_literal_constraint_preserves_literal_type() {
    let source = r#"
declare function bool<T extends boolean>(x: T): T;
const result = bool(true);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Boolean literal should be preserved with boolean constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual type propagation through Promise-like ────────────────────

#[test]
fn generic_call_with_promise_return_context() {
    // Return context from a Promise<T> target should propagate T inference
    let source = r#"
declare function resolve<T>(x: T): Promise<T>;
async function test() {
    const result: Promise<string> = resolve("hello");
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Promise return context should propagate inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call recheck with real types ────────────────────────────────

#[test]
fn recheck_generic_call_detects_argument_mismatch_after_inference() {
    // After inference resolves T, rechecking should catch argument mismatches
    let source = r#"
declare function map<T>(arr: T[], fn: (x: T) => T): T[];
const result = map([1, 2, 3], (x) => "not a number");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322 || *code == 2345),
        "Recheck should detect type mismatch after inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with optional parameter ────────────────────────────────

#[test]
fn generic_inference_with_optional_params() {
    let source = r#"
declare function opt<T>(required: T, optional?: T): T;
const result = opt("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Optional params should not break generic inference. Diagnostics: {diags:#?}"
    );
}

// ─── Multiple constraints interacting ────────────────────────────────────

#[test]
fn generic_with_extends_keyof_constraint() {
    // T extends keyof U should constrain T to string literal union keys of U
    let source = r#"
declare function pick<U, T extends keyof U>(obj: U, key: T): U[T];
const result = pick({ a: 1, b: "hello" }, "a");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "keyof constraint should work with inference. Diagnostics: {diags:#?}"
    );
}

// ─── Recursive generic callback ──────────────────────────────────────────

#[test]
fn recursive_generic_callback_does_not_stack_overflow() {
    // Self-referential constraints should not cause stack overflow
    let source = r#"
interface Tree<T> { value: T; children: Tree<T>[]; }
declare function traverse<T>(tree: Tree<T>, fn: (node: Tree<T>) => void): void;
declare const tree: Tree<number>;
traverse(tree, (node) => {
    const _v: number = node.value;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "Recursive generic callback should not overflow. Diagnostics: {diags:#?}"
    );
}

// ─── TS2454 does not suppress downstream type errors ────────────────

#[test]
fn ts2454_does_not_suppress_ts2322_on_generic_constraint() {
    // When a variable is used before assignment (TS2454), tsc still type-checks
    // the expression using the declared type. Property-level mismatches like
    // TS2322 must still be emitted alongside TS2454.
    // Regression: genericConstraintSatisfaction1.ts
    let source = r#"
interface I<S> {
   f: <T extends S>(x: T) => void
}

var x: I<{s: string}>
declare var x: I<{s: string}>
x.f({s: 1})
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2454),
        "Should emit TS2454 for variable used before assignment. Got: {diags:#?}"
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Should also emit TS2322 for property type mismatch. Got: {diags:#?}"
    );
}

#[test]
fn dependent_type_parameter_constraint_checks_second_argument_against_first_inference() {
    // Regression: typeParameterAsTypeParameterConstraint2.ts
    // For <T, U extends T>, tsc fixes T from the first argument and then
    // validates the second argument's inferred U against that T.
    let source = r#"
interface NumberVariant {
    x: number;
}

var n: NumberVariant;
function foo<T, U extends T>(x: T, y: U): U { return y; }
foo(1, n);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2454),
        "Should emit TS2454 for variable used before assignment. Got: {diags:#?}"
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2345 && message.contains("NumberVariant") && message.contains("number")
        }),
        "Should also emit TS2345 for NumberVariant not assignable to number. Got: {diags:#?}"
    );
}

#[test]
fn ts2454_does_not_suppress_property_access_errors() {
    // Even with TS2454, property accesses on the declared type should
    // still produce type errors when used in incompatible contexts.
    let source = r#"
var x: {s: string}
x.f({s: 1})
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2454),
        "Should emit TS2454. Got: {diags:#?}"
    );
    // x.f doesn't exist on {s: string}, so TS2339 should fire
    assert!(
        diags.iter().any(|(code, _)| *code == 2339),
        "Should also emit TS2339 for missing property. Got: {diags:#?}"
    );
}

// ─── Union type predicates must not narrow when non-predicate members
//     return general boolean ──────────────────────────────────────────

#[test]
fn union_this_predicate_with_boolean_member_does_not_narrow() {
    // Regression: typePredicatesInUnion3.ts
    // When a union method has a `this` type predicate on one member and plain
    // boolean on another, the call is NOT a type predicate. The receiver must
    // NOT be narrowed.
    let source = r#"
type HasAttribute<T> = T & { attribute: number };

class Type1 {
    attribute: number | null = null;
    predicate(): this is HasAttribute<Type1> {
        return true;
    }
}

class Type2 {
    attribute: number | null = null;
    predicate(): boolean {
        return true;
    }
}

function assertType<T>(_val: T) {
}

declare const val: Type1 | Type2;

if (val.predicate()) {
    assertType<number>(val.attribute);  // Error: number | null not assignable to number
}
"#;
    let diags = compile_and_get_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "Should emit TS2345 because val is not narrowed by union predicate. Got: {diags:#?}"
    );
}

#[test]
fn this_predicate_union_with_false_returning_member_narrows() {
    // When ALL non-predicate union members return exclusively `false`,
    // the union IS a valid type predicate. The predicate narrows the
    // receiver and non-predicate members are impossible in the true branch.
    let source = r#"
class Entry {
    c: number = 1;
    guard(): this is Entry { return true; }
}
class Group {
    d: string = "no";
    guard(): false { return false; }
}
declare var chunk: Entry | Group;
let x = chunk.guard() ? chunk.c : chunk.d;
"#;
    let diags = compile_and_get_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2339),
        "Should NOT emit TS2339 - chunk.c should be accessible after guard(). Got: {diags:#?}"
    );
}

// ─── Empty object type ({}) in BCT inference ────────────────────────

#[test]
fn bct_inference_recognizes_empty_object_as_supertype_of_primitives() {
    // When inference candidates include primitives and `{}`, the BCT tournament
    // must recognize `{}` as a supertype of non-nullable primitives. This ensures
    // `{}` is not dropped from inference results due to first-wins tournament logic.
    // Repro: ReadonlyArray<T> inference from union arrays containing `{}[]`.
    let source = r#"
declare function foo<T>(x: ReadonlyArray<T>): T;
declare const a: (string | number)[] | null[] | {}[];
let x = foo(a);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "ReadonlyArray<T> inference from union of arrays with {{}} should work. Got: {diags:#?}"
    );
}

#[test]
fn bivariant_inference_this_parameter_union_of_arrays() {
    // Repro from TypeScript #27337: calling a method with a generic `this`
    // parameter on a union of arrays should infer T from all union members.
    // The empty object type `{}` must be recognized as a supertype of
    // primitives in the BCT tournament to avoid false TS2684.
    let source = r#"
interface Array<T> {
    equalsShallow<T>(this: ReadonlyArray<T>, other: ReadonlyArray<T>): boolean;
}
declare const a: (string | number)[] | null[] | undefined[] | {}[];
declare const b: (string | number)[] | null[] | undefined[] | {}[];
let x = a.equalsShallow(b);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2684),
        "Method with generic this on union of arrays should not emit TS2684. Got: {diags:#?}"
    );
}

#[test]
fn union_this_type_in_functions_emits_ts2684() {
    // unionThisTypeInFunctions conformance test: calling a method with `this: this`
    // on a union type where members have incompatible `data` properties.
    // The `this` context is Real | Fake, but the method requires Real & Fake.
    let source = r#"
interface Real {
    method(this: this, n: number): void;
    data: string;
}
interface Fake {
    method(this: this, n: number): void;
    data: number;
}
function test(r: Real | Fake) {
    r.method(12);
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2684),
        "Should emit TS2684 for union this type mismatch. Got: {diags:#?}"
    );
}

#[test]
fn ts2684_union_method_this_message_uses_interface_names_not_outer_function() {
    // Regression for an ID-conflation bug in the property-access `this`
    // binder: when the noop TypeResolver couldn't translate an interface's
    // SymbolId to a DefId, `nominalize_object_receiver` fell back to
    // `interner.reference(SymbolRef(sym_id.0))`, which created a
    // `Lazy(DefId(sym_id.0))`. Because `SymbolId.0` and `DefId.0` are
    // independent ID spaces, this produced a Lazy that pointed at an
    // *unrelated* declaration (e.g., the enclosing `test` function), so the
    // TS2684 message rendered as `Real & test` (or `Fake & test`) instead of
    // `Real & Fake`. The fix: keep the original Object receiver when no
    // DefId mapping exists, so the formatter can recover the interface name
    // through `shape.symbol`.
    let source = r#"
interface Real {
    method(this: this, n: number): void;
    data: string;
}
interface Fake {
    method(this: this, n: number): void;
    data: number;
}
function test(r: Real | Fake) {
    r.method(12);
}
"#;
    let diags = relevant_diagnostics(source);
    let ts2684 = diags
        .iter()
        .find(|(code, _)| *code == 2684)
        .expect("expected TS2684 diagnostic");
    let msg = &ts2684.1;
    // The expected `this` should display as `Real & Fake` (interface names),
    // not as `Fake & test` or `Lazy(N) & Lazy(M)`.
    assert!(
        msg.contains("'Real & Fake'") || msg.contains("'Fake & Real'"),
        "TS2684 message should reference both interface names, not the outer function. Got: {msg}"
    );
    assert!(
        !msg.contains("test'"),
        "TS2684 message must not leak the enclosing function name `test`. Got: {msg}"
    );
    assert!(
        !msg.contains("Lazy("),
        "TS2684 message must not leak `Lazy(N)` placeholders. Got: {msg}"
    );
}

// ─── Higher-order generic contextual types (compose/flip patterns) ──────

#[test]
fn compose_with_naked_generic_function_arguments() {
    // compose(list, box) should infer <T>(x: T) => Box<T[]>
    // when assigned to a variable with that generic function annotation.
    // Source type params (T in list, V in box) appear directly (naked) as
    // parameter types, enabling proper higher-order inference.
    let source = r#"
type Box<T> = { value: T };
declare function compose<A, B, C>(f: (a: A) => B, g: (b: B) => C): (a: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): Box<V>;
const f11: <T>(x: T) => Box<T[]> = compose(list, box);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "compose(list, box) with generic contextual type should not error. Got: {diags:#?}"
    );
}

#[test]
fn compose_with_wrapped_generic_function_arguments() {
    // compose(unbox, unlist) should infer <T>(x: Box<T[]>) => T
    // when assigned to a variable with that generic function annotation.
    // Source type params (W in unbox, T in unlist) appear inside wrapper
    // types (Box<W>, T[]), requiring the contextual type to drive inference.
    let source = r#"
type Box<T> = { value: T };
declare function compose<A, B, C>(f: (a: A) => B, g: (b: B) => C): (a: A) => C;
declare function unbox<W>(x: Box<W>): W;
declare function unlist<T>(a: T[]): T;
const f13: <T>(x: Box<T[]>) => T = compose(unbox, unlist);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "compose(unbox, unlist) with generic contextual type should not error. Got: {diags:#?}"
    );
}

#[test]
fn flip_with_generic_function_argument() {
    // flip(zip) should infer <A, B>(b: B, a: A) => [A, B]
    // when assigned to a variable with that generic function annotation.
    let source = r#"
declare function zip<A, B>(a: A, b: B): [A, B];
declare function flip<X, Y, Z>(f: (x: X, y: Y) => Z): (y: Y, x: X) => Z;
const f40: <A, B>(b: B, a: A) => [A, B] = flip(zip);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "flip(zip) with generic contextual type should not error. Got: {diags:#?}"
    );
}

#[test]
fn non_inferrable_type_propagation_not_broken() {
    // Regression guard: filter(exists(...)) in a pipe should not produce
    // false TS2345 errors. The generic function result from exists() has
    // non-naked type params that should be erased during inference.
    let source = r#"
interface Predicate<A> { (a: A): boolean }
interface Left<E> { readonly _tag: 'Left'; readonly left: E }
interface Right<A> { readonly _tag: 'Right'; readonly right: A }
type Either<E, A> = Left<E> | Right<A>;
declare const filter: {
    <A, B extends A>(refinement: { (a: A): a is B }): (as: ReadonlyArray<A>) => ReadonlyArray<B>
    <A>(predicate: Predicate<A>): <B extends A>(bs: ReadonlyArray<B>) => ReadonlyArray<B>
    <A>(predicate: Predicate<A>): (as: ReadonlyArray<A>) => ReadonlyArray<A>
};
declare function pipe<A, B>(a: A, ab: (a: A) => B): B;
declare function exists<A>(predicate: Predicate<A>): <E>(ma: Either<E, A>) => boolean;
declare const es: Either<string, number>[];
const x = pipe(es, filter(exists((n) => n > 0)));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "pipe(es, filter(exists(...))) should not produce TS2345. Got: {diags:#?}"
    );
}

#[test]
fn overloaded_pipe_return_context_types_chained_callback_params() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function pipe<A extends any[], B, C, D>(ab: (...args: A) => B, bc: (b: B) => C, cd: (c: C) => D): (...args: A) => D;
type Fn = (n: number) => number;
const fn30: Fn = pipe(
    x => x + 1,
    x => x * 2,
);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2362),
        "pipe return context should type chained callback parameters before checking arithmetic. Got: {diags:#?}"
    );
}

#[test]
fn curried_map_identity_preserves_array_element_type() {
    let source = r#"
interface Array<T> { map<U>(cb: (value: T) => U): U[]; }
declare const identity: <T>(value: T) => T;
declare function map<T, U>(transform: (t: T) => U): (arr: T[]) => U[];
const arr1: string[] = map(identity)(['a']);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("string[]")),
        "map(identity)(['a']) should preserve string[] assignability. Got: {diags:#?}"
    );
}

#[test]
fn pipe_preserves_self_constrained_generic_function_result() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function foo<T extends { value: T }>(x: T): T;

const g10: <T extends { value: T }>(x: T) => T = pipe(foo);
const g12: <T extends { value: T }>(x: T) => T = pipe(foo, foo);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322 || *code == 2345),
        "pipe(foo) should preserve the self-constrained generic signature without stale argument errors. Got: {diags:#?}"
    );
}

#[test]
fn pipe_preserves_generic_component_hoc_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;

type Component<P> = (props: P) => {};
declare const myHoc1: <P>(C: Component<P>) => Component<P>;
declare const myHoc2: <P>(C: Component<P>) => Component<P>;
declare const MyComponent1: Component<{ foo: 1 }>;

const enhance = pipe(myHoc1, myHoc2);
const MyComponent2 = enhance(MyComponent1);
const Preserved: Component<{ foo: 1 }> = MyComponent2;
const Wrong: Component<{ foo: 2 }> = MyComponent2;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "pipe(myHoc1, myHoc2) should preserve the component props type through the returned HOC. Got: {diags:#?}"
    );
    let ts2322_count = diags.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "the returned HOC should reject incompatible props exactly once, proving props were not erased to unknown. Got: {diags:#?}"
    );
}

#[test]
fn pipe_contextual_return_flows_through_generic_function_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const g01: <T>(x: T) => { value: T[] } = pipe(list, box);
const g02: <T>(x: T) => { value: T }[] = pipe(box, list);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2345),
        "pipe(list, box) and pipe(box, list) should infer the intermediate generic argument from the contextual return. Got: {diags:#?}"
    );
}

#[test]
fn pipe_contextual_return_flows_through_lambda_and_generic_function_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const g05: <T>(x: T) => { value: T[] } = pipe(x => list(x), x => box(x));
const inferred = pipe(x => list(x), x => box(x));
const keep: { value: 1[] } = inferred(1 as 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2345),
        "pipe lambdas should inherit contextual return bounds through the generic calls they wrap. Got: {diags:#?}"
    );
}

#[test]
fn pipe_contextual_return_flows_through_nested_generic_call_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const g06: <T>(x: T) => { value: T[] } = pipe(list, pipe(box));
const g07: <T>(x: T) => { value: T[] } = pipe(x => list(x), pipe(box));
const inferred1 = pipe(list, pipe(box));
const inferred2 = pipe(x => list(x), pipe(box));
const keep1: { value: 1[] } = inferred1(1 as 1);
const keep2: { value: 1[] } = inferred2(1 as 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2345),
        "nested pipe calls should use the outer callable context to specialize the inner generic call. Got: {diags:#?}"
    );
}

#[test]
fn generic_function_rest_type_param_target_keeps_return_mismatch() {
    let source = r#"
declare function accepts<A extends any[]>(fn: (...args: A) => string): void;
declare function returnsNumber<T>(x: T): number;

accepts(returnsNumber);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("string")),
        "generic functions passed to rest type-parameter targets must still reject real return mismatches. Got: {diags:#?}"
    );
}

#[test]
fn generic_function_identifier_instantiates_against_fixed_tuple_rest_target() {
    let source = r#"
function callr<T extends unknown[], U>(args: T, f: (...args: T) => U) {
    return f(...args);
}

declare const sn: [string, number];
declare function choose<A, B>(a: A, b: B): A | B;

let value = callr(sn, choose);
let check: string | number = value;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2345),
        "generic function identifiers should infer from fixed tuple-rest parameters before return-context refinement. Got: {diags:#?}"
    );
}

#[test]
fn generic_function_identifier_fixed_tuple_rest_keeps_constraint_mismatch() {
    let source = r#"
function callr<T extends unknown[], U>(args: T, f: (...args: T) => U) {
    return f(...args);
}

declare const sn: [string, number];
declare function numberPair<A extends number, B extends number>(a: A, b: B): A | B;

let value = callr(sn, numberPair);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "fixed tuple-rest refinement must still reject constrained generic parameter mismatches. Got: {diags:#?}"
    );
}

#[test]
fn pipe_nested_generic_call_keeps_parameter_mismatches() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function boxNumbers(x: number[]): { value: number[] };

const bad: <T>(x: T) => { value: T[] } = pipe(list, pipe(boxNumbers));
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322 || *code == 2345),
        "nested generic call contextual typing must not erase real parameter mismatches. Got: {diags:#?}"
    );
}

#[test]
fn return_context_refresh_keeps_callback_marker_context() {
    let source = r#"
type Values<T> = T[keyof T];
type EventObject = { type: string };

interface ActorLogic<TEvent extends EventObject> {
  transition: (ev: TEvent) => unknown;
}

type UnknownActorLogic = ActorLogic<never>;

interface ProvidedActor {
  src: string;
  logic: UnknownActorLogic;
}

interface ActionFunction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface AssignAction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface MachineConfig<TActor extends ProvidedActor> {
  entry?: ActionFunction<TActor>;
}

declare function assign<TActor extends ProvidedActor>(
  _: (spawn: (actor: TActor["src"]) => void) => {},
): AssignAction<TActor>;

type ToProvidedActor<TActors extends Record<string, UnknownActorLogic>> =
  Values<{
    [K in keyof TActors & string]: {
      src: K;
      logic: TActors[K];
    };
  }>;

declare function createMachineFactory<
  TActors extends Record<string, UnknownActorLogic>,
>(actors: TActors): {
  createMachine: <
    const TConfig extends MachineConfig<ToProvidedActor<TActors>>,
  >(
    config: TConfig,
  ) => void;
};

declare const counterLogic: ActorLogic<{ type: "INCREMENT" }>;

createMachineFactory({
  counter: counterLogic,
}).createMachine({
  entry: assign((spawn) => {
    spawn("counter");
    spawn("alarm");
    return {};
  }),
});
"#;
    let diags = relevant_strict_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "return-context refresh should preserve the marker-property context for the nested callback. Got: {diags:#?}"
    );
    assert!(
        ts2345[0].1.contains("\"alarm\"") && ts2345[0].1.contains("\"counter\""),
        "the callback parameter should stay narrowed to the contextual actor source. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_argument_preserves_inferred_props() {
    let source = r#"
declare class Comp<P> {
    props: P;
    constructor(props: P);
}

type CompClass<P> = new (props: P) => Comp<P>;
declare function myHoc<P>(C: CompClass<P>): CompClass<P>;
type GenericProps<T> = { foo: number, stuff: T };
declare class GenericComp<T> extends Comp<GenericProps<T>> {}
declare class StringComp extends Comp<GenericProps<string>> {}

const GenericComp2 = myHoc(GenericComp);
const StringComp2 = myHoc(StringComp);
const madeString = new StringComp2({ foo: 1, stuff: "ok" });
const keepString: string = madeString.props.stuff;
const wrongString: number = madeString.props.stuff;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "generic class constructor arguments should preserve their props inference through constructor HOFI. Got: {diags:#?}"
    );
    let ts2322_count = diags.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "instantiating the returned constructor should preserve `stuff: string` and reject assignment to number exactly once. Got: {diags:#?}"
    );
}

#[test]
fn generic_static_factory_constructor_infers_method_type_parameter() {
    let source = r#"
class Container<T> {
    private value: T;
    constructor(value: T) { this.value = value; }

    static of<U>(value: U): Container<U> {
        return new Container(value);
    }
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "constructor inference inside a generic static method should preserve the method type parameter. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_options_infer_from_context_sensitive_object_member_return() {
    let source = r#"
declare class Connection {
    ok(): void;
}

declare class Pending<R> {
    promise: Promise<R>;
}

interface PoolOptions<R> {
    create: () => R | Promise<R>;
    destroy: (resource: R) => void;
    validate?: (resource: R) => boolean;
}

declare class Pool<R> {
    constructor(options: PoolOptions<R>);
    acquire(): Pending<R>;
    release(resource: R): void;
}

declare const tarn: {
    Pool: typeof Pool;
};

const pool = new tarn.Pool({
    create: async () => new Connection(),
    destroy: (connection) => {
        connection.ok();
    },
    validate: (connection) => true,
});

const keep: Pending<Connection> = pool.acquire();
const reject: Pending<string> = pool.acquire();
"#;
    let diags = relevant_strict_default_lib_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "generic constructor options should infer callback parameter types during Round 2. Got: {diags:#?}"
    );
    let ts2322_count = diags.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Pool should infer R = Connection from create(), accept Connection assignment, and reject string assignment exactly once. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_options_infer_through_method_signature_and_omit_spread() {
    let source = r#"
declare class Connection {
    ok(): void;
}

declare class Pending<R> {
    promise: Promise<R>;
}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

interface PoolOptions<R> {
    create(cb: (err: Error | null, resource: R) => void): any | (() => Promise<R>);
    destroy(resource: R): any;
    validate?(resource: R): boolean;
    min: number;
    max: number;
}

declare class Pool<R> {
    constructor(options: PoolOptions<R>);
    acquire(): Pending<R>;
}

declare const tarn: {
    options: Omit<PoolOptions<any>, "create" | "destroy" | "validate"> & {
        validateConnections?: false;
    };
    Pool: typeof Pool;
};

const { validateConnections, ...poolOptions } = tarn.options;

const pool: Pool<Connection> = new tarn.Pool({
    ...poolOptions,
    create: async () => new Connection(),
    destroy: async (connection) => {
        connection.ok();
    },
    validate:
        validateConnections === false
            ? undefined
            : (connection) => {
                connection.ok();
                return true;
            },
});

const keep: Pending<Connection> = pool.acquire();
"#;
    let diags = relevant_strict_default_lib_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 7006),
        "generic constructor options should contextually type callback parameters through method signatures and spreads. Got: {diags:#?}"
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "Pool should infer R = Connection through method-style create and Omit spread. Got: {diags:#?}"
    );
}

#[test]
fn conflicting_contextual_instantiation_keeps_enclosing_return_type_param() {
    let source = r#"
declare function accept<R>(fn: (a: string, b: number) => R): R;

function outer<X>(source: <T>(a: T, b: T) => X) {
    const out = accept(source);
    const keep: X = out;
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(_code, message)| message.contains("unknown")),
        "contextual conflict handling must not rewrite enclosing return type parameters to unknown. Got: {diags:#?}"
    );
}

#[test]
fn generic_callback_parameter_does_not_override_concrete_array_inference() {
    let source = r#"
export function keyOf<a>(value: { key: a; }): a {
    return value.key;
}
declare class Date {}
export interface Data {
    key: number;
    value: Date;
}

var data: Data[] = [];
declare function toKeys<a>(values: a[], toKey: (value: a) => string): string[];

toKeys(data, keyOf);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("Data[]")),
        "the concrete array argument should own `a`; the callback should be checked against `(value: Data) => string`. Got: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("(value: Data) => string")),
        "generic callback return mismatch should be reported at the callback parameter. Got: {diags:#?}"
    );
}

#[test]
fn contextual_parameter_self_referential_no_excess_constraint_no_false_ts2345() {
    let source = r#"
type NoExcessProperties<T, U> = T & {
  readonly [K in Exclude<keyof U, keyof T>]: never;
};

interface Effect<out A> {
  readonly EffectTypeId: {
    readonly _A: (_: never) => A;
  };
}

declare function pipe<A, B>(a: A, ab: (a: A) => B): B;

interface RepeatOptions<A> {
  until?: (_: A) => boolean;
}

declare const repeat: {
  <O extends NoExcessProperties<RepeatOptions<A>, O>, A>(
    options: O,
  ): (self: Effect<A>) => Effect<A>;
};

pipe(
  {} as Effect<boolean>,
  repeat({
    until: (x) => {
      return x;
    },
  }),
);
"#;
    let diags = relevant_lib_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "self-referential NoExcessProperties constraint should not raise false TS2345. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_nested_generic_spread_inference() {
    let source = r#"
declare function wrap<X>(x: X): { x: X };
declare function call<A extends unknown[], T>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "nested generic spread inference should not produce TS2345. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_inferential_typing_with_function_type() {
    let source = r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;

var s = map("", identity);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "generic function argument identity should not produce TS2345. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_generic_method_overspecialization() {
    let source = r#"
var names = ["list", "table1", "table2", "table3", "summary"];

interface HTMLElement {
    clientWidth: number;
    isDisabled: boolean;
}

declare var document: Document;
interface Document {
    getElementById(elementId: string): HTMLElement;
}

var elements = names.map(function (name) {
    return document.getElementById(name);
});

var xxx = elements.filter(function (e) {
    return !e.isDisabled;
});

var widths:number[] = elements.map(function (e) {
    return e.clientWidth;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2344),
        "generic method overspecialization should not produce TS2344. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_inference_does_not_add_undefined_or_null() {
    let source = r#"
interface NodeArray<T extends Node> extends ReadonlyArray<T> {}

interface Node {
    forEachChild<T>(cbNode: (node: Node) => T | undefined, cbNodeArray?: (nodes: NodeArray<Node>) => T | undefined): T | undefined;
}

declare function toArray<T>(value: T | T[]): T[];
declare function toArray<T>(value: T | readonly T[]): readonly T[];

function flatMapChildren<T>(node: Node, cb: (child: Node) => readonly T[] | T | undefined): readonly T[] {
    const result: T[] = [];
    node.forEachChild(child => {
        const value = cb(child);
        if (value !== undefined) {
            result.push(...toArray(value));
        }
    });
    return result;
}

function flatMapChildren2<T>(node: Node, cb: (child: Node) => readonly T[] | T | null): readonly T[] {
    const result: T[] = [];
    node.forEachChild(child => {
        const value = cb(child);
        if (value !== null) {
            result.push(...toArray(value));
        }
    });
    return result;
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2344),
        "inference should not add undefined or null to T. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_infer_from_generic_function_return_types_2() {
    let source = r#"
type Mapper<T, U> = (x: T) => U;

declare function wrap<T, U>(cb: Mapper<T, U>): Mapper<T, U>;

declare function arrayize<T, U>(cb: Mapper<T, U>): Mapper<T, U[]>;

declare function combine<A, B, C>(f: (x: A) => B, g: (x: B) => C): (x: A) => C;

declare function foo(f: Mapper<string, number>): void;

let f1: Mapper<string, number> = s => s.length;
let f2: Mapper<string, number> = wrap(s => s.length);
let f3: Mapper<string, number[]> = arrayize(wrap(s => s.length));
let f4: Mapper<string, boolean> = combine(wrap(s => s.length), wrap(n => n >= 10));

foo(wrap(s => s.length));

let a1 = ["a", "b"].map(s => s.length);
let a2 = ["a", "b"].map(wrap(s => s.length));
let a3 = ["a", "b"].map(wrap(arrayize(s => s.length)));
let a4 = ["a", "b"].map(combine(wrap(s => s.length), wrap(n => n > 10)));
let a5 = ["a", "b"].map(combine(identity, wrap(s => s.length)));
let a6 = ["a", "b"].map(combine(wrap(s => s.length), identity));

class SetOf<A> {
  _store: A[];

  add(a: A) {
    this._store.push(a);
  }

  transform<B>(transformer: (a: SetOf<A>) => SetOf<B>): SetOf<B> {
    return transformer(this);
  }

  forEach(fn: (a: A, index: number) => void) {
      this._store.forEach((a, i) => fn(a, i));
  }
}

function compose<A, B, C, D, E>(
  fnA: (a: SetOf<A>) => SetOf<B>,
  fnB: (b: SetOf<B>) => SetOf<C>,
  fnC: (c: SetOf<C>) => SetOf<D>,
  fnD: (c: SetOf<D>) => SetOf<E>,
):(x: SetOf<A>) => SetOf<E>;
function compose<T>(...fns: ((x: T) => T)[]): (x: T) => T {
  return (x: T) => fns.reduce((prev, fn) => fn(prev), x);
}

function map<A, B>(fn: (a: A) => B): (s: SetOf<A>) => SetOf<B> {
  return (a: SetOf<A>) => {
    const b: SetOf<B> = new SetOf();
    a.forEach(x => b.add(fn(x)));
    return b;
  }
}

function filter<A>(predicate: (a: A) => boolean): (s: SetOf<A>) => SetOf<A> {
  return (a: SetOf<A>) => {
    const result = new SetOf<A>();
    a.forEach(x => {
      if (predicate(x)) result.add(x);
    });
   return result;
  }
}

const testSet = new SetOf<number>();
testSet.add(1);
testSet.add(2);
testSet.add(3);

const t1 = testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    map(x => x + x),
    map(x => x + '!!!'),
    map(x => x.toUpperCase())
  )
)

declare function identity<T>(x: T): T;

const t2 = testSet.transform(
  compose(
    filter(x => x % 1 === 0),
    identity,
    map(x => x + '!!!'),
    map(x => x.toUpperCase())
  )
)
"#;
    let diags = relevant_default_lib_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322 || *code == 2345),
        "higher-order inference should not produce extra TS2322/TS2345. Got: {diags:#?}"
    );
}

// ─── Const type parameter inference ─────────────────────────────────

#[test]
fn const_type_param_nested_array_in_object_no_false_ts2322() {
    // When a function has `const T` and multiple parameters, nested array
    // literals inside object literal arguments must be inferred as readonly
    // tuples, not plain arrays. Without const assertion flowing into nested
    // expressions, [1, 2] is typed as `number[]` which is not assignable to
    // the inferred `readonly [1, 2]`, producing a false TS2322.
    let source = r#"
declare function f<const T>(x: T, y?: string): T;
const a = f({ d: [1, 2] });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "const type param with multi-param function should not produce false TS2322. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_single_param_nested_array_no_false_ts2322() {
    // Baseline: single-param const type param function should also work.
    let source = r#"
declare function f<const T>(x: T): T;
const a = f({ d: [1, 2] });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "const type param with single-param function should not produce TS2322. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_empty_array_in_object_no_false_ts2322() {
    // Empty arrays inside object literals with const type params should be
    // typed as empty readonly tuples, not `never[]`.
    let source = r#"
declare function f<const T>(x: T, y?: string): T;
const a = f({ d: [] });
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "const type param with empty array should not produce false TS2322. Got: {diags:#?}"
    );
}

// ─── Issue #6261: const type param preserves literals across multiple params ──
//
// Structural rule: when a generic call has a `const` type parameter whose
// constraint does not allow a mutable array-like target, the literal shape of
// the argument expression must be the round-1 inference seed. The presence
// of additional non-const parameters, multiple type parameters, class
// constructors, or interface methods does not change this rule.

#[test]
fn const_type_param_class_constructor_preserves_object_literal() {
    // tsc preserves `g.value.x: 1` even though the constructor signature
    // includes the const type param via a property.
    let source = r#"
class ConstContainer<const T> { constructor(public value: T) {} }
const g = new ConstContainer({ x: 1 });
const _gx: 1 = g.value.x;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "class const type param should preserve literal property type. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_multiple_const_params_preserve_each_literal() {
    let source = r#"
function multiConst<const T, const U>(x: T, y: U): [T, U] { return [x, y]; }
const e = multiConst({ a: 1 }, { b: 2 });
const _e0a: 1 = e[0].a;
const _e1b: 2 = e[1].b;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "multiple const type params must each preserve literal property types. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_with_sibling_primitive_param_preserves_literal() {
    // The presence of a sibling non-const parameter (`y: number`) must not
    // cause T to be widened.
    let source = r#"
function f<const T>(x: T, y: number): T { return x; }
const r = f({ a: 1 }, 2);
const _ra: 1 = r.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "sibling primitive param must not break const literal preservation. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_when_const_param_is_second_preserves_literal() {
    // Position of the const-typed parameter must not matter.
    let source = r#"
function f<const T>(x: number, y: T): T { return y; }
const r = f(2, { b: 1 });
const _rb: 1 = r.b;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "const param at non-first position must still preserve literals. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_renamed_preserves_literal() {
    // Renaming the type parameter must not affect the rule (the fix is
    // structural, not name-driven).
    let source = r#"
function renamed<const P>(x: P, y: number): P { return x; }
const r = renamed({ a: 1 }, 2);
const _ra: 1 = r.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "renaming const type param must not break preservation. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_mixed_const_and_non_const_preserves_const_literal() {
    // `const T` preserves; `U` (non-const) widens normally.
    let source = r#"
function mixed<const T, U>(x: T, y: U): [T, U] { return [x, y]; }
const r = mixed({ a: 1 }, { b: 2 });
const _ra: 1 = r[0].a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "const T should preserve literal even when sibling U is non-const. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_interface_method_preserves_literal() {
    let source = r#"
interface ConstMethod { process<const T>(value: T): T; }
declare const cm: ConstMethod;
const h = cm.process({ y: 2 });
const _hy: 2 = h.y;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "interface method with const type param should preserve literal. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_with_aliased_readonly_array_constraint_preserves_literal() {
    // An alias wrapping the readonly-array constraint must still trigger
    // literal preservation (the constraint is resolved before the
    // mutable-array check).
    let source = r#"
type ROArr = readonly unknown[];
function f<const T extends ROArr>(x: T, y: number): T { return x; }
const r = f([1, 2, 3], 0);
const _r: readonly [1, 2, 3] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "aliased readonly-array constraint must still preserve literals. Got: {diags:#?}"
    );
}

#[test]
fn non_const_type_param_still_widens_object_literal_property() {
    // Negative case: without `const`, the literal property type must widen
    // (proves the fix is gated on `is_const`, not unconditional).
    let source = r#"
function f<T>(x: T, y: number): T { return x; }
const r = f({ a: 1 }, 2);
const _ra: 1 = r.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "non-const T must still widen property literal to number. Got: {diags:#?}"
    );
}

#[test]
fn const_type_param_with_mutable_array_constraint_widens() {
    // Negative case: `const T extends unknown[]` (mutable array) should
    // widen because the constraint allows a mutable-array target. This
    // proves the (c) branch is gated on `constraint_allows_mutable_array_like`.
    let source = r#"
function f<const T extends unknown[]>(x: T): T { return x; }
const r = f([1, 2, 3]);
const _r: [1, 2, 3] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "mutable-array constraint must keep widening behavior. Got: {diags:#?}"
    );
}

// ─── Symbol-keyed property exclusion from string-index inference ─────────────

#[test]
fn object_values_with_symbol_keyed_intersection_no_false_ts2345() {
    // Regression: calling a function that expects T with a value inferred from
    // Object.values on a type that has both a unique-symbol property and a
    // string index signature must NOT include the symbol property value type
    // in the inferred T.
    //
    // Previously `true` was included in T from `{ [sym]?: true }`, causing
    // a false TS2345 where tsc emits none.
    //
    // Reproduces: unionTypeInference.ts repro from #32752
    let source = r#"
declare const sym: unique symbol;
type WithSym<T> = { [sym]?: true } & T;
declare function f<T>(x: WithSym<{ [s: string]: T }>): T;
declare const input: WithSym<{ [s: string]: string }>;
const result: string = f(input);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345 || *code == 2322),
        "symbol property in intersection must not cause false type error. Got: {diags:#?}"
    );
}

// ─── Recursive homomorphic mapped type inference ─────────────────────────────

#[test]
fn recursive_homomorphic_mapped_against_self_referential_interface_no_unknown_property() {
    // Regression for `mappedTypeRecursiveInference.ts`.
    //
    // `Deep<T> = { [K in keyof T]: Deep<T[K]> }` applied to a self-referential
    // interface like `interface A { a: A }` must converge to a structural
    // candidate for T (so accesses `out.a`, `out.a.a` resolve to a real object
    // type, not `unknown`). Before the alias-cycle fix in
    // `reverse_infer_through_template`, every recursive expansion produced a
    // fresh mapped TypeId, so the per-template visited set never detected the
    // cycle and the depth cap was reached only after the instantiation depth
    // limit had already collapsed the template to `error`. The result was T =
    // `{ a: unknown }`, which raised a spurious TS18046 on `out.a.a`.
    let source = r#"
interface A { a: A }
declare let a: A;
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
const out = foo(a);
out.a;
out.a.a;
out.a.a.a.a.a.a.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 18046),
        "recursive Deep<A> inference must not leave nested accesses as unknown. Got: {diags:#?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345 || *code == 2322),
        "recursive Deep<A> inference must not raise an assignability error for the self-referential source. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_against_index_signature_interface_no_unknown_property() {
    // Sibling to the named-property case: `interface B { [s: string]: B }`
    // reverse-maps via the index-signature path. Both paths must converge to a
    // structural candidate so `oub.b.a.n.a` is well-typed.
    let source = r#"
interface B { [s: string]: B }
declare let b: B;
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
const oub = foo(b);
oub.b;
oub.b.b;
oub.b.a.n.a.n.a;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 18046),
        "recursive Deep<B> inference must not leave nested accesses as unknown. Got: {diags:#?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345 || *code == 2322),
        "recursive Deep<B> inference must not raise an assignability error for the self-referential source. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_with_nullable_property_lets_outer_check_reject_null() {
    // Companion case: when the recursively-inferred property has a `T1 | null`
    // source type, reverse inference falls back to `any` (not `unknown`) so
    // subsequent property accesses on the inferred T resolve without
    // TS18046, while the *outer* assignability check (e.g. against
    // `Deep<any>`) still rejects the `null` member and reports TS2345 for
    // the original `foo(...)` call, matching tsc's behaviour for
    // `XMLHttpRequest.responseXML: Document | null`.
    let source = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface DocLike { url: string }
interface XLike {
    responseXML: DocLike | null;
}
declare let xhr: XLike;
const out = foo(xhr);
const ok = out.responseXML.url; // must NOT raise TS18046
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 18046),
        "Nullable property reverse inference must materialise as `any` so chained accesses are well-typed. Got: {diags:#?}"
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "Outer Deep<...> assignability must still reject the `null` constituent. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_materializes_primitive_apparent_members() {
    // `mappedTypeRecursiveInference.ts` includes `XMLHttpRequest` primitive
    // properties such as `readyState: number` and `responseText: string`.
    // Reverse inference through `Deep<T>` must infer apparent primitive member
    // objects for those properties rather than collapsing them to `unknown`.
    // Nullable callback properties should still be uninformative (`unknown`),
    // unlike nullable object properties where we keep the existing `any`
    // approximation so chained property accesses do not raise TS18046.
    let source = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface DocLike { url: string }
interface XLike {
    onreadystatechange: (() => void) | null;
    readonly readyState: number;
    readonly responseText: string;
    responseXML: DocLike | null;
}
declare let xhr: XLike;
const out = foo(xhr);
const ok = out.responseXML.url;
const readyShape: { toString: unknown } = out.readyState;
const textShape: { toString: unknown } = out.responseText;
const callbackNumber: number = out.onreadystatechange;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 18046),
        "Nullable object property should remain usable after recursive reverse inference. Got: {diags:#?}"
    );
    let ts2322_messages: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        ts2322_messages.len() == 1
            && ts2322_messages[0].contains("Type 'unknown' is not assignable to type 'number'"),
        "Only the nullable callback assignment should fail, proving it stayed `unknown` rather than `any`. Got: {diags:#?}"
    );
}

#[test]
fn recursive_homomorphic_mapped_with_nested_indexed_target_does_not_rewalk_target_param() {
    let source = r#"
type Deep<T> = { [K in keyof T]: Deep<T[K]> }
declare function foo<T>(deep: Deep<T>): T;
interface Payload {
    label: string;
    child: Payload;
}
interface XLike {
    response: Payload;
    responseText: string;
    readyState: number;
}
declare let xhr: XLike;
const out = foo(xhr);
const childLabel: string = out.response.child.label;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 18046),
        "nested Deep<T[K]> reverse inference must not leave chained accesses as unknown. Got: {diags:#?}"
    );
}

// ─── Higher-order function inference (HOFI) — tracks compiler/genericFunctionInference1.ts ─

/// Locks in the existing correct behavior: a generic source function with a
/// non-self-referential type-parameter constraint is accepted as the argument
/// of `pipe<A extends any[], B>(ab: (...args: A) => B)`. Inference should not
/// collapse the source's type parameter to `unknown` and reject the call.
///
/// `tsc` accepts each of the calls below; tsz currently does too. This test
/// exists to catch regressions if the inference path that handles non-recursive
/// constraints is reworked while addressing the recursive-constraint gap
/// captured by the `pipe_accepts_*_self_referential_*` ignored test below.
#[test]
fn pipe_accepts_generic_argument_with_simple_constraint() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function fooStr<T extends string>(x: T): T;
declare function fooNum<T extends number>(x: T): T;
declare function fooObj<T extends { other: number }>(x: T): T;
declare function fooBare<T>(x: T): T;

const a = pipe(fooStr);
const b = pipe(fooNum);
const c = pipe(fooObj);
const d = pipe(fooBare);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "pipe(<T extends C>(x: T) => T) with non-self-referential C must not raise TS2345. Got: {diags:#?}"
    );
}

/// HOFI gap: a generic source function whose type-parameter constraint refers
/// back to the type parameter itself (`T extends { value: T }`) is rejected
/// when passed to `pipe<A extends any[], B>(ab: (...args: A) => B)`. tsc
/// accepts it and propagates the constraint into the result type
/// (`<T extends { value: T; }>(x: T) => T`).
///
/// Conformance test: `compiler/genericFunctionInference1.ts` (lines 20, 21,
/// 33, 34 — the recursive-constraint subset of the eight extra TS2345
/// diagnostics).
#[test]
fn pipe_accepts_generic_argument_with_self_referential_constraint() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function fooSelf<T extends { value: T }>(x: T): T;

const f = pipe(fooSelf);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "pipe(<T extends {{ value: T }}>(x: T) => T) must not raise TS2345 once HOFI is implemented. Got: {diags:#?}"
    );
}

#[test]
fn type_literal_generic_method_retains_method_type_params_for_call_inference() {
    let source = r#"
type Matcher<T> = {
    with<P, R>(pattern: P, handler: (value: T) => R): Matcher<T>;
};

declare function match<T>(value: T): Matcher<T>;
declare function oneOf<T>(left: T, right: T): T;
declare const item: { kind: "issue"; priority: "low" | "medium" | "high" };

match(item).with({ kind: "issue", priority: oneOf("medium", "high") }, () => true);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2345),
        "generic methods declared in type literals must retain method type params for call inference. Got: {diags:#?}"
    );
}

// ─── Variadic tuple spread with type-assertion arguments ─────────────────────

#[test]
fn variadic_tuple_spread_type_assertion_preserves_literals() {
    let source = r#"
declare function concat<T extends readonly unknown[], U extends readonly unknown[]>(a: T, b: U): [...T, ...U];
const result = concat([1, 2] as [1, 2], ["a", "b"] as ["a", "b"]);
const _r: [1, 2, "a", "b"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "variadic tuple spread with type-asserted args must preserve literal types. Got: {diags:#?}"
    );
}

#[test]
fn variadic_tuple_spread_type_assertion_preserves_literals_renamed_params() {
    let source = r#"
declare function concat<K extends readonly unknown[], V extends readonly unknown[]>(a: K, b: V): [...K, ...V];
const result = concat([true, false] as [true, false], [42] as [42]);
const _r: [true, false, 42] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "variadic tuple spread literal preservation must not depend on parameter names K/V. Got: {diags:#?}"
    );
}

#[test]
fn variadic_tuple_spread_without_assertion_widens_to_primitives() {
    let source = r#"
declare function concat<T extends readonly unknown[], U extends readonly unknown[]>(a: T, b: U): [...T, ...U];
const result = concat([1, 2], ["a", "b"]);
const _bad: [1, 2, "a", "b"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "variadic tuple spread from fresh (non-asserted) tuple must widen literals. Got: {diags:#?}"
    );
}

#[test]
fn variadic_tuple_spread_three_way_with_assertions_preserves_literals() {
    let source = r#"
declare function concat3<A extends readonly unknown[], B extends readonly unknown[], C extends readonly unknown[]>(
    a: A, b: B, c: C
): [...A, ...B, ...C];
const r = concat3([1] as [1], ["x"] as ["x"], [true] as [true]);
const _check: [1, "x", true] = r;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "three-way variadic spread with asserted tuples must preserve all literals. Got: {diags:#?}"
    );
}

#[test]
fn conditional_type_parameter_default_evaluates_after_prior_arg_known() {
    let source = r#"
type Wrapper<T, W = T extends string ? number : boolean> = {
  value: T;
  wrapped: W;
};

type WrapStr = Wrapper<string>;
const ws: WrapStr = { value: "hello", wrapped: 42 };
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "conditional default depending on earlier known type parameter must evaluate. Got: {diags:#?}"
    );
}

// ─── Template literal type parameter inference (issue #6147) ─────────────────

/// f(x: prefix-T) where T extends string — call with matching literal should infer T.
#[test]
fn template_literal_infers_type_param_trailing_span() {
    let source = r#"
declare function f<T extends string>(x: `prefix-${T}`): T;
const result = f("prefix-hello");
const _check: "hello" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T should be inferred as \"hello\" from template literal argument. Got: {diags:#?}"
    );
}

/// Same rule with a renamed type parameter (`K`) to confirm no identifier is hardcoded.
#[test]
fn template_literal_infers_type_param_renamed() {
    let source = r#"
declare function get<K extends string>(x: `get-${K}`): K;
const result = get("get-name");
const _check: "name" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "K should be inferred as \"name\" from template literal argument. Got: {diags:#?}"
    );
}

/// f(x: pre-T-suf) where T extends string — T is surrounded by text anchors.
#[test]
fn template_literal_infers_type_param_prefix_and_suffix() {
    let source = r#"
declare function f<T extends string>(x: `pre-${T}-suf`): T;
const result = f("pre-mid-suf");
const _check: "mid" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T should be inferred as \"mid\" from surrounded template. Got: {diags:#?}"
    );
}

/// f(x: T-U) where T and U extend string — two type params inferred from a separator-delimited literal.
#[test]
fn template_literal_infers_multiple_type_params() {
    let source = r#"
declare function f<T extends string, U extends string>(x: `${T}-${U}`): [T, U];
const result = f("hello-world");
const _check: ["hello", "world"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T and U should be inferred from two-param template. Got: {diags:#?}"
    );
}

/// Same two-param rule using different names (`A`, `B`) to confirm generality.
#[test]
fn template_literal_infers_multiple_type_params_renamed() {
    let source = r#"
declare function split<A extends string, B extends string>(x: `${A}/${B}`): [A, B];
const result = split("foo/bar");
const _check: ["foo", "bar"] = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "A and B should be inferred from split template. Got: {diags:#?}"
    );
}

/// When the argument does not match the template pattern, a TS2345 error is expected.
#[test]
fn template_literal_type_param_mismatch_errors() {
    let source = r#"
declare function f<T extends string>(x: `prefix-${T}`): T;
f("wrong");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "passing a non-matching literal should raise TS2345. Got: {diags:#?}"
    );
}

/// f(x: T-suffix) where T extends string — T is a leading span with a fixed suffix.
#[test]
fn template_literal_type_param_leading_span() {
    let source = r#"
declare function f<T extends string>(x: `${T}-suffix`): T;
const result = f("hello-suffix");
const _check: "hello" = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "T should be inferred as \"hello\" from leading template span. Got: {diags:#?}"
    );
}
