//! Type reference resolution: interfaces, type aliases, and type references
//! on `CheckerState`.

include!("core_large_methods/get_type_from_type_reference_8_7.rs");

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Keep the lowered generic base/defaults and ordinary argument identity,
    /// but replace checker-owned explicit type-argument slots when needed so
    /// inline type literals preserve computed property names and related facts.
    fn rebuild_application_with_checker_type_args(
        &mut self,
        application: TypeId,
        type_args: &NodeList,
        resolved_type_args: Option<&[TypeId]>,
    ) -> TypeId {
        let Some((base, mut app_args)) = query::get_application_info(self.ctx.types, application)
        else {
            return application;
        };

        for (arg_pos, (slot, &arg_idx)) in
            app_args.iter_mut().zip(type_args.nodes.iter()).enumerate()
        {
            if self.type_arg_needs_checker_resolution(arg_idx) {
                *slot = if let Some(resolved) = resolved_type_args
                    .and_then(|args| args.get(arg_pos))
                    .copied()
                {
                    resolved
                } else {
                    self.get_type_from_type_node(arg_idx)
                };
            }
        }

        self.ctx.types.application(base, app_args)
    }

    fn resolve_type_argument_nodes_once(
        &mut self,
        resolved_type_args: &mut Option<Vec<TypeId>>,
        type_args: &NodeList,
    ) -> Vec<TypeId> {
        if let Some(cached) = resolved_type_args.as_ref() {
            return cached.clone();
        }
        let resolved = type_args
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect::<Vec<_>>();
        *resolved_type_args = Some(resolved.clone());
        resolved
    }

    fn type_arg_needs_checker_resolution(&self, arg_idx: NodeIndex) -> bool {
        let Some(type_lit) = self
            .ctx
            .arena
            .get(arg_idx)
            .and_then(|node| self.ctx.arena.get_type_literal(node))
        else {
            return false;
        };

        type_lit.members.nodes.iter().any(|&member_idx| {
            self.ctx
                .arena
                .get(member_idx)
                .and_then(|member| self.ctx.arena.get_signature(member))
                .and_then(|sig| self.ctx.arena.get(sig.name))
                .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        })
    }

    fn same_file_type_alias_parts_for_name(
        &self,
        name: &str,
    ) -> Option<(Option<NodeList>, NodeIndex, Option<tsz_binder::SymbolId>)> {
        self.ctx
            .arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                let alias_name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                (alias_name == name).then(|| {
                    (
                        type_alias.type_parameters.clone(),
                        type_alias.type_node,
                        self.ctx.binder.node_symbols.get(&(idx as u32)).copied(),
                    )
                })
            })
    }

    fn type_node_is_outside_symbol_declarations(
        &self,
        node_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return true;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        !symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .is_some_and(|decl| node.pos >= decl.pos && node.end <= decl.end)
        })
    }

    fn def_body_involves_depth_poisoned_def(&self, def_id: tsz_solver::DefId) -> bool {
        if !self.ctx.definition_store.has_any_depth_poisoned() {
            return false;
        }

        self.ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_def(def_id))
            .is_some_and(|body| self.ctx.type_involves_depth_poisoned_def(body))
    }

    fn def_body_can_own_ambient_depth(&self, def_id: tsz_solver::DefId) -> bool {
        let Some(body) = self
            .ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_def(def_id))
            .or_else(|| self.ctx.definition_store.get_body(def_id))
        else {
            return false;
        };

        let db = self.ctx.types.as_type_database();
        crate::query_boundaries::common::contains_conditional_type(db, body)
            || crate::query_boundaries::common::contains_keyof_type(db, body)
            || tsz_solver::type_queries::contains_index_access_type(db, body)
            || crate::query_boundaries::common::is_mapped_type(db, body)
    }

    __tsz_split_core_get_type_from_type_reference_8_7!();

    pub(crate) fn handle_missing_global_type_with_args(
        &mut self,
        name: &str,
        type_ref: &tsz_parser::parser::node::TypeRefData,
        type_name_idx: NodeIndex,
    ) -> TypeId {
        if self.is_mapped_type_utility(name) {
            if self.ctx.compiler_options.no_lib {
                self.report_missing_lib_type_name(name, type_name_idx);
                return TypeId::ANY;
            }

            if let Some(args) = &type_ref.type_arguments {
                let type_args: Vec<TypeId> = args
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect();

                if name == "Pick" && type_args.len() == 2 {
                    let factory = self.ctx.types.factory();
                    let key_param = tsz_solver::TypeParamInfo {
                        name: self.ctx.types.intern_string("__pick_key"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    };
                    let key_type = self.ctx.types.type_param(key_param);
                    return factory.mapped(tsz_solver::MappedType {
                        type_param: key_param,
                        constraint: type_args[1],
                        name_type: None,
                        template: factory.index_access(type_args[0], key_type),
                        readonly_modifier: None,
                        optional_modifier: None,
                    });
                }

                let (base_type, _) = self.resolve_lib_type_with_params(name);
                if let Some(base_type) = base_type {
                    return self.ctx.types.factory().application(base_type, type_args);
                }
            }
            return TypeId::ANY;
        }

        self.report_missing_lib_type_name(name, type_name_idx);

        if !self.ctx.compiler_options.no_lib
            && matches!(name, "Promise" | "PromiseLike")
            && let Some(args) = &type_ref.type_arguments
        {
            let type_args: Vec<TypeId> = args
                .nodes
                .iter()
                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                .collect();
            if !type_args.is_empty() {
                let promise_base = {
                    let lib_binders = self.get_lib_binders();
                    crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                        name,
                        self.ctx.binder,
                        self.ctx.global_file_locals_index.as_deref(),
                        self.ctx
                            .all_binders
                            .as_ref()
                            .map(|binders| binders.as_ref().as_slice()),
                        &self.ctx.lib_contexts,
                    )
                    .or_else(|| {
                        lib_binders
                            .iter()
                            .find_map(|binder| binder.file_locals.get(name))
                    })
                    .map(|sym_id| {
                        let _ = self.resolve_lib_type_by_name(name);
                        let def_id = self.ctx.get_canonical_lib_def_id(name, sym_id);
                        self.ctx.types.factory().lazy(def_id)
                    })
                    .unwrap_or(TypeId::PROMISE_BASE)
                };
                return self
                    .ctx
                    .types
                    .factory()
                    .application(promise_base, type_args);
            }
        }

        if let Some(args) = &type_ref.type_arguments {
            for &arg_idx in &args.nodes {
                let _ = self.get_type_from_type_node(arg_idx);
            }
        }
        TypeId::ERROR
    }

    /// Resolve a primitive keyword like `number`, `string`, etc.
    fn resolve_primitive_keyword(name: &str) -> Option<TypeId> {
        match name {
            "number" => Some(TypeId::NUMBER),
            "string" => Some(TypeId::STRING),
            "boolean" => Some(TypeId::BOOLEAN),
            "void" => Some(TypeId::VOID),
            "any" => Some(TypeId::ANY),
            "never" => Some(TypeId::NEVER),
            "unknown" => Some(TypeId::UNKNOWN),
            "undefined" => Some(TypeId::UNDEFINED),
            "null" => Some(TypeId::NULL),
            "object" => Some(TypeId::OBJECT),
            "bigint" => Some(TypeId::BIGINT),
            "symbol" => Some(TypeId::SYMBOL),
            _ => None,
        }
    }
}
