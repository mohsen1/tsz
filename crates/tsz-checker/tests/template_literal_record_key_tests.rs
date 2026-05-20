use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// The test harness runs without lib types, so we use inline mapped-type
// syntax rather than `Record<K, V>` (which requires the lib `Record` alias).

/// Object literal with keys matching a prefix template should be assignable.
/// Issue #6184: `{ onClick: () => void }` must not get TS2322 against
/// a mapped type with a template literal key constraint.
#[test]
fn mapped_type_with_prefix_template_literal_key_accepts_matching_object() {
    let result = codes(
        r#"
type EventHandlers = { [K in `on${string}`]: () => void };
const h: EventHandlers = {
    onClick: () => {},
    onHover: () => {},
};
"#,
    );
    assert!(
        result.is_empty(),
        "expected no errors for matching keys, got: {result:?}"
    );
}

/// Suffix template literal key constraint.
#[test]
fn mapped_type_with_suffix_template_literal_key_accepts_matching_object() {
    let result = codes(
        r#"
type Handlers = { [K in `${string}Handler`]: () => void };
const h: Handlers = {
    clickHandler: () => {},
    hoverHandler: () => {},
};
"#,
    );
    assert!(result.is_empty(), "expected no errors, got: {result:?}");
}

/// Value type mismatch against a template literal mapped type must still error.
#[test]
fn mapped_type_with_template_literal_key_rejects_wrong_value_type() {
    let result = codes(
        r#"
type EventHandlers = { [K in `on${string}`]: () => void };
const h: EventHandlers = { onClick: "not a function" };
"#,
    );
    assert!(
        result.contains(&2322),
        "expected TS2322 for wrong value type, got: {result:?}"
    );
}

/// Properties whose keys do NOT match the template pattern are not constrained
/// by the index signature (structural typing allows extra properties through
/// a variable — non-fresh-object — assignment).
#[test]
fn mapped_type_with_template_literal_key_allows_extra_non_matching_properties() {
    let result = codes(
        r#"
type EventHandlers = { [K in `on${string}`]: () => void };
const obj = { onClick: () => {}, click: () => {} };
const h: EventHandlers = obj;
"#,
    );
    assert!(
        result.is_empty(),
        "expected no errors for extra non-matching key via variable, got: {result:?}"
    );
}

/// Union template literal key constraints should constrain properties matching
/// either template, while leaving unrelated keys alone.
#[test]
fn mapped_type_with_union_template_literal_key_allows_extra_non_matching_properties() {
    let result = codes(
        r#"
type EventHandlers = { [K in `on${string}` | `off${string}`]: () => void };
const obj = { onClick: () => {}, offClick: () => {}, click: 123 };
const h: EventHandlers = obj;
"#,
    );
    assert!(
        result.is_empty(),
        "expected no errors for extra non-matching key with union templates, got: {result:?}"
    );
}

/// Using a different type parameter name (K vs P) must not affect the result.
#[test]
fn mapped_type_template_literal_key_independent_of_param_name() {
    let codes_k = codes(
        r#"
type A = { [K in `data-${string}`]: string };
const a: A = { "data-foo": "x", "data-bar": "y" };
"#,
    );
    let codes_p = codes(
        r#"
type B = { [P in `data-${string}`]: string };
const b: B = { "data-foo": "x", "data-bar": "y" };
"#,
    );
    assert!(
        codes_k.is_empty(),
        "K variant: expected no errors, got: {codes_k:?}"
    );
    assert!(
        codes_p.is_empty(),
        "P variant: expected no errors, got: {codes_p:?}"
    );
}
