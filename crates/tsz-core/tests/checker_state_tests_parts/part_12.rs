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
#[test]
fn test_static_block_property_used_before_initialization_2729() {
    // Error 2729: Property used before initialization in static blocks
    // Static blocks referencing later-declared static properties via C.X or this.X
    use crate::parser::ParserState;

    let source = r#"
class C {
    static f1 = 1;
    static {
        console.log(C.f1, C.f2, C.f3)
    }
    static f2 = 2;
    static {
        console.log(C.f1, C.f2, C.f3)
    }
    static f3 = 3;
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

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // First static block: C.f2 (after) and C.f3 (after) → 2 errors
    // Second static block: C.f3 (after) → 1 error
    // Total: 3 TS2729 errors
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 3,
        "Expected 3 TS2729 errors for static block use-before-init, got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_static_block_this_access_2729() {
    // Error 2729: this.X in static block where X is declared after
    use crate::parser::ParserState;

    let source = r#"
class C {
    static s1 = 1;
    static {
        this.s1;
        C.s1;
        this.s2;
        C.s2;
    }
    static s2 = 2;
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

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // this.s2 and C.s2 are before s2's declaration → 2 errors
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 2,
        "Expected 2 TS2729 errors for this.s2 and C.s2 in static block, got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_static_block_no_error_for_arrow_function_2729() {
    // Accesses inside arrow functions in static blocks are deferred — no TS2729
    use crate::parser::ParserState;

    let source = r#"
class C {
    static {
        const fn = () => C.s1;
    }
    static s1 = 1;
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

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Arrow function defers the access — no TS2729
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 0,
        "Expected 0 TS2729 errors (arrow function defers access), got {count_2729} in: {codes:?}"
    );
}

#[test]
fn test_property_not_assignable_to_same_in_base_2416() {
    // Error 2416: Property 'num' in type 'WrongTypePropertyImpl' is not assignable
    // to the same property in base type 'WrongTypeProperty'.
    use crate::parser::ParserState;

    let source = r#"
abstract class WrongTypeProperty {
    abstract num: number;
}
class WrongTypePropertyImpl extends WrongTypeProperty {
    num = "nope, wrong";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Debug: Print parsed classes
    let arena = parser.get_arena();
    println!("Number of classes in arena: {}", arena.classes.len());
    for (i, class) in arena.classes.iter().enumerate() {
        println!(
            "Class {}: has heritage = {}",
            i,
            class.heritage_clauses.is_some()
        );
        if let Some(ref hc) = class.heritage_clauses {
            println!("  Heritage clause nodes: {}", hc.nodes.len());
        }
    }

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Debug: print file locals
    println!("File locals count: {}", binder.file_locals.len());

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have at least one 2416 error for the incompatible property type
    let count_2416 = codes.iter().filter(|&&c| c == 2416).count();
    assert!(
        count_2416 >= 1,
        "Expected at least 1 error 2416 for property not assignable to base, got {count_2416} in: {codes:?}"
    );
}

#[test]
fn test_property_not_assignable_to_generic_base_2416() {
    use crate::parser::ParserState;

    let source = r#"
abstract class Base<T> {
    abstract value: T;
}
class Derived extends Base<string> {
    value = 123;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2416),
        "Expected error 2416 for generic base property mismatch, got: {codes:?}"
    );
}

#[test]
fn test_non_abstract_class_missing_implementations_2654() {
    // Error 2654: Non-abstract class 'C' is missing implementations for
    // the following members of 'B': 'prop', 'm'.
    use crate::parser::ParserState;

    let source = r#"
abstract class B {
    abstract prop: number;
    abstract m(): void;
}
class C extends B {
    // Missing implementations for 'prop' and 'm'
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have error 2654 for missing abstract implementations
    let count_2654 = codes.iter().filter(|&&c| c == 2654).count();
    assert!(
        count_2654 >= 1,
        "Expected at least 1 error 2654 for missing abstract implementations, got {count_2654} in: {codes:?}"
    );

    // Check the message mentions the missing members
    let has_prop = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 2654 && d.message_text.contains("'prop'"));
    let has_m = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 2654 && d.message_text.contains("'m'"));
    assert!(has_prop, "Error 2654 should mention missing 'prop'");
    assert!(has_m, "Error 2654 should mention missing 'm'");
}

#[test]
fn test_readonly_property_assignment_2540() {
    // Error 2540: Cannot assign to 'ro' because it is a read-only property.
    use crate::parser::ParserState;

    let source = r#"
class C {
    readonly ro: string = "readonly please";
}
let c = new C();
c.ro = "error: lhs of assignment can't be readonly";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    println!("Diagnostics:");
    for diag in &checker.ctx.diagnostics {
        println!("  TS{}: {}", diag.code, diag.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have error 2540 for readonly property assignment
    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert!(
        count_2540 >= 1,
        "Expected at least 1 error 2540 for readonly property assignment, got {count_2540} in: {codes:?}"
    );
}

#[test]
fn test_readonly_element_access_assignment_2540() {
    // Error 2540: Cannot assign to 'name' because it is a read-only property.
    use crate::parser::ParserState;

    let source = r#"
interface Config {
    readonly name: string;
}
let config: Config = { name: "ok" };
config["name"] = "error";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert!(
        count_2540 >= 1,
        "Expected at least 1 error 2540 for readonly element access assignment, got {count_2540} in: {codes:?}"
    );
}

#[test]
fn test_readonly_array_element_assignment_2540() {
    // Error 2542: Index signature in type 'readonly number[]' only permits reading.
    use crate::parser::ParserState;

    let source = r#"
const xs: readonly number[] = [1, 2];
xs[0] = 3;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // TS2542 for readonly index signatures (tsc emits 2542, not 2540, for arrays)
    let count = codes.iter().filter(|&&c| c == 2542 || c == 2540).count();
    assert!(
        count >= 1,
        "Expected at least 1 error 2540/2542 for readonly array element assignment, got {count} in: {codes:?}"
    );
}

#[test]
fn test_readonly_method_signature_assignment_2540() {
    // Error 2540: Cannot assign to 'run' because it is a read-only property.
    use crate::parser::ParserState;

    let source = r#"
interface Service {
    readonly run(): void;
}
let svc: Service = { run() {} };
svc.run = () => {};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    let count_2540 = codes.iter().filter(|&&c| c == 2540).count();
    assert!(
        count_2540 >= 1,
        "Expected at least 1 error 2540 for readonly method signature assignment, got {count_2540} in: {codes:?}"
    );
}
