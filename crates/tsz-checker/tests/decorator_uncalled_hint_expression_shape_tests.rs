//! The TS1329 "did you mean to call it first" decorator hint is gated on the
//! decorator *expression* shape, not only the decorator's signature arity.
//!
//! Structural rule (oracle-verified against pinned `typescript@7.0.2`, see
//! issues #17121 / #17123): when every call signature of a decorator takes
//! zero required parameters, tsc offers the TS1329 factory hint only when the
//! decorator expression is a *reference* the user could plausibly have
//! forgotten to invoke — a bare identifier (`@d`), a property-access chain
//! (`@ns.d`), or a call producing one (`@factory()`). It does **not** offer it
//! for an inline function/arrow literal written directly at the decorator site
//! (`@(() => {})`); grammatically a decorator accepts only a
//! `LeftHandSideExpression`, so an inline function/arrow literal can appear
//! there only when parenthesized, and tsc keeps the generic
//! TS1238/1239/1240/1241 arity elaboration for it instead.
//!
//! The gate lives in the shared `decorator_has_zero_arg_factory_shape` helper
//! (`decorator_signature_checks.rs`), so it applies uniformly to class,
//! method, accessor, and property decorators. Binder names are varied across
//! cases so the rule cannot be satisfied by any name-specific fast path.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn es_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn legacy_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            experimental_decorators: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// Any of the generic "unable to resolve signature of ... decorator" codes —
/// the family tsc keeps when the factory hint does not apply.
fn has_generic_decorator_signature_error(codes: &[u32]) -> bool {
    codes
        .iter()
        .any(|&c| c == 1238 || c == 1239 || c == 1240 || c == 1241)
}

// ─────────────────────── class decorator, reference shapes ───────────────────

#[test]
fn class_bare_identifier_zero_arg_keeps_ts1329() {
    for codes in [
        es_codes("function deco() {}\n@deco\nclass C {}\n"),
        legacy_codes("function deco() {}\n@deco\nclass C {}\n"),
    ] {
        assert!(
            codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
            "bare identifier zero-arg class decorator must keep TS1329; got: {codes:?}"
        );
    }
}

#[test]
fn class_property_access_zero_arg_keeps_ts1329() {
    // A property-access reference (`@registry.build`) is still something the
    // user could have forgotten to call, so the hint applies.
    let source = "const registry = { build: () => {} };\n@registry.build\nclass C {}\n";
    for codes in [es_codes(source), legacy_codes(source)] {
        assert!(
            codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
            "property-access zero-arg class decorator must keep TS1329; got: {codes:?}"
        );
    }
}

#[test]
fn class_factory_call_zero_arg_keeps_ts1329() {
    // A top-level call whose result is itself zero-arg — "forgot to call it
    // again" — still qualifies for the hint (a call expression, not a
    // parenthesized inline literal).
    let source = "function factory() { return () => {}; }\n@factory()\nclass C {}\n";
    for codes in [es_codes(source), legacy_codes(source)] {
        assert!(
            codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
            "factory-call zero-arg class decorator must keep TS1329; got: {codes:?}"
        );
    }
}

// ─────────────────── class decorator, parenthesized inline literal ───────────

#[test]
fn class_parenthesized_zero_arg_drops_ts1329_for_generic_ts1238() {
    // `@(() => {})` is an inline arrow literal — the hint is meaningless, so
    // the generic TS1238 arity failure stands instead.
    for codes in [
        es_codes("@(() => {})\nclass C {}\n"),
        legacy_codes("@(() => {})\nclass C {}\n"),
    ] {
        assert!(
            codes.contains(&1238) && !codes.contains(&1329),
            "parenthesized zero-arg class decorator must be TS1238, not TS1329; got: {codes:?}"
        );
    }
}

#[test]
fn class_parenthesized_reference_keeps_generic_not_ts1329() {
    // The discriminator is the parenthesis *wrapper*, not what it wraps:
    // `@(d)` wraps a bare zero-arg reference, but tsc still keeps the generic
    // TS1238 family (the `@d()` rewrite the hint suggests is not a valid
    // decorator when written as `@(d)()`). Guards against "unwrap the parens
    // and inspect the inner kind", which would wrongly re-offer TS1329 here.
    let source = "function d() {}\n@(d)\nclass C {}\n";
    for codes in [es_codes(source), legacy_codes(source)] {
        assert!(
            codes.contains(&1238) && !codes.contains(&1329),
            "parenthesized reference zero-arg class decorator must keep generic TS1238, \
             not TS1329; got: {codes:?}"
        );
    }
}

#[test]
fn class_parenthesized_one_or_two_params_is_clean() {
    // A parenthesized inline decorator that DOES match the `(value, context)`
    // runtime shape resolves cleanly — neither the hint nor the generic error.
    for source in [
        "@((value: any) => {})\nclass C {}\n",
        "@((value: any, context: any) => {})\nclass C {}\n",
    ] {
        let codes = es_codes(source);
        assert!(
            !codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
            "compatible parenthesized class decorator must be clean; got: {codes:?} for {source}"
        );
    }
}

#[test]
fn class_parenthesized_too_many_params_keeps_generic_ts1238() {
    let codes = es_codes("@((a: any, b: any, c: any) => {})\nclass C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "parenthesized >2-param class decorator must be TS1238, not TS1329; got: {codes:?}"
    );
}

// ─────────────────────── method decorator (ES + legacy) ──────────────────────

#[test]
fn method_bare_identifier_zero_arg_keeps_ts1329() {
    for codes in [
        es_codes("function log() {}\nclass C { @log method() {} }\n"),
        legacy_codes("function log() {}\nclass C { @log method() {} }\n"),
    ] {
        assert!(
            codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
            "bare zero-arg method decorator must keep TS1329; got: {codes:?}"
        );
    }
}

#[test]
fn method_parenthesized_zero_arg_drops_ts1329_for_generic() {
    for codes in [
        es_codes("class C { @(() => {}) method() {} }\n"),
        legacy_codes("class C { @(() => {}) method() {} }\n"),
    ] {
        assert!(
            !codes.contains(&1329) && has_generic_decorator_signature_error(&codes),
            "parenthesized zero-arg method decorator must drop TS1329 for a generic \
             signature error; got: {codes:?}"
        );
    }
}

// ────────────────────── accessor decorator (ES + legacy) ─────────────────────

#[test]
fn accessor_bare_identifier_zero_arg_keeps_ts1329() {
    for codes in [
        es_codes("function seal() {}\nclass C { @seal get value() { return 1; } }\n"),
        legacy_codes("function seal() {}\nclass C { @seal get value() { return 1; } }\n"),
    ] {
        assert!(
            codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
            "bare zero-arg accessor decorator must keep TS1329; got: {codes:?}"
        );
    }
}

#[test]
fn accessor_parenthesized_zero_arg_drops_ts1329_for_generic() {
    for codes in [
        es_codes("class C { @(() => {}) get value() { return 1; } }\n"),
        legacy_codes("class C { @(() => {}) get value() { return 1; } }\n"),
    ] {
        assert!(
            !codes.contains(&1329) && has_generic_decorator_signature_error(&codes),
            "parenthesized zero-arg accessor decorator must drop TS1329 for a generic \
             signature error; got: {codes:?}"
        );
    }
}

// ─────────────────── property/field decorator (legacy path) ──────────────────

#[test]
fn property_bare_identifier_zero_arg_keeps_ts1329() {
    let codes = legacy_codes("function readonly() {}\nclass C { @readonly field = 1; }\n");
    assert!(
        codes.contains(&1329) && !has_generic_decorator_signature_error(&codes),
        "bare zero-arg property decorator must keep TS1329; got: {codes:?}"
    );
}

#[test]
fn property_parenthesized_zero_arg_drops_ts1329_for_generic() {
    let codes = legacy_codes("class C { @(() => {}) field = 1; }\n");
    assert!(
        !codes.contains(&1329) && has_generic_decorator_signature_error(&codes),
        "parenthesized zero-arg property decorator must drop TS1329 for a generic \
         signature error; got: {codes:?}"
    );
}
