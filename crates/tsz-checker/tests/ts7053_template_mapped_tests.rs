/// TS7053 — Element implicitly has 'any' type because expression of type X
/// can't be used to index type Y.
///
/// These tests verify that indexing a mapped type with a template-literal key
/// constraint correctly emits TS7053 when the index expression does not satisfy
/// the template pattern, and suppresses TS7053 when it does.
///
/// Structural rule: when an object type has a string index signature whose key
/// type is a template literal (or other restricted string), any index expression
/// that is not a subtype of that key type must trigger TS7053.
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

// ──────────────────────────────────────────────────────────────────────────────
// Mapped type with `on${string}` template literal key
// ──────────────────────────────────────────────────────────────────────────────

/// Access with a non-matching literal key should emit TS7053.
#[test]
fn non_matching_literal_key_emits_ts7053() {
    let result = codes(
        r#"
type Handler = { [K in `on${string}`]?: () => void };
const h: Handler = {};
h["someKey"];
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for non-matching literal key, got: {result:?}"
    );
}

/// Access with a matching literal key should NOT emit TS7053.
#[test]
fn matching_literal_key_no_ts7053() {
    let result = codes(
        r#"
type Handler = { [K in `on${string}`]?: () => void };
const h: Handler = {};
h["onClick"];
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for matching key 'onClick', got: {result:?}"
    );
}

/// Access with another matching literal key (different spelling) should NOT emit TS7053.
#[test]
fn matching_literal_key_various_spellings_no_ts7053() {
    let result = codes(
        r#"
type Handler = { [K in `on${string}`]?: () => void };
const h: Handler = {};
h["onFoo"];
h["onBar"];
h["onBaz"];
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for matching keys, got: {result:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Template literal prefix `data-${string}`
// ──────────────────────────────────────────────────────────────────────────────

/// Non-matching key for a `data-${string}` template should emit TS7053.
#[test]
fn data_prefix_non_matching_emits_ts7053() {
    let result = codes(
        r#"
type DataAttrs = { [K in `data-${string}`]?: string };
const attrs: DataAttrs = {};
attrs["id"];
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for 'id' on data-prefixed type, got: {result:?}"
    );
}

/// Matching key for a `data-${string}` template should NOT emit TS7053.
#[test]
fn data_prefix_matching_no_ts7053() {
    let result = codes(
        r#"
type DataAttrs = { [K in `data-${string}`]?: string };
const attrs: DataAttrs = {};
attrs["data-id"];
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for 'data-id' on data-prefixed type, got: {result:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Renamed iteration variable — rule must be spelling-independent
// ──────────────────────────────────────────────────────────────────────────────

/// The mapped-type iteration variable name (K, P, T, etc.) must not affect
/// whether TS7053 fires. This guards against hardcoded name checks.
#[test]
fn renamed_iteration_var_non_matching_emits_ts7053() {
    let result_k = codes(
        r#"
type H1 = { [K in `on${string}`]?: () => void };
const h1: H1 = {};
h1["foo"];
"#,
    );
    let result_p = codes(
        r#"
type H2 = { [P in `on${string}`]?: () => void };
const h2: H2 = {};
h2["foo"];
"#,
    );
    let result_t = codes(
        r#"
type H3 = { [T in `on${string}`]?: () => void };
const h3: H3 = {};
h3["foo"];
"#,
    );
    assert!(
        result_k.contains(&7053),
        "K: expected TS7053, got: {result_k:?}"
    );
    assert!(
        result_p.contains(&7053),
        "P: expected TS7053, got: {result_p:?}"
    );
    assert!(
        result_t.contains(&7053),
        "T: expected TS7053, got: {result_t:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Type alias wrapping — should behave identically
// ──────────────────────────────────────────────────────────────────────────────

/// Non-matching access through a named type alias should still emit TS7053.
#[test]
fn type_alias_non_matching_emits_ts7053() {
    let result = codes(
        r#"
type EventMap = { [K in `on${string}`]?: () => void };
type Handler = EventMap;
const h: Handler = {};
h["click"];
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 through alias, got: {result:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Broad string index — no restriction
// ──────────────────────────────────────────────────────────────────────────────

/// A type with a broad `string` index should NOT emit TS7053 for any string key.
#[test]
fn broad_string_index_no_ts7053() {
    let result = codes(
        r#"
type AnyMap = { [key: string]: number };
const m: AnyMap = {};
m["whatever"];
m["foo"];
m["bar"];
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for broad string index, got: {result:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Non-optional mapped type
// ──────────────────────────────────────────────────────────────────────────────

/// Non-optional mapped type with template literal key — non-matching access
/// should emit TS7053.
#[test]
fn non_optional_mapped_non_matching_emits_ts7053() {
    let result = codes(
        r#"
type Handler = { [K in `on${string}`]: () => void };
declare const h: Handler;
h["click"];
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for non-optional mapped type, got: {result:?}"
    );
}
