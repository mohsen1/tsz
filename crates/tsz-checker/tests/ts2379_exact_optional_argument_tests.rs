//! Tests for TS2379 — exactOptionalPropertyTypes for call arguments.
//!
//! Structural rule:
//!
//! > When `exactOptionalPropertyTypes` is enabled and a call argument fails to
//! > assign to its parameter purely because the argument has `| undefined` on a
//! > property that the parameter declares as `?`-optional-without-undefined,
//! > the diagnostic must be TS2379 with the
//! > `with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to
//! > the types of the target's properties.` helper text, NOT plain TS2345.
//!
//! This mirrors the existing TS2375 (vs. TS2322) split on the
//! assignment-context path. The fix lives in
//! `error_argument_not_assignable_at` and
//! `error_argument_not_assignable_preserving_param_display`, which both route
//! through the new shared `argument_not_assignable_code_and_template` helper.
//!
//! The adjacent-case matrix below intentionally varies user-chosen names
//! (`a`/`x`/`p`/`inner`), property positions, nesting depth, and call shape
//! (direct call, multi-arg, generic-bound call, constructor call, method call,
//! callback call) so a hardcoded-spelling fix would not pass.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_codes, check_with_options, diagnostic_codes, has_any_diagnostic_code,
    has_diagnostic_code, strict_checker_options,
};

fn eopt_options() -> CheckerOptions {
    CheckerOptions {
        exact_optional_property_types: true,
        ..strict_checker_options()
    }
}

// ── 1) Reported bug class: TS2379 on the basic optional-vs-undefined shape ───

#[test]
fn ts2379_basic_optional_vs_explicit_undefined_call_argument() {
    let source = r#"
declare function takes(o: { x?: string }): void;
declare const objXU: { x?: string | undefined };
takes(objXU);
"#;
    let diagnostics = check_with_options(source, eopt_options());
    assert!(
        has_diagnostic_code(&diagnostics, 2379),
        "expected TS2379 for EOPT call-argument mismatch, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2345),
        "expected NO TS2345 (the EOPT-specific code should take its place), got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    // Primary-message coverage: code, EOPT helper text, and arg/param displays.
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2379)
        .expect("expected TS2379");
    for fragment in [
        "with 'exactOptionalPropertyTypes: true'",
        "Consider adding 'undefined'",
        "Argument of type '{ x?: string | undefined; }'",
        "parameter of type '{ x?: string; }'",
    ] {
        assert!(
            diag.message_text.contains(fragment),
            "expected message to contain {fragment:?}, got: {}",
            diag.message_text
        );
    }
}

// ── 2) Adjacent: rule is name-agnostic (prop, param, primitive type) ─────────

#[test]
fn ts2379_independent_of_property_name_and_primitive() {
    // Vary the property identifier AND its primitive type to prove the rule
    // is structural, not keyed on any specific spelling.
    for (prop, ty) in [("p", "number"), ("value", "boolean"), ("target", "string")] {
        let source = format!(
            "declare function takes(o: {{ {prop}?: {ty} }}): void;\n\
             declare const v: {{ {prop}?: {ty} | undefined }};\n\
             takes(v);\n"
        );
        let diagnostics = check_with_options(&source, eopt_options());
        assert!(
            has_diagnostic_code(&diagnostics, 2379),
            "expected TS2379 for property '{prop}: {ty}', got: {:?}",
            diagnostic_codes(&diagnostics)
        );
    }
}

#[test]
fn ts2379_through_named_alias_target() {
    let source = r#"
type Target = { a?: string };
type SourceShape = { a?: string | undefined };
declare function takes(o: Target): void;
declare const v: SourceShape;
takes(v);
"#;
    assert!(
        has_diagnostic_code(&check_with_options(source, eopt_options()), 2379),
        "expected TS2379 when parameter type is a named alias"
    );
}

// ── 3) Adjacent: nested/deep optional positions ─────────────────────────────

#[test]
fn ts2379_nested_optional_object_property() {
    let source = r#"
declare function takes(o: { inner?: { x?: string } }): void;
declare const v: { inner?: { x?: string | undefined } | undefined };
takes(v);
"#;
    assert!(
        has_diagnostic_code(&check_with_options(source, eopt_options()), 2379),
        "expected TS2379 for nested EOPT mismatch"
    );
}

#[test]
fn ts2379_multi_argument_one_with_eopt_mismatch() {
    let source = r#"
declare function takes(x: number, o: { p?: string }, z?: boolean): void;
declare const v: { p?: string | undefined };
takes(1, v, true);
"#;
    let diagnostics = check_with_options(source, eopt_options());
    assert!(
        has_diagnostic_code(&diagnostics, 2379),
        "expected TS2379 for the offending argument in a multi-arg call, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

// ── 4) Negative: orthogonal mismatches stay TS2345 ──────────────────────────

#[test]
fn ts2345_optional_to_required_under_eopt_is_not_ts2379() {
    // Optional → required is a different mismatch class. It should remain TS2345
    // even under EOPT, because the failure is "property is optional in source
    // but required in target", not "source has explicit undefined".
    let source = r#"
declare function strict(o: { a: string }): void;
declare const optA: { a?: string };
strict(optA);
"#;
    let diagnostics = check_with_options(source, eopt_options());
    assert!(
        has_diagnostic_code(&diagnostics, 2345),
        "expected TS2345 for optional-to-required mismatch under EOPT, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2379),
        "expected NO TS2379 for orthogonal mismatch, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn ts2345_unrelated_type_mismatch_stays_2345_in_both_modes() {
    // A primitive mismatch in a non-literal call argument is orthogonal to EOPT:
    // it must emit TS2345 (never TS2379) whether EOPT is on or off. Object
    // literal arguments elaborate at the literal site instead, so we use a
    // declared variable.
    let source = r#"
declare function takes(o: { a: string }): void;
declare const v: { a: number };
takes(v);
"#;
    for options in [eopt_options(), strict_checker_options()] {
        let diagnostics = check_with_options(source, options);
        assert!(
            has_diagnostic_code(&diagnostics, 2345),
            "expected TS2345 for unrelated type mismatch, got: {:?}",
            diagnostic_codes(&diagnostics)
        );
        assert!(
            !has_diagnostic_code(&diagnostics, 2379),
            "TS2379 must not fire on orthogonal mismatch, got: {:?}",
            diagnostic_codes(&diagnostics)
        );
    }
}

// ── 5) Negative: no-EOPT mode is structurally silent ────────────────────────

#[test]
fn no_diagnostic_without_eopt_for_optional_vs_undefined_call_arg() {
    // Without exactOptionalPropertyTypes, `{a?:T}` and `{a?:T|undefined}` are
    // structurally equivalent — no diagnostic at all.
    let source = r#"
declare function takes(o: { a?: string }): void;
declare const v: { a?: string | undefined };
takes(v);
"#;
    let diagnostics = check_source_codes(source);
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics without EOPT, got: {diagnostics:?}"
    );
}

// ── 6) Adjacent: TS2375 path on plain assignment is unchanged ──────────────

#[test]
fn ts2375_assignment_under_eopt_unchanged() {
    let source = r#"
type A = { a?: string };
type B = { a?: string | undefined };
declare const b: B;
const a: A = b;
"#;
    let diagnostics = check_with_options(source, eopt_options());
    assert!(
        has_diagnostic_code(&diagnostics, 2375),
        "expected TS2375 on plain assignment (regression guard), got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    assert!(
        !has_diagnostic_code(&diagnostics, 2379),
        "TS2379 is the call-argument variant — must not fire on plain assignment, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

// ── 7) Adjacent: method call also routes through the same emitter ──────────

#[test]
fn ts2379_method_call_argument() {
    let source = r#"
declare const obj: { m(o: { x?: string }): void };
declare const v: { x?: string | undefined };
obj.m(v);
"#;
    assert!(
        has_diagnostic_code(&check_with_options(source, eopt_options()), 2379),
        "expected TS2379 for method-call argument under EOPT"
    );
}

// ── 8) Adjacent: constructor call argument ─────────────────────────────────

#[test]
fn ts2379_constructor_call_argument() {
    let source = r#"
declare class C { constructor(o: { x?: string }); }
declare const v: { x?: string | undefined };
new C(v);
"#;
    assert!(
        has_diagnostic_code(&check_with_options(source, eopt_options()), 2379),
        "expected TS2379 for constructor-call argument under EOPT"
    );
}

// ── 9) Sanity: when EOPT detects no mismatch on the arg pair, fallback ─────

#[test]
fn ts2379_does_not_fire_when_eopt_mismatch_check_returns_false() {
    // Both source and target use the same `T | undefined` shape — no mismatch.
    let source = r#"
declare function takes(o: { x?: string | undefined }): void;
declare const v: { x?: string | undefined };
takes(v);
"#;
    let diagnostics = check_with_options(source, eopt_options());
    assert!(
        !has_any_diagnostic_code(&diagnostics, &[2379, 2345]),
        "expected no call-argument errors when there is no mismatch, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}
