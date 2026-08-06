//! TS2407: a *union* for-in operand is quantified with ALL, not ANY.
//!
//! `tsc`'s `checkForInStatement` gates the RHS on
//! `allTypesAssignableToKind(rightType, NonPrimitive | InstantiableNonPrimitive)`,
//! and `allTypesAssignableToKind` recurses into a union with `every`. A single
//! non-object constituent therefore rejects the whole operand.
//! `for_in_expr_type_is_valid_union` returned on the FIRST valid member, so
//! every mixed union — `string | object`, `{ a: number } | number`,
//! `Shape | number` — was silently accepted and tsz reported nothing where
//! `tsc` reports TS2407.
//!
//! Every row below was run through the pinned oracle before being written
//! (`scripts/conformance/oracle.sh <case>.ts --strict --target es2022 --lib
//! es2022`, `tsc` 7.0.2 with the `--singleThreaded --stableTypeOrdering` flags
//! the cache generator adds for TypeScript 7+, see #16413). The mixed-union
//! rows were also re-run without `--strict` and behave identically.
//!
//! Two constituent-level rules ride along with the quantifier, both oracled:
//!
//! - `null`/`undefined` constituents are stripped, not judged
//!   (`getNonNullableTypeIfNeeded`), so `{ a: number } | undefined` stays
//!   clean. Without the strip, ALL would invent a false positive on every
//!   optional object operand.
//! - A type variable is a valid operand ON ITS OWN but contributes to a union
//!   only through its base constraint: `f<T>(u: T)` is clean, `f<T>(u: T | { a:
//!   number })` reports, and constraining `T` to an object-like type makes the
//!   union clean again.
//!
//! Binder names are varied across rows (`u`, `holder`, `payload`, `subject`,
//! `bag`) and the type-parameter rows use `T`/`Item`/`Elem`, so nothing here
//! can be satisfied by a user-chosen identifier.

use crate::test_utils::{
    check_source_non_strict, check_source_strict_messages, diagnostic_code_messages,
};

const TS2407: u32 = 2407;

fn ts2407_types(messages: &[(u32, String)]) -> Vec<String> {
    messages
        .iter()
        .filter(|(code, _)| *code == TS2407)
        .map(|(_, text)| {
            text.rsplit_once("but here has type ").map_or_else(
                || text.clone(),
                |(_, ty)| ty.trim_end_matches('.').to_string(),
            )
        })
        .collect()
}

/// TS2407 codes reported for `source` under `--strict`, as the operand types
/// named by the message (empty when the operand is accepted).
fn strict_ts2407(source: &str) -> Vec<String> {
    ts2407_types(&check_source_strict_messages(source))
}

fn non_strict_ts2407(source: &str) -> Vec<String> {
    ts2407_types(&diagnostic_code_messages(check_source_non_strict(source)))
}

fn assert_reports(source: &str) {
    assert_eq!(
        strict_ts2407(source).len(),
        1,
        "expected exactly one TS2407 for {source:?}, got {:?}",
        check_source_strict_messages(source)
    );
}

fn assert_clean(source: &str) {
    assert!(
        strict_ts2407(source).is_empty(),
        "expected no TS2407 for {source:?}, got {:?}",
        check_source_strict_messages(source)
    );
}

// ---------------------------------------------------------------------------
// The quantifier itself: one bad constituent rejects the union.
// ---------------------------------------------------------------------------

#[test]
fn union_of_string_and_object_reports() {
    // The witness from the report: `object` alone is a fine operand, but the
    // union is not, because `string` is not assignable to `object`.
    assert_reports("declare const u: string | object;\nfor (var i in u) {}");
}

#[test]
fn union_of_object_type_and_number_reports() {
    assert_reports("declare const holder: { a: number } | number;\nfor (var key in holder) {}");
}

#[test]
fn union_of_interface_and_number_reports() {
    // Renamed binder + a named interface rather than an anonymous literal:
    // the rule is structural, not keyed to any spelling.
    assert_reports(
        "interface Shape { side: number }\ndeclare const payload: Shape | number;\nfor (var key in payload) {}",
    );
}

#[test]
fn union_of_object_type_and_boolean_symbol_or_void_reports() {
    for bad in ["boolean", "symbol", "void"] {
        assert_reports(&format!(
            "declare const subject: {{ a: number }} | {bad};\nfor (var key in subject) {{}}"
        ));
    }
}

#[test]
fn union_of_enum_and_object_type_reports() {
    assert_reports(
        "enum Channel { Red }\ndeclare const bag: Channel | { a: number };\nfor (var key in bag) {}",
    );
}

#[test]
fn union_of_string_literals_reports() {
    assert_reports("declare const bag: 'a' | 'b';\nfor (var key in bag) {}");
}

#[test]
fn union_of_two_primitives_reports() {
    assert_reports("declare const u: string | number;\nfor (var i in u) {}");
}

#[test]
fn one_bad_constituent_among_three_reports() {
    assert_reports("declare const u: { a: 1 } | { b: 2 } | string;\nfor (var i in u) {}");
}

#[test]
fn nested_union_alias_hiding_a_primitive_reports() {
    // The bad constituent is one level down, behind an alias — the ALL rule
    // has to recurse, not just scan the top level.
    assert_reports(
        "type Inner = { a: number } | number;\ndeclare const u: Inner | { b: string };\nfor (var i in u) {}",
    );
}

#[test]
fn generic_application_union_with_a_primitive_reports() {
    assert_reports(
        "type Box<T> = { value: T };\ndeclare const u: Box<number> | number;\nfor (var i in u) {}",
    );
}

// ---------------------------------------------------------------------------
// Negatives: all-object unions stay clean (no false positive from the flip).
// ---------------------------------------------------------------------------

#[test]
fn union_of_two_object_types_is_clean() {
    assert_clean("declare const u: { a: number } | { b: string };\nfor (var i in u) {}");
}

#[test]
fn union_of_three_object_types_is_clean() {
    assert_clean("declare const u: { a: 1 } | { b: 2 } | { c: 3 };\nfor (var i in u) {}");
}

#[test]
fn union_of_object_type_aliases_is_clean() {
    assert_clean(
        "type A = { a: number };\ntype B = { b: number };\ndeclare const holder: A | B;\nfor (var key in holder) {}",
    );
}

#[test]
fn union_of_generic_applications_is_clean() {
    // Deferred members: `Box<number>` is object-like only after per-member
    // resolution, which must not collapse the union.
    assert_clean(
        "type Box<T> = { value: T };\ndeclare const u: Box<number> | Box<string>;\nfor (var i in u) {}",
    );
}

#[test]
fn union_with_an_array_or_function_constituent_is_clean() {
    assert_clean("declare const u: { a: number } | string[];\nfor (var i in u) {}");
    assert_clean("declare const u: { a: number } | (() => void);\nfor (var i in u) {}");
}

#[test]
fn union_with_any_is_clean() {
    assert_clean("declare const u: { a: number } | any;\nfor (var i in u) {}");
}

#[test]
fn union_of_object_type_and_intersection_is_clean() {
    // The quantifier flips back inside an intersection constituent: `A & B` is
    // a subtype of each member, so ANY object-like member suffices there.
    assert_clean(
        "declare const u: ({ a: number } & { b: number }) | { c: number };\nfor (var i in u) {}",
    );
}

// ---------------------------------------------------------------------------
// The nullable strip: `getNonNullableTypeIfNeeded`, not a judged constituent.
// ---------------------------------------------------------------------------

#[test]
fn optional_object_operand_is_clean() {
    assert_clean("declare const u: { a: number } | undefined;\nfor (var i in u) {}");
    assert_clean("declare const holder: { a: number } | null;\nfor (var key in holder) {}");
    assert_clean(
        "declare const payload: { a: number } | null | undefined;\nfor (var key in payload) {}",
    );
}

#[test]
fn optional_primitive_operand_still_reports() {
    // The strip removes `undefined` and then judges what is left, so this must
    // still report — it is the strip, not a blanket exemption for optionals.
    assert_reports("declare const u: string | undefined;\nfor (var i in u) {}");
}

#[test]
fn mixed_union_reports_in_non_strict_mode_too() {
    // The rule is an assignability fact about the constituents, not a
    // strictNullChecks-dependent one; oracled both ways.
    assert_eq!(
        non_strict_ts2407("declare const u: string | { a: number };\nfor (var i in u) {}").len(),
        1
    );
    assert!(
        non_strict_ts2407("declare const u: { a: number } | { b: string };\nfor (var i in u) {}")
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Type variables: valid alone, but only via an object-like constraint in a
// union.
// ---------------------------------------------------------------------------

#[test]
fn bare_type_parameter_operand_is_clean() {
    // Unchanged by this fix, and the reason the union rule cannot simply reuse
    // the leaf predicate.
    assert_clean("function f<T>(u: T) { for (var i in u) {} }");
    assert_clean("function f<Item extends string>(u: Item) { for (var i in u) {} }");
}

#[test]
fn unconstrained_type_parameter_in_a_union_reports() {
    assert_reports("function f<T>(u: T | { a: number }) { for (var i in u) {} }");
    // Constituent order must not matter.
    assert_reports("function f<T>(u: { a: number } | T) { for (var i in u) {} }");
    assert_reports("function f<T, U>(u: T | U) { for (var i in u) {} }");
}

#[test]
fn explicitly_unknown_constrained_type_parameter_in_a_union_reports() {
    assert_reports(
        "function f<Elem extends unknown>(u: Elem | { b: number }) { for (var i in u) {} }",
    );
}

#[test]
fn primitive_constrained_type_parameter_in_a_union_reports() {
    assert_reports(
        "function f<Elem extends string>(u: Elem | { b: number }) { for (var i in u) {} }",
    );
}

#[test]
fn object_constrained_type_parameter_in_a_union_is_clean() {
    assert_clean("function f<T extends object>(u: T | { a: number }) { for (var i in u) {} }");
    assert_clean(
        "function f<Item extends { a: number }>(u: Item | { b: number }) { for (var i in u) {} }",
    );
    assert_clean(
        "function f<Elem extends unknown[]>(u: Elem | { b: number }) { for (var i in u) {} }",
    );
}

#[test]
fn indexed_access_on_an_unconstrained_parameter_in_a_union_reports() {
    assert_reports("function f<T>(u: T[keyof T] | { b: number }) { for (var i in u) {} }");
}

// ---------------------------------------------------------------------------
// Non-union operands are untouched by the quantifier change.
// ---------------------------------------------------------------------------

#[test]
fn non_union_operands_keep_their_verdicts() {
    assert_clean("declare const u: object;\nfor (var i in u) {}");
    assert_clean("declare const u: { a: number };\nfor (var i in u) {}");
    assert_clean("declare const u: any;\nfor (var i in u) {}");
    assert_reports("declare const u: unknown;\nfor (var i in u) {}");
    assert_reports("declare const u: string;\nfor (var i in u) {}");
}

#[test]
fn union_collapsing_to_unknown_reports() {
    // `{ a: number } | unknown` is `unknown`, which is not a valid operand.
    assert_reports("declare const u: { a: number } | unknown;\nfor (var i in u) {}");
}
