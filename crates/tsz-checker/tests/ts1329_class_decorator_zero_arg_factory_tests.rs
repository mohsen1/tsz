//! TS1329 for class decorators (`isPotentiallyUncalledDecorator`).
//!
//! When every call signature of a class-decorator expression takes zero
//! parameters, tsc reports only the "accepts too few arguments... did you
//! mean to call it first" hint (TS1329), anchored at the whole decorator
//! (`@`) — not the generic TS1238 signature-resolution failure. This holds
//! uniformly for a bare zero-arg decorator (`@d`) and a zero-arg factory
//! call (`@d()`) whose result also takes zero parameters, and in both
//! `experimentalDecorators` (legacy) and ES (TC39 stage-3) decorator mode.
//!
//! tsz previously had this check wired for method/accessor/field/parameter
//! decorators (`decorator_has_zero_arg_factory_shape` in
//! `decorator_signature_checks.rs`) but not for class decorators, which fell
//! through to the generic TS1238 arity-mismatch path in both modes instead.
//! Oracle-verified against pinned `typescript@7.0.2`.

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

// =========================================================================
// Legacy (`experimentalDecorators`) class decorators
// =========================================================================

#[test]
fn legacy_bare_zero_arg_class_decorator_emits_ts1329_not_ts1238() {
    let codes = legacy_codes("function d() {}\n@d class C {}\n");
    assert!(
        codes.contains(&1329) && !codes.contains(&1238),
        "expected TS1329 only for a bare zero-arg class decorator; got: {codes:?}"
    );
}

#[test]
fn legacy_zero_arg_factory_call_class_decorator_emits_ts1329_not_ts1238() {
    // `d()` is itself called, returning a further zero-arg function — still
    // the "forgot to call it" shape, just one level deeper.
    let codes = legacy_codes("function d() { return () => {}; }\n@d() class C {}\n");
    assert!(
        codes.contains(&1329) && !codes.contains(&1238),
        "expected TS1329 only for a zero-arg factory-call class decorator; got: {codes:?}"
    );
}

#[test]
fn legacy_too_few_args_class_decorator_still_emits_ts1238() {
    // Adjacent case / regression guard for #17109: a decorator declaring
    // MORE than zero (but still too many) required params is a genuine
    // TS1238 arity mismatch, not the TS1329 factory hint.
    let codes = legacy_codes("function d(a: string, b: string, c: string) {}\n@d class C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "expected TS1238 (not TS1329) when the decorator declares required params; got: {codes:?}"
    );
}

#[test]
fn legacy_not_callable_class_decorator_still_emits_ts1238() {
    let codes = legacy_codes("const d = 1;\n@d class C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "expected TS1238 (not TS1329) for a non-callable decorator; got: {codes:?}"
    );
}

// =========================================================================
// ES (TC39 stage-3) class decorators
// =========================================================================

#[test]
fn es_bare_zero_arg_class_decorator_emits_ts1329_not_ts1238() {
    let codes = es_codes("function d() {}\n@d class C {}\n");
    assert!(
        codes.contains(&1329) && !codes.contains(&1238),
        "expected TS1329 only for a bare zero-arg ES class decorator; got: {codes:?}"
    );
}

#[test]
fn es_zero_arg_factory_call_class_decorator_emits_ts1329_not_ts1238() {
    let codes = es_codes("function d() { return () => {}; }\n@d() class C {}\n");
    assert!(
        codes.contains(&1329) && !codes.contains(&1238),
        "expected TS1329 only for a zero-arg factory-call ES class decorator; got: {codes:?}"
    );
}

#[test]
fn es_too_many_required_params_class_decorator_still_emits_ts1238() {
    // Adjacent case: a decorator requiring more than the 2 args ES
    // decorators supply (`value, context`) is a genuine TS1238 arity
    // mismatch, not the TS1329 factory hint.
    let codes = es_codes("function d(a: unknown, b: unknown, c: unknown) {}\n@d class C {}\n");
    assert!(
        codes.contains(&1238) && !codes.contains(&1329),
        "expected TS1238 (not TS1329) when the ES decorator requires more than 2 params; got: {codes:?}"
    );
}

#[test]
fn es_compatible_class_decorator_no_diagnostic() {
    // Negative control: a decorator matching the `(value, context)` runtime
    // shape must be clean under both codes.
    let codes = es_codes("function d(value: unknown, context: unknown) {}\n@d class C {}\n");
    assert!(
        !codes.contains(&1238) && !codes.contains(&1329),
        "compatible ES class decorator must be clean; got: {codes:?}"
    );
}
