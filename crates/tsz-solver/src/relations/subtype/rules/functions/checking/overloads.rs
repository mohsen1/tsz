use crate::types::{CallSignature, FunctionShape, TypeId};
use crate::visitor::function_shape_id;

use super::super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::super::{erase_call_sig_to_any, erase_fn_shape_to_any};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Compare a function type against a call signature after erasing both signatures'
    /// type parameters to `any`. Matches tsc's N x M `signaturesRelatedTo` path.
    pub(super) fn check_erased_fn_subtype_to_sig(
        &mut self,
        s_fn: &FunctionShape,
        t_sig: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_fn_shape_to_any(s_fn, self.interner);
        let t_erased = erase_call_sig_to_any(t_sig, self.interner);
        self.check_function_subtype(&s_erased, &t_erased)
    }

    pub(super) fn check_erased_fn_params_to_sig_with_matching_return_base(
        &mut self,
        s_fn: &FunctionShape,
        t_sig: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_fn_shape_to_any(s_fn, self.interner);
        let t_erased = erase_call_sig_to_any(t_sig, self.interner);
        self.check_erased_function_shapes_params_with_matching_return_base(
            s_erased, t_erased, false,
        )
    }

    pub fn check_erased_function_type_params_with_matching_return_base(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeResult {
        let Some(s_fn_id) = function_shape_id(self.interner, source) else {
            return SubtypeResult::False;
        };
        let Some(t_fn_id) = function_shape_id(self.interner, target) else {
            return SubtypeResult::False;
        };
        let s_shape = self.interner.function_shape(s_fn_id);
        let t_shape = self.interner.function_shape(t_fn_id);
        let s_erased = erase_fn_shape_to_any(&s_shape, self.interner);
        let t_erased = erase_fn_shape_to_any(&t_shape, self.interner);
        self.check_erased_function_shapes_params_with_matching_return_base(s_erased, t_erased, true)
    }

    fn check_erased_function_shapes_params_with_matching_return_base(
        &mut self,
        mut source: FunctionShape,
        mut target: FunctionShape,
        reject_exact_params: bool,
    ) -> SubtypeResult {
        if !self.return_application_bases_match(source.return_type, target.return_type) {
            return SubtypeResult::False;
        }
        source.return_type = TypeId::ANY;
        target.return_type = TypeId::ANY;
        if reject_exact_params && function_params_match_exactly(&source, &target) {
            return SubtypeResult::False;
        }
        self.check_function_subtype_either_direction(&source, &target)
    }

    fn return_application_bases_match(&self, source_return: TypeId, target_return: TypeId) -> bool {
        let Some((source_base, _)) =
            crate::type_queries::get_application_info(self.interner, source_return)
        else {
            return false;
        };
        let Some((target_base, _)) =
            crate::type_queries::get_application_info(self.interner, target_return)
        else {
            return false;
        };
        source_base == target_base
    }

    /// Compare a call signature against a function type after erasing both signatures'
    /// type parameters to `any`. Matches tsc's N x M `signaturesRelatedTo` path.
    pub(super) fn check_erased_signature_subtype_to_fn(
        &mut self,
        s_sig: &CallSignature,
        t_fn: &FunctionShape,
    ) -> SubtypeResult {
        let mut s_erased = erase_call_sig_to_any(s_sig, self.interner);
        // Preserve constructor-vs-callable intent from the target function shape.
        s_erased.is_constructor = t_fn.is_constructor;
        let t_erased = erase_fn_shape_to_any(t_fn, self.interner);
        self.check_function_subtype(&s_erased, &t_erased)
    }

    /// Compare two call signatures after erasing both signatures' type parameters
    /// to `any`. Used in the N x M callable subtype path to match tsc's behavior.
    pub(super) fn check_erased_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_call_sig_to_any(source, self.interner);
        let t_erased = erase_call_sig_to_any(target, self.interner);
        self.check_function_subtype(&s_erased, &t_erased)
    }

    pub(super) fn check_erased_call_signature_params_with_matching_return_base(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_call_sig_to_any(source, self.interner);
        let t_erased = erase_call_sig_to_any(target, self.interner);
        self.check_erased_function_shapes_params_with_matching_return_base(
            s_erased, t_erased, false,
        )
    }

    /// Compare constructor signatures after erasing type parameters to `any`.
    /// Used in N x M constructor-signature comparison to match tsc behavior.
    ///
    /// `force_strict_construct_params` carries the target's construct-signature
    /// origin (see
    /// [`super::SubtypeChecker::construct_target_requires_strict_params`]) so the
    /// erased multi-overload fallback applies the same `strictFunctionTypes`
    /// contravariance as the primary path; only type parameters are erased, the
    /// declared parameter types still drive variance.
    pub(super) fn check_erased_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        force_strict_construct_params: bool,
    ) -> SubtypeResult {
        for (s_param, t_param) in source.params.iter().zip(target.params.iter()) {
            let (s_has_call, s_has_construct) =
                self.callable_modality_flags_for_type(s_param.type_id);
            let (t_has_call, t_has_construct) =
                self.callable_modality_flags_for_type(t_param.type_id);
            let modality_mismatch =
                (s_has_construct != t_has_construct) || (s_has_call != t_has_call);
            if modality_mismatch && (s_has_call || s_has_construct || t_has_call || t_has_construct)
            {
                return SubtypeResult::False;
            }
        }

        let mut s_erased = erase_call_sig_to_any(source, self.interner);
        let mut t_erased = erase_call_sig_to_any(target, self.interner);
        s_erased.is_constructor = true;
        t_erased.is_constructor = true;
        self.check_function_subtype_with_constructor_strictness(
            &s_erased,
            &t_erased,
            force_strict_construct_params,
        )
    }

    fn check_function_subtype_either_direction(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> SubtypeResult {
        let forward = self.check_function_subtype(source, target);
        if forward.is_true() {
            return forward;
        }
        self.check_function_subtype(target, source)
    }
}

fn function_params_match_exactly(source: &FunctionShape, target: &FunctionShape) -> bool {
    source.params.len() == target.params.len()
        && source
            .params
            .iter()
            .zip(target.params.iter())
            .all(|(source_param, target_param)| {
                source_param.type_id == target_param.type_id
                    && source_param.optional == target_param.optional
                    && source_param.rest == target_param.rest
            })
}
