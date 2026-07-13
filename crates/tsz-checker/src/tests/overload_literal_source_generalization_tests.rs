//! Regression tests for the source-type display of each overload's argument
//! failure in a TS2769 ("No overload matches this call") elaboration.
//!
//! tsc's `reportRelationError` generalizes a fresh literal source to its base
//! type unless the target could hold a top-level singleton type (a `never`
//! target also preserves the raw literal). tsc 7.0.2 renders only the LAST
//! argument-error candidate under the single `The last overload gave the
//! following error.` header, so the assertions below key on that candidate's
//! message. Every expectation is differential-verified against the pinned
//! tsc 7.0.2 binary.
//!
//! See `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
//! (`error_no_overload_matches_at`) and
//! `tsz_solver::type_queries::type_could_have_top_level_singleton_types`.

use crate::test_utils::check_source_diagnostics;

/// Collect the related-information messages of the single TS2769 diagnostic.
fn overload_failure_messages(source: &str) -> Vec<String> {
    let diags = check_source_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "Expected exactly one TS2769. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2769[0]
        .related_information
        .iter()
        .map(|r| r.message_text.clone())
        .collect()
}

/// A boolean-literal argument against `number`/`string` overload parameters: the
/// per-overload source is generalized to `boolean` (target is not a singleton),
/// matching the single-overload TS2345 display and tsc.
#[test]
fn boolean_literal_argument_widens_to_boolean_against_primitive_overloads() {
    let messages = overload_failure_messages(
        r#"
declare function f(x: number): void;
declare function f(x: string): void;
f(true);
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m
                == "Argument of type 'boolean' is not assignable to parameter of type 'string'."),
        "expected widened 'boolean' source against the LAST (string) overload, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Argument of type 'true'")),
        "the raw boolean literal must not leak into the overload elaboration, got: {messages:?}"
    );
}

/// A numeric-literal argument against `string`/`boolean` overload parameters:
/// generalized to `number` (neither target is a singleton).
#[test]
fn numeric_literal_argument_widens_to_number_against_primitive_overloads() {
    let messages = overload_failure_messages(
        r#"
declare function f(x: string): void;
declare function f(x: boolean): void;
f(1);
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m
                == "Argument of type 'number' is not assignable to parameter of type 'boolean'."),
        "expected widened 'number' source against the LAST (boolean) overload, got: {messages:?}"
    );
}

/// Control: when every overload parameter is itself a literal/singleton type,
/// the source literal is preserved (tsc keeps the literal-vs-literal mismatch
/// legible). The fix must not over-widen these.
#[test]
fn literal_argument_preserved_against_literal_overload_targets() {
    let messages = overload_failure_messages(
        r#"
declare function f(x: 1): void;
declare function f(x: 2): void;
f(true);
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m == "Argument of type 'true' is not assignable to parameter of type '2'."),
        "literal source must be preserved against the LAST literal target, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("'boolean'")),
        "must not widen the source against a singleton target, got: {messages:?}"
    );
}

/// Control: a union target with at least one singleton member preserves the
/// source literal (tsc's `typeCouldHaveTopLevelSingletonTypes` is any-member),
/// even though the union also contains a non-singleton (`string`).
#[test]
fn literal_argument_preserved_against_union_with_singleton_member() {
    let messages = overload_failure_messages(
        r#"
declare function f(x: 1 | string): void;
declare function f(x: 2 | string): void;
f(true);
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m.contains("Argument of type 'true'")),
        "literal source must be preserved when the union target has a singleton member, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Argument of type 'boolean'")),
        "must not widen against a union target that could hold a singleton, got: {messages:?}"
    );
}

/// Anti-hardcoding: the rule is structural, not keyed to specific binder names
/// or parameter spellings. A renamed callee/parameter produces the same
/// generalized display.
#[test]
fn generalization_is_independent_of_binder_names() {
    let messages = overload_failure_messages(
        r#"
declare function pick(value: number): void;
declare function pick(value: string): void;
pick(false);
"#,
    );

    assert!(
        messages
            .iter()
            .any(|m| m
                == "Argument of type 'boolean' is not assignable to parameter of type 'string'."),
        "expected widened 'boolean' regardless of binder names, got: {messages:?}"
    );
}
