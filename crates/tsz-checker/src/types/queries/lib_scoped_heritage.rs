//! Scoped library heritage helpers.
//!
//! Some lib files intentionally hide implementation-only declarations inside an
//! external module, then expose them through a `declare global` interface.  For
//! example, `lib.es2025.iterator.d.ts` uses a module-scoped
//! `type IteratorObjectConstructor = typeof Iterator` so the global
//! `IteratorConstructor` can inherit an abstract construct signature without
//! polluting the global namespace with the helper class declaration.

use crate::state::CheckerState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, CallableShape, TypeId, TypeParamInfo};

use super::lib_resolution::{
    keyword_name_to_type_id, keyword_syntax_to_type_id, resolve_scope_chain,
};

pub(super) struct LibHeritageBase<'a> {
    pub(super) name: String,
    pub(super) expr_idx: NodeIndex,
    pub(super) type_arg_indices: Vec<NodeIndex>,
    pub(super) arena: &'a NodeArena,
}

impl<'a> CheckerState<'a> {
    pub(super) fn resolve_scoped_lib_typeof_class_heritage(
        &mut self,
        base: &LibHeritageBase<'_>,
        lib_contexts: &[crate::context::LibContext],
    ) -> Option<TypeId> {
        let (lib_ctx, alias_sym_id) = lib_contexts.iter().find_map(|ctx| {
            resolve_scope_chain(&ctx.binder, base.arena, base.expr_idx)
                .or_else(|| ctx.binder.get_node_symbol(base.expr_idx))
                .or_else(|| ctx.binder.file_locals.get(&base.name))
                .map(|sym_id| (ctx, sym_id))
        })?;
        let alias_symbol = lib_ctx.binder.get_symbol(alias_sym_id)?;
        if !alias_symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return None;
        }

        for &decl_idx in &alias_symbol.declarations {
            let Some(decl_node) = base.arena.get(decl_idx) else {
                continue;
            };
            let Some(alias) = base.arena.get_type_alias(decl_node) else {
                continue;
            };
            let Some(type_node) = base.arena.get(alias.type_node) else {
                continue;
            };
            let Some(type_query) = base.arena.get_type_query(type_node) else {
                continue;
            };
            let class_sym_id =
                resolve_scope_chain(&lib_ctx.binder, base.arena, type_query.expr_name)
                    .or_else(|| lib_ctx.binder.get_node_symbol(type_query.expr_name))
                    .or_else(|| {
                        base.arena
                            .get_identifier_text(type_query.expr_name)
                            .and_then(|name| lib_ctx.binder.file_locals.get(name))
                    })?;
            let class_symbol = lib_ctx.binder.get_symbol(class_sym_id)?;
            if class_symbol.flags
                & (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::ABSTRACT)
                != (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::ABSTRACT)
            {
                continue;
            }

            let class_name = class_symbol.escaped_name.as_str();
            let return_base_sym = self.resolve_lib_symbol_by_name(class_name)?;
            let return_base = self
                .ctx
                .types
                .factory()
                .lazy(self.ctx.get_lib_def_id(return_base_sym));
            let type_params = self.collect_scoped_lib_class_type_params(
                base.arena,
                class_symbol.primary_declaration()?,
            );
            let return_args = type_params
                .iter()
                .cloned()
                .map(|param| self.ctx.types.type_param(param))
                .collect::<Vec<_>>();
            let return_type = if return_args.is_empty() {
                return_base
            } else {
                self.ctx.types.application(return_base, return_args)
            };
            let construct_signature = CallSignature {
                type_params,
                params: Vec::new(),
                this_type: None,
                return_type,
                type_predicate: None,
                is_method: false,
            };
            return Some(self.ctx.types.factory().callable(CallableShape {
                call_signatures: Vec::new(),
                construct_signatures: vec![construct_signature],
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: Some(class_sym_id),
                is_abstract: true,
            }));
        }

        None
    }

    fn collect_scoped_lib_class_type_params(
        &mut self,
        arena: &NodeArena,
        class_decl_idx: NodeIndex,
    ) -> Vec<TypeParamInfo> {
        let Some(class_node) = arena.get(class_decl_idx) else {
            return Vec::new();
        };
        let Some(class_data) = arena.get_class(class_node) else {
            return Vec::new();
        };
        let Some(type_parameters) = class_data.type_parameters.as_ref() else {
            return Vec::new();
        };

        type_parameters
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = arena.get(param_idx)?;
                let param = arena.get_type_parameter(param_node)?;
                let name = arena
                    .get_identifier_at(param.name)
                    .map(|ident| ident.escaped_text.as_str())
                    .unwrap_or("T");
                let name_atom = self.ctx.types.intern_string(name);
                let default = arena
                    .get(param.default)
                    .and_then(|node| keyword_syntax_to_type_id(node.kind))
                    .or_else(|| {
                        arena
                            .get_identifier_text(param.default)
                            .and_then(keyword_name_to_type_id)
                    });
                let constraint = arena
                    .get(param.constraint)
                    .and_then(|node| keyword_syntax_to_type_id(node.kind))
                    .or_else(|| {
                        arena
                            .get_identifier_text(param.constraint)
                            .and_then(keyword_name_to_type_id)
                    });
                Some(TypeParamInfo {
                    name: name_atom,
                    constraint,
                    default,
                    is_const: arena.has_modifier(&param.modifiers, SyntaxKind::ConstKeyword),
                })
            })
            .collect()
    }
}
