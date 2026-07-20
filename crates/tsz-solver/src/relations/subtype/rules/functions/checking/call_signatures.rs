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
        self.check_call_signature_subtype_impl(source, target, false)
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

    pub(crate) fn check_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, true)
    }

    fn check_call_signature_subtype_impl(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        is_constructor: bool,
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
        self.check_function_subtype(&source_fn, &target_fn)
    }

    /// Resolve a symbol's [`DefKind`](crate::def::DefKind) through the nominal
    /// `symbol -> def` mapping, falling back to the injected `is_class_symbol`
    /// classifier (the binder `CLASS` flag) when no def mapping is available
    /// (e.g. partially constructed programs). The closure can only witness
    /// classes, so a positive fallback resolves to [`DefKind::Class`].
    ///
    /// Shared by the relation layer's nominal-kind checks, including
    /// `requires_explicit_declared_index_signature`.
    pub(crate) fn symbol_def_kind(
        &self,
        symbol_ref: crate::SymbolRef,
    ) -> Option<crate::def::DefKind> {
        if let Some(def_id) = self.resolver.symbol_to_def_id(symbol_ref) {
            return self.resolver.get_def_kind(def_id);
        }
        self.is_class_symbol.and_then(|is_class_symbol| {
            is_class_symbol(symbol_ref).then_some(crate::def::DefKind::Class)
        })
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
