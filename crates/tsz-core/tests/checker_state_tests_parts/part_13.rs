#[test]
fn test_interface_extension_property_access_ts2339() {
    // Tests that accessing properties from extended interface doesn't produce TS2339
    let source = r#"
interface A { a: string; }
interface B extends A { b: number; }
function f(obj: B) {
    return obj.a;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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
    // Tests that accessing interface properties via typed parameter works
    // Note: 'implements' itself doesn't contribute to 'this' type lookup,
    // but a parameter typed as the interface should resolve properties
    let source = r#"
interface I { x: number; }
class C implements I { x: number = 0; }
function f(i: I) { return i.x; }
"#;

    let (parser, root) = parse_test_source(source);
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
    // Tests that multi-level interface extension properly resolves properties
    let source = r#"
interface A { a: string; }
interface B extends A { b: number; }
interface C extends B { c: boolean; }
function f(obj: C) {
    return obj.a + obj.b + obj.c;
}
"#;

    let (parser, root) = parse_test_source(source);
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
    // Tests that calling inherited methods doesn't produce TS2339
    let source = r#"
class Base {
    baseMethod(): number { return 42; }
}
class Derived extends Base {
    derivedMethod() { return this.baseMethod(); }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_class_extends_intersection_type_ts2339() {
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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
    let source = r#"
class Foo {
    method() {
        this.
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);

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
    let source = r#"
// @noImplicitReturns: true
function maybeReturn(x: boolean) {
    if (x) {
        return 42;
    }
    // Missing return when x is false - should trigger TS7030
}
"#;

    let (parser, root) = parse_test_source(source);
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
    checker.enable_source_file_test_pragmas();
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
    let source = r#"
// @noImplicitReturns: false
function maybeReturn(x: boolean) {
    if (x) {
        return 42;
    }
    // Should not trigger TS7030 since noImplicitReturns is false
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let (parser, root) = parse_test_source(source);
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
    checker.enable_source_file_test_pragmas();
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

    let (parser, root) = parse_test_source(source);
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
    checker.enable_source_file_test_pragmas();
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

    let source = r#"
let a = 1;
let b = 2;
a, b;
1, b;
function aFn() {}
aFn(), b;
"#;

    let (parser, root) = parse_test_source(source);
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

#[test]
fn test_ts2695_comma_operator_edge_cases() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare function eval(input: string): any;
let a = 1;
let b = 2;
const obj = { method() {} };

a + b, b;
!a, b;
a ? b : 3, b;
a!, b;
typeof a, b;
`template`, b;

void a, b;
(a as any), b;
(0, eval)("1");
(0, obj.method)();
(0, obj["method"])();
"#;

    let (parser, root) = parse_test_source(source);
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
        6,
        "Expected six TS2695 errors, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    // Note: other diagnostics (e.g. TS1100 for eval in strict mode) may also be emitted.
    // We only verify the TS2695 count above.
}

#[test]
fn test_variadic_tuple_rest_param_no_ts2769() {
    // Regression test for TS2769 false positives with variadic tuple rest parameters
    // https://github.com/microsoft/TypeScript/issues/...
    // For signature: foo<T extends unknown[]>(x: number, ...args: [...T, number]): T
    // Call foo(1, 2) should infer T = [], not emit TS2769
    let source = r#"
        declare function foo3<T extends unknown[]>(x: number, ...args: [...T, number]): T;

        // These should all be valid calls (no TS2769)
        foo3(1, 2);  // T = [], args = [2]
        foo3(1, 'hello', true, 2);  // T = ['hello', true], args = ['hello', true, 2]

        function test<U extends unknown[]>(u: U) {
            foo3(1, ...u, 'hi', 2);  // Should work with spread
        }
    "#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

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

    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 for variadic tuple rest parameters, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
    );
}

#[test]
fn test_variadic_tuple_optional_tail_inference_no_ts2769() {
    let source = r#"
        declare function ft3<T extends unknown[]>(t: [...T]): T;
        declare function f20<T extends unknown[] = []>(args: [...T, number?]): T;
        declare function f22<T extends unknown[] = []>(args: [...T, number]): T;
        declare function f22<T extends unknown[] = []>(args: [...T]): T;

        ft3(['hello', 42]);
        f20(["foo", "bar"]);
        f20(["foo", 42]);

        function f21<U extends string[]>(args: [...U, number?]) {
            f20(args);
            f20(["foo", "bar"]);
            f20(["foo", 42]);
        }

        function f23<U extends string[]>(args: [...U, number]) {
            f22(args);
            f22(["foo", "bar"]);
            f22(["foo", 42]);
        }
    "#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

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

    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 for optional variadic tuple tails, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
    );
}

#[test]
fn test_recursive_mapped_types_no_crash() {
    // Regression test for recursive mapped type stack overflow
    // Tests that simple recursive mapped types don't cause infinite loops or crashes
    let code = r#"
// Direct recursion
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

// Mutual recursion
type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
}

// Generic recursive mapped type
type Circular<T> = {[P in keyof T]: Circular<T>};
type tup = [number, number];

declare var x: Circular<tup>;
"#;

    let (parser, root) = parse_test_source(code);

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

    // Should complete without crashing or hanging
    checker.check_source_file(root);

    // May have errors, but should not crash
    // The recursion guard should prevent infinite loops
    // If we get here without panicking, the test passed
    let _ = checker.ctx.diagnostics.len();
}

#[test]
fn test_recursive_mapped_property_access_no_crash() {
    // Regression test for recursive mapped type property access
    let code = r#"
type Transform<T> = { [K in keyof T]: Transform<T[K]> };

interface Product {
    users: string[];
}

declare var product: Transform<Product>;
product.users;
"#;

    let (parser, root) = parse_test_source(code);

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

    // Should complete without crashing or hanging
    checker.check_source_file(root);

    // If we get here without panicking, the test passed
    let _ = checker.ctx.diagnostics.len();
}
#[test]
fn test_object_destructuring_assignability() {
    let source = r#"
let obj: { x: number, y: string } = { x: 10, y: "hello" };

// Should trigger TS2322: Type 'number' is not assignable to type 'string'
let { x, y }: { x: string, y: string } = obj;
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for object destructuring type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_array_destructuring_assignability() {
    let source = r#"
let arr: [number, string] = [10, "hello"];

// Should trigger TS2322: Type 'string' is not assignable to type 'number'
let [a, b]: [number, number] = arr;
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "[ARRAY] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("[ARRAY] TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for array destructuring type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_destructuring_with_default_values_assignability() {
    let source = r#"
let obj: { x?: number } = {};

// Should trigger TS2322: Type 'number' is not assignable to type 'string'
// (The default value type should be checked against the declared type)
let { x = 42 }: { x: string } = obj;
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "[DEFAULT] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("[DEFAULT] TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for destructuring default value type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_nested_destructuring_assignability() {
    let source = r#"
let obj: { a: { b: number } } = { a: { b: 10 } };

// Should trigger TS2322 for nested property mismatch
let { a: { b } }: { a: { b: string } } = obj;
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "[NESTED] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("[NESTED] TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for nested destructuring type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_destructuring_binding_element_default_value_mismatch() {
    let source = r#"
// The default value 42 (number) should trigger TS2322: Type 'number' is not assignable to type 'string'
let obj: { x?: string } = {};
let { x = 42 }: { x: string } = obj;
"#;

    let (parser, root) = parse_test_source(source);
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

    println!(
        "[BINDING_DEFAULT] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    // This should find TS2322 for the default value 42 (number) not being assignable to string
    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for binding element default value type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_binding_element_default_value_isolated_check() {
    let source = r#"
// The initializer {} is valid for { x?: number } (x is optional)
// But the default value "hello" (string) should NOT be assignable to number
// This should give TS2322: Type 'string' is not assignable to type 'number'
let { x = "hello" }: { x?: number } = {};
"#;

    let (parser, root) = parse_test_source(source);
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

    println!(
        "[ISOLATED_DEFAULT] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    // EXPECTED: TS2322 for "hello" (string) not assignable to number
    // This test may currently fail if default values in binding elements aren't being checked
    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for binding element default value 'hello' (string) not assignable to number, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

/// Test that recursive mapped types don't crash and circular type detection works
///
/// NOTE: Currently ignored - circular type alias detection in mapped types is not
/// fully implemented. The checker should detect circular type aliases and emit TS2456,
/// but this is not being detected correctly for recursive mapped types.
#[test]
fn test_recursive_mapped_type_no_crash_and_ts2456() {
    let source = r#"
// TS2456: Type alias 'DirectCircular' circularly references itself
type DirectCircular = DirectCircular;

// TS2456: Mutually circular type aliases
type MutualA = MutualB;
type MutualB = MutualA;

// Valid recursive mapped types (should NOT crash or error)
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
}

// Property access on recursive mapped type (should not crash)
type Box<T> = { value: T };
type RecursiveBox = { [K in keyof Box<RecursiveBox>]: Box<RecursiveBox>[K] };

function test(r: RecursiveBox) {
    return r.value; // Should not crash
}

// Circular mapped type from #27881
export type Circular<T> = {[P in keyof T]: Circular<T>};
type tup = [number, number, number, number];

function foo(arg: Circular<tup>): tup {
  return arg;
}

// Deep recursive mapped type from #29442
type DeepMap<T extends unknown[], R> = {
  [K in keyof T]: T[K] extends unknown[] ? DeepMap<T[K], R> : R;
};

type tpl = [string, [string, [string]]];
type t1 = DeepMap<tpl, number>;
"#;

    let (parser, root) = parse_test_source(source);
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

    // This should NOT crash even with recursive types
    checker.check_source_file(root);

    // Verify TS2456 is emitted for direct circular type alias
    let ts2456_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2456)
        .count();

    // We should have at least TS2456 errors for:
    // 1. DirectCircular
    // 2. MutualA
    // 3. MutualB
    // Note: Depending on implementation, we might get 2 (one per declaration) or 3
    assert!(
        ts2456_count >= 2,
        "Expected at least 2 TS2456 errors for circular type aliases, got {} - diagnostics: {:?}",
        ts2456_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_type_parameter_in_function_body_no_ts2304() {
    let source = r#"
function identity<T>(x: T): T {
    const y: T = x;
    return y;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        !codes.contains(&2304),
        "Should not report TS2304 for type parameter T in function body, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_static_private_field_access_no_ts2339() {
    // Regression test for static private field access
    // Previously failed with TS2339 because static private members were excluded from constructor type
    let source = r#"
class C {
    static #x = 123;
    static {
        console.log(C.#x);
    }
    foo() {
        return C.#x;
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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

    // Should have NO TS2339 errors for C.#x access
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 errors for static private field access, got {} - diagnostics: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_static_private_accessor_access_no_ts2339() {
    // Regression test for static private accessor access
    let source = r#"
class A {
    static get #prop() { return ""; }
    static set #prop(param: string) { }

    static get #roProp() { return ""; }

    constructor(name: string) {
        A.#prop = "";
        console.log(A.#prop);
        console.log(A.#roProp);
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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

    // Filter out TS2540 for read-only property assignment (expected error)
    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert_eq!(
        ts2339_errors.len(),
        0,
        "Expected no TS2339 errors for static private accessor access, got {} - TS2339 diagnostics: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_type_parameter_in_type_query() {
    let source = r#"
// Type parameters should be resolved in typeof type queries
function identity<T>(x: T): T {
    return x;
}

// typeof on type parameter should not error
type IdentityReturnType<T> = ReturnType<typeof identity<T>>;

// Type parameter in Extract with typeof
function extract<T>(x: Extract<T, typeof identity>): T {
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    // Check that we don't have TS2304 for type parameter names (T, etc.)
    let ts2304_for_type_params: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .filter(|d| d.message_text.contains("'T'") || d.message_text.contains("type parameter"))
        .map(|d| &d.message_text)
        .collect();

    assert!(
        ts2304_for_type_params.is_empty(),
        "Should not report TS2304 for type parameter T in type query. Found errors: {ts2304_for_type_params:?}"
    );
}

