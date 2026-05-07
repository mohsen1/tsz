//! Tests that the truthy branch of `prop in x` narrows an unconstrained
//! type parameter to `T & Record<prop, unknown>`, so that subsequent `in`
//! checks (or property accesses) in the same `&&` chain do not re-emit
//! invalid-RHS diagnostics.
//!
//! Regression for `conditionalTypeDoesntSpinForever.ts`: tsz used to
//! emit an invalid-RHS diagnostic for every `in` operator in an `&&` chain
//! when the operand was an unconstrained type parameter, instead of just the
//! first TS2322. tsc narrows after each successful `in`, so subsequent checks
//! see `T & Record<..., unknown>` and pass the operand-type check.

#[test]
fn in_chain_emits_ts2322_only_at_first_link_for_unconstrained_type_param() {
    let source = r#"
const f = <T>(x: T) => "a" in x && "b" in x && "c" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    let ts2638: Vec<_> = diagnostics.iter().filter(|d| d.code == 2638).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly 1 TS2322 (only the first `in` on an unconstrained T), got: {:?}",
        ts2322
            .iter()
            .map(|d| (d.start, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        ts2638.is_empty(),
        "bare `T` should use TS2322, not TS2638; got: {:?}",
        ts2638
            .iter()
            .map(|d| (d.start, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn in_chain_narrowing_keys_off_token_kind_not_param_or_property_names() {
    // The fix is structural: any unconstrained type parameter should narrow
    // via `in` regardless of the names chosen for the parameter or the
    // property literals.
    for (tparam, props) in [
        ("T", ["a", "b", "c"]),
        ("U", ["x", "y", "z"]),
        ("MyParam", ["foo", "bar", "baz"]),
        ("_", ["__a", "__b", "__c"]),
    ] {
        let p0 = props[0];
        let p1 = props[1];
        let p2 = props[2];
        let source = format!(
            r#"const f = <{tparam}>(x: {tparam}) => "{p0}" in x && "{p1}" in x && "{p2}" in x;"#
        );
        let diagnostics = tsz_checker::test_utils::check_source_diagnostics(&source);
        let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
        let ts2638: Vec<_> = diagnostics.iter().filter(|d| d.code == 2638).collect();
        assert_eq!(
            ts2322.len(),
            1,
            "param={tparam} props={props:?}: expected 1 TS2322, got {} ({:?})",
            ts2322.len(),
            ts2322
                .iter()
                .map(|d| d.message_text.clone())
                .collect::<Vec<_>>()
        );
        assert!(
            ts2638.is_empty(),
            "param={tparam} props={props:?}: bare RHS should not produce TS2638 ({:?})",
            ts2638
                .iter()
                .map(|d| d.message_text.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn in_chain_in_or_chain_still_emits_ts2322_per_link() {
    // The narrowing only applies in the `&&` truthy chain. In a `||` chain
    // each `in` check is independent (the right side runs when the left
    // failed) so the operand-type error must still fire for each one.
    let source = r#"
const f = <T>(x: T) => "a" in x || "b" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    let ts2638: Vec<_> = diagnostics.iter().filter(|d| d.code == 2638).collect();
    assert_eq!(
        ts2322.len(),
        2,
        "|| chain should report each `in` independently with TS2322, got: {:?}",
        ts2322
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        ts2638.is_empty(),
        "bare `T` in a `||` chain should not produce TS2638, got: {:?}",
        ts2638
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn constrained_type_param_keeps_existing_behavior() {
    // T extends object — operand type already valid, no TS2322 expected.
    let source = r#"
const f = <T extends object>(x: T) => "a" in x && "b" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "constrained `T extends object` should not produce TS2322 for `in`, got: {:?}",
        ts2322
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

fn ts2638_count(source: &str) -> usize {
    tsz_checker::test_utils::check_source_codes(source)
        .into_iter()
        .filter(|&code| code == 2638)
        .count()
}

#[test]
fn in_operator_rejects_generic_nonnullable_intersections() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function f<P>(a: P & {}) {
  "foo" in a;
}

type NonNull<T> = T & {};
function g<T>(a: NonNull<T>) {
  "foo" in a;
}
"#,
    );

    let ts2638: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2638)
        .collect();
    assert_eq!(
        ts2638.len(),
        2,
        "Expected TS2638 for both generic non-nullable RHS values, got {diagnostics:#?}"
    );
    assert!(
        ts2638
            .iter()
            .any(|(_, message)| message.contains("NonNullable<P>")),
        "Expected diagnostic to mention NonNullable<P>, got {diagnostics:#?}"
    );
    assert!(
        ts2638
            .iter()
            .any(|(_, message)| message.contains("NonNull<T>")),
        "Expected diagnostic to mention NonNull<T>, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_allows_object_constrained_intersections() {
    let source = r#"
function object_rhs(a: object) {
  "foo" in a;
}

function object_intersection<T>(a: T & object) {
  "foo" in a;
}

function non_empty_object_intersection<T>(a: T & { value: number }) {
  "foo" in a;
}

interface EmptyInterface {}
function empty_interface_intersection<T>(a: T & EmptyInterface) {
  "foo" in a;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_code_messages(source);
    assert!(
        diagnostics.is_empty(),
        "Expected object-constrained RHS values to be accepted, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_rejects_empty_object_type_alias_intersection() {
    let source = r#"
type Empty = {};
function alias_intersection<T>(a: T & Empty) {
  "foo" in a;
}
"#;

    assert_eq!(
        ts2638_count(source),
        1,
        "Expected empty object type alias intersection to emit TS2638"
    );
}

#[test]
fn in_operator_narrows_unique_symbol_property_presence_on_object() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
const sym = Symbol();
function f(x: object) {
  if ("a" in x && 1 in x && sym in x) {
    x[sym];
  }
}
"#,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 7053),
        "Expected unique-symbol `in` narrowing to suppress TS7053, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_reports_ts2322_for_bare_generic_rhs() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function f<T>(x: T) {
  if ("a" in x) {
    x.a;
  }
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2322 && message.contains("Type 'T'") }),
        "Expected TS2322 for bare generic `in` RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2638),
        "Expected no TS2638 for bare generic `in` RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected `in` narrowing to expose property `a`, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_reports_ts2322_not_ts2638_for_primitive_intersections() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function intersection2<T>(thing: T & (0 | 1 | 2)) {
  "key" in thing;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("T &") && message.contains("not assignable")
        }),
        "Expected TS2322 for primitive-only intersection RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2638),
        "Expected no TS2638 for primitive-only intersection RHS, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_reports_ts2322_not_ts2638_for_generic_symbol_key_lookup() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function hasField<SO_FAR>(soFar: SO_FAR, fieldName: string | number | symbol) {
  return fieldName in soFar;
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2322 && message.contains("SO_FAR") }),
        "Expected TS2322 for bare generic RHS with symbolic key, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2638),
        "Expected no TS2638 for bare generic RHS with symbolic key, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_keeps_generic_and_unknown_property_narrowing_through_truthiness_chains() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function unknownCase(x: unknown) {
  if (x && "a" in x) {
    x.a;
  }
  if (x && typeof x === "object" && "a" in x) {
    x.a;
  }
}

function genericCase<T>(x: T) {
  if (x && "a" in x) {
    x.a;
  }
  if (x && typeof x === "object" && "a" in x) {
    x.a;
  }
  if (x && typeof x === "object" && "a" in x && "b" in x && "c" in x) {
    x.a;
    x.b;
    x.c;
  }
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2638 && message.contains("{}") }),
        "Expected TS2638 for unknown or truthiness-narrowed unknown, got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2638 && message.contains("NonNullable<T>") }),
        "Expected TS2638 for truthiness-narrowed generic, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected property accesses to use the `in`-narrowed record shape, got {diagnostics:#?}"
    );
}
