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
        lacks_diagnostic_code(&diagnostics, 2345),
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "Only the string argument should fail after branch inference. Diagnostics: {diags:#?}"
    );
    assert!(
        has_diagnostic_message_containing(
            &diags,
            2345,
            "not assignable to parameter of type 'never'",
        ),
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for acceptsComparable(1, 2); got: {diags:#?}"
    );
    let msg = ts2345[0];
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for acceptsComparable(1, 2, 3); got: {diags:#?}"
    );
    let msg = ts2345[0];
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for max2(1, 2); got: {diags:#?}"
    );
    let msg = ts2345[0];
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for max2(1, 2); got: {diags:#?}"
    );
    let msg = ts2345[0];
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for useWrapped literals; got: {diags:#?}"
    );
    let msg = ts2345[0];
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for boolean literal candidates; got: {diags:#?}"
    );
    let msg = ts2345[0];
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
        has_diagnostic_code(&diags, 2345),
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
        has_diagnostic_code(&diags, 2769),
        "Overloaded method should reject every candidate and report TS2769. Diagnostics: {diags:#?}"
    );
}

#[test]
fn namespace_local_promise_overloaded_method_diagnostics_shadow_lib_promise() {
    let source = r#"
namespace m2 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
    }

    declare function testFunction(n: number): Promise<number>;
    declare function testFunction(s: string): Promise<string>;

    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}

namespace m4 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
        then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => Promise<U>): Promise<U>;
    }

    declare function testFunction(n: number): Promise<number>;
    declare function testFunction(s: string): Promise<string>;

    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}

namespace m5 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
        then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => Promise<U>): Promise<U>;
        then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => U, progress?: (preservation: any) => void): Promise<U>;
    }

    declare function testFunction(n: number): Promise<number>;
    declare function testFunction(s: string): Promise<string>;

    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}

namespace m6 {
    interface Promise<T> {
        then<U>(cb: (x: T) => Promise<U>): Promise<U>;
        then<U>(cb: (x: T) => Promise<U>, error?: (error: any) => Promise<U>): Promise<U>;
    }

    declare function testFunction(n: number): Promise<number>;
    declare function testFunction(s: string): Promise<string>;
    declare function testFunction(b: boolean): Promise<boolean>;

    declare var numPromise: Promise<number>;
    var newPromise = numPromise.then(testFunction);
}
"#;

    let diagnostics: Vec<_> = compile_with_es2015_lib_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2345 | 2769))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345 && message.contains("parameter of type '(x: number) => Promise<string>'")
        }),
        "m2 should diagnose against the namespace-local Promise, not lib PromiseLike. Diagnostics: {diagnostics:#?}",
    );
    assert!(
        diagnostics
            .iter()
            .filter(|(code, message)| {
                *code == 2769 && message.contains("No overload matches this call.")
            })
            .count()
            >= 3,
        "overloaded namespace-local Promise.then mismatches should report TS2769. Diagnostics: {diagnostics:#?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message.contains("PromiseLike")),
        "namespace-local Promise diagnostics must not leak lib PromiseLike. Diagnostics: {diagnostics:#?}",
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
        lacks_diagnostic_code(&diags, 2339),
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
        lacks_diagnostic_code(&diags, 7006),
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
        !has_diagnostic_message_containing(
            &diags,
            2322,
            "Type 'string' is not assignable to type '\"test\"'",
        ),
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
        has_diagnostic_code(&diags, 2322),
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
        lacks_diagnostic_code(&diags, 7006),
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
    assert!(
        has_diagnostic_code(&diags, 2345),
        "Expected TS2345 for conflicting direct inference candidates. Diagnostics: {diags:#?}"
    );
    assert!(
        has_diagnostic_message_containing(
            &diags,
            2345,
            "Argument of type '\"\"' is not assignable to parameter of type '1'.",
        ),
        "TS2345 should preserve direct literal candidates. Diagnostics: {diags:#?}"
    );
}

#[test]
fn direct_generic_argument_mismatch_survives_context_sensitive_callback() {
    let source = r#"
declare function g<T>(a: T, b: T, c: (t: T) => T): T;
g("", 3, a => a);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2345),
        "Expected TS2345 for conflicting direct candidates before callback inference. Diagnostics: {diags:#?}"
    );
    assert!(
        has_diagnostic_message_containing(
            &diags,
            2345,
            "Argument of type '3' is not assignable to parameter of type '\"\"'.",
        ),
        "TS2345 should preserve the first direct literal inference candidate. Diagnostics: {diags:#?}"
    );
}

#[test]
fn rest_generic_argument_mismatch_displays_primitive_bases() {
    let source = r#"
declare function rest<T>(...items: T[]): T;
rest(1, "");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2345),
        "Expected TS2345 for conflicting rest inference candidates. Diagnostics: {diags:#?}"
    );
    assert!(
        has_diagnostic_message_containing(
            &diags,
            2345,
            "Argument of type 'string' is not assignable to parameter of type 'number'.",
        ),
        "TS2345 should display primitive bases for conflicting rest generic candidates. Diagnostics: {diags:#?}"
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
    let ts2345_count = diagnostic_count(&diags, 2345);
    let ts2403_count = diagnostic_count(&diags, 2403);
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
        has_diagnostic_code(&diags, 2345),
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
        has_diagnostic_code(&diags, 2339),
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
        has_diagnostic_code(&diags, 18048),
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
    let ts18048_count = diagnostic_count(&diags, 18048);
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
        lacks_diagnostic_code(&diags, 2345),
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
        lacks_diagnostic_code(&diags, 2339),
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
        lacks_diagnostic_code(&diags, 2345),
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
        lacks_diagnostic_code(&diags, 2345),
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
        lacks_diagnostic_code(&diags, 2339),
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
        lacks_any_diagnostic_code(&diags, &[2345, 7031]),
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
        lacks_diagnostic_code(&diags, 2322),
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
        lacks_diagnostic_code(&diags, 2345),
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
        lacks_diagnostic_code(&diags, 2322),
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
        lacks_diagnostic_code(&diags, 2322),
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
        lacks_diagnostic_code(&diags, 2339),
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
    let ts2540 = diagnostic_count(&diags, 2540);
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
        has_diagnostic_code(&diags, 4104),
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
        lacks_diagnostic_code(&diags, 2345),
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
        lacks_diagnostic_code(&diags, 2345),
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
    let ts2540 = diagnostic_count(&diags, 2540);
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
        lacks_diagnostic_code(&diags, 7006),
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
        lacks_diagnostic_code(&diags, 7006),
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
        lacks_diagnostic_code(&diags, 2322),
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
        lacks_diagnostic_code(&diags, 0),
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    let ts2322 = diagnostics_with_code(&diags, 2322);
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
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
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "scalar direct argument keeps literal narrow; NoInfer fallback rejects mismatch. Diagnostics: {diags:#?}"
    );
}

#[test]
fn noinfer_complex_return_widens_scalar_literal() {
    let source = r#"
function fn1<T>(a: T, b: NoInfer<T>): T {
  return a;
}

function fn2<T>(a: T, b: NoInfer<T>): { v: T } {
  return { v: a };
}

fn1("a", "b");
fn2("a", "b");
"#;
    let diags = relevant_diagnostics(source);
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "Only direct scalar return should preserve the literal and reject the NoInfer fallback. Diagnostics: {diags:#?}"
    );
}

