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
/// TS Unsoundness #1: The "Any" Type - Everything is assignable to any
///
/// Any specific type is assignable to `any`. This is the escape hatch
/// that allows bypassing type checking.
#[test]
fn test_specific_types_assignable_to_any() {
    use crate::parser::ParserState;

    let source = r#"
declare let anyTarget: any;

// Everything is assignable to any
const str = "hello";
const num = 42;
const bool = true;
const obj = { x: 1 };
const fn = (x: string) => x.length;
const arr = [1, 2, 3];

anyTarget = str;
anyTarget = num;
anyTarget = bool;
anyTarget = obj;
anyTarget = fn;
anyTarget = arr;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Specific To Any Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // All specific types should be assignable to any (0 errors)
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "All types should be assignable to any: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any in function arguments
///
/// Any can be passed where a specific type is expected, and any function
/// can accept any as an argument.
#[test]
fn test_any_type_in_function_calls() {
    use crate::parser::ParserState;

    let source = r#"
declare const anyVal: any;

function expectString(s: string): void {}
function expectNumber(n: number): void {}
function expectObject(o: { x: number }): void {}

// Any can be passed where specific types are expected
expectString(anyVal);
expectNumber(anyVal);
expectObject(anyVal);
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Any In Function Calls Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Any should be valid in function calls expecting specific types
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Any should be valid in function calls: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any propagation in operations
///
/// Operations on any produce any, maintaining the escape hatch.
#[test]
fn test_any_type_propagation() {
    use crate::parser::ParserState;

    let source = r#"
declare const anyVal: any;

// Operations on any produce any
const propAccess = anyVal.foo;
const elemAccess = anyVal[0];
const call = anyVal();
const method = anyVal.bar();

// Results can be assigned to any specific type
const str: string = propAccess;
const num: number = elemAccess;
const obj: { x: number } = call;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Any Propagation Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Any should propagate through operations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Any should propagate through operations: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any does NOT bypass never
///
/// While any is both top and bottom, never is the true bottom.
/// Assigning never to any is allowed, but it doesn't mean anything
/// because never has no values.
#[test]
fn test_any_type_never_relationship() {
    use crate::parser::ParserState;

    let source = r#"
declare const neverVal: never;
declare let anyTarget: any;

// Never is assignable to any (but has no values)
anyTarget = neverVal;

// Any is NOT assignable to never (you can't produce a never value)
// This should produce an error
function returnNever(): never {
    throw "error";
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Any Never Relationship Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // never -> any is allowed, but we don't test any -> never here
    // as it requires implicit return checking
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Never should be assignable to any: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Fresh objects checked
///
/// Object literals ("fresh" objects) are subject to excess property checks.
/// This prevents typos and catches unintended extra properties.
#[test]
fn test_freshness_object_literal_excess_property() {
    use crate::parser::ParserState;

    let source = r#"
interface Config {
    host: string;
    port: number;
}

// Object literal (fresh) - excess property should be caught
const config: Config = {
    host: "localhost",
    port: 8080,
    extra: "not allowed"  // Error: excess property
};
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Freshness Object Literal Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Fresh object literal should have excess property error: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Variables not checked
///
/// Variables with excess properties are NOT subject to excess property checks.
/// This is the "stale" object behavior - width subtyping is allowed.
#[test]
fn test_freshness_variable_no_excess_check() {
    use crate::parser::ParserState;

    let source = r#"
interface Config {
    host: string;
    port: number;
}

// Variable assignment (not fresh) - no excess property check
const obj = {
    host: "localhost",
    port: 8080,
    extra: "allowed because not fresh"
};

// Assigning variable to typed binding - width subtyping allowed
const config: Config = obj;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Freshness Variable Assignment Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    // No excess property error for variable assignment
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Variable assignment should allow width subtyping: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Function argument
///
/// Fresh object literals passed as function arguments are checked for excess properties.
#[test]
fn test_freshness_function_argument_checked() {
    use crate::parser::ParserState;

    let source = r#"
interface Options {
    timeout: number;
}

function configure(opts: Options): void {}

// Fresh object literal in function call - excess property checked
configure({ timeout: 5000, retries: 3 });
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Freshness Function Argument Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Fresh object in function call should have excess property error: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Return statement
///
/// Fresh object literals in return statements are checked for excess properties.
#[test]
fn test_freshness_return_statement_checked() {
    use crate::parser::ParserState;

    let source = r#"
interface Result {
    value: number;
}

function getResult(): Result {
    return { value: 42, extra: "not allowed" };
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Freshness Return Statement Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Fresh object in return should have excess property error: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_union_optional_object_literal_excess_property() {
    use crate::parser::ParserState;

    let source = r#"
type U = { a?: number } | { b?: number };
const u: U = { a: 1, c: 2 };
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Union Optional Excess Property Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert!(
        !excess_errors.is_empty(),
        "Expected excess property error for union optional object literal: {:?}",
        checker.ctx.diagnostics
    );

    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Did not expect TS2322 for union optional excess property, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_union_optional_object_literal_no_common_property() {
    use crate::parser::ParserState;

    let source = r#"
type U = { a?: number } | { b?: number };
const u: U = { c: 1 };
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Union Optional No Common Property Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Expected excess property error for union optional object literal with no overlap: {:?}",
        checker.ctx.diagnostics
    );

    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Did not expect TS2322 for union optional no-common property, got: {:?}",
        checker.ctx.diagnostics
    );
}
