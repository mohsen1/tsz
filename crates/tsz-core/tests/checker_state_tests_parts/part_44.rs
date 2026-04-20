// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// Test that switch statements with default case pass TS2564 check
#[test]
fn test_ts2564_switch_with_default_passes() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    value: number;
    constructor(type: number) {
        switch (type) {
            case 0:
                this.value = 0;
                break;
            default:
                this.value = -1;
                break;
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because default case ensures initialization
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for switch with default, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that destructuring assignments to this.* are tracked
#[test]
fn test_ts2564_destructuring_assignment_passes() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    a: number;
    b: string;
    constructor(data: { a: number; b: string }) {
        ({ a: this.a, b: this.b } = data);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because properties are initialized via destructuring
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for destructuring assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that array destructuring assignments to this.* are tracked
#[test]
fn test_ts2564_array_destructuring_assignment_passes() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    a: number;
    b: string;
    constructor(data: [number, string]) {
        [this.a, this.b] = data;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because properties are initialized via array destructuring
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for array destructuring assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties assigned only in loop body emit TS2564
#[test]
fn test_ts2564_loop_assignment_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    value: number;
    constructor() {
        for (let i = 0; i < 10; i++) {
            this.value = i;
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 because loop might not execute
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for loop assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that properties assigned in do-while loop pass (executes at least once)
#[test]
fn test_ts2564_do_while_assignment_passes() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    value: number;
    constructor() {
        do {
            this.value = 1;
        } while (false);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 because do-while always executes at least once
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for do-while assignment, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that while loop with false condition doesn't count as definite assignment
#[test]
fn test_ts2564_while_loop_false_condition_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    value: number;
    constructor() {
        while (false) {
            this.value = 1;
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 because while loop might not execute
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for while loop with false condition, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that computed properties with identifier keys emit TS2564
#[test]
fn test_ts2564_computed_property_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
const key1 = "computedKey";
class Foo {
    [key1]: number;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have TS2564 for computed property without initialization
    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2564)
        .count();
    assert_eq!(
        count, 1,
        "Expected TS2564 for computed property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that computed properties initialized in constructor pass TS2564 check
#[test]
fn test_ts2564_computed_property_initialized_passes() {
    use crate::parser::ParserState;

    let source = r#"
const key2 = "initInConstructor";
class Foo {
    [key2]: number;
    constructor() {
        this[key2] = 42;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should NOT have TS2564 for property initialized in constructor
    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for initialized computed property, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_recursive_mapped_type_stack_guard() {
    use crate::parser::ParserState;

    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
type Obj = { a: number };
declare let foo: Circular<Obj>;
foo.a;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
}

#[test]
fn test_recursive_mapped_type_list_widget_guard() {
    use crate::parser::ParserState;

    let source = r#"
type NonOptionalKeys<T> = { [P in keyof T]: undefined extends T[P] ? never : P }[keyof T];
type Child<T> = { [P in NonOptionalKeys<T>]: T[P] };

interface ListWidget {
    "type": "list",
    "minimum_count": number,
    "maximum_count": number,
    "collapsable"?: boolean,
    "each": Child<ListWidget>;
}

type ListChild = Child<ListWidget>;

declare let x: ListChild;
x.type;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
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
}
