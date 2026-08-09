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
//! identically in both; the one place the harness and CLI can drift is the
//! *type-inference* TS7057 row, which is why the class-method TS7057 gap is
//! documented as an `#[ignore]`d row at the end rather than asserted live (see
//! its comment and the linked follow-up issue).

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
// Documented follow-up: the TS7057 type-inference gap for function-body class
// members.
//
// The TS1163 *grammar* axis above is uniform across every member kind: the
// container jump reaches the enclosing generator for methods, accessors,
// properties, object literals and interface signatures alike. The
// type-inference axis is not. Inside an unannotated generator, a `yield` in a
// computed name whose type cannot be pinned should raise TS7057 (`'yield'
// expression implicitly results in an 'any' type because its containing
// generator lacks a return-type annotation`). The compiled CLI already does so
// for object-literal computed names, class *property* computed names, and
// *interface* method-signature computed names — but drops it for the
// function-body class members (method / getter / setter / static method),
// whose own function-type construction resets the enclosing generator's
// yield-collection state before the name is evaluated.
//
// This is a genuine tsz false negative (verified against the CLI, not just this
// harness), tracked separately from the do-not-patch #16104 await parity as
// #17057. The row below asserts the correct target and is `#[ignore]`d so it
// documents the gap without gating CI, following the repo's established pattern
// (`generator_yield_self_similar_nesting_tests.rs`).
#[test]
#[ignore = "#17057 open: a `yield` in a function-body class member's computed \
name inside an unannotated generator should raise TS7057 like the object-literal \
/ class-property / interface-signature siblings do, but the member's \
function-type construction resets the generator's yield-collection state before \
the name is checked."]
fn generator_class_method_computed_name_yield_should_report_ts7057() {
    let codes = check_source_codes_with_parse_health(
        r#"
function* gen() { class Holder { [yield 1]() {} } }
"#,
    );
    assert!(
        codes.contains(&7057),
        "a function-body class member's computed-name yield should raise TS7057 like its siblings; got {codes:?}"
    );
}
