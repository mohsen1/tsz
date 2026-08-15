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
