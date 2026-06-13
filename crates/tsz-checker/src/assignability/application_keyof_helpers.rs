use crate::query_boundaries::common::TypeResolver;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

use crate::query_boundaries::application_keyof as query;

impl<'a> CheckerState<'a> {
    /// Post-relation true-override gate group for the checker-final
    /// assignability funnel (issue #13243 step 4).
    ///
    /// When the Lawyer relation reports `true`, these checker-side
    /// compatibility gates can still reject the assignment:
    /// 1. same-alias application argument rejection (`Alias<A>` vs `Alias<B>`
    ///    whose unwitnessed arguments differ),
    /// 2. iterator-protocol display mismatches the solver relation cannot
    ///    observe,
    /// 3. namespace-module source property mismatches,
    /// 4. a string-literal source outside a resolvable `keyof` target's key
    ///    set.
    ///
    /// The combined verdict is cached by the funnel under the dedicated
    /// `RelationCacheKind::CheckerAssignable` key, so a cached answer is
    /// authoritative and these gates only run on cache misses.
    pub(crate) fn assignability_true_override_rejects(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.same_type_alias_application_args_reject(source, target)
            || self
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
            || self.namespace_source_has_matching_property_mismatch(source, target)
            || self.string_literal_source_outside_keyof_target(source, target)
    }

    /// A string-literal source is not assignable to a `keyof` target whose
    /// key set is concretely resolvable and does not contain the literal.
    /// An empty key set means the `keyof` operand could not be resolved
    /// (e.g. `ThisType`, `TypeParameter`, or `Application`); in that case
    /// the solver relation verdict stands.
    fn string_literal_source_outside_keyof_target(&self, source: TypeId, target: TypeId) -> bool {
        let Some(keyof_type) =
            crate::query_boundaries::assignability::get_keyof_type(self.ctx.types, target)
        else {
            return false;
        };
        let Some(source_atom) = crate::query_boundaries::assignability::get_string_literal_value(
            self.ctx.types,
            source,
        ) else {
            return false;
        };
        let source_str = self.ctx.types.resolve_atom(source_atom);
        let allowed_keys =
            crate::query_boundaries::assignability::get_allowed_keys(self.ctx.types, keyof_type);
        !allowed_keys.is_empty() && !allowed_keys.contains(&source_str)
    }
    pub(crate) fn application_info_or_display_alias(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        query::application_info(self.ctx.types, type_id).or_else(|| {
            self.ctx
                .types
                .get_display_alias(type_id)
                .and_then(|alias| query::application_info(self.ctx.types, alias))
        })
    }

    fn application_info_preferring_display_alias(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        self.ctx
            .types
            .get_display_alias(type_id)
            .and_then(|alias| query::application_info(self.ctx.types, alias))
            .or_else(|| query::application_info(self.ctx.types, type_id))
    }

    fn is_promise_like_application_pair(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some((source_base, _)) = self.application_info_or_display_alias(source) else {
            return false;
        };
        let Some((target_base, _)) = self.application_info_or_display_alias(target) else {
            return false;
        };
        self.application_bases_are_same_nominal_type(source_base, target_base)
            && (source_base == TypeId::PROMISE_BASE
                || crate::query_boundaries::flow_analysis::is_promise_like_type(
                    self.ctx.types,
                    source,
                )
                || crate::query_boundaries::flow_analysis::is_promise_like_type(
                    self.ctx.types,
                    target,
                )
                || self.application_has_callable_then_member(source)
                || self.application_has_callable_then_member(target)
                || self.application_base_declares_then_method(source_base))
    }

    fn application_has_callable_then_member(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::property_access::resolve_property_access(
            self.ctx.types,
            type_id,
            self.ctx.types.intern_string("then"),
        )
        .success_type()
        .and_then(|then_type| query::call_signatures_for_type(self.ctx.types, then_type))
        .is_some_and(|signatures| !signatures.is_empty())
    }

    fn application_base_declares_then_method(&self, base: TypeId) -> bool {
        let Some(sym_id) = self
            .application_base_symbol_id(base)
            .or_else(|| self.ctx.resolve_type_to_symbol_id(base))
        else {
            return false;
        };
        let Some(symbol) = self.get_symbol_globally(sym_id) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);

        symbol.declarations.iter().any(|&decl_idx| {
            if let Some(class) = arena.get_class_at(decl_idx)
                && class
                    .members
                    .nodes
                    .iter()
                    .any(|&member_idx| method_member_named(arena, member_idx, "then"))
            {
                return true;
            }

            arena.get_interface_at(decl_idx).is_some_and(|interface| {
                interface
                    .members
                    .nodes
                    .iter()
                    .any(|&member_idx| method_member_named(arena, member_idx, "then"))
            })
        })
    }

    fn type_has_same_nominal_origin_as_application_base(
        &self,
        type_id: TypeId,
        base: TypeId,
    ) -> bool {
        if let Some((nested_base, _)) = self.application_info_preferring_display_alias(type_id)
            && self.application_bases_are_same_nominal_type(nested_base, base)
        {
            return true;
        }

        let Some(base_sym) = self.application_base_symbol_id(base) else {
            return false;
        };
        self.ctx
            .resolve_type_to_symbol_id(type_id)
            .or_else(|| query::object_symbol(self.ctx.types, type_id))
            == Some(base_sym)
    }

    pub(crate) fn is_nested_same_wrapper_application_assignment(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            self.application_info_preferring_display_alias(source),
            self.application_info_preferring_display_alias(target),
        ) else {
            return false;
        };
        if self.application_bases_are_same_nominal_type(source_base, target_base)
            && source_args.len() == 1
            && target_args.len() == 1
            && (self.is_promise_like_application_pair(source, target)
                || source_base == TypeId::PROMISE_BASE
                || self.application_has_callable_then_member(source)
                || self.application_has_callable_then_member(target)
                || self.application_base_declares_then_method(source_base))
            && self.type_has_same_nominal_origin_as_application_base(source_args[0], source_base)
            && !self.type_has_same_nominal_origin_as_application_base(target_args[0], target_base)
        {
            return true;
        }

        false
    }

    pub(crate) fn same_base_application_to_constrained_type_param_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(((source_base, source_args), (target_base, target_args))) = self
            .application_info_or_display_alias(source)
            .zip(query::application_info(self.ctx.types, target))
        else {
            return false;
        };
        if source_base != target_base || source_args.len() != target_args.len() {
            return false;
        }
        if self.application_base_is_conditional_alias(source_base)
            || self.application_base_is_conditional_alias(target_base)
        {
            return false;
        }

        let def_id = query::lazy_def_id(self.ctx.types, source_base);
        let variances = def_id.and_then(|d| {
            TypeResolver::get_type_param_variance(&self.ctx, d).or_else(|| {
                crate::query_boundaries::variance::compute_type_param_variances_with_resolver_cached(
                    self.ctx.types.as_type_database(),
                    &self.ctx,
                    self.ctx.types,
                    d,
                )
            })
        });

        source_args
            .iter()
            .copied()
            .zip(target_args.iter().copied())
            .enumerate()
            .any(|(i, (source_arg, target_arg))| {
                if let Some(ref variances) = variances
                    && variances.get(i).is_some_and(|v| v.is_contravariant())
                {
                    return false;
                }
                query::type_param_info(self.ctx.types, target_arg)
                    .and_then(|param| param.constraint)
                    .is_some_and(|constraint| {
                        constraint == source_arg
                            || (self
                                .keyof_diagnostic_suppression_relation_outcome(
                                    source_arg, constraint,
                                )
                                .related
                                && self
                                    .keyof_diagnostic_suppression_relation_outcome(
                                        constraint, source_arg,
                                    )
                                    .related)
                            || query::type_param_info(self.ctx.types, constraint)
                                .zip(query::type_param_info(self.ctx.types, source_arg))
                                .is_some_and(|(constraint_param, source_param)| {
                                    constraint_param.name == source_param.name
                                })
                    })
            })
    }

    fn application_base_is_conditional_alias(&self, base: TypeId) -> bool {
        crate::query_boundaries::conditional_infer_alias::application_base_is_raw_conditional_alias(
            self.ctx.types.as_type_database(),
            &self.ctx,
            base,
        ) || crate::query_boundaries::conditional_infer_alias::application_base_uses_conditional_infer(
            self.ctx.types.as_type_database(),
            &self.ctx,
            base,
        )
    }

    pub(crate) fn keyof_interface_augmentation_literals_cover_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{INTERFACE_DECLARATION, PROPERTY_SIGNATURE};

        let Some(source_members) = query::union_members(self.ctx.types, source) else {
            return false;
        };

        let target_keyof_inner = query::keyof_inner_type(self.ctx.types, target);
        let source_keyof_inner = source_members.iter().find_map(|&member| {
            query::keyof_inner_type(self.ctx.types, member).filter(|_| {
                member == target
                    || self
                        .keyof_diagnostic_suppression_relation_outcome(member, target)
                        .related
            })
        });
        let Some(inner) = target_keyof_inner.or(source_keyof_inner) else {
            return false;
        };
        let Some(def_id) = query::lazy_def_id(self.ctx.types, inner) else {
            return false;
        };
        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return false;
        };
        let Some(interface_name) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
            .map(|symbol| symbol.escaped_name.as_str())
        else {
            return false;
        };

        let mut augmented_keys = rustc_hash::FxHashSet::default();
        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for entries in aug_index.values() {
                for (file_idx, aug) in entries {
                    if aug.name != interface_name {
                        continue;
                    }
                    let arena = aug
                        .arena
                        .as_deref()
                        .or_else(|| {
                            self.ctx.all_arenas.as_ref().and_then(|arenas| {
                                arenas.get(*file_idx).map(|arena| arena.as_ref())
                            })
                        })
                        .unwrap_or(self.ctx.arena);
                    let Some(node) = arena.get(aug.node) else {
                        continue;
                    };
                    if node.kind != INTERFACE_DECLARATION {
                        continue;
                    }
                    let Some(interface) = arena.get_interface(node) else {
                        continue;
                    };
                    for &member_idx in &interface.members.nodes {
                        let Some(member_node) = arena.get(member_idx) else {
                            continue;
                        };
                        if member_node.kind == PROPERTY_SIGNATURE
                            && let Some(sig) = arena.get_signature(member_node)
                            && let Some(name_node) = arena.get(sig.name)
                            && let Some(id) = arena.get_identifier(name_node)
                        {
                            augmented_keys.insert(self.ctx.types.intern_string(&id.escaped_text));
                        }
                    }
                }
            }
        }
        if augmented_keys.is_empty() {
            return false;
        }

        let resolved_target_keyof =
            crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                self.ctx.types,
                &self.ctx,
                target,
            );
        if let Some(atom) = query::string_literal_value(self.ctx.types, resolved_target_keyof) {
            augmented_keys.insert(atom);
        }
        if let Some(members) = query::union_members(self.ctx.types, resolved_target_keyof) {
            for member in members {
                if let Some(atom) = query::string_literal_value(self.ctx.types, member) {
                    augmented_keys.insert(atom);
                }
            }
        }

        source_members.iter().all(|&member| {
            let evaluated_member =
                crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                    self.ctx.types,
                    &self.ctx,
                    member,
                );
            self.keyof_diagnostic_suppression_relation_outcome(member, target)
                .related
                || query::keyof_inner_type(self.ctx.types, member)
                    .and_then(|member_inner| query::lazy_def_id(self.ctx.types, member_inner))
                    .is_some_and(|member_def_id| member_def_id == def_id)
                || (evaluated_member != member
                    && query::keyof_inner_type(self.ctx.types, evaluated_member)
                        .and_then(|member_inner| query::lazy_def_id(self.ctx.types, member_inner))
                        .is_some_and(|member_def_id| member_def_id == def_id))
                || query::string_literal_value(self.ctx.types, member)
                    .is_some_and(|atom| augmented_keys.contains(&atom))
        })
    }
}

fn method_member_named(arena: &NodeArena, member_idx: NodeIndex, name: &str) -> bool {
    arena
        .get_method_decl_at(member_idx)
        .and_then(|method| {
            crate::types_domain::queries::core::get_literal_property_name(arena, method.name)
        })
        .as_deref()
        == Some(name)
}
