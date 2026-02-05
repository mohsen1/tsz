// Tests for control flow validation (TS1104, TS1105)
// TS1104: A 'continue' statement can only be used within an enclosing iteration statement
// TS1105: A 'break' statement can only be used within an enclosing iteration statement

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

#[test]
fn test_break_at_top_level_emits_ts1105() {
    // break at top level should emit TS1105
    let source = "break;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        checker.ctx.diagnostics.iter().any(|d| d.code == 1105),
        "Should emit TS1105 for break at top level, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_continue_at_top_level_emits_ts1104() {
    // continue at top level should emit TS1104
    let source = "continue;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        checker.ctx.diagnostics.iter().any(|d| d.code == 1104),
        "Should emit TS1104 for continue at top level, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_break_in_switch_is_valid() {
    // break inside switch should NOT emit error
    let source = r#"
switch (0) {
    case 0:
        break;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        !checker.ctx.diagnostics.iter().any(|d| d.code == 1105),
        "Should NOT emit TS1105 for break in switch, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_continue_in_switch_emits_ts1104() {
    // continue inside switch (without loop) should emit TS1104
    let source = r#"
switch (0) {
    default:
        continue;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        checker.ctx.diagnostics.iter().any(|d| d.code == 1104),
        "Should emit TS1104 for continue in switch without loop, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_break_in_while_loop_is_valid() {
    // break inside while loop should NOT emit error
    let source = r#"
while (true) {
    break;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        !checker.ctx.diagnostics.iter().any(|d| d.code == 1105),
        "Should NOT emit TS1105 for break in while loop, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_continue_in_for_loop_is_valid() {
    // continue inside for loop should NOT emit error
    let source = r#"
for (let i = 0; i < 10; i++) {
    continue;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        !checker.ctx.diagnostics.iter().any(|d| d.code == 1104),
        "Should NOT emit TS1104 for continue in for loop, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_break_in_for_of_loop_is_valid() {
    // break inside for-of loop should NOT emit error
    let source = r#"
const arr = [1, 2, 3];
for (const x of arr) {
    break;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        !checker.ctx.diagnostics.iter().any(|d| d.code == 1105),
        "Should NOT emit TS1105 for break in for-of loop, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_continue_in_nested_switch_inside_loop_is_valid() {
    // continue inside a switch that's inside a loop should be valid
    let source = r#"
while (true) {
    switch (0) {
        default:
            continue;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        !checker.ctx.diagnostics.iter().any(|d| d.code == 1104),
        "Should NOT emit TS1104 for continue in switch inside loop, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_break_in_function_inside_loop_emits_ts1107() {
    // break inside a function inside a loop should emit TS1107
    // "Jump target cannot cross function boundary" - there IS an outer loop,
    // but the function boundary blocks access to it
    let source = r#"
while (true) {
    function f() {
        break;  // Error: TS1107 - jump target cannot cross function boundary
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        checker.ctx.diagnostics.iter().any(|d| d.code == 1107),
        "Should emit TS1107 for break in function inside loop, got: {:?}",
        checker.ctx.diagnostics
    );
}
