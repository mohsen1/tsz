//! Issue #3956: a broad inline JSDoc `@type` cast should NOT suppress
//! TS7006 ("Parameter X implicitly has an 'any' type") for nested
//! closure parameters when the cast type does not provide a contextual
//! parameter type.
//!
//! tsc treats `*` (synonym for `any`), `any`, `Object`, and `Function`
//! as casts that contribute no contextual parameter type to inner
//! closures, so an unannotated parameter inside such a cast still
//! triggers TS7006 under `--noImplicitAny`.

use crate::context::CheckerOptions;

fn diagnostics_for_js(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(
        source,
        "repro.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// `/** @type {*} */(...)` — JSDoc-specific "any" alias. The cast
/// provides no contextual parameter type for nested closures, so the
/// inner `q` parameter must still report TS7006.
#[test]
fn jsdoc_type_cast_star_does_not_suppress_ts7006_for_nested_closure() {
    let codes = diagnostics_for_js("const x = /** @type {*} */({ a: (q) => q });\n");
    assert!(
        codes.contains(&7006),
        "expected TS7006 for `q` inside `@type {{*}}` cast, got {codes:?}"
    );
}

/// `/** @type {any} */(...)` — same rule.
#[test]
fn jsdoc_type_cast_any_does_not_suppress_ts7006_for_nested_closure() {
    let codes = diagnostics_for_js("const x = /** @type {any} */({ a: (q) => q });\n");
    assert!(
        codes.contains(&7006),
        "expected TS7006 for `q` inside `@type {{any}}` cast, got {codes:?}"
    );
}

/// `/** @type {Object} */(...)` — `Object` is too broad to contextualize
/// closure parameters; tsc still emits TS7006.
#[test]
fn jsdoc_type_cast_capital_object_does_not_suppress_ts7006_for_nested_closure() {
    let codes = diagnostics_for_js("const x = /** @type {Object} */({ a: (q) => q });\n");
    assert!(
        codes.contains(&7006),
        "expected TS7006 for `q` inside `@type {{Object}}` cast, got {codes:?}"
    );
}

/// `/** @type {Function} */(...)` — same rule. Note that tsc additionally
/// reports TS2352 for the cast mismatch; we only assert that the TS7006
/// suppression is removed.
#[test]
fn jsdoc_type_cast_function_does_not_suppress_ts7006_for_nested_closure() {
    let codes = diagnostics_for_js("const x = /** @type {Function} */({ a: (q) => q });\n");
    assert!(
        codes.contains(&7006),
        "expected TS7006 for `q` inside `@type {{Function}}` cast, got {codes:?}"
    );
}

/// Anchor: a *specific* JSDoc cast type that DOES contextualize `q`
/// must continue to suppress TS7006. Guards against the broad-cast
/// fix over-triggering.
#[test]
fn jsdoc_type_cast_specific_signature_still_suppresses_ts7006() {
    let codes = diagnostics_for_js(
        "const y = /** @type {{a: (q: string) => string}} */({ a: (q) => q });\n",
    );
    assert!(
        !codes.contains(&7006),
        "did not expect TS7006 when cast supplies a contextual signature, got {codes:?}"
    );
}
