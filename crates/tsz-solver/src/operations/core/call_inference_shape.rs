use super::call_evaluator::{AssignabilityChecker, CallEvaluator};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{FunctionShape, ParamInfo, TypeId, TypeParamInfo};
use rustc_hash::FxHashSet;
use tsz_common::Atom;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(crate) fn generic_function_shape_for_inference(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<FunctionShape> {
        if func.type_params.is_empty() {
            return None;
        }

        // Only same-named type parameters from an outer scope are collisions;
        // type parameters owned by this signature may appear through contextual typing.
        let own_tp_names: FxHashSet<Atom> = func.type_params.iter().map(|tp| tp.name).collect();
        let own_type_param_ids: FxHashSet<TypeId> = func
            .params
            .iter()
            .map(|p| p.type_id)
            .chain(std::iter::once(func.return_type))
            .chain(func.this_type)
            .flat_map(|ty| {
                crate::visitor::collect_referenced_types(self.interner.as_type_database(), ty)
            })
            .filter(|&ref_ty| {
                crate::type_param_info(self.interner.as_type_database(), ref_ty)
                    .is_some_and(|info| own_tp_names.contains(&info.name))
            })
            .collect();

        let collides = func.type_params.iter().any(|tp| {
            arg_types
                .iter()
                .copied()
                .flat_map(|ty| {
                    crate::visitor::collect_referenced_types(self.interner.as_type_database(), ty)
                })
                .any(|referenced| {
                    if own_type_param_ids.contains(&referenced) {
                        return false;
                    }
                    crate::type_param_info(self.interner.as_type_database(), referenced)
                        .is_some_and(|referenced_tp| referenced_tp.name == tp.name)
                })
        });
        if !collides {
            return None;
        }

        let mut substitution = TypeSubstitution::new();
        let mut fresh_params = Vec::with_capacity(func.type_params.len());
        let mut name_buf = String::with_capacity(32);
        for tp in &func.type_params {
            let placeholder_id = self.checker.next_inference_placeholder_id();
            crate::operations::generic_call::write_placeholder_name(&mut name_buf, placeholder_id);
            let fresh_name = self.interner.intern_string(&name_buf);
            let fresh_info = TypeParamInfo {
                name: fresh_name,
                constraint: None,
                default: None,
                is_const: tp.is_const,
                origin: crate::types::TypeParamOrigin::InferPlaceholder { id: placeholder_id },
            };
            let fresh_type = self.interner.type_param(fresh_info);
            substitution.insert(tp.name, fresh_type);
            fresh_params.push((tp, fresh_info));
        }

        Some(FunctionShape {
            params: func
                .params
                .iter()
                .map(|param| ParamInfo {
                    suppress_display_optional: false,
                    name: param.name,
                    type_id: instantiate_type(self.interner, param.type_id, &substitution),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            return_type: instantiate_type(self.interner, func.return_type, &substitution),
            this_type: func
                .this_type
                .map(|this_type| instantiate_type(self.interner, this_type, &substitution)),
            type_params: fresh_params
                .into_iter()
                .map(|(tp, fresh_info)| TypeParamInfo {
                    name: fresh_info.name,
                    constraint: tp.constraint.map(|constraint| {
                        instantiate_type(self.interner, constraint, &substitution)
                    }),
                    default: tp
                        .default
                        .map(|default| instantiate_type(self.interner, default, &substitution)),
                    is_const: tp.is_const,
                    // The quantifier and every substituted leaf represent the
                    // same freshly minted binder. If the quantifier keeps the
                    // declaration origin while its leaves use
                    // `InferPlaceholder`, exact-domain instantiation treats
                    // the signature's own leaves as foreign binders.
                    origin: fresh_info.origin,
                })
                .collect(),
            type_predicate: func.type_predicate.as_ref().map(|predicate| {
                crate::types::TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target,
                    type_id: predicate
                        .type_id
                        .map(|ty| instantiate_type(self.interner, ty, &substitution)),
                    parameter_index: predicate.parameter_index,
                }
            }),
            is_constructor: func.is_constructor,
            is_method: func.is_method,
        })
    }
}
