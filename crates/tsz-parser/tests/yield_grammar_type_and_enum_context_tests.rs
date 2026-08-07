//! Regression coverage for the `yield`-grammar (TS1163) mirror of the
//! `await` computed-name grammar family (#16094/#16099/#16100/#16103/#16104).
//!
//! `yield` legality is a parser-context decision in tsz: `parse_yield_expression`
//! reports TS1163 ("A 'yield' expression is only allowed in a generator body")
//! from the parser, keyed on `in_generator_context()`. Two positions parse
//! *outside* the enclosing generator's yield context in `tsc`, but tsz used to
//! keep the enclosing context and so under-reported:
//!
//! 1. **Type-literal computed names.** `tsc` parses every type under
//!    `doOutsideOfContext(TypeExcludesFlags)`, and `TypeExcludesFlags` clears
//!    `YieldContext`, so a `{ [yield x]: T }` member name reports TS1163 even
//!    inside a `function*`. An `interface` body is reached through
//!    `parseObjectTypeMembers` (not `parseType`), so it *keeps* the enclosing
//!    context — the same asymmetry the `await` side pins in #16103. Fixed by
//!    clearing `CONTEXT_FLAG_GENERATOR` for a type literal's members in
//!    `parse_type_literal_rest` (which interfaces do not route through).
//! 2. **Enum member initializers.** `tsc` reports TS1163 for
//!    `enum E { A = yield x }` even inside a `function*`: an enum member
//!    initializer is its own container for this check (the mirror of the
//!    `await`/enum-member own-container rule the checker already owns). Fixed by
//!    clearing `CONTEXT_FLAG_GENERATOR` around the initializer parse in
//!    `parse_enum_members`.
//!
//! Every expectation below is pinned against a live `tsc --noEmit --strict
//! --target es2022 --module esnext` run (grammar-level TS1163 is stable across
//! tsc versions), and cross-checked against the compiled `tsz` CLI.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

const TS1163: u32 = diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY;

fn ts1163_count(source: &str) -> usize {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == TS1163)
        .count()
}

// --- Type-literal computed names inside a generator: now report TS1163 ---

#[test]
fn type_literal_property_signature_computed_yield_in_generator_reports_ts1163() {
    // The type literal is parsed outside the generator's yield context.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { type T = { [yield x]: number }; }"),
        1,
    );
}

#[test]
fn type_literal_method_signature_computed_yield_in_generator_reports_ts1163() {
    // A method signature is a separate arm of the type-literal member walk.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { type T = { [yield x](): number }; }"),
        1,
    );
}

#[test]
fn type_literal_in_variable_annotation_computed_yield_in_generator_reports_ts1163() {
    // No type alias involved — a type literal reached through a variable's
    // type annotation answers the same way.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { let slot: { [yield x]: number }; }"),
        1,
    );
}

#[test]
fn type_literal_nested_in_interface_member_computed_yield_in_generator_reports_ts1163() {
    // The rule is keyed on the *type literal*, not the alias: a literal reached
    // through an `interface` member's type annotation still clears the context,
    // even though the enclosing interface member's own name would not (see the
    // interface control below).
    assert_eq!(
        ts1163_count(
            "declare const x: any; function* g() { interface O { inner: { [yield x]: number } } }"
        ),
        1,
    );
}

#[test]
fn deeply_nested_type_literal_computed_yield_in_generator_reports_ts1163() {
    assert_eq!(
        ts1163_count(
            "declare const x: any; function* g() { type T = { a: { b: { [yield x]: number } } }; }"
        ),
        1,
    );
}

#[test]
fn async_generator_type_literal_computed_yield_reports_ts1163() {
    assert_eq!(
        ts1163_count(
            "declare const x: any; async function* g() { type T = { [yield x]: number }; }"
        ),
        1,
    );
}

#[test]
fn type_literal_computed_yield_is_binder_name_invariant() {
    // Anti-hardcoding: nothing depends on the spelling of the key, alias, or
    // the generator.
    assert_eq!(
        ts1163_count(
            "declare const tokenValue: any; function* produce() { type Shape = { [yield tokenValue]: number }; }"
        ),
        1,
    );
}

// --- Enum member initializers inside a generator: now report TS1163 ---

#[test]
fn enum_member_initializer_yield_in_generator_reports_ts1163() {
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { enum E { A = yield x } }"),
        1,
    );
}

#[test]
fn enum_member_initializer_yield_in_generator_is_binder_name_invariant() {
    assert_eq!(
        ts1163_count(
            "declare const seedValue: any; function* generate() { enum Palette { First = yield seedValue } }"
        ),
        1,
    );
}

// --- Regression guards: positions that MUST keep the enclosing context ---

#[test]
fn interface_member_computed_yield_in_generator_stays_ts1163_free() {
    // An interface body is parsed through `parse_type_members`, not
    // `parse_type_literal_rest`, so it keeps the enclosing generator context —
    // exactly tsc's `parseObjectTypeMembers` asymmetry.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { interface I { [yield x]: number } }"),
        0,
    );
}

#[test]
fn class_method_computed_yield_in_generator_stays_ts1163_free() {
    // A class member's computed name is evaluated in the enclosing scope, so an
    // enclosing generator makes `yield` legal there.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { class C { [yield x]() {} } }"),
        0,
    );
}

#[test]
fn object_literal_computed_yield_in_generator_stays_ts1163_free() {
    // An object literal is a value position and keeps the enclosing async/yield
    // context.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { const o = { [yield x]: 1 }; }"),
        0,
    );
}

#[test]
fn nested_generator_expression_in_type_literal_name_keeps_its_own_context() {
    // The clearing must not leak into a nested `function*` inside the computed
    // name: that expression re-establishes its own generator context, so its
    // `yield` stays legal (no TS1163).
    assert_eq!(
        ts1163_count(
            "declare const x: any; function* g() { type T = { [(function*(){ yield x; return 1; })()]: number }; }"
        ),
        0,
    );
}

#[test]
fn type_literal_index_signature_in_generator_is_ts1163_free() {
    // An index signature `[k: string]` is not a computed-name expression; the
    // clearing must not manufacture a diagnostic here.
    assert_eq!(
        ts1163_count("declare const x: any; function* g() { type T = { [k: string]: number }; }"),
        0,
    );
}

// --- Positions that already reported TS1163 must still report it ---

#[test]
fn type_literal_computed_yield_at_top_level_still_reports_ts1163() {
    // No enclosing generator at all — the type literal was never in yield
    // context, and stays a TS1163 site.
    assert_eq!(
        ts1163_count("declare const x: any; type T = { [yield x]: number };"),
        1,
    );
}

#[test]
fn enum_member_initializer_yield_at_top_level_still_reports_ts1163() {
    assert_eq!(
        ts1163_count("declare const x: any; enum E { A = yield x }"),
        1,
    );
}

#[test]
fn type_literal_computed_yield_in_plain_function_still_reports_ts1163() {
    // A plain (non-generator) function never had yield context to begin with.
    assert_eq!(
        ts1163_count("declare const x: any; function f() { type T = { [yield x]: number }; }"),
        1,
    );
}
