use crate::query_boundaries::common::{FunctionShape, TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use std::fmt::Write;
use tsz_solver::{CallSignature, ParamInfo, TypeId, TypeParamInfo};

use super::SelectedTypePredicate;

impl<'a> CheckerState<'a> {
    pub(crate) fn return_context_refinement_for_arg_inference(
        &mut self,
        existing: TypeId,
        contextual: TypeId,
    ) -> Option<TypeId> {
        self.return_context_refinement_for_arg_inference_inner(existing, contextual, 0)
    }

    fn return_context_refinement_for_arg_inference_inner(
        &mut self,
        existing: TypeId,
        contextual: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        if depth > 8 {
            return None;
        }

        let contextual = self.evaluate_awaited_application_for_assignability(contextual);
        let readonly_stripped_contextual =
            crate::query_boundaries::common::unwrap_readonly(self.ctx.types, contextual);

        if existing.is_any_unknown_or_error()
            || self.inference_type_is_anyish(existing)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, existing)
            || crate::query_boundaries::common::contains_infer_types(self.ctx.types, existing)
        {
            return Some(contextual);
        }

        if let Some(refined) =
            self.return_context_preserve_awaited_wrapper_refinement(existing, contextual, depth + 1)
        {
            return Some(refined);
        }

        if crate::query_boundaries::assignability::is_fresh_subtype_of(
            self.ctx.types,
            contextual,
            existing,
        ) {
            return Some(contextual);
        }

        if readonly_stripped_contextual != contextual
            && crate::query_boundaries::assignability::is_fresh_subtype_of(
                self.ctx.types,
                readonly_stripped_contextual,
                existing,
            )
        {
            return Some(readonly_stripped_contextual);
        }

        self.return_context_readonly_container_refines_arg_inference(existing, contextual, 0)
            .then_some(contextual)
    }

    fn return_context_preserve_awaited_wrapper_refinement(
        &mut self,
        existing: TypeId,
        contextual: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        if depth > 8 {
            return None;
        }

        if let (Some(existing_elem), Some(contextual_elem)) = (
            crate::query_boundaries::common::array_element_type(self.ctx.types, existing),
            crate::query_boundaries::common::array_element_type(self.ctx.types, contextual),
        ) && let Some(refined_elem) = self.return_context_refinement_for_arg_inference_inner(
            existing_elem,
            contextual_elem,
            depth + 1,
        ) && refined_elem != existing_elem
        {
            return Some(self.ctx.types.factory().array(refined_elem));
        }

        if let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, existing)
            && args.len() == 1
            && self.return_context_application_base_has_name(base, &["Promise", "PromiseLike"])
            && let Some(refined_arg) = self.return_context_refinement_for_arg_inference_inner(
                args[0],
                contextual,
                depth + 1,
            )
            && refined_arg != args[0]
        {
            return Some(
                self.ctx
                    .types
                    .factory()
                    .application(base, vec![refined_arg]),
            );
        }

        None
    }

    fn return_context_readonly_container_refines_arg_inference(
        &mut self,
        existing: TypeId,
        contextual: TypeId,
        depth: u8,
    ) -> bool {
        if depth > 8 {
            return false;
        }

        let existing = crate::query_boundaries::common::unwrap_readonly(self.ctx.types, existing);
        let contextual =
            crate::query_boundaries::common::unwrap_readonly(self.ctx.types, contextual);
        if crate::query_boundaries::assignability::is_fresh_subtype_of(
            self.ctx.types,
            contextual,
            existing,
        ) {
            return true;
        }

        let existing_elem =
            crate::query_boundaries::common::array_element_type(self.ctx.types, existing);
        let contextual_elem =
            crate::query_boundaries::common::array_element_type(self.ctx.types, contextual);
        if let (Some(existing_elem), Some(contextual_elem)) = (existing_elem, contextual_elem) {
            return self.return_context_readonly_container_refines_arg_inference(
                existing_elem,
                contextual_elem,
                depth + 1,
            );
        }

        if let (Some(existing_elems), Some(contextual_elems)) = (
            crate::query_boundaries::common::tuple_elements(self.ctx.types, existing),
            crate::query_boundaries::common::tuple_elements(self.ctx.types, contextual),
        ) && existing_elems.len() == contextual_elems.len()
        {
            return existing_elems.iter().zip(contextual_elems.iter()).all(
                |(existing_elem, contextual_elem)| {
                    self.return_context_readonly_container_refines_arg_inference(
                        existing_elem.type_id,
                        contextual_elem.type_id,
                        depth + 1,
                    )
                },
            );
        }

        false
    }

    pub(super) fn merge_return_context_substitution(
        &mut self,
        combined_substitution: &mut TypeSubstitution,
        type_params: &[TypeParamInfo],
        return_substitution: &TypeSubstitution,
    ) {
        for tp in type_params {
            let Some(contextual) = return_substitution.get(tp.name) else {
                continue;
            };
            let refined = match combined_substitution.get(tp.name) {
                None => Some(contextual),
                Some(existing) if existing == contextual => None,
                Some(existing) => {
                    self.return_context_refinement_for_arg_inference(existing, contextual)
                }
            };
            if let Some(refined) = refined {
                combined_substitution.insert(tp.name, refined);
            }
        }
    }

    pub(super) fn overload_signature_for_inference(
        &mut self,
        sig: &CallSignature,
        signature_index: usize,
        arg_types: &[TypeId],
        contextual_type: Option<TypeId>,
    ) -> CallSignature {
        if sig.type_params.is_empty() {
            return sig.clone();
        }

        let collides = sig.type_params.iter().any(|tp| {
            arg_types
                .iter()
                .copied()
                .chain(contextual_type)
                .flat_map(|ty| {
                    crate::query_boundaries::common::collect_referenced_types(self.ctx.types, ty)
                })
                .any(|referenced| {
                    crate::query_boundaries::common::type_param_info(self.ctx.types, referenced)
                        .is_some_and(|referenced_tp| referenced_tp.name == tp.name)
                })
        });
        if !collides {
            return sig.clone();
        }

        let mut substitution = TypeSubstitution::new();
        let mut renamed_type_params = Vec::with_capacity(sig.type_params.len());
        let mut name_buf = String::with_capacity(48);
        for (index, tp) in sig.type_params.iter().enumerate() {
            name_buf.clear();
            write!(name_buf, "__overload_sig_{signature_index}_tp_{index}")
                .expect("write to String is infallible");
            let fresh_name = self.ctx.types.intern_string(&name_buf);
            let fresh_type = self.ctx.types.factory().type_param(TypeParamInfo {
                name: fresh_name,
                constraint: None,
                default: None,
                is_const: tp.is_const,
            });
            substitution.insert(tp.name, fresh_type);
            renamed_type_params.push(TypeParamInfo {
                name: fresh_name,
                constraint: tp
                    .constraint
                    .map(|constraint| instantiate_type(self.ctx.types, constraint, &substitution)),
                default: tp
                    .default
                    .map(|default| instantiate_type(self.ctx.types, default, &substitution)),
                is_const: tp.is_const,
            });
        }

        CallSignature {
            params: sig
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: instantiate_type(self.ctx.types, param.type_id, &substitution),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            return_type: instantiate_type(self.ctx.types, sig.return_type, &substitution),
            this_type: sig
                .this_type
                .map(|this_type| instantiate_type(self.ctx.types, this_type, &substitution)),
            type_params: renamed_type_params,
            type_predicate: sig
                .type_predicate
                .map(|predicate| tsz_solver::TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target,
                    type_id: predicate
                        .type_id
                        .map(|ty| instantiate_type(self.ctx.types, ty, &substitution)),
                    parameter_index: predicate.parameter_index,
                }),
            is_method: sig.is_method,
        }
    }

    pub(super) fn selected_overload_type_predicate(
        sig: &tsz_solver::CallSignature,
        instantiated_predicate: SelectedTypePredicate,
    ) -> SelectedTypePredicate {
        instantiated_predicate.or_else(|| {
            sig.type_predicate
                .map(|predicate| (predicate, sig.params.clone()))
        })
    }

    pub(super) fn instantiate_overload_return_with_context(
        &mut self,
        sig: &CallSignature,
        instantiated_params: Option<&[ParamInfo]>,
        contextual_type: Option<TypeId>,
        fallback_return_type: TypeId,
    ) -> TypeId {
        if sig.type_params.is_empty() || contextual_type.is_none() {
            return fallback_return_type;
        }

        let sig_shape = FunctionShape {
            params: sig.params.clone(),
            return_type: sig.return_type,
            this_type: sig.this_type,
            type_params: sig.type_params.clone(),
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        };
        let return_substitution =
            self.compute_return_context_substitution_from_shape(&sig_shape, contextual_type);
        if return_substitution.is_empty() {
            return fallback_return_type;
        }

        let mut combined_substitution = if let Some(instantiated_params) = instantiated_params {
            self.extract_arg_inference_substitution(
                &sig.params,
                instantiated_params,
                &sig.type_params,
            )
        } else {
            TypeSubstitution::new()
        };
        self.merge_return_context_substitution(
            &mut combined_substitution,
            &sig.type_params,
            &return_substitution,
        );

        instantiate_type(self.ctx.types, sig.return_type, &combined_substitution)
    }
}
