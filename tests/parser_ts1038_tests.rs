// Tests for TS1038: 'declare' modifier cannot be used in an already ambient context
// TypeScript emits TS1038 for redundant 'declare' inside ambient contexts

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

#[test]
fn test_declare_inside_declare_namespace_emits_error() {
    // TypeScript EMITS TS1038 for this - redundant 'declare' is NOT allowed
    let source = r#"
declare namespace chrome {
    declare var tabId: number;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Parser phase doesn't emit this error - it's a checker error
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parser should not emit errors for this - TS1038 is a checker error"
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.iter().any(|d| d.code == 1038),
        "Checker should emit TS1038 for declare inside declare namespace, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_declare_inside_regular_namespace_no_error() {
    // TypeScript does NOT emit TS1038 for this - regular namespace is not ambient
    let source = r#"
namespace M {
    declare module 'nope' { }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Parser should not emit errors"
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        !checker.ctx.diagnostics.iter().any(|d| d.code == 1038),
        "Checker should NOT emit TS1038 for declare inside regular namespace, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_declare_function_inside_declare_namespace() {
    // TypeScript EMITS TS1038 for this
    let source = r#"
declare namespace M {
    declare function F(): void;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parser should not emit errors"
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.iter().any(|d| d.code == 1038),
        "Checker should emit TS1038 for declare function inside declare namespace, got: {:?}",
        checker.ctx.diagnostics
    );
}
