#[test]
fn test_optional_chain_discriminant_narrows_union() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare const o: { x: 1, y: string } | { x: 2, y: number } | undefined;
if (o?.x === 1) {
    const x: 1 = o.x;
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
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Unexpected TS2322 for optional-chain discriminant narrowing, got: {codes:?}"
    );
}

#[test]
fn test_class_inheritance_property_access() {
    // Tests that accessing inherited instance properties doesn't produce TS2339
    let source = r#"
class Base {
    baseProp: number = 1;
}
class Derived extends Base {
    method() { return this.baseProp; }
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
        "Should not emit TS2339 for inherited class property, got errors: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_mixin_inheritance_property_access() {
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
        "Should not emit TS2339 for class-like constructor inheritance, got errors: {:?}",
        checker.ctx.diagnostics
    );
}
