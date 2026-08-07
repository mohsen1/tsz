//! `parseErrorForMissingSemicolonAfter`'s fallback to TS1434 ("Unexpected
//! keyword or identifier") for an identifier expression statement whose text
//! is *itself* an exact keyword (`get`, `set`, `async`, `from`, `abstract`,
//! `out`, ...), oracle-verified against `typescript@7.0.2`.
//!
//! tsc's `parseErrorForMissingSemicolonAfter` special-cases exactly five
//! identifier texts (`const`/`let`/`var`, `declare`, `interface`, `is`,
//! `module`/`namespace`, `type`) and otherwise falls through to the generic
//! "Unexpected keyword or identifier" (TS1434) diagnostic — there is no
//! blanket rule in tsc that suppresses this for keyword-looking identifiers
//! in general. tsz previously suppressed TS1434 unconditionally whenever the
//! parsed identifier's text matched any entry in
//! `spelling::VIABLE_KEYWORD_SUGGESTIONS` (~90 keywords), reasoning that such
//! an identifier could only appear as a downstream artifact of an
//! already-reported error. That is true for some recovery paths but not in
//! general: a genuinely fresh statement like `async foo` (two bare
//! identifiers, no operator, same line) has no prior diagnostic and tsc
//! reports TS1434 at `async`, while tsz reported nothing at all.
//!
//! Fixed in `crates/tsz-parser/src/parser/state/recovery.rs`'s
//! `parse_error_for_missing_semicolon_after`: removed the blanket
//! keyword-text suppression, keeping only the one case tsc's own algorithm
//! actually special-cases differently for keyword text — a keyword-exact
//! identifier immediately followed by a closing delimiter (`)`/`]`) that
//! cannot start a new statement still gets TS1434 there (a failed nested
//! expression recovery, not a fresh statement).
//!
//! This also unblocks the `declare`/`abstract`/`override`/`out` prefix of
//! `crates/tsz-parser/tests/type_member_modifier_grammar_tests.rs`'s
//! documented follow-up (those modifiers before an interface/type-literal
//! accessor cascade into this exact tail via a fall-through to statement-level
//! parsing) — routing that cascade through this now-fixed function is left as
//! a separate, larger change (the type-member list-abort/fall-through
//! plumbing itself is not touched here).

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

/// `(code, line, column)` fingerprints, 1-based, in report order.
fn fingerprints(source: &str) -> Vec<(u32, u32, u32)> {
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (diag.code, pos.line + 1, pos.character + 1)
        })
        .collect()
}

const TS1434: u32 = diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER;
const TS1005: u32 = diagnostic_codes::EXPECTED;

// ---------------------------------------------------------------------------
// A fresh statement starting with an exact-keyword identifier, immediately
// followed by another token on the same line (no valid expression
// continuation, no ASI point): tsc reports TS1434 at the identifier.
// ---------------------------------------------------------------------------

#[test]
fn bare_async_before_identifier_reports_ts1434() {
    assert_eq!(fingerprints("async foo"), vec![(TS1434, 1, 1)]);
}

#[test]
fn bare_set_before_identifier_reports_ts1434() {
    assert_eq!(fingerprints("set foo"), vec![(TS1434, 1, 1)]);
}

#[test]
fn bare_get_before_identifier_reports_ts1434() {
    assert_eq!(fingerprints("get foo"), vec![(TS1434, 1, 1)]);
}

#[test]
fn bare_from_before_identifier_reports_ts1434() {
    assert_eq!(fingerprints("from foo"), vec![(TS1434, 1, 1)]);
}

#[test]
fn bare_out_before_identifier_reports_ts1434() {
    assert_eq!(fingerprints("out foo"), vec![(TS1434, 1, 1)]);
}

/// `declare` alone stays suppressed (tsc's own dedicated `case "declare"`),
/// but the identifier statement it cascades into (`get`, here) is a fresh
/// statement in its own right and still reports TS1434 — followed by the
/// call-expression tail's TS1005 for the missing `;` before the return-type
/// colon. Oracle-verified against `typescript@7.0.2`.
#[test]
fn declare_before_accessor_shaped_tail_reports_ts1434_then_ts1005() {
    assert_eq!(
        fingerprints("declare get x(): number"),
        vec![(TS1434, 1, 9), (TS1005, 1, 16)],
    );
}

/// `abstract` has no dedicated suppression case in tsc (unlike `declare`), so
/// it reports its own TS1434 too — two TS1434s in a row, one per bare
/// identifier statement, then the same call-expression TS1005 tail.
#[test]
fn abstract_before_accessor_shaped_tail_reports_two_ts1434_then_ts1005() {
    assert_eq!(
        fingerprints("abstract get x(): number"),
        vec![(TS1434, 1, 1), (TS1434, 1, 10), (TS1005, 1, 17)],
    );
}

/// `yield(foo)` outside a generator is a genuine regression witness, not a
/// synthetic case: `parse_yield_expression`'s outside-generator fallback
/// (`crates/tsz-parser/src/parser/state_expressions_unary.rs`) intentionally
/// returns a bare `yield` identifier node without continuing into
/// postfix/call parsing, as its own disambiguation strategy against a real
/// `YieldExpression` — unlike an ordinary identifier, it never reaches
/// `parse_postfix_expression`'s call-continuation. That leaves `(foo)`
/// unconsumed and this recovery path reached with `expression_text ==
/// "yield"`, which — absent the dedicated `"yield" => true` suppression
/// case in `parse_missing_semicolon_keyword_error` — would report a spurious
/// TS1434 that `tsc` never emits here (`tsc`'s own parser never reaches this
/// fallback for `yield`, since it does continue into the call). The checker's own
/// reserved-word check on the `yield` identifier (TS1212) plus the
/// unresolved-name check (TS2304) already reproduce `tsc`'s exact output
/// independently of this parser-level gap. Found via
/// `TypeScript/tests/cases/conformance/es6/yieldExpressions/{YieldExpression8,YieldExpression18}_es6.ts`
/// during this fix's own conformance verification.
#[test]
fn bare_yield_called_outside_generator_stays_suppressed() {
    assert_eq!(fingerprints("yield(foo);"), vec![]);
}

/// Inside a generator, `yield` always builds a full `YieldExpression` node
/// (never the bare-identifier fallback above), so this path is unaffected
/// either way — kept as a control.
#[test]
fn yield_inside_generator_parses_clean() {
    assert_eq!(fingerprints("function* g() { yield(foo); }"), vec![],);
}

// ---------------------------------------------------------------------------
// Negative / adjacent cases: keyword-exact identifiers that ARE valid
// continuations, or fall into tsc's own special-cased suppression, must stay
// clean.
// ---------------------------------------------------------------------------

/// `(` is a valid expression continuation (call expression) — no missing
/// semicolon at all, so this recovery path is never reached.
#[test]
fn get_called_as_a_function_parses_clean() {
    assert_eq!(fingerprints("get()"), vec![]);
}

/// tsc's own dedicated `case "declare"` in `parseErrorForMissingSemicolonAfter`
/// still suppresses the bare `declare` identifier itself when nothing
/// unparseable immediately follows on the same line.
#[test]
fn declare_type_alias_parses_clean() {
    assert_eq!(fingerprints("declare type X = number;"), vec![]);
}

#[test]
fn async_function_declaration_parses_clean() {
    assert_eq!(fingerprints("async function f() {}"), vec![]);
}

/// Regression witness for the fix: a failed old-style type-predicate
/// assertion (`as numOrStr is string`) leaves the contextual keyword `is`
/// as a fresh identifier-expression-statement once the assertion expression
/// gives up — tsc still reports TS1434 there. This exact source previously
/// passed with a coarse "TS1434 is present somewhere" assertion; pinning
/// the full oracle-verified fingerprint list here instead.
#[test]
fn failed_type_predicate_assertion_reports_full_oracle_cascade() {
    let source = "\ndeclare var numOrStr: number | string;\n\nif (<numOrStr is string>(numOrStr === undefined)) {\n}\n\nif ((numOrStr === undefined) as numOrStr is string) {\n}\n";
    assert_eq!(
        fingerprints(source),
        vec![
            (TS1005, 4, 15),
            (TS1005, 4, 18),
            (TS1005, 4, 49),
            (TS1005, 7, 42),
            (TS1434, 7, 45),
            (diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED, 7, 51),
        ],
    );
}
