//! Arena-aware text heritage helpers for cross-file interface lowering.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(crate) struct TextHeritageBasePlan {
    pub(crate) direct_interface_sym: Option<SymbolId>,
    pub(crate) args: Vec<TypeId>,
    pub(crate) fallback: TypeId,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn collect_text_based_interface_heritage_plans(
        &self,
        lowering: &tsz_lowering::TypeLowering<'_>,
        decls_with_arenas: &[(NodeIndex, &NodeArena)],
        sym_id: SymbolId,
    ) -> Vec<TextHeritageBasePlan> {
        let mut base_plans = Vec::new();
        for (decl_idx, decl_arena) in decls_with_arenas {
            let Some(node) = decl_arena.get(*decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                continue;
            };
            for clause_idx in heritage_clauses.nodes.iter().copied() {
                let Some(clause_node) = decl_arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = decl_arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for heritage_type_idx in heritage.types.nodes.iter().copied() {
                    if let Some(expr_type_args) = decl_arena
                        .get(heritage_type_idx)
                        .and_then(|node| decl_arena.get_expr_type_args(node))
                    {
                        let owner_binder = self
                            .ctx
                            .get_binder_for_arena(decl_arena)
                            .unwrap_or(self.ctx.binder);
                        let direct_interface_sym = decl_arena
                            .get_identifier_text(expr_type_args.expression)
                            .and_then(|name| owner_binder.file_locals.get(name))
                            .filter(|base_sym_id| *base_sym_id != sym_id)
                            .filter(|base_sym_id| {
                                owner_binder
                                    .get_symbol(*base_sym_id)
                                    .is_some_and(|base_symbol| {
                                        base_symbol.has_any_flags(symbol_flags::INTERFACE)
                                    })
                            });
                        let args = {
                            let decl_lowering = lowering.with_arena(decl_arena);
                            expr_type_args
                                .type_arguments
                                .as_ref()
                                .map(|type_arguments| {
                                    type_arguments
                                        .nodes
                                        .iter()
                                        .map(|&arg_idx| decl_lowering.lower_type(arg_idx))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        };
                        let fallback = if args.is_empty() {
                            let decl_lowering = lowering.with_arena(decl_arena);
                            decl_lowering.lower_type(expr_type_args.expression)
                        } else {
                            let decl_lowering = lowering.with_arena(decl_arena);
                            let base = decl_lowering.lower_type(expr_type_args.expression);
                            self.ctx.types.application(base, args.clone())
                        };
                        base_plans.push(TextHeritageBasePlan {
                            direct_interface_sym,
                            args,
                            fallback,
                        });
                        continue;
                    }

                    let decl_lowering = lowering.with_arena(decl_arena);
                    let fallback = decl_lowering.lower_type(heritage_type_idx);
                    if !matches!(fallback, TypeId::ERROR | TypeId::UNKNOWN | TypeId::ANY) {
                        base_plans.push(TextHeritageBasePlan {
                            direct_interface_sym: None,
                            args: Vec::new(),
                            fallback,
                        });
                    }
                }
            }
        }
        base_plans
    }

    pub(crate) fn register_text_merged_interface_body_if_inert(
        &mut self,
        def_id: tsz_solver::DefId,
        merged: TypeId,
        interface_type: TypeId,
        reg_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if merged != interface_type
            && self.published_body_covers_local_members(merged, interface_type)
            && !tsz_solver::type_queries::contains_callable_or_conditional(
                self.ctx.types.as_type_database(),
                merged,
            )
        {
            self.ctx
                .register_def_auto_params_in_envs(def_id, merged, reg_params);
            self.ctx.clear_type_evaluation_caches_for_def(def_id);
            self.ctx
                .types
                .invalidate_application_eval_cache_for_def(def_id);
        }
    }

    pub(crate) fn merge_text_based_interface_heritage_plans(
        &mut self,
        mut merged: TypeId,
        plans: Vec<TextHeritageBasePlan>,
    ) -> TypeId {
        for plan in plans {
            let base_type = if let Some(base_sym_id) = plan.direct_interface_sym {
                let (body, params) = self.type_reference_symbol_type_with_params(base_sym_id);
                let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                    self.ctx.types,
                    &params,
                    &plan.args,
                );
                crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    body,
                    &substitution,
                )
            } else {
                plan.fallback
            };
            if !matches!(base_type, TypeId::ERROR | TypeId::UNKNOWN | TypeId::ANY) {
                merged = self.merge_interface_types_heritage(merged, base_type);
            }
        }
        merged
    }
}
