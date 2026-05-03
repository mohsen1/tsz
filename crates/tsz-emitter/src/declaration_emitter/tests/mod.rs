// Shared imports and helpers for declaration emitter tests

pub(super) use super::*;
pub(super) use rustc_hash::FxHashMap;
pub(super) use std::sync::Arc;
pub(super) use tsz_binder::{BinderState, symbol_flags};
pub(super) use tsz_parser::parser::node::NodeAccess;
pub(super) use tsz_parser::parser::syntax_kind_ext;
pub(super) use tsz_parser::parser::{NodeIndex, ParserState};
pub(super) use tsz_solver::{
    CallSignature, CallableShape, DefId, FunctionShape, ObjectFlags, ObjectShape, ParamInfo,
    PropertyInfo, SymbolRef, TupleElement, TypeId, TypeInterner,
};

// =============================================================================
// Helper
// =============================================================================

pub(super) fn emit_dts(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

pub(super) fn emit_dts_with_binding(source: &str) -> String {
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

pub(super) fn emit_dts_with_usage_analysis(source: &str) -> String {
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

pub(super) fn emit_js_dts(source: &str) -> String {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

pub(super) fn emit_js_dts_with_usage_analysis(source: &str) -> String {
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

pub(super) fn find_class_symbol(
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

pub(super) fn find_interface_symbol(
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

pub(super) fn find_first_class_method_name(
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

pub(super) fn find_first_class_node(parser: &ParserState, class_kind: u16) -> NodeIndex {
    parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| (node.kind == class_kind).then_some(NodeIndex(idx as u32)))
        .unwrap_or_else(|| panic!("missing class node of kind {class_kind}"))
}

pub(super) fn find_class_node(
    parser: &ParserState,
    class_name: &str,
    class_kind: u16,
) -> NodeIndex {
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

pub(super) fn find_class_extends_expression(
    parser: &ParserState,
    class_idx: NodeIndex,
) -> NodeIndex {
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

mod class_features;
mod comprehensive_parity;
mod computed_properties;
mod enum_template_and_advanced;
mod export_specifiers;
mod fix_verification;
mod generics_and_ambient;
mod misc_features;
mod probes_issues;
mod probes_systematic;
mod probes_tsc_comparison;
mod simple_declarations;
mod type_formatting;
mod type_info;
