//! Duplicate parameter names (TS2300) for the function-*expression* forms.
//!
//! `tsc` reports `TS2300 Duplicate identifier '<name>'` on every occurrence of a
//! repeated parameter name in any function-like signature, whatever syntactic
//! form it takes. tsz already covered the named *declaration* forms — function
//! declarations, class methods, constructors, interface methods and ambient
//! (`declare`) function declarations — because those route through their own
//! declaration checkers, which call `check_duplicate_parameters`.
//!
//! The function-*expression* forms did not: an arrow function, a function
//! expression and an object-literal method reach their signature grammar checks
//! (`check_parameter_ordering`, rest-parameter typing, …) through
//! `get_type_of_function`/the object-literal computation instead, and the
//! duplicate check was simply never wired in alongside them. So
//!
//! ```ts
//! const f = (a: number, a: string) => a;   // tsc: TS2300 x2, tsz: (nothing)
//! ```
//!
//! compiled clean on tsz while `function f(a: number, a: string) {}` did not.
//! Every row below is oracle-pinned against `typescript@7.0.2`
//! (`--noEmit --strict`); the exact-code assertions also prove no *other*
//! diagnostic fires on these signatures.
//!
//! Scope note: this fixes the *call-site* gap for simple (identifier)
//! parameters. Two neighbouring gaps are pre-existing and orthogonal — they
//! affect the declaration forms too and are left for a follow-up: duplicate
//! *binding-pattern* names (`function f({ a, a }) {}`) and the function-*type*
//! form (`type F = (a, a) => void`) reporting one occurrence instead of two.

use crate::test_utils::check_source_strict_codes as codes;

// ---------------------------------------------------------------------------
// The forms that regressed: function-expression shapes now report TS2300.
// tsc reports the code once per occurrence, so a two-parameter clash is two
// diagnostics — and nothing else.
// ---------------------------------------------------------------------------

#[test]
fn arrow_function_duplicate_parameter_reports_ts2300() {
    assert_eq!(
        codes("const f = (a: number, a: string) => a;\n"),
        vec![2300, 2300]
    );
}

#[test]
fn function_expression_duplicate_parameter_reports_ts2300() {
    assert_eq!(
        codes("const f = function (a: number, a: string) { return a; };\n"),
        vec![2300, 2300],
    );
}

#[test]
fn object_literal_method_duplicate_parameter_reports_ts2300() {
    assert_eq!(
        codes("const o = { m(a: number, a: string) { return a; } };\n"),
        vec![2300, 2300],
    );
}

/// A statement-position function expression is checked through both the
/// statement callback and `get_type_of_function`; the exact-code assertion
/// proves the diagnostic does not double up (two occurrences, not four).
#[test]
fn statement_position_function_expression_reports_ts2300_once_per_occurrence() {
    assert_eq!(
        codes("(function (a: number, a: string) { return a; });\n"),
        vec![2300, 2300],
    );
}

/// A nested arrow inside another function is still a distinct signature and is
/// checked on its own.
#[test]
fn nested_arrow_duplicate_parameter_reports_ts2300() {
    assert_eq!(
        codes("function outer() { const g = (b: number, b: string) => b; return g; }\n"),
        vec![2300, 2300],
    );
}

// ---------------------------------------------------------------------------
// Anti-hardcoding: the rule keys on the parameter *name* collision, not on any
// particular spelling. Renamed binders behave identically.
// ---------------------------------------------------------------------------

#[test]
fn arrow_duplicate_parameter_rule_is_binder_name_independent() {
    assert_eq!(
        codes("const f = (payload: 1, payload: 2) => payload;\n"),
        vec![2300, 2300]
    );
    assert_eq!(
        codes("const g = function (widget: 1, widget: 2) { return widget; };\n"),
        vec![2300, 2300],
    );
    assert_eq!(
        codes("const o = { run(ctx: 1, ctx: 2) { return ctx; } };\n"),
        vec![2300, 2300]
    );
}

/// Three occurrences draw three diagnostics — one per parameter — matching
/// `tsc`, and a non-adjacent duplicate (separated by a distinct parameter) is
/// still caught.
#[test]
fn arrow_triple_and_non_adjacent_duplicates() {
    assert_eq!(
        codes("const f = (a: 1, a: 2, a: 3) => a;\n"),
        vec![2300, 2300, 2300]
    );
    assert_eq!(
        codes("const f = (a: 1, b: 2, a: 3) => a;\n"),
        vec![2300, 2300]
    );
}

// ---------------------------------------------------------------------------
// The declaration forms were already correct — pin them so the shared
// `check_duplicate_parameters` wiring cannot start double-reporting them.
// ---------------------------------------------------------------------------

#[test]
fn declaration_forms_still_report_exactly_two() {
    assert_eq!(
        codes("function f(a: number, a: string) {}\n"),
        vec![2300, 2300]
    );
    assert_eq!(
        codes("class C { m(a: number, a: string) {} }\n"),
        vec![2300, 2300]
    );
    assert_eq!(
        codes("class D { constructor(a: number, a: string) {} }\n"),
        vec![2300, 2300]
    );
    assert_eq!(
        codes("interface I { m(a: number, a: string): void; }\n"),
        vec![2300, 2300]
    );
    assert_eq!(
        codes("declare function g(a: number, a: string): void;\n"),
        vec![2300, 2300]
    );
}

// ---------------------------------------------------------------------------
// Distinct parameter names stay clean on every form — the wiring must not
// invent a collision.
// ---------------------------------------------------------------------------

#[test]
fn distinct_parameters_stay_clean_on_every_form() {
    assert_eq!(
        codes("const f = (a: number, b: string) => a;\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        codes("const f = function (a: number, b: string) { return a; };\n"),
        Vec::<u32>::new(),
    );
    assert_eq!(
        codes("const o = { m(a: number, b: string) { return a; } };\n"),
        Vec::<u32>::new()
    );
    // A getter takes no parameters and a setter takes one, so neither can carry
    // a duplicate; they must stay clean rather than trip the new wiring.
    assert_eq!(
        codes("const o = { get p() { return 1; } };\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        codes("const o = { set p(v: number) {} };\n"),
        Vec::<u32>::new()
    );
}
