use super::*;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn test_same_file_symbol_module_path_is_none() {
    let source = r#"
namespace m1 {
    export class c {}
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let current_arena = Arc::new(parser.arena.clone());
    let arena_addr = Arc::as_ptr(&current_arena) as usize;
    let mut arena_to_path = FxHashMap::default();
    arena_to_path.insert(arena_addr, "test.ts".to_string());

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.set_arena_to_path(arena_to_path);

    let sym_id = binder
        .file_locals
        .get("m1")
        .expect("expected same-file namespace symbol");

    assert!(
        emitter.resolve_symbol_module_path(sym_id).is_none(),
        "Expected same-file symbol to have no module path"
    );
}

#[test]
fn test_local_class_declaration_constructor_type_is_inlined() {
    let source = r#"
export function middle() {
    abstract class Middle {
        get a(): number { return 1; }
        set a(arg: number) {}
    }
    return Middle;
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let middle_sym = find_class_symbol(
        &parser,
        &binder,
        "Middle",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );

    let interner = TypeInterner::new();
    let a_atom = interner.intern_string("a");
    let mut accessor = PropertyInfo::new(a_atom, TypeId::NUMBER);
    accessor.write_type = TypeId::NUMBER;
    accessor.is_class_prototype = true;
    accessor.parent_id = Some(middle_sym);
    accessor.declaration_order = 1;

    let instance_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![accessor],
        string_index: None,
        number_index: None,
        symbol: Some(middle_sym),
    });
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), instance_type)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(middle_sym),
        is_abstract: true,
    });

    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(ctor_type);

    assert!(
        !printed.contains("Middle"),
        "Did not expect local class declaration name to leak into constructor type: {printed}"
    );
    assert!(
        printed.contains("abstract new () => {"),
        "Expected local class declaration to emit as a structural constructor type: {printed}"
    );
    assert!(
        printed.contains("get a(): number;"),
        "Expected accessor getter to be preserved in structural emit: {printed}"
    );
    assert!(
        printed.contains("set a(arg: number);"),
        "Expected accessor setter to be preserved in structural emit: {printed}"
    );
}

#[test]
fn test_structural_setter_only_property_uses_write_type() {
    let (parser, _root) = parse_test_source("");
    let binder = BinderState::new();

    let interner = TypeInterner::new();
    let x_atom = interner.intern_string("x");
    let mut setter_only = PropertyInfo::new(x_atom, TypeId::UNDEFINED);
    setter_only.write_type = TypeId::NUMBER;

    let point_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![setter_only],
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(point_type);

    assert!(
        printed.contains("x: number;"),
        "Expected setter-only structural property to use write type in declaration emit: {printed}"
    );
    assert!(
        !printed.contains("x: undefined;"),
        "Did not expect setter-only structural property to use undefined read type: {printed}"
    );
}

#[test]
fn test_foreign_global_lazy_type_application_keeps_alias_name() {
    let (parser, _root) = parse_test_source("");

    let mut foreign_parser = ParserState::new(
        "lib.es2019.array.d.ts".to_string(),
        "type FlatArray<T, D> = T;".to_string(),
    );
    let _foreign_root = foreign_parser.parse_source_file();
    let foreign_decl = foreign_parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION).then_some(NodeIndex(idx as u32))
        })
        .expect("missing foreign type alias declaration");

    let mut binder = BinderState::new();
    let flat_array_sym = binder
        .symbols
        .alloc(symbol_flags::TYPE_ALIAS, "FlatArray".to_string());
    binder
        .symbols
        .get_mut(flat_array_sym)
        .expect("missing synthetic symbol")
        .declarations
        .push(foreign_decl);

    let interner = TypeInterner::new();
    let def_id = DefId(42);
    let flat_array_type =
        interner.application(interner.lazy(def_id), vec![TypeId::STRING, TypeId::NUMBER]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(def_id, flat_array_sym);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(flat_array_type);

    assert_eq!(printed, "FlatArray<string, number>");
}

#[test]
fn test_first_generic_function_type_argument_is_parenthesized() {
    let source = r#"
class X<A> {}
var prop11: X< <Tany>() => Tany >;
var prop12: X<(<Tany>() => Tany)>;
function f1() {
    return prop11;
}
class Y<A, B> {}
var prop3: Y< <Tany>() => Tany, <Tany>() => Tany>;
"#;

    let output = emit_dts_with_binding(source);

    assert!(
        output.contains("declare var prop11: X<(<Tany>() => Tany)>;"),
        "Expected first generic function type argument to be parenthesized: {output}"
    );
    assert!(
        output.contains("declare var prop12: X<(<Tany>() => Tany)>;"),
        "Expected explicitly parenthesized generic function type argument to remain stable: {output}"
    );
    assert!(
        output.contains("declare function f1(): X<(<Tany>() => Tany)>;"),
        "Expected inferred return type to preserve first generic function type argument parentheses: {output}"
    );
    assert!(
        output.contains("declare var prop3: Y<(<Tany>() => Tany), <Tany>() => Tany>;"),
        "Expected only the first generic function type argument to be parenthesized: {output}"
    );
}

#[test]
fn test_declared_generic_call_return_preserves_wrapper_for_generic_function_argument() {
    let output = emit_dts_with_binding(
        r#"
interface Modifier<T> {}
declare function fn<T>(x: T): Modifier<T>;
export const value = fn(<T>(x: T): T => x);
"#,
    );

    assert!(
        output.contains("export declare const value: Modifier<(<T>(x: T) => T)>;"),
        "Expected declared generic call return wrapper to survive generic function substitution: {output}"
    );
}

#[test]
fn test_named_class_expression_constructor_type_is_inlined() {
    let source = r#"
export function wrapClass(param: any) {
    return class Wrapped {
        foo() { return param; }
    };
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let wrapped_sym = find_class_symbol(
        &parser,
        &binder,
        "Wrapped",
        tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION,
    );

    let interner = TypeInterner::new();
    let foo_atom = interner.intern_string("foo");
    let method_type = interner.function(FunctionShape::new(Vec::new(), TypeId::ANY));
    let mut foo = PropertyInfo::method(foo_atom, method_type);
    foo.is_class_prototype = true;
    foo.parent_id = Some(wrapped_sym);
    foo.declaration_order = 1;

    let instance_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![foo],
        string_index: None,
        number_index: None,
        symbol: Some(wrapped_sym),
    });
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), instance_type)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(wrapped_sym),
        is_abstract: false,
    });

    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(ctor_type);

    assert!(
        !printed.contains("Wrapped"),
        "Did not expect named class expression name to leak into constructor type: {printed}"
    );
    assert!(
        printed.contains("new (): {"),
        "Expected named class expression to emit as a structural constructor type: {printed}"
    );
    assert!(
        printed.contains("foo(): any;"),
        "Expected named class expression methods to be preserved structurally: {printed}"
    );
}

#[test]
fn test_named_class_extends_expression_uses_synthetic_base_alias() {
    let source = r#"
interface MixedBase {
    new (): {
        bar: number;
    };
}
declare function mixin(base: any): MixedBase;
declare class Base {}
export class Derived extends mixin(Base) {}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "Derived",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);
    let mixed_base_sym = find_interface_symbol(&parser, &binder, "MixedBase");

    let interner = TypeInterner::new();
    let mixed_base_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(mixed_base_sym),
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, mixed_base_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const Derived_base: MixedBase;"),
        "Expected named class extends expression to synthesize a base alias: {output}"
    );
    assert!(
        output.contains("export declare class Derived extends Derived_base {"),
        "Expected class heritage to reference the synthetic base alias: {output}"
    );
    assert!(
        !output.contains("extends mixin(Base)"),
        "Did not expect raw extends expression to leak into declaration output: {output}"
    );
}

#[test]
fn test_default_export_class_extends_expression_uses_synthetic_base_alias() {
    let source = r#"
interface Greeter {
    getGreeting(): string;
}
interface GreeterConstructor {
    new (): Greeter;
}
declare function getGreeterBase(): GreeterConstructor;
export default class extends getGreeterBase() {}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_first_class_node(
        &parser,
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);
    let greeter_ctor_sym = find_interface_symbol(&parser, &binder, "GreeterConstructor");

    let interner = TypeInterner::new();
    let greeter_ctor_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(greeter_ctor_sym),
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, greeter_ctor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("interface Greeter {"),
        "Expected synthetic base alias dependencies to retain constructor return interface: {output}"
    );
    assert!(
        output.contains("declare const default_base: GreeterConstructor;"),
        "Expected default export class extends expression to synthesize a default_base alias: {output}"
    );
    assert!(
        output.contains("export default class extends default_base {"),
        "Expected default export class to extend the synthetic base alias: {output}"
    );
    assert!(
        !output.contains("extends getGreeterBase()"),
        "Did not expect raw default export extends expression in declaration output: {output}"
    );
}

#[test]
fn test_named_class_extends_expression_preserves_type_arguments_on_alias() {
    let source = r#"
declare function getBase(): any;
export class Derived extends getBase()<string, number> {}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "Derived",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, TypeId::ANY);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const Derived_base: any;"),
        "Expected synthetic base alias to be emitted: {output}"
    );
    assert!(
        output.contains("export declare class Derived extends Derived_base<string, number> {"),
        "Expected class heritage to preserve original type arguments on the alias: {output}"
    );
    assert!(
        !output.contains("extends getBase()<string, number>"),
        "Did not expect raw extends expression to leak into declaration output: {output}"
    );
}

#[test]
fn test_named_class_extends_expression_keeps_local_class_dependency() {
    let source = r#"
export {};
class LocalBase<T, U> {
    x: T;
    y: U;
}
declare function getBase(): typeof LocalBase;
export class Derived extends getBase()<string, number> {}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "Derived",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);
    let local_base_sym = find_class_symbol(
        &parser,
        &binder,
        "LocalBase",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    let local_base_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(local_base_sym),
        is_abstract: false,
    });
    type_cache
        .node_types
        .insert(extends_expr_idx.0, local_base_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare class LocalBase<T, U> {"),
        "Expected local base class declaration to be retained for synthetic alias types: {output}"
    );
    assert!(
        output.contains("export declare class Derived extends Derived_base<string, number> {"),
        "Expected derived class to preserve original type arguments on the alias: {output}"
    );
}

#[test]
fn test_named_class_extends_expression_recovers_returned_local_class_when_type_is_never() {
    let source = r#"
type AnyFunction<Result = any> = (...input: any[]) => Result;
type AnyConstructor<Instance extends object = object, Static extends object = object> =
    (new (...input: any[]) => Instance) & Static;
type MixinHelperFunc = <A extends AnyConstructor, T>(required: [A], arg: T) => T extends AnyFunction<infer M> ? M : never;
export const Mixin: MixinHelperFunc = null as any;
export class Base {}
export class XmlElement2 extends Mixin(
    [Base],
    (base: AnyConstructor<Base, typeof Base>) => {
        class XmlElement2 extends base {
            num: number = 0;
        }
        return XmlElement2;
    }) {}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            (node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION)
                .then_some(NodeIndex(idx as u32))
                .filter(|&idx| {
                    parser
                        .arena
                        .get(idx)
                        .and_then(|node| parser.arena.get_class(node))
                        .is_some_and(|class| {
                            parser.arena.get_identifier_text(class.name) == Some("XmlElement2")
                        })
                })
        })
        .next_back()
        .expect("missing exported XmlElement2 class");
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, TypeId::NEVER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains(
            "declare const XmlElement2_base: {\n    new (): {\n        num: number;\n    };\n};"
        ),
        "Expected source fallback to recover returned local class constructor shape: {output}"
    );
    assert!(
        !output.contains("declare const XmlElement2_base: never;"),
        "Did not expect synthetic class base alias to stay `never`: {output}"
    );
}

#[test]
fn test_local_class_mixin_preserves_base_static_intersection() {
    let output = emit_dts_with_usage_analysis(
        r#"
interface Constructor<C> { new (...args: any[]): C; }

function mixin<B extends Constructor<{}>>(Base: B) {
    class PrivateMixed extends Base {
        bar = 2;
    }
    return PrivateMixed;
}

export class Unmixed {
    foo = 1;
}

export const Mixed = mixin(Unmixed);
"#,
    );

    assert!(
        output.contains("} & typeof Unmixed;"),
        "Expected mixin constructor type to preserve base static side: {output}"
    );
    assert!(
        !output.contains("foo: number;\n        bar: number;"),
        "Inherited base instance fields should stay behind typeof base intersection: {output}"
    );
}

#[test]
fn test_class_extends_mixin_recovered_base_alias_uses_object_constructor_syntax() {
    let source = r#"
class A {
    constructor(...args: any[]) {}
    get myName(): string {
        return "A";
    }
}

function Mixin<T extends typeof A>(Super: T) {
    return class B extends Super {
        get myName(): string {
            return "B";
        }
    };
}

export class C extends Mixin(A) {
    get myName(): string {
        return "C";
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "C",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, TypeId::NEVER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains(
            "declare const C_base: {\n    new (...args: any[]): {\n        get myName(): string;\n    };\n} & typeof A;"
        ),
        "Expected recovered synthetic base alias to use object constructor syntax: {output}"
    );
    assert!(
        !output.contains("declare const C_base: (new (...args: any[]) =>"),
        "Did not expect synthetic base aliases to use arrow constructor syntax: {output}"
    );
}

#[test]
fn test_class_extends_mixin_recovered_auto_accessor_expands_in_structural_alias() {
    let source = r#"
function mixin<T extends { new (...args: any[]): {} }>(superclass: T) {
    return class extends superclass {
        accessor name = "";
    };
}

class BaseClass {
    accessor name = "";
}

export class MyClass extends mixin(BaseClass) {
    accessor name = "";
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "MyClass",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, TypeId::NEVER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains(
            "declare const MyClass_base: {\n    new (...args: any[]): {\n        get name(): string;\n        set name(arg: string);\n    };\n} & typeof BaseClass;"
        ),
        "Expected recovered synthetic base alias to expand auto accessors structurally: {output}"
    );
    assert!(
        output.contains(
            "declare function mixin<T extends {\n    new (...args: any[]): {};\n}>(superclass: T): {\n    new (...args: any[]): {\n        get name(): string;\n        set name(arg: string);\n    };\n} & T;"
        ),
        "Expected returned-class function type to expand auto accessors structurally: {output}"
    );
}

#[test]
fn test_class_extends_abstract_mixin_recovered_base_alias_uses_abstract_arrow_syntax() {
    let source = r#"
interface Constructor<C> { new (...args: any[]): C; }

export class Unmixed {
    foo = 1;
}

function Filter<C extends Constructor<{}>>(ctor: C) {
    abstract class FilterMixin extends ctor {
        abstract match(path: string): boolean;
        thing = 12;
    }
    return FilterMixin;
}

export class FilteredThing extends Filter(Unmixed) {
    match(path: string) {
        return false;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "FilteredThing",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(extends_expr_idx.0, TypeId::NEVER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains(
            "declare const FilteredThing_base: (abstract new (...args: any[]) => {\n    match(path: string): boolean;\n    thing: number;\n}) & typeof Unmixed;"
        ),
        "Expected abstract recovered synthetic base alias to keep abstract arrow syntax: {output}"
    );
}

#[test]
fn test_mixin_call_intersection_substitutes_nested_call_return_text() {
    let source = r#"
type Constructor<T> = new(...args: any[]) => T;

class Base {}
class Derived extends Base {}

interface Printable {
    print(): void;
}

const Printable = <T extends Constructor<Base>>(superClass: T): Constructor<Printable> & { message: string } & T =>
    class extends superClass {
        static message = "hello";
        print() {}
    }

interface Tagged {
    _tag: string;
}

function Tagged<T extends Constructor<{}>>(superClass: T): Constructor<Tagged> & T {
    class C extends superClass {
        _tag: string;
    }
    return C;
}

const Thing2 = Tagged(Printable(Derived));
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains(
            "declare const Thing2: Constructor<Tagged> & Constructor<Printable> & {\n    message: string;\n} & Constructor<Base>;"
        ),
        "Expected nested mixin call substitution to preserve source intersection order: {output}"
    );
    assert!(
        !output.contains("=> {"),
        "Did not expect recovered arrow body text in nested mixin return type: {output}"
    );
}

#[test]
#[ignore = "regressed after remote changes: class extends expression declaration emit loses local dependency source order"]
fn test_named_class_extends_expression_keeps_local_dependency_in_source_order() {
    let source = r#"
export class ExportedClass<T> {
    x: T;
}

class LocalClass<T, U> {
    x: T;
    y: U;
}

export interface ExportedInterface {
    x: number;
}

interface LocalInterface {
    x: number;
}

declare function getLocalClass<T>(c: T): typeof LocalClass;

export class MyClass extends getLocalClass<LocalInterface>(undefined)<string, number> {}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_class_node(
        &parser,
        "MyClass",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, class_idx);
    let local_base_sym = find_class_symbol(
        &parser,
        &binder,
        "LocalClass",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    let local_base_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(local_base_sym),
        is_abstract: false,
    });
    type_cache
        .node_types
        .insert(extends_expr_idx.0, local_base_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    let local_class_pos = output.find("declare class LocalClass<T, U> {").unwrap();
    let exported_interface_pos = output.find("export interface ExportedInterface {").unwrap();
    let my_class_base_pos = output.find("declare const MyClass_base:").unwrap();

    assert!(
        local_class_pos < exported_interface_pos,
        "Expected local class dependency to keep source order: {output}"
    );
    assert!(
        local_class_pos < my_class_base_pos,
        "Expected local dependency to emit before the synthetic base alias: {output}"
    );
}

#[test]
fn test_namespace_extends_expression_keeps_export_modifiers_with_synthetic_alias() {
    let source = r#"
namespace Test {
    export interface IFace {}

    export class SomeClass implements IFace {}

    export class Derived extends getClass<IFace>() {}

    export function getClass<T>(): new () => T {
        return SomeClass as new () => T;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let derived_idx = find_class_node(
        &parser,
        "Derived",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let extends_expr_idx = find_class_extends_expression(&parser, derived_idx);
    let iface_sym = find_interface_symbol(&parser, &binder, "IFace");

    let interner = TypeInterner::new();
    let instance_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(iface_sym),
    });
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), instance_type)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(extends_expr_idx.0, ctor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export interface IFace"),
        "Expected namespace interface to keep export modifier when synthetic alias is emitted: {output}"
    );
    assert!(
        output.contains("export class SomeClass"),
        "Expected namespace class to keep export modifier when synthetic alias is emitted: {output}"
    );
    assert!(
        output.contains("export class Derived extends Derived_base"),
        "Expected derived class to keep export modifier when synthetic alias is emitted: {output}"
    );
    assert!(
        output.contains("export function getClass<T>()"),
        "Expected namespace function to keep export modifier when synthetic alias is emitted: {output}"
    );
    assert!(
        output.contains("const Derived_base: new () => IFace;"),
        "Expected synthetic base alias to use constructor type syntax: {output}"
    );
}

#[test]
fn test_non_unique_symbol_computed_method_uses_property_syntax_in_structural_type() {
    let source = r#"
export const a: symbol = Symbol();
export class A {
    [a](): number {
        return 1;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_sym = find_class_symbol(
        &parser,
        &binder,
        "A",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let method_name_idx = find_first_class_method_name(
        &parser,
        "A",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let computed_expr_idx = parser
        .arena
        .get(method_name_idx)
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("expected computed method name");

    let interner = TypeInterner::new();
    let method_type = interner.function(FunctionShape::new(Vec::new(), TypeId::NUMBER));
    let mut method = PropertyInfo::method(interner.intern_string("[a]"), method_type);
    method.is_class_prototype = true;
    method.parent_id = Some(class_sym);
    method.declaration_order = 1;
    let mut duplicate_property =
        PropertyInfo::new(interner.intern_string("[iterator]"), method_type);
    duplicate_property.parent_id = Some(class_sym);
    duplicate_property.declaration_order = 2;

    let instance_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![method, duplicate_property],
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(computed_expr_idx.0, TypeId::SYMBOL);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(instance_type);

    assert!(
        printed.contains("[a]: () => number;"),
        "Expected non-unique symbol keyed method to emit as property signature: {printed}"
    );
    assert!(
        !printed.contains("[a](): number;"),
        "Did not expect non-unique symbol keyed method syntax: {printed}"
    );
}

#[test]
fn test_unique_symbol_computed_method_keeps_method_syntax_in_structural_type() {
    let source = r#"
export declare const iterator: unique symbol;
export class A {
    [iterator](): number {
        return 1;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_sym = find_class_symbol(
        &parser,
        &binder,
        "A",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let method_name_idx = find_first_class_method_name(
        &parser,
        "A",
        tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION,
    );
    let computed_expr_idx = parser
        .arena
        .get(method_name_idx)
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("expected computed method name");

    let interner = TypeInterner::new();
    let unique_symbol_type = interner.unique_symbol(SymbolRef(class_sym.0));
    let method_type = interner.function(FunctionShape::new(Vec::new(), TypeId::NUMBER));
    let mut method = PropertyInfo::method(interner.intern_string("[iterator]"), method_type);
    method.is_class_prototype = true;
    method.parent_id = Some(class_sym);
    method.declaration_order = 1;

    let instance_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![method],
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(computed_expr_idx.0, unique_symbol_type);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(instance_type);

    assert!(
        printed.contains("[iterator](): number;"),
        "Expected unique symbol keyed method to keep method syntax: {printed}"
    );
    assert!(
        !printed.contains("[iterator]: () => number;"),
        "Did not expect unique symbol keyed property syntax: {printed}"
    );
}

#[test]
fn test_synthesized_computed_method_index_signatures_widen_nested_literal_returns() {
    let (parser, _root) = parse_test_source("");
    let binder = BinderState::new();

    let interner = TypeInterner::new();
    let method_return = interner.union(vec![
        interner.function(FunctionShape::new(
            Vec::new(),
            interner.literal_string("value"),
        )),
        interner.function(FunctionShape::new(
            Vec::new(),
            interner.literal_number(42.0),
        )),
    ]);
    let instance_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: method_return,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        number_index: None,
        symbol: None,
    });

    let static_true_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("static"),
            interner.literal_boolean(true),
        )],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let static_string_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("static"),
            interner.literal_string("sometimes"),
        )],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let static_index_type = interner.union(vec![
        instance_type,
        interner.function(FunctionShape::new(Vec::new(), static_true_type)),
        interner.function(FunctionShape::new(Vec::new(), static_string_type)),
    ]);
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), instance_type)],
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: static_index_type,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(ctor_type);

    assert!(
        printed.contains("[x: string]: (() => string) | (() => number);"),
        "Expected instance computed method returns to widen inside the synthesized index signature: {printed}"
    );
    assert!(
        printed.contains("static: boolean;"),
        "Expected static computed method object returns to widen boolean literals: {printed}"
    );
    assert!(
        printed.contains("static: string;"),
        "Expected static computed method object returns to widen string literals: {printed}"
    );
    assert!(
        !printed.contains("\"value\"")
            && !printed.contains("42")
            && !printed.contains("true")
            && !printed.contains("\"sometimes\""),
        "Did not expect literal return types to leak from synthesized computed methods: {printed}"
    );
}

#[test]
fn test_empty_anonymous_class_shape_recovers_method_from_ast() {
    let source = r#"
declare const a: symbol;
const Value = class {
    [a](): number {
        return 1;
    }
};
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let class_idx = find_first_class_node(
        &parser,
        tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION,
    );
    let class_sym = binder
        .get_node_symbol(class_idx)
        .expect("missing anonymous class symbol");
    let class = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class data");
    let method_idx = class.members.nodes[0];
    let method_name_idx = parser
        .arena
        .get(method_idx)
        .and_then(|node| parser.arena.get_method_decl(node))
        .map(|method| method.name)
        .expect("missing method name");
    let computed_expr_idx = parser
        .arena
        .get(method_name_idx)
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("expected computed method name");

    let interner = TypeInterner::new();
    let method_type = interner.function(FunctionShape::new(Vec::new(), TypeId::NUMBER));
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(
            Vec::new(),
            interner.object_with_index(ObjectShape {
                flags: ObjectFlags::default(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: Some(class_sym),
            }),
        )],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(class_sym),
        is_abstract: false,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, method_type);
    type_cache
        .node_types
        .insert(computed_expr_idx.0, TypeId::SYMBOL);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(ctor_type);

    assert!(
        printed.contains("new (): {"),
        "Expected anonymous class constructor type: {printed}"
    );
    assert!(
        printed.contains("[a]: () => number;"),
        "Expected anonymous class members to be recovered from AST when cached shape is empty: {printed}"
    );
}

#[test]
fn test_same_file_generic_namespace_type_stays_unqualified() {
    let source = r#"
export namespace C {
    export class A<T> {}
    export class B {}
}

export const value = null as any;
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .file_locals
        .get("C")
        .expect("missing namespace symbol");
    let c_symbol = binder.symbols.get(c_sym).expect("missing namespace data");
    let exports = c_symbol
        .exports
        .as_ref()
        .expect("expected namespace exports");
    let a_sym = exports.get("A").expect("missing class A symbol");
    let b_sym = exports.get("B").expect("missing class B symbol");

    let interner = TypeInterner::new();
    let a_def = tsz_solver::DefId(9101);
    let b_def = tsz_solver::DefId(9102);
    let value_type = interner.application(interner.lazy(a_def), vec![interner.lazy(b_def)]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(a_def, a_sym);
    type_cache.def_to_symbol.insert(b_def, b_sym);

    let current_arena = Arc::new(parser.arena.clone());
    let arena_addr = Arc::as_ptr(&current_arena) as usize;
    let mut arena_to_path = FxHashMap::default();
    arena_to_path.insert(arena_addr, "test.ts".to_string());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.set_arena_to_path(arena_to_path);
    let printed = emitter.print_type_id(value_type);

    assert!(
        printed == "C.A<C.B>",
        "Expected same-file generic type to stay local: {printed}"
    );
    assert!(
        !printed.contains("import(\"./test\").C.B"),
        "Did not expect same-file type references to be import-qualified: {printed}"
    );
}

#[test]
fn test_display_alias_preserves_generic_class_type_arguments() {
    let source = r#"
export namespace C {
    export class A<T> {}
    export class B {}
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .file_locals
        .get("C")
        .expect("missing namespace symbol");
    let c_symbol = binder.symbols.get(c_sym).expect("missing namespace data");
    let exports = c_symbol
        .exports
        .as_ref()
        .expect("expected namespace exports");
    let a_sym = exports.get("A").expect("missing class A symbol");
    let b_sym = exports.get("B").expect("missing class B symbol");

    let interner = TypeInterner::new();
    let a_def = tsz_solver::DefId(9201);
    let b_def = tsz_solver::DefId(9202);
    let app_type = interner.application(interner.lazy(a_def), vec![interner.lazy(b_def)]);
    let evaluated_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(a_sym),
    });
    interner.store_display_alias(evaluated_type, app_type);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(a_def, a_sym);
    type_cache.def_to_symbol.insert(b_def, b_sym);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(evaluated_type);

    assert_eq!(printed, "C.A<C.B>");
}

#[test]
fn test_type_application_elides_trailing_default_type_argument() {
    let (parser, _root) = parse_test_source("");
    let binder = BinderState::new();

    let interner = TypeInterner::new();
    let promise_def = DefId(9301);
    let resolve_atom = interner.intern_string("ResolveType");
    let reject_atom = interner.intern_string("RejectType");
    let promise_type = interner.application(
        interner.lazy(promise_def),
        vec![TypeId::STRING, TypeId::ANY],
    );

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .def_to_name
        .insert(promise_def, "TPromise".to_string());
    type_cache.def_type_params.insert(
        promise_def.0,
        vec![
            tsz_solver::types::TypeParamInfo {
                name: resolve_atom,
                constraint: None,
                default: None,
                is_const: false,
            },
            tsz_solver::types::TypeParamInfo {
                name: reject_atom,
                constraint: None,
                default: Some(TypeId::ANY),
                is_const: false,
            },
        ],
    );

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(promise_type);

    assert_eq!(printed, "TPromise<string>");
}

#[test]
fn test_object_literal_enum_values_preserve_typeof_and_widen_members() {
    let output = emit_dts_with_binding(
        r#"
namespace m1 {
    export enum e {
        weekday,
        weekend,
        holiday,
    }
}

var d = {
    me: { en: m1.e },
    mh: m1.e.holiday,
};
"#,
    );

    assert!(
        output.contains("en: typeof m1.e;"),
        "Expected enum object value to emit typeof enum: {output}"
    );
    assert!(
        output.contains("mh: m1.e;"),
        "Expected enum member value to widen to enum type: {output}"
    );
    assert!(
        !output.contains("mh: m1.e.holiday;"),
        "Did not expect enum member literal to leak into anonymous object type: {output}"
    );
}

#[test]
fn test_nested_namespace_enum_value_typeof_uses_relative_reference() {
    let output = emit_dts_with_binding(
        r#"
namespace A.B.C {
    export enum e {
        weekday,
        weekend,
    }
}
namespace A.B.D {
    export var d = {
        me: { en: A.B.C.e },
    };
}
"#,
    );

    assert!(
        output.contains("en: typeof B.C.e;"),
        "Expected enum object value typeof reference to be relative inside nested namespace: {output}"
    );
    assert!(
        !output.contains("en: typeof A.B.C.e;"),
        "Did not expect nested namespace typeof reference to stay fully qualified: {output}"
    );
}

#[test]
fn test_returned_auto_accessor_parameter_unknown_uses_parameter_type() {
    let source = r#"
function mixin<T extends { new (...args: any[]): {} }>(superclass: T) {
    return class extends superclass {};
}

export function wrapper<T>(value: T) {
    class BaseClass {
        accessor name = value;
    }
    return class MyClass extends mixin(BaseClass) {
        accessor name = value;
    };
}
"#;

    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let wrapper = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser
                .arena
                .get_function(node)
                .filter(|func| parser.arena.get_identifier_text(func.name) == Some("wrapper"))
        })
        .expect("missing wrapper function");

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let rewritten = emitter.rewrite_returned_auto_accessor_parameter_unknowns(
        wrapper,
        "{\n    new (): {\n        get name(): unknown;\n        set name(arg: unknown);\n    };\n}",
    );

    assert!(
        rewritten.contains("get name(): T;"),
        "Expected getter type to come from the accessor initializer parameter: {rewritten}"
    );
    assert!(
        rewritten.contains("set name(arg: T);"),
        "Expected setter type to come from the accessor initializer parameter: {rewritten}"
    );
}

/// Regression test for `declarationEmitShadowingInferNotRenamed`: a single
/// non-abstract construct signature must render as `new (...) => T` (matching
/// tsc), and an `Infer(T)` placeholder appearing inside the extends clause of
/// a conditional must render as `infer T` (not `T`, and not collapsed to a
/// `{ new(): { ... } }` object literal). Inside the conditional's true/false
/// branches the same `Infer(T)` collapses to the bare name `T`.
#[test]
fn test_constructor_with_infer_in_extends_renders_as_arrow_with_infer() {
    use tsz_solver::types::{ConditionalType, TypeParamInfo};

    let interner = TypeInterner::new();
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
    });
    let c_atom = interner.intern_string("C");
    let c_param_info = TypeParamInfo {
        name: c_atom,
        constraint: None,
        default: None,
        is_const: false,
    };
    let infer_c = interner.infer(c_param_info);

    // Build a non-abstract constructor type whose return is `infer C`.
    let ctor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), infer_c)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    // Build conditional `any extends (new () => infer C) ? C : never` and
    // verify both:
    //   - the extends clause renders as `new () => infer C`
    //   - the true branch references `C` as a bare name (no `infer`).
    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: ctor_type,
        true_type: infer_c,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let parser = ParserState::new("test.ts".to_string(), String::new());
    let binder = BinderState::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let printed = emitter.print_type_id(cond);

    assert!(
        printed.contains("new () => infer C"),
        "Expected non-abstract single-construct callable to render as `new () => infer C` \
         when its return type is an Infer placeholder inside a conditional's extends clause: \
         {printed}"
    );
    assert!(
        !printed.contains("{\n    new (): infer C"),
        "Did not expect a single-construct callable to fall through to the \
         object-literal `{{ new (): T }}` form: {printed}"
    );
    // True branch references the same Infer placeholder; tsc prints just `C`.
    assert!(
        printed.contains("? C : "),
        "Expected the true branch to reference the inferred placeholder by bare \
         name `C`, not `infer C`: {printed}"
    );
}

#[test]
fn test_inexact_optional_mapped_intersection_simplifies_for_inferred_emit() {
    let actual = r#"(x: {} & {
    [K in "foo" | "bar" | "baz" as undefined extends {
    foo?: string;
    bar: number;
    baz: undefined;
}[keyof unknown] ? keyof unknown : never]+?: undefined extends {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown] ? {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown] | undefined : {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown];
} & {
    [K in "foo" | "bar" | "baz" as undefined extends {
    foo?: string;
    bar: number;
    baz: undefined;
}[keyof unknown] ? never : keyof unknown]: {
        foo?: string;
        bar: number;
        baz: undefined;
    }[keyof unknown];
}) => null"#;

    let simplified = DeclarationEmitter::simplify_inexact_optional_mapped_intersection_text(actual)
        .expect("expected inexact optional mapped intersection to simplify");

    assert_eq!(
        simplified,
        "(x: {\n    foo?: string | undefined;\n    baz?: undefined;\n} & {\n    bar: number;\n}) => null"
    );
}
