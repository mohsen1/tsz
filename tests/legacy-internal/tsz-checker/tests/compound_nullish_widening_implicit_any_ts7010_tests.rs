//! TS7010 for a function declaration whose inferred return type is a
//! non-empty array literal widened to `any[]` by non-strict null/undefined
//! widening — the return-position twin of `TS7005`'s
//! `compound_nullish_widening_implicit_any_ts7005_tests.rs`.
//!
//! Structural rule: `widen_nullish_return_contribution`
//! (`types/utilities/return_type_nullish.rs`) already widens a `return
//! [undefined, null];` contribution to `any[]` when `strictNullChecks` is
//! off — but `maybe_report_implicit_any_return`'s `should_report_implicit_any_return`
//! gate only fires for a return type of *exactly* `any` (deliberately, so a
//! deeply-nested `any` inside e.g. `Promise<any>` doesn't false-positive), so
//! the `any[]` case fell through silently.
//!
//! Owner: `state/state_checking_members/overload_compatibility.rs`'s
//! `maybe_report_implicit_any_return`, which now also accepts
//! `array_element_type(return_type) == ANY` gated on
//! `any_return_is_array_literal_with_nullish_leaf`
//! (`types/utilities/return_type_nullish.rs`) — a genuine `null`/`undefined`/
//! elided-hole leaf in at least one `return <array literal>;`, not merely a
//! resulting element type of `any` (which `declare var y: any; return [y];`
//! also has, purely from `y`'s own declaration, and tsc stays silent there).
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

/// The reported repro (`wideningTuples3.ts`'s theme, return-position leg).
#[test]
fn array_of_nullish_keywords_return_reports_ts7010() {
    let source = "\
function f() {
    return [undefined, null];
}
";
    assert_eq!(
        messages(source),
        vec![(
            7010,
            "'f', which lacks return-type annotation, implicitly has an 'any[]' return type."
                .to_string()
        )],
    );
}

/// Renamed binder: the diagnostic text must track the actual identifier.
#[test]
fn renamed_function_reports_ts7010_with_its_own_name() {
    let source = "\
function widgetFactory() {
    return [undefined, null];
}
";
    assert_eq!(
        messages(source),
        vec![(
            7010,
            "'widgetFactory', which lacks return-type annotation, implicitly has an 'any[]' return type."
                .to_string()
        )],
    );
}

/// Multiple return paths (`if`/else-less fallthrough) both widening: still
/// one TS7010 at the declaration site, not per return statement.
#[test]
fn multiple_widening_return_paths_report_ts7010_once() {
    let source = "\
function f(cond: boolean) {
    if (cond) {
        return [undefined, null];
    }
    return [undefined, null];
}
";
    assert_eq!(
        messages(source),
        vec![(
            7010,
            "'f', which lacks return-type annotation, implicitly has an 'any[]' return type."
                .to_string()
        )],
    );
}

/// Negative control, oracle-verified: a nullish leaf alongside a concrete
/// sibling element reduces to the concrete element's type (`number`), so the
/// function's return type is `number[]`, not `any[]` — no diagnostic.
#[test]
fn mixed_concrete_and_nullish_return_reports_nothing() {
    let source = "\
function f() {
    return [1, undefined];
}
";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control (regression guard, oracle-verified): an element whose
/// type is already `any` through its own declaration — not through nullish
/// widening — must not report TS7010, mirroring the identical TS7005 guard
/// for the mutable-binding seam.
#[test]
fn array_of_already_any_element_return_reports_nothing() {
    let source = "\
function f(y: any) {
    return [y];
}
";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: an explicit return-type annotation makes the declared
/// type authoritative — no implicit-any diagnostic applies at all.
#[test]
fn annotated_return_type_does_not_report_ts7010() {
    let source = "\
function f(): any[] {
    return [undefined, null];
}
";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: with `strictNullChecks` on, tsc never widens
/// null/undefined regardless of provenance, so this path must not fire.
#[test]
fn strict_null_checks_on_does_not_report_ts7010() {
    let source = "\
function f() {
    return [undefined, null];
}
";
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

/// Negative control: `noImplicitAny` off suppresses every `TS7xxx`
/// implicit-any diagnostic, including this path.
#[test]
fn no_implicit_any_off_does_not_report_ts7010() {
    let source = "\
function f() {
    return [undefined, null];
}
";
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

/// Negative control: the pre-existing bare-nullish-scalar TS7010 path
/// (`function f() { return null; }`) must keep reporting unaffected by this
/// change — it goes through the exactly-`ANY` branch, not the new array leg.
#[test]
fn bare_nullish_scalar_return_still_reports_ts7010() {
    let source = "\
function f() {
    return null;
}
";
    assert_eq!(
        messages(source),
        vec![(
            7010,
            "'f', which lacks return-type annotation, implicitly has an 'any' return type."
                .to_string()
        )],
    );
}
