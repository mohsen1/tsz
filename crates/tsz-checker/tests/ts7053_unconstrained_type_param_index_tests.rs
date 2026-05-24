//! TS7053 — indexing an unconstrained generic type parameter by a concrete key.
//!
//! Structural rule: when the indexed object is a type parameter, indexability is
//! governed by its *base constraint* — the declared `extends` type, or `unknown`
//! when there is none. A concrete key (literal / `string` / `number`) cannot be
//! proven to index `unknown`, so tsc reports TS7053 for the unconstrained case
//! exactly as it does for an object constraint that lacks the key. A generic key
//! (`K extends keyof T`) defers to instantiation and stays clean.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/9672>
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

// 1. Reported repro: string-literal key on an unconstrained parameter.
#[test]
fn unconstrained_param_string_literal_key_emits_ts7053() {
    let result = codes(r#"function f<T>(o: T) { return o["x"]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for o[\"x\"] on unconstrained T, got: {result:?}"
    );
}

// 2. Renamed parameter and key — proves the rule is structural, not name-based.
#[test]
fn unconstrained_param_renamed_is_structural() {
    let result = codes(r#"function g<U>(p: U) { return p["k"]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 with renamed param/key, got: {result:?}"
    );
    let multi = codes(r#"function h<Data>(value: Data) { return value["field"]; }"#);
    assert!(
        multi.contains(&7053),
        "expected TS7053 with multi-char type-param name, got: {multi:?}"
    );
}

// 5. Number-literal key variant.
#[test]
fn unconstrained_param_number_literal_key_emits_ts7053() {
    let result = codes(r#"function f<T>(o: T) { return o[0]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for o[0] on unconstrained T, got: {result:?}"
    );
}

// 5. Wide `string`-typed key variant.
#[test]
fn unconstrained_param_string_typed_key_emits_ts7053() {
    let result = codes(r#"function f<T>(o: T, k: string) { return o[k]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for o[k:string] on unconstrained T, got: {result:?}"
    );
}

// 3. Negative control: a `keyof T` key defers to instantiation and stays clean.
#[test]
fn keyof_param_key_no_ts7053() {
    let result = codes(r#"function ok<T, K extends keyof T>(o: T, k: K) { return o[k]; }"#);
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for o[k] where K extends keyof T, got: {result:?}"
    );
}

// 4. Positive control kept passing: an object constraint lacking the key.
#[test]
fn object_constraint_missing_key_still_emits_ts7053() {
    let result = codes(r#"function bad<T extends object>(o: T) { return o["b"]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for o[\"b\"] on T extends object, got: {result:?}"
    );
}

// 4. Positive control: an object constraint that *has* the key stays clean.
#[test]
fn object_constraint_with_key_no_ts7053() {
    let result = codes(r#"function ok<T extends { a: number }>(o: T) { return o["a"]; }"#);
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for o[\"a\"] on T extends {{ a: number }}, got: {result:?}"
    );
}

// A generic constraint (still mentions a type parameter) defers — no TS7053.
#[test]
fn generic_constraint_defers_no_ts7053() {
    let result = codes(
        r#"function f<K extends string, T extends Record<K, number>>(o: T, k: K) { return o[k]; }"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for indexing a generically-constrained param, got: {result:?}"
    );
}

// for-in narrowing: `typeof o === "object"` narrows `T` to `T & object`, so the
// indexed access is not a bare type parameter and must NOT report TS7053
// (matches tsc).
#[test]
fn for_in_narrowed_object_no_false_ts7053() {
    let result = codes(
        r#"
function each<T>(o: T) {
    if (typeof o === "object" && o !== null) {
        for (const k in o) {
            o[k];
        }
    }
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for for-in access on narrowed object, got: {result:?}"
    );
}
