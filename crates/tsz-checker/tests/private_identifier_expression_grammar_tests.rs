//! Grammar checks for a `PrivateIdentifier` used as a standalone expression.
//!
//! A private name (`#field`) is only legal in three positions: a member-access
//! name (`obj.#field`), the direct left-hand side of an `in` expression
//! (`#field in obj`), or a class-member declaration. Anywhere else it is a
//! standalone expression, which `tsc`'s `checkGrammarPrivateIdentifierExpression`
//! rejects with TS18016 (`Private identifiers are not allowed outside class
//! bodies`) when it is outside any class body, or TS1451 (`Private identifiers
//! are only allowed in class bodies and may only be used as part of a class
//! member declaration, property access, or on the left-hand-side of an 'in'
//! expression`) when it is inside a class but in one of these invalid positions.
//!
//! Binder names are varied (not `#foo`/`C`) to rule out any name-keyed path.
//! Oracle-verified against `tsc` (positions and codes).

use tsz_binder::BinderState;
use tsz_checker::{context::CheckerOptions, state::CheckerState};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Count how many diagnostics with `code` the parser+checker produce for `source`.
fn count_code(source: &str, code: u32) -> usize {
    let mut parser = ParserState::new("case.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "case.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let parser_hits = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == code)
        .count();
    let checker_hits = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == code)
        .count();
    parser_hits + checker_hits
}

const TS18016: u32 = 18016;
const TS1451: u32 = 1451;

// --- Outside any class body: TS18016, exactly once, in every position. ---

#[test]
fn bare_private_expression_outside_class_reports_ts18016() {
    assert_eq!(count_code("const quux = #alpha;", TS18016), 1);
    assert_eq!(count_code("const quux = #alpha;", TS1451), 0);
}

#[test]
fn private_expression_positions_outside_class_each_report_ts18016_once() {
    for source in [
        "let sink; sink = #beta;",
        "function grab() { return #gamma; }",
        "const bucket = [#delta];",
        "declare function take(value: unknown): void; take(#epsilon);",
        "#zeta;",
        "const sum = #eta + 1;",
        "const wrapped = (#theta);",
    ] {
        assert_eq!(count_code(source, TS18016), 1, "source: {source:?}");
        assert_eq!(count_code(source, TS1451), 0, "source: {source:?}");
    }
}

#[test]
fn parenthesized_in_lhs_outside_class_reports_ts18016_not_ts1451() {
    // A parenthesized private identifier is not the *direct* LHS of `in`, so it
    // is a standalone expression; outside a class that is TS18016, not TS1451.
    assert_eq!(count_code("(#iota) in {};", TS18016), 1);
    assert_eq!(count_code("(#iota) in {};", TS1451), 0);
    assert_eq!(count_code("((#kappa)) in {};", TS18016), 1);
}

// --- Inside a class body but an invalid position: TS1451, exactly once. ---

#[test]
fn bare_private_expression_inside_class_reports_ts1451() {
    let source = "class Widget { run() { return #lambda; } }";
    assert_eq!(count_code(source, TS1451), 1);
    assert_eq!(count_code(source, TS18016), 0);
}

#[test]
fn private_expression_positions_inside_class_each_report_ts1451_once() {
    for source in [
        "class Gadget { #mu = 1; run() { return [#mu]; } }",
        "class Gizmo { #nu = 1; run() { return (#nu) in this; } }",
        "class Doohickey { #xi = 1; run() { const echo = #xi + 0; return echo; } }",
    ] {
        assert_eq!(count_code(source, TS1451), 1, "source: {source:?}");
        assert_eq!(count_code(source, TS18016), 0, "source: {source:?}");
    }
}

// --- Valid positions: no standalone-expression grammar diagnostic. ---

#[test]
fn valid_private_positions_report_neither_code() {
    // Direct `in` LHS and member access inside the declaring class are legal.
    for source in [
        "class Vessel { #omicron = 1; run() { return #omicron in this; } }",
        "class Crate { #pi = 1; run() { return this.#pi; } }",
    ] {
        assert_eq!(count_code(source, TS1451), 0, "source: {source:?}");
        assert_eq!(count_code(source, TS18016), 0, "source: {source:?}");
    }
}

#[test]
fn direct_private_in_lhs_outside_class_reports_ts18016_once() {
    // `#name in obj` outside a class is TS18016 (owned by the `in`-operator
    // checker), and must not double now that standalone-expression dispatch
    // also handles private identifiers.
    assert_eq!(count_code("#rho in {};", TS18016), 1);
    assert_eq!(count_code("#rho in {};", TS1451), 0);
}

// --- `for..in` / `for..of` binding position (tsc: TS2406 / TS2487 own it). ---

#[test]
fn for_in_binding_inside_class_reports_neither_grammar_code() {
    // tsc leaves the `for..in` binding to TS2406 and does not add TS1451.
    let source = "class Reactor { #sigma = 1; run() { for (#sigma in {}) {} } }";
    assert_eq!(count_code(source, TS1451), 0);
    assert_eq!(count_code(source, TS18016), 0);
}

#[test]
fn for_in_binding_outside_class_still_reports_ts18016() {
    // Outside any class the no-class check wins even in a `for..in` head.
    assert_eq!(count_code("for (#tau in {}) {}", TS18016), 1);
    assert_eq!(count_code("for (#tau in {}) {}", TS1451), 0);
}

#[test]
fn for_of_binding_is_not_exempted_like_for_in() {
    // `for..of` is *not* the exempted position: inside a class its private-id
    // binding is TS1451 (alongside TS2487), outside a class it is TS18016.
    let inside = "class Turbine { #upsilon = 1; run() { for (#upsilon of []) {} } }";
    assert_eq!(count_code(inside, TS1451), 1);
    assert_eq!(count_code(inside, TS18016), 0);
    assert_eq!(count_code("for (#phi of []) {}", TS18016), 1);
    assert_eq!(count_code("for (#phi of []) {}", TS1451), 0);
}
