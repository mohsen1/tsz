//! Diagnostics for invalid modifiers on type parameters and on class members.
//!
//! These tests lock in the rules that distinguish:
//! - TS1273 — modifier categorically invalid on a type parameter (e.g. `public T`).
//! - TS1274 — variance modifier valid on type parameters in some contexts but
//!   not the current one (e.g. `in`/`out` on a function type parameter, or as
//!   a class member modifier).
//!
//! The rules are expressed structurally (token kind / member kind), so each
//! test exercises at least two name choices for the bound variable to ensure
//! the checker is not pattern-matching on user-chosen identifier names.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn diags(source: &str) -> Vec<u32> {
    check_source(source, "a.ts", ts_options())
        .iter()
        .map(|d| d.code)
        .collect()
}

// =========================================================================
// TS1273: modifiers categorically invalid on a type parameter
// =========================================================================

#[test]
fn public_modifier_on_type_alias_param_emits_ts1273_not_ts1274() {
    // `public` is never valid on a type parameter (any context). tsc reports
    // TS1273, not TS1274 — TS1274 is reserved for `in`/`out` mis-placement.
    for name in ["T", "K", "Foo"] {
        let source = format!("type Bad<public {name}> = {name};");
        let codes = diags(&source);
        assert!(
            codes.contains(&1273),
            "expected TS1273 for `public {name}`, got: {codes:?}"
        );
        assert!(
            !codes.contains(&1274),
            "should not emit TS1274 for `public {name}`, got: {codes:?}"
        );
    }
}

#[test]
fn private_static_readonly_on_type_param_all_emit_ts1273() {
    // Same rule for every never-valid keyword. Iterate to confirm none of
    // them silently fall through to TS1274.
    for kw in ["private", "protected", "static", "readonly", "abstract"] {
        let source = format!("type Bad<{kw} K> = K;");
        let codes = diags(&source);
        assert!(
            codes.contains(&1273),
            "`{kw}` on a type parameter should emit TS1273, got: {codes:?}"
        );
    }
}

// =========================================================================
// TS1274: variance modifiers (`in`, `out`) on a class member
// =========================================================================

#[test]
fn in_modifier_on_class_field_emits_ts1274_not_ts1434() {
    // The pre-fix behaviour was a generic TS1434 ("Unexpected keyword or
    // identifier") because the parser refused to consume `in` as a
    // class-member modifier. tsc emits TS1274 at the modifier position.
    for field_name in ["a", "value", "x_y_z"] {
        let source = format!("class C {{ in {field_name} = 0; }}");
        let codes = diags(&source);
        assert!(
            codes.contains(&1274),
            "expected TS1274 for `in {field_name}`, got: {codes:?}"
        );
        assert!(
            !codes.contains(&1434),
            "should not emit TS1434 for `in {field_name}`, got: {codes:?}"
        );
    }
}

#[test]
fn out_modifier_on_class_field_emits_ts1274() {
    for field_name in ["b", "result", "_count"] {
        let source = format!("class C {{ out {field_name} = 0; }}");
        let codes = diags(&source);
        assert!(
            codes.contains(&1274),
            "expected TS1274 for `out {field_name}`, got: {codes:?}"
        );
    }
}

#[test]
fn in_used_as_class_field_name_does_not_emit_ts1274() {
    // `class C { in: number; }` uses `in` as the field name (followed by `:`),
    // not a modifier. The fix must not regress this — we should not emit TS1274.
    let source = "class C { in: number = 0; }";
    let codes = diags(source);
    assert!(
        !codes.contains(&1274),
        "`in` as a class field name should not emit TS1274, got: {codes:?}"
    );
}

#[test]
fn in_used_as_class_method_name_does_not_emit_ts1274() {
    // Methods named `in` / `out` are valid; the parser should treat the
    // keyword as a property name when followed by `(`.
    for source in [
        "class C { in() { return 1; } }",
        "class C { out() { return 1; } }",
    ] {
        let codes = diags(source);
        assert!(
            !codes.contains(&1274),
            "method named `in`/`out` should not emit TS1274, got: {codes:?} for {source}"
        );
    }
}
