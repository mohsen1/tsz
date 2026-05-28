//! TS7053 — indexing a type parameter constrained to an intersection that
//! includes a member with an index signature.
//!
//! Structural rule: when the indexed object is a type parameter and its
//! constraint is an intersection type, tsc allows the access if any
//! intersection member—when resolved through the checker's type resolver—has
//! a string or number index signature that covers the key. tsz must evaluate
//! each intersection member through the resolver before deciding the
//! intersection is unindexable.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10726>
//!
//! Complementary to #10728 (direct `T extends Record<K,V>` constraint), which
//! fixes the same resolver gap for the non-intersection path.

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

// ── Positive cases: TS7053 must NOT fire ─────────────────────────────────────

/// 1. Reported repro: Record<string,V> as second intersection member, indexed
///    by `string`.
#[test]
fn intersection_record_second_member_no_ts7053() {
    let result = codes(
        r#"
function f<T extends { a: number } & Record<string, unknown>>(obj: T, key: string): unknown {
    return obj[key];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for T extends {{a:number}} & Record<string,unknown>, got: {result:?}"
    );
}

/// 2. Renamed type parameter — rule is structural, not name-based.
#[test]
fn intersection_record_renamed_param_no_ts7053() {
    let result = codes(
        r#"
function g<Row extends { id: number } & Record<string, unknown>>(row: Row, key: string): unknown {
    return row[key];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 with renamed param (Row), got: {result:?}"
    );
}

/// 3. Record as first intersection member (order-independence).
#[test]
fn intersection_record_first_member_no_ts7053() {
    let result = codes(
        r#"
function h<T extends Record<string, unknown> & { a: number }>(obj: T, key: string): unknown {
    return obj[key];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for T extends Record<string,unknown> & {{a:number}}, got: {result:?}"
    );
}

/// 4. Concrete value type variant: `Record<string, number>`.
#[test]
fn intersection_record_concrete_value_type_no_ts7053() {
    let result = codes(
        r#"
function i<T extends Record<string, number> & { b: boolean }>(obj: T, key: string): number {
    return obj[key];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for T extends Record<string,number> & {{b:boolean}}, got: {result:?}"
    );
}

/// 5. Inline index signature in intersection (structural, not via Application).
///    This case already worked; verify it continues to work.
#[test]
fn intersection_inline_index_sig_no_ts7053() {
    let result = codes(
        r#"
function j<T extends { readonly [key: string]: unknown } & { a: number }>(
    obj: T,
    key: string,
): unknown {
    return obj[key];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for inline string index in intersection, got: {result:?}"
    );
}

/// 6. Number index: `Record<number, string>` member in intersection indexed by
///    a `number` key.
#[test]
fn intersection_record_number_index_no_ts7053() {
    let result = codes(
        r#"
function k<T extends { length: number } & Record<number, string>>(obj: T, i: number): string {
    return obj[i];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for number index in T extends {{length:number}} & Record<number,string>, got: {result:?}"
    );
}

// ── Negative cases: TS7053 MUST still fire ───────────────────────────────────

/// 7. Intersection with no index signature in any member → TS7053 fires.
#[test]
fn intersection_without_index_sig_emits_ts7053() {
    let result = codes(
        r#"
function m<T extends { a: number } & { b: boolean }>(obj: T, key: string): unknown {
    return obj[key as any];
}
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for T extends {{a:number}} & {{b:boolean}} (no index sig), got: {result:?}"
    );
}

/// 8. Unconstrained type parameter → TS7053 fires (base constraint is `unknown`).
#[test]
fn unconstrained_param_emits_ts7053() {
    let result2 = codes(r#"function q<T>(obj: T) { return obj["x"]; }"#);
    assert!(
        result2.contains(&7053),
        "expected TS7053 for unconstrained T indexed by \"x\", got: {result2:?}"
    );
}

/// 9. Constraint with still-generic Record (`T extends Record<K, V>` where K and
///    V are type params) — `contains_type_parameters` returns true, so no
///    TS7053 is emitted (deferred to instantiation). Verify no false positive.
#[test]
fn intersection_generic_record_no_ts7053_deferred() {
    let result = codes(
        r#"
function r<T extends { a: number } & Record<K, V>, K extends string, V>(
    obj: T,
    key: K,
): V {
    return obj[key];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 when Record type args are generic, got: {result:?}"
    );
}
