//! TS7005 for a non-empty array literal whose widened element type is `any`
//! due to non-strict null/undefined widening — e.g. `var b = [undefined,
//! null]` widens to `any[]`, and tsc reports the implicit-any at the
//! declaration site the same way it does for a bare scalar `any`.
//!
//! Structural rule: `widen_initializer_type_for_mutable_binding_gated`
//! already widens a fresh array literal's `null`/`undefined` leaves to `any`
//! when `strictNullChecks` is off and every nullish leaf found is a genuine
//! widening source (`initializer_nullish_leaves_are_widening`,
//! `types/utilities/mutable_binding_nullish.rs`) — but nothing downstream
//! ever reported the resulting implicit-any. The existing TS7005 checks in
//! `state/variable_checking/core.rs` only fire for a *bare* `final_type ==
//! TypeId::ANY` (no initializer) or a direct *empty* array literal (deferred
//! "evolving any" tracking); a non-empty literal whose widened element type
//! is `any` fell through both and was silently accepted.
//!
//! Owner: `state/variable_checking/core.rs`'s
//! `compound_nullish_widening_implicit_any`, which checks the *resulting*
//! element type (`query::array_element_type(..) == Some(TypeId::ANY)`)
//! rather than walking the initializer's syntax for a nullish leaf — a mixed
//! literal like `[1, undefined]` reduces its best-common-type to `number`
//! (verified against the real `tsc` 6.0.2 oracle: no diagnostic at all), so
//! an AST-only "contains a nullish leaf" gate would have been wrong. Unlike
//! the bare-scalar case, this fires unconditionally at the declaration site
//! regardless of `var`/`let`/`const`: a non-empty literal's type is never
//! "evolving," so there is no deferred-tracking mechanism for it to go
//! through. Object literals are deliberately excluded — an implicit-any
//! property inside a fresh object literal already gets its own per-property
//! TS7018, and tsc does not additionally report TS7005 for the variable.
//!
//! Every row below is pinned against a real `tsc` 6.0.2 oracle
//! (`/opt/node22/bin/tsc`), `--target es2015 --strict false --noImplicitAny true`.

use crate::context::CheckerOptions;
use crate::test_utils::check_with_options_code_messages;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: false,
        strict_null_checks: false,
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

fn messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, opts())
}

/// The reported repro (`wideningTuples3.ts`'s theme): a tuple-shaped literal
/// made entirely of `null`/`undefined` keywords widens to `any[]`. Oracle:
/// `TS7005: Variable 'b' implicitly has an 'any[]' type.`
#[test]
fn array_of_nullish_keywords_reports_ts7005() {
    let source = "var b = [undefined, null];";
    assert_eq!(
        messages(source),
        vec![(
            7005,
            "Variable 'b' implicitly has an 'any[]' type.".to_string()
        )],
    );
}

/// Renamed binder: the diagnostic text must track the actual identifier, not
/// a hardcoded name.
#[test]
fn renamed_binder_reports_ts7005_with_its_own_name() {
    let source = "var widgetList = [undefined, null];";
    assert_eq!(
        messages(source),
        vec![(
            7005,
            "Variable 'widgetList' implicitly has an 'any[]' type.".to_string()
        )],
    );
}

/// Negative control, oracle-verified: a nullish leaf alongside a concrete
/// sibling element reduces to the concrete element's type (`number`), not
/// `any` — no diagnostic at all. An AST-only "does this literal contain a
/// nullish leaf" gate would wrongly fire here; the fix must check the
/// resulting element type instead.
#[test]
fn mixed_concrete_and_nullish_elements_reports_nothing() {
    let source = "var b = [1, undefined];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: object literals are excluded from this path — the
/// nullish property already gets its own per-property TS7018, and tsc does
/// not also report TS7005 for the variable (oracle-verified).
#[test]
fn object_literal_with_nullish_property_reports_only_ts7018() {
    let source = "var b = { p: undefined };";
    assert_eq!(
        messages(source),
        vec![(
            7018,
            "Object literal's property 'p' implicitly has an 'any' type.".to_string()
        )],
    );
}

/// Negative control: no nullish leaf at all, so the widen is a no-op and no
/// TS7005 fires — the gate must not fire merely because the initializer is a
/// literal.
#[test]
fn array_without_nullish_leaf_reports_nothing() {
    let source = "var b = [1, 2];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: a *declared* (non-widening) `undefined` value is not a
/// widening source (`initializer_nullish_leaves_are_widening` gate), so the
/// array keeps `undefined[]` and no TS7005 fires — mirrors the sibling
/// nonstrict-widening test suite's `declared_undefined_element_keeps_array_unwidened`.
#[test]
fn declared_undefined_element_does_not_report_ts7005() {
    let source = "\
declare var q: undefined;
var b = [q];
";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: an explicit type annotation means the declared type is
/// authoritative, not the widened literal type — no implicit-any diagnostic
/// applies at all.
#[test]
fn annotated_declaration_does_not_report_ts7005() {
    let source = "var b: any[] = [undefined, null];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: with `strictNullChecks` on, tsc never widens
/// null/undefined regardless of provenance, so the compound-widening TS7005
/// path must not fire either.
#[test]
fn strict_null_checks_on_does_not_report_ts7005() {
    let source = "var b = [undefined, null];";
    let strict_opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    assert_eq!(
        check_with_options_code_messages(source, strict_opts),
        Vec::<(u32, String)>::new()
    );
}

/// Negative control: `noImplicitAny` off suppresses every `TS7xxx` implicit-any
/// diagnostic, including this new compound path.
#[test]
fn no_implicit_any_off_does_not_report_ts7005() {
    let source = "var b = [undefined, null];";
    let lenient_opts = CheckerOptions {
        strict: false,
        strict_null_checks: false,
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    assert_eq!(
        check_with_options_code_messages(source, lenient_opts),
        Vec::<(u32, String)>::new()
    );
}

/// Negative control: an empty array literal is the pre-existing "evolving
/// any" path (`direct_empty_array_implicit_any`, deferred via
/// `pending_implicit_any_vars`), not this new immediate compound-literal
/// path — must not double-report.
#[test]
fn empty_array_literal_is_not_handled_by_the_compound_path() {
    let source = "var b = [];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: a destructuring pattern binding never gets TS7005 from
/// this path — mirrors the existing bare-scalar and empty-array guards'
/// identical exclusion. (tsc instead reports per-element `TS7031` here —
/// `Binding element 'a'/'b' implicitly has an 'any' type.` — a distinct,
/// pre-existing, unimplemented mechanism this PR does not touch; the
/// assertion only pins down that TS7005 itself never fires on a pattern.)
#[test]
fn destructuring_pattern_never_reports_ts7005() {
    let source = "var [a, b] = [undefined, null];";
    assert!(
        !messages(source).iter().any(|(code, _)| *code == 7005),
        "TS7005 must never fire for a destructuring pattern binding"
    );
}
