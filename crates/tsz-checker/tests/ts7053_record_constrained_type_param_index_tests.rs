//! TS7053 — indexing a type parameter whose constraint resolves, through a
//! wrapper, to a type that has an applicable index signature.
//!
//! Structural rule: when the indexed object is a type parameter, indexability is
//! governed by its *base constraint*. That constraint may still be an
//! unevaluated wrapper — `T extends Record<string, V>` stores the constraint as
//! an `Application` of a mapped type. tsz must resolve the constraint to its
//! apparent type (`{ [k: string]: V }`) before deciding the parameter is
//! unindexable; otherwise a resolver-less evaluation leaves the wrapper intact
//! and a `string` key looks unusable, producing a false TS7053. tsc allows the
//! access because the constraint's string index signature covers a `string` key.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10674>
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

// 1. Reported repro: `Record<string, unknown>` constraint indexed by `string`.
#[test]
fn record_string_unknown_constraint_string_key_no_ts7053() {
    let result =
        codes(r#"function f<T extends Record<string, unknown>>(o: T, k: string) { return o[k]; }"#);
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for Record<string, unknown>-constrained param, got: {result:?}"
    );
}

// 2. Renamed type parameter — proves the rule is structural, not name-based.
#[test]
fn record_constraint_renamed_param_is_structural() {
    let renamed = codes(
        r#"function g<U extends Record<string, unknown>>(p: U, key: string) { return p[key]; }"#,
    );
    assert!(
        !renamed.contains(&7053),
        "expected no TS7053 with renamed param/key, got: {renamed:?}"
    );
    let multi = codes(
        r#"function h<Row extends Record<string, number>>(value: Row, field: string) { return value[field]; }"#,
    );
    assert!(
        !multi.contains(&7053),
        "expected no TS7053 with multi-char param name, got: {multi:?}"
    );
}

// 3. Value-type variant: the rule does not depend on the index value type.
#[test]
fn record_string_number_constraint_string_key_no_ts7053() {
    let result =
        codes(r#"function f<T extends Record<string, number>>(o: T, k: string) { return o[k]; }"#);
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for Record<string, number>-constrained param, got: {result:?}"
    );
}

// 4. Nested wrapper: `Partial<Record<...>>` must also resolve to an indexable shape.
#[test]
fn partial_record_constraint_string_key_no_ts7053() {
    let result = codes(
        r#"function f<T extends Partial<Record<string, number>>>(o: T, k: string) { return o[k]; }"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for Partial<Record<...>>-constrained param, got: {result:?}"
    );
}

// 4b. `Readonly<Record<...>>` wrapper constraint.
#[test]
fn readonly_record_constraint_string_key_no_ts7053() {
    let result = codes(
        r#"function f<T extends Readonly<Record<string, number>>>(o: T, k: string) { return o[k]; }"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for Readonly<Record<...>>-constrained param, got: {result:?}"
    );
}

// 5. User-defined mapped-type alias as the constraint (renamed iteration var too).
#[test]
fn user_mapped_alias_constraint_string_key_no_ts7053() {
    let result = codes(
        r#"
type MyRec<V> = { [P in string]: V };
function f<T extends MyRec<number>>(o: T, k: string) { return o[k]; }
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for user mapped-alias-constrained param, got: {result:?}"
    );
}

// 5b. Plain alias to `Record` as the constraint.
#[test]
fn alias_to_record_constraint_string_key_no_ts7053() {
    let result = codes(
        r#"
type RowLike = Record<string, unknown>;
function f<T extends RowLike>(o: T, k: string) { return o[k]; }
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for alias-to-Record-constrained param, got: {result:?}"
    );
}

// 6. Negative control: a wrapper constraint that resolves to a shape WITHOUT a
//    string index signature must still report TS7053 (the fix only resolves the
//    apparent type — it does not invent index signatures).
#[test]
fn object_constraint_missing_index_still_emits_ts7053() {
    let result =
        codes(r#"function bad<T extends { a: number }>(o: T, k: string) { return o[k]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for object constraint lacking a string index, got: {result:?}"
    );
}

// 6b. Negative control: an unconstrained parameter still reports TS7053.
#[test]
fn unconstrained_param_string_key_still_emits_ts7053() {
    let result = codes(r#"function f<T>(o: T, k: string) { return o[k]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for unconstrained param, got: {result:?}"
    );
}

// 6c. Negative control: a still-generic constraint (`Record<K, V>`) defers to
//     instantiation and stays clean (no TS7053, no premature acceptance error).
#[test]
fn generic_record_constraint_defers_no_ts7053() {
    let result = codes(
        r#"function f<K extends string, T extends Record<K, number>>(o: T, k: K) { return o[k]; }"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for generically-constrained param, got: {result:?}"
    );
}
