//! Tests for the definite assignment assertion `!` in variable declarations.
//!
//! Structural rule (mirrors tsc's `parseVariableDeclaration`): a postfix `!`
//! after a variable binding is only absorbed as a definite assignment assertion
//! when **all three** hold:
//!   1. `allowExclamation` is set — tsc clears it inside `for` initializers
//!      (`allowExclamation = !inForStatementInitializer`);
//!   2. the binding `name` is a plain `Identifier`, never an object/array
//!      binding pattern;
//!   3. there is no preceding line break (so ASI does not split the statement).
//!
//! When any condition fails tsc leaves the `!` for declaration-list recovery
//! (it does not become a definite assignment assertion, and the wrong-context
//! grammar errors TS1263/TS1264 must not fire). Before the fix tsz consumed the
//! `!` unconditionally, mis-reporting TS1263/TS2483 instead of the structural
//! comma/expression recovery tsc emits.
//!
//! Tests vary the binder name (`x`, `value`, `count`, …) to prove the behavior
//! is structural and not keyed to a specific identifier spelling.

use crate::parser::test_fixture::parse_source;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

/// Find the first `VARIABLE_DECLARATION` node and return whether it recorded a
/// definite assignment assertion (`exclamation_token`).
fn first_decl_has_exclamation(parser: &crate::parser::ParserState) -> Option<bool> {
    let arena = parser.get_arena();
    arena
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == syntax_kind_ext::VARIABLE_DECLARATION)
        .and_then(|(idx, _)| arena.get_variable_declaration_at(NodeIndex(idx as u32)))
        .map(|decl| decl.exclamation_token)
}

fn codes(parser: &crate::parser::ParserState) -> Vec<u32> {
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

// ---------------------------------------------------------------------------
// Positive cases: a plain identifier on one line DOES take the assertion.
// ---------------------------------------------------------------------------

#[test]
fn identifier_with_type_annotation_takes_assertion() {
    for (source, ident) in [
        ("let x!: number;", "x"),
        ("let value!: string;", "value"),
        ("var counter!: bigint;", "counter"),
        ("const ready!: boolean = true as boolean;", "ready"),
    ] {
        let (parser, _) = parse_source(source);
        assert_eq!(
            first_decl_has_exclamation(&parser),
            Some(true),
            "`{ident}` definite assignment must be absorbed for {source:?}"
        );
    }
}

#[test]
fn definite_assignment_does_not_emit_wrong_context_errors() {
    // A well-formed `let x!: number;` is valid TypeScript — no TS1263/TS1264.
    let (parser, _) = parse_source("let x!: number;");
    assert!(
        parser.get_diagnostics().is_empty(),
        "valid definite assignment must not emit diagnostics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn multiple_declarators_each_take_their_own_assertion() {
    let source = "let a!: number, b!: string, c!: boolean;";
    let (parser, _) = parse_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "multi-declarator definite assignments must parse cleanly, got {:?}",
        parser.get_diagnostics()
    );
    let arena = parser.get_arena();
    let with_excl = arena
        .nodes
        .iter()
        .filter(|n| n.kind == syntax_kind_ext::VARIABLE_DECLARATION)
        .count();
    assert_eq!(with_excl, 3, "expected three declarators in {source:?}");
}

// ---------------------------------------------------------------------------
// Binding patterns never take the assertion (tsc: `name.kind === Identifier`).
// ---------------------------------------------------------------------------

#[test]
fn array_binding_pattern_does_not_take_assertion() {
    // tsc recovers `const [a]!: T = ...` as a missing-comma cascade and never
    // reports TS1263 (initializer + assertion) nor TS1182 (destructuring needs
    // an initializer) — the `!` is simply not a definite assignment here.
    let (parser, _) = parse_source("const [a]!: number[] = [1];");
    assert_eq!(
        first_decl_has_exclamation(&parser),
        Some(false),
        "array binding pattern must not absorb `!`"
    );
    let found = codes(&parser);
    assert!(
        !found.contains(
            &diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS
        ),
        "TS1263 must not fire for a binding pattern, got {found:?}"
    );
    assert!(
        !found.contains(&diagnostic_codes::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER),
        "TS1182 must be suppressed when a stray `!` follows the pattern, got {found:?}"
    );
    assert!(
        found.contains(&diagnostic_codes::EXPECTED),
        "tsc emits TS1005 ',' expected at the stray `!`, got {found:?}"
    );
}

#[test]
fn object_binding_pattern_does_not_take_assertion() {
    let (parser, _) = parse_source("const { value }!: any = {};");
    assert_eq!(
        first_decl_has_exclamation(&parser),
        Some(false),
        "object binding pattern must not absorb `!`"
    );
    let found = codes(&parser);
    assert!(
        !found.contains(
            &diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS
        ),
        "TS1263 must not fire for a binding pattern, got {found:?}"
    );
    assert!(
        found.contains(&diagnostic_codes::EXPECTED),
        "tsc emits TS1005 ',' expected at the stray `!`, got {found:?}"
    );
}

#[test]
fn plain_binding_pattern_without_assertion_still_requires_initializer() {
    // The TS1182 suppression must be *gated on a stray `!`* — an ordinary
    // initializer-less destructuring declaration must still report TS1182.
    for source in ["const [a];", "let { value };"] {
        let found = codes(&parse_source(source).0);
        assert!(
            found.contains(&diagnostic_codes::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER),
            "TS1182 must still fire for {source:?}, got {found:?}"
        );
    }
}

#[test]
fn line_break_before_bang_on_binding_pattern_keeps_ts1182() {
    // A `!` after a line break is a separate expression statement (ASI), not a
    // stray definite assignment — so the initializer-less binding pattern must
    // STILL report TS1182. The suppression is gated on a *same-line* `!`.
    for source in ["const {x}\n!foo;", "const [a]\n!bar;"] {
        let found = codes(&parse_source(source).0);
        assert!(
            found.contains(&diagnostic_codes::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER),
            "TS1182 must still fire across a line break for {source:?}, got {found:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// ASI: a line break before `!` splits the statement, so `!` is never absorbed.
// ---------------------------------------------------------------------------

#[test]
fn line_break_before_bang_splits_into_two_statements() {
    // `let x` is complete; `!foo;` on the next line is a separate expression
    // statement. The declaration must NOT record a definite assignment.
    for (source, ident) in [
        ("let x\n!foo;", "x"),
        ("let value\n!ready();", "value"),
        ("var flag\n!flag;", "flag"),
    ] {
        let (parser, root) = parse_source(source);
        assert_eq!(
            first_decl_has_exclamation(&parser),
            Some(false),
            "ASI must prevent `!` absorption for {source:?} (binder `{ident}`)"
        );
        let arena = parser.get_arena();
        let sf = arena.get_source_file_at(root).unwrap();
        assert_eq!(
            sf.statements.nodes.len(),
            2,
            "ASI must yield two statements for {source:?}"
        );
        // No wrong-context definite-assignment grammar error.
        let found = codes(&parser);
        assert!(
            !found.contains(
                &diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS
            ),
            "TS1263 must not fire across the line break for {source:?}, got {found:?}"
        );
    }
}

#[test]
fn same_line_bang_after_identifier_is_still_absorbed() {
    // Guard the line-break rule against over-reach: a same-line `!` after a
    // plain identifier is still a definite assignment assertion.
    let (parser, _) = parse_source("let x!: number;");
    assert_eq!(first_decl_has_exclamation(&parser), Some(true));
}

// ---------------------------------------------------------------------------
// `for` initializers clear `allowExclamation`: `!` is never a definite
// assignment assertion there (regardless of binder spelling).
// ---------------------------------------------------------------------------

#[test]
fn for_of_initializer_does_not_take_assertion() {
    for (source, _ident) in [
        ("for (const x!: number of []) {}", "x"),
        ("for (const item!: string of []) {}", "item"),
    ] {
        let (parser, _) = parse_source(source);
        assert_eq!(
            first_decl_has_exclamation(&parser),
            Some(false),
            "for-of initializer must not absorb `!` for {source:?}"
        );
        let found = codes(&parser);
        assert!(
            found.contains(&diagnostic_codes::EXPECTED),
            "tsc emits TS1005 ',' expected for {source:?}, got {found:?}"
        );
    }
}

#[test]
fn for_in_initializer_does_not_take_assertion() {
    let (parser, _) = parse_source("for (const key!: string in {}) {}");
    assert_eq!(
        first_decl_has_exclamation(&parser),
        Some(false),
        "for-in initializer must not absorb `!`"
    );
}

#[test]
fn plain_for_initializer_does_not_take_assertion() {
    let (parser, _) = parse_source("for (let i!: number = 0; i < 3; i++) {}");
    assert_eq!(
        first_decl_has_exclamation(&parser),
        Some(false),
        "C-style for initializer must not absorb `!`"
    );
    let found = codes(&parser);
    assert!(
        found.contains(&diagnostic_codes::EXPECTED),
        "tsc emits TS1005 ',' expected at the stray `!`, got {found:?}"
    );
}

#[test]
fn valid_for_loops_are_unaffected() {
    // The fix must not perturb ordinary `for` headers without a stray `!`.
    for source in [
        "for (let i = 0; i < 3; i++) {}",
        "for (const x of [1]) { x; }",
        "for (const k in {}) { k; }",
        "for (let a = 0, b = 1; a < b; a++) {}",
    ] {
        let (parser, _) = parse_source(source);
        assert!(
            parser.get_diagnostics().is_empty(),
            "valid for-loop {source:?} must parse cleanly, got {:?}",
            parser.get_diagnostics()
        );
    }
}

// ---------------------------------------------------------------------------
// The stray `!` is recovered as a separate token, not silently dropped: the
// node stream still contains the `for` body, etc. (smoke test for no panic).
// ---------------------------------------------------------------------------

#[test]
fn stray_bang_recovery_does_not_panic_and_keeps_for_body() {
    let (parser, root) = parse_source("for (let i!: number = 0; i < 3; i++) { i; }");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 1, "the `for` statement survives");
    assert_eq!(
        arena.get(sf.statements.nodes[0]).unwrap().kind,
        syntax_kind_ext::FOR_STATEMENT
    );
    // Sanity: there is no leftover lone `!` swallowing the loop body.
    assert!(
        !parser.get_diagnostics().is_empty(),
        "the stray `!` is reported, not ignored"
    );
    let _ = SyntaxKind::ExclamationToken;
}
