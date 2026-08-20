//! Tests for issue #14831: a concise (expression-body) arrow / function
//! expression with an explicit declared return type and a fresh object/array
//! literal body must run the contextual excess-property check (TS2353), exactly
//! as the block-body return path does.
//!
//! Before the fix the direct concise-body path
//! (`(): { x: number } => ({ x: 1, y: 2 })`) routed only through structural
//! assignability, which does not emit TS2353 for fresh object literals, so
//! excess properties were silently accepted. Block-body returns and conditional
//! concise bodies were already checked.
//!
//! Structural rule: when a concise expression body is a fresh object (or array
//! of objects) literal and the function declares its return type, tsc runs EPC
//! against that declared return type; tsz now mirrors that through
//! `check_concise_body_excess_properties`.

use crate::test_utils::check_source_diagnostics;

fn ts2353(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2353)
        .count()
}

// ── Failing witnesses from the issue (must now report TS2353) ──

#[test]
fn concise_object_body_reports_excess_property() {
    assert_eq!(
        ts2353("const h = (): { x: number } => ({ x: 1, y: 2 });"),
        1,
        "concise object body must run EPC against the declared return type",
    );
}

#[test]
fn concise_array_of_objects_body_reports_excess_property() {
    assert_eq!(
        ts2353("const h = (): { x: number }[] => [{ x: 1, y: 2 }];"),
        1,
        "concise array-of-objects body must run EPC on each element",
    );
}

#[test]
fn function_expression_concise_body_reports_excess_property() {
    // A function expression cannot have a concise body, but an arrow wrapped in
    // a parenthesized object literal still must be checked.
    assert_eq!(
        ts2353("const h = (): { x: number } => (({ x: 1, y: 2 }));"),
        1,
        "parenthesized concise object body must still run EPC",
    );
}

#[test]
fn nested_object_property_literal_reports_excess_property() {
    assert_eq!(
        ts2353("const h = (): { a: { x: number } } => ({ a: { x: 1, y: 2 } });"),
        1,
        "EPC must recurse into nested object-literal property values",
    );
}

#[test]
fn nested_array_of_objects_reports_excess_property() {
    assert_eq!(
        ts2353("const h = (): { a: { x: number } }[] => [{ a: { x: 1, y: 2 } }];"),
        1,
        "EPC must recurse through array literals into nested objects",
    );
}

#[test]
fn async_concise_object_body_reports_excess_property() {
    assert_eq!(
        ts2353("const h = async (): Promise<{ x: number }> => ({ x: 1, y: 2 });"),
        1,
        "async concise body checks excess against the unwrapped return type",
    );
}

#[test]
fn jsdoc_returns_concise_object_body_reports_excess_property() {
    // An explicit JSDoc `@returns` is a declared return type and triggers EPC.
    let count = check_source_diagnostics(
        "/** @returns {{ x: number }} */\nconst h = () => ({ x: 1, y: 2 });",
    )
    .iter()
    .filter(|d| d.code == 2353)
    .count();
    assert_eq!(count, 1, "JSDoc-declared return type must run concise EPC");
}

// ── Passing controls (already correct; guard against regressions) ──

#[test]
fn block_body_return_still_reports_excess_property() {
    assert_eq!(
        ts2353("const h = (): { x: number } => { return { x: 1, y: 2 }; };"),
        1,
    );
}

#[test]
fn conditional_concise_body_still_reports_excess_property() {
    assert_eq!(
        ts2353("const h = (b: boolean): { x: number } => b ? { x: 1 } : { x: 2, y: 3 };"),
        1,
    );
}

#[test]
fn valid_concise_object_body_reports_nothing() {
    assert_eq!(ts2353("const h = (): { x: number } => ({ x: 1 });"), 0);
}

#[test]
fn valid_concise_array_body_reports_nothing() {
    assert_eq!(
        ts2353("const h = (): { x: number }[] => [{ x: 1 }, { x: 2 }];"),
        0
    );
}

#[test]
fn optional_property_concise_body_reports_nothing() {
    assert_eq!(
        ts2353("const h = (): { x: number; y?: number } => ({ x: 1, y: 2 });"),
        0,
    );
}

#[test]
fn index_signature_return_concise_body_reports_nothing() {
    // An index signature accepts arbitrary properties; no excess.
    assert_eq!(
        ts2353("const h = (): { [k: string]: number } => ({ x: 1, y: 2 });"),
        0,
    );
}

#[test]
fn as_cast_concise_body_suppresses_excess_property() {
    // tsc does not run EPC against the return type when the body is itself an
    // assertion; only parentheses are unwrapped, never `as`/`satisfies`.
    assert_eq!(
        ts2353("const h = (): { x: number } => ({ x: 1, y: 2 } as { x: number });"),
        0,
    );
}

#[test]
fn contextual_only_return_type_does_not_introduce_excess_property() {
    // Without an explicit annotation, a concise body that receives a purely
    // contextual return type from an interface method must NOT run EPC, matching
    // tsc (neither compiler reports here).
    assert_eq!(
        ts2353(
            r#"
interface I { m(): { x: number }; }
const o: I = { m: () => ({ x: 1, y: 2 }) };
"#,
        ),
        0,
    );
}

// ── Anti-hardcoding: structural, not name-driven ──

#[test]
fn concise_body_excess_property_is_name_independent() {
    // Vary the binder, property, and excess-property names: the fix keys off the
    // AST concise-body + declared-return shape, never specific identifiers.
    assert_eq!(
        ts2353("const buildThing = (): { alpha: number } => ({ alpha: 1, zzExtra: 2 });"),
        1,
    );
    assert_eq!(
        ts2353("const make = (): { keyName: string }[] => [{ keyName: \"a\", spare: 0 }];"),
        1,
    );
}
