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

// =========================================================================
// First-grammar-error-wins per type parameter.
//
// `tsc`'s `checkGrammarModifiers` walks a node's modifiers in source order and
// returns after the FIRST grammar error, so each type parameter yields at most
// one modifier diagnostic — never one-per-token and never one-per-error-kind.
// These lock that in across owners (type alias, interface, function, class,
// call signature) and across the TS1273/TS1274/TS1277 mix, varying the bound
// name so the rule stays structural rather than name-keyed.
// =========================================================================

fn count(source: &str, code: u32) -> usize {
    diags(source).iter().filter(|&&c| c == code).count()
}

#[test]
fn duplicate_const_on_alias_param_reports_ts1277_once() {
    // `const` is invalid on a type-alias type parameter (TS1277). Two or three
    // `const`s on the same parameter must still report exactly once, at the
    // first `const`, matching tsc.
    for name in ["T", "K", "Elem"] {
        for source in [
            format!("type Dup<const const {name}> = {name};"),
            format!("type Dup<const const const {name}> = {name};"),
        ] {
            assert_eq!(
                count(&source, 1277),
                1,
                "expected a single TS1277 for `{source}`, got: {:?}",
                diags(&source)
            );
        }
    }
}

#[test]
fn duplicate_never_valid_modifier_reports_ts1273_once() {
    for name in ["T", "Key", "Value"] {
        let source = format!("type Dup<public public {name}> = {name};");
        assert_eq!(
            count(&source, 1273),
            1,
            "expected a single TS1273 for `{source}`, got: {:?}",
            diags(&source)
        );
    }
}

#[test]
fn first_modifier_wins_when_kinds_differ() {
    // Two different invalid modifiers on one parameter: tsc reports only the
    // FIRST in source order and stops.
    for name in ["T", "N", "Acc"] {
        // `public` (TS1273) precedes `const` (TS1277): only TS1273.
        let public_first = format!("type Mix<public const {name}> = {name};");
        let codes = diags(&public_first);
        assert!(
            codes.contains(&1273) && !codes.contains(&1277),
            "expected only TS1273 for `{public_first}`, got: {codes:?}"
        );

        // `const` (TS1277) precedes `public` (TS1273): only TS1277.
        let const_first = format!("type Mix<const public {name}> = {name};");
        let codes = diags(&const_first);
        assert!(
            codes.contains(&1277) && !codes.contains(&1273),
            "expected only TS1277 for `{const_first}`, got: {codes:?}"
        );
    }
}

#[test]
fn function_type_param_reports_first_modifier_not_variance() {
    // Regression: a function type parameter with `public` before `in` used to
    // miss `public` (TS1273) and report the trailing `in` (TS1274). tsc reports
    // only the first offending modifier — `public`.
    for name in ["T", "R", "Item"] {
        let source = format!("function f<public in {name}>(x: {name}) {{ return x; }}");
        let codes = diags(&source);
        assert!(
            codes.contains(&1273) && !codes.contains(&1274),
            "expected only TS1273 for `{source}`, got: {codes:?}"
        );
    }
}

#[test]
fn each_type_parameter_reports_independently() {
    // First-error-wins is per parameter, not per declaration: a valid-then-
    // invalid neighbour list still reports the second parameter's error.
    for (a, b) in [("T", "U"), ("K", "V")] {
        let source = format!("type Two<const {a}, const const {b}> = [{a}, {b}];");
        // One TS1277 for the lone `const` on `{a}` plus one for `{b}`'s pair.
        assert_eq!(
            count(&source, 1277),
            2,
            "expected two TS1277 (one per parameter) for `{source}`, got: {:?}",
            diags(&source)
        );
    }
}

#[test]
fn class_type_parameter_never_valid_modifier_emits_ts1273() {
    // New coverage: class type parameters were previously unchecked. `public`
    // on a class type parameter is TS1273 in tsc, reported once even when
    // repeated.
    for name in ["T", "Model", "S"] {
        let single = format!("class C<public {name}> {{}}");
        assert_eq!(
            count(&single, 1273),
            1,
            "expected TS1273 for `{single}`, got: {:?}",
            diags(&single)
        );
        let doubled = format!("class C<public public {name}> {{}}");
        assert_eq!(
            count(&doubled, 1273),
            1,
            "expected a single TS1273 for `{doubled}`, got: {:?}",
            diags(&doubled)
        );
    }
}

#[test]
fn class_type_parameter_valid_variance_and_const_is_clean() {
    // `const` and `in`/`out` are both valid on class type parameters; no
    // modifier grammar diagnostic should fire.
    for name in ["T", "U"] {
        let source = format!("class C<in out {name}, const M> {{}}");
        let codes = diags(&source);
        assert!(
            !codes.contains(&1273) && !codes.contains(&1274) && !codes.contains(&1277),
            "expected no modifier diagnostics for `{source}`, got: {codes:?}"
        );
    }
}

#[test]
fn call_signature_type_parameter_never_valid_modifier_emits_ts1273() {
    // New coverage: a method/call signature is function-like — `public` on its
    // type parameter is TS1273.
    for name in ["T", "Out"] {
        let source = format!("interface I {{ m<public {name}>(): void; }}");
        assert_eq!(
            count(&source, 1273),
            1,
            "expected TS1273 for `{source}`, got: {:?}",
            diags(&source)
        );
    }
}

#[test]
fn export_and_default_on_type_param_are_not_ts1273() {
    // `tsc` rejects `export`/`default` in a type-parameter position earlier, in
    // the parser (TS1139 "Type parameter declaration expected"), and never
    // reports TS1273 for them. The checker must not claim them as a never-valid
    // modifier — doing so emits a code `tsc` does not.
    for owner in [
        "type Bad<{kw} T> = T;",
        "class C<{kw} T> {{}}",
        "interface I {{ m<{kw} T>(): void; }}",
    ] {
        for kw in ["export", "default"] {
            let source = owner.replace("{kw}", kw);
            assert!(
                !diags(&source).contains(&1273),
                "`{kw}` in a type-parameter position must not emit TS1273: `{source}`, got {:?}",
                diags(&source)
            );
        }
    }
}
