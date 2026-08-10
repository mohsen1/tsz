//! TS1329's "did you mean to call it first" hint only fires for a decorator
//! expression written as exactly an identifier or a property-access chain
//! ending in one — not for any other syntactic shape at the decorator site,
//! even one that resolves to the exact same zero-parameter callee.
//!
//! Oracle-verified (`typescript@7.0.2`): for the same zero-param decorator,
//! `@d` and `@obj.d` get TS1329, but `@(d)`, `@(obj.d)`, and `@(() => {})`
//! all get the ordinary TS1238/TS1241 signature-failure diagnostic instead —
//! parenthesizing (or writing a function literal inline) isn't the "forgot
//! to call it" shape the hint is for.
//!
//! `decorator_has_zero_arg_factory_shape` previously gated only on the
//! callee's parameter count, so it fired TS1329 for `@(() => {})` too — and
//! since the ES class-decorator arity check had no fallback for a
//! zero-param, non-reference decorator, that shape silently passed with no
//! diagnostic at all once TS1329 stood down.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

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

fn es_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn es_parenthesized_identifier_zero_arg_decorator_emits_ts1238_not_ts1329() {
    let codes = es_codes("function d() { return undefined as any; }\n@(d)\nclass C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "a parenthesized identifier is not a 'forgot to call it' reference; got: {codes:?}"
    );
}

#[test]
fn legacy_parenthesized_identifier_zero_arg_decorator_emits_ts1238_not_ts1329() {
    let codes = legacy_codes("function d() { return undefined as any; }\n@(d)\nclass C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "a parenthesized identifier is not a 'forgot to call it' reference; got: {codes:?}"
    );
}

#[test]
fn es_inline_arrow_zero_param_decorator_emits_ts1238_not_ts1329() {
    let codes = es_codes("@(() => {})\nclass C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "an inline zero-param arrow decorator gets the generic arity failure, not the hint; got: {codes:?}"
    );
}

#[test]
fn es_property_access_zero_arg_decorator_still_emits_ts1329() {
    // Positive control: a property-access chain IS a plausible "forgot to
    // call it" reference, same as a bare identifier.
    let codes = es_codes("const obj = { d() { return undefined as any; } };\n@obj.d\nclass C {}\n");
    assert!(
        codes.contains(&1329) && !codes.contains(&1238),
        "a property-access reference should still get the TS1329 hint; got: {codes:?}"
    );
}

#[test]
fn es_parenthesized_property_access_zero_arg_decorator_emits_ts1238_not_ts1329() {
    let codes =
        es_codes("const obj = { d() { return undefined as any; } };\n@(obj.d)\nclass C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "parenthesizing a property-access reference still isn't the hint shape; got: {codes:?}"
    );
}

#[test]
fn es_parenthesized_identifier_zero_arg_method_decorator_emits_ts1241_not_ts1329() {
    // Same shape rule applies uniformly to member decorators
    // (`decorator_has_zero_arg_factory_shape` is shared).
    let codes = es_codes("function d() {}\nclass C { @(d) method(): void {} }\n");
    assert!(
        codes.contains(&1241) && !codes.contains(&1329),
        "a parenthesized identifier method decorator gets TS1241, not the hint; got: {codes:?}"
    );
}
