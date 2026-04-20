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
/// TS1064 fires for async functions in JS files with `@type {function(): string}`.
/// When a variable in a JS file has a JSDoc `@type` annotation declaring a function
/// type with a non-Promise return type, and the initializer is async, tsc emits TS1064.
#[test]
fn test_ts1064_jsdoc_type_function_async() {
    use crate::parser::ParserState;

    let source = r#"
interface Promise<T> {}

/** @type {function(): string} */
const a = async () => 0

/** @type {function(): string} */
const b = async () => {
    return 0
}
"#;

    let mut parser = ParserState::new("file.js".to_string(), source.to_string());
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
        "file.js".to_string(),
        crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2017,
            allow_js: true,
            check_js: true,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts1064_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1064)
        .count();
    assert!(
        ts1064_count >= 2,
        "Expected at least 2 TS1064 errors for async functions with JSDoc @type {{function(): string}}, got {ts1064_count}. Diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_duplicate_class_members() {
    use crate::parser::ParserState;

    // Simplified test - just duplicate properties
    let source = r#"
class DuplicateProperties {
    x: number;
    x: string;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    println!("All diagnostics: {:?}", checker.ctx.diagnostics);

    // tsc emits TS2300 only on the second property (TS2717 is also emitted but not yet implemented)
    assert_eq!(
        codes.iter().filter(|&&c| c == 2300).count(),
        1,
        "Expected 1 TS2300 error for duplicate class members (on second property), got: {codes:?}"
    );
}

#[test]
fn test_duplicate_object_literal_properties() {
    use crate::parser::ParserState;

    // Test duplicate properties in object literal (TS1117 only fires for ES5 target)
    let source = r#"
const obj = {
    x: 1,
    x: 2,
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
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        target: tsz_common::common::ScriptTarget::ES5,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 1 TS1117 error for the duplicate 'x' property
    assert_eq!(
        codes.iter().filter(|&&c| c == 1117).count(),
        1,
        "Expected 1 TS1117 error for duplicate object literal properties, got: {codes:?}"
    );
}

#[test]
fn test_duplicate_object_literal_mixed_properties() {
    use crate::parser::ParserState;

    // Test duplicate properties with different syntax (shorthand, method)
    // TS1117 only fires for ES5 target
    let source = r#"
const obj1 = {
    x: 1,
    x: 2,  // duplicate
    y: 3,
};

const obj2 = {
    a: 1,
    a: 2,  // duplicate
    b: 3,
    c() { return 4; },
    c() { return 5; },  // duplicate method
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
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        target: tsz_common::common::ScriptTarget::ES5,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 3 TS1117 errors (x, a, c)
    assert_eq!(
        codes.iter().filter(|&&c| c == 1117).count(),
        3,
        "Expected 3 TS1117 errors for duplicate object literal properties, got: {codes:?}"
    );
}

#[test]
fn test_global_augmentation_tracks_interface_declarations() {
    // Test that interface declarations inside `declare global` are tracked as augmentations
    use crate::parser::ParserState;

    let source = r#"
export {};

declare global {
    interface Window {
        myCustomProperty: string;
    }
    interface CustomGlobal {
        value: number;
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

    // Verify that the binder tracked the global augmentations
    assert!(
        binder.global_augmentations.contains_key("Window"),
        "Expected 'Window' in global_augmentations, got: {:?}",
        binder.global_augmentations.keys().collect::<Vec<_>>()
    );
    assert!(
        binder.global_augmentations.contains_key("CustomGlobal"),
        "Expected 'CustomGlobal' in global_augmentations, got: {:?}",
        binder.global_augmentations.keys().collect::<Vec<_>>()
    );

    // Check the declarations count
    assert_eq!(
        binder
            .global_augmentations
            .get("Window")
            .map(std::vec::Vec::len),
        Some(1),
        "Expected 1 Window augmentation declaration"
    );
    assert_eq!(
        binder
            .global_augmentations
            .get("CustomGlobal")
            .map(std::vec::Vec::len),
        Some(1),
        "Expected 1 CustomGlobal augmentation declaration"
    );
}

#[test]
fn test_global_augmentation_interface_no_ts2304() {
    // Test that augmented interfaces inside `declare global` don't cause TS2304 errors
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
export {};

declare global {
    interface Window {
        myCustomProperty: string;
    }
}

// Access the augmented property via window (Window type)
const win: Window = {} as Window;
const prop = win.myCustomProperty;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should not have TS2304 (Cannot find name) for Window or myCustomProperty
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for global augmentation interface, got: {codes:?}"
    );
}

// ===== TS2564 Edge Case Tests (Worker 14) =====

/// Test that class expressions emit TS2564 for uninitialized properties
#[test]
fn test_ts2564_class_expression_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
const MyClass = class {
    value: number;  // Should emit TS2564
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

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        has_2564,
        "Expected TS2564 for class expression with uninitialized property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that class expressions with constructor assignments skip TS2564
#[test]
fn test_ts2564_class_expression_constructor_assignment() {
    use crate::parser::ParserState;

    let source = r#"
const MyClass = class {
    value: number;

    constructor() {
        this.value = 42;  // Properly initialized
    }
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

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        !has_2564,
        "Expected no TS2564 for class expression with initialized property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that named class expressions emit TS2564 for uninitialized properties
#[test]
fn test_ts2564_named_class_expression_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
const MyClass = class NamedClass {
    value: string;  // Should emit TS2564
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

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        has_2564,
        "Expected TS2564 for named class expression with uninitialized property, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that class expressions extending a base class emit TS2564
#[test]
fn test_ts2564_class_expression_derived_emits_error() {
    use crate::parser::ParserState;

    let source = r#"
class Base {
    baseValue: number = 0;
}

const Derived = class extends Base {
    derivedValue: string;  // Should emit TS2564
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

    let has_2564 = checker.ctx.diagnostics.iter().any(|d| d.code == 2564);
    assert!(
        has_2564,
        "Expected TS2564 for derived class expression with uninitialized property, got: {:?}",
        checker.ctx.diagnostics
    );
}

