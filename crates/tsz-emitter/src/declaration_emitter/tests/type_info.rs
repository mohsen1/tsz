use super::*;

#[test]
fn test_same_file_symbol_module_path_is_none() {
    let source = r#"
namespace m1 {
    export class c {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), "".to_string());
    let _root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), "".to_string());
    let _root = parser.parse_source_file();

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
fn test_named_class_expression_constructor_type_is_inlined() {
    let source = r#"
export function wrapClass(param: any) {
    return class Wrapped {
        foo() { return param; }
    };
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
interface GreeterConstructor {
    new (): {};
}
declare function getGreeterBase(): GreeterConstructor;
export default class extends getGreeterBase() {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
fn test_empty_anonymous_class_shape_recovers_method_from_ast() {
    let source = r#"
declare const a: symbol;
const Value = class {
    [a](): number {
        return 1;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
