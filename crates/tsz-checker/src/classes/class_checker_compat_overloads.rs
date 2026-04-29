use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext::METHOD_SIGNATURE;
use tsz_solver::TypeId;

fn overload_method_wrapper_value_type(
    types: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(types, type_id)
        && shape.properties.len() == 1
        && shape.properties[0].is_method
    {
        return Some(shape.properties[0].type_id);
    }
    None
}

impl<'a> CheckerState<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_interface_overload_coverage(
        &mut self,
        iface_name: NodeIndex,
        derived_name: &str,
        base_name: &str,
        base_iface_indices: &[NodeIndex],
        derived_member_names: &rustc_hash::FxHashSet<String>,
        derived_members: &[(String, TypeId, NodeIndex, u16, bool)],
        substitution: &TypeSubstitution,
    ) {
        let base_method_overloads: Vec<(String, Vec<TypeId>)>;
        {
            let mut by_name: rustc_hash::FxHashMap<String, Vec<TypeId>> =
                rustc_hash::FxHashMap::default();
            for &base_iface_idx in base_iface_indices {
                if let Some(base_node) = self.ctx.arena.get(base_iface_idx)
                    && let Some(base_iface) = self.ctx.arena.get_interface(base_node)
                {
                    for &base_member_idx in &base_iface.members.nodes {
                        let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                            continue;
                        };
                        if base_member_node.kind != METHOD_SIGNATURE {
                            continue;
                        }
                        let Some(sig) = self.ctx.arena.get_signature(base_member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };
                        if !derived_member_names.contains(&name) {
                            continue;
                        }
                        let base_type = instantiate_type(
                            self.ctx.types,
                            self.get_type_of_interface_member(base_member_idx),
                            substitution,
                        );
                        by_name.entry(name).or_default().push(base_type);
                    }
                }
            }
            base_method_overloads = by_name.into_iter().filter(|(_, v)| v.len() > 1).collect();
        }

        let mut derived_method_overloads: rustc_hash::FxHashMap<String, Vec<(TypeId, NodeIndex)>> =
            rustc_hash::FxHashMap::default();
        for (name, type_id, idx, kind, _) in derived_members {
            if *kind == METHOD_SIGNATURE {
                derived_method_overloads
                    .entry(name.clone())
                    .or_default()
                    .push((*type_id, *idx));
            }
        }

        let signature_has_literal_parameter = |type_id: TypeId| -> bool {
            let has_literal_param = |params: &[crate::query_boundaries::common::ParamInfo]| {
                params.iter().any(|param| {
                    crate::query_boundaries::common::is_literal_type(self.ctx.types, param.type_id)
                })
            };

            if let Some(signatures) =
                crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, type_id)
            {
                return signatures
                    .iter()
                    .any(|signature| has_literal_param(&signature.params));
            }

            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            {
                return has_literal_param(&shape.params);
            }

            if let Some(shape) =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            {
                return shape
                    .call_signatures
                    .iter()
                    .any(|signature| has_literal_param(&signature.params));
            }

            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                && shape.properties.len() == 1
                && shape.properties[0].is_method
            {
                let method_type = shape.properties[0].type_id;
                if let Some(signatures) = crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    method_type,
                ) {
                    return signatures
                        .iter()
                        .any(|signature| has_literal_param(&signature.params));
                }
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    method_type,
                ) {
                    return has_literal_param(&shape.params);
                }
                if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    method_type,
                ) {
                    return shape
                        .call_signatures
                        .iter()
                        .any(|signature| has_literal_param(&signature.params));
                }
            }

            false
        };

        let select_implementation_signature = |signatures: &[TypeId]| -> Option<TypeId> {
            if signatures.is_empty() {
                return None;
            }

            let mut last_non_specialized: Option<TypeId> = None;
            for &signature in signatures {
                if !signature_has_literal_parameter(signature) {
                    last_non_specialized = Some(signature);
                }
            }

            last_non_specialized.or_else(|| signatures.last().copied())
        };

        let has_non_specialized_signature = |signatures: &[TypeId]| -> bool {
            signatures
                .iter()
                .any(|&signature| !signature_has_literal_parameter(signature))
        };

        let select_implementation_signature_with_node =
            |signatures: &[(TypeId, NodeIndex)]| -> Option<(TypeId, NodeIndex)> {
                if signatures.is_empty() {
                    return None;
                }

                let mut last_non_specialized: Option<(TypeId, NodeIndex)> = None;
                for &(signature, node_idx) in signatures {
                    if !signature_has_literal_parameter(signature) {
                        last_non_specialized = Some((signature, node_idx));
                    }
                }

                last_non_specialized.or_else(|| signatures.last().copied())
            };

        let has_non_specialized_signature_with_node = |signatures: &[(TypeId, NodeIndex)]| -> bool {
            signatures
                .iter()
                .any(|&(signature, _)| !signature_has_literal_parameter(signature))
        };
        let signature_contains_error = |signature: TypeId| {
            crate::query_boundaries::common::contains_error_type_in_args(self.ctx.types, signature)
        };

        // For overloaded method inheritance, tsc compatibility hinges on the trailing
        // implementation signature.
        'overload_check: for (method_name, base_sigs) in &base_method_overloads {
            let Some(derived_sigs) = derived_method_overloads.get(method_name) else {
                continue;
            };
            if base_sigs.iter().copied().any(signature_contains_error)
                || derived_sigs
                    .iter()
                    .any(|(signature, _)| signature_contains_error(*signature))
            {
                continue;
            }
            if has_non_specialized_signature(base_sigs)
                && !has_non_specialized_signature_with_node(derived_sigs)
            {
                self.error_at_node(
                    iface_name,
                    &format!(
                        "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                    ),
                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                );
                break 'overload_check;
            }
            let Some(base_trailing_sig) = select_implementation_signature(base_sigs) else {
                continue;
            };
            let Some((derived_trailing_sig, derived_trailing_idx)) =
                select_implementation_signature_with_node(derived_sigs)
            else {
                continue;
            };

            let derived_method_value =
                overload_method_wrapper_value_type(self.ctx.types, derived_trailing_sig);
            let base_method_value =
                overload_method_wrapper_value_type(self.ctx.types, base_trailing_sig);
            let (derived_compare_sig, base_compare_sig) =
                match (derived_method_value, base_method_value) {
                    (Some(derived), None) => (derived, base_trailing_sig),
                    (None, Some(base)) => (derived_trailing_sig, base),
                    _ => (derived_trailing_sig, base_trailing_sig),
                };

            if !self.is_assignable_to_no_erase_generics(derived_compare_sig, base_compare_sig)
                && !self.should_suppress_assignability_for_parse_recovery(
                    derived_trailing_idx,
                    derived_trailing_idx,
                )
            {
                self.error_at_node(
                    iface_name,
                    &format!(
                        "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                    ),
                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                );
                break 'overload_check;
            }
        }
    }
}
