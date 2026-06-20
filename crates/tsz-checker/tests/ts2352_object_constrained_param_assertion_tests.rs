//! Regression coverage for #14152: assertion comparability from an object-like
//! value to a type parameter whose base constraint is the `object` primitive.
//!
//! Structural rule: when the source of an `as` / comparability check is an
//! object-like value (e.g. `Record<PropertyKey, unknown>`, `{ [k: string]:
//! unknown }`, a concrete object literal type) and the target is a type
//! parameter whose base constraint resolves to the `object` primitive (`T
//! extends object`, transitively through `V extends U extends object`), `tsc`'s
//! `isTypeComparableTo` resolves the parameter to its `object` constraint. Since
//! `object` is the structureless non-primitive supertype, every object-like
//! source overlaps it, so the assertion is allowed without an intermediate
//! `as unknown`. The check is symmetric (source/target may be either side).
//!
//! Before the fix, comparability against a bare `object`-constrained parameter
//! fell through to the property-overlap check (both sides expose no extractable
//! properties against the parameter), so `copiedValue as T` in remeda's
//! `clone.ts` emitted a false `TS2352`.
//!
//! Only the bare `object` primitive triggers the rule; primitive sources keep
//! firing `TS2352` (a number/string truly does not overlap `object`).
//!
//! Verified against `tsc` 5.8.3 (all positive cases exit 0; the primitive
//! negative controls still report `TS2352`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_codes};

fn codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    diagnostic_codes(&check_source(source, "test.ts", options))
}

fn count(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|&c| c == code).count()
}

// ── Positive cases: object-like source asserted to an object-constrained param ──

#[test]
fn record_propertykey_unknown_as_object_param_is_valid() {
    // remeda clone.ts witness: the universal object record asserted to `T`.
    let src = "export const f = <T extends object,>(v: Record<PropertyKey, unknown>) => v as T;";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

#[test]
fn record_string_unknown_as_object_param_is_valid() {
    let src = "export const f = <T extends object,>(v: Record<string, unknown>) => v as T;";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

#[test]
fn string_index_signature_as_object_param_is_valid() {
    let src = "export const f = <T extends object,>(v: { [k: string]: unknown }) => v as T;";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

#[test]
fn concrete_object_literal_as_object_param_is_valid() {
    let src = "export const f = <T extends object,>(v: { a: number }) => v as T;";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

#[test]
fn renamed_binder_object_param_is_valid() {
    // Binder name varies; the structural rule must not depend on identifiers.
    let src =
        "export const f = <Shape extends object,>(v: Record<PropertyKey, unknown>) => v as Shape;";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

#[test]
fn transitive_object_constraint_chain_is_valid() {
    // `V extends U extends object`: the base constraint resolves to `object`
    // through a parameter chain.
    let src = "export const f = <U extends object, V extends U,>(v: Record<PropertyKey, unknown>) => v as V;";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

// ── Negative controls: primitive sources still report TS2352 ──

#[test]
fn number_as_object_param_still_reports_ts2352() {
    let src = "export const f = <T extends object,>(v: number) => v as T;";
    assert_eq!(count(src, 2352), 1, "TS2352 expected: {:?}", codes(src));
}

#[test]
fn string_as_object_param_still_reports_ts2352() {
    let src = "export const f = <T extends object,>(v: string) => v as T;";
    assert_eq!(count(src, 2352), 1, "TS2352 expected: {:?}", codes(src));
}
