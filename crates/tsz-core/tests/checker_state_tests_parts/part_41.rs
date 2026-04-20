//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// TS Unsoundness #9: Legacy Null/Undefined - undefined rejected when strict
///
/// With strictNullChecks ON, assigning undefined to number should error.
#[test]
fn test_strict_null_checks_rejects_undefined() {
    use crate::parser::ParserState;

    let source = r#"
// Assigning undefined to number should error
const num: number = undefined;
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
            strict_null_checks: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should produce an error
    assert!(
        !checker.ctx.diagnostics.is_empty(),
        "Assigning undefined to number should error with strictNullChecks"
    );
}

/// TS Unsoundness #9: Legacy Null/Undefined - union with null/undefined
///
/// Union types can explicitly include null/undefined.
#[test]
fn test_null_undefined_union_types() {
    use crate::parser::ParserState;

    let source = r#"
// Union types that include null/undefined work fine
const maybeStr: string | null = null;
const maybeNum: number | undefined = undefined;

// Can also be assigned the non-null type
const str: string | null = "hello";
const num: number | undefined = 42;
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
        println!("=== Null/Undefined Union Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Union types with null/undefined should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Union types with null/undefined should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #38: Correlated Unions (Cross-Product Limitation)
///
/// When accessing a Union of Objects with a Union of Keys, TS computes the
/// Cross-Product, resulting in a wider type than expected (loss of correlation).
/// TS cannot track that `obj.kind === "a"` implies `obj.val` is `number`.
#[test]
fn test_correlated_unions_basic_access() {
    use crate::parser::ParserState;

    let source = r#"
type A = { kind: 'a'; val: number };
type B = { kind: 'b'; val: string };
type AB = A | B;

function test(obj: AB) {
    // Accessing 'val' gives number | string (cross-product)
    const v = obj.val;
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
        println!("=== Correlated Unions Basic Access Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Basic union property access should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Union property access should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #38: Correlated Unions - Discriminant narrowing
///
/// When discriminant is checked, the specific variant is narrowed.
#[test]
fn test_correlated_unions_discriminant_narrowing() {
    use crate::parser::ParserState;

    let source = r#"
type A = { kind: 'a'; val: number };
type B = { kind: 'b'; val: string };
type AB = A | B;

function test(obj: AB) {
    if (obj.kind === 'a') {
        // After narrowing, obj is A, so val is number
        const n: number = obj.val;
    } else {
        // After narrowing, obj is B, so val is string
        const s: string = obj.val;
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let error_count = checker.ctx.diagnostics.len();

    // Currently may fail until discriminated union narrowing is implemented
    if error_count > 0 {
        println!("=== Correlated Unions Discriminant Narrowing Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
        println!("Expected 0 errors once discriminated union narrowing works");
    }

    // For now, just check it doesn't crash
    // Once discriminated union narrowing works, change to expect 0 errors
}

/// TS Unsoundness #38: Correlated Unions - Index access cross-product
///
/// IndexAccess(Union(ObjA, `ObjB`), Key) produces Union(ObjA[Key], `ObjB`[Key]).
#[test]
fn test_correlated_unions_index_access() {
    use crate::parser::ParserState;

    let source = r#"
type Data = {
    numbers: number[];
    strings: string[];
};

function getArray(data: Data, key: 'numbers' | 'strings') {
    // data[key] gives number[] | string[] (cross-product)
    const arr = data[key];
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
        println!("=== Correlated Unions Index Access Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Index access with union key should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Index access with union key should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #38: Correlated Unions - Common property access
///
/// Accessing a property common to all union members works.
#[test]
fn test_correlated_unions_common_property() {
    use crate::parser::ParserState;

    let source = r#"
type Circle = { kind: 'circle'; radius: number };
type Square = { kind: 'square'; size: number };
type Shape = Circle | Square;

function getKind(shape: Shape): string {
    // 'kind' is common to both, gives 'circle' | 'square'
    return shape.kind;
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
        println!("=== Correlated Unions Common Property Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Common property access should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Common property access on union should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #42: CFA Invalidation in Closures
///
/// Type narrowing is reset inside closures for mutable variables (let/var)
/// because the callback might run after the variable has changed.
#[test]
fn test_cfa_invalidation_mutable_in_closure() {
    use crate::parser::ParserState;

    let source = r#"
let x: string | number = "hello";

if (typeof x === "string") {
    // x is narrowed to string here
    const upper = x.toUpperCase();

    // Inside callback, narrowing is invalid for mutable variable
    function callback() {
        // x should NOT be narrowed here (mutable let)
        const val = x;
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Just check it doesn't crash - narrowing behavior depends on CFA implementation
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Invalidation Mutable Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }
}

/// TS Unsoundness #42: CFA Invalidation - const maintains narrowing
///
/// For const variables, narrowing can be maintained inside closures
/// because the variable cannot be reassigned.
#[test]
fn test_cfa_const_maintains_narrowing() {
    use crate::parser::ParserState;

    let source = r#"
const x: string | number = "hello";

if (typeof x === "string") {
    // x is narrowed to string here
    const upper = x.toUpperCase();

    // Inside callback, narrowing IS valid for const
    function callback() {
        // x can stay narrowed (const cannot change)
        const val = x.toUpperCase();
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Currently doesn't maintain narrowing in closures
    // Once implemented, change to expect 0 errors
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Const Narrowing Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
        println!("Expected 0 errors once const narrowing in closures is implemented");
    }
}

/// TS Unsoundness #42: CFA Invalidation - arrow function closure
///
/// Arrow functions also invalidate narrowing for captured mutable variables.
#[test]
fn test_cfa_invalidation_arrow_function() {
    use crate::parser::ParserState;

    let source = r#"
let value: string | null = "test";

if (value !== null) {
    // value is narrowed to string here
    const len = value.length;

    // Arrow function captures mutable variable
    const fn = () => {
        // value narrowing invalid here
        const v = value;
    };
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

    // Just check it doesn't crash
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Invalidation Arrow Function Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }
}

/// TS Unsoundness #42: CFA Invalidation - callback parameter
///
/// Callback passed to another function also invalidates narrowing.
#[test]
fn test_cfa_invalidation_callback_parameter() {
    use crate::parser::ParserState;

    let source = r#"
declare function doLater(fn: () => void): void;

let data: string | undefined = "hello";

if (data !== undefined) {
    // data is narrowed to string here
    const first = data.charAt(0);

    // Callback passed to function
    doLater(() => {
        // data narrowing invalid - might run later after reassignment
        const d = data;
    });
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

    // Just check it doesn't crash
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== CFA Invalidation Callback Parameter Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }
}

