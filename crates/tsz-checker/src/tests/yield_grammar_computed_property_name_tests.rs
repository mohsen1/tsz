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
//! Every expectation below is pinned against the compiled `tsz` CLI
//! (`--noEmit --strict --pretty false --target es2022 --module esnext`) — the
//! same oracle-cross-checked behavior the await suite pins — and read through
//! this crate's parse-health harness. The TS1163 grammar axis reproduces
//! identically in both.
//!
//! The type-inference TS7057 axis is covered at the end. `tsc` emits TS7057
//! (`'yield' expression implicitly results in an 'any' type ...`) for a `yield`
//! in a computed name **only when the member is a data property** — the key
//! feeds the property's own type. For a *function member* (method, accessor, or
//! bodyless method signature) the key does not consume the yield's implicit-any
//! result, so `tsc` emits no TS7057. #17057 was filed as the mirror image of
//! this (that class methods *should* raise TS7057 and drop it); the oracle shows
//! the opposite — class/object methods already match `tsc` (silent), and the
//! real divergence was a false-positive TS7057 on a bodyless interface /
//! type-literal method **signature**, now fixed.

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
// The TS7057 type-inference axis (#17057).
//
// `tsc` emits TS7057 for a `yield` in a computed name *only* when the member is
// a data property; a function member (method / accessor / method signature)
// never raises it. The rows below pin both directions against the oracle.

// --- Data-property members: TS7057 fires. ---

#[test]
fn generator_class_property_computed_name_yield_reports_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1] = 2; } }
"#,
    );
    assert!(codes.contains(&7057), "class data property; got {codes:?}");
}

#[test]
fn generator_object_literal_property_computed_name_yield_reports_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { const bag = { [yield 1]: 1 }; }
"#,
    );
    assert!(
        codes.contains(&7057),
        "object-literal property; got {codes:?}"
    );
}

#[test]
fn generator_interface_property_signature_computed_name_yield_reports_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { interface Shape { [yield 1]: number } }
"#,
    );
    assert!(
        codes.contains(&7057),
        "interface property signature; got {codes:?}"
    );
}

// --- Function members: TS7057 must NOT fire. ---
//
// The class/object method rows guard the pre-existing (correct) silence; the
// interface / type-literal method-signature rows are the #17057 fix — tsz used
// to emit a false-positive TS7057 there because the bodyless signature has no
// function boundary to stop the yield dispatch, unlike a method with a body.

#[test]
fn generator_class_method_computed_name_yield_no_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1]() {} } }
"#,
    );
    assert!(!codes.contains(&7057), "class method; got {codes:?}");
}

#[test]
fn generator_class_getter_computed_name_yield_no_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { get [yield 1]() { return 1; } } }
"#,
    );
    assert!(!codes.contains(&7057), "class getter; got {codes:?}");
}

#[test]
fn generator_class_static_method_computed_name_yield_no_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { static [yield 1]() {} } }
"#,
    );
    assert!(!codes.contains(&7057), "class static method; got {codes:?}");
}

#[test]
fn generator_object_literal_method_computed_name_yield_no_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { const o = { [yield 1]() {} }; }
"#,
    );
    assert!(
        !codes.contains(&7057),
        "object-literal method; got {codes:?}"
    );
}

#[test]
fn generator_interface_method_signature_computed_name_yield_no_ts7057() {
    // #17057 regression: a bodyless interface method signature used to emit a
    // false-positive TS7057; tsc reports only TS1169 here.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { interface Shape { [yield 1](): void } }
"#,
    );
    assert!(
        !codes.contains(&7057),
        "interface method signature is a function member; got {codes:?}"
    );
}

#[test]
fn generator_type_literal_method_signature_computed_name_yield_no_ts7057() {
    // #17057 regression, type-literal twin of the interface method signature.
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { type Shape2 = { [yield 1](): void }; }
"#,
    );
    assert!(
        !codes.contains(&7057),
        "type-literal method signature is a function member; got {codes:?}"
    );
}

#[test]
fn renamed_binder_generator_interface_method_signature_computed_name_yield_no_ts7057() {
    // Anti-hardcoding: the rule keys on member kind, not identifier spelling.
    let codes = check_source_codes_with_parse_health(
        r#"
function* makeStream() { interface ConnectionPool { [yield token](): void } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}

#[test]
fn async_generator_interface_method_signature_computed_name_yield_no_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
async function* gen() { interface Shape { [yield 1](): void } }
"#,
    );
    assert!(!codes.contains(&7057), "got {codes:?}");
}
