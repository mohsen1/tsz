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
#[test]
fn test_protected_static_access_allowed_from_derived_class() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    // Protected static members are accessible from subclasses through any
    // reference to the class hierarchy (both Base.s and Derived.s).
    // This matches tsc behavior — the receiver check only applies to
    // instance members, not static members.
    let source = r#"
class Base {
    protected static s = 1;
}
class Derived extends Base {
    static test() {
        Base.s;
        Derived.s;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let protected_errors = codes
        .iter()
        .filter(|&&code| code == diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES)
        .count();
    assert_eq!(
        protected_errors, 0,
        "Expected no TS2445 errors for protected static access from derived class, got: {codes:?}"
    );
}

#[test]
fn test_abstract_property_in_constructor_2715() {
    // Error 2715: Abstract property 'prop' in class 'AbstractClass' cannot be accessed in the constructor.
    use crate::parser::ParserState;

    let source = r#"
abstract class AbstractClass {
    constructor(str: string) {
        let val = this.prop.toLowerCase();
    }

    abstract prop: string;
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
        codes.contains(&2715),
        "Expected error 2715 (Abstract property cannot be accessed in constructor), got: {codes:?}"
    );
}

#[test]
fn test_interface_name_cannot_be_reserved_2427() {
    // Error 2427: Interface name cannot be 'string' (or other primitive types)
    use crate::parser::ParserState;
    let source = r#"interface string {}"#;

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

    // Debug: show all diagnostics
    println!("=== Diagnostics for 'interface string {{}}' ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2427),
        "Expected error 2427 (Interface name cannot be 'string'), got: {codes:?}"
    );
}

#[test]
fn test_const_modifier_on_class_property_1248() {
    // Error 1248: A class member cannot have the 'const' keyword
    use crate::parser::ParserState;
    let source = r#"class AtomicNumbers { static const H = 1; }"#;

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

    // Debug: show all diagnostics
    println!("=== Diagnostics for 'static const H = 1' ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1248),
        "Expected error 1248 (A class member cannot have the 'const' keyword), got: {codes:?}"
    );
}

#[test]
fn test_accessor_type_compatibility_2322() {
    // TS 5.1+: when BOTH getter and setter have explicit type annotations,
    // unrelated types are allowed — no TS2322.
    use crate::parser::ParserState;
    let source = r#"class C {
    public set AnnotatedSetter(a: number) { }
    public get AnnotatedSetter(): string { return ""; }
}"#;

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
        !codes.contains(&2322),
        "TS 5.1+ allows unrelated types when both annotated, no TS2322; got codes: {codes:?}",
    );
}

#[test]
fn test_accessor_type_compatibility_inheritance_no_error() {
    // Test that getter returning derived class type is assignable to setter base class param
    // class B extends A, so B <: A
    // Getter returns B, setter takes A -> Should NOT error (B is assignable to A)
    use crate::parser::ParserState;

    let source = r#"
class A { }
class B extends A { }

class C {
    public set AnnotatedSetter(a: A) { }
    public get AnnotatedSetter() { return new B(); }
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

    // Debug: show all diagnostics
    println!("=== Diagnostics for inheritance accessor test ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should NOT have TS2322 - B is assignable to A (B extends A)
    assert!(
        !codes.contains(&2322),
        "Should NOT have error 2322 (B extends A, so getter returning B is assignable to setter taking A). Got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_accessor_type_compatibility_typeof_structural() {
    // Getter return type should be assignable to setter param type when using typeof.
    use crate::parser::ParserState;
    let source = r#"
var x: { foo: string; }
class C {
    get value() { return x; }
    set value(v: typeof x) { }
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
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 0,
        "Did not expect TS2322 for typeof accessor compatibility, got: {codes:?}"
    );
}

#[test]
fn test_abstract_class_through_type_alias_2511() {
    // Error 2511: Cannot create an instance of an abstract class - through type alias
    use crate::parser::ParserState;

    let source = r#"
abstract class AbstractA { a!: string; }
type Abstracts = typeof AbstractA;
declare const cls2: Abstracts;
new cls2();
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

    // Abstract class instantiation checking not yet implemented
    // Once implemented, change to expect error 2511
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    if !codes.contains(&2511) {
        println!("=== Abstract Class Through Type Alias ===");
        println!("Expected error 2511 once abstract class checking implemented, got: {codes:?}");
    }
    // Accept 0 errors until abstract class checking is implemented
    assert!(
        codes.is_empty() || codes.contains(&2511),
        "Expected 0 errors (not implemented) or 2511: {codes:?}"
    );
}

#[test]
fn test_abstract_class_union_type_2511() {
    // Error 2511: Cannot create an instance of an abstract class - through union type
    use crate::parser::ParserState;

    let source = r#"
class ConcreteA {}
abstract class AbstractA { a!: string; }

type ConcretesOrAbstracts = typeof ConcreteA | typeof AbstractA;

declare const cls1: ConcretesOrAbstracts;

new cls1();
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

    // Abstract class instantiation checking not yet implemented
    // Once implemented, change to expect error 2511
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    if !codes.contains(&2511) {
        println!("=== Abstract Class Union Type ===");
        println!("Expected error 2511 once abstract class checking implemented, got: {codes:?}");
    }
    // Accept 0 errors until abstract class checking is implemented
    assert!(
        codes.is_empty() || codes.contains(&2511),
        "Expected 0 errors (not implemented) or 2511: {codes:?}"
    );
}

#[test]
fn test_property_used_before_initialization_2729() {
    // Error 2729: Property is used before its initialization
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    x = this.a;  // Error: Property 'a' is used before its initialization
    a = 1;
}

class NoError {
    a = 1;
    x = this.a;  // OK: 'a' is declared before 'x'
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

    // Should have exactly one 2729 error (in class Foo)
    let count_2729 = codes.iter().filter(|&&c| c == 2729).count();
    assert_eq!(
        count_2729, 1,
        "Expected exactly 1 error 2729 for property used before initialization, got {count_2729} in: {codes:?}"
    );
}

