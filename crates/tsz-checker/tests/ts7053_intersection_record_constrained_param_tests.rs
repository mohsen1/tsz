//! TS7053 — indexing a type parameter whose constraint is an *intersection*
//! containing a `Record`-style application member.
//!
//! Structural rule: when the indexed object is a type parameter, indexability
//! is governed by its constraint resolved through the checker's
//! `TypeEnvironment`. Each constituent of a constraint intersection /union
//! must also be resolved before classification — otherwise an
//! `Application(Lazy(DefId), args)` member like `Record<string, V>` stays
//! opaque, the classifier returns `Other` for that member, and the
//! intersection is reported as unindexable even though tsc accepts the access.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/10726>
//!
//! The fix threads a `TypeResolver` through the shared
//! `classify_element_indexable` query so the evaluator can expand applications
//! / lazy wrappers nested inside intersection or union members.

use std::sync::OnceLock;

use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn lib_files() -> &'static [std::sync::Arc<LibFile>] {
    // `Record`, `Partial`, `Readonly`, etc. live in `lib.es5.d.ts`. Without them
    // loaded, the test source would reference unresolved identifiers (TS2304),
    // the constraint type would degrade to error/unknown, and the assertions
    // would trivially pass even if the indexability fix were absent. Lib files
    // are loaded once for the whole test run.
    static LIBS: OnceLock<Vec<std::sync::Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files).as_slice()
}

fn codes(source: &str) -> Vec<u32> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
        lib_files(),
    )
    .into_iter()
    .map(|d| d.code)
    // TS2318 (missing default lib) is unrelated lib-loading noise from the
    // stripped test bundle. The fix path under test does not touch that code,
    // and including it would mask real regressions.
    .filter(|code| *code != 2318)
    .collect()
}

// 1. Reported repro: `T extends { a: number } & Record<string, unknown>` indexed
//    by `string` is clean in real tsc 6.0.2.
#[test]
fn intersection_with_record_member_no_ts7053() {
    let result = codes(
        r#"
function i1<T extends { a: number } & Record<string, unknown>>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for x[k] on intersection-with-Record-member, got: {result:?}"
    );
}

// 2. Renamed type parameter / key — proves the rule is structural, not name-based.
#[test]
fn intersection_with_record_member_renamed_param_no_ts7053() {
    let result = codes(
        r#"
function r1<U extends { a: number } & Record<string, unknown>>(value: U, key: string) {
    return value[key];
}
function r2<Row extends { id: number } & Record<string, unknown>>(row: Row, k: string) {
    return row[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 with renamed params, got: {result:?}"
    );
}

// 3. Record value-type variant: a concrete value type instead of `unknown`.
#[test]
fn intersection_with_record_value_variant_no_ts7053() {
    let result = codes(
        r#"
function v1<T extends { a: number } & Record<string, number>>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for Record<string, number> intersection member, got: {result:?}"
    );
}

// 4. Reversed intersection order: Record member appears first.
#[test]
fn intersection_record_first_order_no_ts7053() {
    let result = codes(
        r#"
function ro<T extends Record<string, unknown> & { a: number }>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 with Record member first, got: {result:?}"
    );
}

// 5. Three-way intersection — still resolves indexability through the Record member.
#[test]
fn three_way_intersection_no_ts7053() {
    let result = codes(
        r#"
function tri<T extends { a: number } & { b: string } & Record<string, unknown>>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 with three-way intersection, got: {result:?}"
    );
}

// 6. Nested utility wrappers `Partial<Record<...>>` and `Readonly<Record<...>>`
//    inside an intersection still resolve to an indexable structural form.
#[test]
fn intersection_with_partial_record_no_ts7053() {
    let result = codes(
        r#"
function p1<T extends { a: number } & Partial<Record<string, number>>>(x: T, k: string) {
    return x[k];
}
function p2<T extends { a: number } & Readonly<Record<string, number>>>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for Partial/Readonly<Record<...>> in intersection, got: {result:?}"
    );
}

// 7. User-defined mapped alias inside intersection — `{ [P in string]: V }`.
//    The rule is structural; a user-defined alias must behave identically to
//    the built-in `Record` utility.
#[test]
fn intersection_with_user_mapped_alias_no_ts7053() {
    let result = codes(
        r#"
type MyRec<V> = { [P in string]: V };
function m1<T extends { a: number } & MyRec<unknown>>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for user mapped alias in intersection, got: {result:?}"
    );
}

// 8. Plain alias to `Record` resolved through `Lazy(DefId)` inside intersection.
#[test]
fn intersection_with_record_alias_no_ts7053() {
    let result = codes(
        r#"
type Bag = Record<string, unknown>;
function a1<T extends { a: number } & Bag>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for alias to Record in intersection, got: {result:?}"
    );
}

// 9. Number-literal key on an intersection-with-Record-of-number — still clean,
//    matches tsc.
#[test]
fn intersection_number_index_into_record_no_ts7053() {
    let result = codes(
        r#"
function n1<T extends { a: number } & Record<string, number>>(x: T) {
    return x["b"];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for string-literal key on intersection-with-Record, got: {result:?}"
    );
}

// 10. Direct (non-parameterized) intersection-with-Record receiver also
//     classifies as indexable. This guards against the same false positive
//     showing up when the value is a concrete intersection instead of a
//     generic type parameter.
#[test]
fn direct_intersection_with_record_no_ts7053() {
    let result = codes(
        r#"
declare const x: { a: number } & Record<string, unknown>;
declare const k: string;
const y = x[k];
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for direct intersection-with-Record receiver, got: {result:?}"
    );
}

// 11. Negative control: an intersection with NO indexable member must still
//     report TS7053. The fix must not silence genuine errors.
#[test]
fn intersection_without_indexable_member_still_emits_ts7053() {
    let result = codes(
        r#"
function bad<T extends { a: number } & { b: string }>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for intersection without indexable member, got: {result:?}"
    );
}

// 12. Negative control: a still-generic Record constraint (both key and value
//     parameters generic) defers — tsc accepts because it cannot prove
//     unindexability until instantiation.
#[test]
fn still_generic_record_intersection_defers_no_ts7053() {
    let result = codes(
        r#"
function defer<K extends string, V, T extends { a: number } & Record<K, V>>(x: T, k: K) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for still-generic Record intersection, got: {result:?}"
    );
}

// 13. Direct case (no intersection) — preserved.
//     `T extends Record<string, V>` indexed by string must remain clean.
#[test]
fn direct_record_constraint_no_ts7053_regression() {
    let result = codes(
        r#"
function d1<T extends Record<string, unknown>>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        !result.contains(&7053),
        "expected no TS7053 for direct Record-constrained param, got: {result:?}"
    );
}

// 14. Unconstrained parameter still reports TS7053 — the resolver-aware
//     classifier must not over-accept.
#[test]
fn unconstrained_param_still_emits_ts7053() {
    let result = codes(r#"function f<T>(x: T, k: string) { return x[k]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for unconstrained param, got: {result:?}"
    );
}

// 15. Object-only constraint that doesn't include the indexed key — TS7053 still fires.
#[test]
fn object_only_constraint_missing_key_still_emits_ts7053() {
    let result = codes(r#"function o<T extends { a: number }>(x: T, k: string) { return x[k]; }"#);
    assert!(
        result.contains(&7053),
        "expected TS7053 for object-only constraint missing key, got: {result:?}"
    );
}

// 16. Resolver-aware negative: an intersection of a plain object with a
//     **resolver-expandable** alias that does NOT carry an index signature must
//     still emit TS7053. Without this, the new resolver-aware path could
//     accidentally over-accept by expanding an alias that happens to evaluate
//     to a structurally-empty / property-only object.
#[test]
fn intersection_with_property_only_alias_emits_ts7053() {
    let result = codes(
        r#"
type PropOnly = { foo: number };
function p<T extends { a: number } & PropOnly>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for intersection of property-only alias (no index signature), got: {result:?}"
    );
}

// 17. Resolver-aware negative: alias that expands through a chain of generic
//     applications but still has no index signature. Guards against the new
//     path silently classifying a fully-resolved property-only type as
//     indexable.
#[test]
fn intersection_with_pick_only_alias_emits_ts7053() {
    let result = codes(
        r#"
interface Source { a: number; b: string; c: boolean }
type Picked = Pick<Source, "a" | "b">;
function q<T extends { z: number } & Picked>(x: T, k: string) {
    return x[k];
}
"#,
    );
    assert!(
        result.contains(&7053),
        "expected TS7053 for Pick<...> alias (no index signature) in intersection, got: {result:?}"
    );
}
