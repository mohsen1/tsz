use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    fn application_bases_are_same_nominal_type(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }

        if let (Some(left), Some(right)) = (
            self.application_base_symbol_id(left),
            self.application_base_symbol_id(right),
        ) {
            return left == right;
        }

        false
    }

    fn application_base_symbol_id(&self, base: TypeId) -> Option<tsz_binder::SymbolId> {
        if let Some(def_id) = query::lazy_def_id(self.ctx.types, base) {
            return self.ctx.def_to_symbol_id(def_id);
        }
        crate::query_boundaries::common::type_query_symbol(self.ctx.types, base)
            .map(|sym_ref| tsz_binder::SymbolId(sym_ref.0))
            .or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(base)
                    .and_then(|alias| query::get_application_info(self.ctx.types, alias))
                    .and_then(|(alias_base, _)| self.application_base_symbol_id(alias_base))
            })
    }

    pub(crate) fn is_same_class_static_method_new_result(
        &self,
        new_expr_idx: NodeIndex,
        callee_expr: NodeIndex,
        contextual_type: TypeId,
    ) -> bool {
        let Some(enclosing_class) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        if !self.is_in_static_class_method_context(new_expr_idx) {
            return false;
        }

        let Some(enclosing_sym) = self
            .ctx
            .binder
            .node_symbols
            .get(&enclosing_class.class_idx.0)
            .copied()
        else {
            return false;
        };

        let Some(target_sym) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, callee_expr)
            .or_else(|| self.ctx.binder.get_node_symbol(callee_expr))
            .or_else(|| self.resolve_qualified_symbol(callee_expr))
        else {
            return false;
        };

        if target_sym != enclosing_sym {
            return false;
        }

        query::get_application_info(self.ctx.types, contextual_type)
            .and_then(|(ctx_base, _)| self.application_base_symbol_id(ctx_base))
            == Some(enclosing_sym)
    }

    pub(crate) fn contextual_application_directly_supplies_type_parameters(
        &self,
        result_type: TypeId,
        contextual_type: TypeId,
    ) -> bool {
        let result_app = query::get_application_info(self.ctx.types, result_type).or_else(|| {
            self.ctx
                .types
                .get_display_alias(result_type)
                .and_then(|alias| query::get_application_info(self.ctx.types, alias))
        });
        let contextual_app = query::get_application_info(self.ctx.types, contextual_type);

        let (Some((result_base, result_args)), Some((ctx_base, ctx_args))) =
            (result_app, contextual_app)
        else {
            return false;
        };

        self.application_bases_are_same_nominal_type(result_base, ctx_base)
            && result_args.len() == ctx_args.len()
            && !result_args.is_empty()
            && result_args.iter().all(|&arg| arg == TypeId::UNKNOWN)
            && ctx_args
                .iter()
                .any(|&arg| query::type_parameter_info(self.ctx.types, arg).is_some())
            && ctx_args.iter().all(|&arg| {
                arg == TypeId::UNKNOWN
                    || arg == TypeId::ERROR
                    || query::type_parameter_info(self.ctx.types, arg).is_some()
            })
    }

    pub(crate) fn contextual_application_recovers_unresolved_constructor_result(
        &self,
        callee_expr: NodeIndex,
        result_type: TypeId,
        contextual_type: TypeId,
        constructor_type_params: &[TypeParamInfo],
    ) -> bool {
        if constructor_type_params.is_empty() {
            return false;
        }

        let constructor_param_names: FxHashSet<_> = constructor_type_params
            .iter()
            .map(|type_param| type_param.name)
            .collect();

        let context_supplies_specific_args = |ctx_args: &[TypeId]| {
            ctx_args
                .iter()
                .any(|&arg| arg != TypeId::ANY && arg != TypeId::UNKNOWN && arg != TypeId::ERROR)
        };

        let result_matches_context = self
            .application_infos_for_type(result_type)
            .into_iter()
            .any(|(result_base, result_args)| {
                !result_args.is_empty()
                    && result_args.iter().all(|&arg| {
                        arg == TypeId::UNKNOWN
                            || query::type_parameter_info(self.ctx.types, arg)
                                .is_some_and(|info| constructor_param_names.contains(&info.name))
                    })
                    && self
                        .application_infos_for_type(contextual_type)
                        .into_iter()
                        .any(|(ctx_base, ctx_args)| {
                            self.application_bases_are_same_nominal_type(result_base, ctx_base)
                                && result_args.len() == ctx_args.len()
                                && context_supplies_specific_args(&ctx_args)
                        })
            });
        if result_matches_context {
            return true;
        }

        let Some(target_sym) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, callee_expr)
            .or_else(|| self.ctx.binder.get_node_symbol(callee_expr))
            .or_else(|| self.resolve_qualified_symbol(callee_expr))
        else {
            return false;
        };

        self.application_infos_for_type(contextual_type)
            .into_iter()
            .any(|(ctx_base, ctx_args)| {
                self.contextual_application_base_matches_target(
                    target_sym,
                    ctx_base,
                    &ctx_args,
                    Some(constructor_type_params.len()),
                )
            })
    }

    pub(crate) fn contextual_application_matches_new_target(
        &self,
        callee_expr: NodeIndex,
        contextual_type: TypeId,
    ) -> bool {
        let Some(target_sym) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, callee_expr)
            .or_else(|| self.ctx.binder.get_node_symbol(callee_expr))
            .or_else(|| self.resolve_qualified_symbol(callee_expr))
        else {
            return false;
        };

        self.application_infos_for_type(contextual_type)
            .into_iter()
            .any(|(ctx_base, ctx_args)| {
                self.contextual_application_base_matches_target(
                    target_sym, ctx_base, &ctx_args, None,
                )
            })
    }

    fn contextual_application_base_matches_target(
        &self,
        target_sym: tsz_binder::SymbolId,
        ctx_base: TypeId,
        ctx_args: &[TypeId],
        max_arg_count: Option<usize>,
    ) -> bool {
        self.application_base_symbol_id(ctx_base) == Some(target_sym)
            && max_arg_count.is_none_or(|max| ctx_args.len() <= max)
            && !ctx_args.is_empty()
            && ctx_args
                .iter()
                .any(|&arg| arg != TypeId::ANY && arg != TypeId::UNKNOWN && arg != TypeId::ERROR)
    }

    pub(crate) fn contextual_application_recovers_unknown_result(
        &self,
        result_type: TypeId,
        contextual_type: TypeId,
    ) -> bool {
        self.contextual_application_recovers_unresolved_result_by_base(
            result_type,
            contextual_type,
            |arg| arg == TypeId::UNKNOWN,
        )
    }

    pub(crate) fn contextual_application_recovers_type_param_result(
        &self,
        result_type: TypeId,
        contextual_type: TypeId,
    ) -> bool {
        self.contextual_application_recovers_unresolved_result_by_base(
            result_type,
            contextual_type,
            |arg| {
                arg == TypeId::UNKNOWN
                    || query::type_parameter_info(self.ctx.types, arg).is_some()
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        arg,
                    )
            },
        )
    }

    fn contextual_application_recovers_unresolved_result_by_base(
        &self,
        result_type: TypeId,
        contextual_type: TypeId,
        unresolved: impl Fn(TypeId) -> bool,
    ) -> bool {
        self.application_infos_for_type(result_type)
            .into_iter()
            .any(|(result_base, result_args)| {
                !result_args.is_empty()
                    && result_args.iter().all(|&arg| unresolved(arg))
                    && self
                        .application_infos_for_type(contextual_type)
                        .into_iter()
                        .any(|(ctx_base, ctx_args)| {
                            self.application_bases_are_same_nominal_type(result_base, ctx_base)
                                && result_args.len() == ctx_args.len()
                                && ctx_args.iter().any(|&arg| {
                                    arg != TypeId::ANY
                                        && arg != TypeId::UNKNOWN
                                        && arg != TypeId::ERROR
                                })
                        })
            })
    }

    fn application_infos_for_type(&self, type_id: TypeId) -> Vec<(TypeId, Vec<TypeId>)> {
        let mut applications = Vec::with_capacity(2);
        if let Some(app) = query::get_application_info(self.ctx.types, type_id) {
            applications.push(app);
        }
        if let Some(alias_app) = self
            .ctx
            .types
            .get_display_alias(type_id)
            .and_then(|alias| query::get_application_info(self.ctx.types, alias))
            && !applications.contains(&alias_app)
        {
            applications.push(alias_app);
        }
        applications
    }

    fn is_in_static_class_method_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            CLASS_DECLARATION, CLASS_EXPRESSION, METHOD_DECLARATION,
        };

        let mut current = idx;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > 256 {
                return false;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == METHOD_DECLARATION => {
                    return self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|method| self.has_static_modifier(&method.modifiers));
                }
                k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => return false,
                _ => {}
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }

        false
    }
}
