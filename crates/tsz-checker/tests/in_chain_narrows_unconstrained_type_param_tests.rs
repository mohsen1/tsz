//! Tests that the truthy branch of `prop in x` narrows an unconstrained
//! type parameter to `T & object`, so that subsequent `in` checks (or
//! property accesses) in the same `&&` chain do not re-emit TS2322.
//!
//! Regression for `conditionalTypeDoesntSpinForever.ts`: tsz used to
//! emit TS2322 ("Type 'T' is not assignable to type 'object'") for every
//! `in` operator in an `&&` chain when the operand was an unconstrained
//! type parameter, instead of just for the first one. tsc narrows
//! after each successful `in`, so subsequent checks see `T & object`
//! and pass the operand-type check.

#[test]
fn in_chain_emits_ts2322_only_at_first_link_for_unconstrained_type_param() {
    let source = r#"
const f = <T>(x: T) => "a" in x && "b" in x && "c" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly 1 TS2322 (only the first `in` on an unconstrained T), got: {:?}",
        ts2322
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
    }
}

#[test]
fn in_chain_in_or_chain_still_emits_per_link() {
    // The narrowing only applies in the `&&` truthy chain. In a `||` chain
    // each `in` check is independent (the right side runs when the left
    // failed) so the operand-type error must still fire for each one.
    let source = r#"
const f = <T>(x: T) => "a" in x || "b" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        2,
        "|| chain should report each `in` independently, got: {:?}",
        ts2322
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
