//! Parity coverage for the `yield` axis of the computed-member-name grammar
//! walk — the sibling of the `await`/TS1308 axis pinned in
//! `await_grammar_computed_property_name_tests.rs` (#16094 / #16100 / #16104).
//!
//! A computed member name (`[expr]` on a `class`/`interface`/type-literal
//! member, or an object-literal property) is evaluated once, when the
//! enclosing declaration is defined, in the *enclosing* scope — not inside the
//! member's own function body. So the container that decides whether a `yield`
//! inside it is legal is the enclosing function, reached by skipping the member
//! itself. `tsc` resolves it exactly that way (its
//! `getContainingFunctionOrClassStaticBlock` skips a class computed property
//! name's own member), and TS1163 (`A 'yield' expression is only allowed in a
//! generator body.`) is the grammar diagnostic that reports the miss.
//!
//! This is the direct structural twin of the async-context rule the await
//! suite documents: there, `state/state_checking/class.rs` resets `async_depth`
//! to `0` before checking a class's members, and the computed-name check swaps
//! the enclosing depth back in so `await` sees the surrounding function's
//! `async`-ness. Here the same "evaluated in the enclosing scope" principle
//! decides which container a `yield` belongs to.
//!
//! Every expectation below is pinned against the pinned `typescript@7.0.2`
//! oracle (`--noEmit --strict --pretty false --target es2022 --module
//! esnext`), the same oracle-cross-checked behavior the await suite pins,
//! and read through this crate's parse-health harness.
//!
//! The TS1163 *grammar* axis is uniform across every member kind: the
//! container jump reaches the enclosing generator for methods, accessors,
//! properties, object literals and interface signatures alike. The
//! *type-inference* TS7057 axis (`'yield' expression implicitly results in
//! an 'any' type because its containing generator lacks a return-type
//! annotation`) is **not** uniform, and — unlike the grammar axis — this is
//! genuine `tsc` behavior, not a tsz gap: oracle-verified, `tsc` reports
//! TS7057 for a computed name owned by a plain **property** (object-literal
//! property, class property, i.e. never itself a function scope), but never
//! for one owned by a **function-like** member (class/object-literal method,
//! getter, setter, static method) — even though the exact same yield is
//! legal there (no TS1163) and resolves to the identical enclosing
//! generator. #17057 originally read this split as a tsz false negative on
//! the function-like side; the oracle matrix below (added while closing that
//! issue) shows tsz already matches `tsc` on every row. A type-literal
//! member is its own separate case: it is outside await/generator context
//! entirely (see `check_computed_property_name_await`'s doc comment for the
//! same rule on the async axis), so its `yield` gets BOTH TS1163 (illegal
//! here) and TS7057 (still typed as implicit-any) together — the pairing
//! that first suggested TS7057 might track legality, which it does not.

use crate::test_utils::check_source_codes_with_parse_health;

fn sorted(mut codes: Vec<u32>) -> Vec<u32> {
    codes.sort_unstable();
    codes
}

// ---------------------------------------------------------------------------
// A `yield` in a computed name whose enclosing container is NOT a generator
// answers TS1163 — the whole member family, keyed on the computed-name
// position rather than on the member kind.

#[test]
fn non_generator_class_method_computed_name_yield_reports_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { class Holder { [yield 1]() {} } }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn non_generator_class_getter_computed_name_yield_reports_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { class Holder { get [yield 1]() { return 1; } } }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn non_generator_class_setter_computed_name_yield_reports_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { class Holder { set [yield 1](value: number) {} } }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn non_generator_class_static_method_computed_name_yield_reports_ts1163() {
    // `static` does not change the container question, exactly as it does not
    // for the await sibling.
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { class Holder { static [yield 1]() {} } }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn non_generator_class_expression_method_computed_name_yield_reports_ts1163() {
    // A class *expression* is class-like too, so the same jump applies.
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { const Holder = class { [yield 1]() {} }; }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn non_generator_class_property_computed_name_yield_reports_ts1163_and_ts1166() {
    // A class-property computed name pairs the grammar TS1163 with TS1166
    // (class-property literal-name requirement), exactly as the await sibling
    // pairs TS1308 with TS1166.
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { class Holder { [yield 1] = 2; } }
"#,
    );
    assert_eq!(sorted(codes.clone()), vec![1163, 1166], "got {codes:?}");
}

#[test]
fn non_generator_object_literal_computed_name_yield_reports_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { const bag = { [yield 1]: 1 }; }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn non_generator_interface_computed_name_yield_reports_ts1163_and_ts1169() {
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { interface Shape { [yield 1](): void } }
"#,
    );
    assert_eq!(sorted(codes.clone()), vec![1163, 1169], "got {codes:?}");
}

#[test]
fn non_generator_type_literal_computed_name_yield_reports_ts1163_and_ts1170() {
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { type Shape2 = { [yield 1]: number }; }
"#,
    );
    assert_eq!(sorted(codes.clone()), vec![1163, 1170], "got {codes:?}");
}

#[test]
fn non_generator_nested_class_in_method_computed_name_yield_reports_ts1163() {
    // The jump lands on the inner class, and the walk then hits the enclosing
    // non-generator method — still TS1163.
    let codes = check_source_codes_with_parse_health(
        r#"
function outer() { class O { build() { class Holder { [yield 1]() {} } } } }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn renamed_binder_non_generator_class_method_computed_name_yield_reports_ts1163() {
    // Anti-hardcoding: no identifier-spelling predicate drives the rule.
    let codes = check_source_codes_with_parse_health(
        r#"
function makeContainer() { class ConnectionPool { [yield token]() {} } }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Top-level (script) positions: still not a generator, so still TS1163.

#[test]
fn script_top_level_class_method_computed_name_yield_reports_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Holder { [yield 1]() {} }
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn script_top_level_object_literal_computed_name_yield_reports_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
const holder = { [yield 1]: 1 };
"#,
    );
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Inside a generator, the container the computed name resolves to IS the
// generator, so `yield` is legal and TS1163 must NOT fire — the positive proof
// that the container jump reaches the enclosing generator, not the member.

#[test]
fn generator_class_method_computed_name_yield_is_not_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1]() {} } }
"#,
    );
    assert!(
        !codes.contains(&1163),
        "the name's container is the enclosing generator, so `yield` is legal; got {codes:?}"
    );
}

#[test]
fn generator_class_setter_computed_name_yield_is_not_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { set [yield 1](value: number) {} } }
"#,
    );
    assert!(!codes.contains(&1163), "got {codes:?}");
}

#[test]
fn generator_class_static_method_computed_name_yield_is_not_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { static [yield 1]() {} } }
"#,
    );
    assert!(!codes.contains(&1163), "got {codes:?}");
}

#[test]
fn generator_object_literal_computed_name_yield_is_not_ts1163() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { const bag = { [yield 1]: 1 }; }
"#,
    );
    assert!(!codes.contains(&1163), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Negative controls — the container jump must stop at the right boundary.

#[test]
fn generator_with_non_generator_method_holding_nested_class_yield_reports_ts1163() {
    // The `yield` sits in a computed name inside a class defined in a *plain*
    // method body, which is itself inside a generator. The container jump lands
    // on that non-generator method, not the outer generator — so TS1163 fires.
    // This is the yield twin of the await suite's arrow-boundary control.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class O { build() { class Holder { [yield 1]() {} } } } }
"#,
    );
    assert!(
        codes.contains(&1163),
        "a plain method body is a non-generator container; got {codes:?}"
    );
}

#[test]
fn yield_inside_arrow_within_generator_computed_name_reports_ts1163() {
    // An arrow within the computed name is its own (non-generator) container,
    // so the `yield` is illegal even though the class sits in a generator — the
    // function-like boundary stops the walk.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [(() => yield 1)()]() {} } }
"#,
    );
    assert!(
        codes.contains(&1163),
        "an arrow body is its own container; got {codes:?}"
    );
}

#[test]
fn plain_identifier_member_named_yield_is_unaffected() {
    // Not a computed name at all — `yield` used directly as a method name in a
    // non-strict-reserved position. Different parser path; must stay clean.
    let codes = check_source_codes_with_parse_health(
        r#"
class K { yield() {} }
"#,
    );
    assert!(codes.is_empty(), "got {codes:?}");
}

#[test]
fn class_method_computed_name_no_yield_stays_clean() {
    // Negative control: the grammar root must not fire when there is no `yield`
    // anywhere in the computed name.
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { [key]() {} }
"#,
    );
    assert!(codes.is_empty(), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// #17057, closed as not-a-bug: the TS7057 split between plain-property and
// function-like computed-name owners is genuine `tsc` behavior.
//
// Oracle-verified (`typescript@7.0.2`, `--strict`): TS7057 fires for an
// object-literal PROPERTY and a class PROPERTY computed name, never for a
// class/object-literal method, getter, setter, or static method — despite
// the `yield` being equally legal (no TS1163) and resolving to the same
// enclosing generator in every row. tsz already matches this split; these
// are the rows #17057's own isolation matrix got backwards (it asserted
// interface/method-signature TS7057 without oracle verification — see the
// header comment above).

#[test]
fn generator_class_method_computed_name_yield_does_not_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1]() {} } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_class_getter_computed_name_yield_does_not_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { get [yield 1]() { return 1; } } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_class_setter_computed_name_yield_does_not_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { set [yield 1](value: number) {} } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_class_static_method_computed_name_yield_does_not_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { static [yield 1]() {} } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_object_literal_method_computed_name_yield_does_not_report_ts7057() {
    // The split is keyed on function-likeness, not on class-vs-object-literal:
    // an object-literal shorthand METHOD behaves like a class method, not like
    // the object-literal PROPERTY case below.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { const bag = { [yield 1]() {} }; }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_object_literal_property_computed_name_yield_reports_ts7057() {
    // Positive control: the plain-property sibling DOES get TS7057 — the
    // split is real, not a harness artifact.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { const bag = { [yield 1]: 1 }; }
"#,
    );
    assert!(codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_class_property_computed_name_yield_reports_ts7057() {
    // Positive control, class-property form.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1] = 2; } }
"#,
    );
    assert!(codes.contains(&7057), "got {codes:?}");
}

#[test]
fn renamed_binder_generator_class_method_computed_name_yield_does_not_report_ts7057() {
    // Anti-hardcoding: no identifier-spelling predicate drives the rule.
    let codes = check_source_codes_with_parse_health(
        r#"
function* makeContainer() { class ConnectionPool { [yield token]() {} } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// #17057 continued: interface/type-literal METHOD SIGNATURES (bodyless — no
// `is_function_like()` node kind of their own) reach the yield ANY-fallback
// through the real enclosing generator, unlike a class/object-literal method
// or accessor, whose own function-like node accidentally short-circuits the
// `is_in_generator` walk before it ever gets there (see the doc comment on
// `yield_computed_name_owner_is_method_signature` in `dispatch/yield_.rs`).
// Oracle-verified (`typescript@7.0.2`): `tsc` suppresses TS7057 for a method
// signature exactly like it does for a method/accessor with a body.

#[test]
fn generator_interface_method_signature_computed_name_yield_does_not_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { interface Shape { [yield 1](): void } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
    assert_eq!(codes, vec![1169], "got {codes:?}");
}

#[test]
fn generator_type_literal_method_signature_computed_name_yield_does_not_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { type Shape2 = { [yield 1](): void }; }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
    assert_eq!(sorted(codes.clone()), vec![1163, 1170], "got {codes:?}");
}

#[test]
fn renamed_binder_generator_interface_method_signature_computed_name_yield_does_not_report_ts7057()
{
    // Anti-hardcoding: no identifier-spelling predicate drives the rule.
    let codes = check_source_codes_with_parse_health(
        r#"
function* makeContainer() { interface ConnectionOptions { [yield token](): void } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn generator_interface_getter_signature_computed_name_yield_stays_clean() {
    // Negative control: accessor signatures reuse the GET_ACCESSOR/SET_ACCESSOR
    // node kinds, already `is_function_like()`, so this path was already
    // correct before this fix — pinned here so a regression on the new
    // METHOD_SIGNATURE-only helper shows up immediately.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { interface Shape { get [yield 1](): number; } }
"#,
    );
    assert!(codes.is_empty(), "got {codes:?}");
}

#[test]
fn generator_interface_setter_signature_computed_name_yield_stays_clean() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { interface Shape { set [yield 1](v: number); } }
"#,
    );
    assert!(codes.is_empty(), "got {codes:?}");
}

#[test]
fn generator_type_literal_getter_signature_computed_name_yield_reports_ts1163_only() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { type Shape2 = { get [yield 1](): number; }; }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
    assert_eq!(codes, vec![1163], "got {codes:?}");
}

#[test]
fn generator_interface_property_computed_name_yield_still_reports_ts7057() {
    // Positive control: a plain interface PROPERTY signature (not a method
    // signature) must keep TS7057 — the fix is scoped to METHOD_SIGNATURE
    // only, not every interface member kind.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { interface Shape { [yield 1]: number; } }
"#,
    );
    assert!(codes.contains(&7057), "got {codes:?}");
    assert_eq!(sorted(codes.clone()), vec![1169, 7057], "got {codes:?}");
}

#[test]
fn generator_type_literal_property_computed_name_yield_still_reports_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { type Shape2 = { [yield 1]: number }; }
"#,
    );
    assert!(codes.contains(&7057), "got {codes:?}");
    assert_eq!(
        sorted(codes.clone()),
        vec![1163, 1170, 7057],
        "got {codes:?}"
    );
}
