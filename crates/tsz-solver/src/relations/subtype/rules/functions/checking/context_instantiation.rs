//! tsc-style `instantiateSignatureInContextOf` fallback for relating two
//! same-arity generic signatures.
//!
//! tsz's primary same-arity generic-vs-generic path alpha-renames the target's
//! type parameters onto the source's and compares structurally. That is correct
//! when each source type parameter appears *bare* on both sides, but it diverges
//! from tsc when the target expresses a source type parameter through a type
//! *function* (a conditional, indexed-access, mapped, or other deferred alias
//! application). In that case tsc instead infers the source's type parameters
//! from the target (`compareSignaturesRelated` -> `instantiateSignatureInContextOf`)
//! before comparing, so the two signatures can relate by identity.

use crate::relations::subtype::{SubtypeChecker, SubtypeResult, TypeResolver};
use crate::types::{FunctionShape, ParamInfo};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// tsc parity: `instantiateSignatureInContextOf` for two same-arity generic
    /// signatures.
    ///
    /// When relating a generic source signature to a generic target whose
    /// parameter/return types express the source's type parameters through a type
    /// *function*, the same-arity alpha-rename path compares the source type
    /// parameter directly against that expression and fails. tsc instead INFERS the
    /// source's type parameters from the target before comparing, so e.g.
    ///
    /// ```ignore
    /// <T, R extends K>() => Box<T>
    ///   ≤  <T, R extends K>() => Box<MappedResponseType<R, T>>
    /// ```
    ///
    /// relates because the source `T` is inferred to `MappedResponseType<R, T>`,
    /// making the two return types identical. This mirrors `compareSignaturesRelated`
    /// calling `instantiateSignatureInContextOf(source, target)` for the generic
    /// case.
    ///
    /// Applied only as a fallback after the direct comparison failed, so it can only
    /// turn a non-`True` result into `True` (never the reverse). It is gated to
    /// ordinary assignability (`erase_generics`): strict member-compatibility checks
    /// (TS2416/TS2430) keep their existing opaque-marker comparison, matching tsc's
    /// stricter handling there.
    pub(super) fn retry_generic_signature_with_context_instantiation(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
        direct_result: SubtypeResult,
        callback_modes: (bool, bool),
        allow_provisional_rest_union_at_this_depth: bool,
    ) -> Option<SubtypeResult> {
        if direct_result.is_true() {
            return None;
        }
        if !self.erase_generics {
            return None;
        }
        if source.type_params.is_empty() || source.type_params.len() != target.type_params.len() {
            return None;
        }
        if source.is_constructor != target.is_constructor {
            return None;
        }
        // Only worth retrying when the target actually references its own type
        // parameters through a non-bare (type-function) occurrence, since a bare
        // alpha-rename already handles the identity case. Without such an
        // occurrence the inference is an identity operation and the re-comparison
        // would fail again.
        if !self.target_references_own_type_params_non_bare(target) {
            return None;
        }
        // A retry may infer the source signature into apparent equality, but it
        // must not erase a strict failure already established inside one of its
        // callback parameters. Determine strictness at the nested slot itself:
        // even a bivariant method parent enters `SignatureCheckMode::Callback`
        // when both slot types are callable.
        let parent_params_are_method = !callback_modes.0 && target.is_method;
        if self.nested_rigid_rest_blocks_contextual_retry(source, target, parent_params_are_method)
        {
            return None;
        }
        // Contextual inference matches the source's type parameters positionally
        // against the target. For that match to find candidates, source and target
        // must be compared in the *same* representation: evaluating only one side
        // (e.g. the target's `Box<MappedResponseType<R, T>>` to its `{ data?: … }`
        // object shape) while the other stays a deferred `Application` leaves the
        // inference with nothing to unify, so the source type parameter silently
        // defaults to `unknown` and the re-comparison fails. Evaluate BOTH shapes
        // to their structural form, infer in that form, instantiate the evaluated
        // source, and re-compare against the evaluated target — all four steps in
        // one representation. This mirrors tsc, where
        // `instantiateSignatureInContextOf` works over the (resolved) apparent
        // types of both signatures.
        let source_for_inference = self.evaluate_function_shape_types(source);
        let target_for_inference = self.evaluate_function_shape_types(target);
        let substitution = self
            .infer_source_type_param_substitution(&source_for_inference, &target_for_inference)
            .ok()?;
        let inferred_source = self.instantiate_function_shape(&source_for_inference, &substitution);
        let allow_constructor_bivariance =
            target_for_inference.is_constructor && target_for_inference.is_method;
        self.in_callback_param_check = callback_modes.0;
        self.in_bivariant_callback_return_check = callback_modes.1;
        let retry = self.check_function_subtype_impl(
            &inferred_source,
            &target_for_inference,
            allow_constructor_bivariance,
            allow_provisional_rest_union_at_this_depth,
        );
        retry.is_true().then_some(retry)
    }

    pub(super) fn rigid_bare_rest_parameter_mismatch(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
        allow_provisional_rest_union: bool,
    ) -> bool {
        self.rigid_bare_rest_params_mismatch(
            &source.params,
            &target.params,
            allow_provisional_rest_union,
        )
    }

    fn rigid_bare_rest_params_mismatch(
        &mut self,
        source: &[ParamInfo],
        target: &[ParamInfo],
        allow_provisional_rest_union: bool,
    ) -> bool {
        let Some((source_rest_index, source_rest)) =
            source.iter().enumerate().find(|(_, param)| param.rest)
        else {
            return false;
        };
        let source_is_bare = if let Some(db) = self.query_db {
            match crate::type_queries::transparent_bare_rest_type_parameter_with_resolver_query(
                db,
                self.resolver,
                source_rest.type_id,
            ) {
                crate::type_queries::RestBinderQuery::Complete(Some(_)) => true,
                crate::type_queries::RestBinderQuery::Complete(None) => false,
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    return true;
                }
            }
        } else {
            self.is_bare_rest_type_param(source_rest.type_id)
        };
        if !source_is_bare {
            return false;
        }
        let target_fixed_count = target.iter().take_while(|param| !param.rest).count();
        if target_fixed_count > source_rest_index {
            return true;
        }
        let Some(target_rest) = target.last().filter(|param| param.rest) else {
            return false;
        };
        let provisional_union =
            allow_provisional_rest_union && self.rest_type_has_union_surface(target_rest.type_id);
        self.bare_source_rest_compatibility(
            source_rest.type_id,
            target_rest.type_id,
            false,
            provisional_union,
        ) == Some(false)
    }

    /// Preserve a rigid nested callback failure across the generic
    /// context-instantiation retry. Function parameters are contravariant, so
    /// the written target parameter is the nested relation source. For each
    /// written source-parameter signature (the nested relation target), every
    /// target-parameter overload must fail rigidly before retry is blocked.
    pub(super) fn nested_rigid_rest_blocks_contextual_retry(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
        parent_params_are_method: bool,
    ) -> bool {
        let Some(db) = self.query_db else {
            return false;
        };
        let source_params = self.normalized_unpacked_params(&source.params);
        let target_params = self.normalized_unpacked_params(&target.params);
        let parent_method_is_bivariant =
            parent_params_are_method && !self.disable_method_bivariance;

        for (source_param, target_param) in source_params.iter().zip(target_params.iter()) {
            let (source_type, target_type) =
                self.effective_param_type_pair(source_param, target_param);
            let source_has_call = self.callable_modality_flags_for_type(source_type).0;
            let target_has_call = self.callable_modality_flags_for_type(target_type).0;
            let Some(pair) = self.classify_callback_parameter_pair(
                source_type,
                target_type,
                source_has_call,
                target_has_call,
                parent_method_is_bivariant,
            ) else {
                continue;
            };
            // Instantiated-generic callback slots retain the parent's ordinary
            // method bivariance, including its reverse-direction retry.
            if parent_method_is_bivariant && !pair.enters_callback_mode {
                continue;
            }
            // Written source parameters become nested relation targets under
            // contravariance; written target parameters become nested sources.
            let nested_target_signatures = crate::type_queries::call_signatures_with_resolver(
                db,
                self.resolver,
                pair.source_nonnull,
            );
            let nested_source_signatures = crate::type_queries::call_signatures_with_resolver(
                db,
                self.resolver,
                pair.target_nonnull,
            );
            let nested_target_signatures = match nested_target_signatures {
                crate::type_queries::RestBinderQuery::Complete(Some(value)) => value,
                crate::type_queries::RestBinderQuery::Complete(None) => continue,
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    return true;
                }
            };
            let nested_source_signatures = match nested_source_signatures {
                crate::type_queries::RestBinderQuery::Complete(Some(value)) => value,
                crate::type_queries::RestBinderQuery::Complete(None) => continue,
                crate::type_queries::RestBinderQuery::Incomplete => {
                    self.note_incomplete_evaluation_relation_event();
                    return true;
                }
            };
            // This guard proves a rigid failure only for the single-signature
            // callback lane. Overloads have N x M matching plus erased-signature
            // fallbacks, which this narrow provenance check does not reproduce.
            if nested_target_signatures.len() != 1 || nested_source_signatures.len() != 1 {
                continue;
            }
            for nested_target in &nested_target_signatures {
                let nested_is_strict = self.strict_function_types
                    && (pair.enters_callback_mode
                        || !nested_target.is_method
                        || self.disable_method_bivariance);
                if !nested_is_strict {
                    continue;
                }
                let nested_target_params = &nested_target.params;
                let mut every_nested_source_fails_rigidly = true;
                for nested_source in &nested_source_signatures {
                    let nested_source_params = &nested_source.params;
                    let rigid_mismatch = self.rigid_bare_rest_params_mismatch(
                        nested_source_params,
                        nested_target_params,
                        false,
                    );
                    if !rigid_mismatch {
                        every_nested_source_fails_rigidly = false;
                        break;
                    }
                }
                if every_nested_source_fails_rigidly {
                    return true;
                }
            }
        }
        false
    }

    /// True when any of `target`'s own type parameters occurs in a parameter,
    /// `this`, or return position through something other than a bare reference —
    /// i.e. inside a type-function application (`Foo<T>`, `T[K]`, a conditional, a
    /// mapped type, …). This is the shape where inference-based contextual
    /// instantiation differs from a plain alpha-rename.
    fn target_references_own_type_params_non_bare(&self, target: &FunctionShape) -> bool {
        let own_ids = self.own_type_param_identity_ids(target);
        if own_ids.is_empty() {
            return false;
        }
        let mentions_non_bare = |type_id| -> bool {
            own_ids.iter().any(|&tp_id| {
                crate::visitor::contains_type_parameters(self.interner, type_id)
                    && !self.type_param_appears_bare(type_id, tp_id)
                    && crate::visitor::collect_all_types(self.interner, type_id)
                        .into_iter()
                        .any(|ty| ty == tp_id)
            })
        };
        target.params.iter().any(|p| mentions_non_bare(p.type_id))
            || target.this_type.is_some_and(mentions_non_bare)
            || mentions_non_bare(target.return_type)
    }

    /// Return a copy of `shape` with its parameter, `this`, and return types
    /// evaluated to their structural form. The type-parameter list and parameter
    /// metadata are preserved. The caller evaluates *both* the source and target
    /// signatures with this before contextual type-parameter inference, so the two
    /// are compared in the same representation.
    fn evaluate_function_shape_types(&mut self, shape: &FunctionShape) -> FunctionShape {
        let mut evaluated = shape.clone();
        for param in &mut evaluated.params {
            param.type_id = self.evaluate_type(param.type_id);
        }
        evaluated.this_type = evaluated.this_type.map(|t| self.evaluate_type(t));
        evaluated.return_type = self.evaluate_type(evaluated.return_type);
        evaluated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::relations::subtype::SubtypeChecker;
    use crate::types::{TypeId, TypeParamInfo, TypeParamOrigin};

    #[test]
    fn nested_retry_guard_follows_contravariant_parameter_direction() {
        let interner = TypeInterner::new();
        let pack = interner.fresh_type_param(TypeParamInfo {
            name: interner.intern_string("Pack"),
            constraint: Some(interner.array(TypeId::UNKNOWN)),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("nested-rest-direction.ts"),
                node: 1,
            },
        });
        let rest_params = vec![ParamInfo {
            suppress_display_optional: false,
            name: None,
            type_id: pack,
            optional: false,
            rest: true,
        }];
        let fixed_params = vec![ParamInfo::unnamed(pack)];
        let callback = |params| {
            interner.function(FunctionShape {
                type_params: vec![],
                params,
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            })
        };
        let outer = |callback_type| FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::unnamed(callback_type)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        let written_source = outer(callback(fixed_params.clone()));
        let written_target = outer(callback(rest_params.clone()));

        let mut checker = SubtypeChecker::new(&interner).with_query_db(&interner);
        checker.strict_function_types = true;
        checker.allow_bivariant_rest = true;

        assert!(checker.rigid_bare_rest_params_mismatch(&rest_params, &fixed_params, false));
        assert!(!checker.rigid_bare_rest_params_mismatch(&fixed_params, &rest_params, false));
        assert!(checker.nested_rigid_rest_blocks_contextual_retry(
            &written_source,
            &written_target,
            false,
        ));
    }
}
