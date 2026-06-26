//! Regression tests for #14805: a self-referential **class** member whose
//! return type is inferred (un-annotated) must trigger the circular
//! implicit-`any` diagnostic — TS7023 for named members (methods / getters),
//! TS7024 for anonymous arrow-function fields — exactly as `tsc` does.
//!
//! Detection is symbol/receiver gated: only a `this.`/`Class.` self-reference
//! whose receiver resolves to the enclosing class counts. An unrelated
//! `obj.member` access that merely shares a name must stay clean (the inverse of
//! the object-literal FP tracked by #14730).
//!
//! Binder names are varied across cases (anti-hardcoding): the logic keys off
//! structure (receiver + resolved member), never a specific identifier.

use crate::test_utils::check_source_strict_codes;

fn codes(src: &str) -> Vec<u32> {
    check_source_strict_codes(src)
}

fn assert_clean(src: &str) {
    let got = codes(src);
    assert!(
        got.is_empty(),
        "expected no diagnostics, got: {got:?}\n{src}"
    );
}

fn assert_codes(src: &str, expected: &[u32]) {
    let mut got = codes(src);
    got.sort_unstable();
    let mut want = expected.to_vec();
    want.sort_unstable();
    assert_eq!(got, want, "for source:\n{src}");
}

// ---------------------------------------------------------------------------
// The reported witnesses.
// ---------------------------------------------------------------------------

#[test]
fn instance_arrow_field_self_call_is_ts7024() {
    // `class C { f = () => this.f(); }` — anonymous arrow → TS7024.
    assert_codes("class Alpha { run = () => this.run(); }", &[7024]);
}

#[test]
fn instance_method_self_call_is_ts7023() {
    assert_codes("class Beta { step() { return this.step(); } }", &[7023]);
}

#[test]
fn static_arrow_field_self_call_via_class_name_is_ts7024() {
    assert_codes("class Gamma { static go = () => Gamma.go(); }", &[7024]);
}

#[test]
fn static_method_self_call_via_class_name_is_ts7023() {
    assert_codes(
        "class Delta { static tick() { return Delta.tick(); } }",
        &[7023],
    );
}

#[test]
fn static_arrow_field_self_call_via_this_is_ts7024() {
    // In a static field initializer `this` is the constructor, so `this.go`
    // selects the static member.
    assert_codes("class Eta { static go = () => this.go(); }", &[7024]);
}

// ---------------------------------------------------------------------------
// Getters: a property *read* invokes the accessor, so it is circular too.
// ---------------------------------------------------------------------------

#[test]
fn getter_self_read_is_ts7023() {
    assert_codes("class Zeta { get value() { return this.value; } }", &[7023]);
}

#[test]
fn getter_self_call_is_ts7023() {
    assert_codes(
        "class Theta { get value() { return this.value(); } }",
        &[7023],
    );
}

#[test]
fn getter_wrapped_self_read_is_ts7023() {
    assert_codes(
        "class Iota { get value() { return [this.value][0]; } }",
        &[7023],
    );
}

// ---------------------------------------------------------------------------
// Indirect / mutual cycles.
// ---------------------------------------------------------------------------

#[test]
fn mutually_recursive_methods_both_report() {
    assert_codes(
        "class Kappa { a() { return this.b(); } b() { return this.a(); } }",
        &[7023, 7023],
    );
}

#[test]
fn three_member_cycle_reports_all() {
    assert_codes(
        "class Lambda { a() { return this.b(); } b() { return this.c(); } c() { return this.a(); } }",
        &[7023, 7023, 7023],
    );
}

#[test]
fn generator_method_return_self_call_is_ts7023() {
    // A generator method is circular through its `Generator` return value when
    // it returns a self-call — `tsc` reports TS7023.
    assert_codes("class Pi { *gen() { return this.gen(); } }", &[7023]);
}

#[test]
fn plain_generator_method_stays_clean() {
    assert_clean("class Rho { *gen() { yield 1; } }");
}

#[test]
fn self_call_buried_in_return_expression() {
    // The self-call need not be the whole return value.
    assert_codes("class Mu { a() { return this.a() + 1; } }", &[7023]);
    assert_codes(
        "class Nu { a() { return wrap(this.a()); } } declare function wrap(x: any): any;",
        &[7023],
    );
}

// ---------------------------------------------------------------------------
// Only the member actually on a cycle is reported.
// ---------------------------------------------------------------------------

#[test]
fn caller_of_circular_member_is_not_itself_circular() {
    // `runner` calls the circular `loop`, but `runner` is not on a cycle.
    assert_codes(
        "class Xi { loop = () => this.loop(); runner() { return this.loop(); } }",
        &[7024],
    );
}

// ---------------------------------------------------------------------------
// Must stay clean — false-positive guards.
// ---------------------------------------------------------------------------

#[test]
fn annotated_return_type_is_not_circular() {
    assert_clean("class C1 { step(): number { return this.step(); } }");
    assert_clean("class C2 { go = (): number => this.go(); }");
}

#[test]
fn method_value_read_without_call_is_not_circular() {
    // Reading `this.step` (a method) yields its function value, not its return
    // type — not circular.
    assert_clean("class C3 { step() { return this.step; } }");
}

#[test]
fn arrow_field_value_read_without_call_is_not_circular() {
    assert_clean("class C4 { go = () => this.go; }");
}

#[test]
fn self_call_through_unrelated_receiver_is_not_circular() {
    // Same member name on an unrelated object — not a self-reference.
    assert_clean(
        "class C5 { run() { return other.run(); } } declare const other: { run(): number };",
    );
}

#[test]
fn calling_a_different_annotated_member_is_not_circular() {
    assert_clean("class C6 { a() { return this.b(); } b(): number { return 1; } }");
}

#[test]
fn function_expression_field_rebinds_this_not_circular_return() {
    // A `function` expression field gets its own `this` (TS2683), not a circular
    // return-type diagnostic.
    assert_codes(
        "class C7 { f = function () { return this.f(); }; }",
        &[2683],
    );
}

#[test]
fn self_call_in_nested_object_literal_with_same_name_is_not_circular() {
    assert_clean("class C8 { build = () => { const o = { build: () => 1 }; return o.build(); }; }");
}

#[test]
fn self_reference_outside_return_position_is_not_circular() {
    // A self-call as a statement (not contributing to the return value) is not
    // circular — matches `tsc`.
    assert_clean("class C9 { step() { this.step(); return 1; } }");
}

#[test]
fn member_read_passed_as_argument_is_not_circular() {
    assert_clean(
        "class C10 { run = () => helper(this.run); } declare function helper(x: any): number;",
    );
}

#[test]
fn non_self_referential_members_stay_clean() {
    assert_clean(
        "class C11 { value = 1; total() { return this.value; } get half() { return this.value / 2; } }",
    );
}
