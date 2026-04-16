//! Tests for TS2344: type argument constraint validation when the type
//! parameter's base constraint involves a self-referential interface.
//!
//! When `W extends string & Base` where `Base` is the enclosing interface,
//! and `Inner<W>` is used where `Inner<C extends Constraint>`, tsc correctly
//! emits TS2344 because `string & Base` does not satisfy `Constraint`.
//!
//! Previously, `contains_type_parameters` traversed into the method signatures
//! of the `Base` interface and found the bound type parameter `W` inside
//! `bar<W>(): Inner<W>`, incorrectly concluding that the base constraint
//! contained free type parameters and deferring the constraint check.
//!
//! Fix: Use `contains_free_type_parameters` which skips function/callable
//! bodies that have their own type parameters, avoiding false positives
//! from bound method type parameters.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Self-referential interface: `W extends string & Base` where Base is the
/// enclosing interface. `Inner<W>` should emit TS2344 because
/// `string & Base` does not satisfy `Constraint`.
#[test]
fn test_ts2344_self_referential_interface_constraint() {
    let source = r#"
type Constraint = { x: number };
type Inner<C extends Constraint> = C;

interface Base {
    bar<W extends string & Base>(): Inner<W>;
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();

    assert!(
        !ts2344_errors.is_empty(),
        "Expected TS2344 for 'W' not satisfying 'Constraint' when W extends self-referential interface.\nGot diagnostics: {diagnostics:#?}"
    );
}

/// Type alias wrapping a self-referential interface: `W extends Wrap`
/// where `type Wrap = string & Base`. Same issue.
#[test]
fn test_ts2344_type_alias_wrapping_self_referential_interface() {
    let source = r#"
type Constraint = { x: number };
type Inner<C extends Constraint> = C;

type Wrap = string & Base;
interface Base {
    bar<W extends Wrap>(): Inner<W>;
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();

    assert!(
        !ts2344_errors.is_empty(),
        "Expected TS2344 for 'W' not satisfying 'Constraint' through type alias wrapping self-ref interface.\nGot diagnostics: {diagnostics:#?}"
    );
}

/// Non-self-referential case should still work (was working before fix).
#[test]
fn test_ts2344_non_self_referential_still_works() {
    let source = r#"
type Constraint = { x: number };
type Inner<C extends Constraint> = C;

interface Other { z: number }
interface Base {
    bar<W extends string & Other>(): Inner<W>;
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();

    assert!(
        !ts2344_errors.is_empty(),
        "Expected TS2344 for 'W' not satisfying 'Constraint' (non-self-ref case).\nGot diagnostics: {diagnostics:#?}"
    );
}

/// When the constraint IS satisfied, no TS2344 should be emitted.
#[test]
fn test_no_ts2344_when_constraint_is_satisfied() {
    let source = r#"
type Constraint = { x: number };
type Inner<C extends Constraint> = C;

interface Base {
    bar<W extends { x: number; y: string }>(): Inner<W>;
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();

    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 when constraint is satisfied.\nGot TS2344 errors: {ts2344_errors:#?}"
    );
}
