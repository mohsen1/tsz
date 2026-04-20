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
fn test_extends_null_no_2304() {
    use crate::parser::ParserState;

    let source = r#"
class C extends null {}
class D extends (null) {}
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
        !codes.contains(&2304),
        "Unexpected TS2304 for extends null, got: {codes:?}"
    );
}

#[test]
fn test_decorator_invalid_declarations_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
declare function dec<T>(target: T): T;

@dec
enum E {}

@dec
interface I {}

@dec
namespace M {}

@dec
type T = number;

@dec
var x: number;
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
        !codes.contains(&2304),
        "Unexpected TS2304 for invalid decorator declarations, got: {codes:?}"
    );
}

#[test]
fn test_abstract_class_in_local_scope_2511() {
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    // Test case from tests/cases/compiler/abstractClassInLocalScopeIsAbstract.ts
    // Abstract class declared inside an IIFE should still error on instantiation
    let code = r#"
        (() => {
            abstract class A {}
            class B extends A {}
            new A();
            new B();
        })()
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Debug: Check symbols
    let symbols = binder.get_symbols();
    println!("=== Symbols ===");
    for i in 0..symbols.len() {
        if let Some(sym) = symbols.get(crate::binder::SymbolId(i as u32)) {
            println!(
                "  {:?}: {} flags={:#x} abstract={}",
                sym.id,
                sym.escaped_name,
                sym.flags,
                sym.flags & symbol_flags::ABSTRACT != 0
            );
        }
    }

    // Also try manually checking new expression
    println!("=== Class name lookup test ===");
    if let Some(sym_id) = binder.get_symbols().find_by_name("A") {
        println!("Found symbol A: {sym_id:?}");
        if let Some(symbol) = binder.get_symbol(sym_id) {
            println!(
                "  flags={:#x} abstract={}",
                symbol.flags,
                symbol.flags & symbol_flags::ABSTRACT != 0
            );
        }
    } else {
        println!("Symbol A not found!");
    }

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

    // Debug: Check diagnostics
    println!("=== Diagnostics ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    // Should have error 2511 for `new A()` but not for `new B()`
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2511),
        "Expected error 2511 for abstract class instantiation in local scope, got: {codes:?}"
    );

    // Should only have one 2511 error (for A, not B)
    let count_2511 = codes.iter().filter(|&&c| c == 2511).count();
    assert_eq!(
        count_2511, 1,
        "Expected exactly 1 error 2511 (for abstract class A only), got {count_2511} from: {codes:?}"
    );

    // Should NOT have error 2304 (Cannot find name) - both A and B should be found
    let count_2304 = codes.iter().filter(|&&c| c == 2304).count();
    assert_eq!(
        count_2304, 0,
        "Should NOT have 'Cannot find name' error (2304) for classes in local scope, got {count_2304} from: {codes:?}"
    );
}

#[test]
fn test_static_member_suggestion_2662() {
    // Error 2662: Cannot find name 'foo'. Did you mean the static member 'C.foo'?
    use crate::parser::ParserState;
    let source = r#"
class C {
    static foo: string;

    bar() {
        let k = foo;
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

    // Debug: show all diagnostics
    println!("=== Diagnostics for static member suggestion ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2662),
        "Expected error 2662 (Cannot find name 'foo'. Did you mean the static member 'C.foo'?), got: {codes:?}"
    );

    // Should NOT have generic "cannot find name" error 2304
    assert!(
        !codes.contains(&2304),
        "Should not have generic error 2304, should have specific 2662 instead. Got: {codes:?}"
    );
}

#[test]
fn test_static_member_suggestion_2662_assignment_target() {
    // Error 2662: Cannot find name 's'. Did you mean the static member 'C.s'?
    // This tests the case where a static member is used as an assignment target,
    // which goes through get_type_of_assignment_target instead of get_type_of_identifier.
    use crate::parser::ParserState;
    let source = r#"
class C {
    static s: any;

    constructor() {
        s = 1;
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

    // Debug: show all diagnostics
    println!("=== Diagnostics for static member assignment target ===");
    for d in &checker.ctx.diagnostics {
        println!("  code={}, msg={}", d.code, d.message_text);
    }

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2662),
        "Expected error 2662 (Cannot find name 's'. Did you mean the static member 'C.s'?) for assignment target, got: {codes:?}"
    );

    // Should NOT have generic "cannot find name" error 2304
    assert!(
        !codes.contains(&2304),
        "Should not have generic error 2304, should have specific 2662 instead. Got: {codes:?}"
    );
}

#[test]
fn test_class_static_side_property_assignability() {
    use crate::parser::ParserState;

    let source = r#"
class A {
    static foo: number;
}
class B {}
let ctor: typeof A = B;
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
    // Accept either 2741 (property missing) or 2322 (type not assignable)
    // Both correctly indicate the assignment is rejected due to missing static member
    assert!(
        codes.contains(&2741) || codes.contains(&2322),
        "Expected error 2741 or 2322 for missing static member on constructor type, got: {codes:?}"
    );
}

#[test]
fn test_private_member_nominal_class_assignability() {
    use crate::parser::ParserState;

    let source = r#"
class A {
    private x: number;
}
class B {
    private x: number;
}
const a: A = new B();
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
    // Accept either 2741 (property missing - TypeScript's preferred message) or 2322 (type not assignable)
    // Both indicate the assignment is correctly rejected due to private member nominality
    assert!(
        codes.contains(&2741) || codes.contains(&2322),
        "Expected error 2741 or 2322 for private member nominal mismatch, got: {codes:?}"
    );
}

#[test]
fn test_private_protected_property_access_errors() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    private x = 1;
    protected y = 2;
}
const f = new Foo();
f.x;
f.y;
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
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS),
        "Expected error 2341 for private property access, got: {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES),
        "Expected error 2445 for protected property access, got: {codes:?}"
    );
}

#[test]
fn test_private_protected_property_access_ok() {
    use crate::parser::ParserState;

    let source = r#"
class Base {
    protected z = 3;
}
class Derived extends Base {
    test() { return this.z; }
}
class Baz {
    private w = 4;
    getW() { return this.w; }
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that protected access requires derived instance
///
/// NOTE: Currently ignored - protected access control is not fully implemented.
/// The checker emits duplicate TS2445 errors for protected member access.
#[test]
fn test_protected_access_requires_derived_instance() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    // In tsc, accessing a protected member on a base-class-typed reference from
    // a derived class is TS2446 ("Property 'y' is protected and only accessible
    // through an instance of class 'Derived'. This is an instance of class 'Base'."),
    // not TS2445 ("only accessible within class and its subclasses").
    let source = r#"
class Base {
    protected y = 2;
}
class Derived extends Base {
    test(b: Base, d: Derived) {
        b.y;
        d.y;
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
        .filter(|&&code| code == diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A)
        .count();
    assert_eq!(
        protected_errors, 1,
        "Expected one error 2446 for protected access on base instance from derived, got: {codes:?}"
    );
}
