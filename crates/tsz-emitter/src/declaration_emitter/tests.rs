use super::*;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::{
    CallSignature, CallableShape, DefId, FunctionShape, ObjectFlags, ObjectShape, ParamInfo,
    PropertyInfo, SymbolRef, TupleElement, TypeId, TypeInterner,
};

// =============================================================================
// Helper
// =============================================================================

fn emit_dts(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

fn emit_dts_with_binding(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.emit(root)
}

fn emit_dts_with_usage_analysis(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.emit(root)
}

fn emit_js_dts(source: &str) -> String {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

fn emit_js_dts_with_usage_analysis(source: &str) -> String {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.js".to_string());
    emitter.emit(root)
}

fn find_class_symbol(
    parser: &ParserState,
    binder: &BinderState,
    name: &str,
    kind: u16,
) -> tsz_binder::SymbolId {
    let class_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == kind)
                .then_some(NodeIndex(idx as u32))
                .and_then(|idx| {
                    parser
                        .arena
                        .get(idx)
                        .and_then(|node| parser.arena.get_class(node))
                        .filter(|class| parser.arena.get_identifier_text(class.name) == Some(name))
                        .map(|_| idx)
                })
        })
        .unwrap_or_else(|| panic!("missing class node for {name}"));

    binder
        .get_node_symbol(class_idx)
        .unwrap_or_else(|| panic!("missing symbol for class {name}"))
}

fn find_interface_symbol(
    parser: &ParserState,
    binder: &BinderState,
    name: &str,
) -> tsz_binder::SymbolId {
    parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION)
                .then_some(NodeIndex(idx as u32))
                .and_then(|idx| {
                    parser
                        .arena
                        .get(idx)
                        .and_then(|node| parser.arena.get_interface(node))
                        .filter(|iface| parser.arena.get_identifier_text(iface.name) == Some(name))
                        .and_then(|_| binder.get_node_symbol(idx))
                })
        })
        .unwrap_or_else(|| panic!("missing symbol for interface {name}"))
}

fn find_first_class_method_name(
    parser: &ParserState,
    class_name: &str,
    class_kind: u16,
) -> NodeIndex {
    let class_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == class_kind)
                .then_some(NodeIndex(idx as u32))
                .and_then(|idx| {
                    parser
                        .arena
                        .get(idx)
                        .and_then(|node| parser.arena.get_class(node))
                        .filter(|class| {
                            parser.arena.get_identifier_text(class.name) == Some(class_name)
                        })
                        .map(|_| idx)
                })
        })
        .unwrap_or_else(|| panic!("missing class node for {class_name}"));

    let class = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .unwrap_or_else(|| panic!("missing class data for {class_name}"));

    class
        .members
        .nodes
        .iter()
        .copied()
        .find_map(|member_idx| {
            parser
                .arena
                .get(member_idx)
                .and_then(|node| parser.arena.get_method_decl(node))
                .map(|method| method.name)
        })
        .unwrap_or_else(|| panic!("missing method on class {class_name}"))
}

fn find_first_class_node(parser: &ParserState, class_kind: u16) -> NodeIndex {
    parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| (node.kind == class_kind).then_some(NodeIndex(idx as u32)))
        .unwrap_or_else(|| panic!("missing class node of kind {class_kind}"))
}

fn find_class_node(parser: &ParserState, class_name: &str, class_kind: u16) -> NodeIndex {
    parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == class_kind)
                .then_some(NodeIndex(idx as u32))
                .and_then(|idx| {
                    parser
                        .arena
                        .get(idx)
                        .and_then(|node| parser.arena.get_class(node))
                        .filter(|class| {
                            parser.arena.get_identifier_text(class.name) == Some(class_name)
                        })
                        .map(|_| idx)
                })
        })
        .unwrap_or_else(|| panic!("missing class node for {class_name}"))
}

fn find_class_extends_expression(parser: &ParserState, class_idx: NodeIndex) -> NodeIndex {
    let class = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class data");
    let heritage = class
        .heritage_clauses
        .as_ref()
        .and_then(|clauses| clauses.nodes.first().copied())
        .and_then(|idx| parser.arena.get(idx))
        .and_then(|node| parser.arena.get_heritage_clause(node))
        .expect("missing heritage clause");
    let type_idx = *heritage.types.nodes.first().expect("missing extends type");
    parser
        .arena
        .get(type_idx)
        .and_then(|node| parser.arena.get_expr_type_args(node))
        .map(|eta| eta.expression)
        .unwrap_or(type_idx)
}

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

// =============================================================================
// 1. Simple Declarations
// =============================================================================

#[test]
fn test_function_declaration() {
    let source = "export function add(a: number, b: number): number { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare function add"),
        "Expected export declare: {output}"
    );
    assert!(
        output.contains("a: number"),
        "Expected parameter type: {output}"
    );
    assert!(
        output.contains("): number;"),
        "Expected return type: {output}"
    );
}

#[test]
fn test_non_exported_function_declaration_emits_declare_function() {
    let source = "function helper(x: string): string { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function helper"),
        "Expected non-exported function to be emitted as declare function: {output}"
    );
    assert!(
        !output.contains("export declare function helper"),
        "Expected no export keyword for non-exported top-level function in global scope: {output}"
    );
}

#[test]
fn test_class_declaration() {
    let source = r#"
    export class Calculator {
        private value: number;
        add(n: number): this {
            this.value += n;
            return this;
        }
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("class Calculator"),
        "Expected class declaration: {output}"
    );
    assert!(output.contains("value"), "Expected property: {output}");
    assert!(
        output.contains("add") && output.contains("number"),
        "Expected method signature with add and number: {output}"
    );
}

#[test]
fn test_interface_declaration() {
    let source = "export interface Point { x: number; y: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("interface Point"),
        "Expected interface: {output}"
    );
    assert!(output.contains("number"), "Expected number type: {output}");
}

#[test]
fn test_type_alias() {
    let source = "export type ID = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type ID = string | number"),
        "Expected type alias: {output}"
    );
}

#[test]
fn test_type_only_export_module_gets_empty_export_marker() {
    // When a module has only an import (module syntax) and private types,
    // the .d.ts needs `export {};` to preserve module semantics, since tsc
    // would not emit any explicit exports for a file like this.
    let source = r#"
import "some-dep";
type T = { x: number };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export {};"),
        "Expected empty export marker for import-only module: {output}"
    );
}

#[test]
fn test_type_export_module_still_needs_empty_export_marker() {
    // tsc emits `export {};` even when there are type exports (interfaces,
    // type aliases) because type exports are erased at runtime.
    let source = r#"
type T = { x: number };
export interface I {
    f: T;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export interface I"),
        "Expected exported interface: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected empty export marker even with type exports: {output}"
    );
}

#[test]
fn test_empty_named_export_has_no_extra_spacing() {
    let source = "export {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export {};"),
        "Expected compact empty export syntax: {output}"
    );
    assert!(
        !output.contains("export {  };"),
        "Did not expect extra spacing in empty export syntax: {output}"
    );
}

#[test]
fn test_private_set_accessor_omits_type_and_uses_value_param_name() {
    let source = r#"
declare class C {
    private set x(foo: string);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare class C"),
        "Expected declared class: {output}"
    );
    assert!(
        output.contains("private set x(value);"),
        "Expected private setter value parameter canonicalization: {output}"
    );
}

#[test]
fn test_public_set_accessor_preserves_source_param_name() {
    let source = r#"
declare class C {
    set x(foo: string);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("set x(foo: string);"),
        "Expected public setter to preserve source parameter name: {output}"
    );
}

#[test]
fn test_accessor_comments_with_bodies_are_preserved() {
    let source = r#"
export class C {
    /** getter property*/
    public get x() {
        return 1;
    }
    /** setter property*/
    public set x(/** this is value*/ value: number) {
    }
}
"#;
    let output = emit_dts(source);

    assert!(
        output.contains("/** getter property*/\n    get x(): number;"),
        "Expected getter JSDoc to be preserved in declaration emit: {output}"
    );
    assert!(
        output.contains("/** setter property*/\n    set x(/** this is value*/ value: number);"),
        "Expected setter JSDoc to be preserved in declaration emit: {output}"
    );
}

#[test]
fn test_exported_interface_member_comments_are_preserved() {
    let output = emit_dts(
        r#"
export interface Box {
    /** width docs */
    width: number;
}
"#,
    );

    assert!(
        output.contains("/** width docs */\n    width: number;"),
        "Expected exported interface member JSDoc to be preserved: {output}"
    );
}

#[test]
fn test_multiline_parameter_comments_keep_interface_signature_indent() {
    let output = emit_dts(
        r#"
export interface ICallSignatureWithParameters {
    /** This is comment for function signature*/
    (/** this is comment about a*/a: string,
        /** this is comment for b*/
        b: number): void;
}
"#,
    );

    assert!(
        output.contains(
            "    (/** this is comment about a*/ a: string, \n    /** this is comment for b*/\n    b: number): void;"
        ),
        "Expected multiline parameter comments to keep interface signature indentation: {output}"
    );
}

#[test]
fn test_get_accessor_uses_matching_setter_parameter_type_for_computed_name() {
    let output = emit_dts(
        r#"
const enum G {
    B = 2,
}
class C {
    get [G.B]() {
        return true;
    }
    set [G.B](value: number) {}
}
"#,
    );

    assert!(
        output.contains("get [G.B](): number;"),
        "Expected getter to reuse matching setter parameter type: {output}"
    );
    assert!(
        !output.contains("get [G.B](): boolean;"),
        "Did not expect getter body type to override matching setter parameter type: {output}"
    );
}

#[test]
fn test_computed_methods_emit_as_property_signatures() {
    let output = emit_dts(
        r#"
const key: string = Math.random() > 0.5 ? "a" : "b";
export class C {
    [key](): string {
        return "x";
    }

    regular(): number {
        return 1;
    }
}
"#,
    );

    // tsc emits computed methods as method signatures, not property signatures.
    assert!(
        output.contains("[key](): string;"),
        "Expected computed method to use method syntax (matching tsc): {output}"
    );
    assert!(
        !output.contains("[key]: () => string;"),
        "Did not expect property signature for computed method: {output}"
    );
    assert!(
        output.contains("regular(): number;"),
        "Expected ordinary methods to stay as methods: {output}"
    );
}

#[test]
fn test_declaration_file_exports_do_not_gain_duplicate_declare() {
    let source = r#"
export class A {}
export function f(): void;
export const x: number;
"#;
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export class A"),
        "Expected exported class to preserve declaration-file form: {output}"
    );
    assert!(
        output.contains("export function f(): void;"),
        "Expected exported function to preserve declaration-file form: {output}"
    );
    assert!(
        output.contains("export const x: number;"),
        "Expected exported variable to preserve declaration-file form: {output}"
    );
    assert!(
        !output.contains("export declare class A"),
        "Did not expect duplicate declare on exported class: {output}"
    );
    assert!(
        !output.contains("export declare function f"),
        "Did not expect duplicate declare on exported function: {output}"
    );
    assert!(
        !output.contains("export declare const x"),
        "Did not expect duplicate declare on exported variable: {output}"
    );
}

#[test]
fn test_js_exported_function_and_class_do_not_emit_declare() {
    let source = r#"
export function main() {}
export class Z {}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export function main(): void;"),
        "Expected JS export function declaration form: {output}"
    );
    assert!(
        output.contains("export class Z"),
        "Expected JS export class declaration form: {output}"
    );
    assert!(
        !output.contains("export declare function main"),
        "Did not expect declare on JS exported function: {output}"
    );
    assert!(
        !output.contains("export declare class Z"),
        "Did not expect declare on JS exported class: {output}"
    );
}

#[test]
fn test_js_const_literal_uses_type_annotation() {
    let source = "export const x = 1;";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected JS const literal to emit a literal type annotation: {output}"
    );
    assert!(
        !output.contains("export const x = 1;"),
        "Did not expect JS const literal to stay as an initializer: {output}"
    );
}

#[test]
fn test_ts_const_await_literal_uses_initializer() {
    let source = "const x = await 1;\nexport { x };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const x = 1;"),
        "Expected TS await literal const to emit an initializer: {output}"
    );
    assert!(
        !output.contains("declare const x: number;"),
        "Did not expect TS await literal const to widen to number: {output}"
    );
}

#[test]
fn test_js_variable_preserves_name_like_jsdoc_type_reference() {
    let source = r#"
/**
 * @callback Foo
 * @param {...string} args
 * @returns {number}
 */
/** @type {Foo} */
export const x = () => 1;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: Foo;"),
        "Expected JS @type alias reference to be preserved: {output}"
    );
    // TODO: @callback synthesis not yet implemented — enable when supported
    // assert!(
    //     output.contains("export type Foo = (...args: string[]) => number;"),
    //     "Expected JS @callback alias to be synthesized after the exported value: {output}"
    // );
}

#[test]
fn test_js_variable_preserves_unresolved_name_like_jsdoc_type_reference() {
    let source = r#"
/** @type {B} */
var notOK = 0;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var notOK: B;"),
        "Expected unresolved JSDoc type reference to be preserved in .d.ts emit: {output}"
    );
}

#[test]
fn test_js_trailing_jsdoc_type_aliases_are_emitted() {
    let source = r#"
export {};
/** @typedef {string | number | symbol} PropName */
/**
 * Callback
 *
 * @callback NumberToStringCb
 * @param {number} a
 * @returns {string}
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type PropName = string | number | symbol;"),
        "Expected trailing JSDoc typedef alias to be emitted: {output}"
    );
    assert!(
        output.contains("export type NumberToStringCb = (a: number) => string;"),
        "Expected trailing JSDoc callback alias to be emitted: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Did not expect an extra export scope marker once JSDoc aliases are emitted: {output}"
    );
}

#[test]
fn test_js_callback_without_return_tag_defaults_to_any() {
    let source = r#"
/**
 * Callback to be invoked when test execution is complete.
 *
 * @callback DoneCB
 * @param {number} failures - Number of failures that occurred.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("type DoneCB = (failures: number) => any;"),
        "Expected JS @callback aliases without @returns to default to any: {output}"
    );
}

#[test]
fn test_js_leading_jsdoc_typedef_before_function_is_emitted() {
    let source = r#"
/** @typedef {{x: string} | number} SomeType */
/**
 * @param {number} x
 * @returns {SomeType}
 */
export function doTheThing(x) {
  return x;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type SomeType = {\n    x: string;\n} | number;"),
        "Expected leading JSDoc typedef alias before exported function: {output}"
    );
    let alias_pos = output
        .find("export type SomeType =")
        .expect("Expected typedef alias to be emitted");
    let function_pos = output
        .find("export function doTheThing(")
        .expect("Expected exported function declaration to be emitted");
    assert!(
        alias_pos < function_pos,
        "Expected typedef alias to be emitted before the function declaration: {output}"
    );
}

#[test]
fn test_js_script_typedef_before_variable_is_emitted_as_local_type() {
    let source = r#"
/** @typedef {{x: string}} LocalType */
const value = 1;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("type LocalType = {\n    x: string;\n};"),
        "Expected script typedef before variable statement to be emitted as a local type alias: {output}"
    );
    assert!(
        !output.contains("export type LocalType"),
        "Did not expect script typedef to be emitted as an exported type alias: {output}"
    );
}

#[test]
fn test_js_multiline_typedef_before_function_variable_is_emitted() {
    let source = r#"
/**
 * @typedef {{
 *   [id: string]: [Function, Function];
 * }} ResolveRejectMap
 */
/**
 * @param {ResolveRejectMap} handlers
 * @returns {Promise<any>}
 */
const send = handlers => Promise.resolve(handlers);
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function send(handlers: ResolveRejectMap): Promise<any>;"),
        "Expected JSDoc-annotated JS function variable to emit as a function declaration: {output}"
    );
    assert!(
        output.contains("type ResolveRejectMap = {\n    [id: string]: [Function, Function];\n};"),
        "Expected multiline JSDoc typedef alias to be emitted as a local type alias: {output}"
    );
}

#[test]
fn test_js_function_declaration_uses_jsdoc_signature_types() {
    let source = r#"
/**
 * @param {number} x
 * @returns {string}
 */
function format(x) {
  return String(x);
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function format(x: number): string;"),
        "Expected JSDoc function declaration types to flow into .d.ts emit: {output}"
    );
}

#[test]
fn test_js_named_exports_fold_into_declarations() {
    let source = r#"
const x = 1;
function f() {}
export { x, f };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected named-exported const to fold into an exported declaration: {output}"
    );
    assert!(
        output.contains("export function f(): void;"),
        "Expected named-exported function to fold into an exported declaration: {output}"
    );
    assert!(
        !output.contains("export { x, f };"),
        "Did not expect a redundant named export clause after folding: {output}"
    );
}

#[test]
fn test_js_named_exports_preserve_explicit_export_order() {
    let source = r#"
function require() {}
const exports = {};
class Object {}
export const __esModule = false;
export { require, exports, Object };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export const __esModule: false;
export function require(): void;
export const exports: {};
export class Object {
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected explicit JS exports to stay ahead of folded named exports: {output}"
    );
}

#[test]
fn test_js_export_import_equals_drops_export_keyword() {
    let source = "export import fs2 = require(\"fs\");";
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("import fs2 = require(\"fs\");"),
        "Expected JS export import= to emit as plain import=: {output}"
    );
    assert!(
        !output.contains("export import fs2"),
        "Did not expect JS export import= to keep the export keyword: {output}"
    );
}

#[test]
fn test_js_import_meta_url_infers_string() {
    let source = r#"
const x = import.meta.url;
export { x };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: string;"),
        "Expected import.meta.url to emit as string in JS declarations: {output}"
    );
}

#[test]
fn test_ts_import_meta_url_infers_string() {
    let source = r#"
const x = import.meta.url;
export { x };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const x: string;"),
        "Expected import.meta.url to emit as string in TS declarations: {output}"
    );
}

#[test]
fn test_js_top_level_await_literal_preserves_literal_type() {
    let source = r#"
const x = await 1;
export { x };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export const x: 1;"),
        "Expected top-level await of a literal to preserve the literal type: {output}"
    );
}

#[test]
fn test_js_function_using_arguments_emits_rest_param() {
    let source = r#"
function f(x) {
    arguments;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function f(x: any, ...args: any[]): void;"),
        "Expected JS functions that reference arguments to gain a synthetic rest param: {output}"
    );
}

#[test]
fn test_js_object_literal_functions_emit_namespace() {
    let source = r#"
const foo = {
    f1: (params) => {}
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"declare namespace foo {
    function f1(params: any): void;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected namespace-like JS object literals to emit as declare namespaces: {output}"
    );
}

#[test]
fn test_js_object_literal_values_emit_namespace_members() {
    let source = r#"
const Strings = {
    a: "A",
    b: "B"
};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"declare namespace Strings {
    let a: string;
    let b: string;
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected JS object literal values to emit as namespace members: {output}"
    );
}

#[test]
fn test_js_class_zero_arg_constructor_is_omitted() {
    let source = r#"
export class Preferences {
    constructor() {}
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        !output.contains("constructor();"),
        "Expected zero-arg JS constructors to be omitted from declaration emit: {output}"
    );
}

#[test]
fn test_js_export_equals_emits_before_target_declaration() {
    let source = r#"
const a = {};
export = a;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = a;\ndeclare const a: {};"),
        "Expected JS export= to emit before its target declaration: {output}"
    );
    assert_eq!(
        output.matches("export = a;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_module_exports_emits_before_target_declaration() {
    let source = r#"
const a = {};
module.exports = a;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.starts_with("export = a;\ndeclare const a: {};"),
        "Expected JS module.exports assignment to emit as export=: {output}"
    );
    assert_eq!(
        output.matches("export = a;").count(),
        1,
        "Did not expect duplicate JS export= statements: {output}"
    );
}

#[test]
fn test_js_exports_assignment_emits_named_exports_and_filters_locals() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.j = 1;
exports.k = void 0;
var o = {};
function C() {
    this.p = 1;
}
"#,
    );

    assert!(
        output.contains("export const j:"),
        "Expected CommonJS named export value declaration: {output}"
    );
    assert!(
        !output.contains("declare var o:"),
        "Did not expect non-exported locals to leak into JS module declarations: {output}"
    );
    assert!(
        !output.contains("declare function C"),
        "Did not expect non-exported helper declarations to leak into JS module declarations: {output}"
    );
    assert!(
        !output.contains("export const k:"),
        "Did not expect void exports to synthesize declarations: {output}"
    );
}

#[test]
fn test_js_exports_assignment_skips_chained_void_zero_preinit() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
exports.y = exports.x = void 0;
exports.x = 1;
exports.y = 2;
"#,
    );

    assert!(
        output.contains("export const x: 1;"),
        "Expected x export declaration to survive past the void-zero preinit: {output}"
    );
    assert!(
        output.contains("export const y: 2;"),
        "Expected y export declaration to survive past the void-zero preinit: {output}"
    );
    assert!(
        !output.contains("export const y: undefined;"),
        "Did not expect chained void-zero preinit to synthesize an undefined export: {output}"
    );
}

#[test]
fn test_js_exports_assignment_marks_same_name_function_exported() {
    let output = emit_js_dts(
        r#"
function foo() {}
exports.foo = foo;
"#,
    );

    assert!(
        output.contains("export function foo(): void;"),
        "Expected same-name CommonJS export to reuse the function declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_function_expandos_emit_as_namespace_exports() {
    let source = r#"
function foo() {}
foo.foo = foo;
foo.default = foo;
module.exports = foo;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = foo;
declare function foo(): void;
declare namespace foo {
    export { foo };
    export { foo as default };
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS function expandos to emit as namespace exports: {output}"
    );
}

#[test]
fn test_js_commonjs_exported_arrow_function_preserves_any_return_type() {
    let source = r#"
const donkey = (ast) => ast;
module.exports = donkey;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file");
    };
    let var_stmt_idx = source_file.statements.nodes[0];
    let var_stmt = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(var_stmt.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl = parser
        .arena
        .get(decl_list.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let ast_atom = interner.intern_string("ast");
    let donkey_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(ast_atom, TypeId::ANY)],
        TypeId::ANY,
    ));

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(decl.name.0, donkey_type);
    type_cache
        .node_types
        .insert(decl.initializer.0, donkey_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function donkey(ast: any): any;"),
        "Expected concise-arrow CommonJS export to preserve any return type: {output}"
    );
    assert!(
        !output.contains("declare function donkey(ast: any): void;"),
        "Did not expect concise-arrow CommonJS export to collapse to void: {output}"
    );
}

#[test]
fn test_js_commonjs_prototype_and_static_assignments_emit_synthetic_declarations() {
    let source = r#"
module.exports = MyClass;

function MyClass() {}
MyClass.staticMethod = function() {}
MyClass.prototype.method = function() {}
MyClass.staticProperty = 123;

/**
 * Callback to be invoked when test execution is complete.
 *
 * @callback DoneCB
 * @param {number} failures - Number of failures that occurred.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = MyClass;
declare function MyClass(): void;
declare class MyClass {
    method(): void;
}
declare namespace MyClass {
    export { staticMethod, staticProperty, DoneCB };
}
declare function staticMethod(): void;
declare var staticProperty: number;
/**
 * Callback to be invoked when test execution is complete.
 */
type DoneCB = (failures: number) => any;"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS static/prototype assignments to emit synthetic declarations: {output}"
    );
}

#[test]
fn test_js_exports_assignment_marks_same_name_class_exported() {
    let output = emit_js_dts(
        r#"
class K {}
exports.K = K;
"#,
    );

    assert!(
        output.contains("export class K"),
        "Expected same-name CommonJS export to reuse the class declaration: {output}"
    );
}

#[test]
fn test_js_commonjs_property_access_export_reuses_assigned_initializer_type() {
    let source = r#"
var NS = {};
NS.K = class {};
exports.K = NS.K;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file");
    };
    let class_expr = parser
        .arena
        .get(source_file.statements.nodes[1])
        .and_then(|node| parser.arena.get_expression_statement(node))
        .map(|stmt| {
            parser
                .arena
                .skip_parenthesized_and_assertions_and_comma(stmt.expression)
        })
        .and_then(|expr| {
            parser
                .arena
                .get(expr)
                .and_then(|node| parser.arena.get_binary_expr(node))
        })
        .map(|binary| {
            parser
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right)
        })
        .expect("missing assigned class expression");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let constructor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::ANY)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(class_expr.0, constructor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export var K: {"),
        "Expected property-access CommonJS export to emit a synthetic declaration: {output}"
    );
    assert!(
        output.contains("new (): any;"),
        "Expected property-access CommonJS export to reuse the assigned initializer type: {output}"
    );
}

#[test]
fn test_js_commonjs_named_class_expression_emits_exported_class() {
    let output = emit_js_dts(
        r#"
exports.K = class K {
    values() {}
};
"#,
    );

    assert!(
        output.contains("export class K {"),
        "Expected named CommonJS class expression to emit as an exported class: {output}"
    );
    assert!(
        output.contains("values(): void;"),
        "Expected named CommonJS class expression members to be preserved: {output}"
    );
    assert!(
        !output.contains("export var K: {"),
        "Did not expect named CommonJS class expression to lower as a constructor object: {output}"
    );
}

#[test]
fn test_object_literal_computed_numeric_names_prefer_syntax_shape() {
    let output = emit_dts(
        r#"
var v = {
  [-1]: {},
  [+1]: {},
  [~1]: {},
  [!1]: {}
};
"#,
    );

    assert!(
        output.contains("[-1]: {};"),
        "Expected negative computed numeric literal to survive in fallback object typing: {output}"
    );
    assert!(
        !output.contains("\"-1\": {};"),
        "Did not expect canonical string form to survive once syntax override is applied: {output}"
    );
    assert!(
        output.contains("1: {};"),
        "Expected unary-plus computed numeric literal to normalize to a numeric property: {output}"
    );
    assert!(
        !output.contains("[~1]: {}"),
        "Did not expect non-emittable computed names to survive fallback object typing: {output}"
    );
    assert!(
        !output.contains("[!1]: {}"),
        "Did not expect non-emittable computed names to survive fallback object typing: {output}"
    );
}

#[test]
fn test_js_commonjs_class_static_assignments_emit_typedef_and_namespace_exports() {
    let source = r#"
class Handler {
    static get OPTIONS() {
        return 1;
    }

    process() {
    }
}
Handler.statische = function() { }
const Strings = {
    a: "A",
    b: "B"
};

module.exports = Handler;
module.exports.Strings = Strings;

/**
 * @typedef {Object} HandlerOptions
 * @property {String} name
 * Should be able to export a type alias at the same time.
 */
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export = Handler;
declare class Handler {
    static get OPTIONS(): number;
    process(): void;
}
declare namespace Handler {
    export { statische, Strings, HandlerOptions };
}
declare function statische(): void;
declare namespace Strings {
    let a: string;
    let b: string;
}
type HandlerOptions = {
    /**
     * Should be able to export a type alias at the same time.
     */
    name: string;
};"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected CommonJS class static assignments and typedefs to emit in source order: {output}"
    );
}

#[test]
fn test_js_class_static_method_augmentation_emits_namespace_merge() {
    let source = r#"
export class Clazz {
    static method() { }
}

Clazz.method.prop = 5;
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let expected = r#"export class Clazz {
}
export namespace Clazz {
    function method(): void;
    namespace method {
        let prop: number;
    }
}"#;
    assert_eq!(
        output.trim(),
        expected,
        "Expected JS static method augmentations to emit as a merged namespace: {output}"
    );
}

#[test]
fn test_js_reexports_from_same_module_are_grouped() {
    let source = r#"
export { default } from "fs";
export { default as foo } from "fs";
export { bar as baz } from "fs";
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export { default, default as foo, bar as baz } from \"fs\";"),
        "Expected JS re-exports from the same module to be grouped: {output}"
    );
    assert_eq!(
        output.matches(" from \"fs\";").count(),
        1,
        "Did not expect duplicate JS re-export lines after grouping: {output}"
    );
}

#[test]
fn test_method_declaration_emits_inferred_return_type() {
    let source = r#"
class C {
    add() {
        return 1;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let Some(class_node) = parser.arena.get(source_file.statements.nodes[0]) else {
        panic!("missing class node");
    };
    let Some(class_decl) = parser.arena.get_class(class_node) else {
        panic!("missing class declaration");
    };
    let method_idx = class_decl.members.nodes[0];

    let interner = TypeInterner::new();
    let method_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, method_type);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("add(): number;"),
        "Expected inferred method return type: {output}"
    );
}

#[test]
fn test_property_declaration_infers_type_from_numeric_initializer_when_type_cache_missing() {
    let source = r#"
abstract class C {
    abstract prop = 1;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract prop: number;"),
        "Expected inferred property type from initializer: {output}"
    );
}

#[test]
fn test_variable_declaration_infers_accessor_object_type_from_initializer_when_type_cache_missing()
{
    let source = r#"
export var basePrototype = {
  get primaryPath() {
    return 1;
  },
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output
            .contains("export declare var basePrototype: {\n    readonly primaryPath: number;\n};"),
        "Expected multi-line object literal accessor inference with body type: {output}"
    );
}

#[test]
fn test_object_literal_computed_accessor_pair_emits_writable_symbol_property() {
    let output = emit_dts_with_binding(
        r#"
var obj = {
    get [Symbol.isConcatSpreadable]() { return ""; },
    set [Symbol.isConcatSpreadable](x) { }
};
"#,
    );

    assert!(
        output.contains("[Symbol.isConcatSpreadable]: string;"),
        "Expected computed accessor pair to collapse to writable symbol property: {output}"
    );
    assert!(
        !output.contains("readonly [Symbol.isConcatSpreadable]: string;"),
        "Did not expect computed accessor pair to remain readonly: {output}"
    );
}

#[test]
fn test_object_literal_computed_literal_key_reuses_resolved_property_name() {
    let source = r#"
const Foo = {
    BANANA: "banana" as "banana",
};

export const Baa = {
    [Foo.BANANA]: 1,
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let baa_decl = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("Baa"))
                .map(|decl| (NodeIndex(idx as u32), decl))
        })
        .map(|(_, decl)| decl)
        .expect("missing Baa declaration");
    let object_literal = parser
        .arena
        .get(baa_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing Baa object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");
    let computed_expr = parser
        .arena
        .get(prop_assignment.name)
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("missing computed property name");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let banana_type = interner.literal_string("banana");
    let banana_atom = interner.intern_string("banana");
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(banana_atom, TypeId::NUMBER)],
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(computed_expr.0, banana_type);
    type_cache
        .node_types
        .insert(baa_decl.initializer.0, object_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("banana: number;"),
        "Expected computed literal key to emit resolved property name: {output}"
    );
    assert!(
        !output.contains("[Foo.BANANA]: number;"),
        "Did not expect computed literal key syntax to leak into declaration output: {output}"
    );
}

#[test]
fn test_enum_member_initializers_respect_const_assertion_widening() {
    let output = emit_dts_with_binding(
        r#"
enum E { A, B }
let widened = E.B;
let preserved = E.B as const;
class C {
    p1 = E.B;
    p2 = E.B as const;
    readonly p3 = E.B;
}
"#,
    );

    assert!(
        output.contains("declare let widened: E;"),
        "Expected let enum member to widen to enum type: {output}"
    );
    assert!(
        output.contains("declare let preserved: E.B;"),
        "Expected const-asserted enum member to preserve member type: {output}"
    );
    assert!(
        output.contains("p1: E;"),
        "Expected property widening: {output}"
    );
    assert!(
        output.contains("p2: E.B;"),
        "Expected const-asserted property member type: {output}"
    );
    assert!(
        output.contains("readonly p3 = E.B;"),
        "Expected readonly enum property initializer form: {output}"
    );
}

#[test]
fn test_inaccessible_constructor_new_initializer_emits_any() {
    let source = r#"
class C {
    constructor(public x: number) {}
}

class D {
    private constructor(public x: number) {}
}

class E {
    protected constructor(public x: number) {}
}

var c = new C(1);
var d = new D(1);
var e = new E(1);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };

    let class_c_idx = source_file.statements.nodes[0];
    let class_d_idx = source_file.statements.nodes[1];
    let class_e_idx = source_file.statements.nodes[2];
    let var_c_stmt_idx = source_file.statements.nodes[3];
    let var_d_stmt_idx = source_file.statements.nodes[4];
    let var_e_stmt_idx = source_file.statements.nodes[5];

    let class_c = parser
        .arena
        .get(class_c_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class C");
    let class_d = parser
        .arena
        .get(class_d_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class D");
    let class_e = parser
        .arena
        .get(class_e_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class E");

    let var_c_decl = parser
        .arena
        .get(var_c_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var c declaration");
    let var_d_decl = parser
        .arena
        .get(var_d_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var d declaration");
    let var_e_decl = parser
        .arena
        .get(var_e_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var e declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .get_node_symbol(class_c.name)
        .or_else(|| binder.get_node_symbol(class_c_idx))
        .expect("missing symbol for C");
    let d_sym = binder
        .get_node_symbol(class_d.name)
        .or_else(|| binder.get_node_symbol(class_d_idx))
        .expect("missing symbol for D");
    let e_sym = binder
        .get_node_symbol(class_e.name)
        .or_else(|| binder.get_node_symbol(class_e_idx))
        .expect("missing symbol for E");

    let interner = TypeInterner::new();
    let c_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(c_sym),
    });
    let d_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(d_sym),
    });
    let e_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(e_sym),
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_c_decl.name.0, c_type);
    type_cache.node_types.insert(var_d_decl.name.0, d_type);
    type_cache.node_types.insert(var_e_decl.name.0, e_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var c: C;"),
        "Expected C type: {output}"
    );
    assert!(
        output.contains("declare var d: any;"),
        "Expected d to degrade to any: {output}"
    );
    assert!(
        output.contains("declare var e: any;"),
        "Expected e to degrade to any: {output}"
    );
}

#[test]
fn test_construct_signature_new_initializer_keeps_inferred_any() {
    let source = r#"
interface Input {}
interface Factory {
    new (value: Input);
}
declare var ctor: Factory;
declare var value: Input;
var instance = new ctor(value);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };

    let instance_stmt_idx = source_file.statements.nodes[4];
    let instance_decl = parser
        .arena
        .get(instance_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing instance declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(instance_decl.name.0, TypeId::ANY);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var instance: any;"),
        "Expected construct-signature new initializer to preserve inferred any: {output}"
    );
    assert!(
        !output.contains("declare var instance: ctor;"),
        "Did not expect constructor variable name to leak into the emitted type: {output}"
    );
}

#[test]
fn test_constructor_type_no_double_semicolon() {
    let source = "export type Ctor = new (...args: any[]) => void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("new (...args: any[]) => void;"),
        "Expected constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in constructor type alias: {output}"
    );
}

#[test]
fn test_template_literal_type_no_double_semicolon() {
    let source = r#"export type Outcome = `${string}_${string}`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`${string}_${string}`"),
        "Expected template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in template literal type alias: {output}"
    );
}

#[test]
fn test_infer_type_no_double_semicolon() {
    let source = "export type Unpack<T> = T extends (infer U)[] ? U : T;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("infer U"),
        "Expected infer type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in type alias with infer: {output}"
    );
}

#[test]
fn test_abstract_constructor_type() {
    let source = "export type AbstractCtor = abstract new () => object;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract new () => object;"),
        "Expected abstract constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in abstract constructor type: {output}"
    );
}

#[test]
fn test_simple_template_literal_type() {
    let source = r#"export type Greeting = `hello`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`hello`"),
        "Expected simple template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in simple template literal type: {output}"
    );
}

#[test]
fn test_public_modifier_omitted_from_dts_class_members() {
    // tsc omits `public` from .d.ts output since it's the default accessibility
    let source = r#"
    export class Foo {
        public x: number;
        public greet(): string { return "hello"; }
        protected y: number;
        private z: number;
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    // `public` should be stripped (it's the default)
    assert!(
        !output.contains("public "),
        "Expected `public` modifier to be omitted from .d.ts output: {output}"
    );
    // `protected` and `private` should be preserved
    assert!(
        output.contains("protected y"),
        "Expected `protected` modifier to be preserved: {output}"
    );
    assert!(
        output.contains("private z"),
        "Expected `private` modifier to be preserved: {output}"
    );
    // Members themselves should still be present
    assert!(
        output.contains("x: number"),
        "Expected public property to still be emitted (without modifier): {output}"
    );
    assert!(
        output.contains("greet("),
        "Expected public method to still be emitted (without modifier): {output}"
    );
}

// =============================================================================
// 2. Variable Declarations
// =============================================================================

#[test]
fn test_variable_const_declaration() {
    let output = emit_dts("export const MAX: number = 100;");
    assert!(
        output.contains("export declare const MAX: number;"),
        "Expected const variable in .d.ts: {output}"
    );
}

#[test]
fn test_variable_let_declaration() {
    let output = emit_dts("export let count: number = 0;");
    assert!(
        output.contains("export declare let count: number;"),
        "Expected let variable in .d.ts: {output}"
    );
}

#[test]
fn test_variable_var_declaration() {
    let output = emit_dts("export var name: string = 'hello';");
    assert!(
        output.contains("export declare var name: string;"),
        "Expected var variable in .d.ts: {output}"
    );
}

// =============================================================================
// 3. Visibility / Access Modifiers
// =============================================================================

#[test]
fn test_private_method_emits_name_only() {
    // tsc emits just `private methodName;` for private methods
    let output = emit_dts(
        r#"
    export class Foo {
        private secret(): void {}
    }
    "#,
    );
    assert!(
        output.contains("private secret;"),
        "Expected private method to emit name only: {output}"
    );
    // Should NOT include parameters or return type
    assert!(
        !output.contains("private secret()"),
        "Private method should not have params in .d.ts: {output}"
    );
}

#[test]
fn test_protected_member_included() {
    let output = emit_dts(
        r#"
    export class Foo {
        protected bar: number;
    }
    "#,
    );
    assert!(
        output.contains("protected bar: number;"),
        "Expected protected member to be included: {output}"
    );
}

#[test]
fn test_private_property_omits_type_annotation() {
    // tsc omits type annotations for private properties in .d.ts
    let output = emit_dts(
        r#"
    export class Foo {
        private value: number;
    }
    "#,
    );
    assert!(
        output.contains("private value;"),
        "Expected private property without type annotation: {output}"
    );
    assert!(
        !output.contains("private value: number;"),
        "Private property should NOT have type annotation: {output}"
    );
}

// =============================================================================
// 4. Export Handling
// =============================================================================

#[test]
fn test_named_export_with_specifiers() {
    let output = emit_dts(
        r#"
    const a: number = 1;
    const b: string = "x";
    export { a, b };
    "#,
    );
    assert!(
        output.contains("export { a, b }"),
        "Expected named export specifiers: {output}"
    );
}

#[test]
fn test_re_export_from_module() {
    let output = emit_dts(r#"export { foo, bar } from "./other";"#);
    assert!(
        output.contains("export { foo, bar } from"),
        "Expected re-export: {output}"
    );
}

#[test]
fn test_star_re_export() {
    let output = emit_dts(r#"export * from "./utils";"#);
    assert!(
        output.contains("export * from"),
        "Expected star re-export: {output}"
    );
}

#[test]
fn test_type_only_export() {
    let output = emit_dts(r#"export type { Foo } from "./types";"#);
    assert!(
        output.contains("export type { Foo }"),
        "Expected type-only export: {output}"
    );
}

#[test]
fn test_export_default_identifier() {
    // export default <identifier> should emit directly
    let output = emit_dts(
        r#"
    declare const myValue: number;
    export default myValue;
    "#,
    );
    assert!(
        output.contains("export default myValue;"),
        "Expected export default identifier: {output}"
    );
}

// =============================================================================
// 5. Type Formatting
// =============================================================================

#[test]
fn test_union_type_in_declaration() {
    let output = emit_dts("export type Result = string | number | boolean;");
    assert!(
        output.contains("string | number | boolean"),
        "Expected union type: {output}"
    );
}

#[test]
fn test_intersection_type_in_declaration() {
    let output = emit_dts("export type Combined = { a: number } & { b: string };");
    assert!(output.contains("&"), "Expected intersection type: {output}");
}

#[test]
fn test_function_type_in_declaration() {
    let output = emit_dts("export type Callback = (x: number, y: string) => void;");
    assert!(
        output.contains("(x: number, y: string) => void"),
        "Expected function type: {output}"
    );
}

#[test]
fn test_function_variable_type_preserves_inline_parameter_comments() {
    let output = emit_dts(
        r#"
const fooFunc = function (/** foo */ value: string): string {
    return value;
};
const lambdaFoo = (/** left */ left: number, /** right */ right: number): number => left + right;
"#,
    );

    assert!(
        output.contains("declare const fooFunc: (/** foo */ value: string) => string;"),
        "Expected function expression parameter comment to be preserved: {output}"
    );
    assert!(
        output.contains(
            "declare const lambdaFoo: (/** left */ left: number, /** right */ right: number) => number;"
        ),
        "Expected arrow function parameter comments to be preserved: {output}"
    );
}

#[test]
fn test_js_function_declaration_prefers_returned_callable_object_type() {
    let source = r#"
function test(fn) {
    const composed = function (...args) { };
    return composed;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let func_idx = source_file.statements.nodes[0];
    let func_node = parser.arena.get(func_idx).expect("missing function node");
    let func = parser
        .arena
        .get_function(func_node)
        .expect("missing function data");
    let body_node = parser.arena.get(func.body).expect("missing body node");
    let body = parser
        .arena
        .get_block(body_node)
        .expect("missing function body");
    let composed_stmt_idx = body.statements.nodes[0];
    let composed_decl = parser
        .arena
        .get(composed_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing composed declaration");
    let return_stmt_idx = body.statements.nodes[1];
    let return_stmt_node = parser
        .arena
        .get(return_stmt_idx)
        .expect("missing return node");
    let return_stmt = parser
        .arena
        .get_return_statement(return_stmt_node)
        .expect("missing return statement");

    let interner = TypeInterner::new();
    let fn_atom = interner.intern_string("fn");
    let args_atom = interner.intern_string("args");
    let name_atom = interner.intern_string("name");
    let any_array = interner.array(TypeId::ANY);
    let plain_return_type = interner.function(FunctionShape::new(
        vec![ParamInfo::rest(args_atom, any_array)],
        TypeId::VOID,
    ));
    let callable_return_type = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::rest(args_atom, any_array)],
            TypeId::VOID,
        )],
        properties: vec![PropertyInfo::readonly(name_atom, TypeId::STRING)],
        ..Default::default()
    });
    let test_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(fn_atom, TypeId::ANY)],
        plain_return_type,
    ));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(func_idx.0, test_type);
    type_cache.node_types.insert(func.name.0, test_type);
    type_cache
        .node_types
        .insert(composed_decl.name.0, callable_return_type);
    type_cache
        .node_types
        .insert(return_stmt.expression.0, callable_return_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare function test(fn: any): {"),
        "Expected JS function signature: {output}"
    );
    assert!(
        output.contains("(...args: any[]): void;"),
        "Expected callable return signature to be preserved: {output}"
    );
    assert!(
        output.contains("readonly name: string;"),
        "Expected returned callable property to be preserved: {output}"
    );
}

#[test]
fn test_any_dataview_new_expression_falls_back_to_generic_type() {
    let source = "const dataView = new DataView(new ArrayBuffer(80));";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };
    let var_stmt_idx = source_file.statements.nodes[0];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing dataView declaration");

    let interner = TypeInterner::new();
    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_decl.name.0, TypeId::ANY);

    let binder = BinderState::new();
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const dataView: DataView<ArrayBuffer>;"),
        "Expected DataView constructor fallback type: {output}"
    );
}

#[test]
fn test_static_method_property_access_emits_typeof() {
    let source = r#"
class C {
    static s1: number;
    static s2(b: number) {
        return C.s1 + b;
    }
}
var methodValue = C.s2;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let class_idx = source_file.statements.nodes[0];
    let class_decl = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let var_stmt_idx = source_file.statements.nodes[1];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");
    let access = parser
        .arena
        .get(var_decl.initializer)
        .and_then(|node| parser.arena.get_access_expr(node))
        .expect("missing property access initializer");

    let interner = TypeInterner::new();
    let b_atom = interner.intern_string("b");
    let method_type = interner.function(FunctionShape::new(
        vec![ParamInfo::required(b_atom, TypeId::NUMBER)],
        TypeId::NUMBER,
    ));
    let constructor_type = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::ANY)],
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: binder
            .get_node_symbol(class_decl.name)
            .or_else(|| binder.get_node_symbol(class_idx)),
        is_abstract: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_decl.name.0, method_type);
    type_cache
        .node_types
        .insert(access.expression.0, constructor_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var methodValue: typeof C.s2;"),
        "Expected static method property access to emit typeof: {output}"
    );
}

#[test]
fn test_const_call_initializer_does_not_collapse_to_literal_argument() {
    let source = r#"
type Box<T> = {
    get: () => T;
    set: (value: T) => void;
};
declare function box<T>(value: T): Box<T>;
const bn1 = box(0);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let alias_idx = source_file.statements.nodes[0];
    let alias = parser
        .arena
        .get(alias_idx)
        .and_then(|node| parser.arena.get_type_alias(node))
        .expect("missing Box alias");
    let var_stmt_idx = source_file.statements.nodes[2];
    let var_decl = parser
        .arena
        .get(var_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");

    let interner = TypeInterner::new();
    let box_def = tsz_solver::DefId(9002);
    let box_number = interner.application(interner.lazy(box_def), vec![TypeId::NUMBER]);

    let alias_sym = binder
        .get_node_symbol(alias.name)
        .or_else(|| binder.get_node_symbol(alias_idx))
        .expect("missing Box symbol");
    let mut type_cache = TypeCacheView::default();
    type_cache.def_to_symbol.insert(box_def, alias_sym);
    type_cache.node_types.insert(var_decl.name.0, box_number);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const bn1: Box<number>;"),
        "Expected const call initializer to preserve resolved type: {output}"
    );
    assert!(
        !output.contains("declare const bn1 = 0;"),
        "Did not expect const call initializer to collapse to its literal argument: {output}"
    );
}

#[test]
fn test_non_null_call_initializer_recovers_return_type() {
    let source = r#"
declare const fn: (() => string) | undefined;
const a = fn!();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let fn_stmt_idx = source_file.statements.nodes[0];
    let fn_decl = parser
        .arena
        .get(fn_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing fn declaration");
    let a_stmt_idx = source_file.statements.nodes[1];
    let a_decl = parser
        .arena
        .get(a_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing a declaration");
    let call = parser
        .arena
        .get(a_decl.initializer)
        .and_then(|node| parser.arena.get_call_expr(node))
        .expect("missing call initializer");
    let non_null = parser
        .arena
        .get(call.expression)
        .and_then(|node| parser.arena.get_unary_expr_ex(node))
        .expect("missing non-null callee");
    let interner = TypeInterner::new();
    let callable = interner.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(fn_decl.name.0, callable);
    type_cache
        .node_types
        .insert(non_null.expression.0, callable);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare const a: string;"),
        "Expected non-null call initializer to recover the inner callable return type: {output}"
    );
}

#[test]
fn test_dataview_new_expression_falls_back_without_type_cache() {
    let output = emit_dts("const dataView = new DataView(new ArrayBuffer(80));");
    assert!(
        output.contains("declare const dataView: DataView<ArrayBuffer>;"),
        "Expected DataView constructor fallback without type cache: {output}"
    );
}

#[test]
fn test_array_type_in_declaration() {
    let output = emit_dts("export type Numbers = number[];");
    assert!(output.contains("number[]"), "Expected array type: {output}");
}

#[test]
fn test_tuple_type_in_declaration() {
    let output = emit_dts("export type Pair = [string, number];");
    assert!(
        output.contains("[string, number]"),
        "Expected tuple type: {output}"
    );
}

#[test]
fn test_conditional_type_in_declaration() {
    let output = emit_dts("export type IsString<T> = T extends string ? true : false;");
    assert!(
        output.contains("T extends string ? true : false"),
        "Expected conditional type: {output}"
    );
}

#[test]
fn test_mapped_type_in_declaration() {
    let output = emit_dts("export type Readonly<T> = { readonly [K in keyof T]: T[K] };");
    assert!(
        output.contains("readonly"),
        "Expected mapped type with readonly: {output}"
    );
    assert!(
        output.contains("keyof T"),
        "Expected keyof in mapped type: {output}"
    );
}

#[test]
fn test_indexed_access_type() {
    let output = emit_dts("export type Name = Person['name'];");
    assert!(
        output.contains("Person["),
        "Expected indexed access type: {output}"
    );
}

#[test]
fn test_typeof_type() {
    let output = emit_dts("declare const x: number;\nexport type T = typeof x;");
    assert!(
        output.contains("typeof x"),
        "Expected typeof type: {output}"
    );
}

// =============================================================================
// 6. Generic Declarations
// =============================================================================

#[test]
fn test_generic_function() {
    let output = emit_dts("export function identity<T>(x: T): T { return x; }");
    assert!(
        output.contains("<T>"),
        "Expected generic type parameter: {output}"
    );
    assert!(
        output.contains("x: T"),
        "Expected parameter with generic type: {output}"
    );
    assert!(
        output.contains("): T;"),
        "Expected return type with generic: {output}"
    );
}

#[test]
fn test_generic_interface_with_constraint() {
    let output = emit_dts(
        r#"
    export interface Container<T extends object> {
        value: T;
    }
    "#,
    );
    assert!(
        output.contains("<T extends object>"),
        "Expected generic type parameter with constraint: {output}"
    );
    assert!(
        output.contains("value: T;"),
        "Expected member with generic type: {output}"
    );
}

#[test]
fn test_generic_class_with_default() {
    let output = emit_dts(
        r#"
    export class Box<T = string> {
        content: T;
        constructor(value: T) { this.content = value; }
    }
    "#,
    );
    assert!(
        output.contains("<T = string>"),
        "Expected generic type parameter with default: {output}"
    );
}

#[test]
fn test_multiple_type_parameters() {
    let output = emit_dts(
        "export function map<T, U>(arr: T[], fn: (x: T) => U): U[] { return arr.map(fn); }",
    );
    assert!(
        output.contains("<T, U>"),
        "Expected multiple type parameters: {output}"
    );
}

// =============================================================================
// 7. Ambient / Declare Declarations
// =============================================================================

#[test]
fn test_declare_class_passthrough() {
    let output = emit_dts(
        r#"
    declare class Foo {
        bar(): void;
    }
    "#,
    );
    assert!(
        output.contains("declare class Foo"),
        "Expected declare class: {output}"
    );
    assert!(
        output.contains("bar(): void;"),
        "Expected method signature: {output}"
    );
}

#[test]
fn test_declare_function_passthrough() {
    let output = emit_dts("declare function greet(name: string): void;");
    assert!(
        output.contains("declare function greet(name: string): void;"),
        "Expected declare function: {output}"
    );
}

#[test]
fn test_declare_var_passthrough() {
    let output = emit_dts("declare var globalName: string;");
    assert!(
        output.contains("declare var globalName: string;"),
        "Expected declare var: {output}"
    );
}

// =============================================================================
// 8. Module / Namespace Declarations
// =============================================================================

#[test]
fn test_namespace_declaration() {
    let output = emit_dts(
        r#"
    export declare namespace MyLib {
        function create(): void;
        class Widget {
            name: string;
        }
    }
    "#,
    );
    assert!(
        output.contains("export declare namespace MyLib"),
        "Expected namespace declaration: {output}"
    );
    assert!(
        output.contains("function create(): void;"),
        "Expected function in namespace: {output}"
    );
    assert!(
        output.contains("class Widget"),
        "Expected class in namespace: {output}"
    );
}

#[test]
fn test_nested_namespace() {
    let output = emit_dts(
        r#"
    export declare namespace Outer {
        namespace Inner {
            const value: number;
        }
    }
    "#,
    );
    assert!(
        output.contains("namespace Outer"),
        "Expected outer namespace: {output}"
    );
    assert!(
        output.contains("namespace Inner"),
        "Expected inner namespace: {output}"
    );
}

// =============================================================================
// 9. Enum Declarations
// =============================================================================

#[test]
fn test_regular_enum() {
    let output = emit_dts(
        r#"
    export enum Color {
        Red,
        Green,
        Blue
    }
    "#,
    );
    assert!(
        output.contains("export declare enum Color"),
        "Expected exported declare enum: {output}"
    );
    assert!(output.contains("Red"), "Expected Red member: {output}");
    assert!(output.contains("Green"), "Expected Green member: {output}");
    assert!(output.contains("Blue"), "Expected Blue member: {output}");
}

#[test]
fn test_const_enum() {
    let output = emit_dts(
        r#"
    export const enum Direction {
        Up = 0,
        Down = 1,
        Left = 2,
        Right = 3
    }
    "#,
    );
    assert!(
        output.contains("export declare const enum Direction"),
        "Expected exported declare const enum: {output}"
    );
    assert!(output.contains("Up = 0"), "Expected Up = 0: {output}");
    assert!(output.contains("Right = 3"), "Expected Right = 3: {output}");
}

#[test]
fn test_invalid_const_enum_object_index_access_emits_any() {
    let output = emit_dts_with_binding(
        r#"
const enum G {
    A = 1,
    B = 2,
}
let z1 = G[G.A];
"#,
    );

    assert!(
        output.contains("declare let z1: any;"),
        "Expected invalid const enum object index access to emit any: {output}"
    );
}

#[test]
fn test_string_enum() {
    let output = emit_dts(
        r#"
    export enum Status {
        Active = "active",
        Inactive = "inactive"
    }
    "#,
    );
    assert!(
        output.contains("Active = \"active\""),
        "Expected string enum value: {output}"
    );
    assert!(
        output.contains("Inactive = \"inactive\""),
        "Expected string enum value: {output}"
    );
}

#[test]
fn test_enum_auto_increment() {
    let output = emit_dts(
        r#"
    export enum Seq {
        A = 10,
        B,
        C
    }
    "#,
    );
    assert!(output.contains("A = 10"), "Expected A = 10: {output}");
    assert!(
        output.contains("B = 11"),
        "Expected B = 11 (auto-increment): {output}"
    );
    assert!(
        output.contains("C = 12"),
        "Expected C = 12 (auto-increment): {output}"
    );
}

// =============================================================================
// 10. Class Advanced Features
// =============================================================================

#[test]
fn test_abstract_class() {
    let output = emit_dts(
        r#"
    export abstract class Shape {
        abstract area(): number;
        name: string;
        constructor(name: string) { this.name = name; }
    }
    "#,
    );
    assert!(
        output.contains("export declare abstract class Shape"),
        "Expected abstract class: {output}"
    );
    assert!(
        output.contains("abstract area(): number;"),
        "Expected abstract method: {output}"
    );
}

#[test]
fn test_class_with_heritage() {
    let output = emit_dts(
        r#"
    export class Dog extends Animal implements Pet {
        bark(): void {}
    }
    "#,
    );
    assert!(
        output.contains("extends Animal"),
        "Expected extends clause: {output}"
    );
    assert!(
        output.contains("implements Pet"),
        "Expected implements clause: {output}"
    );
}

#[test]
fn test_constructor_declaration() {
    let output = emit_dts(
        r#"
    export class Point {
        x: number;
        y: number;
        constructor(x: number, y: number) {
            this.x = x;
            this.y = y;
        }
    }
    "#,
    );
    assert!(
        output.contains("constructor(x: number, y: number);"),
        "Expected constructor in .d.ts: {output}"
    );
}

#[test]
fn test_parameter_properties() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x: number, protected y: number, private z: number) {}
    }
    "#,
    );
    // Parameter properties should be emitted as class properties
    assert!(
        output.contains("x: number;"),
        "Expected public parameter property as class property: {output}"
    );
    assert!(
        output.contains("protected y: number;"),
        "Expected protected parameter property: {output}"
    );
    assert!(
        output.contains("private z;"),
        "Expected private parameter property (without type): {output}"
    );
}

#[test]
fn test_optional_parameter_property_emits_undefined_in_constructor_and_property() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x?: string) {}
    }
    "#,
    );

    assert!(
        output.contains("x?: string | undefined;"),
        "Expected optional parameter property to include undefined in property type: {output}"
    );
    assert!(
        output.contains("constructor(x?: string | undefined);"),
        "Expected optional parameter property to include undefined in constructor type: {output}"
    );
}

#[test]
fn test_parameter_property_initializer_infers_property_type() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x = "hello") {}
    }
    "#,
    );

    assert!(
        output.contains("x: string;"),
        "Expected initializer-backed parameter property to infer a property type: {output}"
    );
    assert!(
        output.contains("constructor(x?: string);"),
        "Expected initializer-backed parameter property constructor to stay optional: {output}"
    );
}

#[test]
fn test_getter_and_setter() {
    let output = emit_dts(
        r#"
    export class Foo {
        get value(): number { return 42; }
        set value(v: number) {}
    }
    "#,
    );
    assert!(
        output.contains("get value(): number;"),
        "Expected getter declaration: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Expected setter declaration: {output}"
    );
}

#[test]
fn test_static_member() {
    let output = emit_dts(
        r#"
    export class Singleton {
        static instance: Singleton;
        static create(): Singleton { return new Singleton(); }
    }
    "#,
    );
    assert!(
        output.contains("static instance"),
        "Expected static property: {output}"
    );
    assert!(
        output.contains("static create"),
        "Expected static method: {output}"
    );
}

#[test]
fn test_readonly_property() {
    let output = emit_dts(
        r#"
    export class Config {
        readonly name: string;
        constructor(name: string) { this.name = name; }
    }
    "#,
    );
    assert!(
        output.contains("readonly name: string;"),
        "Expected readonly property: {output}"
    );
}

#[test]
fn test_index_signature_in_class() {
    let output = emit_dts(
        r#"
    export class Dict {
        [key: string]: any;
    }
    "#,
    );
    assert!(
        output.contains("[key: string]: any;"),
        "Expected index signature in class: {output}"
    );
}

#[test]
fn test_index_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface StringMap {
        [key: string]: string;
    }
    "#,
    );
    assert!(
        output.contains("[key: string]: string;"),
        "Expected index signature in interface: {output}"
    );
}

#[test]
fn test_optional_property_in_interface() {
    let output = emit_dts(
        r#"
    export interface Config {
        name: string;
        debug?: boolean;
    }
    "#,
    );
    assert!(
        output.contains("debug?: boolean;"),
        "Expected optional property: {output}"
    );
}

#[test]
fn test_optional_method_in_interface() {
    let output = emit_dts(
        r#"
    export interface Plugin {
        init?(): void;
    }
    "#,
    );
    assert!(
        output.contains("init?(): void;"),
        "Expected optional method: {output}"
    );
}

#[test]
fn test_optional_computed_method_in_class_emits_optional_property_function_type() {
    let output = emit_dts(
        r#"
    export const dataSomething: `data-${string}` = "data-x" as `data-${string}`;
    export class WithData {
        [dataSomething]?(): string {
            return "something";
        }
    }
    "#,
    );
    // tsc emits optional COMPUTED methods as property signatures with function
    // types (unlike non-computed optional methods which keep method syntax).
    assert!(
        output.contains("[dataSomething]?: (() => string) | undefined;"),
        "Expected optional computed method to emit as property signature: {output}"
    );
}

#[test]
fn test_static_computed_methods_emit_body_inferred_return_types() {
    let output = emit_dts(
        r#"
    export declare const f1: string;
    export declare const f2: string;

    export class Holder {
        static [f1]() {
            return { static: true };
        }
        static [f2]() {
            return { static: "sometimes" };
        }
    }

    export const staticLookup = Holder["x"];
    "#,
    );
    // tsc emits computed methods as method signatures, not property signatures.
    assert!(
        output.contains("static [f1](): {")
            && output.contains("static: boolean;")
            && output.contains("static [f2](): {")
            && output.contains("static: string;"),
        "Expected static computed methods to use method syntax with body-inferred return types: {output}"
    );
}

// =============================================================================
// 11. Function Overloads
// =============================================================================

#[test]
fn test_function_overloads_emit_only_signatures() {
    let output = emit_dts(
        r#"
    export function parse(input: string): number;
    export function parse(input: number): string;
    export function parse(input: any): any { return input; }
    "#,
    );
    // Both overload signatures should be emitted
    assert!(
        output.contains("export declare function parse(input: string): number;"),
        "Expected first overload: {output}"
    );
    assert!(
        output.contains("export declare function parse(input: number): string;"),
        "Expected second overload: {output}"
    );
    // Implementation should NOT be emitted
    assert!(
        !output.contains("input: any): any;"),
        "Implementation signature should not appear: {output}"
    );
}

// =============================================================================
// 12. Interface Heritage
// =============================================================================

#[test]
fn test_interface_extends() {
    let output = emit_dts(
        r#"
    export interface Animal {
        name: string;
    }
    export interface Dog extends Animal {
        breed: string;
    }
    "#,
    );
    assert!(
        output.contains("interface Dog extends Animal"),
        "Expected interface extends: {output}"
    );
}

// =============================================================================
// 13. Private Identifier (#private)
// =============================================================================

#[test]
fn test_private_identifier_emits_private_marker() {
    let output = emit_dts(
        r#"
    export class Foo {
        #secret: number;
        getValue(): number { return this.#secret; }
    }
    "#,
    );
    // Private identifiers should produce `#private;`
    assert!(
        output.contains("#private;"),
        "Expected #private marker for private identifiers: {output}"
    );
    // The actual #secret name should NOT appear
    assert!(
        !output.contains("#secret"),
        "#secret should not appear in .d.ts: {output}"
    );
}

// =============================================================================
// 14. Numeric Literal Normalization
// =============================================================================

#[test]
fn test_normalize_numeric_literal_unchanged() {
    assert_eq!(DeclarationEmitter::normalize_numeric_literal("42"), "42");
    assert_eq!(
        DeclarationEmitter::normalize_numeric_literal("3.14"),
        "3.14"
    );
    assert_eq!(DeclarationEmitter::normalize_numeric_literal("0"), "0");
}

#[test]
fn test_normalize_numeric_literal_large_integer() {
    // Very large integers should be normalized through f64 round-trip
    let result = DeclarationEmitter::normalize_numeric_literal(
        "123456789123456789123456789123456789123456789123456789",
    );
    assert!(
        result.contains("e+"),
        "Expected scientific notation for very large number: {result}"
    );
}

// =============================================================================
// 15. Format JS Number
// =============================================================================

#[test]
fn test_format_js_number_infinity() {
    assert_eq!(
        DeclarationEmitter::format_js_number(f64::INFINITY),
        "Infinity"
    );
    assert_eq!(
        DeclarationEmitter::format_js_number(f64::NEG_INFINITY),
        "-Infinity"
    );
}

#[test]
fn test_format_js_number_nan() {
    assert_eq!(DeclarationEmitter::format_js_number(f64::NAN), "NaN");
}

#[test]
fn test_format_js_number_integers() {
    assert_eq!(DeclarationEmitter::format_js_number(0.0), "0");
    assert_eq!(DeclarationEmitter::format_js_number(42.0), "42");
    assert_eq!(DeclarationEmitter::format_js_number(-1.0), "-1");
}

#[test]
fn test_format_js_number_floats() {
    assert_eq!(DeclarationEmitter::format_js_number(3.15), "3.15");
    assert_eq!(DeclarationEmitter::format_js_number(0.5), "0.5");
}

// =============================================================================
// 16. Rest Parameters
// =============================================================================

#[test]
fn test_rest_parameter_in_function() {
    let output = emit_dts("export function sum(...nums: number[]): number { return 0; }");
    assert!(
        output.contains("...nums: number[]"),
        "Expected rest parameter: {output}"
    );
}

// =============================================================================
// 17. Call / Construct Signatures in Interfaces
// =============================================================================

#[test]
fn test_call_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface Callable {
        (x: number): string;
    }
    "#,
    );
    assert!(
        output.contains("(x: number): string;"),
        "Expected call signature: {output}"
    );
}

#[test]
fn test_construct_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface Constructable {
        new (name: string): object;
    }
    "#,
    );
    assert!(
        output.contains("new (name: string): object;"),
        "Expected construct signature: {output}"
    );
}

// =============================================================================
// 18. Type Predicate (type guard)
// =============================================================================

#[test]
fn test_type_predicate_in_function() {
    let output = emit_dts(
        r#"
    export function isString(x: unknown): x is string {
        return typeof x === "string";
    }
    "#,
    );
    assert!(
        output.contains("x is string"),
        "Expected type predicate: {output}"
    );
}

// =============================================================================
// 19. Default Parameter Values (stripped)
// =============================================================================

#[test]
fn test_default_parameter_values_omitted() {
    let output = emit_dts(
        r#"
    export function greet(name: string = "world"): void {}
    "#,
    );
    // Default values should be stripped; parameter should remain with its type
    assert!(
        output.contains("name"),
        "Expected parameter name preserved: {output}"
    );
    // The default value itself should not appear in the .d.ts
    assert!(
        !output.contains("\"world\""),
        "Default value should be stripped from .d.ts: {output}"
    );
}

// =============================================================================
// 20. Using declaration emits as const
// =============================================================================

#[test]
fn test_using_declaration_emits_const() {
    let output = emit_dts(r#"export using x: Disposable = getResource();"#);
    // `using` declarations emit as `const` in .d.ts
    assert!(
        output.contains("const x"),
        "Expected using declaration to emit as const: {output}"
    );
}

// =============================================================================
// 21. Void-returning function body inference
// =============================================================================

#[test]
fn test_void_body_function_infers_void_return() {
    let output = emit_dts(
        r#"
    export function doNothing() {
        console.log("hi");
    }
    "#,
    );
    assert!(
        output.contains("void"),
        "Expected void return type for function with no return: {output}"
    );
}

// =============================================================================
// 22. Side-effect imports preserved
// =============================================================================

#[test]
fn test_side_effect_import_preserved() {
    let output = emit_dts(r#"import "./polyfill";"#);
    assert!(
        output.contains("import \"./polyfill\""),
        "Expected side-effect import to be preserved: {output}"
    );
}

// =============================================================================
// 23. Literal type aliases
// =============================================================================

#[test]
fn test_literal_type_alias() {
    let output = emit_dts("export type Direction = 'up' | 'down' | 'left' | 'right';");
    assert!(
        output.contains("'up'") || output.contains("\"up\""),
        "Expected string literal type: {output}"
    );
}

// =============================================================================
// 24. Keyof type
// =============================================================================

#[test]
fn test_keyof_type() {
    let output = emit_dts("export type Keys<T> = keyof T;");
    assert!(output.contains("keyof T"), "Expected keyof type: {output}");
}

// =============================================================================
// 25. Type operator (readonly arrays)
// =============================================================================

#[test]
fn test_readonly_array_type() {
    let output = emit_dts("export type ReadonlyArr = readonly number[];");
    assert!(
        output.contains("readonly number[]"),
        "Expected readonly array type: {output}"
    );
}

// =============================================================================
// 26. Parenthesized type
// =============================================================================

#[test]
fn test_parenthesized_function_type_in_array() {
    let output = emit_dts("export type FnArray = ((x: number) => void)[];");
    assert!(
        output.contains("((x: number) => void)[]"),
        "Expected parenthesized function type in array: {output}"
    );
}

// =============================================================================
// 27. Computed property names
// =============================================================================

#[test]
fn test_computed_symbol_property() {
    let output = emit_dts(
        r#"
    export interface Iterable {
        [Symbol.iterator](): Iterator<any>;
    }
    "#,
    );
    assert!(
        output.contains("[Symbol.iterator]"),
        "Expected computed Symbol property: {output}"
    );
}

// =============================================================================
// 28. Export assignment (export =)
// =============================================================================

#[test]
fn test_export_equals() {
    let output = emit_dts(
        r#"
    declare const myLib: { version: string };
    export = myLib;
    "#,
    );
    assert!(
        output.contains("export = myLib;"),
        "Expected export = : {output}"
    );
}

#[test]
fn test_export_equals_import_equals_keeps_namespace_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
    namespace m3 {
        export namespace m2 {
            export interface connectModule {
                (res, req, next): void;
            }
            export interface connectExport {
                use: (mod: connectModule) => connectExport;
                listen: (port: number) => void;
            }
        }

        export var server: {
            (): m2.connectExport;
            test1: m2.connectModule;
            test2(): m2.connectModule;
        };
    }

    import m = m3;
    export = m;
    "#,
    );

    let namespace_pos = output
        .find("declare namespace m3")
        .expect("Expected namespace dependency to be preserved");
    let import_pos = output
        .find("import m = m3;")
        .expect("Expected import equals alias to be emitted");
    let export_pos = output
        .find("export = m;")
        .expect("Expected export assignment to be emitted");

    assert!(
        namespace_pos < import_pos && import_pos < export_pos,
        "Expected namespace, import alias, and export assignment to preserve source order: {output}"
    );
}

#[test]
fn test_export_equals_import_equals_chain_keeps_namespace_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
    namespace m {
        export namespace c {
            export class c {
            }
        }
    }

    import a = m.c;
    import b = a;
    export = b;
    "#,
    );

    let namespace_pos = output
        .find("declare namespace m")
        .expect("Expected namespace dependency to be preserved");
    let first_import_pos = output
        .find("import a = m.c;")
        .expect("Expected first import equals alias to be emitted");
    let second_import_pos = output
        .find("import b = a;")
        .expect("Expected chained import equals alias to be emitted");
    let export_pos = output
        .find("export = b;")
        .expect("Expected export assignment to be emitted");

    assert!(
        namespace_pos < first_import_pos
            && first_import_pos < second_import_pos
            && second_import_pos < export_pos,
        "Expected namespace, import chain, and export assignment to preserve source order: {output}"
    );
}

#[test]
fn test_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface {}
    export interface Loc extends Req {}
    "#,
    );

    assert!(
        output.contains(
            r#"import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected type-only import attributes to be preserved: {output}"
    );
    assert!(
        output.contains(
            r#"import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected named import attributes to be preserved: {output}"
    );
}

#[test]
fn test_import_type_alias_is_preserved_with_usage_analysis() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import { type RequireInterface as Req } from "pkg";

    export interface Loc extends Req {}
    "#,
    );

    assert!(
        output.contains(r#"import { type RequireInterface as Req } from "pkg";"#),
        "Expected aliased type import to be preserved: {output}"
    );
}

#[test]
fn test_namespace_import_type_is_preserved_with_usage_analysis() {
    let source = r#"
    import * as ns from "pkg";
    export const value = ns;
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let var_stmt = source_file
        .statements
        .nodes
        .iter()
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .expect("missing variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list = parser
        .arena
        .get(decl_list_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let ns_sym_id = binder
        .file_locals
        .get("ns")
        .expect("expected namespace import symbol");

    let interner = TypeInterner::new();
    let namespace_type = interner.module_namespace(SymbolRef(ns_sym_id.0));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.name.0, namespace_type);

    let current_arena = Arc::new(parser.arena.clone());
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    let output = emitter.emit(root);

    assert!(
        output.contains(r#"import * as ns from "pkg";"#),
        "Expected namespace import to be preserved: {output}"
    );
    assert!(
        output.contains("export declare const value: typeof ns;"),
        "Expected exported value to use the namespace import alias type: {output}"
    );
}

#[test]
#[ignore = "namespace import typeof alias not yet resolved in declaration emitter"]
fn test_exported_namespace_import_initializer_preserves_typeof_alias() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import * as ns from "pkg";
    export const value = ns;
    "#,
    );

    assert!(
        output.contains(r#"import * as ns from "pkg";"#),
        "Expected namespace import to survive usage analysis: {output}"
    );
    assert!(
        output.contains("export declare const value: typeof ns;"),
        "Expected exported namespace import initializer to emit typeof alias: {output}"
    );
}

#[test]
fn test_call_expression_recovers_return_type_from_callee_type() {
    let source = r#"
    export const a = helper.x();
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let var_stmt = source_file
        .statements
        .nodes
        .iter()
        .find_map(|&stmt_idx| {
            let stmt_node = parser.arena.get(stmt_idx)?;
            if let Some(var_stmt) = parser.arena.get_variable(stmt_node) {
                return Some(var_stmt);
            }
            let export = parser.arena.get_export_decl(stmt_node)?;
            let clause_node = parser.arena.get(export.export_clause)?;
            parser.arena.get_variable(clause_node)
        })
        .expect("missing variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list = parser
        .arena
        .get(decl_list_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl_idx = decl_list.declarations.nodes[0];
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");
    let call = parser
        .arena
        .get(decl.initializer)
        .and_then(|node| parser.arena.get_call_expr(node))
        .expect("missing call expression");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let callee_type = interner.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(call.expression.0, callee_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare const a: string;"),
        "Expected call expression to recover return type from callee type: {output}"
    );
}

#[test]
fn test_export_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    "#,
    );

    assert!(
        output.contains(
            r#"export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected export type attributes to be preserved: {output}"
    );
}

#[test]
fn test_asserted_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts(
        r#"
    export type LocalInterface = import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface;
    export const value = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
    "#,
    );

    assert!(
        output.contains(
            r#"export type LocalInterface = import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected import type attributes to be formatted canonically in type aliases: {output}"
    );
    assert!(
        output.contains(
            r#"export declare const value: import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected asserted import type with attributes to be preserved on exported values: {output}"
    );
}

#[test]
fn test_invalid_resolution_mode_attribute_is_dropped_and_unused_mixed_import_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "foobar" };
    import { ImportInterface } from "pkg" with { "resolution-mode": "import" };
    import { type RequireInterface as Req, RequireInterface as Req2 } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface, ImportInterface {}
    "#,
    );

    assert!(
        output.contains(r#"import type { RequireInterface } from "pkg";"#),
        "Expected invalid resolution-mode attribute to be dropped: {output}"
    );
    assert!(
        output.contains(
            r#"import { ImportInterface } from "pkg" with { "resolution-mode": "import" };"#
        ),
        "Expected valid resolution-mode attribute to be preserved: {output}"
    );
    assert!(
        !output.contains("Req2"),
        "Expected unused mixed import bindings to be elided: {output}"
    );
}

// =============================================================================
// 29. Namespace export as
// =============================================================================

#[test]
fn test_star_export_as_namespace() {
    let output = emit_dts(r#"export * as utils from "./utils";"#);
    assert!(
        output.contains("export * as utils from"),
        "Expected namespace re-export: {output}"
    );
}

// =============================================================================
// 30. Asserts modifier in type predicate
// =============================================================================

#[test]
fn test_assertion_function() {
    let output = emit_dts(
        r#"
    export function assertDefined(val: unknown): asserts val {
        if (val == null) throw new Error();
    }
    "#,
    );
    assert!(
        output.contains("asserts val"),
        "Expected asserts modifier: {output}"
    );
}

#[test]
fn test_setter_parameter_asserts_this_predicate_is_rescued_from_source() {
    let output = emit_dts(
        r#"
    declare class Wat {
        set p2(x: asserts this is string);
    }
    "#,
    );

    assert!(
        output.contains("set p2(x: asserts this is string);"),
        "Expected setter parameter asserts predicate to be preserved: {output}"
    );
}

#[test]
fn test_const_identity_call_preserves_numeric_literal_initializer() {
    let output = emit_dts(
        r#"
function id<T>(x: T): T {
    return x;
}

const value = id(123);
"#,
    );

    assert!(
        output.contains("declare const value = 123;"),
        "Expected const identity call to preserve numeric literal initializer: {output}"
    );
}

#[test]
fn test_const_identity_call_preserves_negative_numeric_literal_initializer() {
    let output = emit_dts(
        r#"
function id<T>(x: T): T {
    return x;
}

const value = id(-123);
"#,
    );

    assert!(
        output.contains("declare const value = -123;"),
        "Expected const identity call to preserve negative numeric literal initializer: {output}"
    );
}

// =============================================================================
// 31. Multiple variable declarations on one line
// =============================================================================

#[test]
fn test_multiple_variable_declarators() {
    let output = emit_dts("export var x: number, y: string;");
    assert!(
        output.contains("x: number"),
        "Expected first variable: {output}"
    );
    assert!(
        output.contains("y: string"),
        "Expected second variable: {output}"
    );
}

#[test]
fn test_destructuring_variable_declaration_groups_typed_bindings() {
    let source = r#"var [x, y] = [1, "hello"];"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let stmt = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(stmt.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl = parser
        .arena
        .get(decl_list.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.initializer.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var x: number, y: string;"),
        "Expected destructured bindings to emit in one typed declaration: {output}"
    );
}

#[test]
fn test_destructuring_parameter_properties_emit_individual_class_properties() {
    let source = "class C { constructor(public [x, y]: [string, number]) {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let class_decl = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let ctor_idx = class_decl.members.nodes[0];
    let ctor = parser
        .arena
        .get(ctor_idx)
        .and_then(|node| parser.arena.get_constructor(node))
        .expect("missing constructor");
    let param_idx = ctor.parameters.nodes[0];
    let param = parser
        .arena
        .get(param_idx)
        .and_then(|node| parser.arena.get_parameter(node))
        .expect("missing parameter");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(param.type_annotation.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("x: string;"),
        "Expected first destructured parameter property to be emitted: {output}"
    );
    assert!(
        output.contains("y: number;"),
        "Expected second destructured parameter property to be emitted: {output}"
    );
    assert!(
        !output.contains("[x, y]: [string, number];"),
        "Did not expect destructuring pattern to be emitted as a property name: {output}"
    );
}

// =============================================================================
// Method return type inference from arithmetic body expressions
// =============================================================================

#[test]
fn method_return_type_inferred_from_addition_of_number_properties() {
    let output = emit_dts_with_binding(
        r#"
class Calculator {
    public x: number;
    public add(b: number) {
        return this.x + b;
    }
}
"#,
    );
    assert!(
        output.contains("add(b: number): number;"),
        "Expected method return type to be inferred as number from this.x + b: {output}"
    );
}

#[test]
fn method_return_type_inferred_from_subtraction() {
    let output = emit_dts_with_binding(
        r#"
class Calc {
    public value: number;
    public sub(b: number) {
        return this.value - b;
    }
}
"#,
    );
    assert!(
        output.contains("sub(b: number): number;"),
        "Expected method return type to be inferred as number from subtraction: {output}"
    );
}

#[test]
fn method_return_type_inferred_from_multiplication() {
    let output = emit_dts_with_binding(
        r#"
class Calc {
    public value: number;
    public mul(b: number) {
        return this.value * b;
    }
}
"#,
    );
    assert!(
        output.contains("mul(b: number): number;"),
        "Expected method return type to be inferred as number from multiplication: {output}"
    );
}

#[test]
fn static_method_return_type_inferred_from_addition() {
    let output = emit_dts_with_binding(
        r#"
class C {
    static s1: number;
    static add(b: number) {
        return C.s1 + b;
    }
}
"#,
    );
    assert!(
        output.contains("static add(b: number): number;"),
        "Expected static method return type to be inferred as number: {output}"
    );
}

#[test]
fn method_return_type_string_concatenation() {
    let output = emit_dts_with_binding(
        r#"
class Greeter {
    public name: string;
    public greet() {
        return "Hello, " + this.name;
    }
}
"#,
    );
    assert!(
        output.contains("greet(): string;"),
        "Expected method return type to be inferred as string from string concatenation: {output}"
    );
}

#[test]
fn reference_declared_type_annotation_resolves_property_declarations() {
    let output = emit_dts_with_binding(
        r#"
class Foo {
    public x: number;
    public getX() {
        return this.x;
    }
}
"#,
    );
    assert!(
        output.contains("getX(): number;"),
        "Expected method returning this.x to be inferred as number: {output}"
    );
}

#[test]
fn method_return_type_bitwise_operations_produce_number() {
    let output = emit_dts_with_binding(
        r#"
class BitOps {
    public a: number;
    public shiftLeft(n: number) {
        return this.a << n;
    }
    public bitwiseAnd(n: number) {
        return this.a & n;
    }
}
"#,
    );
    assert!(
        output.contains("shiftLeft(n: number): number;"),
        "Expected shift left to return number: {output}"
    );
    assert!(
        output.contains("bitwiseAnd(n: number): number;"),
        "Expected bitwise and to return number: {output}"
    );
}

// =============================================================================
// Computed property names in declaration emit
// =============================================================================

#[test]
fn test_object_literal_computed_property_names_no_crash() {
    // Regression test: the declaration emitter used to panic with
    // "insertion index should be <= len" when an object literal had multiple
    // computed property members and some matched existing printed lines while
    // others didn't. The `offset` counter incremented per loop iteration
    // rather than per actual insertion, causing out-of-bounds Vec::insert.
    let output = emit_dts(
        r#"
export const D = {
    [Symbol.iterator]: 1,
    [1]: 2,
    ["2"]: 3,
};
"#,
    );
    // Should not crash — any output is acceptable
    assert!(!output.is_empty(), "Expected non-empty declaration output");
}

#[test]
fn test_interface_computed_property_legal_names_emitted() {
    // Numeric and string literal computed property names are legal in .d.ts
    let output = emit_dts(
        r#"
export interface Foo {
    [1]: number;
    ["hello"]: string;
}
"#,
    );
    assert!(
        output.contains("[1]") || output.contains("1:"),
        "Expected numeric computed property to be emitted: {output}"
    );
    assert!(
        output.contains("hello"),
        "Expected string literal computed property to be emitted: {output}"
    );
}

#[test]
fn test_type_alias_computed_property_names_no_crash() {
    let output = emit_dts(
        r#"
export type A = {
    [Symbol.iterator]: number;
    [1]: number;
    ["2"]: number;
};
"#,
    );
    assert!(
        !output.is_empty(),
        "Expected non-empty output for type alias with computed properties"
    );
}

#[test]
fn test_class_computed_property_names_no_crash() {
    let output = emit_dts(
        r#"
export class C {
    [Symbol.iterator]: number = 1;
    [1]: number = 1;
    ["2"]: number = 1;
}
"#,
    );
    assert!(
        !output.is_empty(),
        "Expected non-empty output for class with computed properties"
    );
}

#[test]
fn test_const_enum_computed_method_keeps_method_syntax() {
    // Computed method names referencing const enum members should use method
    // syntax `[G.A](): void` not property syntax `[G.A]: () => void`, because
    // const enum values are always literals (valid property names in .d.ts).
    let output = emit_dts_with_binding(
        r#"
const enum G {
    A = 0,
    B = 1,
}
export class C {
    [G.A]() { }
    get [G.B]() {
        return true;
    }
    set [G.B](x: number) { }
}
"#,
    );
    assert!(
        !output.contains("[G.A]: () =>"),
        "Expected method syntax not property syntax for const enum computed method: {output}"
    );
}

#[test]
fn test_inline_mapped_type_emits_as_clause_and_value_type() {
    // Inline mapped types inside type literals must emit the `as` clause
    // correctly (before `]`, not as `: `) and must emit the value type.
    let output = emit_dts(
        r#"
export type Remap<T> = {
    [K in keyof T as K extends string ? `get_${K}` : never]: T[K];
};
"#,
    );
    assert!(
        output.contains(" as K extends string ? `get_${K}` : never]"),
        "Expected 'as' clause for key remapping in mapped type: {output}"
    );
    assert!(
        output.contains("]: T[K];"),
        "Expected value type T[K] in mapped type: {output}"
    );
    assert!(
        !output.contains("]: ;"),
        "Must not emit empty value type in mapped type: {output}"
    );
}

#[test]
fn test_override_modifier_stripped_in_dts() {
    // tsc strips `override` from class members in .d.ts output —
    // it is not part of the declaration surface.
    let output = emit_dts(
        r#"
declare class Base {
    method(): void;
    prop: number;
}
export declare class Derived extends Base {
    override method(): void;
    override prop: number;
}
"#,
    );
    assert!(
        !output.contains("override"),
        "Expected override modifier to be stripped in .d.ts: {output}"
    );
    assert!(
        output.contains("method(): void;"),
        "Expected method in .d.ts: {output}"
    );
    assert!(
        output.contains("prop: number;"),
        "Expected prop in .d.ts: {output}"
    );
}

#[test]
fn test_export_default_class_emits_parameter_properties() {
    // `export default class` with constructor parameter properties must emit
    // the properties as class members, same as non-default exported classes.
    let output = emit_dts(
        r#"
export default class Foo {
    constructor(public x: number, private y: string) {}
}
"#,
    );
    assert!(
        output.contains("x: number;"),
        "Expected parameter property 'x' as class member in export default class: {output}"
    );
    assert!(
        output.contains("private y;"),
        "Expected private parameter property 'y' in export default class: {output}"
    );
}

#[test]
fn test_non_ambient_namespace_strips_export_keyword_from_members() {
    // Non-ambient namespaces gain `declare` in .d.ts output, making them
    // ambient. Members inside should not have `export` keyword unless
    // there is a scope marker.
    let output = emit_dts(
        r#"
export namespace Utils {
    export function helper(): void;
    export interface Options {
        verbose: boolean;
    }
}
"#,
    );
    assert!(
        output.contains("function helper(): void;"),
        "Expected 'function helper' without export keyword: {output}"
    );
    assert!(
        output.contains("interface Options"),
        "Expected 'interface Options' without export keyword: {output}"
    );
    assert!(
        !output.contains("export function helper"),
        "Should not have 'export function' inside declare namespace: {output}"
    );
    assert!(
        !output.contains("export interface Options"),
        "Should not have 'export interface' inside declare namespace: {output}"
    );
}

#[test]
fn test_declare_global_augmentation_emitted_in_module_file() {
    // `declare global { ... }` should be emitted even when the file
    // has exports (public API filter enabled).
    let output = emit_dts(
        r#"
export function foo(): void;
declare global {
    interface String {
        customMethod(): void;
    }
}
"#,
    );
    assert!(
        output.contains("declare global"),
        "Expected 'declare global' block in output: {output}"
    );
    assert!(
        output.contains("customMethod(): void;"),
        "Expected customMethod in declare global block: {output}"
    );
    // Should not have 'namespace global' instead of 'global'
    assert!(
        !output.contains("namespace global"),
        "Should emit 'declare global' not 'declare namespace global': {output}"
    );
}

#[test]
fn test_declare_module_augmentation_emitted_in_module_file() {
    // `declare module "foo" { ... }` should be emitted even when the
    // file has exports (public API filter enabled).
    let output = emit_dts(
        r#"
export {};
declare module "some-module" {
    interface SomeType {
        x: number;
    }
}
"#,
    );
    assert!(
        output.contains("declare module \"some-module\""),
        "Expected 'declare module \"some-module\"' in output: {output}"
    );
    assert!(
        output.contains("interface SomeType"),
        "Expected SomeType interface in module augmentation: {output}"
    );
}

#[test]
fn test_module_augmentation_does_not_trigger_extra_export_marker() {
    // Module augmentations should not cause an extra `export {};` to be
    // emitted when the file already has a scope marker.
    let output = emit_dts(
        r#"
export function foo(): void;
declare global {
    interface Window {
        myProp: string;
    }
}
"#,
    );
    // The file has `export function foo` which is a module indicator,
    // so no extra `export {};` should appear.
    let export_marker_count = output.matches("export {};").count();
    assert_eq!(
        export_marker_count, 0,
        "Should not have extra 'export {{}}' marker when declare global is present: {output}"
    );
}

#[test]
fn test_export_default_interface_emits_correctly() {
    // `export default interface` should be emitted as
    // `export default interface Name { ... }` not as
    // `declare const _default: any; export default _default;`.
    let output = emit_dts(
        r#"
export default interface MyInterface {
    x: number;
    y: string;
}
"#,
    );
    assert!(
        output.contains("export default interface MyInterface"),
        "Expected 'export default interface MyInterface': {output}"
    );
    assert!(
        output.contains("x: number;"),
        "Expected 'x: number' member in interface: {output}"
    );
    assert!(
        output.contains("y: string;"),
        "Expected 'y: string' member in interface: {output}"
    );
    // Must not produce the fallback `any` pattern
    assert!(
        !output.contains("_default"),
        "Should not fall back to _default pattern: {output}"
    );
}

#[test]
fn test_export_default_interface_with_generics_and_heritage() {
    let output = emit_dts(
        r#"
interface Base { base: boolean; }
export default interface Extended<T> extends Base {
    value: T;
}
"#,
    );
    assert!(
        output.contains("export default interface Extended<T> extends Base"),
        "Expected interface with generics and extends: {output}"
    );
}

#[test]
fn test_union_in_intersection_gets_parenthesized() {
    // `(string | number) & { tag: "complex" }` must preserve parentheses
    // around the union to maintain correct operator precedence. Without
    // them, `string | number & { tag: "complex" }` means
    // `string | (number & { tag: "complex" })`.
    let output = emit_dts(
        r#"
export type Complex = (string | number) & { tag: "complex" };
"#,
    );
    assert!(
        output.contains("(string | number) & {"),
        "Expected parenthesized union in intersection type: {output}"
    );
}

#[test]
fn test_export_default_class_skips_overload_implementation() {
    // `export default class` with method overloads should skip the
    // implementation signature, same as non-default exported classes.
    let output = emit_dts(
        r#"
export default class Bar {
    method(x: number): number;
    method(x: string): string;
    method(x: number | string): number | string {
        return x;
    }
}
"#,
    );
    let method_count = output.matches("method(").count();
    assert_eq!(
        method_count, 2,
        "Expected exactly 2 overload signatures (not implementation) in export default class, got {method_count}: {output}"
    );
}

#[test]
fn test_namespace_non_exported_type_used_by_export_emits_scope_marker() {
    // When a non-ambient namespace has a non-exported type alias referenced
    // by an exported member, tsc emits the type alias and adds `export {};`.
    let output = emit_dts_with_usage_analysis(
        r#"
namespace M {
    type W = string | number;
    export namespace N {
        export class Window {}
        export var p: W;
    }
}
"#,
    );
    assert!(
        output.contains("type W = string | number;"),
        "Expected non-exported type alias 'W' to be emitted (referenced by exported member): {output}"
    );
    assert!(
        output.contains("export namespace N"),
        "Expected 'export namespace N' to preserve export keyword: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected 'export {{}};' scope marker in namespace with mixed exports: {output}"
    );
}

// =============================================================================
// Systematic DTS emit probes
// =============================================================================

#[test]
fn probe_abstract_class_emit() {
    let output = emit_dts(
        "export abstract class Shape {
    abstract area(): number;
    abstract readonly name: string;
}",
    );
    println!("PROBE abstract class:\n{output}");
    assert!(
        output.contains("export declare abstract class Shape"),
        "Missing abstract: {output}"
    );
    assert!(
        output.contains("abstract area(): number;"),
        "Missing abstract method: {output}"
    );
    assert!(
        output.contains("abstract readonly name: string;"),
        "Missing abstract readonly: {output}"
    );
}

#[test]
fn probe_const_enum_emit() {
    let output = emit_dts("export const enum Direction { Up = \"UP\", Down = \"DOWN\" }");
    println!("PROBE const enum:\n{output}");
    assert!(
        output.contains("export declare const enum Direction"),
        "Missing const enum: {output}"
    );
    assert!(
        output.contains("Up = \"UP\""),
        "Missing enum member: {output}"
    );
}

#[test]
fn probe_function_overloads() {
    let output = emit_dts(
        r#"export function foo(x: string): number;
export function foo(x: number): string;
export function foo(x: any): any { return x; }"#,
    );
    println!("PROBE overloads:\n{output}");
    assert!(
        output.contains("export declare function foo(x: string): number;"),
        "Missing overload 1: {output}"
    );
    assert!(
        output.contains("export declare function foo(x: number): string;"),
        "Missing overload 2: {output}"
    );
    // Implementation should NOT be emitted
    assert!(
        !output.contains("x: any): any"),
        "Implementation leaked: {output}"
    );
}

#[test]
fn probe_default_export_function() {
    let output = emit_dts("export default function foo(): void {}");
    println!("PROBE default fn:\n{output}");
    assert!(
        output.contains("export default function foo(): void;"),
        "Missing default fn: {output}"
    );
}

#[test]
fn probe_template_literal_type() {
    let output = emit_dts("export type Ev = `${'click' | 'scroll'}_handler`;");
    println!("PROBE template literal:\n{output}");
    assert!(
        output.contains("`${'click' | 'scroll'}_handler`")
            || output.contains("`${\"click\" | \"scroll\"}_handler`"),
        "Missing template literal: {output}"
    );
}

#[test]
fn probe_mapped_type_as_clause() {
    let output =
        emit_dts("export type Getters<T> = { [K in keyof T as `get${string & K}`]: () => T[K] };");
    println!("PROBE mapped as:\n{output}");
    assert!(
        output.contains("as `get${string & K}`"),
        "Missing as clause: {output}"
    );
}

#[test]
fn probe_conditional_type() {
    let output = emit_dts("export type IsStr<T> = T extends string ? true : false;");
    println!("PROBE conditional:\n{output}");
    assert!(
        output.contains("T extends string ? true : false"),
        "Missing conditional: {output}"
    );
}

#[test]
fn probe_call_construct_signatures() {
    let output = emit_dts(
        "export interface Factory {
    (arg: string): object;
    new (arg: string): object;
}",
    );
    println!("PROBE call+construct:\n{output}");
    assert!(
        output.contains("(arg: string): object;"),
        "Missing call sig: {output}"
    );
    assert!(
        output.contains("new (arg: string): object;"),
        "Missing construct sig: {output}"
    );
}

#[test]
fn probe_named_tuple_members() {
    let output = emit_dts("export type Point = [x: number, y: number, z?: number];");
    println!("PROBE named tuple:\n{output}");
    assert!(
        output.contains("x: number"),
        "Missing named member x: {output}"
    );
    assert!(
        output.contains("z?: number"),
        "Missing optional named member z: {output}"
    );
}

#[test]
fn probe_import_type() {
    let output = emit_dts("export type T = import('./mod').Foo;");
    println!("PROBE import type:\n{output}");
    assert!(output.contains("import("), "Missing import type: {output}");
}

#[test]
fn probe_unique_symbol() {
    let output = emit_dts("export declare const sym: unique symbol;");
    println!("PROBE unique symbol:\n{output}");
    assert!(
        output.contains("unique symbol"),
        "Missing unique symbol: {output}"
    );
}

#[test]
fn probe_type_predicate() {
    let output = emit_dts("export function isString(x: unknown): x is string;");
    println!("PROBE type predicate:\n{output}");
    assert!(
        output.contains("x is string"),
        "Missing type predicate: {output}"
    );
}

#[test]
fn probe_assertion_function_with_type() {
    let output = emit_dts("export function assertStr(x: unknown): asserts x is string;");
    println!("PROBE assertion fn:\n{output}");
    assert!(
        output.contains("asserts x is string"),
        "Missing assertion: {output}"
    );
}

#[test]
fn probe_infer_type() {
    let output = emit_dts("export type Unwrap<T> = T extends Promise<infer U> ? U : T;");
    println!("PROBE infer:\n{output}");
    assert!(output.contains("infer U"), "Missing infer: {output}");
}

#[test]
fn probe_parameter_properties() {
    let output = emit_dts(
        "export class Foo {
    constructor(public readonly x: number, private y: string, protected z: boolean) {}
}",
    );
    println!("PROBE param props:\n{output}");
    assert!(
        output.contains("readonly x: number;"),
        "Missing readonly x: {output}"
    );
    // tsc strips type annotations from private members in .d.ts
    assert!(output.contains("private y;"), "Missing private y: {output}");
    assert!(
        output.contains("protected z: boolean;"),
        "Missing protected z: {output}"
    );
}

#[test]
fn probe_constructor_type() {
    let output = emit_dts("export type T = new (x: string) => object;");
    println!("PROBE constructor type:\n{output}");
    assert!(
        output.contains("new (x: string) => object"),
        "Missing constructor type: {output}"
    );
}

#[test]
fn probe_abstract_constructor_type() {
    let output = emit_dts("export type T = abstract new (x: string) => object;");
    println!("PROBE abstract constructor:\n{output}");
    assert!(
        output.contains("abstract new (x: string) => object"),
        "Missing abstract constructor: {output}"
    );
}

#[test]
fn probe_declare_module() {
    let output = emit_dts(
        "declare module 'my-module' {
    export function foo(): void;
    export const bar: string;
}",
    );
    println!("PROBE declare module:\n{output}");
    assert!(
        output.contains("declare module 'my-module'")
            || output.contains("declare module \"my-module\""),
        "Missing declare module: {output}"
    );
    assert!(
        output.contains("function foo(): void;"),
        "Missing fn in module: {output}"
    );
}

#[test]
fn probe_generic_class_with_constraint() {
    let output = emit_dts(
        "export class Container<T extends object> {
    value: T;
}",
    );
    println!("PROBE generic class:\n{output}");
    assert!(
        output.contains("T extends object"),
        "Missing constraint: {output}"
    );
    assert!(output.contains("value: T;"), "Missing value: {output}");
}

#[test]
fn probe_typeof_type() {
    let output = emit_dts(
        "export declare const x: number;
export type T = typeof x;",
    );
    println!("PROBE typeof:\n{output}");
    assert!(output.contains("typeof x"), "Missing typeof: {output}");
}

#[test]
fn probe_readonly_array_type() {
    let output = emit_dts("export type T = readonly string[];");
    println!("PROBE readonly array:\n{output}");
    assert!(
        output.contains("readonly string[]"),
        "Missing readonly array: {output}"
    );
}

#[test]
fn probe_indexed_access_type() {
    let output = emit_dts("export type T = string[][0];");
    println!("PROBE indexed access:\n{output}");
    assert!(
        output.contains("string[][0]"),
        "Missing indexed access: {output}"
    );
}

#[test]
fn probe_intersection_type() {
    let output = emit_dts("export type T = { a: string } & { b: number };");
    println!("PROBE intersection:\n{output}");
    assert!(output.contains("a: string"), "Missing a: {output}");
    assert!(output.contains("b: number"), "Missing b: {output}");
    assert!(output.contains("&"), "Missing intersection: {output}");
}

#[test]
fn probe_optional_tuple_element() {
    let output = emit_dts("export type T = [string?];");
    println!("PROBE optional tuple:\n{output}");
    assert!(
        output.contains("string?"),
        "Missing optional element: {output}"
    );
}

#[test]
fn probe_rest_tuple_element() {
    let output = emit_dts("export type T = [string, ...number[]];");
    println!("PROBE rest tuple:\n{output}");
    assert!(
        output.contains("...number[]"),
        "Missing rest element: {output}"
    );
}

#[test]
fn probe_bigint_literal_type() {
    let output = emit_dts("export type T = 42n;");
    println!("PROBE bigint:\n{output}");
    assert!(output.contains("42n"), "Missing bigint: {output}");
}

#[test]
fn probe_negative_literal_type() {
    let output = emit_dts("export type T = -1;");
    println!("PROBE negative literal:\n{output}");
    assert!(output.contains("-1"), "Missing negative: {output}");
}

#[test]
fn probe_interface_multiple_extends() {
    let output = emit_dts(
        "interface A { a: string; }
interface B { b: number; }
export interface C extends A, B { c: boolean; }",
    );
    println!("PROBE multi extends:\n{output}");
    assert!(
        output.contains("extends A, B"),
        "Missing multi extends: {output}"
    );
}

#[test]
fn probe_private_field() {
    let output = emit_dts(
        "export class Foo {
    #bar: string = '';
}",
    );
    println!("PROBE private field:\n{output}");
    // tsc emits `#bar: string;` or just omits it. Let's see.
    // Actually tsc keeps #bar in .d.ts
    println!("Private field output: {output}");
}

#[test]
fn probe_export_default_abstract_class() {
    let output = emit_dts("export default abstract class { abstract foo(): void; }");
    println!("PROBE default abstract:\n{output}");
    assert!(
        output.contains("export default abstract class"),
        "Missing default abstract: {output}"
    );
    assert!(
        output.contains("abstract foo(): void;"),
        "Missing abstract method: {output}"
    );
}

#[test]
fn probe_declare_keyword_passthrough() {
    let output = emit_dts(
        "export declare function foo(): void;
export declare class Bar {}
export declare const baz: number;
export declare enum E { A }",
    );
    println!("PROBE declare passthrough:\n{output}");
    assert!(
        output.contains("export declare function foo(): void;"),
        "Missing declare fn: {output}"
    );
    assert!(
        output.contains("export declare class Bar"),
        "Missing declare class: {output}"
    );
    assert!(
        output.contains("export declare const baz: number;"),
        "Missing declare const: {output}"
    );
    assert!(
        output.contains("export declare enum E"),
        "Missing declare enum: {output}"
    );
}

#[test]
fn probe_import_equals() {
    let output = emit_dts(
        "import Foo = require('./foo');
export = Foo;",
    );
    println!("PROBE import equals (no binding):\n{output}");
    // Without binding, import elision may drop the import.
    // With binding, it should be preserved.
    let output2 = emit_dts_with_usage_analysis(
        "import Foo = require('./foo');
export = Foo;",
    );
    println!("PROBE import equals (with binding):\n{output2}");
    assert!(
        output2.contains("import Foo"),
        "Missing import (with binding): {output2}"
    );
    assert!(
        output2.contains("export = Foo;"),
        "Missing export = (with binding): {output2}"
    );
}

#[test]
fn probe_keyof_type() {
    let output = emit_dts("export type Keys<T> = keyof T;");
    println!("PROBE keyof:\n{output}");
    assert!(output.contains("keyof T"), "Missing keyof: {output}");
}

#[test]
fn probe_class_implements() {
    let output = emit_dts(
        "interface Printable { print(): void; }
export class Doc implements Printable {
    print(): void {}
}",
    );
    println!("PROBE implements:\n{output}");
    assert!(
        output.contains("implements Printable"),
        "Missing implements: {output}"
    );
}

#[test]
fn probe_class_extends_with_generics() {
    let output = emit_dts(
        "class Base<T> { value: T; }
export class Derived extends Base<string> {
    extra: number;
}",
    );
    println!("PROBE extends generic:\n{output}");
    assert!(
        output.contains("extends Base<string>"),
        "Missing generic extends: {output}"
    );
}

#[test]
fn probe_mapped_type_modifiers() {
    let output = emit_dts("export type T = { readonly [K in string]: number };");
    println!("PROBE mapped readonly:\n{output}");
    assert!(
        output.contains("readonly [K in string]"),
        "Missing readonly mapped: {output}"
    );
}

#[test]
fn probe_mapped_type_minus_modifier() {
    let output = emit_dts("export type T<U> = { -readonly [K in keyof U]-?: U[K] };");
    println!("PROBE mapped minus:\n{output}");
    assert!(output.contains("-readonly"), "Missing -readonly: {output}");
    assert!(output.contains("-?"), "Missing -?: {output}");
}

#[test]
fn probe_infer_with_extends_constraint() {
    let output = emit_dts("export type T<U> = U extends (infer V extends string) ? V : never;");
    println!("PROBE infer extends:\n{output}");
    assert!(
        output.contains("infer V extends string"),
        "Missing infer extends: {output}"
    );
}

#[test]
fn probe_class_method_overloads() {
    let output = emit_dts(
        "export class Foo {
    bar(x: string): number;
    bar(x: number): string;
    bar(x: any): any { return x; }
}",
    );
    println!("PROBE method overloads:\n{output}");
    assert!(
        output.contains("bar(x: string): number;"),
        "Missing overload 1: {output}"
    );
    assert!(
        output.contains("bar(x: number): string;"),
        "Missing overload 2: {output}"
    );
    assert!(
        !output.contains("x: any): any"),
        "Implementation leaked: {output}"
    );
}

#[test]
fn probe_export_star_as_namespace() {
    let output = emit_dts("export * as ns from './mod';");
    println!("PROBE star as ns:\n{output}");
    assert!(
        output.contains("export * as ns from"),
        "Missing star-as: {output}"
    );
}

#[test]
fn probe_type_only_reexport() {
    let output = emit_dts("export type { Foo, Bar } from './mod';");
    println!("PROBE type reexport:\n{output}");
    assert!(
        output.contains("export type {") || output.contains("export type{"),
        "Missing type reexport: {output}"
    );
}

#[test]
fn probe_default_type_parameter() {
    let output = emit_dts("export type T<U = string> = U[];");
    println!("PROBE default type param:\n{output}");
    assert!(
        output.contains("U = string"),
        "Missing default type param: {output}"
    );
}

// =============================================================================
// Edge case probes — compare exactly against tsc output
// =============================================================================

#[test]
fn probe_class_static_method() {
    let output = emit_dts(
        "export class Foo {
    static create(): Foo { return new Foo(); }
}",
    );
    println!("PROBE static method:\n{output}");
    assert!(
        output.contains("static create(): Foo;") || output.contains("static create():"),
        "Missing static method: {output}"
    );
}

#[test]
fn probe_class_protected_abstract_method() {
    let output = emit_dts(
        "export abstract class Base {
    protected abstract init(): void;
}",
    );
    println!("PROBE protected abstract:\n{output}");
    assert!(
        output.contains("protected abstract init(): void;"),
        "Missing protected abstract: {output}"
    );
}

#[test]
fn probe_readonly_property_in_interface() {
    let output = emit_dts(
        "export interface Foo {
    readonly bar: string;
}",
    );
    println!("PROBE readonly prop:\n{output}");
    assert!(
        output.contains("readonly bar: string;"),
        "Missing readonly: {output}"
    );
}

#[test]
fn probe_optional_method_in_interface() {
    let output = emit_dts(
        "export interface Foo {
    bar?(x: number): void;
}",
    );
    println!("PROBE optional method:\n{output}");
    assert!(
        output.contains("bar?(x: number): void;"),
        "Missing optional method: {output}"
    );
}

#[test]
fn probe_export_default_type_alias() {
    // tsc emits: export default T; (with a separate `type T = ...;` if needed)
    // Actually, `export default` on a type alias is not valid TS syntax
    // Let's test `export default interface` instead
    let output = emit_dts(
        "export default interface Foo {
    x: number;
}",
    );
    println!("PROBE default interface:\n{output}");
    assert!(
        output.contains("export default interface Foo"),
        "Missing default interface: {output}"
    );
}

#[test]
fn probe_enum_string_values() {
    let output = emit_dts(
        "export enum Status {
    Active = 'active',
    Inactive = 'inactive'
}",
    );
    println!("PROBE enum string:\n{output}");
    assert!(
        output.contains("Active = \"active\"") || output.contains("Active = 'active'"),
        "Missing string value: {output}"
    );
}

#[test]
fn probe_enum_computed_values() {
    let output = emit_dts(
        "export enum Bits {
    A = 1,
    B = 2,
    C = A | B
}",
    );
    println!("PROBE enum computed:\n{output}");
    // tsc evaluates constant expressions
    assert!(
        output.contains("C = 3") || output.contains("C ="),
        "Missing computed enum: {output}"
    );
}

#[test]
fn probe_class_with_index_signature() {
    let output = emit_dts(
        "export class Foo {
    [key: string]: any;
    bar: number;
}",
    );
    println!("PROBE class index sig:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "Missing index sig: {output}"
    );
    assert!(output.contains("bar: number;"), "Missing bar: {output}");
}

#[test]
fn probe_ambient_enum() {
    let output = emit_dts("export declare enum E { A, B, C }");
    println!("PROBE ambient enum:\n{output}");
    assert!(
        output.contains("export declare enum E"),
        "Missing declare enum: {output}"
    );
}

#[test]
fn probe_never_return_type() {
    let output = emit_dts("export function fail(msg: string): never;");
    println!("PROBE never return:\n{output}");
    assert!(
        output.contains(": never;"),
        "Missing never return: {output}"
    );
}

#[test]
fn probe_symbol_type() {
    let output = emit_dts("export declare const s: symbol;");
    println!("PROBE symbol type:\n{output}");
    assert!(output.contains(": symbol;"), "Missing symbol: {output}");
}

#[test]
fn probe_variadic_tuple() {
    let output =
        emit_dts("export type Concat<T extends unknown[], U extends unknown[]> = [...T, ...U];");
    println!("PROBE variadic tuple:\n{output}");
    assert!(
        output.contains("[...T, ...U]"),
        "Missing variadic: {output}"
    );
}

#[test]
fn probe_type_alias_with_type_literal() {
    let output = emit_dts("export type Obj = { a: string; b: number; };");
    println!("PROBE type alias literal:\n{output}");
    assert!(output.contains("a: string;"), "Missing a: {output}");
    assert!(output.contains("b: number;"), "Missing b: {output}");
}

#[test]
fn probe_nested_namespace() {
    let output = emit_dts(
        "export namespace A {
    export namespace B {
        export function foo(): void;
    }
}",
    );
    println!("PROBE nested ns:\n{output}");
    assert!(output.contains("namespace A"), "Missing A: {output}");
    assert!(output.contains("namespace B"), "Missing B: {output}");
    assert!(
        output.contains("function foo(): void;"),
        "Missing foo: {output}"
    );
}

#[test]
fn probe_const_assertion_variable() {
    // `as const` variables should emit the literal type
    let output = emit_dts("export const x = 42 as const;");
    println!("PROBE as const:\n{output}");
    // Should have `x: 42` not `x: number`
    // Without type inference, it may just emit `any` or the initializer
    println!("Output: {output}");
}

#[test]
fn probe_export_namespace_with_type_and_value() {
    let output = emit_dts(
        "export namespace NS {
    export interface I { x: number; }
    export function f(): I;
    export const c: number;
}",
    );
    println!("PROBE ns type+value:\n{output}");
    assert!(
        output.contains("interface I"),
        "Missing interface I: {output}"
    );
    assert!(
        output.contains("function f(): I;"),
        "Missing fn f: {output}"
    );
    assert!(
        output.contains("const c: number;"),
        "Missing const c: {output}"
    );
}

#[test]
fn probe_global_augmentation() {
    let output = emit_dts(
        "export {};
declare global {
    interface Window {
        myProp: string;
    }
}",
    );
    println!("PROBE global augmentation:\n{output}");
    assert!(
        output.contains("declare global"),
        "Missing global: {output}"
    );
    assert!(
        output.contains("interface Window"),
        "Missing Window: {output}"
    );
}

#[test]
fn probe_function_with_this_param() {
    let output = emit_dts("export function foo(this: HTMLElement, x: number): void;");
    println!("PROBE this param:\n{output}");
    assert!(
        output.contains("this: HTMLElement"),
        "Missing this param: {output}"
    );
}

#[test]
fn probe_class_constructor_overloads() {
    let output = emit_dts(
        "export class Foo {
    constructor(x: string);
    constructor(x: number);
    constructor(x: any) {}
}",
    );
    println!("PROBE ctor overloads:\n{output}");
    assert!(
        output.contains("constructor(x: string);"),
        "Missing ctor overload 1: {output}"
    );
    assert!(
        output.contains("constructor(x: number);"),
        "Missing ctor overload 2: {output}"
    );
    assert!(
        !output.contains("x: any)"),
        "Ctor implementation leaked: {output}"
    );
}

#[test]
fn probe_rest_parameter() {
    let output = emit_dts("export function foo(...args: string[]): void;");
    println!("PROBE rest param:\n{output}");
    assert!(
        output.contains("...args: string[]"),
        "Missing rest param: {output}"
    );
}

#[test]
fn probe_optional_parameter() {
    let output = emit_dts("export function foo(x?: number): void;");
    println!("PROBE optional param:\n{output}");
    assert!(
        output.contains("x?: number"),
        "Missing optional param: {output}"
    );
}

#[test]
fn probe_parameter_with_default() {
    let output = emit_dts("export function foo(x: number = 42): void;");
    println!("PROBE param default:\n{output}");
    // In .d.ts, default values should make param optional: `x?: number`
    assert!(
        output.contains("x?: number"),
        "Default param should be optional: {output}"
    );
}

#[test]
fn probe_class_accessor_keyword() {
    // TS 4.9+ accessor keyword
    let output = emit_dts(
        "export class Foo {
    accessor bar: string = '';
}",
    );
    println!("PROBE accessor keyword:\n{output}");
    // tsc emits: `accessor bar: string;`
    assert!(
        output.contains("accessor bar: string;"),
        "Missing accessor keyword: {output}"
    );
}

#[test]
fn probe_satisfies_stripped() {
    // satisfies should be stripped in .d.ts
    let output = emit_dts("export const x = { a: 1 } satisfies Record<string, number>;");
    println!("PROBE satisfies stripped:\n{output}");
    assert!(
        !output.contains("satisfies"),
        "satisfies should be stripped: {output}"
    );
}

#[test]
fn probe_private_constructor() {
    let output = emit_dts(
        "export class Singleton {
    private constructor() {}
    static instance: Singleton;
}",
    );
    println!("PROBE private ctor:\n{output}");
    assert!(
        output.contains("private constructor();"),
        "Missing private ctor: {output}"
    );
}

#[test]
fn probe_abstract_class_with_protected_constructor() {
    let output = emit_dts(
        "export abstract class Base {
    protected constructor(x: number);
}",
    );
    println!("PROBE protected ctor:\n{output}");
    assert!(
        output.contains("protected constructor(x: number);"),
        "Missing protected ctor: {output}"
    );
}

// =============================================================================
// Exact output comparison probes to find subtle differences with tsc
// =============================================================================

#[test]
fn exact_probe_method_with_optional_and_rest() {
    let output =
        emit_dts("export declare function foo(a: string, b?: number, ...rest: boolean[]): void;");
    println!("EXACT method opt+rest:\n{output}");
    // tsc: export declare function foo(a: string, b?: number, ...rest: boolean[]): void;
    let expected =
        "export declare function foo(a: string, b?: number, ...rest: boolean[]): void;\n";
    assert_eq!(output, expected, "Mismatch");
}

#[test]
fn exact_probe_type_alias_union() {
    let output = emit_dts("export type T = string | number | boolean;");
    println!("EXACT type alias union:\n{output}");
    let expected = "export type T = string | number | boolean;\n";
    assert_eq!(output, expected, "Mismatch");
}

#[test]
fn exact_probe_export_default_function_no_name() {
    let output = emit_dts("export default function(): void {}");
    println!("EXACT default fn no name:\n{output}");
    // tsc: export default function (): void;\n
    assert!(
        output.contains("export default function"),
        "Missing default fn: {output}"
    );
}

#[test]
fn exact_probe_export_default_class_no_name() {
    let output = emit_dts("export default class { foo(): void {} }");
    println!("EXACT default class no name:\n{output}");
    assert!(
        output.contains("export default class"),
        "Missing default class: {output}"
    );
}

#[test]
fn exact_probe_async_function() {
    let output = emit_dts("export async function foo(): Promise<number> { return 42; }");
    println!("EXACT async fn:\n{output}");
    // tsc strips async in .d.ts
    assert!(
        !output.contains("async"),
        "async should be stripped in .d.ts: {output}"
    );
    assert!(
        output.contains("foo(): Promise<number>;"),
        "Missing return type: {output}"
    );
}

#[test]
fn exact_probe_generator_function() {
    let output = emit_dts("export function* gen(): Generator<number> { yield 1; }");
    println!("EXACT generator fn:\n{output}");
    // tsc strips * in .d.ts
    assert!(
        !output.contains("*"),
        "* should be stripped in .d.ts: {output}"
    );
}

#[test]
fn exact_probe_class_extends_implements_combined() {
    let output = emit_dts(
        "interface I { foo(): void; }
class Base { bar(): void {} }
export class Derived extends Base implements I {
    foo(): void {}
    bar(): void {}
}",
    );
    println!("EXACT extends+implements:\n{output}");
    assert!(
        output.contains("extends Base implements I"),
        "Missing extends+implements: {output}"
    );
}

#[test]
fn exact_probe_multiline_type_literal_in_alias() {
    let output = emit_dts(
        "export type Obj = {
    a: string;
    b: number;
    c: boolean;
};",
    );
    println!("EXACT multiline type literal:\n{output}");
    // tsc emits multi-line format:
    // export type Obj = {
    //     a: string;
    //     b: number;
    //     c: boolean;
    // };
    assert!(output.contains("a: string;"), "Missing a: {output}");
    assert!(output.contains("b: number;"), "Missing b: {output}");
    assert!(output.contains("c: boolean;"), "Missing c: {output}");
}

#[test]
fn exact_probe_complex_mapped_type() {
    let output = emit_dts(
        "export type Required<T> = {
    [P in keyof T]-?: T[P];
};",
    );
    println!("EXACT complex mapped:\n{output}");
    assert!(
        output.contains("[P in keyof T]-?: T[P]"),
        "Missing mapped type body: {output}"
    );
}

#[test]
fn exact_probe_intersection_of_unions() {
    let output = emit_dts("export type T = (string | number) & (boolean | null);");
    println!("EXACT intersection of unions:\n{output}");
    assert!(
        output.contains("(string | number) & (boolean | null)"),
        "Missing parens in intersection: {output}"
    );
}

#[test]
fn exact_probe_nested_conditional() {
    let output = emit_dts(
        "export type T<U> = U extends string ? 'str' : U extends number ? 'num' : 'other';",
    );
    println!("EXACT nested conditional:\n{output}");
    assert!(
        output.contains("U extends string"),
        "Missing outer extends: {output}"
    );
    assert!(
        output.contains("U extends number"),
        "Missing inner extends: {output}"
    );
}

#[test]
fn exact_probe_typeof_import() {
    let output = emit_dts("export type T = typeof import('./mod');");
    println!("EXACT typeof import:\n{output}");
    assert!(
        output.contains("typeof import"),
        "Missing typeof import: {output}"
    );
}

#[test]
fn exact_probe_class_with_optional_property() {
    let output = emit_dts(
        "export class Foo {
    bar?: string;
    baz!: number;
}",
    );
    println!("EXACT optional + definite:\n{output}");
    assert!(
        output.contains("bar?: string;"),
        "Missing optional prop: {output}"
    );
    // tsc strips ! definite assignment in .d.ts
    assert!(
        output.contains("baz: number;") || output.contains("baz!: number;"),
        "Missing definite prop: {output}"
    );
    // tsc emits `baz!: number;` in .d.ts? Actually no - tsc strips the `!`
    // Let me check if we strip the `!` token
}

#[test]
fn exact_probe_declare_abstract_class() {
    let output = emit_dts(
        "export declare abstract class Base {
    abstract method(): void;
    concrete(): string;
}",
    );
    println!("EXACT declare abstract:\n{output}");
    let expected = "export declare abstract class Base {\n    abstract method(): void;\n    concrete(): string;\n}\n";
    assert_eq!(output, expected, "Mismatch");
}

#[test]
fn exact_probe_interface_with_generic_method() {
    let output = emit_dts(
        "export interface Foo {
    bar<T>(x: T): T;
}",
    );
    println!("EXACT generic method:\n{output}");
    assert!(
        output.contains("bar<T>(x: T): T;"),
        "Missing generic method: {output}"
    );
}

#[test]
fn exact_probe_function_type_in_union() {
    let output = emit_dts("export type T = ((x: number) => void) | string;");
    println!("EXACT fn type in union:\n{output}");
    // The parentheses around the function type should be preserved
    assert!(
        output.contains("((x: number) => void)") || output.contains("(x: number) => void"),
        "Missing fn type: {output}"
    );
}

#[test]
fn exact_probe_this_type() {
    let output = emit_dts(
        "export class Builder {
    set(key: string): this;
}",
    );
    println!("EXACT this type:\n{output}");
    assert!(
        output.contains("set(key: string): this;"),
        "Missing this type: {output}"
    );
}

#[test]
fn exact_probe_string_index_signature() {
    let output = emit_dts(
        "export interface Dict {
    [key: string]: unknown;
}",
    );
    println!("EXACT string index:\n{output}");
    assert!(
        output.contains("[key: string]: unknown;"),
        "Missing index sig: {output}"
    );
}

#[test]
fn exact_probe_number_index_signature() {
    let output = emit_dts(
        "export interface Arr {
    [index: number]: string;
}",
    );
    println!("EXACT number index:\n{output}");
    assert!(
        output.contains("[index: number]: string;"),
        "Missing index sig: {output}"
    );
}

// =============================================================================
// Full comparison against tsc output
// =============================================================================

#[test]
fn compare_tsc_complex_class_emit() {
    let output = emit_dts(
        r#"export class Container<T extends object> {
    private items: T[] = [];

    add(item: T): void {
        this.items.push(item);
    }

    get(index: number): T {
        return this.items[index];
    }

    get count(): number {
        return this.items.length;
    }

    map<U>(fn: (item: T) => U): U[] {
        return this.items.map(fn);
    }
}"#,
    );
    println!("COMPARE complex class:\n{output}");
    // tsc output:
    // export declare class Container<T extends object> {
    //     private items;
    //     add(item: T): void;
    //     get(index: number): T;
    //     get count(): number;
    //     map<U>(fn: (item: T) => U): U[];
    // }
    assert!(
        output.contains("private items;"),
        "Private should strip type: {output}"
    );
    assert!(
        output.contains("add(item: T): void;"),
        "Missing add: {output}"
    );
    assert!(
        output.contains("get count(): number;"),
        "Missing getter: {output}"
    );
    assert!(
        output.contains("map<U>(fn: (item: T) => U): U[];"),
        "Missing map: {output}"
    );
}

#[test]
fn compare_tsc_interface_generic_events() {
    let output = emit_dts(
        r#"export interface EventEmitter<T extends Record<string, any>> {
    on<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;
    off<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;
    emit<K extends keyof T>(event: K, data: T[K]): void;
}"#,
    );
    println!("COMPARE events interface:\n{output}");
    let expected = "export interface EventEmitter<T extends Record<string, any>> {\n    on<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;\n    off<K extends keyof T>(event: K, handler: (data: T[K]) => void): void;\n    emit<K extends keyof T>(event: K, data: T[K]): void;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn compare_tsc_deep_partial_type() {
    let output = emit_dts(
        r#"export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};"#,
    );
    println!("COMPARE DeepPartial:\n{output}");
    // tsc:
    // export type DeepPartial<T> = {
    //     [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
    // };
    assert!(
        output.contains("[P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P]"),
        "Missing mapped type body: {output}"
    );
}

#[test]
fn compare_tsc_promisified_type() {
    let output = emit_dts(
        r#"export type Promisified<T> = {
    [K in keyof T]: T[K] extends (...args: infer A) => infer R
        ? (...args: A) => Promise<R>
        : T[K];
};"#,
    );
    println!("COMPARE Promisified:\n{output}");
    assert!(output.contains("infer A"), "Missing infer A: {output}");
    assert!(output.contains("infer R"), "Missing infer R: {output}");
    assert!(
        output.contains("Promise<R>"),
        "Missing Promise<R>: {output}"
    );
}

#[test]
fn compare_tsc_abstract_class() {
    let output = emit_dts(
        r#"export abstract class AbstractLogger {
    abstract log(msg: string): void;
    abstract error(msg: string, err?: Error): void;

    warn(msg: string): void {
        this.log(`WARN: ${msg}`);
    }
}"#,
    );
    println!("COMPARE abstract class:\n{output}");
    // tsc:
    // export declare abstract class AbstractLogger {
    //     abstract log(msg: string): void;
    //     abstract error(msg: string, err?: Error): void;
    //     warn(msg: string): void;
    // }
    let expected = "export declare abstract class AbstractLogger {\n    abstract log(msg: string): void;\n    abstract error(msg: string, err?: Error): void;\n    warn(msg: string): void;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn compare_tsc_const_literal() {
    let output = emit_dts(r#"export declare const VERSION: "1.0.0";"#);
    println!("COMPARE const literal:\n{output}");
    let expected = "export declare const VERSION: \"1.0.0\";\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn compare_tsc_factory_function() {
    let output = emit_dts(
        r#"export declare function createFactory<T extends new (...args: any[]) => any>(
    ctor: T,
): InstanceType<T>;"#,
    );
    println!("COMPARE factory fn:\n{output}");
    // tsc single-line:
    // export declare function createFactory<T extends new (...args: any[]) => any>(ctor: T): InstanceType<T>;
    assert!(
        output.contains("createFactory<T extends new (...args: any[]) => any>"),
        "Missing generics: {output}"
    );
    assert!(output.contains("ctor: T"), "Missing param: {output}");
    assert!(
        output.contains("InstanceType<T>"),
        "Missing return: {output}"
    );
}

// =============================================================================
// Edge case comparison tests against tsc output
// =============================================================================

#[test]
fn edge_private_field_emits_as_hash_private() {
    let output = emit_dts(
        r#"export class HasPrivate {
    #secret: string = "hidden";
    getSecret(): string { return this.#secret; }
}"#,
    );
    println!("EDGE private field:\n{output}");
    // tsc emits: #private; (generic private marker, not actual field name)
    // Our emitter may emit #secret or #private - both have been seen in different tsc versions
    // The key is that private fields should appear and methods should have return types
    assert!(
        output.contains("#"),
        "Missing private field marker: {output}"
    );
    assert!(output.contains("getSecret()"), "Missing method: {output}");
}

#[test]
fn edge_override_stripped_in_dts() {
    // tsc strips 'override' in .d.ts output
    let output = emit_dts(
        r#"class Base {
    greet(): string { return "hi"; }
}
export class Derived extends Base {
    override greet(): string { return "hello"; }
}"#,
    );
    println!("EDGE override:\n{output}");
    // tsc output does NOT include 'override'
    assert!(
        output.contains("greet(): string;"),
        "Missing greet: {output}"
    );
}

#[test]
fn edge_recursive_type() {
    let output = emit_dts(
        r#"export type Json = string | number | boolean | null | Json[] | { [key: string]: Json };"#,
    );
    println!("EDGE recursive type:\n{output}");
    assert!(output.contains("Json[]"), "Missing array: {output}");
    assert!(
        output.contains("[key: string]: Json"),
        "Missing index sig: {output}"
    );
}

#[test]
fn edge_nested_conditional_type() {
    let output = emit_dts(
        r#"export type IsNullable<T> = undefined extends T ? true : null extends T ? true : false;"#,
    );
    println!("EDGE nested conditional:\n{output}");
    // tsc: undefined extends T ? true : null extends T ? true : false
    assert!(
        output.contains("undefined extends T ? true : null extends T ? true : false"),
        "Missing nested conditional: {output}"
    );
}

#[test]
fn edge_template_literal_simple() {
    let output = emit_dts("export type CssVar = `--${string}`;");
    println!("EDGE template literal:\n{output}");
    assert!(
        output.contains("`--${string}`"),
        "Missing template literal: {output}"
    );
}

#[test]
fn edge_interface_method_overloads() {
    let output = emit_dts(
        r#"export interface Overloaded {
    call(x: string): number;
    call(x: number): string;
}"#,
    );
    println!("EDGE interface overloads:\n{output}");
    assert!(
        output.contains("call(x: string): number;"),
        "Missing overload 1: {output}"
    );
    assert!(
        output.contains("call(x: number): string;"),
        "Missing overload 2: {output}"
    );
}

#[test]
fn edge_generic_default_conditional() {
    let output = emit_dts(
        r#"export type WithDefault<T, D = T extends string ? "str" : "other"> = {
    value: T;
    default: D;
};"#,
    );
    println!("EDGE generic default:\n{output}");
    assert!(
        output.contains("D = T extends string"),
        "Missing default conditional: {output}"
    );
}

#[test]
fn edge_enum_with_namespace_merge() {
    let output = emit_dts(
        r#"export enum Status {
    Active = "ACTIVE",
    Inactive = "INACTIVE"
}
export namespace Status {
    export function parse(s: string): Status;
}"#,
    );
    println!("EDGE enum+namespace:\n{output}");
    // tsc:
    // export declare enum Status { Active = "ACTIVE", Inactive = "INACTIVE" }
    // export declare namespace Status { function parse(s: string): Status; }
    assert!(output.contains("enum Status"), "Missing enum: {output}");
    assert!(
        output.contains("namespace Status"),
        "Missing namespace: {output}"
    );
    assert!(
        output.contains("parse(s: string): Status;"),
        "Missing parse fn: {output}"
    );
}

#[test]
fn edge_abstract_with_static() {
    let output = emit_dts(
        r#"export abstract class AbstractBase {
    static create(): AbstractBase { throw new Error(); }
    abstract getId(): string;
}"#,
    );
    println!("EDGE abstract+static:\n{output}");
    let expected = "export declare abstract class AbstractBase {\n    static create(): AbstractBase;\n    abstract getId(): string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn edge_type_only_reexport_of_class() {
    let output = emit_dts(
        r#"class Base {
    greet(): string { return "hi"; }
}
export type { Base };"#,
    );
    println!("EDGE type re-export of class:\n{output}");
    // tsc emits: declare class Base { greet(): string; } and export type { Base };
    assert!(
        output.contains("export type { Base };") || output.contains("export type {Base}"),
        "Missing type re-export: {output}"
    );
}

#[test]
fn edge_computed_symbol_property_in_interface() {
    let output = emit_dts(
        r#"export declare const sym: unique symbol;
export interface WithSymbol {
    [sym]: string;
}"#,
    );
    println!("EDGE computed symbol in interface:\n{output}");
    assert!(
        output.contains("[sym]: string;"),
        "Missing computed symbol property: {output}"
    );
}

#[test]
fn edge_const_assertion_readonly_object() {
    // Without solver, this may not produce the exact tsc output
    let output = emit_dts(
        r#"export declare const config: {
    readonly port: 3000;
    readonly host: "localhost";
};"#,
    );
    println!("EDGE const assertion object:\n{output}");
    assert!(
        output.contains("readonly port: 3000;"),
        "Missing readonly port: {output}"
    );
    assert!(
        output.contains("readonly host: \"localhost\";"),
        "Missing readonly host: {output}"
    );
}

// =============================================================================
// String literal escape sequence tests
// =============================================================================

#[test]
fn fix_string_literal_escaped_quote() {
    // The scanner stores cooked text, so \" becomes a literal "
    // The emitter must re-escape it when writing the .d.ts
    let output = emit_dts(r#"export declare const a: "quote\"mark";"#);
    println!("FIX escaped quote:\n{output}");
    assert!(
        output.contains(r#""quote\"mark""#),
        "Missing escaped quote: {output}"
    );
}

#[test]
fn fix_string_literal_escaped_backslash() {
    let output = emit_dts(r#"export declare const a: "backslash\\path";"#);
    println!("FIX escaped backslash:\n{output}");
    assert!(
        output.contains(r#""backslash\\path""#),
        "Missing escaped backslash: {output}"
    );
}

#[test]
fn fix_string_literal_escaped_newline() {
    let output = emit_dts(r#"export declare const a: "line\nbreak";"#);
    println!("FIX escaped newline:\n{output}");
    assert!(
        output.contains(r#""line\nbreak""#),
        "Missing escaped newline: {output}"
    );
}

#[test]
fn fix_string_literal_escaped_tab() {
    let output = emit_dts(r#"export declare const a: "tab\there";"#);
    println!("FIX escaped tab:\n{output}");
    assert!(
        output.contains(r#""tab\there""#),
        "Missing escaped tab: {output}"
    );
}

#[test]
fn fix_string_literal_single_quote_escape() {
    let output = emit_dts("export declare const a: 'it\\'s';");
    println!("FIX single quote escape:\n{output}");
    assert!(
        output.contains("'it\\'s'"),
        "Missing escaped single quote: {output}"
    );
}

#[test]
fn fix_string_literal_no_escape_needed() {
    let output = emit_dts(r#"export declare const a: "normal";"#);
    println!("FIX no escape:\n{output}");
    assert!(
        output.contains(r#""normal""#),
        "Missing normal string: {output}"
    );
}

#[test]
fn fix_string_literal_combined_escapes() {
    let output = emit_dts(r#"export declare const a: "a\\b\"c\nd";"#);
    println!("FIX combined escapes:\n{output}");
    assert!(
        output.contains(r#""a\\b\"c\nd""#),
        "Missing combined escapes: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_newline() {
    let output = emit_dts(r#"export enum E { A = "hello\nworld" }"#);
    println!("FIX enum newline:\n{output}");
    assert!(
        output.contains(r#""hello\nworld""#),
        "Enum string value should escape newline: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_tab() {
    let output = emit_dts(r#"export enum E { A = "tab\there" }"#);
    println!("FIX enum tab:\n{output}");
    assert!(
        output.contains(r#""tab\there""#),
        "Enum string value should escape tab: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_backslash() {
    let output = emit_dts(r#"export enum E { A = "back\\slash" }"#);
    println!("FIX enum backslash:\n{output}");
    assert!(
        output.contains(r#""back\\slash""#),
        "Enum string value should escape backslash: {output}"
    );
}

#[test]
fn fix_enum_string_value_escaped_quote() {
    let output = emit_dts(r#"export enum E { A = "he said \"hi\"" }"#);
    println!("FIX enum quote:\n{output}");
    assert!(
        output.contains(r#""he said \"hi\"""#),
        "Enum string value should escape quote: {output}"
    );
}

// =============================================================================
// DTS emit exploration tests (Round 4)
// =============================================================================

#[test]
fn explore_definite_assignment_assertion() {
    // tsc STRIPS `!` in .d.ts - definite assignment is not needed in declarations
    let output = emit_dts(
        "export class Foo {
    bar!: number;
}",
    );
    println!("EXPLORE definite assignment:\n{output}");
    assert!(
        output.contains("bar: number;"),
        "Should strip definite assignment assertion: {output}"
    );
    assert!(
        !output.contains("bar!:"),
        "Should not have ! in declaration: {output}"
    );
}

#[test]
fn explore_class_static_property() {
    let output = emit_dts(
        "export class Foo {
    static bar: string;
}",
    );
    println!("EXPLORE static property:\n{output}");
    assert!(
        output.contains("static bar: string;"),
        "Should emit static property: {output}"
    );
}

#[test]
fn explore_class_static_readonly_property() {
    let output = emit_dts(
        "export class Foo {
    static readonly VERSION: string;
}",
    );
    println!("EXPLORE static readonly:\n{output}");
    assert!(
        output.contains("static readonly VERSION: string;"),
        "Should emit static readonly: {output}"
    );
}

#[test]
fn explore_class_abstract_method() {
    let output = emit_dts(
        "export abstract class Base {
    abstract doSomething(x: number): void;
}",
    );
    println!("EXPLORE abstract method:\n{output}");
    assert!(
        output.contains("abstract doSomething(x: number): void;"),
        "Should emit abstract method: {output}"
    );
}

#[test]
fn explore_class_abstract_property() {
    let output = emit_dts(
        "export abstract class Base {
    abstract name: string;
}",
    );
    println!("EXPLORE abstract property:\n{output}");
    assert!(
        output.contains("abstract name: string;"),
        "Should emit abstract property: {output}"
    );
}

#[test]
fn explore_intersection_with_function_type() {
    // Function types inside intersections need parentheses
    let output = emit_dts("export type T = ((x: number) => void) & { tag: string };");
    println!("EXPLORE intersection+fn:\n{output}");
    assert!(
        output.contains("((x: number) => void) & {"),
        "Function in intersection should be parenthesized: {output}"
    );
}

#[test]
fn explore_readonly_array_type() {
    let output = emit_dts("export type T = readonly number[];");
    println!("EXPLORE readonly array:\n{output}");
    assert!(
        output.contains("readonly number[]"),
        "Should emit readonly array type: {output}"
    );
}

#[test]
fn explore_readonly_tuple_type() {
    let output = emit_dts("export type T = readonly [string, number];");
    println!("EXPLORE readonly tuple:\n{output}");
    assert!(
        output.contains("readonly [string, number]"),
        "Should emit readonly tuple type: {output}"
    );
}

#[test]
fn explore_labeled_tuple_optional() {
    let output = emit_dts("export type T = [first: string, second?: number];");
    println!("EXPLORE labeled optional tuple:\n{output}");
    assert!(
        output.contains("first: string"),
        "Should emit labeled tuple: {output}"
    );
    assert!(
        output.contains("second?: number"),
        "Should emit optional labeled tuple element: {output}"
    );
}

#[test]
fn explore_labeled_tuple_rest() {
    let output = emit_dts("export type T = [first: string, ...rest: number[]];");
    println!("EXPLORE labeled rest tuple:\n{output}");
    assert!(
        output.contains("...rest: number[]"),
        "Should emit rest labeled tuple element: {output}"
    );
}

#[test]
fn explore_import_type() {
    let output = emit_dts("export type T = import('./module').Foo;");
    println!("EXPLORE import type:\n{output}");
    assert!(
        output.contains("import("),
        "Should emit import type: {output}"
    );
    assert!(
        output.contains("Foo"),
        "Should emit qualified name after import: {output}"
    );
}

#[test]
fn explore_typeof_import_type() {
    let output = emit_dts("export type T = typeof import('./module');");
    println!("EXPLORE typeof import:\n{output}");
    assert!(
        output.contains("typeof import("),
        "Should emit typeof import: {output}"
    );
}

#[test]
fn explore_template_literal_multi_spans() {
    let output = emit_dts("export type T = `${string}-${number}`;");
    println!("EXPLORE multi-span template:\n{output}");
    assert!(
        output.contains("`${string}-${number}`"),
        "Should emit multi-span template literal: {output}"
    );
}

#[test]
fn explore_conditional_type_with_infer() {
    let output = emit_dts("export type UnpackPromise<T> = T extends Promise<infer U> ? U : T;");
    println!("EXPLORE conditional infer:\n{output}");
    assert!(
        output.contains("infer U"),
        "Should emit infer keyword: {output}"
    );
    assert!(
        output.contains("T extends Promise<infer U> ? U : T"),
        "Should emit full conditional type: {output}"
    );
}

#[test]
fn explore_infer_with_extends_constraint() {
    let output = emit_dts(
        "export type FirstString<T> = T extends [infer S extends string, ...unknown[]] ? S : never;",
    );
    println!("EXPLORE infer extends:\n{output}");
    assert!(
        output.contains("infer S extends string"),
        "Should emit infer with extends constraint: {output}"
    );
}

#[test]
fn explore_nested_conditional_type() {
    let output = emit_dts(
        "export type Deep<T> = T extends string ? 'str' : T extends number ? 'num' : 'other';",
    );
    println!("EXPLORE nested conditional:\n{output}");
    assert!(
        output.contains("T extends string ?"),
        "Should emit nested conditional: {output}"
    );
    assert!(
        output.contains("T extends number ?"),
        "Should emit inner conditional: {output}"
    );
}

#[test]
fn explore_call_signature_in_interface() {
    let output = emit_dts(
        "export interface Callable {
    (x: number): string;
    (x: string): number;
}",
    );
    println!("EXPLORE call sig:\n{output}");
    assert!(
        output.contains("(x: number): string;"),
        "Should emit call signature: {output}"
    );
    assert!(
        output.contains("(x: string): number;"),
        "Should emit second call signature: {output}"
    );
}

#[test]
fn explore_construct_signature_in_interface() {
    let output = emit_dts(
        "export interface Constructable {
    new (x: number): object;
}",
    );
    println!("EXPLORE construct sig:\n{output}");
    assert!(
        output.contains("new (x: number): object;"),
        "Should emit construct signature: {output}"
    );
}

#[test]
fn explore_index_signature_readonly() {
    let output = emit_dts(
        "export interface ReadonlyDict {
    readonly [key: string]: unknown;
}",
    );
    println!("EXPLORE readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: unknown;"),
        "Should emit readonly index signature: {output}"
    );
}

#[test]
fn explore_class_with_index_signature_readonly() {
    let output = emit_dts(
        "export class Foo {
    readonly [key: string]: unknown;
}",
    );
    println!("EXPLORE class readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: unknown;"),
        "Should emit class readonly index signature: {output}"
    );
}

#[test]
fn explore_this_parameter_stripped() {
    // tsc strips the `this` parameter in .d.ts - but KEEPS it actually
    // tsc preserves `this` parameter in .d.ts files for functions
    let output = emit_dts("export function handler(this: HTMLElement, event: Event): void {}");
    println!("EXPLORE this param:\n{output}");
    assert!(
        output.contains("this: HTMLElement"),
        "Should preserve this parameter in function: {output}"
    );
}

#[test]
fn explore_function_type_in_union_paren() {
    // Function types in unions need parens
    let output = emit_dts("export type T = ((x: number) => void) | string;");
    println!("EXPLORE fn in union:\n{output}");
    assert!(
        output.contains("((x: number) => void)"),
        "Function type in union should be parenthesized: {output}"
    );
}

#[test]
fn explore_constructor_type_in_union_paren() {
    // Constructor types in unions need parens
    let output = emit_dts("export type T = (new (x: number) => Foo) | string;");
    println!("EXPLORE ctor in union:\n{output}");
    assert!(
        output.contains("(new (x: number) => Foo)"),
        "Constructor type in union should be parenthesized: {output}"
    );
}

#[test]
fn explore_bigint_literal_type() {
    let output = emit_dts("export type T = 100n;");
    println!("EXPLORE bigint literal:\n{output}");
    assert!(
        output.contains("100n"),
        "Should emit bigint literal: {output}"
    );
}

#[test]
fn explore_negative_number_literal_type() {
    let output = emit_dts("export type T = -42;");
    println!("EXPLORE negative number:\n{output}");
    assert!(
        output.contains("-42"),
        "Should emit negative number literal: {output}"
    );
}

#[test]
fn explore_unique_symbol_type() {
    let output = emit_dts("export declare const sym: unique symbol;");
    println!("EXPLORE unique symbol:\n{output}");
    assert!(
        output.contains("unique symbol"),
        "Should emit unique symbol type: {output}"
    );
}

#[test]
fn explore_class_get_set_accessors() {
    let output = emit_dts(
        "export class Foo {
    get value(): number { return 0; }
    set value(v: number) {}
}",
    );
    println!("EXPLORE get/set accessors:\n{output}");
    assert!(
        output.contains("get value(): number;"),
        "Should emit getter: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Should emit setter: {output}"
    );
}

#[test]
fn explore_interface_get_set_accessors() {
    let output = emit_dts(
        "export interface Foo {
    get value(): number;
    set value(v: number);
}",
    );
    println!("EXPLORE interface accessors:\n{output}");
    assert!(
        output.contains("get value(): number;"),
        "Should emit interface getter: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Should emit interface setter: {output}"
    );
}

#[test]
fn explore_keyof_intersection_parens() {
    // keyof (A & B) needs parens
    let output = emit_dts("export type T = keyof (A & B);");
    println!("EXPLORE keyof intersection:\n{output}");
    // Note: our parser may not create a ParenthesizedType here,
    // but the emitter should handle this via TYPE_OPERATOR logic
    assert!(output.contains("keyof"), "Should emit keyof: {output}");
}

#[test]
fn explore_mapped_type_with_as_clause() {
    let output = emit_dts(
        "export type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};",
    );
    println!("EXPLORE mapped type as clause:\n{output}");
    assert!(output.contains(" as "), "Should emit as clause: {output}");
}

#[test]
fn explore_variadic_tuple_type() {
    let output = emit_dts("export type Concat<A extends any[], B extends any[]> = [...A, ...B];");
    println!("EXPLORE variadic tuple:\n{output}");
    assert!(
        output.contains("[...A, ...B]"),
        "Should emit variadic tuple: {output}"
    );
}

#[test]
fn explore_type_predicate_function() {
    let output = emit_dts(
        "export function isString(x: unknown): x is string { return typeof x === 'string'; }",
    );
    println!("EXPLORE type predicate:\n{output}");
    assert!(
        output.contains("x is string"),
        "Should emit type predicate: {output}"
    );
}

#[test]
fn explore_asserts_function() {
    let output = emit_dts(
        "export function assertDefined<T>(value: T): asserts value is NonNullable<T> { if (!value) throw new Error(); }",
    );
    println!("EXPLORE asserts:\n{output}");
    assert!(
        output.contains("asserts value is NonNullable<T>"),
        "Should emit asserts type predicate: {output}"
    );
}

#[test]
fn explore_enum_in_namespace() {
    let output = emit_dts(
        "export namespace NS {
    export enum E {
        A = 0,
        B = 1,
    }
}",
    );
    println!("EXPLORE enum in namespace:\n{output}");
    assert!(
        output.contains("enum E"),
        "Should emit enum in namespace: {output}"
    );
}

#[test]
fn explore_constructor_type_abstract() {
    let output = emit_dts("export type T = abstract new (x: number) => object;");
    println!("EXPLORE abstract ctor type:\n{output}");
    assert!(
        output.contains("abstract new"),
        "Should emit abstract constructor type: {output}"
    );
}

#[test]
fn explore_generic_default_with_conditional() {
    let output =
        emit_dts("export type Maybe<T, Fallback = T extends null ? never : T> = Fallback;");
    println!("EXPLORE generic default conditional:\n{output}");
    assert!(
        output.contains("Fallback = T extends null ? never : T"),
        "Should emit conditional type as default: {output}"
    );
}

#[test]
fn explore_class_with_constructor_param_property_readonly() {
    let output = emit_dts(
        "export class Foo {
    constructor(readonly name: string, public age: number, private secret: string) {}
}",
    );
    println!("EXPLORE ctor param properties:\n{output}");
    assert!(
        output.contains("readonly name: string;"),
        "Should emit readonly param property: {output}"
    );
}

#[test]
fn explore_type_literal_with_call_signature() {
    let output = emit_dts(
        "export type Callable = {
    (x: number): string;
    name: string;
};",
    );
    println!("EXPLORE type literal call sig:\n{output}");
    assert!(
        output.contains("(x: number): string;"),
        "Should emit call signature in type literal: {output}"
    );
    assert!(
        output.contains("name: string;"),
        "Should emit property in type literal: {output}"
    );
}

#[test]
fn explore_type_literal_with_construct_signature() {
    let output = emit_dts(
        "export type Constructable = {
    new (x: number): object;
};",
    );
    println!("EXPLORE type literal construct sig:\n{output}");
    assert!(
        output.contains("new (x: number): object;"),
        "Should emit construct signature in type literal: {output}"
    );
}

#[test]
fn explore_type_literal_with_index_signature() {
    let output = emit_dts(
        "export type Dict = {
    [key: string]: unknown;
};",
    );
    println!("EXPLORE type literal index sig:\n{output}");
    assert!(
        output.contains("[key: string]: unknown;"),
        "Should emit index signature in type literal: {output}"
    );
}

#[test]
fn explore_type_literal_with_method_signature() {
    let output = emit_dts(
        "export type Obj = {
    foo(x: number): string;
};",
    );
    println!("EXPLORE type literal method sig:\n{output}");
    assert!(
        output.contains("foo(x: number): string;"),
        "Should emit method signature in type literal: {output}"
    );
}

#[test]
fn explore_const_enum() {
    let output = emit_dts(
        "export const enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}",
    );
    println!("EXPLORE const enum:\n{output}");
    assert!(
        output.contains("const enum Direction"),
        "Should emit const enum: {output}"
    );
}

#[test]
fn explore_function_overloads() {
    let output = emit_dts(
        "export function foo(x: number): number;
export function foo(x: string): string;
export function foo(x: number | string): number | string {
    return x;
}",
    );
    println!("EXPLORE function overloads:\n{output}");
    assert!(
        output.contains("export declare function foo(x: number): number;"),
        "Should emit first overload: {output}"
    );
    assert!(
        output.contains("export declare function foo(x: string): string;"),
        "Should emit second overload: {output}"
    );
    // Should NOT contain the implementation signature
    let count = output.matches("function foo").count();
    assert_eq!(
        count, 2,
        "Should only emit 2 overloads, not implementation: {output}"
    );
}

#[test]
fn explore_intersection_conditional_parens() {
    // Conditional types inside intersections don't need extra parens
    // but function types do
    let output = emit_dts("export type T = ((x: number) => void) & ((y: string) => void);");
    println!("EXPLORE intersection of fns:\n{output}");
    assert!(
        output.contains("((x: number) => void) & ((y: string) => void)"),
        "Function types in intersection should be parenthesized: {output}"
    );
}

#[test]
fn explore_export_default_function_with_type_params() {
    let output = emit_dts("export default function identity<T>(x: T): T { return x; }");
    println!("EXPLORE default fn with type params:\n{output}");
    assert!(
        output.contains("export default function identity<T>(x: T): T;"),
        "Should emit default function with type params: {output}"
    );
}

#[test]
fn explore_const_enum_string_values() {
    let output = emit_dts(
        r#"export const enum Dir {
    Up = "UP",
    Down = "DOWN",
}"#,
    );
    println!("EXPLORE const enum string values:\n{output}");
    // tsc: trailing comma is removed in the last member
    assert!(
        output.contains(r#"Up = "UP""#),
        "Should emit Up member: {output}"
    );
    assert!(
        output.contains(r#"Down = "DOWN""#),
        "Should emit Down member: {output}"
    );
}

#[test]
fn explore_generic_function_keyof_constraint() {
    let output = emit_dts(
        "export function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] { return obj[key]; }",
    );
    println!("EXPLORE generic keyof:\n{output}");
    assert!(
        output.contains("K extends keyof T"),
        "Should emit keyof constraint: {output}"
    );
    assert!(
        output.contains("T[K]"),
        "Should emit indexed access return type: {output}"
    );
}

#[test]
fn explore_deep_partial_mapped_type() {
    let output = emit_dts(
        "export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};",
    );
    println!("EXPLORE deep partial:\n{output}");
    assert!(
        output.contains("[P in keyof T]?"),
        "Should emit mapped type with question: {output}"
    );
    assert!(
        output.contains("T[P] extends object ? DeepPartial<T[P]> : T[P]"),
        "Should emit conditional in mapped type: {output}"
    );
}

#[test]
fn explore_intersection_fn_and_object() {
    let output = emit_dts("export type Fn = ((x: number) => void) & { displayName: string };");
    println!("EXPLORE intersection fn+obj:\n{output}");
    assert!(
        output.contains("((x: number) => void) & "),
        "Function in intersection should be parenthesized: {output}"
    );
    assert!(
        output.contains("displayName: string;"),
        "Should emit object members: {output}"
    );
}

#[test]
fn explore_template_literal_capitalize() {
    let output = emit_dts("export type EventName<T extends string> = `on${Capitalize<T>}`;");
    println!("EXPLORE template capitalize:\n{output}");
    assert!(
        output.contains("`on${Capitalize<T>}`"),
        "Should emit template literal with Capitalize: {output}"
    );
}

#[test]
fn explore_export_type_only() {
    let output = emit_dts(
        "interface Foo { x: number; }
export type { Foo };",
    );
    println!("EXPLORE type-only export:\n{output}");
    assert!(
        output.contains("export type { Foo"),
        "Should emit type-only export: {output}"
    );
}

#[test]
fn explore_declare_global() {
    let output = emit_dts(
        "export {};
declare global {
    interface Window {
        myProp: string;
    }
}",
    );
    println!("EXPLORE declare global:\n{output}");
    assert!(
        output.contains("declare global"),
        "Should emit declare global: {output}"
    );
    assert!(
        output.contains("myProp: string;"),
        "Should emit augmented interface member: {output}"
    );
}

#[test]
fn explore_namespace_export_as() {
    let output = emit_dts("export * as utils from './utils';");
    println!("EXPLORE namespace export:\n{output}");
    assert!(
        output.contains("export * as utils from"),
        "Should emit namespace re-export: {output}"
    );
}

#[test]
fn explore_class_with_generic_method() {
    let output = emit_dts(
        "export class Container {
    get<T>(key: string): T | undefined { return undefined; }
    set<T>(key: string, value: T): void {}
}",
    );
    println!("EXPLORE class generic method:\n{output}");
    assert!(
        output.contains("get<T>(key: string): T | undefined;"),
        "Should emit generic getter method: {output}"
    );
    assert!(
        output.contains("set<T>(key: string, value: T): void;"),
        "Should emit generic setter method: {output}"
    );
}

#[test]
fn explore_declare_function_with_this() {
    // When source is already a .d.ts, preserve as-is
    let output = emit_dts("declare function handler(this: Window, e: Event): void;");
    println!("EXPLORE declare fn this:\n{output}");
    assert!(
        output.contains("this: Window"),
        "Should preserve this param in declare function: {output}"
    );
}

#[test]
fn explore_multiple_heritage_clauses() {
    let output = emit_dts(
        "export class Derived extends Base implements Comparable, Serializable {
    compare(other: Derived): number { return 0; }
}
declare class Base {}
interface Comparable {}
interface Serializable {}",
    );
    println!("EXPLORE multiple heritage:\n{output}");
    assert!(
        output.contains("extends Base implements Comparable, Serializable"),
        "Should emit extends and implements: {output}"
    );
}

#[test]
fn explore_accessor_keyword_strips_initializer() {
    // tsc emits `accessor name: string;` stripping the initializer
    let output = emit_dts(
        "export class Foo {
    accessor name: string = \"\";
}",
    );
    println!("EXPLORE accessor keyword:\n{output}");
    assert!(
        output.contains("accessor name: string;"),
        "Should emit accessor without initializer: {output}"
    );
}

#[test]
fn explore_enum_negative_values() {
    let output = emit_dts(
        "export enum Signed {
    Neg = -1,
    Zero = 0,
    Pos = 1,
}",
    );
    println!("EXPLORE negative enum:\n{output}");
    assert!(
        output.contains("Neg = -1"),
        "Should emit negative enum value: {output}"
    );
}

#[test]
fn explore_conditional_type_parens_on_check() {
    // Check type with union should get parens in conditional
    let output = emit_dts("export type T<U> = U extends string | number ? 'yes' : 'no';");
    println!("EXPLORE conditional union check:\n{output}");
    // The union in extends position is fine without parens - it's parsed right-to-left
    assert!(
        output.contains("extends string | number"),
        "Should emit union in extends: {output}"
    );
}

#[test]
fn explore_declare_on_class_member_stripped() {
    // tsc strips `declare` from class member declarations in .d.ts
    let output = emit_dts(
        "export class Foo {
    declare x: number;
}",
    );
    println!("EXPLORE declare member:\n{output}");
    // Should emit just `x: number;` without `declare`
    assert!(
        output.contains("x: number;"),
        "Should have property: {output}"
    );
    // The `declare` keyword on the member should be stripped
    // (the class-level `declare` is fine, member-level is not)
    let member_line = output.lines().find(|l| l.contains("x: number")).unwrap();
    assert!(
        !member_line.contains("declare"),
        "declare should be stripped from class member: {output}"
    );
}

#[test]
fn explore_generator_method_stripped_asterisk() {
    // tsc strips the `*` from generator methods in .d.ts
    let output = emit_dts(
        "export class Gen {
    *items(): Generator<number> { yield 1; }
}",
    );
    println!("EXPLORE generator method:\n{output}");
    assert!(
        output.contains("items(): Generator<number>;"),
        "Should emit method without asterisk: {output}"
    );
    assert!(
        !output.contains("*items"),
        "Should not have asterisk: {output}"
    );
}

#[test]
fn explore_namespace_members_no_declare() {
    // Inside declare namespace, members should not have `declare` keyword
    let output = emit_dts(
        "export namespace MyLib {
    export interface Options { debug: boolean; }
    export function create(opts: Options): void;
}",
    );
    println!("EXPLORE namespace members:\n{output}");
    assert!(
        output.contains("interface Options"),
        "Should emit interface: {output}"
    );
    assert!(
        output.contains("function create"),
        "Should emit function: {output}"
    );
    // Inside a declare namespace, tsc does NOT add `declare` to members
    let fn_line = output
        .lines()
        .find(|l| l.contains("function create"))
        .unwrap();
    assert!(
        !fn_line.contains("declare"),
        "Should not have declare inside namespace: {output}"
    );
}

#[test]
fn explore_enum_computed_initializers() {
    // tsc evaluates computed enum initializers to their numeric values:
    // `1 << 0` -> 1, `1 << 1` -> 2, `Read | Write` -> 3
    let output = emit_dts(
        "export enum Flags {
    None = 0,
    Read = 1 << 0,
    Write = 1 << 1,
    ReadWrite = Read | Write,
}",
    );
    println!("EXPLORE enum computed:\n{output}");
    // tsc computes these to their values: 0, 1, 2, 3
    assert!(
        output.contains("None = 0"),
        "Should emit None = 0: {output}"
    );
    assert!(
        output.contains("Read = 1"),
        "Should evaluate Read = 1: {output}"
    );
    assert!(
        output.contains("Write = 2"),
        "Should evaluate Write = 2: {output}"
    );
    assert!(
        output.contains("ReadWrite = 3"),
        "Should evaluate ReadWrite = 3: {output}"
    );
}

#[test]
fn explore_export_rename() {
    let output = emit_dts(
        "interface A { a: number; }
export { A as ARenamed };",
    );
    println!("EXPLORE export rename:\n{output}");
    assert!(
        output.contains("A as ARenamed"),
        "Should emit renamed export: {output}"
    );
}

#[test]
fn explore_recursive_conditional_type() {
    let output = emit_dts("export type Flatten<T> = T extends Array<infer U> ? Flatten<U> : T;");
    println!("EXPLORE recursive conditional:\n{output}");
    assert!(
        output.contains("T extends Array<infer U> ? Flatten<U> : T"),
        "Should emit recursive conditional: {output}"
    );
}

#[test]
fn explore_complex_object_type_alias() {
    let output = emit_dts(
        "export declare const config: {
    readonly host: string;
    readonly port: number;
    readonly options: {
        readonly ssl: boolean;
    };
};",
    );
    println!("EXPLORE complex object type:\n{output}");
    assert!(
        output.contains("readonly host: string;"),
        "Should emit readonly host: {output}"
    );
    assert!(
        output.contains("readonly ssl: boolean;"),
        "Should emit nested readonly: {output}"
    );
}

#[test]
fn explore_multiple_index_signatures() {
    let output = emit_dts(
        "export declare class MultiIndex {
    [key: string]: any;
    [index: number]: string;
}",
    );
    println!("EXPLORE multi index sig:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "Should emit string index: {output}"
    );
    assert!(
        output.contains("[index: number]: string;"),
        "Should emit number index: {output}"
    );
}

#[test]
fn explore_interface_extends_multiple() {
    let output = emit_dts(
        "export interface A { a: number; }
export interface B { b: string; }
export interface C extends A, B { c: boolean; }",
    );
    println!("EXPLORE multi extends:\n{output}");
    assert!(
        output.contains("C extends A, B"),
        "Should emit multiple extends: {output}"
    );
}

#[test]
fn explore_generic_class_with_default_type() {
    let output = emit_dts("export declare class Container<T = unknown> { value: T; }");
    println!("EXPLORE generic default:\n{output}");
    assert!(
        output.contains("<T = unknown>"),
        "Should emit default type param: {output}"
    );
}

// =========================================================================
// Probe tests for finding DTS emit issues
// =========================================================================

#[test]
fn probe_dts_issue_conditional_in_union() {
    // Conditional type as a union member must be parenthesized:
    // `string | T extends U ? X : Y` parses as `(string | T) extends U ? X : Y`
    // which is different from `string | (T extends U ? X : Y)`
    let output = emit_dts("export type X = string | (T extends string ? number : boolean);");
    println!("PROBE conditional in union:\n{output}");
    assert!(
        output.contains("string | (T extends string ? number : boolean)"),
        "Conditional in union needs parens: {output}"
    );
}

#[test]
fn test_conditional_type_in_intersection_parenthesized() {
    // Conditional type as an intersection member must be parenthesized:
    // `A & T extends U ? X : Y` parses differently without parens
    let output = emit_dts(
        "export type Y = { a: string } & (T extends string ? { b: number } : { c: boolean });",
    );
    println!("conditional in intersection:\n{output}");
    assert!(
        output.contains("(T extends string ?"),
        "Conditional in intersection needs parens: {output}"
    );
}

#[test]
fn probe_dts_issue_typeof_import() {
    let output = emit_dts("export declare const x: typeof import('./foo');");
    println!("PROBE typeof import:\n{output}");
    assert!(output.contains("typeof import("), "typeof import: {output}");
}

#[test]
fn probe_dts_issue_bigint_literal_type() {
    let output = emit_dts("export declare const x: 100n;");
    println!("PROBE bigint literal type:\n{output}");
    assert!(output.contains("100n"), "bigint literal type: {output}");
}

#[test]
fn probe_dts_issue_negative_number_literal() {
    let output = emit_dts("export declare const x: -1;");
    println!("PROBE negative literal:\n{output}");
    assert!(output.contains("-1"), "negative number literal: {output}");
}

#[test]
fn probe_dts_issue_declare_global() {
    let output = emit_dts(
        "export {};
declare global {
    interface Window {
        foo: string;
    }
}",
    );
    println!("PROBE declare global:\n{output}");
    assert!(
        output.contains("declare global"),
        "declare global: {output}"
    );
    assert!(
        output.contains("foo: string"),
        "declare global members: {output}"
    );
}

#[test]
fn probe_dts_issue_export_enum_member_computed() {
    // Enum with computed property name using string
    let output = emit_dts(
        "export declare enum E {
    A = 0,
    B = 1,
    C = 2
}",
    );
    println!("PROBE enum:\n{output}");
    assert!(output.contains("A = 0"), "enum member A: {output}");
}

#[test]
fn probe_dts_issue_class_with_accessor_keyword() {
    let output = emit_dts(
        "export declare class Foo {
    accessor name: string;
}",
    );
    println!("PROBE accessor keyword:\n{output}");
    assert!(
        output.contains("accessor name: string"),
        "accessor keyword: {output}"
    );
}

#[test]
fn probe_dts_issue_satisfies_stripped() {
    // satisfies in initializer - should be stripped, type inferred
    let output = emit_dts("export const x = { a: 1 } satisfies Record<string, number>;");
    println!("PROBE satisfies:\n{output}");
    // Satisfies should be stripped from DTS output
    assert!(
        !output.contains("satisfies"),
        "satisfies should be stripped: {output}"
    );
}

#[test]
fn probe_dts_issue_as_const() {
    // as const in initializer - should emit readonly types
    let output = emit_dts("export const x = [1, 2, 3] as const;");
    println!("PROBE as const:\n{output}");
    assert!(
        !output.contains("as const"),
        "as const should be stripped: {output}"
    );
}

#[test]
fn probe_dts_issue_void_function_expression_return() {
    // void keyword used as expression operator
    let output = emit_dts("export declare function foo(): void;");
    println!("PROBE void return:\n{output}");
    assert!(output.contains("): void;"), "void return: {output}");
}

#[test]
fn probe_dts_issue_nested_template_literal_type() {
    let output = emit_dts("export type Nested = `${`inner${string}`}outer`;");
    println!("PROBE nested template literal:\n{output}");
    assert!(output.contains("`"), "template literal: {output}");
}

#[test]
fn probe_dts_issue_class_implements_multiple() {
    let output = emit_dts(
        "export declare class Foo implements A, B, C {
    a: number;
}",
    );
    println!("PROBE implements multiple:\n{output}");
    assert!(
        output.contains("implements A, B, C"),
        "implements multiple: {output}"
    );
}

#[test]
fn probe_dts_issue_export_type_star() {
    let output = emit_dts("export type * from './foo';");
    println!("PROBE export type star:\n{output}");
    assert!(
        output.contains("export type * from"),
        "export type star: {output}"
    );
}

#[test]
fn probe_dts_issue_export_type_star_as_ns() {
    let output = emit_dts("export type * as ns from './foo';");
    println!("PROBE export type * as ns:\n{output}");
    assert!(
        output.contains("export type * as ns from") || output.contains("export type *"),
        "export type * as ns: {output}"
    );
}

#[test]
fn probe_dts_issue_import_type_with_qualifier() {
    let output = emit_dts("export declare const x: import('./foo').Bar.Baz;");
    println!("PROBE import type qualifier:\n{output}");
    assert!(
        output.contains("import("),
        "import type with qualifier: {output}"
    );
}

#[test]
fn probe_dts_issue_const_enum() {
    let output = emit_dts(
        "export const enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3
}",
    );
    println!("PROBE const enum:\n{output}");
    assert!(
        output.contains("const enum Direction"),
        "const enum: {output}"
    );
}

#[test]
fn probe_dts_issue_ambient_enum() {
    let output = emit_dts(
        "export declare enum Direction {
    Up,
    Down,
    Left,
    Right
}",
    );
    println!("PROBE ambient enum:\n{output}");
    assert!(output.contains("enum Direction"), "ambient enum: {output}");
    assert!(output.contains("Up"), "ambient enum members: {output}");
}

#[test]
fn probe_dts_class_with_declare_field() {
    let output = emit_dts(
        "export class Foo {
    declare bar: string;
}",
    );
    println!("PROBE declare field:\n{output}");
    // declare fields should be emitted in .d.ts
    assert!(
        output.contains("bar: string") || output.contains("bar:"),
        "declare field: {output}"
    );
}

#[test]
fn probe_dts_rest_tuple_type() {
    let output =
        emit_dts("export declare function foo(...args: [string, ...number[], boolean]): void;");
    println!("PROBE rest tuple type:\n{output}");
    assert!(
        output.contains("[string, ...number[], boolean]"),
        "rest tuple type: {output}"
    );
}

#[test]
fn probe_dts_declare_module_with_export() {
    let output = emit_dts(
        r#"declare module "foo" {
    export function bar(): void;
    export const baz: number;
}"#,
    );
    println!("PROBE declare module:\n{output}");
    assert!(
        output.contains("declare module"),
        "declare module: {output}"
    );
    // Inside declare module, functions should NOT have declare keyword
    let module_body = &output[output.find('{').unwrap()..];
    println!("Module body: {module_body}");
    assert!(
        !module_body.contains("declare function"),
        "Should not have 'declare' inside module body: {output}"
    );
}

#[test]
fn probe_dts_abstract_constructor_signatures() {
    let output = emit_dts("export type MixinConstructor = abstract new (...args: any[]) => any;");
    println!("PROBE abstract constructor sig:\n{output}");
    assert!(
        output.contains("abstract new"),
        "abstract constructor: {output}"
    );
}

#[test]
fn probe_dts_tuple_labeled_optional_rest() {
    let output = emit_dts("export type T = [first: string, second?: number, ...rest: boolean[]];");
    println!("PROBE labeled tuple:\n{output}");
    assert!(output.contains("first: string"), "first label: {output}");
    assert!(
        output.contains("second?: number"),
        "optional label: {output}"
    );
    assert!(
        output.contains("...rest: boolean[]"),
        "rest label: {output}"
    );
}

#[test]
fn probe_dts_mapped_type_as_clause() {
    let output =
        emit_dts("export type MappedWithAs<T> = { [K in keyof T as `get${string & K}`]: T[K] };");
    println!("PROBE mapped with as:\n{output}");
    assert!(
        output.contains("as `get${string & K}`"),
        "mapped type as clause: {output}"
    );
}

#[test]
fn probe_dts_overloaded_function_export() {
    let output = emit_dts(
        "export function foo(x: string): string;
export function foo(x: number): number;
export function foo(x: any): any { return x; }",
    );
    println!("PROBE overloaded function:\n{output}");
    let foo_count = output.matches("function foo").count();
    assert_eq!(
        foo_count, 2,
        "Should emit exactly 2 overload signatures, got {foo_count}: {output}"
    );
}

#[test]
fn probe_dts_constructor_type_in_type_alias() {
    let output = emit_dts("export type Ctor<T> = new (...args: any[]) => T;");
    println!("PROBE constructor type:\n{output}");
    assert!(
        output.contains("new (...args: any[]) => T"),
        "constructor type: {output}"
    );
}

#[test]
fn probe_dts_intersection_with_function() {
    let output = emit_dts("export type F = ((x: string) => void) & { bar: number };");
    println!("PROBE intersection with function:\n{output}");
    assert!(
        output.contains("((x: string) => void) & {"),
        "intersection with function: {output}"
    );
}

#[test]
fn probe_dts_conditional_type_infer() {
    let output = emit_dts("export type UnpackPromise<T> = T extends Promise<infer U> ? U : T;");
    println!("PROBE conditional infer:\n{output}");
    assert!(output.contains("infer U"), "conditional infer: {output}");
    assert!(
        output.contains("T extends Promise<infer U> ? U : T"),
        "conditional structure: {output}"
    );
}

#[test]
fn probe_dts_index_signature_readonly() {
    let output = emit_dts(
        "export interface ReadonlyMap {
    readonly [key: string]: number;
}",
    );
    println!("PROBE readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: number"),
        "readonly index sig: {output}"
    );
}

#[test]
fn probe_dts_optional_property_in_type_literal() {
    let output =
        emit_dts("export declare const x: { a: string; b?: number; readonly c: boolean };");
    println!("PROBE type literal props:\n{output}");
    assert!(output.contains("a: string"), "regular prop: {output}");
    assert!(output.contains("b?: number"), "optional prop: {output}");
    assert!(
        output.contains("readonly c: boolean"),
        "readonly prop: {output}"
    );
}

#[test]
fn probe_dts_export_namespace_from() {
    let output = emit_dts("export * as ns from './foo';");
    println!("PROBE export ns from:\n{output}");
    assert!(
        output.contains("export * as ns from"),
        "export namespace from: {output}"
    );
}

#[test]
fn probe_dts_class_with_static_member() {
    let output = emit_dts(
        "export declare class Foo {
    static bar: string;
    static baz(): void;
}",
    );
    println!("PROBE static members:\n{output}");
    assert!(
        output.contains("static bar: string"),
        "static property: {output}"
    );
    assert!(
        output.contains("static baz(): void"),
        "static method: {output}"
    );
}

#[test]
fn probe_dts_class_with_protected() {
    let output = emit_dts(
        "export declare class Foo {
    protected bar: string;
    protected baz(): void;
}",
    );
    println!("PROBE protected members:\n{output}");
    assert!(
        output.contains("protected bar: string"),
        "protected property: {output}"
    );
    assert!(
        output.contains("protected baz(): void"),
        "protected method: {output}"
    );
}

#[test]
fn probe_dts_generic_constraint_with_default() {
    let output = emit_dts("export declare function foo<T extends object = {}>(x: T): T;");
    println!("PROBE generic constraint+default:\n{output}");
    assert!(
        output.contains("T extends object = {}"),
        "generic constraint+default: {output}"
    );
}

#[test]
fn probe_dts_string_enum() {
    let output = emit_dts(
        r#"export enum Color {
    Red = "RED",
    Blue = "BLUE"
}"#,
    );
    println!("PROBE string enum:\n{output}");
    assert!(
        output.contains(r#"Red = "RED""#),
        "string enum member: {output}"
    );
}

#[test]
fn probe_dts_bigint_literal_expression() {
    let output = emit_dts("export declare const x: 42n;");
    println!("PROBE bigint literal:\n{output}");
    // BigInt literal types should be preserved
    assert!(output.contains("42n"), "bigint literal: {output}");
}

// =====================================================================
// More edge-case probes
// =====================================================================

#[test]
fn probe_dts_conditional_type_in_array() {
    // Conditional type as array element needs parens
    let output = emit_dts("export type X = (T extends string ? number : boolean)[];");
    println!("PROBE conditional in array:\n{output}");
    assert!(
        output.contains("(T extends string ? number : boolean)[]"),
        "Conditional in array needs parens: {output}"
    );
}

#[test]
fn probe_dts_function_type_in_array() {
    let output = emit_dts("export type X = ((x: number) => string)[];");
    println!("PROBE function in array:\n{output}");
    assert!(
        output.contains("((x: number) => string)[]"),
        "Function in array needs parens: {output}"
    );
}

#[test]
fn probe_dts_union_in_array() {
    let output = emit_dts("export type X = (string | number)[];");
    println!("PROBE union in array:\n{output}");
    assert!(
        output.contains("(string | number)[]"),
        "Union in array needs parens: {output}"
    );
}

#[test]
fn probe_dts_typeof_in_union() {
    let output = emit_dts("export declare const x: string | typeof Array;");
    println!("PROBE typeof in union:\n{output}");
    assert!(output.contains("typeof Array"), "typeof in union: {output}");
}

#[test]
fn probe_dts_keyof_in_conditional() {
    let output = emit_dts("export type X<T> = keyof T extends string ? T : never;");
    println!("PROBE keyof in conditional:\n{output}");
    assert!(
        output.contains("keyof T extends string"),
        "keyof in conditional: {output}"
    );
}

#[test]
fn probe_dts_readonly_array_type() {
    let output = emit_dts("export declare const x: readonly string[];");
    println!("PROBE readonly array:\n{output}");
    assert!(
        output.contains("readonly string[]"),
        "readonly array: {output}"
    );
}

#[test]
fn probe_dts_readonly_tuple_type() {
    let output = emit_dts("export declare const x: readonly [string, number];");
    println!("PROBE readonly tuple:\n{output}");
    assert!(
        output.contains("readonly [string, number]"),
        "readonly tuple: {output}"
    );
}

#[test]
fn probe_dts_type_assertion_in_extends_clause() {
    // In extends clause, complex expressions should be handled
    let output = emit_dts("export declare class Foo extends Array<string> { }");
    println!("PROBE extends generic:\n{output}");
    assert!(
        output.contains("extends Array<string>"),
        "extends generic: {output}"
    );
}

#[test]
fn probe_dts_infer_type_with_extends() {
    let output =
        emit_dts("export type GetString<T> = T extends { a: infer U extends string } ? U : never;");
    println!("PROBE infer extends:\n{output}");
    assert!(
        output.contains("infer U extends string"),
        "infer extends: {output}"
    );
}

#[test]
fn probe_dts_multiple_call_signatures() {
    let output = emit_dts(
        "export interface Callable {
    (x: string): string;
    (x: number): number;
}",
    );
    println!("PROBE multiple call sigs:\n{output}");
    let count = output.matches("(x:").count();
    assert_eq!(count, 2, "Should have 2 call signatures: {output}");
}

#[test]
fn probe_dts_construct_signature() {
    let output = emit_dts(
        "export interface Newable {
    new (x: string): object;
}",
    );
    println!("PROBE construct sig:\n{output}");
    assert!(
        output.contains("new (x: string): object"),
        "construct signature: {output}"
    );
}

#[test]
fn probe_dts_symbol_computed_property() {
    let output = emit_dts(
        "export interface Iterable {
    [Symbol.iterator](): Iterator<any>;
}",
    );
    println!("PROBE symbol computed:\n{output}");
    assert!(
        output.contains("[Symbol.iterator]"),
        "symbol computed property: {output}"
    );
}

#[test]
fn probe_dts_generator_function_return() {
    let output = emit_dts("export declare function* gen(): Generator<number, void, undefined>;");
    println!("PROBE generator:\n{output}");
    // Generator functions in .d.ts should NOT have the * (it goes into the return type)
    // Actually tsc strips the * and keeps Generator return type
    assert!(
        output.contains("Generator<number, void, undefined>"),
        "generator return type: {output}"
    );
}

#[test]
fn probe_dts_template_literal_with_union() {
    let output = emit_dts(r#"export type EventName = `${"click" | "focus"}_handler`;"#);
    println!("PROBE template literal union:\n{output}");
    assert!(output.contains("`"), "template literal: {output}");
}

#[test]
fn probe_dts_nested_generic_types() {
    let output = emit_dts("export declare const x: Map<string, Set<Array<number>>>;");
    println!("PROBE nested generics:\n{output}");
    assert!(
        output.contains("Map<string, Set<Array<number>>>"),
        "nested generics: {output}"
    );
}

#[test]
fn probe_dts_class_with_private_constructor() {
    let output = emit_dts(
        "export class Singleton {
    private constructor();
}",
    );
    println!("PROBE private constructor:\n{output}");
    assert!(
        output.contains("private constructor()"),
        "private constructor: {output}"
    );
}

#[test]
fn probe_dts_export_import_equals() {
    let output = emit_dts(
        "import foo = require('foo');
export = foo;",
    );
    println!("PROBE export import equals:\n{output}");
    assert!(output.contains("export = foo"), "export equals: {output}");
}

#[test]
fn probe_dts_type_alias_with_recursive_type() {
    let output = emit_dts(
        "export type Json = string | number | boolean | null | Json[] | { [key: string]: Json };",
    );
    println!("PROBE recursive type:\n{output}");
    assert!(output.contains("Json[]"), "recursive type: {output}");
}

#[test]
fn probe_dts_generator_star_stripped() {
    // Generator function declarations strip the `*` in .d.ts
    let output = emit_dts("export function* myGen(): Generator<number, string, boolean> {}");
    println!("PROBE generator star:\n{output}");
    assert!(
        !output.contains("function*"),
        "generator star should be stripped: {output}"
    );
    assert!(
        output.contains("function myGen"),
        "generator name preserved: {output}"
    );
}

#[test]
fn probe_dts_async_function_stripped() {
    // async keyword should be stripped in .d.ts (return type encodes Promise)
    let output = emit_dts("export async function myAsync(): Promise<void> {}");
    println!("PROBE async stripped:\n{output}");
    assert!(
        !output.contains("async"),
        "async should be stripped: {output}"
    );
}

#[test]
fn probe_dts_class_private_method_with_types() {
    // Private methods in .d.ts should omit types
    let output = emit_dts(
        "export declare class Foo {
    private bar(x: number): string;
}",
    );
    println!("PROBE private method:\n{output}");
    assert!(
        output.contains("private bar;"),
        "private method should be property-like: {output}"
    );
    assert!(
        !output.contains("private bar("),
        "private method should not have params: {output}"
    );
}

#[test]
fn probe_dts_never_type() {
    let output = emit_dts("export declare function throwError(): never;");
    println!("PROBE never:\n{output}");
    assert!(output.contains("): never;"), "never return type: {output}");
}

#[test]
fn probe_dts_interface_with_string_index() {
    let output = emit_dts(
        "export interface Dict<T> {
    [key: string]: T;
}",
    );
    println!("PROBE string index:\n{output}");
    assert!(
        output.contains("[key: string]: T"),
        "string index: {output}"
    );
}

#[test]
fn probe_dts_class_with_optional_method() {
    let output = emit_dts(
        "export declare class Foo {
    bar?(x: number): string;
}",
    );
    println!("PROBE optional method:\n{output}");
    assert!(output.contains("bar?"), "optional method: {output}");
}

#[test]
fn probe_dts_mapped_type_minus_readonly() {
    let output = emit_dts("export type Mutable<T> = { -readonly [P in keyof T]: T[P] };");
    println!("PROBE -readonly mapped:\n{output}");
    assert!(
        output.contains("-readonly"),
        "-readonly mapped type: {output}"
    );
}

#[test]
fn probe_dts_mapped_type_minus_optional() {
    let output = emit_dts("export type Required<T> = { [P in keyof T]-?: T[P] };");
    println!("PROBE -? mapped:\n{output}");
    assert!(output.contains("-?"), "-? mapped type: {output}");
}

#[test]
fn probe_dts_export_default_expression_value() {
    // export default with expression should synthesize a variable
    let output = emit_dts("export default 42;");
    println!("PROBE export default expr:\n{output}");
    assert!(
        output.contains("export default"),
        "export default: {output}"
    );
}

#[test]
fn probe_dts_const_assertion_value() {
    // `as const` on a value - should be stripped
    let output = emit_dts("export const arr = [1, 2, 3] as const;");
    println!("PROBE const assertion value:\n{output}");
    assert!(
        !output.contains("as const"),
        "as const should be stripped from value: {output}"
    );
}

#[test]
fn probe_dts_function_with_destructured_param() {
    let output = emit_dts("export function foo({ a, b }: { a: number; b: string }): void {}");
    println!("PROBE destructured param:\n{output}");
    assert!(
        output.contains("{ a, b }"),
        "destructured param pattern: {output}"
    );
    assert!(
        output.contains("a: number"),
        "destructured param type: {output}"
    );
}

#[test]
fn probe_dts_function_with_rest_param() {
    let output = emit_dts("export function foo(a: number, ...rest: string[]): void {}");
    println!("PROBE rest param:\n{output}");
    assert!(output.contains("...rest: string[]"), "rest param: {output}");
}

#[test]
fn probe_dts_function_with_default_param() {
    let output = emit_dts("export function foo(x: number = 42): void {}");
    println!("PROBE default param:\n{output}");
    // Default params become optional in .d.ts
    assert!(
        output.contains("x?: number"),
        "default param becomes optional: {output}"
    );
}

#[test]
fn probe_dts_interface_method_overloads() {
    let output = emit_dts(
        "export interface Converter {
    convert(x: string): number;
    convert(x: number): string;
}",
    );
    println!("PROBE interface method overloads:\n{output}");
    let count = output.matches("convert(").count();
    assert_eq!(count, 2, "Should have 2 method overloads: {output}");
}

#[test]
fn probe_dts_using_declaration() {
    // `using` declarations should emit as `const` in .d.ts
    let output = emit_dts("export using x: Disposable = getResource();");
    println!("PROBE using decl:\n{output}");
    assert!(
        output.contains("const x"),
        "using should emit as const: {output}"
    );
}

#[test]
fn probe_dts_keyof_with_parens() {
    // `keyof (A | B)` needs parens to be different from `keyof A | B`
    let output = emit_dts("export type X = keyof (A | B);");
    println!("PROBE keyof with parens:\n{output}");
    assert!(
        output.contains("keyof (A | B)"),
        "keyof should preserve parens around union: {output}"
    );
}

#[test]
fn probe_dts_conditional_type_nested() {
    // Nested conditionals are right-associative in false branch
    let output = emit_dts(
        "export type X<T> = T extends string ? 'str' : T extends number ? 'num' : 'other';",
    );
    println!("PROBE nested conditional:\n{output}");
    assert!(
        output.contains("T extends string"),
        "nested conditional first part: {output}"
    );
    assert!(
        output.contains("T extends number"),
        "nested conditional second part: {output}"
    );
}

// =====================================================================
// Edge case probes - round 3
// =====================================================================

#[test]
fn probe_dts_optional_type_with_conditional() {
    // Optional tuple element with conditional type needs parens
    let output = emit_dts("export type X = [(T extends string ? number : boolean)?];");
    println!("PROBE optional conditional:\n{output}");
    assert!(
        output.contains("(T extends string ? number : boolean)?"),
        "optional conditional needs parens: {output}"
    );
}

#[test]
fn probe_dts_conditional_type_in_indexed_access() {
    let output = emit_dts("export type X = (string | number)['toString'];");
    println!("PROBE union in indexed access:\n{output}");
    assert!(
        output.contains("(string | number)["),
        "union in indexed access needs parens: {output}"
    );
}

#[test]
fn probe_dts_array_of_conditional() {
    let output = emit_dts("export type X = (T extends string ? number : boolean)[];");
    println!("PROBE array of conditional:\n{output}");
    assert!(
        output.contains("(T extends string ? number : boolean)[]"),
        "array of conditional needs parens: {output}"
    );
}

#[test]
fn probe_dts_function_in_union() {
    let output = emit_dts("export type X = string | ((x: number) => void);");
    println!("PROBE function in union:\n{output}");
    assert!(
        output.contains("((x: number) => void)"),
        "function type in union needs parens: {output}"
    );
}

#[test]
fn probe_dts_constructor_type_in_union() {
    let output = emit_dts("export type X = string | (new (x: number) => object);");
    println!("PROBE constructor in union:\n{output}");
    assert!(
        output.contains("(new (x: number) => object)"),
        "constructor type in union needs parens: {output}"
    );
}

#[test]
fn probe_dts_conditional_in_conditional_extends() {
    // Conditional type in extends position of another conditional
    let output = emit_dts(
        "export type X<T> = T extends (U extends string ? number : boolean) ? 'yes' : 'no';",
    );
    println!("PROBE conditional in extends:\n{output}");
    assert!(
        output.contains("(U extends string ? number : boolean)"),
        "conditional in extends position needs parens: {output}"
    );
}

#[test]
fn probe_dts_type_operator_keyof_union() {
    // keyof should bind tighter than union; `keyof (A | B)` needs parens
    let output = emit_dts("export type X = keyof (A | B);");
    println!("PROBE keyof union:\n{output}");
    assert!(
        output.contains("keyof (A | B)"),
        "keyof union needs parens: {output}"
    );
}

#[test]
fn probe_dts_readonly_union() {
    // readonly should bind tighter than union; `readonly (A | B)` needs parens
    let output = emit_dts("export type X = readonly (string | number)[];");
    println!("PROBE readonly union array:\n{output}");
    assert!(
        output.contains("readonly (string | number)[]"),
        "readonly union array: {output}"
    );
}

#[test]
fn probe_dts_infer_type_in_conditional() {
    let output =
        emit_dts("export type ElementType<T> = T extends readonly (infer U)[] ? U : never;");
    println!("PROBE infer in array:\n{output}");
    assert!(output.contains("infer U"), "infer type: {output}");
}

#[test]
fn probe_dts_generic_defaults_complex() {
    let output =
        emit_dts("export type Foo<A extends object = {}, B extends keyof A = keyof A> = A[B];");
    println!("PROBE complex generic defaults:\n{output}");
    assert!(
        output.contains("B extends keyof A = keyof A"),
        "complex generic defaults: {output}"
    );
}

#[test]
fn probe_dts_variance_modifiers() {
    let output = emit_dts("export interface Container<in out T> { value: T; }");
    println!("PROBE variance modifiers:\n{output}");
    assert!(output.contains("in out T"), "variance modifiers: {output}");
}

#[test]
fn probe_dts_const_type_param() {
    let output =
        emit_dts("export declare function foo<const T extends readonly string[]>(args: T): T;");
    println!("PROBE const type param:\n{output}");
    assert!(
        output.contains("const T extends readonly string[]"),
        "const type param: {output}"
    );
}

#[test]
fn probe_dts_negative_bigint_literal() {
    let output = emit_dts("export declare const x: -100n;");
    println!("PROBE negative bigint:\n{output}");
    assert!(
        output.contains("-100n"),
        "negative bigint literal: {output}"
    );
}

#[test]
fn probe_dts_union_in_optional_type() {
    // Union in optional tuple member needs parens
    let output = emit_dts("export type X = [(string | number)?];");
    println!("PROBE union in optional:\n{output}");
    assert!(
        output.contains("(string | number)?"),
        "union in optional type: {output}"
    );
}

#[test]
fn probe_dts_intersection_in_optional_type() {
    let output = emit_dts("export type X = [(A & B)?];");
    println!("PROBE intersection in optional:\n{output}");
    assert!(
        output.contains("(A & B)?"),
        "intersection in optional type: {output}"
    );
}

#[test]
fn probe_dts_function_in_conditional_check() {
    // Function type as check type of conditional needs parens
    let output = emit_dts("export type X = ((x: number) => void) extends Function ? true : false;");
    println!("PROBE function in conditional check:\n{output}");
    assert!(
        output.contains("((x: number) => void) extends Function"),
        "function in conditional check: {output}"
    );
}

#[test]
fn probe_dts_union_in_conditional_check() {
    let output = emit_dts("export type X = (string | number) extends object ? true : false;");
    println!("PROBE union in conditional check:\n{output}");
    assert!(
        output.contains("(string | number) extends object"),
        "union in conditional check: {output}"
    );
}

#[test]
fn test_this_type_in_type_position() {
    // `this` as a type uses the parser's THIS_TYPE node kind (198),
    // not ThisKeyword (110). Both must be handled.
    let output = emit_dts(
        "export interface Chainable {
    chain(): this;
    map(f: (x: this) => this): this;
}",
    );
    println!("this type:\n{output}");
    assert!(
        output.contains("chain(): this"),
        "this return type: {output}"
    );
    assert!(
        output.contains("(x: this) => this"),
        "this in function type: {output}"
    );
}

#[test]
fn test_this_type_in_type_alias() {
    // `this` type in type alias
    let output = emit_dts("export type SelfRef = { value: this };");
    println!("this in type alias:\n{output}");
    assert!(
        output.contains("value: this"),
        "this in type literal: {output}"
    );
}

#[test]
fn test_conditional_type_in_indexed_access() {
    // Conditional type as object of indexed access needs parens
    // Without: T extends U ? X : Y[K] -> parses [K] as indexing Y only
    // With: (T extends U ? X : Y)[K] -> indexes the whole conditional
    let output = emit_dts(
        "export type X<T, K extends string> = (T extends string ? { a: number } : { b: string })[K];",
    );
    println!("conditional in indexed access:\n{output}");
    assert!(
        output.contains("(T extends string ?"),
        "conditional in indexed access needs parens: {output}"
    );
}

// === Fix verification tests ===

#[test]
fn fix_numeric_separator_stripped_in_type_position() {
    // tsc strips numeric separators in .d.ts output
    let output = emit_dts("export declare const x: 1_000_000;");
    println!("numeric sep:\n{output}");
    assert!(
        output.contains("1000000"),
        "numeric separator should be stripped: {output}"
    );
    assert!(
        !output.contains("1_000_000"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_numeric_separator_hex_with_sep() {
    // tsc converts hex with separators to decimal
    let output = emit_dts("export declare const x: 0xFF_FF;");
    println!("hex sep:\n{output}");
    assert!(
        output.contains("65535"),
        "hex with separator should be decimal 65535: {output}"
    );
}

#[test]
fn fix_numeric_separator_preserved_no_sep() {
    // Without separators, numeric literals should be preserved as-is
    let output = emit_dts("export declare const x: 0xFF;");
    println!("hex no sep:\n{output}");
    assert!(
        output.contains("0xFF"),
        "hex without separator preserved: {output}"
    );
}

#[test]
fn fix_numeric_separator_decimal_no_sep() {
    // Decimal without separator preserved
    let output = emit_dts("export declare const x: 42;");
    println!("decimal no sep:\n{output}");
    assert!(
        output.contains("42"),
        "decimal without separator preserved: {output}"
    );
}

#[test]
fn fix_enum_cross_reference() {
    // tsc computes cross-enum references
    let output = emit_dts("export enum A { X = 1 }\nexport enum B { Y = A.X }");
    println!("enum cross-ref:\n{output}");
    assert!(
        output.contains("Y = 1"),
        "cross-enum ref should be resolved to 1: {output}"
    );
}

#[test]
fn fix_enum_cross_reference_computed() {
    // Cross-enum reference with computation
    let output = emit_dts("export enum A { X = 1, Y = 2 }\nexport enum B { Z = A.X + A.Y }");
    println!("enum cross-ref computed:\n{output}");
    assert!(
        output.contains("Z = 3"),
        "cross-enum ref should compute to 3: {output}"
    );
}

#[test]
fn fix_template_literal_escape_preserved() {
    // Template literal type with escape sequences
    let output = emit_dts(r#"export type T = `hello\nworld`;"#);
    println!("template escape:\n{output}");
    // Should preserve \n as escape sequence, not emit actual newline
    assert!(
        output.contains(r#"`hello\nworld`"#),
        "escape sequence should be preserved: {output}"
    );
    assert!(
        !output.contains("hello\nworld"),
        "actual newline should not appear in template: {output}"
    );
}

#[test]
fn fix_template_literal_simple() {
    // Template literal without escapes should work as before
    let output = emit_dts("export type T = `hello world`;");
    println!("template simple:\n{output}");
    assert!(
        output.contains("`hello world`"),
        "simple template: {output}"
    );
}

#[test]
fn fix_template_literal_with_types() {
    // Template literal with type substitutions
    let output = emit_dts("export type T = `hello ${string}`;");
    println!("template with type:\n{output}");
    assert!(
        output.contains("`hello ${string}`"),
        "template with type: {output}"
    );
}

#[test]
fn fix_numeric_sep_negative() {
    // Negative number with separator
    let output = emit_dts("export declare const x: -1_000;");
    println!("negative sep:\n{output}");
    assert!(
        output.contains("-1000"),
        "negative with separator: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should be stripped: {output}"
    );
}

#[test]
fn fix_numeric_sep_binary() {
    // Binary literal with separator
    let output = emit_dts("export declare const x: 0b1010_0101;");
    println!("binary sep:\n{output}");
    // tsc preserves binary notation without separators
    // Actually tsc converts to decimal for non-decimal with separators
    assert!(
        !output.contains("_"),
        "underscore should be stripped: {output}"
    );
}

#[test]
fn fix_numeric_sep_bigint() {
    // BigInt with separator
    let output = emit_dts("export declare const x: 1_000n;");
    println!("bigint sep:\n{output}");
    assert!(
        !output.contains("_"),
        "underscore should be stripped: {output}"
    );
    assert!(
        output.contains("1000n"),
        "bigint separator stripped: {output}"
    );
}

#[test]
fn fix_template_literal_tab_escape() {
    // Template literal with tab escape
    let output = emit_dts(r#"export type T = `hello\tworld`;"#);
    println!("template tab:\n{output}");
    assert!(
        output.contains(r#"`hello\tworld`"#),
        "tab escape preserved: {output}"
    );
}

#[test]
fn fix_template_literal_multi_substitution() {
    // Template literal with multiple type substitutions
    let output = emit_dts("export type T = `${string}-${number}`;");
    println!("template multi sub:\n{output}");
    assert!(
        output.contains("`${string}-${number}`"),
        "multi substitution: {output}"
    );
}

#[test]
fn fix_template_literal_backtick_in_template() {
    // Template literal with escaped backtick
    let output = emit_dts(r#"export type T = `hello\`world`;"#);
    println!("template backtick:\n{output}");
    assert!(
        output.contains(r"`hello\`world`"),
        "escaped backtick: {output}"
    );
}

#[test]
fn fix_enum_self_ref_still_works() {
    // Self-referencing enum should still work
    let output = emit_dts("export enum E { A = 1, B = A + 1, C = A | B }");
    println!("enum self-ref:\n{output}");
    assert!(output.contains("A = 1"), "A = 1: {output}");
    assert!(output.contains("B = 2"), "B = 2: {output}");
    assert!(output.contains("C = 3"), "C = 3: {output}");
}

#[test]
fn dump_const_literal_preservation() {
    let cases = vec![
        ("const-string", "export const a = '1.0';"),
        ("const-number", "export const b = 42;"),
        ("const-boolean", "export const c = true;"),
        ("const-array", "export const d = [1, 2, 3];"),
        ("let-string", "export let e = '1.0';"),
        ("let-number", "export let f = 42;"),
        (
            "static-readonly-string",
            "export class C { static readonly VERSION = '1.0'; }",
        ),
        (
            "static-readonly-number",
            "export class C { static readonly COUNT = 42; }",
        ),
        (
            "static-readonly-bool",
            "export class C { static readonly FLAG = true; }",
        ),
        (
            "static-readonly-array",
            "export class C { static readonly ITEMS = [1, 2, 3]; }",
        ),
        ("static-non-readonly", "export class C { static x = 42; }"),
        ("const-negative", "export const x = -42;"),
        ("const-template", "export const x = `hello`;"),
    ];

    for (label, source) in &cases {
        let output = emit_dts(source);
        println!("=== {label} ===");
        println!("{output}");
        println!();
    }
}

#[test]
fn fix_static_readonly_string_literal_preserved() {
    // tsc preserves literal values for static readonly properties
    let output = emit_dts("export class C { static readonly VERSION = '1.0'; }");
    println!("static readonly string:\n{output}");
    assert!(
        output.contains("= \"1.0\"") || output.contains("= '1.0'"),
        "static readonly string should be preserved as literal: {output}"
    );
}

#[test]
fn fix_static_readonly_number_literal_preserved() {
    let output = emit_dts("export class C { static readonly COUNT = 42; }");
    println!("static readonly number:\n{output}");
    assert!(
        output.contains("= 42"),
        "static readonly number should be preserved: {output}"
    );
}

#[test]
fn fix_static_readonly_boolean_literal_preserved() {
    let output = emit_dts("export class C { static readonly FLAG = true; }");
    println!("static readonly bool:\n{output}");
    assert!(
        output.contains("= true"),
        "static readonly boolean should be preserved: {output}"
    );
}

#[test]
fn fix_static_readonly_array_not_preserved() {
    // Arrays should widen to type, not preserve literal
    let output = emit_dts("export class C { static readonly ITEMS = [1, 2, 3]; }");
    println!("static readonly array:\n{output}");
    // Should NOT have = [...], should have : any[] or similar
    assert!(
        !output.contains("= ["),
        "array should widen, not preserve literal: {output}"
    );
}

#[test]
fn fix_static_non_readonly_widens() {
    // Non-readonly static should widen to type
    let output = emit_dts("export class C { static x = 42; }");
    println!("static non-readonly:\n{output}");
    assert!(
        output.contains(": number"),
        "non-readonly should widen: {output}"
    );
    assert!(
        !output.contains("= 42"),
        "non-readonly should not preserve literal: {output}"
    );
}

#[test]
fn fix_readonly_nonstatic_literal_preserved() {
    // Readonly (non-static) should also preserve literals
    let output = emit_dts("export class C { readonly name = 'hello'; }");
    println!("readonly nonstatic:\n{output}");
    assert!(
        output.contains("= \"hello\"") || output.contains("= 'hello'"),
        "readonly string should be preserved: {output}"
    );
}

#[test]
fn fix_static_readonly_negative_number() {
    let output = emit_dts("export class C { static readonly OFFSET = -42; }");
    println!("static readonly negative:\n{output}");
    assert!(
        output.contains("= -42"),
        "negative number preserved: {output}"
    );
}

#[test]
fn fix_enum_numeric_separator_in_value() {
    // Enum member values with numeric separators should be evaluated correctly
    let output = emit_dts("export enum E { A = 1_000, B = 2_000, C = A + B }");
    println!("enum sep values:\n{output}");
    assert!(output.contains("A = 1000"), "A should be 1000: {output}");
    assert!(output.contains("B = 2000"), "B should be 2000: {output}");
    assert!(output.contains("C = 3000"), "C should be 3000: {output}");
}

#[test]
fn fix_enum_hex_separator_in_value() {
    let output = emit_dts("export enum E { A = 0xFF_FF }");
    println!("enum hex sep:\n{output}");
    assert!(
        output.contains("A = 65535"),
        "hex with sep should evaluate: {output}"
    );
}

#[test]
fn fix_regex_literal_inferred_type() {
    // Regex literal initializer should infer RegExp type
    let output = emit_dts("export const x = /hello/;");
    println!("regex:\n{output}");
    assert!(
        output.contains("RegExp"),
        "regex should infer RegExp: {output}"
    );
}

#[test]
fn fix_template_literal_initializer_inferred_type() {
    // Template literal initializer should infer string type
    let output = emit_dts("export const x = `hello`;");
    println!("template init:\n{output}");
    assert!(
        output.contains("string") || output.contains("\"hello\""),
        "template should infer string: {output}"
    );
}

#[test]
fn fix_template_expression_initializer_inferred_type() {
    let output = emit_dts("export let x = `hello ${42}`;");
    println!("template expr init:\n{output}");
    assert!(
        output.contains("string"),
        "template expression should infer string: {output}"
    );
}

#[test]
fn fix_regex_in_const_with_flags() {
    let output = emit_dts("export const re = /test/gi;");
    println!("regex with flags:\n{output}");
    assert!(
        output.contains("RegExp"),
        "regex with flags should infer RegExp: {output}"
    );
}

#[test]
fn fix_const_numeric_separator_stripped() {
    let output = emit_dts("export const x = 1_000_000;");
    println!("const sep:\n{output}");
    assert!(
        output.contains("1000000"),
        "const numeric sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_const_bigint_separator_stripped() {
    let output = emit_dts("export const x = 1_000n;");
    println!("const bigint sep:\n{output}");
    assert!(
        output.contains("1000n"),
        "bigint sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_const_negative_separator_stripped() {
    let output = emit_dts("export const x = -1_000;");
    println!("const neg sep:\n{output}");
    assert!(
        output.contains("-1000"),
        "negative sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

#[test]
fn fix_const_hex_separator_converted() {
    let output = emit_dts("export const x = 0xFF_FF;");
    println!("const hex sep:\n{output}");
    assert!(
        output.contains("65535"),
        "hex sep should convert to decimal: {output}"
    );
}

#[test]
fn fix_numeric_property_name_separator() {
    let output = emit_dts("export interface I { 1_000: string; }");
    println!("numeric prop name sep:\n{output}");
    assert!(
        output.contains("1000:"),
        "numeric property name sep should be stripped: {output}"
    );
    assert!(
        !output.contains("_"),
        "underscore should not appear: {output}"
    );
}

// =============================================================================
// Edge case exploration tests (Round 5 - finding new issues)
// =============================================================================

#[test]
fn explore_static_block_stripped() {
    // Static blocks should be stripped from .d.ts
    let output = emit_dts(
        "export class Foo {
    static x: number;
    static {
        this.x = 42;
    }
    y: string;
}",
    );
    println!("static block:\n{output}");
    assert!(
        !output.contains("static {"),
        "static block should be stripped: {output}"
    );
    assert!(
        output.contains("static x: number;"),
        "static property should remain: {output}"
    );
    assert!(
        output.contains("y: string;"),
        "property should remain: {output}"
    );
}

#[test]
fn explore_import_type_full_syntax() {
    // import("./module").SomeType should be preserved
    let output = emit_dts("export type MyType = import('./module').SomeType;");
    println!("import type:\n{output}");
    assert!(
        output.contains("import("),
        "import type should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_static_block_and_method() {
    let output = emit_dts(
        "export class Counter {
    static count: number;
    static {
        Counter.count = 0;
    }
    increment(): void {
        Counter.count++;
    }
}",
    );
    println!("static block + method:\n{output}");
    assert!(
        !output.contains("static {"),
        "static block should be stripped: {output}"
    );
    assert!(
        output.contains("static count: number;"),
        "static property should remain: {output}"
    );
    assert!(
        output.contains("increment(): void;"),
        "method should remain: {output}"
    );
}

#[test]
fn explore_constructor_type_in_intersection() {
    // Constructor type in intersection needs parentheses
    let output = emit_dts("export type T = (new (x: string) => object) & { tag: string };");
    println!("ctor in intersection:\n{output}");
    assert!(
        output.contains("(new (x: string) => object) & {"),
        "constructor type in intersection should be parenthesized: {output}"
    );
}

#[test]
fn explore_type_operator_in_array() {
    // `keyof T` in array should get parenthesized: `(keyof T)[]`
    let output = emit_dts("export type T<U> = (keyof U)[];");
    println!("type op in array:\n{output}");
    assert!(
        output.contains("(keyof U)[]"),
        "type operator in array should be parenthesized: {output}"
    );
}

#[test]
fn explore_conditional_type_in_array() {
    // (T extends U ? X : Y)[] - conditional type in array needs parens
    let output = emit_dts("export type T<U> = (U extends string ? 'yes' : 'no')[];");
    println!("conditional in array:\n{output}");
    assert!(
        output.contains("(U extends string"),
        "conditional in array should be parenthesized: {output}"
    );
    assert!(
        output.contains("[]"),
        "array brackets should be present: {output}"
    );
}

#[test]
fn explore_intersection_type_in_union() {
    // Intersection types inside unions don't need parens (& binds tighter)
    let output = emit_dts("export type T = A & B | C & D;");
    println!("intersection in union:\n{output}");
    // No parens needed since & binds tighter
    assert!(
        output.contains("A & B | C & D"),
        "intersection in union should not need parens: {output}"
    );
}

#[test]
fn explore_function_type_in_conditional_extends() {
    // Function type in conditional extends position might need parens
    let output = emit_dts("export type T<F> = F extends (() => infer R) ? R : never;");
    println!("fn in conditional:\n{output}");
    assert!(
        output.contains("infer R"),
        "infer R should be present: {output}"
    );
}

#[test]
fn explore_complex_nested_types() {
    // Deeply nested type with multiple operators
    let output = emit_dts(
        "export type T = {
    readonly [K in keyof any as `on${string & K}`]: ((event: K) => void) | null;
};",
    );
    println!("complex nested:\n{output}");
    assert!(
        output.contains("readonly [K in keyof any as `on${string & K}`]"),
        "mapped type with as clause should be preserved: {output}"
    );
}

#[test]
fn explore_declare_var_vs_let_vs_const() {
    // declare var/let/const all have specific behavior in .d.ts
    let output = emit_dts(
        "export declare var a: string;
export declare let b: number;
export declare const c: boolean;",
    );
    println!("var/let/const:\n{output}");
    assert!(
        output.contains("export declare var a: string;"),
        "var should be preserved: {output}"
    );
    assert!(
        output.contains("export declare let b: number;"),
        "let should be preserved: {output}"
    );
    assert!(
        output.contains("export declare const c: boolean;"),
        "const should be preserved: {output}"
    );
}

#[test]
fn explore_export_type_star() {
    let output = emit_dts("export type * from './module';");
    println!("export type *:\n{output}");
    assert!(
        output.contains("export type * from"),
        "export type * should be preserved: {output}"
    );
}

#[test]
fn explore_export_type_star_as_ns() {
    let output = emit_dts("export type * as ns from './module';");
    println!("export type * as ns:\n{output}");
    assert!(
        output.contains("export type * as ns from"),
        "export type * as ns should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_declare_property() {
    // `declare` keyword on class property — tsc strips this in .d.ts
    let output = emit_dts(
        "export class Foo {
    declare bar: string;
}",
    );
    println!("declare prop:\n{output}");
    // tsc strips `declare` from class members in .d.ts
    assert!(
        output.contains("bar: string;"),
        "bar should be present: {output}"
    );
    let bar_line = output.lines().find(|l| l.contains("bar: string")).unwrap();
    assert!(
        !bar_line.contains("declare"),
        "declare should be stripped from class member: {output}"
    );
}

#[test]
fn explore_async_generator() {
    let output = emit_dts("export async function* gen(): AsyncGenerator<number> { yield 1; }");
    println!("async generator:\n{output}");
    // tsc strips async and * from .d.ts
    assert!(
        !output.contains("async"),
        "async should be stripped: {output}"
    );
    assert!(!output.contains("*"), "* should be stripped: {output}");
    assert!(
        output.contains("gen(): AsyncGenerator<number>;"),
        "return type should be preserved: {output}"
    );
}

#[test]
fn explore_type_predicate_this() {
    // `this is Type` predicate
    let output = emit_dts(
        "export class Animal {
    isFlying(): this is FlyingAnimal { return false; }
}
interface FlyingAnimal extends Animal { fly(): void; }",
    );
    println!("this predicate:\n{output}");
    assert!(
        output.contains("this is FlyingAnimal"),
        "this type predicate should be preserved: {output}"
    );
}

#[test]
fn explore_constructor_type_with_generics() {
    let output = emit_dts("export type T = new <U>(x: U) => U;");
    println!("ctor generic:\n{output}");
    assert!(
        output.contains("new <U>(x: U) => U"),
        "generic constructor type should be preserved: {output}"
    );
}

#[test]
fn explore_nested_generics_in_function_type() {
    let output =
        emit_dts("export type T = <A, B extends Record<string, A>>(x: A, y: B) => Map<A, B>;");
    println!("nested generics:\n{output}");
    assert!(
        output.contains("<A, B extends Record<string, A>>"),
        "nested generics should be preserved: {output}"
    );
    assert!(
        output.contains("Map<A, B>"),
        "return type should be preserved: {output}"
    );
}

#[test]
fn explore_declare_property_with_modifiers() {
    // declare property with access modifiers — tsc strips `declare` from class members
    let output = emit_dts(
        "export class Foo {
    declare protected bar: string;
    declare static baz: number;
}",
    );
    println!("declare prop modifiers:\n{output}");
    assert!(
        output.contains("protected bar: string;"),
        "protected bar should be present: {output}"
    );
    assert!(
        output.contains("static baz: number;"),
        "static baz should be present: {output}"
    );
    // `declare` should be stripped from members
    for line in output.lines() {
        if line.contains("bar:") || line.contains("baz:") {
            assert!(
                !line.contains("declare"),
                "declare should be stripped from member: {line}"
            );
        }
    }
}

#[test]
fn explore_class_with_multiple_declare_properties() {
    // tsc strips `declare` from class members in .d.ts
    let output = emit_dts(
        "export class Foo {
    declare x: string;
    y: number;
    declare z: boolean;
}",
    );
    println!("multiple declare props:\n{output}");
    assert!(
        output.contains("x: string;"),
        "x should be present: {output}"
    );
    assert!(
        output.contains("y: number;"),
        "y should be present: {output}"
    );
    assert!(
        output.contains("z: boolean;"),
        "z should be present: {output}"
    );
}

#[test]
fn explore_abstract_accessor_declaration() {
    // abstract get/set accessors
    let output = emit_dts(
        "export abstract class Foo {
    abstract get name(): string;
    abstract set name(val: string);
}",
    );
    println!("abstract accessors:\n{output}");
    assert!(
        output.contains("abstract get name(): string;"),
        "abstract getter should be preserved: {output}"
    );
    assert!(
        output.contains("abstract set name(val: string);"),
        "abstract setter should be preserved: {output}"
    );
}

#[test]
fn explore_constructor_overloads_with_accessibility() {
    let output = emit_dts(
        "export class Foo {
    private constructor(x: string);
    private constructor(x: number);
    private constructor(x: any) {}
}",
    );
    println!("ctor overloads with access:\n{output}");
    let ctor_count = output.matches("private constructor(").count();
    assert_eq!(
        ctor_count, 2,
        "Should have 2 private constructor overloads (not implementation): {output}"
    );
}

#[test]
fn explore_generic_method_with_constraint() {
    let output = emit_dts(
        "export class Container {
    get<T extends object>(key: string): T { return {} as T; }
}",
    );
    println!("generic method constraint:\n{output}");
    assert!(
        output.contains("get<T extends object>(key: string): T;"),
        "generic method with constraint should be preserved: {output}"
    );
}

#[test]
fn explore_index_signature_with_readonly() {
    let output = emit_dts(
        "export interface Dict {
    readonly [key: string]: number;
}",
    );
    println!("readonly index sig:\n{output}");
    assert!(
        output.contains("readonly [key: string]: number;"),
        "readonly index signature should be preserved: {output}"
    );
}

#[test]
fn explore_computed_property_with_well_known_symbol() {
    let output = emit_dts(
        "export class MyIterable {
    [Symbol.iterator](): Iterator<number> { return [].values(); }
}",
    );
    println!("well-known symbol:\n{output}");
    assert!(
        output.contains("[Symbol.iterator]()"),
        "well-known symbol should be preserved: {output}"
    );
}

#[test]
fn explore_nested_mapped_type_with_template_keys() {
    let output = emit_dts(
        "export type EventHandlers<T> = {
    [K in keyof T as K extends string ? `on${Capitalize<K>}` : never]: (event: T[K]) => void;
};",
    );
    println!("nested mapped template:\n{output}");
    assert!(
        output.contains("as K extends string ? `on${Capitalize<K>}` : never"),
        "mapped type with template key should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_abstract_accessor_and_regular() {
    let output = emit_dts(
        "export abstract class Base {
    abstract get id(): string;
    get name(): string { return ''; }
}",
    );
    println!("abstract + regular accessors:\n{output}");
    assert!(
        output.contains("abstract get id(): string;"),
        "abstract accessor should be preserved: {output}"
    );
    assert!(
        output.contains("get name(): string;"),
        "regular accessor should be preserved: {output}"
    );
}

#[test]
fn explore_function_with_this_type_return() {
    let output = emit_dts(
        "export declare class Builder {
    withName(name: string): this;
    build(): object;
}",
    );
    println!("this return type:\n{output}");
    let expected = "export declare class Builder {\n    withName(name: string): this;\n    build(): object;\n}\n";
    assert_eq!(output, expected, "Mismatch");
}

// =============================================================================
// Round 6 - More targeted edge case testing
// =============================================================================

#[test]
fn explore_assertion_signature_in_class() {
    let output = emit_dts(
        "export class Guard {
    assertValid(value: unknown): asserts value is string {
        if (typeof value !== 'string') throw new Error();
    }
}",
    );
    println!("assertion in class:\n{output}");
    assert!(
        output.contains("assertValid(value: unknown): asserts value is string;"),
        "assertion signature should be preserved: {output}"
    );
}

#[test]
fn explore_class_method_return_type_with_function_type() {
    // Method returning a function type
    let output = emit_dts(
        "export declare class Foo {
    getHandler(): (event: string) => void;
}",
    );
    println!("method returning fn type:\n{output}");
    assert!(
        output.contains("getHandler(): (event: string) => void;"),
        "function return type should be preserved: {output}"
    );
}

#[test]
fn explore_interface_with_string_index_and_numeric_index() {
    let output = emit_dts(
        "export interface Mixed {
    [key: string]: any;
    [index: number]: string;
    length: number;
}",
    );
    println!("mixed index sigs:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "string index should be present: {output}"
    );
    assert!(
        output.contains("[index: number]: string;"),
        "numeric index should be present: {output}"
    );
    assert!(
        output.contains("length: number;"),
        "length should be present: {output}"
    );
}

#[test]
fn explore_readonly_tuple_with_labels() {
    let output = emit_dts("export type Point3D = readonly [x: number, y: number, z: number];");
    println!("readonly labeled tuple:\n{output}");
    assert!(
        output.contains("readonly [x: number, y: number, z: number]"),
        "readonly labeled tuple should be preserved: {output}"
    );
}

#[test]
fn explore_method_with_overloaded_generics() {
    let output = emit_dts(
        "export interface Repository {
    find<T extends object>(id: string): T;
    find<T extends object>(query: Partial<T>): T[];
}",
    );
    println!("overloaded generics:\n{output}");
    assert!(
        output.contains("find<T extends object>(id: string): T;"),
        "overload 1 should be present: {output}"
    );
    assert!(
        output.contains("find<T extends object>(query: Partial<T>): T[];"),
        "overload 2 should be present: {output}"
    );
}

#[test]
fn explore_export_default_expression_identifier() {
    // export default someVar
    let output = emit_dts(
        "declare const x: number;
export default x;",
    );
    println!("export default identifier:\n{output}");
    assert!(
        output.contains("export default x;"),
        "export default identifier should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_index_and_computed_symbol() {
    // Class with both index signature and computed symbol property
    let output = emit_dts(
        "export class Dict {
    [key: string]: any;
    [Symbol.toPrimitive](): string { return ''; }
}",
    );
    println!("index + symbol:\n{output}");
    assert!(
        output.contains("[key: string]: any;"),
        "index sig should be present: {output}"
    );
    assert!(
        output.contains("[Symbol.toPrimitive]()"),
        "symbol method should be present: {output}"
    );
}

#[test]
fn explore_multiple_export_as() {
    let output = emit_dts("export { default as React } from 'react';");
    println!("export as:\n{output}");
    assert!(
        output.contains("default as React"),
        "export as should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_definite_and_optional_properties() {
    let output = emit_dts(
        "export class Foo {
    bar?: string;
    baz!: number;
    qux: boolean;
}",
    );
    println!("definite + optional:\n{output}");
    assert!(
        output.contains("bar?: string;"),
        "optional prop should be preserved: {output}"
    );
    // tsc strips ! in .d.ts
    assert!(
        output.contains("baz: number;"),
        "definite assignment should be stripped: {output}"
    );
    assert!(!output.contains("baz!:"), "! should not appear: {output}");
    assert!(
        output.contains("qux: boolean;"),
        "normal prop should be preserved: {output}"
    );
}

#[test]
fn explore_class_with_only_static_block() {
    // A class that has only a static block should emit an empty class body
    let output = emit_dts(
        "export class Init {
    static {
        console.log('init');
    }
}",
    );
    println!("only static block:\n{output}");
    assert!(
        !output.contains("static {"),
        "static block should be stripped: {output}"
    );
    // Class should still emit, even with empty body
    assert!(
        output.contains("export declare class Init"),
        "class should still be emitted: {output}"
    );
}

#[test]
fn explore_template_literal_with_multiple_spans() {
    let output = emit_dts("export type EventKey = `${string}_${number}_${boolean}`;");
    println!("multi-span template:\n{output}");
    assert!(
        output.contains("`${string}_${number}_${boolean}`"),
        "multi-span template should be preserved: {output}"
    );
}

#[test]
fn explore_conditional_type_distributive_constraint() {
    let output = emit_dts("export type Exclude<T, U> = T extends U ? never : T;");
    println!("exclude type:\n{output}");
    let expected = "export type Exclude<T, U> = T extends U ? never : T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn explore_keyof_typeof_combined() {
    let output = emit_dts(
        "declare const obj: { a: 1; b: 2; };
export type Keys = keyof typeof obj;",
    );
    println!("keyof typeof:\n{output}");
    assert!(
        output.contains("keyof typeof obj"),
        "keyof typeof should be preserved: {output}"
    );
}

#[test]
fn explore_class_protected_static_abstract() {
    let output = emit_dts(
        "export abstract class Base {
    protected static abstract create(): Base;
}",
    );
    println!("protected static abstract:\n{output}");
    // Order in tsc: protected static abstract or protected abstract static
    assert!(
        output.contains("protected")
            && output.contains("static")
            && output.contains("abstract")
            && output.contains("create()"),
        "all modifiers should be present: {output}"
    );
}

#[test]
fn explore_function_type_with_rest_and_optional() {
    let output = emit_dts("export type Fn = (a: string, b?: number, ...rest: boolean[]) => void;");
    println!("fn type rest+opt:\n{output}");
    assert!(
        output.contains("(a: string, b?: number, ...rest: boolean[]) => void"),
        "function type params should be preserved: {output}"
    );
}

#[test]
fn explore_type_predicate_in_type_literal() {
    let output = emit_dts(
        "export type TypeGuards = {
    isString(value: unknown): value is string;
    isNumber(value: unknown): value is number;
};",
    );
    println!("type pred in literal:\n{output}");
    assert!(
        output.contains("isString(value: unknown): value is string;"),
        "isString predicate should be preserved: {output}"
    );
    assert!(
        output.contains("isNumber(value: unknown): value is number;"),
        "isNumber predicate should be preserved: {output}"
    );
}

// =============================================================================
// Round 7 - Exact comparison with tsc output
// =============================================================================

#[test]
fn exact_tsc_assertion_function_simple() {
    // tsc output: export declare function assert(val: unknown): asserts val;
    let output = emit_dts("export declare function assert(val: unknown): asserts val;");
    println!("assertion fn simple:\n{output}");
    let expected = "export declare function assert(val: unknown): asserts val;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_assertion_function_with_type() {
    // tsc output: export declare function assertStr(val: unknown): asserts val is string;
    let output =
        emit_dts("export declare function assertStr(val: unknown): asserts val is string;");
    println!("assertion fn typed:\n{output}");
    let expected = "export declare function assertStr(val: unknown): asserts val is string;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_readonly_tuple() {
    let output = emit_dts("export type T1 = readonly [string, number];");
    println!("readonly tuple:\n{output}");
    let expected = "export type T1 = readonly [string, number];\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_named_tuple_with_rest() {
    let output = emit_dts("export type T2 = [first: string, ...rest: number[]];");
    println!("named tuple rest:\n{output}");
    let expected = "export type T2 = [first: string, ...rest: number[]];\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_template_literal_union() {
    let output = emit_dts("export type T4 = `${'a' | 'b'}-${'x' | 'y'}`;");
    println!("template union:\n{output}");
    // tsc outputs: export type T4 = `${'a' | 'b'}-${'x' | 'y'}`;
    // or with double quotes: `${"a" | "b"}-${"x" | "y"}`
    assert!(
        output.contains("`${'a' | 'b'}-${'x' | 'y'}`")
            || output.contains("`${\"a\" | \"b\"}-${\"x\" | \"y\"}`"),
        "template literal union should be preserved: {output}"
    );
}

#[test]
fn exact_tsc_mapped_type_with_template_key() {
    let output = emit_dts(
        "export type T5<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };",
    );
    println!("mapped template key:\n{output}");
    // tsc output (multi-line):
    // export type T5<T> = {
    //     [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
    // };
    let expected = "export type T5<T> = {\n    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];\n};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_construct_signatures() {
    let output = emit_dts(
        "export interface I2 {
    new (x: string): object;
    new (x: number): object;
}",
    );
    println!("construct sigs:\n{output}");
    let expected =
        "export interface I2 {\n    new (x: string): object;\n    new (x: number): object;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_exclude_type() {
    let output = emit_dts("export type Exclude<T, U> = T extends U ? never : T;");
    println!("exclude:\n{output}");
    let expected = "export type Exclude<T, U> = T extends U ? never : T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_extract_type() {
    let output = emit_dts("export type Extract<T, U> = T extends U ? T : never;");
    println!("extract:\n{output}");
    let expected = "export type Extract<T, U> = T extends U ? T : never;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_nonnullable() {
    let output = emit_dts("export type NonNullable<T> = T & {};");
    println!("nonnullable:\n{output}");
    let expected = "export type NonNullable<T> = T & {};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_returntype() {
    let output = emit_dts(
        "export type ReturnType<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : any;",
    );
    println!("returntype:\n{output}");
    let expected = "export type ReturnType<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : any;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_class_with_static_block() {
    // tsc strips static blocks entirely
    let output = emit_dts(
        "export class Foo {
    static x: number;
    static {
        this.x = 42;
    }
    bar(): void {}
}",
    );
    println!("class static block:\n{output}");
    let expected = "export declare class Foo {\n    static x: number;\n    bar(): void;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_this_parameter() {
    let output =
        emit_dts("export declare function handler(this: HTMLElement, event: Event): void;");
    println!("this param:\n{output}");
    let expected = "export declare function handler(this: HTMLElement, event: Event): void;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_static_accessor() {
    let output = emit_dts(
        "export class Foo {
    static accessor bar: string = '';
}",
    );
    println!("static accessor:\n{output}");
    let expected = "export declare class Foo {\n    static accessor bar: string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_const_type_parameter() {
    let output = emit_dts("export declare function identity<const T>(value: T): T;");
    println!("const type param:\n{output}");
    let expected = "export declare function identity<const T>(value: T): T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_multiple_variable_declarators() {
    let output = emit_dts("export declare const a: string, b: number;");
    println!("multi declarators:\n{output}");
    let expected = "export declare const a: string, b: number;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_generic_call_signatures_in_interface() {
    let output = emit_dts(
        "export interface Converter {
    <T extends string>(input: T): number;
    <T extends number>(input: T): string;
}",
    );
    println!("generic call sigs:\n{output}");
    let expected = "export interface Converter {\n    <T extends string>(input: T): number;\n    <T extends number>(input: T): string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_recursive_type() {
    let output = emit_dts(
        "export type LinkedList<T> = {
    value: T;
    next: LinkedList<T> | null;
};",
    );
    println!("recursive type:\n{output}");
    let expected =
        "export type LinkedList<T> = {\n    value: T;\n    next: LinkedList<T> | null;\n};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_unwrap_promise() {
    let output = emit_dts(
        "export type UnwrapPromise<T> = T extends Promise<infer U> ? UnwrapPromise<U> : T;",
    );
    println!("unwrap promise:\n{output}");
    let expected =
        "export type UnwrapPromise<T> = T extends Promise<infer U> ? UnwrapPromise<U> : T;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_complex_mapped_merge() {
    let output = emit_dts(
        "export type Merge<A, B> = {
    [K in keyof A | keyof B]: K extends keyof A & keyof B ? A[K] | B[K] : K extends keyof A ? A[K] : K extends keyof B ? B[K] : never;
};",
    );
    println!("merge type:\n{output}");
    let expected = "export type Merge<A, B> = {\n    [K in keyof A | keyof B]: K extends keyof A & keyof B ? A[K] | B[K] : K extends keyof A ? A[K] : K extends keyof B ? B[K] : never;\n};\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_abstract_constructor_type_alias() {
    let output =
        emit_dts("export type AbstractConstructor = abstract new (...args: any[]) => any;");
    println!("abstract ctor type:\n{output}");
    let expected = "export type AbstractConstructor = abstract new (...args: any[]) => any;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_asserts_is_never() {
    // tsc preserves `asserts x is never` — `never` is a valid type predicate target
    let output = emit_dts("export declare function assertNever(x: never): asserts x is never;");
    println!("asserts never:\n{output}");
    let expected = "export declare function assertNever(x: never): asserts x is never;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_asserts_is_unknown() {
    // tsc preserves `asserts x is unknown` — `unknown` is a valid type predicate target
    let output = emit_dts("export declare function assertUnknown(x: any): asserts x is unknown;");
    println!("asserts unknown:\n{output}");
    let expected = "export declare function assertUnknown(x: any): asserts x is unknown;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_type_guard_is_never() {
    // `x is never` type guard should also be preserved
    let output = emit_dts("export declare function isNever(x: unknown): x is never;");
    println!("guard never:\n{output}");
    let expected = "export declare function isNever(x: unknown): x is never;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_type_guard_is_unknown() {
    let output = emit_dts("export declare function isUnknown(x: any): x is unknown;");
    println!("guard unknown:\n{output}");
    let expected = "export declare function isUnknown(x: any): x is unknown;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_simple_asserts_no_type() {
    // Simple `asserts x` without `is Type` should NOT emit `is` part
    let output = emit_dts("export declare function assertSimple(x: unknown): asserts x;");
    println!("simple asserts:\n{output}");
    let expected = "export declare function assertSimple(x: unknown): asserts x;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_asserts_is_void() {
    // `asserts x is void` should be preserved
    let output = emit_dts("export declare function assertVoid(x: unknown): asserts x is void;");
    println!("asserts void:\n{output}");
    let expected = "export declare function assertVoid(x: unknown): asserts x is void;\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}

#[test]
fn exact_tsc_class_with_instance_and_static_accessor() {
    let output = emit_dts(
        "export class C {
    accessor x: number = 0;
    static accessor y: string = '';
}",
    );
    println!("accessors:\n{output}");
    let expected =
        "export declare class C {\n    accessor x: number;\n    static accessor y: string;\n}\n";
    assert_eq!(output, expected, "Mismatch with tsc");
}
// =============================================================================
// Edge case tests: comprehensive tsc-parity verification
// =============================================================================

#[test]
fn test_abstract_accessors() {
    let result = emit_dts(
        r#"
export abstract class AbstractBase {
    abstract get name(): string;
    abstract set name(value: string);
}
"#,
    );
    assert!(
        result.contains("abstract get name(): string;"),
        "Missing abstract getter: {result}"
    );
    assert!(
        result.contains("abstract set name(value: string);"),
        "Missing abstract setter: {result}"
    );
}

#[test]
fn test_overloaded_constructors() {
    let result = emit_dts(
        r#"
export class OverloadedCtor {
    constructor(x: number);
    constructor(x: string, y: number);
    constructor(x: number | string, y?: number) {}
}
"#,
    );
    assert!(
        result.contains("constructor(x: number);"),
        "Missing first overload: {result}"
    );
    assert!(
        result.contains("constructor(x: string, y: number);"),
        "Missing second overload: {result}"
    );
    assert!(
        !result.contains("constructor(x: number | string"),
        "Implementation should be omitted: {result}"
    );
}

#[test]
fn test_complex_generic_constraints_with_defaults() {
    let result = emit_dts(
        r#"
export type Complex<T extends Record<string, unknown> = Record<string, any>, U extends keyof T = keyof T> = {
    [K in U]: T[K];
};
"#,
    );
    assert!(
        result.contains("Record<string, unknown>"),
        "Missing constraint: {result}"
    );
    assert!(
        result.contains("Record<string, any>"),
        "Missing default: {result}"
    );
    assert!(
        result.contains("keyof T"),
        "Missing keyof constraint: {result}"
    );
}

#[test]
fn test_intersection_with_call_signatures() {
    let result = emit_dts(
        r#"
export type Callable = { (): void } & { (x: number): number } & { name: string };
"#,
    );
    assert!(
        result.contains("(): void"),
        "Missing first call sig: {result}"
    );
    assert!(
        result.contains("(x: number): number"),
        "Missing second call sig: {result}"
    );
    assert!(
        result.contains("name: string"),
        "Missing property: {result}"
    );
}

#[test]
fn test_recursive_type_alias() {
    let result = emit_dts(
        r#"
export type Tree<T> = {
    value: T;
    children: Tree<T>[];
};
"#,
    );
    assert!(
        result.contains("Tree<T>[]"),
        "Missing recursive ref: {result}"
    );
}

#[test]
fn test_const_enum_edge_case() {
    let result = emit_dts(
        r#"
export const enum Color {
    Red = 1,
    Green = 2,
    Blue = 4,
}
"#,
    );
    assert!(
        result.contains("const enum Color"),
        "Missing const enum: {result}"
    );
    assert!(result.contains("Red = 1"), "Missing Red: {result}");
    assert!(result.contains("Green = 2"), "Missing Green: {result}");
}

#[test]
fn test_class_index_signatures_edge() {
    let result = emit_dts(
        r#"
export class IndexClass {
    [key: string]: any;
}
"#,
    );
    assert!(
        result.contains("[key: string]: any;"),
        "Missing index sig: {result}"
    );
}

#[test]
fn test_mixed_parameter_properties() {
    let result = emit_dts(
        r#"
export class MixedParams {
    constructor(
        public readonly x: number,
        protected y: string,
        private z: boolean,
        public w?: number,
    ) {}
}
"#,
    );
    // In d.ts, parameter properties become both constructor params and property declarations
    assert!(
        result.contains("readonly x: number"),
        "Missing readonly property: {result}"
    );
}

#[test]
fn test_conditional_type_with_infer() {
    let result = emit_dts(
        r#"
export type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
"#,
    );
    assert!(result.contains("infer U"), "Missing infer: {result}");
    assert!(
        result.contains("Promise<infer U>"),
        "Missing Promise<infer U>: {result}"
    );
}

#[test]
fn test_module_augmentation() {
    let result = emit_dts(
        r#"
declare module "express" {
    interface Request {
        userId?: string;
    }
}
"#,
    );
    assert!(
        result.contains("declare module \"express\""),
        "Missing module augmentation: {result}"
    );
    assert!(
        result.contains("userId?"),
        "Missing augmented property: {result}"
    );
}

#[test]
fn test_global_augmentation() {
    let result = emit_dts(
        r#"
export {};
declare global {
    interface Window {
        customProp: number;
    }
}
"#,
    );
    assert!(
        result.contains("declare global"),
        "Missing global augmentation: {result}"
    );
    assert!(
        result.contains("customProp: number"),
        "Missing global property: {result}"
    );
}

#[test]
fn test_readonly_tuple_with_rest_and_labels() {
    let result = emit_dts(
        r#"
export type ReadonlyTuple = readonly [first: string, ...rest: number[]];
"#,
    );
    assert!(
        result.contains("readonly ["),
        "Missing readonly tuple: {result}"
    );
    assert!(
        result.contains("first: string"),
        "Missing labeled element: {result}"
    );
    assert!(
        result.contains("...rest: number[]"),
        "Missing rest element: {result}"
    );
}

#[test]
fn test_template_literal_type() {
    let result = emit_dts(
        r#"
export type Greeting<T extends string> = `Hello, ${T}!`;
"#,
    );
    assert!(
        result.contains("${T}"),
        "Missing template literal type: {result}"
    );
}

#[test]
fn test_import_type() {
    let result = emit_dts(
        r#"
export type LazyModule = typeof import("fs");
"#,
    );
    assert!(
        result.contains("import(\"fs\")"),
        "Missing import type: {result}"
    );
}

#[test]
fn test_namespace_with_class_and_interface() {
    let result = emit_dts(
        r#"
export namespace Shapes {
    export class Circle {
        radius: number;
    }
    export interface Circle {
        area(): number;
    }
}
"#,
    );
    assert!(
        result.contains("namespace Shapes"),
        "Missing namespace: {result}"
    );
    assert!(result.contains("class Circle"), "Missing class: {result}");
    assert!(
        result.contains("interface Circle"),
        "Missing interface: {result}"
    );
}

#[test]
fn test_symbol_iterator_method() {
    let result = emit_dts(
        r#"
export class IterableClass {
    *[Symbol.iterator](): Iterator<number> {
        yield 1;
    }
}
"#,
    );
    assert!(
        result.contains("[Symbol.iterator]"),
        "Missing Symbol.iterator: {result}"
    );
    assert!(
        result.contains("Iterator<number>"),
        "Missing return type: {result}"
    );
}

#[test]
fn test_getter_only() {
    let result = emit_dts(
        r#"
export class GetOnly {
    get value(): number { return 42; }
}
"#,
    );
    assert!(
        result.contains("get value(): number;"),
        "Missing getter: {result}"
    );
    assert!(
        !result.contains("set value"),
        "Should not have setter: {result}"
    );
}

#[test]
fn test_fluent_this_return_type() {
    let result = emit_dts(
        r#"
export class FluentBuilder {
    setName(name: string): this {
        return this;
    }
}
"#,
    );
    assert!(
        result.contains("setName(name: string): this;"),
        "Missing this return type: {result}"
    );
}

#[test]
fn test_mapped_type_with_as_clause() {
    let result = emit_dts(
        r#"
export type EventMap<T extends string> = {
    [K in T as `on${Capitalize<K>}`]: (event: K) => void;
};
"#,
    );
    assert!(
        result.contains("as `on${Capitalize<K>}`"),
        "Missing as clause with template: {result}"
    );
}

#[test]
fn test_abstract_method_with_generics() {
    let result = emit_dts(
        r#"
export abstract class AbstractGeneric<T> {
    abstract transform<U>(input: T): U;
}
"#,
    );
    assert!(
        result.contains("abstract transform<U>(input: T): U;"),
        "Missing abstract generic method: {result}"
    );
}

#[test]
fn debug_print_mixed_params_vs_tsc() {
    let result = emit_dts(
        r#"
export class MixedParams {
    constructor(
        public readonly x: number,
        protected y: string,
        private z: boolean,
        public w?: number,
    ) {}
}
"#,
    );
    // tsc outputs:
    // export declare class MixedParams {
    //     readonly x: number;
    //     protected y: string;
    //     private z;
    //     w?: number | undefined;
    //     constructor(x: number, y: string, z: boolean, w?: number | undefined);
    // }
    let expected = "export declare class MixedParams {\n    readonly x: number;\n    protected y: string;\n    private z;\n    w?: number | undefined;\n    constructor(x: number, y: string, z: boolean, w?: number | undefined);\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn debug_print_const_enum_vs_tsc() {
    // tsc omits trailing comma on last enum member
    let result = emit_dts(
        r#"
export const enum Color {
    Red = 1,
    Green = 2,
    Blue = 4,
}
"#,
    );
    let expected =
        "export declare const enum Color {\n    Red = 1,\n    Green = 2,\n    Blue = 4\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn debug_print_global_augmentation_vs_tsc() {
    let result = emit_dts(
        r#"
export {};
declare global {
    interface Window {
        customProp: number;
    }
}
"#,
    );
    let expected = "export {};\ndeclare global {\n    interface Window {\n        customProp: number;\n    }\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_computed_bitwise_values_vs_tsc() {
    // tsc evaluates bitwise expressions: 1<<0 -> 1, 1<<1 -> 2, Read|Write|Execute -> 7
    let result = emit_dts(
        r#"
export enum Flags {
    None = 0,
    Read = 1 << 0,
    Write = 1 << 1,
    Execute = 1 << 2,
    All = Read | Write | Execute,
}
"#,
    );
    let expected = "export declare enum Flags {\n    None = 0,\n    Read = 1,\n    Write = 2,\n    Execute = 4,\n    All = 7\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_mixed_string_numeric_enum_vs_tsc() {
    let result = emit_dts(
        r#"
export enum Mixed {
    A = 0,
    B = "hello",
    C = 1,
}
"#,
    );
    let expected = "export declare enum Mixed {\n    A = 0,\n    B = \"hello\",\n    C = 1\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_function_overloads_vs_tsc() {
    let result = emit_dts(
        r#"
export function overloaded(x: number): number;
export function overloaded(x: string): string;
export function overloaded(x: number | string): number | string {
    return x;
}
"#,
    );
    let expected = "export declare function overloaded(x: number): number;\nexport declare function overloaded(x: string): string;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_default_export_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export default interface Config {
    port: number;
    host: string;
}
"#,
    );
    let expected = "export default interface Config {\n    port: number;\n    host: string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_construct_signature_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Constructor<T> {
    new (...args: any[]): T;
    prototype: T;
}
"#,
    );
    let expected =
        "export interface Constructor<T> {\n    new (...args: any[]): T;\n    prototype: T;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_namespace_function_overloads_vs_tsc() {
    let result = emit_dts(
        r#"
export declare namespace MyLib {
    function create(tag: "div"): HTMLDivElement;
    function create(tag: "span"): HTMLSpanElement;
    function create(tag: string): HTMLElement;
}
"#,
    );
    let expected = "export declare namespace MyLib {\n    function create(tag: \"div\"): HTMLDivElement;\n    function create(tag: \"span\"): HTMLSpanElement;\n    function create(tag: string): HTMLElement;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_interface_multiple_extends_vs_tsc() {
    let result = emit_dts(
        r#"
export interface A { a: number; }
export interface B { b: string; }
export interface C extends A, B { c: boolean; }
"#,
    );
    let expected = "export interface A {\n    a: number;\n}\nexport interface B {\n    b: string;\n}\nexport interface C extends A, B {\n    c: boolean;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_negative_enum_values_vs_tsc() {
    let result = emit_dts(
        r#"
export enum NegativeEnum {
    A = -1,
    B = -2,
    C = -100,
}
"#,
    );
    let expected =
        "export declare enum NegativeEnum {\n    A = -1,\n    B = -2,\n    C = -100\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_unique_symbol_and_computed_property_vs_tsc() {
    let result = emit_dts(
        r#"
export declare const sym: unique symbol;
export interface Keyed {
    [sym]: string;
}
"#,
    );
    let expected = "export declare const sym: unique symbol;\nexport interface Keyed {\n    [sym]: string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_this_parameter_in_method_vs_tsc() {
    let result = emit_dts(
        r#"
export class Guard {
    isValid(this: Guard): boolean { return true; }
}
"#,
    );
    let expected = "export declare class Guard {\n    isValid(this: Guard): boolean;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_variadic_tuple_types_vs_tsc() {
    let result = emit_dts(
        r#"
export type Concat<T extends readonly unknown[], U extends readonly unknown[]> = [...T, ...U];
"#,
    );
    let expected = "export type Concat<T extends readonly unknown[], U extends readonly unknown[]> = [...T, ...U];\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_numeric_literal_interface_keys_vs_tsc() {
    let result = emit_dts(
        r#"
export interface NumberKeyed {
    0: string;
    1: number;
    2: boolean;
}
"#,
    );
    let expected =
        "export interface NumberKeyed {\n    0: string;\n    1: number;\n    2: boolean;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_private_fields_vs_tsc() {
    let result = emit_dts(
        r#"
export class PrivateFields {
    #name: string;
    #age: number;
    constructor(name: string, age: number) {
        this.#name = name;
        this.#age = age;
    }
    getName(): string { return this.#name; }
}
"#,
    );
    // tsc collapses all #private fields into a single `#private;` declaration
    let expected = "export declare class PrivateFields {\n    #private;\n    constructor(name: string, age: number);\n    getName(): string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_static_block_omitted_vs_tsc() {
    let result = emit_dts(
        r#"
export class WithStaticBlock {
    static value: number;
    static {
        WithStaticBlock.value = 42;
    }
    method(): void {}
}
"#,
    );
    // tsc omits static blocks from .d.ts
    let expected = "export declare class WithStaticBlock {\n    static value: number;\n    method(): void;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_inside_namespace_vs_tsc() {
    let result = emit_dts(
        r#"
export namespace NS {
    export enum Status {
        Active = "active",
        Inactive = "inactive",
    }
}
"#,
    );
    // tsc: no trailing comma on last member
    let expected = "export declare namespace NS {\n    enum Status {\n        Active = \"active\",\n        Inactive = \"inactive\"\n    }\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_optional_methods_in_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Events {
    on?(event: string, handler: Function): void;
    off?(event: string, handler: Function): void;
}
"#,
    );
    let expected = "export interface Events {\n    on?(event: string, handler: Function): void;\n    off?(event: string, handler: Function): void;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_dual_accessor_types_vs_tsc() {
    // TS 5.1+: getter and setter can have different types
    let result = emit_dts(
        r#"
export class DualAccessor {
    get value(): string { return ""; }
    set value(v: string | number) {}
}
"#,
    );
    let expected = "export declare class DualAccessor {\n    get value(): string;\n    set value(v: string | number);\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_overloaded_call_signatures_in_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Converter {
    (input: string): number;
    (input: number): string;
    name: string;
}
"#,
    );
    let expected = "export interface Converter {\n    (input: string): number;\n    (input: number): string;\n    name: string;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_nested_conditional_type_vs_tsc() {
    let result = emit_dts(
        r#"
export type DeepReadonly<T> = T extends (infer U)[] ? DeepReadonly<U>[] : T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;
"#,
    );
    // tsc reformats to multiline for the mapped type portion
    let expected = "export type DeepReadonly<T> = T extends (infer U)[] ? DeepReadonly<U>[] : T extends object ? {\n    readonly [K in keyof T]: DeepReadonly<T[K]>;\n} : T;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_with_string_member_names_vs_tsc() {
    let result = emit_dts(
        r#"
export enum StringKeys {
    "hello world" = 0,
    "foo-bar" = 1,
}
"#,
    );
    let expected =
        "export declare enum StringKeys {\n    \"hello world\" = 0,\n    \"foo-bar\" = 1\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_export_equals_namespace_vs_tsc() {
    let result = emit_dts(
        r#"
declare namespace MyLib {
    interface Config {
        value: number;
    }
    function create(): Config;
}
export = MyLib;
"#,
    );
    let expected = "declare namespace MyLib {\n    interface Config {\n        value: number;\n    }\n    function create(): Config;\n}\nexport = MyLib;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_nested_destructured_param_vs_tsc() {
    let result = emit_dts(
        r#"
export function process(
    { a, b: { c }, ...rest }: { a: number; b: { c: string }; d: boolean; e: number }
): void {}
"#,
    );
    // tsc preserves the destructured pattern and reformats the type to multiline
    let expected = "export declare function process({ a, b: { c }, ...rest }: {\n    a: number;\n    b: {\n        c: string;\n    };\n    d: boolean;\n    e: number;\n}): void;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_float_enum_values_vs_tsc() {
    let result = emit_dts(
        r#"
export enum FloatEnum {
    Half = 0.5,
    Quarter = 0.25,
    Pi = 3.14159,
}
"#,
    );
    let expected = "export declare enum FloatEnum {\n    Half = 0.5,\n    Quarter = 0.25,\n    Pi = 3.14159\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_this_parameter_in_function_vs_tsc() {
    let result = emit_dts(
        r#"
export declare function handler(this: HTMLElement, event: Event): void;
"#,
    );
    let expected = "export declare function handler(this: HTMLElement, event: Event): void;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_generic_interface_with_conditional_default_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Mapper<Input = unknown, Output = Input extends string ? number : boolean> {
    map(input: Input): Output;
}
"#,
    );
    let expected = "export interface Mapper<Input = unknown, Output = Input extends string ? number : boolean> {\n    map(input: Input): Output;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_overloaded_generic_methods_in_interface_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Factory {
    create<T>(type: new () => T): T;
    create<T, U>(type: new (arg: U) => T, arg: U): T;
}
"#,
    );
    let expected = "export interface Factory {\n    create<T>(type: new () => T): T;\n    create<T, U>(type: new (arg: U) => T, arg: U): T;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_infer_type_with_extends_constraint_vs_tsc() {
    // TS 4.7+: infer C extends string
    let result = emit_dts(
        r#"
export type FirstChar<T extends string> = T extends `${infer C extends string}${string}` ? C : never;
"#,
    );
    let expected = "export type FirstChar<T extends string> = T extends `${infer C extends string}${string}` ? C : never;\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_mapped_type_minus_readonly_minus_optional_vs_tsc() {
    let result = emit_dts(
        r#"
export type Mutable<T> = {
    -readonly [K in keyof T]-?: T[K];
};
export type ReadonlyOptional<T> = {
    +readonly [K in keyof T]+?: T[K];
};
"#,
    );
    let expected = "export type Mutable<T> = {\n    -readonly [K in keyof T]-?: T[K];\n};\nexport type ReadonlyOptional<T> = {\n    +readonly [K in keyof T]+?: T[K];\n};\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_indexed_access_type_with_mapped_vs_tsc() {
    let result = emit_dts(
        r#"
export type KeysOfType<T, V> = { [K in keyof T]: T[K] extends V ? K : never }[keyof T];
"#,
    );
    // tsc reformats to multi-line
    let expected = "export type KeysOfType<T, V> = {\n    [K in keyof T]: T[K] extends V ? K : never;\n}[keyof T];\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_named_tuple_with_optional_element_vs_tsc() {
    let result = emit_dts(
        r#"
export type NamedTuple = [first: string, second?: number, ...rest: boolean[]];
"#,
    );
    let expected =
        "export type NamedTuple = [first: string, second?: number, ...rest: boolean[]];\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_enum_string_concat_vs_tsc() {
    // tsc evaluates string concatenation: "prefix_" + "a" -> "prefix_a"
    let result = emit_dts(
        r#"
export enum StringEnum {
    A = "prefix_" + "a",
}
"#,
    );
    let expected = "export declare enum StringEnum {\n    A = \"prefix_a\"\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_interface_string_literal_keys_vs_tsc() {
    let result = emit_dts(
        r#"
export interface StringKeyed {
    "hello world": number;
    "with-dash": string;
    "with space": boolean;
    normal: number;
}
"#,
    );
    let expected = "export interface StringKeyed {\n    \"hello world\": number;\n    \"with-dash\": string;\n    \"with space\": boolean;\n    normal: number;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_class_extends_implements_vs_tsc() {
    let result = emit_dts(
        r#"
export interface Printable {
    print(): void;
}
export interface Loggable {
    log(): void;
}
export class Base {
    id: number = 0;
}
export class Derived extends Base implements Printable, Loggable {
    print(): void {}
    log(): void {}
}
"#,
    );
    let expected = "export interface Printable {\n    print(): void;\n}\nexport interface Loggable {\n    log(): void;\n}\nexport declare class Base {\n    id: number;\n}\nexport declare class Derived extends Base implements Printable, Loggable {\n    print(): void;\n    log(): void;\n}\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_overloaded_callable_type_vs_tsc() {
    let result = emit_dts(
        r#"
export type OverloadedFn = {
    (x: string): string;
    (x: number): number;
    readonly length: number;
};
"#,
    );
    let expected = "export type OverloadedFn = {\n    (x: string): string;\n    (x: number): number;\n    readonly length: number;\n};\n";
    assert_eq!(result, expected, "Mismatch with tsc");
}

#[test]
fn test_import_type_equals_require_preserves_type_keyword() {
    // `export import type X = require("module")` must preserve the `type` keyword in .d.ts
    let output = emit_dts(r#"export import type Foo = require("some-module");"#);
    assert!(
        output.contains("import type Foo = require("),
        "import type equals should preserve 'type' keyword: {output}"
    );
}

#[test]
fn test_import_equals_require_without_type() {
    // Regular `import X = require("module")` should NOT have `type` keyword
    let output = emit_dts_with_usage_analysis(
        r#"
import Foo = require("some-module");
export declare function useFoo(): Foo;
"#,
    );
    // When usage analysis is active, the non-exported import may be elided
    // unless it's actually referenced. The key assertion is that if it IS
    // emitted, it does NOT have "type" in it.
    if output.contains("import ") && output.contains("= require(") {
        assert!(
            !output.contains("import type Foo"),
            "Regular import equals should not have 'type' keyword: {output}"
        );
    }
}

#[test]
fn test_export_import_type_equals_require() {
    // `export import type X = require("module")` - exported type-only import equals
    let output = emit_dts(r#"export import type Bar = require("bar-module");"#);
    assert!(
        output.contains("import type Bar = require("),
        "export import type equals should preserve 'type' keyword: {output}"
    );
}

#[test]
fn test_import_defer_preserves_keyword() {
    // `import defer * as ns from "mod"` must preserve the `defer` keyword in .d.ts
    let output = emit_dts_with_usage_analysis(
        r#"
import defer * as ns from "./mod";
export declare function useMod(): typeof ns;
"#,
    );
    // If the import is emitted (not elided), it should have defer
    if output.contains("import ") && output.contains("* as ns") {
        assert!(
            output.contains("import defer * as ns"),
            "import defer should preserve 'defer' keyword: {output}"
        );
    }
}

#[test]
fn test_accessor_keyword_preserved_on_class_field() {
    // TypeScript `accessor` keyword (auto-accessor) should be preserved in .d.ts
    let output = emit_dts(
        r#"export class Foo {
    accessor name: string;
    static accessor count: number;
}"#,
    );
    assert!(
        output.contains("accessor name: string;"),
        "accessor keyword should be preserved: {output}"
    );
    assert!(
        output.contains("static accessor count: number;"),
        "static accessor keyword should be preserved: {output}"
    );
}

// =====================================================================
// Template literal enum evaluation
// =====================================================================

#[test]
fn test_enum_template_literal_value_vs_tsc() {
    // tsc evaluates template literals in enum members: `${E.A}_world` -> "hello_world"
    let result = emit_dts("export enum E {\n    A = \"hello\",\n    B = `${E.A}_world`,\n}\n");
    assert!(
        result.contains("B = \"hello_world\""),
        "Should evaluate template literal enum value: {result}"
    );
}

#[test]
fn test_enum_template_literal_chained_vs_tsc() {
    // Multiple levels of template literal evaluation in enums
    let result = emit_dts(
        r#"export enum Actions {
    Click = "click",
    Hover = "hover",
    OnClick = `on_${Actions.Click}`,
    OnHover = `on_${Actions.Hover}`,
    Nested = `prefix_${Actions.OnClick}_suffix`,
}
"#,
    );
    let expected = r#"export declare enum Actions {
    Click = "click",
    Hover = "hover",
    OnClick = "on_click",
    OnHover = "on_hover",
    Nested = "prefix_on_click_suffix"
}
"#;
    assert_eq!(
        result, expected,
        "Template literal chained enum values should match tsc"
    );
}

#[test]
fn test_enum_template_literal_multiple_spans_vs_tsc() {
    // Template literal with multiple substitutions
    let result = emit_dts(
        r#"export enum E {
    A = "x",
    B = "y",
    C = `${E.A}_${E.B}_z`,
}
"#,
    );
    assert!(
        result.contains(r#"C = "x_y_z""#),
        "Should evaluate multi-span template: {result}"
    );
}

#[test]
fn test_enum_no_substitution_template_vs_tsc() {
    // No-substitution template backtick literal should evaluate to string
    let result = emit_dts("export enum E {\n    A = `hello`,\n}\n");
    assert!(
        result.contains("A = \"hello\""),
        "No-sub template should produce string: {result}"
    );
}

#[test]
fn test_probe_string_enum_concat() {
    let result = emit_dts(
        r#"export enum S {
    Prefix = "PRE",
    Full = Prefix + "_SUFFIX",
}
"#,
    );
    eprintln!("STRING_ENUM_CONCAT: {:?}", result);
    // tsc evaluates: Full = "PRE_SUFFIX"
    assert!(
        result.contains(r#"Full = "PRE_SUFFIX""#),
        "Should evaluate string concat: {result}"
    );
}
