//! Regression tests for the last open slice of #15632: a `yield*` **union or
//! intersection** delegate contributes nothing to an unannotated generator's
//! inferred yield type, so the aggregate collapses to `any` and a mismatched
//! `Generator` instantiation is silently accepted.
//!
//! Root cause: the `yield*` element resolution called the solver's structural
//! `get_iterator_info`, which answers its `ANY` sentinel for a union delegate
//! (it never distributes over the constituents) and for any lib iterable behind
//! a `TypeData::Lazy(DefId)` alias body, then only fell back to the
//! non-distributing `resolve_iterator_element_type`. `for..of` already handled
//! unions via `for_of_element_type_classified`'s `ForOfElementKind::Union` /
//! `Intersection` arms; the fix routes `yield*` through that same env-aware,
//! union-distributing chain (`for_of_element_type`), and teaches its async
//! entry to distribute over members using the async protocol (the sync
//! classified fallback can only resolve members through `[Symbol.iterator]`).
//!
//! tsc's iteration type over a union delegate is the **union** of the members'
//! element types, and over an intersection the **intersection**. Every negative
//! row feeds the delegate to a deliberately wrong `Generator` instantiation, so
//! a missing `TS2345` means the union contribution collapsed to `any`; each is
//! paired with a positive control that must stay clean (a narrower-but-wrong
//! inferred type would also satisfy the negative row). All rows were oracled
//! against `tsc@7.0.2` (`--noEmit --strict`).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

/// Core repro: a union of two differently-instantiated `Generator`s must
/// contribute `string | number`, not collapse to `any`.
#[test]
fn union_of_generators_contributes_the_union_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Generator<string> | Generator<number>;
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union of Generators must contribute `string | number`, not `any`: {codes:?}"
    );
}

#[test]
fn union_of_generators_correct_instantiation_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Generator<string> | Generator<number>;
declare function wants(g: Generator<string | number>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "the matching instantiation must stay clean — a narrower-but-wrong yield type would also satisfy the TS2345 row: {codes:?}"
    );
}

/// The array/tuple fast path is a different `get_iterator_info` branch, but the
/// union of two arrays is not itself an array, so it hit the same collapse.
#[test]
fn union_of_arrays_contributes_the_union_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const src: string[] | number[];
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union of arrays must contribute `string | number`: {codes:?}"
    );
}

#[test]
fn union_of_tuples_contributes_the_union_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const src: [string, string] | [number, number];
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union of tuples must contribute `string | number`: {codes:?}"
    );
}

/// `Iterable<T>` reaches its iterator member through a `Lazy(DefId)` alias body,
/// which the structural solver query cannot see — the union stacks that blind
/// spot on top of the non-distribution one.
#[test]
fn union_of_iterables_contributes_the_union_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Iterable<string> | Iterable<number>;
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union of Iterables must contribute `string | number`: {codes:?}"
    );
}

#[test]
fn union_of_sets_contributes_the_union_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Set<string> | Set<number>;
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union of Sets must contribute `string | number`: {codes:?}"
    );
}

/// Members of different iterable *kinds* (array fast path + lazy lib iterable)
/// in one union: each must resolve through its own path and still union.
#[test]
fn union_of_mixed_iterable_kinds_contributes_the_union() {
    let codes = strict_codes(
        r#"
export {};
declare const src: string[] | Generator<number>;
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union mixing an array and a Generator must contribute `string | number`: {codes:?}"
    );
}

/// A string-literal member is iterable and contributes `string`.
#[test]
fn union_with_a_string_member_contributes_string() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Generator<number> | "ab";
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union with a string member must contribute `string | number`: {codes:?}"
    );
}

/// An **intersection** delegate contributes the intersection of the members'
/// element types (`{ a } & { b }`), not their union and not `any`.
#[test]
fn intersection_delegate_contributes_the_intersection_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Iterable<{ a: number }> & Iterable<{ b: number }>;
declare function wants(g: Generator<{ c: number }>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "an intersection delegate must contribute `{{ a }} & {{ b }}`: {codes:?}"
    );
}

#[test]
fn intersection_delegate_correct_instantiation_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Iterable<{ a: number }> & Iterable<{ b: number }>;
declare function wants(g: Generator<{ a: number }>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "`{{ a }} & {{ b }}` is assignable to `{{ a }}`, so the matching instantiation must stay clean: {codes:?}"
    );
}

/// A plain `yield` and a delegated union `yield*` must **union** into the
/// aggregate, not replace one another.
#[test]
fn plain_and_delegated_union_yields_join_rather_than_replace() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Generator<string> | Generator<number>;
declare function wants(g: Generator<string | number | boolean>): void;
const d = function* () { yield true; yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "the plain and delegated union yields must join into `string | number | boolean`: {codes:?}"
    );
}

/// Anti-hardcoding control: the fix is structural, so renaming every binder and
/// the local relay must not change the result.
#[test]
fn renamed_binders_union_delegate_still_contributes() {
    let codes = strict_codes(
        r#"
export {};
declare const zqFeed: Generator<string> | Generator<number>;
declare function zzConsume(gRelay: Generator<boolean>): void;
const zRelay = function* () { yield* zqFeed; };
zzConsume(zRelay());
"#,
    );
    assert!(
        codes.contains(&2345),
        "renamed binders must not change the union contribution: {codes:?}"
    );
}

/// Baseline control: a single (non-union) delegate already worked and must keep
/// working — this fix only adds the union/intersection distribution.
#[test]
fn single_generator_delegate_stays_correct() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Generator<string>;
declare function wants(g: Generator<boolean>): void;
const d = function* () { yield* src; };
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a single Generator delegate must keep contributing its element type: {codes:?}"
    );
}
