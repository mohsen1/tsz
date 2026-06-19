//! Display parity for concrete indexed-access types in assignability
//! diagnostics.
//!
//! `tsc` resolves a *concrete* indexed-access type (`Obj["m"]` whose object and
//! key are fully resolved, with no free type parameters) to its member type
//! during type construction (`getIndexedAccessType`), so the type is never an
//! indexed access by the time a diagnostic renders it — `tsc` shows the reduced
//! member shape. tsz kept the access deferred and rendered the unreduced
//! `Obj["m"]` surface in `TS2741`/`TS2322` assignability messages.
//!
//! The display policy now reduces a bare, type-parameter-free indexed access
//! with a literal key to its member type for the assignment source/target
//! roles, matching `tsc`. Generic/deferred accesses (a free type parameter in
//! the object or key) stay opaque — `tsc` renders `T["m"]` there too — and are
//! guarded by the existing `deferred_keyof_index_access_assignability_tests` /
//! `deferred_conditional_indexed_access_tests` suites.

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2741_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2741)
        .map(|(_, message)| message)
        .collect()
}

fn assert_reduced_member(message: &str, expected_member: &str) {
    assert!(
        message.contains(expected_member),
        "source must render the reduced member shape `{expected_member}`: {message}"
    );
    assert!(
        !message.contains("[\"") && !message.contains("['"),
        "source must not render the unreduced indexed-access surface: {message}"
    );
}

/// String-literal key: `Obj["m"]` renders as its member object, not `Obj["m"]`.
#[test]
fn concrete_string_indexed_access_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Obj { m: { foo: string } }
declare function get(): Obj["m"];
const bad: { foo: string; bar: string } = get();
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ foo: string; }");
}

/// Anti-hardcoding: the reduction keys on the structural concrete-indexed-access
/// condition, not on the identifiers `Obj`/`m`. Renamed binders behave the same.
#[test]
fn concrete_indexed_access_source_reduction_is_binder_name_independent() {
    let messages = ts2741_messages(
        r#"
interface Container { payload: { alpha: string } }
declare function fetchPayload(): Container["payload"];
const bad: { alpha: string; beta: string } = fetchPayload();
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ alpha: string; }");
}

/// Numeric-literal key: `Wrap[0]` reduces the same way.
#[test]
fn concrete_numeric_indexed_access_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Wrap { 0: { a: number } }
declare function g(): Wrap[0];
const bad: { a: number; b: number } = g();
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ a: number; }");
}

/// The reduced member must still be related structurally, so a target that the
/// member *does* satisfy stays clean — the reduction is display-only and never
/// fabricates or suppresses a mismatch.
#[test]
fn concrete_indexed_access_assignable_target_stays_clean() {
    let messages = ts2741_messages(
        r#"
interface Obj { m: { foo: string } }
declare function get(): Obj["m"];
const ok: { foo: string } = get();
"#,
    );
    assert!(
        messages.is_empty(),
        "an assignable indexed-access member must not error: {messages:?}"
    );
}
