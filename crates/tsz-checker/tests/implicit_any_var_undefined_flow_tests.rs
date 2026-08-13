//! An unannotated, uninitialized `var`/`let` is a *plain implicit `any`* when
//! `noImplicitAny` is off — `tsc`'s `getTypeOfVariableOrParameterOrProperty`
//! returns `anyType` and `convertAutoToAny` resolves every control-flow read to
//! `any`. Such a read therefore never surfaces `undefined` (no TS18048, no
//! TS2678 "not comparable to `undefined`") even under `strictNullChecks`, and
//! never narrows to a concrete assigned type for member access.
//!
//! tsz previously ran its auto/evolving-any treatment — which surfaces
//! `undefined` for a not-yet-assigned read under `strictNullChecks` — regardless
//! of `noImplicitAny`, producing false positives on the `@strict: false`
//! `@strictNullChecks: true` corpus fixture `controlFlowCaching.ts`
//! (`labelOffset`/`titlePos`/`labelAlign` are declared with no type and no
//! initializer, `tsc` reports zero errors, tsz reported TS18048 ×8 and TS2678
//! ×3). The evolving treatment is `noImplicitAny`'s behavior; with it off the
//! declaration is a plain `any`.
//!
//! The parity direction matters both ways: under full `--strict`
//! (`noImplicitAny` on) the very same shapes must still report, so the tests
//! below pin both the off-mode silence and the on-mode diagnostics, and vary the
//! binder names so nothing keys on a specific identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

const TS2678: u32 = 2678; // Type '<x>' is not comparable to type '<y>'.
const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.

/// `@strict: false` with `@strictNullChecks: true` — the combination that
/// leaves `noImplicitAny` off while `undefined` is in the domain.
fn snc_no_implicit_any_off() -> CheckerOptions {
    CheckerOptions {
        strict_null_checks: true,
        no_implicit_any: false,
        ..CheckerOptions::default()
    }
}

/// Full `--strict`: `noImplicitAny` on, where the auto/evolving-any treatment is
/// correct and the same shapes must still report.
fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str, options: CheckerOptions) -> Vec<u32> {
    check_multi_file(&[("test.ts", source)], "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn unassigned_property_access_is_not_possibly_undefined_when_no_implicit_any_off() {
    // `var <name>;` then a property write/read: the receiver is plain `any`.
    for binder in ["v", "receiver", "acc"] {
        let source =
            format!("function f() {{ var {binder}; {binder}.y = 1; return {binder}.y; }}\n");
        let diags = codes(&source, snc_no_implicit_any_off());
        assert!(
            !diags.contains(&TS18048),
            "unannotated `var {binder}` under noImplicitAny-off must not be possibly-undefined; got {diags:?}"
        );
    }
}

#[test]
fn unassigned_switch_operand_is_not_compared_against_undefined_when_no_implicit_any_off() {
    for binder in ["s", "labelAlign", "mode"] {
        let source = format!(
            "function f() {{ var {binder}; switch ({binder}) {{ case \"a\": break; }} }}\n"
        );
        let diags = codes(&source, snc_no_implicit_any_off());
        assert!(
            !diags.contains(&TS2678),
            "switch on unannotated `var {binder}` under noImplicitAny-off must not compare against `undefined`; got {diags:?}"
        );
    }
}

#[test]
fn partially_assigned_arithmetic_is_clean_when_no_implicit_any_off() {
    // `let <name>;` assigned on only one branch, then used in `+`.
    for binder in ["x", "acc", "total"] {
        let source = format!(
            "function f(c: boolean) {{ let {binder}; if (c) {binder} = 1; return {binder} + 1; }}\n"
        );
        let diags = codes(&source, snc_no_implicit_any_off());
        assert!(
            !diags.contains(&TS18048),
            "partially-assigned unannotated `let {binder}` under noImplicitAny-off must not be possibly-undefined; got {diags:?}"
        );
    }
}

#[test]
fn multi_declarator_uninitialized_vars_are_clean_when_no_implicit_any_off() {
    // Mirrors the `controlFlowCaching.ts` shape: several no-type no-initializer
    // declarators in one `var` statement, used through member access and a
    // `switch`, all reachable, `tsc` reports nothing.
    let source = "\
function f(offsets: any) {
    var start, stop, titlePos, labelOffset, labelAlign;
    labelOffset.y = 1;
    titlePos.y = offsets.t;
    switch (labelAlign) {
        case \"start\":
            labelAlign = \"end\";
            break;
        case \"middle\":
            labelOffset.y -= 1;
            break;
    }
    return start || stop;
}
";
    let diags = codes(source, snc_no_implicit_any_off());
    assert!(
        !diags.contains(&TS18048) && !diags.contains(&TS2678),
        "multi-declarator uninitialized vars under noImplicitAny-off must be clean; got {diags:?}"
    );
}

#[test]
fn strict_mode_still_reports_possibly_undefined_and_unmatched_switch() {
    // Parity guard: with `noImplicitAny` on, the auto/evolving-any treatment is
    // correct and the same shapes must still surface `undefined`.
    let prop = codes("function f() { var v; v.y = 1; return v.y; }\n", strict());
    assert!(
        prop.contains(&TS18048),
        "under --strict the unassigned `var v` read must still be possibly-undefined; got {prop:?}"
    );

    let switch_diags = codes(
        "function f() { var s; switch (s) { case \"a\": break; } }\n",
        strict(),
    );
    assert!(
        switch_diags.contains(&TS2678),
        "under --strict the switch on unassigned `var s` must still be unmatched vs `undefined`; got {switch_diags:?}"
    );
}
