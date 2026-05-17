//! Correlated identity-reader call helpers.

use super::call_evaluator::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Preserve correlation for identity readers pulled from mapped objects.
    ///
    /// A generic key can turn `{ [K in keyof T]: (value: T[K]) => T[K] }[K]`
    /// into a union of concrete identity functions. The ordinary combined union
    /// signature has an intersected parameter and unioned return, but `tsc`
    /// keeps the key correlation: passing `T[K]` returns `T[K]`.
    pub(super) fn correlated_identity_union_call_return(
        &mut self,
        members: &[TypeId],
        arg_types: &[TypeId],
    ) -> Option<TypeId> {
        if arg_types.len() != 1 {
            return None;
        }
        let arg_type = arg_types[0];
        if !self.is_generic_correlated_union_call_arg(arg_type) {
            return None;
        }

        let mut param_types = Vec::with_capacity(members.len());
        for &member in members {
            let member = self.normalize_union_member(member);
            let (param_type, return_type, param_is_rest) = match self.interner.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let function = self.interner.function_shape(func_id);
                    if !function.type_params.is_empty() {
                        return None;
                    }
                    let [param] = function.params.as_slice() else {
                        return None;
                    };
                    (param.type_id, function.return_type, param.rest)
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.interner.callable_shape(callable_id);
                    if callable.call_signatures.len() != 1 {
                        return None;
                    }
                    let signature = &callable.call_signatures[0];
                    if !signature.type_params.is_empty() {
                        return None;
                    }
                    let [param] = signature.params.as_slice() else {
                        return None;
                    };
                    (param.type_id, signature.return_type, param.rest)
                }
                _ => return None,
            };

            if param_is_rest || param_type != return_type {
                return None;
            }
            param_types.push(param_type);
        }

        let param_union = self.interner.union(param_types);
        let evaluated_arg = self.checker.evaluate_type(arg_type);
        if self.checker.is_assignable_to(arg_type, param_union)
            || (evaluated_arg != arg_type
                && self.checker.is_assignable_to(evaluated_arg, param_union))
        {
            Some(arg_type)
        } else {
            None
        }
    }
}
