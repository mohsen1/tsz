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
fn test_mixin_inheritance_property_access() {
    use crate::parser::ParserState;

    // This test is related to test_abstract_mixin_intersection_ts2339 and requires
    // fixing type parameter scope handling for nested classes in generic functions.
    let source = r#"
interface Mixin {
    mixinMethod(): void;
}

function Mixin<TBaseClass extends abstract new (...args: any) => any>(
    baseClass: TBaseClass
): TBaseClass & (abstract new (...args: any) => Mixin) {
    abstract class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}

class Base {
    baseMethod() {}
}

class Derived extends Mixin(Base) {}

const d = new Derived();
d.baseMethod();
d.mixinMethod();
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
    // Previously a known limitation, now resolved: mixin-based inheritance correctly
    // resolves intersection types, so no TS2339 is emitted.
    assert!(
        !codes.contains(&2339),
        "Mixin-based inheritance should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_mixin_return_type_preserves_base_properties() {
    use crate::parser::ParserState;

    let source = r#"
type Constructor<T> = new (...args: any[]) => T;

class Base {
    constructor(public x: number, public y: number) {}
}

const Printable = <T extends Constructor<Base>>(superClass: T) => class extends superClass {
    static message = "hello";
    print() {
        this.x;
    }
}

function Tagged<T extends Constructor<{}>>(superClass: T) {
    class C extends superClass {
        _tag: string;
        constructor(...args: any[]) {
            super(...args);
            this._tag = "hello";
        }
    }
    return C;
}

const Thing2 = Tagged(Printable(Base));
Thing2.message;

function f() {
    const thing = new Thing2(1, 2);
    thing.x;
    thing._tag;
    thing.print();
}

class Thing3 extends Thing2 {
    test() {
        this.print();
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
    // Previously a known limitation, now resolved: mixin constructor/instance property
    // resolution through generic class expressions works correctly.
    assert!(
        !codes.contains(&2339),
        "Mixin constructor/instance properties should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_class_extends_class_like_constructor_properties() {
    use crate::parser::ParserState;

    let source = r#"
interface Base<T, U> {
    x: T;
    y: U;
}

interface BaseConstructor {
    new (x: string, y: string): Base<string, string>;
    new <T>(x: T): Base<T, T>;
    new <T, U>(x: T, y: U): Base<T, U>;
}

declare function getBase(): BaseConstructor;

class D1 extends getBase() {
    constructor() {
        super("abc", "def");
        this.x;
        this.y;
    }
}

class D2 extends getBase() <number> {
    constructor() {
        super(10);
        super(10, 20);
        this.x;
        this.y;
    }
}

class D3 extends getBase() <string, number> {
    constructor() {
        super("abc", 42);
        this.x;
        this.y;
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
        "Should not emit TS2339 for class-like constructor inheritance, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_interface_extension_property_access_ts2339() {
    use crate::parser::ParserState;

    // Tests that accessing properties from extended interface doesn't produce TS2339
    let source = r#"
interface A { a: string; }
interface B extends A { b: number; }
function f(obj: B) {
    return obj.a;
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
        "Should not emit TS2339 for extended interface property, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_multi_level_inheritance_ts2339() {
    use crate::parser::ParserState;

    // Tests that multi-level class inheritance properly resolves properties
    let source = r#"
class A {
    a: number = 1;
}
class B extends A {
    b: number = 2;
}
class C extends B {
    m() { return this.a + this.b; }
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
        "Should not emit TS2339 for multi-level inherited properties, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_implements_clause_resolution_ts2339() {
    use crate::parser::ParserState;

    // Tests that accessing interface properties via typed parameter works
    // Note: 'implements' itself doesn't contribute to 'this' type lookup,
    // but a parameter typed as the interface should resolve properties
    let source = r#"
interface I { x: number; }
class C implements I { x: number = 0; }
function f(i: I) { return i.x; }
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
        "Should not emit TS2339 for interface property access, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_multi_level_interface_extension_ts2339() {
    use crate::parser::ParserState;

    // Tests that multi-level interface extension properly resolves properties
    let source = r#"
interface A { a: string; }
interface B extends A { b: number; }
interface C extends B { c: boolean; }
function f(obj: C) {
    return obj.a + obj.b + obj.c;
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
        "Should not emit TS2339 for multi-level interface extension, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_inherited_method_call_ts2339() {
    use crate::parser::ParserState;

    // Tests that calling inherited methods doesn't produce TS2339
    let source = r#"
class Base {
    baseMethod(): number { return 42; }
}
class Derived extends Base {
    derivedMethod() { return this.baseMethod(); }
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
        "Should not emit TS2339 for inherited method call, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_intersection_type_typeof_declare_classes_ts2339() {
    use crate::parser::ParserState;

    // Tests that property access works on intersection types of declare class constructors
    // Regression test for: typeof M1 & typeof C1 should resolve properties from both sides
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

function f() {
    let x = new Mixed1("hello");
    x.a;
    x.p;
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
    // Previously a known limitation, now resolved: intersection types of typeof declare
    // classes correctly resolve instance properties from both sides.
    assert!(
        !codes.contains(&2339),
        "Intersection type property access should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_intersection_type_three_way_constructor_ts2339() {
    use crate::parser::ParserState;

    // Tests that three-way intersection types work correctly
    let source = r#"
declare class C1 {
    a: number;
    constructor(s: string);
}

declare class M1 {
    p: number;
    constructor(...args: any[]);
}

declare class M2 {
    f(): number;
    constructor(...args: any[]);
}

declare const Mixed3: typeof M2 & typeof M1 & typeof C1;

function f() {
    let x = new Mixed3("hello");
    x.a;
    x.p;
    x.f();
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
    // Previously a known limitation, now resolved: three-way intersection types of typeof
    // declare classes correctly resolve instance properties.
    assert!(
        !codes.contains(&2339),
        "Three-way intersection type property access should now resolve correctly with no TS2339, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

