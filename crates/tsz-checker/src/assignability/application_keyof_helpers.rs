use crate::state::CheckerState;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn application_info_or_display_alias(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        crate::query_boundaries::common::application_info(self.ctx.types, type_id).or_else(|| {
            self.ctx.types.get_display_alias(type_id).and_then(|alias| {
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
            })
        })
    }

    fn application_info_preferring_display_alias(
        &self,
        type_id: TypeId,
    ) -> Option<(TypeId, Vec<TypeId>)> {
        self.ctx
            .types
            .get_display_alias(type_id)
            .and_then(|alias| {
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
            })
            .or_else(|| crate::query_boundaries::common::application_info(self.ctx.types, type_id))
    }

    pub(crate) fn is_promise_like_application_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
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
                ))
    }

    fn application_has_callable_then_member(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::property_access::resolve_property_access(
            self.ctx.types,
            type_id,
            "then",
        )
        .success_type()
        .and_then(|then_type| {
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, then_type)
        })
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
            .or_else(|| crate::query_boundaries::common::object_symbol(self.ctx.types, type_id))
            == Some(base_sym)
    }

    pub(crate) fn is_unknown_source_application_fallback(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(((source_base, source_args), (target_base, target_args))) =
            self.application_info_or_display_alias(source).zip(
                crate::query_boundaries::common::application_info(self.ctx.types, target),
            )
        else {
            return false;
        };

        source_base == target_base
            && source_args.len() == target_args.len()
            && !source_args.is_empty()
            && source_args.iter().all(|&arg| arg == TypeId::UNKNOWN)
            && target_args.contains(&TypeId::NEVER)
            && target_args.iter().any(|&arg| arg != TypeId::NEVER)
            && target_args.iter().all(|&arg| {
                matches!(arg, TypeId::UNKNOWN | TypeId::NEVER)
                    || crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, arg)
            })
            && self.is_promise_like_application_pair(source, target)
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
        let Some(((source_base, source_args), (target_base, target_args))) =
            self.application_info_or_display_alias(source).zip(
                crate::query_boundaries::common::application_info(self.ctx.types, target),
            )
        else {
            return false;
        };
        if source_base != target_base || source_args.len() != target_args.len() {
            return false;
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, source_base);
        let variances = def_id.and_then(|d| {
            if let Some(cached) =
                tsz_solver::QueryDatabase::get_type_param_variance(self.ctx.types, d)
            {
                return Some(cached);
            }
            if let Some(declared) = tsz_solver::TypeResolver::get_type_param_variance(&self.ctx, d)
            {
                self.ctx
                    .types
                    .insert_type_param_variance(d, declared.clone());
                return Some(declared);
            }
            let computed =
                tsz_solver::relations::variance::compute_type_param_variances_with_resolver(
                    self.ctx.types.as_type_database(),
                    &self.ctx,
                    d,
                );
            if let Some(ref variances) = computed {
                self.ctx
                    .types
                    .insert_type_param_variance(d, variances.clone());
            }
            computed
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
                crate::query_boundaries::common::type_param_info(self.ctx.types, target_arg)
                    .and_then(|param| param.constraint)
                    .is_some_and(|constraint| {
                        constraint == source_arg
                            || (self.is_assignable_to(source_arg, constraint)
                                && self.is_assignable_to(constraint, source_arg))
                            || crate::query_boundaries::common::type_param_info(
                                self.ctx.types,
                                constraint,
                            )
                            .zip(crate::query_boundaries::common::type_param_info(
                                self.ctx.types,
                                source_arg,
                            ))
                            .is_some_and(
                                |(constraint_param, source_param)| {
                                    constraint_param.name == source_param.name
                                },
                            )
                    })
            })
    }

    pub(crate) fn keyof_interface_augmentation_literals_cover_source(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{INTERFACE_DECLARATION, PROPERTY_SIGNATURE};

        let Some(source_members) =
            crate::query_boundaries::common::union_members(self.ctx.types, source)
        else {
            return false;
        };

        let target_keyof_inner =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, target);
        let source_keyof_inner = source_members.iter().find_map(|&member| {
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, member)
                .filter(|_| member == target || self.ctx.types.is_assignable_to(member, target))
        });
        let Some(inner) = target_keyof_inner.or(source_keyof_inner) else {
            return false;
        };
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, inner)
        else {
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
        if let Some(atom) = crate::query_boundaries::common::string_literal_value(
            self.ctx.types,
            resolved_target_keyof,
        ) {
            augmented_keys.insert(atom);
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, resolved_target_keyof)
        {
            for member in members {
                if let Some(atom) =
                    crate::query_boundaries::common::string_literal_value(self.ctx.types, member)
                {
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
            self.ctx.types.is_assignable_to(member, target)
                || crate::query_boundaries::common::keyof_inner_type(self.ctx.types, member)
                    .and_then(|member_inner| {
                        crate::query_boundaries::common::lazy_def_id(self.ctx.types, member_inner)
                    })
                    .is_some_and(|member_def_id| member_def_id == def_id)
                || (evaluated_member != member
                    && crate::query_boundaries::common::keyof_inner_type(
                        self.ctx.types,
                        evaluated_member,
                    )
                    .and_then(|member_inner| {
                        crate::query_boundaries::common::lazy_def_id(self.ctx.types, member_inner)
                    })
                    .is_some_and(|member_def_id| member_def_id == def_id))
                || crate::query_boundaries::common::string_literal_value(self.ctx.types, member)
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
