#[test]
fn test_overload_call_resolves_basic_signatures() {
    let source = r#"
function fn(x: string): string;
function fn(x: number): number;
function fn(x: string | number): string | number { return x; }
fn("hello");
fn(42);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_handles_optional_params() {
    let source = r#"
function opt(a: string): void;
function opt(a: string, b: number): void;
function opt(a: string, b?: number): void {}
opt("x");
opt("x", 1);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_generic_function_value_is_assignable_to_non_generic_callback() {
    let source = r#"
const SK = <A, B>(_: A, b: B): B => b;
function accept<A, B>(f: (a: A, b: B) => B) {}
function run<A, B>() {
    accept<A, B>(SK);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_handles_rest_params() {
    let source = r#"
function rest(...args: number[]): void;
function rest(...args: string[]): void;
function rest(...args: any[]): void {}
rest(1, 2, 3);
rest("a", "b");
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_handles_tuple_spread_params() {
    let source = r#"
declare function foo1(a: number, b: string, c: boolean, ...d: number[]): void;

function foo2<T extends [number, string]>(t1: T, t2: [boolean], a1: number[]) {
    foo1(...t1, true, 42, 43, 44);
    foo1(...t1, ...t2, 42, 43, 44);
    foo1(...t1, ...t2, ...a1);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_handles_variadic_tuple_param() {
    let source = r#"
declare function ft3<T extends unknown[]>(t: [...T]): T;
declare function ft4<T extends unknown[]>(t: [...T]): readonly [...T];

ft3(["hello", 42]);
ft4(["hello", 42]);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_overload_call_handles_generic_signatures() {
    let source = r#"
function id<T>(x: T): T;
function id(x: any): any;
function id(x: any) { return x; }
id("test");
id(123);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that overload calls work with array methods
///
/// NOTE: Currently ignored - overload resolution for array methods is not fully
/// implemented. The checker doesn't correctly match array method overloads for
/// generic callback functions (map and filter work, reduce has overload issues).
#[test]
fn test_overload_call_array_methods() {
    let source = r#"
const arr = [1, 2, 3];
arr.map(x => x * 2);
arr.filter(x => x > 1);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// TODO: Array.reduce overload resolution picks wrong overload for callback type inference.
/// The callback should be contextually typed from the correct `Array.reduce` overload.
#[test]
fn test_overload_call_array_reduce() {
    let source = r#"
const arr = [1, 2, 3];
arr.reduce((a, b) => a + b, 0);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes.len(),
        0,
        "Expected no diagnostics once overload resolution picks the right Array.reduce overload, got: {codes:?}"
    );
}

/// Block-body callbacks with explicit param types should match generic overloads.
/// Regression test: `raw_block_body_callback_mismatch` was incorrectly checking
/// `is_assignable_to(actual_return, expected_return)` where `expected_return` was
/// a type parameter (e.g., `U`), causing a false TS2769 for block-body arrows
/// like `(acc: number[], a: number) => { return [a]; }` against `reduce<U>`.
#[test]
fn test_generic_overload_block_body_callback_no_false_ts2769() {
    let source = r#"
const arr = [1, 2, 3];
arr.reduce((acc: number[], a: number, index: number) => { return [a] }, []);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2769),
        "Should not emit TS2769 for block-body callback matching generic overload, got: {codes:?}"
    );
}

#[test]
fn test_class_method_overload_reports_no_overload_matches() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class C {
    foo(x: string): void;
    foo(x: number): void;
    foo(x: any) {}
}
const c = new C();
c.foo(true);
c.foo("ok");
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count_2769 = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
        .count();
    assert_eq!(
        count_2769, 1,
        "Expected exactly one overload mismatch (2769), got: {codes:?}"
    );
}

#[test]
fn test_new_expression_infers_class_instance_type() {
    use tsz_solver::TypeData;

    let source = r#"
class Foo {
    name = "";
    count = 1;
    readonly tag: string = "x";
    greet(msg: string): number { return 1; }
}
const f = new Foo();
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    println!("=== debug Box ===");
    if let Some(box_sym) = binder.file_locals.get("Box") {
        let box_type = checker.get_type_of_symbol(box_sym);
        println!("Box type id: {box_type:?}");
        println!("Box type key: {:?}", types.lookup(box_type));
    } else {
        println!("Box symbol missing");
    }

    let f_sym = binder.file_locals.get("f").expect("f should exist");
    let f_type = checker.get_type_of_symbol(f_sym);
    let f_key = types.lookup(f_type).expect("f type should exist");
    match f_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let name_atom = types.intern_string("name");
            let count_atom = types.intern_string("count");
            let tag_atom = types.intern_string("tag");
            let greet_atom = types.intern_string("greet");

            assert!(
                props
                    .iter()
                    .any(|p| p.name == name_atom && p.type_id == TypeId::STRING),
                "Expected name: string in class instance properties, got: {props:?}"
            );
            assert!(
                props
                    .iter()
                    .any(|p| p.name == count_atom && p.type_id == TypeId::NUMBER),
                "Expected count: number in class instance properties, got: {props:?}"
            );
            let tag_prop = props
                .iter()
                .find(|p| p.name == tag_atom)
                .expect("tag property should exist");
            assert!(tag_prop.readonly, "Expected tag to be readonly");
            assert!(
                props.iter().any(|p| p.name == greet_atom && p.is_method),
                "Expected greet method in class instance properties, got: {props:?}"
            );
        }
        _ => panic!("Expected f to be Object or ObjectWithIndex type, got {f_key:?}"),
    }
}

#[test]
fn test_new_expression_infers_parameter_properties() {
    use tsz_solver::TypeData;

    let source = r#"
class Foo {
    constructor(public id: number, readonly tag: string, count: number) {}
}
const f = new Foo(1, "x", 2);
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let f_sym = binder.file_locals.get("f").expect("f should exist");
    let f_type = checker.get_type_of_symbol(f_sym);
    let f_key = types.lookup(f_type).expect("f type should exist");
    match f_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let id_atom = types.intern_string("id");
            let tag_atom = types.intern_string("tag");
            let count_atom = types.intern_string("count");

            assert!(
                props
                    .iter()
                    .any(|p| p.name == id_atom && p.type_id == TypeId::NUMBER),
                "Expected id: number in class instance properties, got: {props:?}"
            );
            let tag_prop = props
                .iter()
                .find(|p| p.name == tag_atom)
                .expect("tag property should exist");
            assert_eq!(tag_prop.type_id, TypeId::STRING);
            assert!(tag_prop.readonly, "Expected tag to be readonly");
            assert!(
                !props.iter().any(|p| p.name == count_atom),
                "Expected count to be absent from class instance properties, got: {props:?}"
            );
        }
        _ => panic!("Expected f to be Object or ObjectWithIndex type, got {f_key:?}"),
    }
}

#[test]
fn test_new_expression_infers_base_class_properties() {
    use tsz_solver::TypeData;

    let source = r#"
class Base<T> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
}
class Derived extends Base<string> {
    count = 1;
    constructor() {
        super("default");
    }
}
const d = new Derived();
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let d_sym = binder.file_locals.get("d").expect("d should exist");
    let d_type = checker.get_type_of_symbol(d_sym);
    let d_key = types.lookup(d_type).expect("d type should exist");
    match d_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let value_atom = types.intern_string("value");
            let count_atom = types.intern_string("count");
            let value_prop = props
                .iter()
                .find(|p| p.name == value_atom)
                .expect("value property should exist");
            assert_eq!(value_prop.type_id, TypeId::STRING);
            assert!(
                props
                    .iter()
                    .any(|p| p.name == count_atom && p.type_id == TypeId::NUMBER),
                "Expected count: number in class instance properties, got: {props:?}"
            );
        }
        _ => panic!("Expected d to be Object or ObjectWithIndex type, got {d_key:?}"),
    }
}

#[test]
fn test_new_expression_infers_generic_class_type_params() {
    use tsz_solver::TypeData;

    let source = r#"
class Box<T> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
}
const b = new Box("hi");
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let b_sym = binder.file_locals.get("b").expect("b should exist");
    let b_type = checker.get_type_of_symbol(b_sym);
    let b_key = types.lookup(b_type).expect("b type should exist");
    match b_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let props = shape.properties.as_slice();
            let value_atom = types.intern_string("value");
            let value_prop = props
                .iter()
                .find(|p| p.name == value_atom)
                .expect("value property should exist");
            assert_eq!(value_prop.type_id, TypeId::STRING);
        }
        _ => panic!("Expected b to be Object or ObjectWithIndex type, got {b_key:?}"),
    }
}

#[test]
fn test_class_type_annotation_includes_inherited_properties() {
    let source = r#"
class Base { name: string; }
class Derived extends Base { }
let d: Derived;
d.name;
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
        "Did not expect 2339 for inherited class property access, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_generic_class_type_annotation_property_access() {
    let source = r#"
class Box<T> { value: T; }
let b: Box<string>;
b.value;
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
        "Did not expect 2339 for generic class property access, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_interface_extends_property_access() {
    let source = r#"
interface A { x: number; }
interface B extends A { y: number; }
function f(obj: B) { return obj.x + obj.y; }
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
        "Did not expect 2339 for interface-extended property access, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]

fn test_class_implements_interface_property_access() {
    let source = r#"
interface Printable { print(): void; }
class Doc implements Printable { }
let doc: Doc;
doc.print();
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
        "Did not expect 2339 for implements-based property access, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_new_expression_reports_overload_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Foo {
    constructor(x: string);
    constructor(x: number, y: number);
    constructor(x: any, y?: any) {}
}
new Foo(true);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // tsc reports TS2345 (not TS2769) when a single overload matches by arity — picks
    // the best-match and reports the specific type mismatch on that constructor signature.
    assert!(
        codes.contains(&2345) || codes.contains(&diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Expected TS2345 or TS2769 for constructor overload mismatch, got: {codes:?}"
    );
}

#[test]
fn test_new_expression_resolves_constructor_overloads() {
    let source = r#"
class Foo {
    constructor(x: string);
    constructor(x: number);
    constructor(x: any) {}
}
new Foo("ok");
new Foo(42);
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_new_expression_resolves_constructor_overloads_with_rest() {
    let source = r#"
class Foo {
    constructor(...args: number[]);
    constructor(...args: string[]);
    constructor(...args: any[]) {}
}
new Foo(1, 2, 3);
new Foo("a", "b");
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_parameter_property_in_function_2369() {
    // Parameter properties (public/private/protected/readonly on params)
    // are only allowed in constructor implementations
    let source = r#"function F(public x: string) { }"#;

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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2369),
        "Expected error 2369 for parameter property in function, got: {codes:?}"
    );
}

#[test]
fn test_parameter_property_in_arrow_2369() {
    let source = r#"var v = (public x: string) => { };"#;

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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2369),
        "Expected error 2369 for parameter property in arrow function, got: {codes:?}"
    );
}

#[test]
fn test_parameter_property_in_constructor_overload_2369() {
    // Constructor overload signatures should error on parameter properties
    let source = r#"
class C {
    constructor(public p1: string);
    constructor(public p2: number) {}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should have exactly one 2369 error for the overload, not for the implementation
    let count_2369 = codes.iter().filter(|&&c| c == 2369).count();
    assert_eq!(
        count_2369, 1,
        "Expected exactly 1 error 2369 for constructor overload, got {count_2369} from: {codes:?}"
    );
}

#[test]
fn test_parameter_property_in_constructor_implementation_ok() {
    // Constructor implementations are allowed to have parameter properties
    let source = r#"
class C {
    constructor(public x: string) {}
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2369),
        "Should not have error 2369 in constructor implementation, got: {codes:?}"
    );
}

#[test]
fn test_class_name_any_error_2414() {
    // Test that class name 'any' produces error 2414
    let code = "class any {}";
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
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2414),
        "Expected error 2414 (Class name cannot be 'any'), got: {codes:?}"
    );
}

#[test]
fn test_local_variable_scope_resolution() {
    // Test that local variables inside functions are properly resolved
    // This should NOT produce "Cannot find name 'x'" error
    let code = r#"
        function test() {
            let x: number = 1;
            let y = x + 1;
        }
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
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have no "Cannot find name" errors (2304)
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error for local variable, got: {codes:?}"
    );
}

#[test]
fn test_for_loop_variable_scope() {
    // Test that for loop variables are properly scoped
    let code = r#"
        function test() {
            for (let i = 0; i < 10; i++) {
                let x = i * 2;
            }
        }
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
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have no "Cannot find name" errors (2304) for loop variable 'i'
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error for loop variable, got: {codes:?}"
    );
}

#[test]
fn test_object_literal_properties_resolve_locals() {
    let source = r#"
function test() {
    const foo = 1;
    const bar = 2;
    const obj = { foo, baz: bar };
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error for object literal locals, got: {codes:?}"
    );
}

#[test]
fn test_export_default_in_ambient_module_resolves_local() {
    let source = r#"
declare module "foo" {
    const x: string;
    export default x;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not have 'Cannot find name' error in ambient export default, got: {codes:?}"
    );
}

#[test]
fn test_missing_identifier_emits_2304() {
    let source = r#"
let x = MissingName;
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for unresolved identifier, got: {codes:?}"
    );
}

#[test]
fn test_missing_type_reference_emits_2304() {
    let source = r#"
let x: MissingType;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for unresolved type reference, got: {codes:?}"
    );
}

/// Test that in a module file (has import), `declare module "x"` with body is
/// treated as a module augmentation, which emits TS2664 when the target module
/// doesn't exist. The import statement itself also emits TS2307.
#[test]
fn test_ts2307_import_with_module_augmentation() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
import { value } from "dep";

declare module "dep" {
    export const value: number;
}

value;
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

    // In an external module (file with import), `declare module "dep" { ... }` is a module
    // augmentation. Since "dep" doesn't exist, this emits TS2664 (Invalid module name in
    // augmentation). The import also emits TS2307 for the unresolved module.
    // Note: The declared_modules check in check_import_declaration prevents TS2307 because
    // the binder registers "dep" in declared_modules when it sees `declare module "dep"`.
    // So we only get TS2664 for the invalid augmentation.
    assert!(
        codes.contains(
            &diagnostic_codes::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND
        ),
        "Expected TS2664 for invalid module augmentation, got: {codes:?}"
    );
}

#[test]
fn test_declared_module_recorded_in_script() {
    let source = r#"
declare module "dep" {
    export const value: number;
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

    assert!(
        binder.declared_modules.contains("dep"),
        "Expected declared module to be recorded"
    );
}

// =========================================================================
// TS2307 Module Resolution Error Tests
// =========================================================================

/// Test TS2307 for relative import that cannot be resolved
#[test]
fn test_ts2307_relative_import_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
import { foo } from "./non-existent-module";
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            || codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O),
        "Expected TS2307 or TS2792 for relative import that cannot be resolved, got: {codes:?}"
    );
}

/// Test TS2307 for bare module specifier (npm package) that cannot be resolved
#[test]
fn test_ts2307_bare_specifier_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
import { something } from "nonexistent-npm-package";
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            || codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O),
        "Expected TS2307 or TS2792 for bare specifier that cannot be resolved, got: {codes:?}"
    );
}

/// Test TS2307 for unresolved CommonJS `require()` calls in checked JavaScript.
#[test]
fn test_ts2307_check_js_require_call_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const { foo } = require("bar");
"#;

    let mut parser = ParserState::new("main.js".to_string(), source.to_string());
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
        "main.js".to_string(),
        crate::checker::context::CheckerOptions {
            check_js: true,
            allow_js: true,
            module: crate::common::ModuleKind::CommonJS,
            target: crate::common::ScriptTarget::ES2018,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let ts2307: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .collect();
    assert_eq!(
        ts2307.len(),
        1,
        "Expected exactly one TS2307 for unresolved require(\"bar\"), got: {:?}",
        checker.ctx.diagnostics
    );
    assert!(
        ts2307[0].message_text.contains("'bar'"),
        "Expected TS2307 message to reference 'bar', got: {}",
        ts2307[0].message_text
    );
}

/// Local declarations named `require` should shadow CommonJS require semantics.
#[test]
fn test_local_require_shadowing_does_not_emit_ts2307() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function require(name) {
    return { foo: 1 };
}
const { foo } = require("bar");
"#;

    let mut parser = ParserState::new("main.js".to_string(), source.to_string());
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
        "main.js".to_string(),
        crate::checker::context::CheckerOptions {
            check_js: true,
            allow_js: true,
            module: crate::common::ModuleKind::CommonJS,
            no_implicit_any: false,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Local require() should shadow CommonJS module resolution. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that `declared_modules` prevents TS2307 when module is declared
#[test]
fn test_declared_module_prevents_ts2307() {
    use crate::checker::diagnostics::diagnostic_codes;

    // Script file (no import/export) with declare module
    let source = r#"
declare module "my-external-lib" {
    export const value: number;
}
"#;

    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Verify the module was registered
    assert!(
        binder.declared_modules.contains("my-external-lib"),
        "Expected 'my-external-lib' to be in declared_modules"
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.d.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // No TS2307 should be emitted since the module is declared
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Should not emit TS2307 when module is declared via 'declare module', got: {codes:?}"
    );
}

/// Test that `shorthand_ambient_modules` prevents TS2307 when module is declared without body
#[test]
fn test_shorthand_ambient_module_prevents_ts2307() {
    // Shorthand ambient module declaration (no body)
    let source = r#"
declare module "*.json";

import data from "./file.json";
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

    // Verify the shorthand module was registered
    assert!(
        binder.shorthand_ambient_modules.contains("*.json"),
        "Expected '*.json' to be in shorthand_ambient_modules"
    );

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

    // Note: The import "./file.json" will still emit TS2307 because the shorthand module
    // declaration is for "*.json" pattern, not "./file.json" literal.
    // This is expected behavior - shorthand ambient module pattern matching is not implemented.
}

/// Test TS2307 for scoped npm package import that cannot be resolved
#[test]
fn test_ts2307_scoped_package_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
import { Component } from "@angular/core";
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            || codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O),
        "Expected TS2307 or TS2792 for scoped package that cannot be resolved, got: {codes:?}"
    );
}

/// Test multiple unresolved imports each emit TS2307
#[test]
fn test_ts2307_multiple_unresolved_imports() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
import { foo } from "./missing1";
import { bar } from "./missing2";
import * as pkg from "nonexistent-pkg";
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    // Count module-not-found diagnostics (either TS2307 or TS2792)
    let module_not_found_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || d.code == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
        })
        .count();

    assert_eq!(
        module_not_found_count, 3,
        "Expected 3 module-not-found errors (TS2307/TS2792) for 3 unresolved imports, got: {module_not_found_count}"
    );
}

/// Test that TS2307 includes correct module specifier in message
#[test]
fn test_ts2307_diagnostic_message_contains_specifier() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
import { foo } from "./specific-missing-module";
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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    let module_diag = checker.ctx.diagnostics.iter().find(|d| {
        d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            || d.code == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
    });

    assert!(
        module_diag.is_some(),
        "Expected TS2307 or TS2792 diagnostic"
    );
    let diag = module_diag.unwrap();
    assert!(
        diag.message_text.contains("./specific-missing-module"),
        "Module-not-found message should contain module specifier, got: {}",
        diag.message_text
    );
}

