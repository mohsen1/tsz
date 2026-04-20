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
fn test_class_extends_intersection_type_ts2339() {
    use crate::parser::ParserState;

    // Tests that classes extending intersection types can access properties from both sides
    let source = r#"
declare class C1 {
    a: number;
    constructor(s: string);
}

declare class M1 {
    p: number;
    constructor(...args: any[]);
}

declare const Mixed1: typeof M1 & typeof C1;

class C2 extends Mixed1 {
    constructor() {
        super("hello");
        this.a;
        this.p;
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
    // Previously a known limitation, now resolved: classes extending intersection types
    // of typeof declare classes correctly resolve properties from both sides.
    assert!(
        !codes.contains(&2339),
        "Class extending intersection type should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_abstract_mixin_intersection_ts2339() {
    use crate::parser::ParserState;

    // Tests that abstract mixin patterns with intersection types resolve properties
    // This requires fixing type parameter scope handling when computing parameter types
    // for heritage clauses in nested classes inside generic functions.
    let source = r#"
interface IMixin {
    mixinMethod(): void;
}

function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => IMixin) {
    abstract class MixinClass extends baseClass implements IMixin {
        mixinMethod() {}
    }
    return MixinClass;
}

class ConcreteBase {
    baseMethod() {}
}

class DerivedFromConcrete extends Mixin(ConcreteBase) {
}

const wasConcrete = new DerivedFromConcrete();
wasConcrete.baseMethod();
wasConcrete.mixinMethod();
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
    // Previously a known limitation, now resolved: abstract mixin patterns with
    // intersection return types correctly resolve properties on the derived class.
    assert!(
        !codes.contains(&2339),
        "Abstract mixin pattern should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_intersection_type_lowercase() {
    use crate::parser::ParserState;

    let source = r#"
interface Base {
    x: number;
    y: number;
}

interface BaseCtor {
    new (value: number): Base;
}

declare function getBase(): BaseCtor;

class Derived extends getBase() {
    constructor() {
        super(1);
        this.x = 1;
        this.y = 2;
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
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 for base constructor properties, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_incomplete_property_access_no_ts2339() {
    use crate::parser::ParserState;

    let source = r#"
class Foo {
    method() {
        this.
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().iter().any(|d| d.code == 1003),
        "Expected parse error TS1003 for missing identifier, got: {:?}",
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
        !codes.contains(&2339),
        "Should not emit TS2339 after parse errors, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_interface_extends_class_no_recursion_crash() {
    use crate::parser::ParserState;

    // Regression test for crash: interface extending a class with private fields
    // should not cause infinite recursion during type checking
    let source = r#"
class C {
    #prop;
    func(x: I) {
        x.#prop = 123;
    }
}
interface I extends C {}

function func(x: I) {
    x.#prop = 123;
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

    // This should not crash with stack overflow
    checker.check_source_file(root);

    // The test passes if we get here without crashing
    // (private field access across interface boundaries should produce errors, but no crash)
}

#[test]
fn test_no_implicit_returns_ts7030_function() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitReturns: true
function maybeReturn(x: boolean) {
    if (x) {
        return 42;
    }
    // Missing return when x is false - should trigger TS7030
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

    let ts7030_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7030)
        .collect();

    assert_eq!(
        ts7030_errors.len(),
        1,
        "Expected one TS7030 error, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_no_implicit_returns_disabled() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitReturns: false
function maybeReturn(x: boolean) {
    if (x) {
        return 42;
    }
    // Should not trigger TS7030 since noImplicitReturns is false
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

    let ts7030_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7030)
        .collect();

    assert!(
        ts7030_errors.is_empty(),
        "Expected no TS7030 errors, got: {ts7030_errors:?}"
    );
}

#[test]
fn test_no_implicit_returns_ts7030_method() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitReturns: true
class Example {
    maybeReturn(x: boolean) {
        if (x) {
            return "hello";
        }
        // Missing return when x is false - should trigger TS7030
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

    let ts7030_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7030)
        .collect();

    assert_eq!(
        ts7030_errors.len(),
        1,
        "Expected one TS7030 error for method, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_no_implicit_returns_ts7030_getter() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitReturns: true
class Example {
    private _value = 0;
    get value() {
        if (this._value > 0) {
            return this._value;
        }
        // Missing return when _value <= 0 - should trigger TS7030
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

    let ts7030_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7030)
        .collect();

    assert_eq!(
        ts7030_errors.len(),
        1,
        "Expected one TS7030 error for getter, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2695_comma_operator_side_effects() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
let a = 1;
let b = 2;
a, b;
1, b;
function aFn() {}
aFn(), b;
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

    let ts2695_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS
        })
        .collect();

    assert_eq!(
        ts2695_errors.len(),
        2,
        "Expected two TS2695 errors, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}
