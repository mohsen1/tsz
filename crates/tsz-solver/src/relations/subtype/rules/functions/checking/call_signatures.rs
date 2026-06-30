use crate::relations::subtype::{SubtypeChecker, SubtypeResult, TypeResolver};
use crate::types::{CallSignature, FunctionShape, TypeId};
use crate::visitor::callable_shape_id;

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Check call signature subtyping.
    pub(crate) fn check_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, false, false)
    }

    pub(crate) fn callable_modality_flags_for_type(&mut self, type_id: TypeId) -> (bool, bool) {
        let direct = self.callable_modality_flags_for_type_direct(type_id);
        if direct.0 || direct.1 {
            return direct;
        }
        let evaluated = self.evaluate_type(type_id);
        if evaluated == type_id {
            direct
        } else {
            self.callable_modality_flags_for_type_direct(evaluated)
        }
    }

    fn callable_modality_flags_for_type_direct(&self, type_id: TypeId) -> (bool, bool) {
        if let Some(shape_id) = callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            return (
                !shape.call_signatures.is_empty(),
                !shape.construct_signatures.is_empty(),
            );
        }
        if let Some(fn_id) = crate::visitor::function_shape_id(self.interner, type_id) {
            let f = self.interner.function_shape(fn_id);
            return (!f.is_constructor, f.is_constructor);
        }
        (false, false)
    }

    /// Compare two construct signatures as a constructor relation.
    ///
    /// `force_strict_construct_params` carries the target's construct-signature
    /// origin (see
    /// [`super::SubtypeChecker::construct_target_requires_strict_params`]).
    pub(crate) fn check_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        force_strict_construct_params: bool,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, true, force_strict_construct_params)
    }

    fn check_call_signature_subtype_impl(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        is_constructor: bool,
        force_strict_construct_params: bool,
    ) -> SubtypeResult {
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate,
            is_constructor,
            is_method: source.is_method,
        };
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate,
            is_constructor,
            is_method: target.is_method,
        };
        self.check_function_subtype_with_constructor_strictness(
            &source_fn,
            &target_fn,
            force_strict_construct_params,
        )
    }

    pub(crate) fn constructor_signatures_need_strict_params(
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        if !(source.is_constructor || target.is_constructor) {
            return false;
        }

        let source_generic = !source.type_params.is_empty();
        let target_generic = !target.type_params.is_empty();
        if !source_generic && !target_generic {
            // Non-generic constructors need strict params when there's an
            // optionality mismatch between corresponding parameters. This
            // matches tsc where property-typed constructor types like
            // `new (x?: number) => number` use strict comparison, not
            // constructor bivariance.
            return source
                .params
                .iter()
                .zip(target.params.iter())
                .any(|(sp, tp)| sp.optional != tp.optional);
        }

        if source_generic && !target_generic {
            let has_optionality_mismatch = source
                .params
                .iter()
                .zip(target.params.iter())
                .any(|(sp, tp)| sp.optional != tp.optional);
            return has_optionality_mismatch
                || source.type_params.iter().any(|tp| tp.constraint.is_some());
        }

        source.type_params.len() != target.type_params.len()
            || source
                .type_params
                .iter()
                .chain(target.type_params.iter())
                .any(|tp| tp.constraint.is_some())
            || source.params.len() != 1
            || target.params.len() != 1
    }

    /// Check call signature subtype to function shape.
    pub(crate) fn check_call_signature_subtype_to_fn(
        &mut self,
        source: &CallSignature,
        target: &FunctionShape,
    ) -> SubtypeResult {
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate,
            is_constructor: target.is_constructor,
            is_method: source.is_method,
        };
        self.check_function_subtype(&source_fn, target)
    }

    /// Check function shape subtype to call signature.
    pub(crate) fn check_call_signature_subtype_fn(
        &mut self,
        source: &FunctionShape,
        target: &CallSignature,
    ) -> SubtypeResult {
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate,
            is_constructor: source.is_constructor,
            is_method: target.is_method,
        };
        self.check_function_subtype(source, &target_fn)
    }
}
