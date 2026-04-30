//! Mapped-type helpers used by object literal excess property checking.

use crate::state::CheckerState;
use std::collections::HashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn type_contains_invalid_mapped_key_type(&self, type_id: TypeId) -> bool {
        let mut visited = HashSet::new();
        self.type_contains_invalid_mapped_key_type_inner(type_id, &mut visited)
    }

    fn type_contains_invalid_mapped_key_type_inner(
        &self,
        type_id: TypeId,
        visited: &mut HashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }
        if let Some(mapped) =
            crate::query_boundaries::common::mapped_type_info(self.ctx.types, type_id)
        {
            return crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                mapped.constraint,
            )
            .is_some()
                || !crate::query_boundaries::common::is_valid_mapped_type_key_type(
                    self.ctx.types,
                    mapped.constraint,
                );
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            && members
                .into_iter()
                .any(|member| self.type_contains_invalid_mapped_key_type_inner(member, visited))
        {
            return true;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
            && members
                .into_iter()
                .any(|member| self.type_contains_invalid_mapped_key_type_inner(member, visited))
        {
            return true;
        }
        if let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)
        {
            if self.type_contains_invalid_mapped_key_type_inner(base, visited) {
                return true;
            }
            return args
                .into_iter()
                .any(|arg| self.type_contains_invalid_mapped_key_type_inner(arg, visited));
        }
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
        {
            return shape.properties.iter().any(|prop| {
                self.type_contains_invalid_mapped_key_type_inner(prop.type_id, visited)
                    || (prop.write_type != TypeId::NONE
                        && self
                            .type_contains_invalid_mapped_key_type_inner(prop.write_type, visited))
            });
        }
        false
    }

    pub(super) fn target_is_mapped_or_mapped_application(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::mapped_type_info(self.ctx.types, type_id).is_some_and(
            |mapped| {
                mapped.name_type.is_none()
                    && crate::query_boundaries::checkers::generic::keyof_operand(
                        self.ctx.types,
                        mapped.constraint,
                    )
                    .is_some()
            },
        ) || crate::query_boundaries::common::application_info(self.ctx.types, type_id).is_some_and(
            |(_, args)| {
                args.into_iter().any(|arg| {
                    crate::query_boundaries::common::mapped_type_info(self.ctx.types, arg)
                        .is_some_and(|mapped| {
                            mapped.name_type.is_none()
                                && crate::query_boundaries::checkers::generic::keyof_operand(
                                    self.ctx.types,
                                    mapped.constraint,
                                )
                                .is_some()
                        })
                })
            },
        )
    }

    pub(super) fn check_object_literal_named_property_value(
        &mut self,
        obj_literal_idx: NodeIndex,
        prop_name: Atom,
        source_prop_type: TypeId,
        target_prop_type: TypeId,
    ) -> bool {
        if matches!(
            target_prop_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
        ) {
            return false;
        }

        let source_type_for_check = self
            .object_literal_property_name_and_value(obj_literal_idx, prop_name)
            .map(|(_, value_idx)| {
                self.get_type_of_node_with_request(value_idx, &crate::context::TypingRequest::NONE)
            })
            .filter(|&type_id| !matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN))
            .unwrap_or(source_prop_type);

        if self.is_assignable_to(source_type_for_check, target_prop_type) {
            return false;
        }

        let target_prop_type_for_message =
            self.object_literal_property_value_diagnostic_target_type(target_prop_type);
        let report_idx = self
            .find_object_literal_property_element(obj_literal_idx, prop_name)
            .unwrap_or(obj_literal_idx);
        self.error_type_not_assignable_at_with_anchor(
            source_type_for_check,
            target_prop_type_for_message,
            report_idx,
        );
        true
    }

    fn object_literal_property_value_diagnostic_target_type(&mut self, target: TypeId) -> TypeId {
        let evaluated = self.evaluate_type_for_assignability(target);
        let without_undefined =
            crate::query_boundaries::common::union_members(self.ctx.types, evaluated).and_then(
                |members| {
                    let original_len = members.len();
                    let kept = members
                        .into_iter()
                        .filter(|&member| member != TypeId::UNDEFINED)
                        .collect::<Vec<_>>();
                    (!kept.is_empty() && kept.len() < original_len)
                        .then(|| tsz_solver::utils::union_or_single(self.ctx.types, kept))
                },
            );
        without_undefined.unwrap_or(evaluated)
    }

    fn contextual_named_property_type_candidates(
        &mut self,
        target: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        let evaluated = self.evaluate_type_with_env(target);
        let contextual = self.evaluate_contextual_type(target);
        let resolved = self.resolve_type_for_property_access(target);
        let resolved_evaluated = self.resolve_type_for_property_access(evaluated);

        [target, evaluated, contextual, resolved, resolved_evaluated]
            .into_iter()
            .find_map(|candidate| {
                let candidate_type = self
                    .contextual_object_literal_property_type(candidate, prop_name)
                    .or_else(
                        || match self.resolve_property_access_with_env(candidate, prop_name) {
                            tsz_solver::operations::property::PropertyAccessResult::Success {
                                type_id,
                                from_index_signature: false,
                                ..
                            } => Some(type_id),
                            _ => None,
                        },
                    )
                    .or_else(|| {
                        crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            candidate,
                        )
                        .and_then(|shape| {
                            shape
                                .properties
                                .iter()
                                .find(|prop| {
                                    self.ctx.types.resolve_atom(prop.name).as_str() == prop_name
                                })
                                .map(|prop| prop.type_id)
                        })
                    })
                    .or_else(|| {
                        let (_, args) = crate::query_boundaries::common::application_info(
                            self.ctx.types,
                            candidate,
                        )?;
                        let object_arg = args.first().copied()?;
                        crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            object_arg,
                        )
                        .and_then(|shape| {
                            shape
                                .properties
                                .iter()
                                .find(|prop| {
                                    self.ctx.types.resolve_atom(prop.name).as_str() == prop_name
                                })
                                .map(|prop| prop.type_id)
                        })
                    });
                candidate_type.filter(|&type_id| {
                    !matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
                })
            })
    }

    fn partial_annotation_property_type(
        &mut self,
        obj_literal_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<TypeId> {
        let arg_idx = self.partial_annotation_arg_node(obj_literal_idx)?;
        let arg_type = self.get_type_from_type_node(arg_idx);
        self.contextual_named_property_type_candidates(arg_type, prop_name)
    }

    fn partial_annotation_arg_node(&self, obj_literal_idx: NodeIndex) -> Option<NodeIndex> {
        let var_decl_idx = self.ctx.arena.parent_of(obj_literal_idx)?;
        if var_decl_idx == NodeIndex::NONE {
            return None;
        }
        let var_decl_node = self.ctx.arena.get(var_decl_idx)?;
        if var_decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(var_decl_node)?;
        let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(type_node)?;
        if self
            .get_identifier_text_from_idx(type_ref.type_name)
            .as_deref()
            != Some("Partial")
        {
            return None;
        }
        type_ref.type_arguments.as_ref()?.nodes.first().copied()
    }

    pub(super) fn check_object_literal_named_property_values_against_target(
        &mut self,
        obj_literal_idx: NodeIndex,
        target: TypeId,
    ) -> bool {
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return false;
        };
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return false;
        };

        let target_is_mapped = [
            target,
            self.evaluate_type_with_env(target),
            self.resolve_type_for_property_access(target),
        ]
        .into_iter()
        .any(|candidate| self.target_is_mapped_or_mapped_application(candidate));
        let has_partial_annotation = self.partial_annotation_arg_node(obj_literal_idx).is_some();
        if !target_is_mapped && !has_partial_annotation {
            return false;
        }

        let mut emitted = false;
        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };
            let Some(prop_name) = self.get_property_name_resolved(prop.name) else {
                continue;
            };

            let target_prop_type = self
                .contextual_named_property_type_candidates(target, prop_name.as_ref())
                .or_else(|| self.partial_annotation_property_type(obj_literal_idx, &prop_name));
            let Some(target_prop_type) = target_prop_type else {
                continue;
            };
            if matches!(
                target_prop_type,
                TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN
            ) {
                continue;
            }

            let source_type = self.get_type_of_node_with_request(
                prop.initializer,
                &crate::context::TypingRequest::NONE,
            );
            let target_prop_type_for_check = self.evaluate_type_for_assignability(target_prop_type);
            if matches!(source_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
                || self.is_assignable_to(source_type, target_prop_type_for_check)
            {
                continue;
            }

            let target_prop_type_for_message =
                self.object_literal_property_value_diagnostic_target_type(target_prop_type);
            self.error_type_not_assignable_at_with_anchor(
                source_type,
                target_prop_type_for_message,
                prop.name,
            );
            emitted = true;
        }
        emitted
    }

    pub(super) fn nested_property_target_type(
        &mut self,
        owner_type: TypeId,
        prop_name: Atom,
        fallback: TypeId,
    ) -> TypeId {
        let prop_name_str = self.ctx.types.resolve_atom(prop_name);

        if let Some(type_id) =
            self.contextual_object_literal_property_type(owner_type, prop_name_str.as_ref())
        {
            return type_id;
        }

        if let Some(type_id) = self
            .ctx
            .types
            .contextual_property_type(owner_type, prop_name_str.as_ref())
        {
            return type_id;
        }

        let resolved_owner = self.resolve_type_for_property_access(owner_type);
        if resolved_owner != owner_type
            && let Some(type_id) = self
                .ctx
                .types
                .contextual_property_type(resolved_owner, prop_name_str.as_ref())
        {
            return type_id;
        }

        match self.resolve_property_access_with_env(owner_type, &prop_name_str) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } => {
                type_id
            }
            _ => fallback,
        }
    }
}
