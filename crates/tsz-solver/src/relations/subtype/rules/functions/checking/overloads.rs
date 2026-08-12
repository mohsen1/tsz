use crate::types::{CallSignature, FunctionShape, ParamInfo, TypeData, TypeId};
use crate::visitor::{application_id, function_shape_id};

use super::super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::super::{erase_call_sig_to_any, erase_fn_shape_to_any};

#[derive(Clone, Copy)]
struct ErasedReturnNormalization {
    type_id: TypeId,
    contains_conditional: bool,
    contains_deferred: bool,
}

impl ErasedReturnNormalization {
    const fn unchanged(type_id: TypeId) -> Self {
        Self {
            type_id,
            contains_conditional: false,
            contains_deferred: false,
        }
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(super) fn erased_fn_to_sig_return_variance_rejects(
        &self,
        source: &FunctionShape,
        target: &CallSignature,
    ) -> bool {
        let target_erased = erase_call_sig_to_any(target, &source.type_params, self.interner);
        let source = erase_fn_shape_to_any(source, &target.type_params, self.interner);
        let target = target_erased;
        let normalization = self.normalize_erased_target_return(target.return_type);
        if !normalization.contains_conditional {
            return false;
        }
        self.erased_structural_return_variance_rejects(source.return_type, normalization.type_id)
    }

    pub(super) fn erased_call_sig_return_variance_rejects(
        &self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> bool {
        let target_erased = erase_call_sig_to_any(target, &source.type_params, self.interner);
        let source = erase_call_sig_to_any(source, &target.type_params, self.interner);
        let target = target_erased;
        let normalization = self.normalize_erased_target_return(target.return_type);
        if !normalization.contains_conditional {
            return false;
        }
        self.erased_structural_return_variance_rejects(source.return_type, normalization.type_id)
    }

    fn erased_normalized_return_rejects(
        &mut self,
        source_return: TypeId,
        normalization: ErasedReturnNormalization,
    ) -> bool {
        // Deferred/distributive children stay present in the normalized shape
        // and therefore remain under the ordinary return relation; determinate
        // siblings are selected in place. Plain returns use the same complete
        // verdict unchanged. This is deliberately broader than the top-level
        // application variance classifier: the tuple-union prefix shortcut is
        // allowed to prove only parameters and therefore must never bypass any
        // incompatible return.
        self.erased_structural_return_variance_rejects(source_return, normalization.type_id)
            || !self
                .check_return_compat(source_return, normalization.type_id)
                .is_true()
    }

    /// Compare a function type against a call signature after erasing both signatures'
    /// type parameters to `any`. Matches tsc's N x M `signaturesRelatedTo` path.
    pub(super) fn check_erased_fn_subtype_to_sig(
        &mut self,
        s_fn: &FunctionShape,
        t_sig: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_fn_shape_to_any(s_fn, &t_sig.type_params, self.interner);
        let t_erased = erase_call_sig_to_any(t_sig, &s_fn.type_params, self.interner);
        if self.erased_return_variance_rejects(s_erased.return_type, t_erased.return_type) {
            return SubtypeResult::False;
        }
        self.check_function_subtype(&s_erased, &t_erased)
    }

    pub(super) fn check_erased_fn_params_to_sig_with_matching_return_base(
        &mut self,
        s_fn: &FunctionShape,
        t_sig: &CallSignature,
    ) -> SubtypeResult {
        if s_fn.type_params.is_empty() && t_sig.type_params.is_empty() {
            return SubtypeResult::False;
        }
        let s_erased = erase_fn_shape_to_any(s_fn, &t_sig.type_params, self.interner);
        let t_erased = erase_call_sig_to_any(t_sig, &s_fn.type_params, self.interner);
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
        if s_shape.type_params.is_empty() && t_shape.type_params.is_empty() {
            return SubtypeResult::False;
        }
        let s_erased = erase_fn_shape_to_any(&s_shape, &t_shape.type_params, self.interner);
        let t_erased = erase_fn_shape_to_any(&t_shape, &s_shape.type_params, self.interner);
        self.check_erased_function_shapes_params_with_matching_return_base(s_erased, t_erased, true)
    }

    /// Whether erasing the two function signatures exposes a determinate
    /// conditional return whose selected application arguments have a proven
    /// `any`/`never` variance mismatch.
    ///
    /// This is a classification query for checker diagnostic routing. The
    /// ordinary erased-overload relation returns only false for both a parameter
    /// mismatch and this stronger return mismatch; callers need to distinguish
    /// the latter so a later compatibility fallback cannot re-accept it after
    /// application identity has been structurally expanded away.
    pub(crate) fn erased_function_type_params_return_variance_rejects(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, source),
            function_shape_id(self.interner, target),
        ) else {
            return false;
        };
        let s_shape = self.interner.function_shape(s_fn_id);
        let t_shape = self.interner.function_shape(t_fn_id);
        let source = erase_fn_shape_to_any(&s_shape, &t_shape.type_params, self.interner);
        let target = erase_fn_shape_to_any(&t_shape, &s_shape.type_params, self.interner);
        let normalization = self.normalize_erased_target_return(target.return_type);
        if !normalization.contains_conditional {
            return false;
        }
        self.erased_structural_return_variance_rejects(source.return_type, normalization.type_id)
    }

    fn check_erased_function_shapes_params_with_matching_return_base(
        &mut self,
        mut source: FunctionShape,
        mut target: FunctionShape,
        reject_exact_params: bool,
    ) -> SubtypeResult {
        let normalization = self.normalize_erased_target_return(target.return_type);
        if normalization.contains_conditional {
            target.return_type = normalization.type_id;
            if self
                .erased_structural_return_variance_rejects(source.return_type, target.return_type)
            {
                return SubtypeResult::False;
            }
            let related = self.check_function_subtype(&source, &target);
            if related.is_true() {
                return related;
            }
            // Deferred/distributive conditionals stay under the ordinary return
            // relation. They must never reach the same-base fallback below,
            // which intentionally discards application arguments.
            if normalization.contains_deferred {
                return SubtypeResult::False;
            }
            if !self.erased_return_args_are_identical_or_any(source.return_type, target.return_type)
            {
                return SubtypeResult::False;
            }
            // The N×M overload relation deliberately erases method-local binders
            // to `any`. For the same generic definition, tsc lets that wildcard
            // silence only the differing application slots; retain the normal
            // parameter relation after proving every return argument identical or
            // `any` instead of accepting on generic-base identity alone.
            source.return_type = TypeId::ANY;
            target.return_type = TypeId::ANY;
            return self.check_function_subtype(&source, &target);
        }

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

    fn erased_return_variance_rejects(&self, source_return: TypeId, target_return: TypeId) -> bool {
        let (Some(source_app), Some(target_app)) = (
            application_id(self.interner, source_return),
            application_id(self.interner, target_return),
        ) else {
            return false;
        };
        self.application_any_never_variance_rejects(source_app, target_app)
    }

    /// Reuse the generic application's authoritative variance classifier at
    /// covariant built-in projections of a returned value. This is intentionally
    /// narrow: application arguments are left to the generic relation, while
    /// tuples, arrays, readable object members, and nested function returns are
    /// transparent value containers. The paired memo makes recursive shapes
    /// linear in the number of distinct source/target projections.
    fn erased_structural_return_variance_rejects(
        &self,
        source_return: TypeId,
        target_return: TypeId,
    ) -> bool {
        let mut visited = rustc_hash::FxHashSet::default();
        self.erased_structural_return_variance_rejects_inner(
            source_return,
            target_return,
            &mut visited,
        )
    }

    fn erased_structural_return_variance_rejects_inner(
        &self,
        source: TypeId,
        target: TypeId,
        visited: &mut rustc_hash::FxHashSet<(TypeId, TypeId)>,
    ) -> bool {
        if !visited.insert((source, target)) {
            return false;
        }
        if self.erased_return_variance_rejects(source, target) {
            return true;
        }

        match (self.interner.lookup(source), self.interner.lookup(target)) {
            (
                Some(TypeData::Object(source_id) | TypeData::ObjectWithIndex(source_id)),
                Some(TypeData::Object(target_id) | TypeData::ObjectWithIndex(target_id)),
            ) => {
                let source = self.interner.object_shape(source_id);
                let target = self.interner.object_shape(target_id);
                let mut source_index = 0;
                for target_property in &target.properties {
                    while source_index < source.properties.len()
                        && source.properties[source_index].name < target_property.name
                    {
                        source_index += 1;
                    }
                    if let Some(source_property) = source.properties.get(source_index)
                        && source_property.name == target_property.name
                        && self.erased_structural_return_variance_rejects_inner(
                            source_property.type_id,
                            target_property.type_id,
                            visited,
                        )
                    {
                        return true;
                    }
                }
                [
                    (source.string_index, target.string_index),
                    (source.number_index, target.number_index),
                    (source.symbol_index, target.symbol_index),
                ]
                .into_iter()
                .any(|(source_index, target_index)| {
                    source_index
                        .zip(target_index)
                        .is_some_and(|(source, target)| {
                            self.erased_structural_return_variance_rejects_inner(
                                source.value_type,
                                target.value_type,
                                visited,
                            )
                        })
                })
            }
            (Some(TypeData::Array(source)), Some(TypeData::Array(target))) => {
                self.erased_structural_return_variance_rejects_inner(source, target, visited)
            }
            (Some(TypeData::Tuple(source_id)), Some(TypeData::Tuple(target_id))) => {
                let source = self.interner.tuple_list(source_id);
                let target = self.interner.tuple_list(target_id);
                source.len() == target.len()
                    && source.iter().zip(target.iter()).any(|(source, target)| {
                        source.optional == target.optional
                            && source.rest == target.rest
                            && self.erased_structural_return_variance_rejects_inner(
                                source.type_id,
                                target.type_id,
                                visited,
                            )
                    })
            }
            (Some(TypeData::Function(source_id)), Some(TypeData::Function(target_id))) => {
                let source = self.interner.function_shape(source_id);
                let target = self.interner.function_shape(target_id);
                target.return_type != TypeId::VOID
                    && self.erased_structural_return_variance_rejects_inner(
                        source.return_type,
                        target.return_type,
                        visited,
                    )
            }
            (Some(TypeData::Callable(source_id)), Some(TypeData::Callable(target_id))) => {
                let source = self.interner.callable_shape(source_id);
                let target = self.interner.callable_shape(target_id);
                // Positional pairing is a hard proof only for a single
                // signature. Overload sets use N×M coverage and may be
                // reordered, so leave those to the full callable relation.
                let signature_return_rejects = match (
                    source.call_signatures.as_slice(),
                    target.call_signatures.as_slice(),
                ) {
                    ([source], [target]) => {
                        target.return_type != TypeId::VOID
                            && self.erased_structural_return_variance_rejects_inner(
                                source.return_type,
                                target.return_type,
                                visited,
                            )
                    }
                    _ => false,
                };
                let construct_return_rejects = match (
                    source.construct_signatures.as_slice(),
                    target.construct_signatures.as_slice(),
                ) {
                    ([source], [target]) => {
                        target.return_type != TypeId::VOID
                            && self.erased_structural_return_variance_rejects_inner(
                                source.return_type,
                                target.return_type,
                                visited,
                            )
                    }
                    _ => false,
                };
                let mut source_index = 0;
                let property_rejects = target.properties.iter().any(|target_property| {
                    while source_index < source.properties.len()
                        && source.properties[source_index].name < target_property.name
                    {
                        source_index += 1;
                    }
                    source
                        .properties
                        .get(source_index)
                        .is_some_and(|source_property| {
                            source_property.name == target_property.name
                                && self.erased_structural_return_variance_rejects_inner(
                                    source_property.type_id,
                                    target_property.type_id,
                                    visited,
                                )
                        })
                });
                let index_rejects = [
                    (source.string_index, target.string_index),
                    (source.number_index, target.number_index),
                ]
                .into_iter()
                .any(|(source_index, target_index)| {
                    source_index
                        .zip(target_index)
                        .is_some_and(|(source, target)| {
                            self.erased_structural_return_variance_rejects_inner(
                                source.value_type,
                                target.value_type,
                                visited,
                            )
                        })
                });
                signature_return_rejects
                    || construct_return_rejects
                    || property_rejects
                    || index_rejects
            }
            (
                Some(TypeData::ReadonlyType(source) | TypeData::NoInfer(source)),
                Some(TypeData::ReadonlyType(target) | TypeData::NoInfer(target)),
            ) => self.erased_structural_return_variance_rejects_inner(source, target, visited),
            _ => false,
        }
    }

    /// Select determinate erased overload conditionals anywhere in the returned
    /// value's structural shape without expanding lazy aliases.
    ///
    /// Erasing method-local parameters can make a non-distributive conditional
    /// definitively choose its true branch. For example, `keyof O extends K`
    /// becomes `keyof O extends any`, so tsc compares the application in that
    /// branch when checking an implementation against its overloads. Container,
    /// object-property, and nested-function projections all contribute to the
    /// returned value, so they are rebuilt in place before the return relation
    /// runs. The tri-state flags distinguish absence from a still-deferred
    /// conditional; only absence may reach the historical same-base fallback.
    fn normalize_erased_target_return(&self, type_id: TypeId) -> ErasedReturnNormalization {
        let mut memo = rustc_hash::FxHashMap::default();
        self.normalize_erased_overload_return(type_id, &mut memo)
    }

    fn normalize_erased_call_signature_fields(
        &self,
        signature: &mut CallSignature,
        memo: &mut rustc_hash::FxHashMap<TypeId, ErasedReturnNormalization>,
    ) -> (bool, bool, bool) {
        let mut contains_conditional = false;
        let mut contains_deferred = false;
        let mut changed = false;

        for type_param in &mut signature.type_params {
            for slot in [&mut type_param.constraint, &mut type_param.default]
                .into_iter()
                .flatten()
            {
                let child = self.normalize_erased_overload_return(*slot, memo);
                contains_conditional |= child.contains_conditional;
                contains_deferred |= child.contains_deferred;
                changed |= child.type_id != *slot;
                *slot = child.type_id;
            }
        }
        for param in &mut signature.params {
            let child = self.normalize_erased_overload_return(param.type_id, memo);
            contains_conditional |= child.contains_conditional;
            contains_deferred |= child.contains_deferred;
            changed |= child.type_id != param.type_id;
            param.type_id = child.type_id;
        }
        if let Some(this_type) = &mut signature.this_type {
            let child = self.normalize_erased_overload_return(*this_type, memo);
            contains_conditional |= child.contains_conditional;
            contains_deferred |= child.contains_deferred;
            changed |= child.type_id != *this_type;
            *this_type = child.type_id;
        }
        let returned = self.normalize_erased_overload_return(signature.return_type, memo);
        contains_conditional |= returned.contains_conditional;
        contains_deferred |= returned.contains_deferred;
        changed |= returned.type_id != signature.return_type;
        signature.return_type = returned.type_id;
        if let Some(type_predicate) = &mut signature.type_predicate
            && let Some(predicate_type) = &mut type_predicate.type_id
        {
            let child = self.normalize_erased_overload_return(*predicate_type, memo);
            contains_conditional |= child.contains_conditional;
            contains_deferred |= child.contains_deferred;
            changed |= child.type_id != *predicate_type;
            *predicate_type = child.type_id;
        }

        (contains_conditional, contains_deferred, changed)
    }

    fn normalize_erased_overload_return(
        &self,
        type_id: TypeId,
        memo: &mut rustc_hash::FxHashMap<TypeId, ErasedReturnNormalization>,
    ) -> ErasedReturnNormalization {
        if type_id.is_intrinsic() {
            return ErasedReturnNormalization::unchanged(type_id);
        }
        if let Some(&cached) = memo.get(&type_id) {
            return cached;
        }
        // Mark before descending so recursive/shared structural graphs terminate.
        memo.insert(type_id, ErasedReturnNormalization::unchanged(type_id));

        let normalized = match self.interner.lookup(type_id) {
            Some(TypeData::Conditional(conditional_id)) => {
                let conditional = self.interner.get_conditional(conditional_id);
                if conditional.is_distributive
                    || conditional.check_type == TypeId::ERROR
                    || conditional.true_type == TypeId::ERROR
                    || conditional.extends_type != TypeId::ANY
                    || crate::type_queries::contains_infer_types_db(
                        self.interner,
                        conditional.check_type,
                    )
                    || crate::type_queries::contains_infer_types_db(
                        self.interner,
                        conditional.extends_type,
                    )
                {
                    ErasedReturnNormalization {
                        type_id,
                        contains_conditional: true,
                        contains_deferred: true,
                    }
                } else {
                    let selected =
                        self.normalize_erased_overload_return(conditional.true_type, memo);
                    ErasedReturnNormalization {
                        type_id: selected.type_id,
                        contains_conditional: true,
                        // An infer in the selected result still belongs to the
                        // ordinary inference relation. Infer in the discarded
                        // false branch is intentionally irrelevant.
                        contains_deferred: selected.contains_deferred
                            || crate::type_queries::contains_infer_types_db(
                                self.interner,
                                selected.type_id,
                            ),
                    }
                }
            }
            Some(TypeData::Application(application_id)) => {
                let application = self.interner.type_application(application_id);
                let mut contains_conditional = false;
                let mut contains_deferred = false;
                let mut changed = false;
                let args = application
                    .args
                    .iter()
                    .map(|&arg| {
                        let child = self.normalize_erased_overload_return(arg, memo);
                        contains_conditional |= child.contains_conditional;
                        contains_deferred |= child.contains_deferred;
                        changed |= child.type_id != arg;
                        child.type_id
                    })
                    .collect();
                ErasedReturnNormalization {
                    type_id: if changed {
                        self.interner.application(application.base, args)
                    } else {
                        type_id
                    },
                    contains_conditional,
                    contains_deferred,
                }
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let is_indexed = matches!(
                    self.interner.lookup(type_id),
                    Some(TypeData::ObjectWithIndex(_))
                );
                let mut shape = self.interner.object_shape(shape_id).as_ref().clone();
                let mut contains_conditional = false;
                let mut contains_deferred = false;
                let mut changed = false;

                for property in &mut shape.properties {
                    let read = self.normalize_erased_overload_return(property.type_id, memo);
                    contains_conditional |= read.contains_conditional;
                    contains_deferred |= read.contains_deferred;
                    changed |= read.type_id != property.type_id;
                    property.type_id = read.type_id;

                    let write = self.normalize_erased_overload_return(property.write_type, memo);
                    contains_conditional |= write.contains_conditional;
                    contains_deferred |= write.contains_deferred;
                    changed |= write.type_id != property.write_type;
                    property.write_type = write.type_id;
                }

                for index in [
                    &mut shape.string_index,
                    &mut shape.number_index,
                    &mut shape.symbol_index,
                ]
                .into_iter()
                .flatten()
                {
                    let key = self.normalize_erased_overload_return(index.key_type, memo);
                    let value = self.normalize_erased_overload_return(index.value_type, memo);
                    contains_conditional |= key.contains_conditional || value.contains_conditional;
                    contains_deferred |= key.contains_deferred || value.contains_deferred;
                    changed |= key.type_id != index.key_type || value.type_id != index.value_type;
                    index.key_type = key.type_id;
                    index.value_type = value.type_id;
                }

                ErasedReturnNormalization {
                    type_id: if !changed {
                        type_id
                    } else if is_indexed {
                        self.interner.object_with_index(shape)
                    } else {
                        self.interner.object_with_flags_and_symbol(
                            shape.properties,
                            shape.flags,
                            shape.symbol,
                        )
                    },
                    contains_conditional,
                    contains_deferred,
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let mut shape = self.interner.function_shape(shape_id).as_ref().clone();
                let mut contains_conditional = false;
                let mut contains_deferred = false;
                let mut changed = false;

                for type_param in &mut shape.type_params {
                    for slot in [&mut type_param.constraint, &mut type_param.default]
                        .into_iter()
                        .flatten()
                    {
                        let child = self.normalize_erased_overload_return(*slot, memo);
                        contains_conditional |= child.contains_conditional;
                        contains_deferred |= child.contains_deferred;
                        changed |= child.type_id != *slot;
                        *slot = child.type_id;
                    }
                }
                for param in &mut shape.params {
                    let child = self.normalize_erased_overload_return(param.type_id, memo);
                    contains_conditional |= child.contains_conditional;
                    contains_deferred |= child.contains_deferred;
                    changed |= child.type_id != param.type_id;
                    param.type_id = child.type_id;
                }
                if let Some(this_type) = &mut shape.this_type {
                    let child = self.normalize_erased_overload_return(*this_type, memo);
                    contains_conditional |= child.contains_conditional;
                    contains_deferred |= child.contains_deferred;
                    changed |= child.type_id != *this_type;
                    *this_type = child.type_id;
                }
                let returned = self.normalize_erased_overload_return(shape.return_type, memo);
                contains_conditional |= returned.contains_conditional;
                contains_deferred |= returned.contains_deferred;
                changed |= returned.type_id != shape.return_type;
                shape.return_type = returned.type_id;
                if let Some(type_predicate) = &mut shape.type_predicate
                    && let Some(predicate_type) = &mut type_predicate.type_id
                {
                    let child = self.normalize_erased_overload_return(*predicate_type, memo);
                    contains_conditional |= child.contains_conditional;
                    contains_deferred |= child.contains_deferred;
                    changed |= child.type_id != *predicate_type;
                    *predicate_type = child.type_id;
                }

                ErasedReturnNormalization {
                    type_id: if changed {
                        self.interner.function(shape)
                    } else {
                        type_id
                    },
                    contains_conditional,
                    contains_deferred,
                }
            }
            Some(TypeData::Callable(shape_id)) => {
                let mut shape = self.interner.callable_shape(shape_id).as_ref().clone();
                let mut contains_conditional = false;
                let mut contains_deferred = false;
                let mut changed = false;

                for signature in shape
                    .call_signatures
                    .iter_mut()
                    .chain(shape.construct_signatures.iter_mut())
                {
                    let (signature_conditional, signature_deferred, signature_changed) =
                        self.normalize_erased_call_signature_fields(signature, memo);
                    contains_conditional |= signature_conditional;
                    contains_deferred |= signature_deferred;
                    changed |= signature_changed;
                }
                for property in &mut shape.properties {
                    let read = self.normalize_erased_overload_return(property.type_id, memo);
                    let write = self.normalize_erased_overload_return(property.write_type, memo);
                    contains_conditional |= read.contains_conditional || write.contains_conditional;
                    contains_deferred |= read.contains_deferred || write.contains_deferred;
                    changed |=
                        read.type_id != property.type_id || write.type_id != property.write_type;
                    property.type_id = read.type_id;
                    property.write_type = write.type_id;
                }
                for index in [&mut shape.string_index, &mut shape.number_index]
                    .into_iter()
                    .flatten()
                {
                    let key = self.normalize_erased_overload_return(index.key_type, memo);
                    let value = self.normalize_erased_overload_return(index.value_type, memo);
                    contains_conditional |= key.contains_conditional || value.contains_conditional;
                    contains_deferred |= key.contains_deferred || value.contains_deferred;
                    changed |= key.type_id != index.key_type || value.type_id != index.value_type;
                    index.key_type = key.type_id;
                    index.value_type = value.type_id;
                }

                ErasedReturnNormalization {
                    type_id: if changed {
                        self.interner.callable(shape)
                    } else {
                        type_id
                    },
                    contains_conditional,
                    contains_deferred,
                }
            }
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                let is_union = matches!(self.interner.lookup(type_id), Some(TypeData::Union(_)));
                let mut contains_conditional = false;
                let mut contains_deferred = false;
                let mut changed = false;
                let members = members
                    .iter()
                    .map(|&member| {
                        let child = self.normalize_erased_overload_return(member, memo);
                        contains_conditional |= child.contains_conditional;
                        contains_deferred |= child.contains_deferred;
                        changed |= child.type_id != member;
                        child.type_id
                    })
                    .collect();
                ErasedReturnNormalization {
                    type_id: if !changed {
                        type_id
                    } else if is_union {
                        self.interner.union(members)
                    } else {
                        self.interner.intersection(members)
                    },
                    contains_conditional,
                    contains_deferred,
                }
            }
            Some(TypeData::Array(element)) => {
                let child = self.normalize_erased_overload_return(element, memo);
                ErasedReturnNormalization {
                    type_id: if child.type_id == element {
                        type_id
                    } else {
                        self.interner.array(child.type_id)
                    },
                    contains_conditional: child.contains_conditional,
                    contains_deferred: child.contains_deferred,
                }
            }
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.interner.tuple_list(list_id);
                let mut contains_conditional = false;
                let mut contains_deferred = false;
                let mut changed = false;
                let elements = elements
                    .iter()
                    .copied()
                    .map(|mut element| {
                        let child = self.normalize_erased_overload_return(element.type_id, memo);
                        contains_conditional |= child.contains_conditional;
                        contains_deferred |= child.contains_deferred;
                        changed |= child.type_id != element.type_id;
                        element.type_id = child.type_id;
                        element
                    })
                    .collect();
                ErasedReturnNormalization {
                    type_id: if changed {
                        self.interner.tuple(elements)
                    } else {
                        type_id
                    },
                    contains_conditional,
                    contains_deferred,
                }
            }
            Some(TypeData::IndexAccess(object, index)) => {
                let normalized_object = self.normalize_erased_overload_return(object, memo);
                let normalized_index = self.normalize_erased_overload_return(index, memo);
                let changed =
                    normalized_object.type_id != object || normalized_index.type_id != index;
                ErasedReturnNormalization {
                    type_id: if changed {
                        self.interner
                            .index_access(normalized_object.type_id, normalized_index.type_id)
                    } else {
                        type_id
                    },
                    contains_conditional: normalized_object.contains_conditional
                        || normalized_index.contains_conditional,
                    contains_deferred: normalized_object.contains_deferred
                        || normalized_index.contains_deferred,
                }
            }
            Some(TypeData::ReadonlyType(inner)) => {
                let child = self.normalize_erased_overload_return(inner, memo);
                ErasedReturnNormalization {
                    type_id: if child.type_id == inner {
                        type_id
                    } else {
                        self.interner.readonly_type(child.type_id)
                    },
                    contains_conditional: child.contains_conditional,
                    contains_deferred: child.contains_deferred,
                }
            }
            Some(TypeData::NoInfer(inner)) => {
                let child = self.normalize_erased_overload_return(inner, memo);
                ErasedReturnNormalization {
                    type_id: if child.type_id == inner {
                        type_id
                    } else {
                        self.interner.no_infer(child.type_id)
                    },
                    contains_conditional: child.contains_conditional,
                    contains_deferred: child.contains_deferred,
                }
            }
            Some(TypeData::Substitution {
                base_type,
                constraint,
            }) => {
                let base = self.normalize_erased_overload_return(base_type, memo);
                let normalized_constraint = self.normalize_erased_overload_return(constraint, memo);
                let changed =
                    base.type_id != base_type || normalized_constraint.type_id != constraint;
                ErasedReturnNormalization {
                    type_id: if changed {
                        self.interner
                            .substitution(base.type_id, normalized_constraint.type_id)
                    } else {
                        type_id
                    },
                    contains_conditional: base.contains_conditional
                        || normalized_constraint.contains_conditional,
                    contains_deferred: base.contains_deferred
                        || normalized_constraint.contains_deferred,
                }
            }
            _ => ErasedReturnNormalization::unchanged(type_id),
        };
        memo.insert(type_id, normalized);
        normalized
    }

    fn erased_return_args_are_identical_or_any(
        &self,
        source_return: TypeId,
        target_return: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) =
            crate::type_queries::get_application_info(self.interner, source_return)
        else {
            return false;
        };
        let Some((target_base, target_args)) =
            crate::type_queries::get_application_info(self.interner, target_return)
        else {
            return false;
        };
        source_base == target_base
            && source_args.len() == target_args.len()
            && source_args
                .iter()
                .zip(target_args.iter())
                .all(|(&source_arg, &target_arg)| {
                    source_arg == target_arg
                        || ((source_arg.is_any() || target_arg.is_any())
                            && source_arg != TypeId::NEVER
                            && target_arg != TypeId::NEVER)
                })
            && source_args
                .iter()
                .zip(target_args.iter())
                .any(|(&source_arg, &target_arg)| source_arg != target_arg)
    }

    /// Compare a call signature against a function type after erasing both signatures'
    /// type parameters to `any`. Matches tsc's N x M `signaturesRelatedTo` path.
    pub(super) fn check_erased_signature_subtype_to_fn(
        &mut self,
        s_sig: &CallSignature,
        t_fn: &FunctionShape,
    ) -> SubtypeResult {
        let mut s_erased = erase_call_sig_to_any(s_sig, &t_fn.type_params, self.interner);
        // Preserve constructor-vs-callable intent from the target function shape.
        s_erased.is_constructor = t_fn.is_constructor;
        let t_erased = erase_fn_shape_to_any(t_fn, &s_sig.type_params, self.interner);
        if self.erased_return_variance_rejects(s_erased.return_type, t_erased.return_type) {
            return SubtypeResult::False;
        }
        self.check_function_subtype(&s_erased, &t_erased)
    }

    /// Compare two call signatures after erasing both signatures' type parameters
    /// to `any`. Used in the N x M callable subtype path to match tsc's behavior.
    pub(super) fn check_erased_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_call_sig_to_any(source, &target.type_params, self.interner);
        let t_erased = erase_call_sig_to_any(target, &source.type_params, self.interner);
        if self.erased_return_variance_rejects(s_erased.return_type, t_erased.return_type) {
            return SubtypeResult::False;
        }
        self.check_function_subtype(&s_erased, &t_erased)
    }

    pub(super) fn check_erased_call_signature_params_with_matching_return_base(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        if source.type_params.is_empty() && target.type_params.is_empty() {
            return SubtypeResult::False;
        }
        let s_erased = erase_call_sig_to_any(source, &target.type_params, self.interner);
        let t_erased = erase_call_sig_to_any(target, &source.type_params, self.interner);
        let normalization = self.normalize_erased_target_return(t_erased.return_type);
        // Preserve the hard plain-return variance veto without recursively
        // relating the full returned value here. Conditional returns and the
        // same-base generic fallback are owned by the helper below.
        if !normalization.contains_conditional
            && self.erased_structural_return_variance_rejects(
                s_erased.return_type,
                normalization.type_id,
            )
        {
            return SubtypeResult::False;
        }
        self.check_erased_function_shapes_params_with_matching_return_base(
            s_erased, t_erased, false,
        )
    }

    /// Compare constructor signatures after erasing type parameters to `any`.
    /// Used in N x M constructor-signature comparison to match tsc behavior.
    pub(super) fn check_erased_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
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

        let mut s_erased = erase_call_sig_to_any(source, &target.type_params, self.interner);
        let mut t_erased = erase_call_sig_to_any(target, &source.type_params, self.interner);
        s_erased.is_constructor = true;
        t_erased.is_constructor = true;
        self.check_function_subtype(&s_erased, &t_erased)
    }

    pub(super) fn method_overloads_cover_tuple_union_rest_target(
        &mut self,
        source_sigs: &[CallSignature],
        target_sig: &CallSignature,
    ) -> bool {
        use crate::type_queries::data::get_union_members;
        use crate::type_queries::unpack_tuple_rest_parameter;

        let Some(last_target_param) = target_sig.params.last().filter(|param| param.rest) else {
            return false;
        };
        let Some(union_members) = get_union_members(self.interner, last_target_param.type_id)
        else {
            return false;
        };
        // The tuple-union prefix shortcut below exists only to prove parameter
        // coverage. Normalize the target once and compute the erased return
        // rejection once per source signature so a params-only match cannot
        // bypass a determinate conditional return mismatch (and so V tuple
        // variants do not repeat the return walk).
        // This tuple/union-rest coverage helper compares per-element candidate
        // shapes, not a single signature pair, so there is no single "paired"
        // side to seed identity-shared erasure from; keep the erasure scoped
        // to each signature's own declared type parameters, as before.
        let erased_target = erase_call_sig_to_any(target_sig, &[], self.interner);
        let target_normalization = self.normalize_erased_target_return(erased_target.return_type);
        let return_rejections: Vec<bool> = source_sigs
            .iter()
            .map(|source_sig| {
                let erased_source = erase_call_sig_to_any(source_sig, &[], self.interner);
                self.erased_normalized_return_rejects(
                    erased_source.return_type,
                    target_normalization,
                )
            })
            .collect();

        let prefix_params = &target_sig.params[..target_sig.params.len().saturating_sub(1)];
        union_members.iter().all(|member_type_id| {
            let member_param = ParamInfo {
                suppress_display_optional: false,
                type_id: *member_type_id,
                rest: true,
                ..*last_target_param
            };
            let mut variant_params = prefix_params.to_vec();
            variant_params.extend(unpack_tuple_rest_parameter(self.interner, &member_param));
            source_sigs
                .iter()
                .zip(&return_rejections)
                .any(|(source_sig, &return_rejects)| {
                    if return_rejects {
                        return false;
                    }
                    let source_fn = FunctionShape {
                        type_params: source_sig.type_params.clone(),
                        params: source_sig.params.clone(),
                        this_type: source_sig.this_type,
                        return_type: source_sig.return_type,
                        type_predicate: source_sig.type_predicate,
                        is_constructor: false,
                        is_method: source_sig.is_method,
                    };
                    let variant_fn = FunctionShape {
                        type_params: target_sig.type_params.clone(),
                        params: variant_params.clone(),
                        this_type: target_sig.this_type,
                        return_type: target_sig.return_type,
                        type_predicate: target_sig.type_predicate,
                        is_constructor: false,
                        is_method: target_sig.is_method,
                    };
                    self.check_function_subtype(&source_fn, &variant_fn)
                        .is_true()
                        || self.method_overload_prefix_covers_variant(&source_fn, &variant_fn)
                })
        })
    }

    fn method_overload_prefix_covers_variant(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        if target.params.is_empty() {
            return false;
        }
        if source.params.len() < target.params.len()
            || !self.are_this_parameters_compatible(source.this_type, target.this_type, true)
            || !self.are_type_predicates_compatible(source, target)
        {
            return false;
        }
        source
            .params
            .iter()
            .zip(target.params.iter())
            .take(target.params.len())
            .all(|(source_param, target_param)| {
                let (source_type, target_type) =
                    self.effective_param_type_pair(source_param, target_param);
                self.are_parameters_compatible_impl(source_type, target_type, true)
            })
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
