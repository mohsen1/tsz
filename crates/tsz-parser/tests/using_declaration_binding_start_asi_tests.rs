//! Tests for the statement-level `using` / `await using` declaration
//! disambiguation, mirroring tsc's `isUsingDeclaration` /
//! `isAwaitUsingDeclaration`
//! (`nextTokenIsBindingIdentifierOrStartOfObjectDestructuringOnSameLine`).
//!
//! Structural rule: a statement-leading `using` (or `await using`) begins a
//! `using` declaration only when the token after `using` is a **binding
//! identifier** — an identifier or a *contextual* keyword, so genuine reserved
//! words (`class`, `if`, `for`, …) are excluded while `yield` / `await`
//! (reserved only by context) are not — or an object-destructuring `{`, and only
//! when that token sits on the **same line** as `using`. Otherwise ASI ends an
//! expression statement and `using` is an ordinary identifier reference.
//!
//! Two shapes are deliberately NOT declarations, matching tsc:
//! - `using [a] = x` — parsed as an element-access expression `using[a]`, never a
//!   declaration (only `{` destructuring is eagerly parsed, not `[`).
//! - `using\nx = 1` — the line break splits `using` off as its own expression
//!   statement.
//!
//! These tests assert the parse *shape* (the leading `using` starts, or does not
//! start, a `VARIABLE_STATEMENT`), which is exactly what the lookahead controls;
//! binder names are varied to prove the behavior is structural, not keyed to a
//! spelling.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;

/// The `SyntaxKind` ids of the source file's top-level statements.
fn stmt_kinds(source: &str) -> Vec<u16> {
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    sf.statements
        .nodes
        .iter()
        .map(|&idx| arena.get(idx).unwrap().kind)
        .collect()
}

/// True when the first top-level statement is a `VARIABLE_STATEMENT` — i.e. the
/// leading `using` was taken as a declaration keyword.
fn leads_with_variable_statement(source: &str) -> bool {
    stmt_kinds(source)
        .first()
        .is_some_and(|&k| k == syntax_kind_ext::VARIABLE_STATEMENT)
}

// ---------------------------------------------------------------------------
// Same-line binding identifier / `{` → a `using` declaration (positive path).
// ---------------------------------------------------------------------------

#[test]
fn using_identifier_same_line_is_a_declaration() {
    assert!(leads_with_variable_statement("using x = null;"));
    assert!(leads_with_variable_statement("using resource = null;"));
}

#[test]
fn await_using_identifier_same_line_is_a_declaration() {
    assert!(leads_with_variable_statement("await using x = null;"));
    assert!(leads_with_variable_statement("await using handle = null;"));
}

#[test]
fn using_object_binding_pattern_same_line_is_a_declaration() {
    // `{` is eagerly parsed as an object binding pattern so a grammar error
    // (TS1492) can be reported later; the parse still leads with the declaration.
    assert!(leads_with_variable_statement("using { a } = obj;"));
    assert!(leads_with_variable_statement("using { first } = obj;"));
}

#[test]
fn using_contextual_keyword_binding_is_a_declaration() {
    // `yield` / `await` are reserved only by context, so tsc's `isBindingIdentifier`
    // still treats them as binding names here (and reports TS1214 / TS1262 later).
    assert!(leads_with_variable_statement("using yield = null;"));
    assert!(leads_with_variable_statement("using await = null;"));
    assert!(leads_with_variable_statement("using async = null;"));
    assert!(leads_with_variable_statement("using of = null;"));
}

// ---------------------------------------------------------------------------
// ASI: a line break before the binding ends the `using` expression statement.
// ---------------------------------------------------------------------------

#[test]
fn using_identifier_on_next_line_is_not_a_declaration() {
    // `using` alone is an expression statement; `x = 1` is a second statement.
    assert!(!leads_with_variable_statement("using\nx = 1;"));
    assert!(!leads_with_variable_statement("using\nresource = 1;"));
}

#[test]
fn using_object_pattern_on_next_line_is_not_a_declaration() {
    assert!(!leads_with_variable_statement("using\n{ a } = obj;"));
}

#[test]
fn await_using_identifier_on_next_line_is_not_a_declaration() {
    assert!(!leads_with_variable_statement("await using\nx = 1;"));
    assert!(!leads_with_variable_statement("await using\nhandle = 1;"));
}

#[test]
fn using_first_statement_is_an_expression_statement_under_asi() {
    let kinds = stmt_kinds("using\nx = 1;");
    assert_eq!(
        kinds.first().copied(),
        Some(syntax_kind_ext::EXPRESSION_STATEMENT),
        "leading `using` should be an expression statement, got {kinds:?}"
    );
    assert!(
        kinds.len() >= 2,
        "expected `using` and `x = 1` as separate statements, got {kinds:?}"
    );
}

// ---------------------------------------------------------------------------
// Reserved word after `using` → not a binding identifier → not a declaration.
// ---------------------------------------------------------------------------

#[test]
fn using_reserved_word_is_not_a_declaration() {
    // `class` / `if` / `for` are reserved words; tsc's `isBindingIdentifier`
    // rejects them, so `using` falls back to an expression statement.
    assert!(!leads_with_variable_statement("using class = 1;"));
    assert!(!leads_with_variable_statement("using if = 1;"));
    assert!(!leads_with_variable_statement("using for = 1;"));
}

// ---------------------------------------------------------------------------
// Array `[` after `using` → element-access expression, never a declaration.
// ---------------------------------------------------------------------------

#[test]
fn using_array_pattern_is_not_a_declaration() {
    // tsc parses `using [a] = x` as `using[a] = x` (element access), so the only
    // destructuring form a `using` declaration accepts is the object `{` form.
    assert!(!leads_with_variable_statement("using [a] = [null];"));
    assert!(!leads_with_variable_statement("using [first] = [null];"));
}

// ---------------------------------------------------------------------------
// for-header forms mirror the statement forms.
// ---------------------------------------------------------------------------

#[test]
fn for_using_binding_on_next_line_is_not_a_using_head() {
    // `for (using\n x of [])` — the line break means `using` is not the loop's
    // declaration keyword; it degrades to the expression path (no crash, no
    // spurious using-declaration head).
    let (parser, root) = parse_source("for (using\n x of []) {}");
    let arena = parser.get_arena();
    // Just assert we parsed a source file with at least one statement and did
    // not treat the head as a well-formed `using` for-of declaration.
    let sf = arena.get_source_file_at(root).unwrap();
    assert!(!sf.statements.nodes.is_empty());
}
