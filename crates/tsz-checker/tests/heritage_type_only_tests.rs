//! Tests for heritage clause type-only suppression behavior.
//!
//! TS1361/TS2693 should be suppressed in type-only contexts (interface extends,
//! declare class extends) but NOT in value contexts (non-ambient class extends).

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Non-ambient class extending a type-only symbol (interface) should emit TS2693.
/// `class U extends I {}` where I is an interface → TS2693.
#[test]
fn class_extends_interface_emits_ts2693() {
    let source = r"
interface I { x: number; }
class U extends I {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2693 for using interface as value in class extends
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count >= 1,
        "Expected TS2693 for class extending interface, got {} errors: {:?}",
        ts2693_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Interface extending another interface should NOT emit TS2693.
/// `interface Q extends I {}` → no error.
#[test]
fn interface_extends_interface_no_ts2693() {
    let source = r"
interface I { x: number; }
interface Q extends I {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should NOT emit TS2693 for interface extending interface
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693 for interface extends, got {}: {:?}",
        ts2693_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Declare class extending an interface should NOT emit TS2693.
/// `declare class B extends I {}` → no error (ambient context, no runtime code).
#[test]
fn declare_class_extends_interface_no_ts2693() {
    let source = r"
interface I { x: number; }
declare class B extends I {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should NOT emit TS2693 for declare class extends (ambient context)
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693 for declare class extends, got {}: {:?}",
        ts2693_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
