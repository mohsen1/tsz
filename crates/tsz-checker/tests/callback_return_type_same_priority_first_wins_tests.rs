//! Regression tests for #17553: two directly-passed callback arguments that
//! each contribute a `ReturnType`-priority inference candidate for the same
//! type parameter must NOT be unioned. tsc fixes the type parameter from the
//! first callback's return type and reports the mismatch on the rest — the
//! same "first wins" rule already applied to same-priority naked-argument and
//! array-element candidates (#17484/#9667/#17364), not the
//! `MappedTypeConstraint`/`LiteralKeyof` union rule.
//!
//! Structural rule: when two or more candidates for a type parameter arrive
//! at `InferencePriority::ReturnType` from directly-passed callback
//! arguments' return positions, and the candidates are disjoint bare
//! primitives, `get_common_supertype_for_inference` keeps the leftmost
//! (first) candidate instead of unioning — owner:
//! `crates/tsz-solver/src/inference/{infer_resolve,infer_bct}.rs`.

use tsz_checker::test_utils::check_source_strict_codes;

#[test]
fn two_callback_return_candidates_disjoint_primitives_report_ts2322() {
    let source = r#"
declare function k<T>(a: () => T, b: () => T): T;
const r3 = k(() => "s", () => 1);
const reveal: void = r3;
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322 on the second callback's mismatched return \
         (T fixed to string from the first callback); got {codes:?}"
    );
}

#[test]
fn two_callback_return_candidates_renamed_binder_still_reports_ts2322() {
    let source = r#"
declare function call2<Value>(first: () => Value, second: () => Value): Value;
const result = call2(() => "hello", () => 42);
const reveal: void = result;
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2322),
        "structural rule must not depend on the type parameter's name; got {codes:?}"
    );
}

#[test]
fn two_callback_return_candidates_compatible_types_do_not_error() {
    // Base/Derived are related, so the common-supertype tournament (not the
    // disjoint-primitive first-wins fallback) picks the broader type — no
    // mismatch to report, unaffected by the #17553 fix.
    let source = r#"
class Base {}
class Derived extends Base {}
declare function k<T>(a: () => T, b: () => T): T;
declare let derived: Derived;
declare let base: Base;
const r = k(() => derived, () => base);
const reveal: Base = r;
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2345),
        "related callback return types must still find a common supertype; got {codes:?}"
    );
}

#[test]
fn rest_parameter_disjoint_primitives_still_reports_ts2345() {
    // Adjacent, already-correct shape from the issue: rest-parameter naked
    // inference (not the callback/contextual return-type path) already
    // matched tsc before this fix — must keep matching after it.
    let source = r#"
declare function g<T>(...args: T[]): T;
const r1 = g("s", 1);
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2345),
        "rest-parameter naked inference must keep reporting TS2345 on the disjoint second argument; got {codes:?}"
    );
}

#[test]
fn explicit_type_argument_bypasses_inference_and_does_not_error() {
    // An explicit type argument is unaffected by inference candidate
    // combination entirely — negative control.
    let source = r#"
declare function k<T>(a: () => T, b: () => T): T;
const r = k<string | number>(() => "s", () => 1);
const reveal: string | number = r;
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2345),
        "an explicit type argument must not be affected by candidate-combination policy; got {codes:?}"
    );
}

#[test]
fn single_callback_argument_is_unaffected() {
    // Negative control from the issue's isolation notes: with only one
    // context-sensitive/callback argument there is only one candidate, so
    // there is nothing to combine or pick between — must stay clean.
    let source = r#"
declare function k<T>(a: () => T): T;
const r = k(() => "s");
const reveal: string = r;
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2345),
        "a single callback candidate must be unaffected; got {codes:?}"
    );
}

// --- #17761: the first-wins rule is scoped to callbacks whose type parameter
// is inferred PURELY from their return positions. When the same type parameter
// also appears in the callbacks' PARAMETER positions (`(x: T) => T`), a
// context-sensitive callback contextually types its own parameter with the
// still-unresolved variable, so tsc does NOT first-wins-pin it from one
// callback and reject the sibling — it leaves the variable to the combination
// (union) path and accepts the call. `#17755` widened the callback return
// contributions cross-domain, which surfaced this pre-existing over-fire as a
// false `TS2322` on `genericCallWithGenericSignatureArguments.ts`.

#[test]
fn type_param_in_callback_parameter_position_disables_first_wins() {
    // `T` is in both the parameter and return position of each callback. tsc
    // leaves `T` unresolved and accepts `foo((x) => 1, (x) => '')`; the
    // return-type first-wins pin must not fire. (`r1b` shape.)
    let source = r#"
declare function foo<T>(a: (x: T) => T, b: (x: T) => T): (x: T) => T;
var r1b = foo((x) => 1, (x) => '');
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2345),
        "a bidirectional (param+return) type parameter must not first-wins-pin \
         from one callback; got {codes:?}"
    );
}

#[test]
fn bidirectional_callback_param_renamed_binders_block_bodies_stay_clean() {
    // Structural rule must not depend on the binder names or the body form.
    let source = r#"
declare function bar<Value>(a: (y: Value) => Value, b: (y: Value) => Value): Value;
var q = bar(function (y) { return 'a'; }, function (y) { return 2; });
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322) && !codes.contains(&2345),
        "renamed binders / block bodies must stay clean; got {codes:?}"
    );
}

#[test]
fn return_only_type_param_still_first_wins_with_a_param_of_another_type() {
    // Control: the callback has a parameter, but it is a DIFFERENT (already
    // concrete) type, so the inferred `T` is still return-only. First-wins must
    // still apply and report `TS2322` on the mismatched second callback.
    let source = r#"
declare function k<T>(a: (n: number) => T, b: (n: number) => T): T;
const r = k((n) => "s", (n) => 1);
const reveal: void = r;
"#;
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2322),
        "a return-only type parameter must still first-wins even when the \
         callback has an unrelated concrete parameter; got {codes:?}"
    );
}
