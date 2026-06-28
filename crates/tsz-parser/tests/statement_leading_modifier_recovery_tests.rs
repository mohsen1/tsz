//! Tests for statement-leading misplaced/duplicated declaration modifiers.
//!
//! A statement that begins with a run of declaration modifiers (a duplicated
//! `declare`, a stray `abstract`) before a `var`/`let`/`const`/`function`
//! declaration must be parsed as a modifier-prefixed declaration so the proper
//! grammar diagnostic fires (TS1030 for a duplicate `declare`, TS1242 for a
//! misplaced `abstract`). Previously tsz degraded these to an expression
//! statement, producing a spurious TS2304 "Cannot find name" instead.

use crate::parser::test_fixture::parse_source;

fn diagnostics(source: &str) -> Vec<(u32, u32, String)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start, d.message.clone()))
        .collect()
}

fn codes(source: &str) -> Vec<u32> {
    diagnostics(source).iter().map(|(c, _, _)| *c).collect()
}

fn count(source: &str, code: u32) -> usize {
    codes(source).iter().filter(|c| **c == code).count()
}

// =========================================================================
// Duplicate `declare` modifier -> TS1030 (not TS2304)
// =========================================================================

#[test]
fn duplicate_declare_const_emits_ts1030_not_ts2304() {
    let source = "declare declare const x = 1;";
    let diags = diagnostics(source);
    assert!(
        diags.iter().any(|(c, _, _)| *c == 1030),
        "duplicate `declare` should emit TS1030, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(c, _, _)| *c == 2304),
        "duplicate `declare` must NOT degrade to TS2304, got {diags:?}"
    );
}

#[test]
fn duplicate_declare_ts1030_anchored_at_second_declare() {
    // `declare declare const x = 1;`
    //  ^col1   ^col9 (offset 8)
    let source = "declare declare const x = 1;";
    let ts1030 = diagnostics(source)
        .into_iter()
        .find(|(c, _, _)| *c == 1030)
        .expect("expected TS1030");
    assert_eq!(ts1030.1, 8, "TS1030 should anchor at the second `declare`");
}

#[test]
fn triple_declare_emits_two_ts1030() {
    let source = "declare declare declare const x = 1;";
    assert_eq!(
        count(source, 1030),
        2,
        "two redundant `declare` keywords should each emit TS1030"
    );
}

#[test]
fn duplicate_declare_class_still_parses_and_emits_ts1030() {
    let source = "declare declare class C {}";
    assert_eq!(count(source, 1030), 1);
    assert_eq!(count(source, 2304), 0);
}

// =========================================================================
// Misplaced `abstract` before var/function declarations -> TS1242 (not TS2304)
// =========================================================================

#[test]
fn abstract_const_emits_ts1242_not_ts2304() {
    let source = "abstract const x = 1;";
    let diags = diagnostics(source);
    assert!(
        diags.iter().any(|(c, _, _)| *c == 1242),
        "`abstract const` should emit TS1242, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(c, _, _)| *c == 2304),
        "`abstract const` must NOT degrade to TS2304, got {diags:?}"
    );
}

#[test]
fn abstract_ts1242_anchored_at_abstract_keyword() {
    let source = "abstract const x = 1;";
    let ts1242 = diagnostics(source)
        .into_iter()
        .find(|(c, _, _)| *c == 1242)
        .expect("expected TS1242");
    assert_eq!(
        ts1242.1, 0,
        "TS1242 should anchor at the `abstract` keyword"
    );
}

#[test]
fn abstract_let_var_function_emit_ts1242() {
    for source in [
        "abstract let y = 2;",
        "abstract var z = 3;",
        "abstract function f() {}",
    ] {
        assert_eq!(
            count(source, 1242),
            1,
            "{source:?} should emit exactly one TS1242"
        );
        assert_eq!(count(source, 2304), 0, "{source:?} should not emit TS2304");
    }
}

// =========================================================================
// Genuine identifier uses and valid forms must be unchanged
// =========================================================================

#[test]
fn declare_as_identifier_unchanged() {
    // `declare;`/`abstract;` are genuine identifier-expression uses. The parser
    // must not synthesize a grammar diagnostic for them; they parse as an
    // expression statement (the checker later reports TS2304 "Cannot find name").
    for source in ["declare;", "abstract;"] {
        assert_eq!(count(source, 1030), 0, "{source:?} should not emit TS1030");
        assert_eq!(count(source, 1242), 0, "{source:?} should not emit TS1242");
    }
}

#[test]
fn valid_declare_const_no_grammar_error() {
    let source = "declare const x = 1;";
    assert_eq!(count(source, 1030), 0);
    assert_eq!(count(source, 2304), 0);
    assert_eq!(count(source, 1242), 0);
}

#[test]
fn valid_abstract_class_no_ts1242() {
    let source = "abstract class C {}";
    assert_eq!(count(source, 1242), 0);
    assert_eq!(count(source, 2304), 0);
}

#[test]
fn valid_declare_abstract_class_no_grammar_error() {
    let source = "declare abstract class C {}";
    assert_eq!(count(source, 1030), 0);
    assert_eq!(count(source, 1242), 0);
    assert_eq!(count(source, 2304), 0);
}

#[test]
fn declare_with_line_break_is_asi_expression() {
    // ASI: `declare` then a newline is a standalone expression statement, so the
    // following `declare const` is its own (valid) ambient declaration.
    let source = "declare\ndeclare const x = 1;";
    assert_eq!(
        count(source, 1030),
        0,
        "ASI must prevent treating the two `declare`s as one modifier run"
    );
}
