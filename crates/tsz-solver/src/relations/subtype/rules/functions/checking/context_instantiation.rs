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
use crate::types::FunctionShape;

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
        );
        retry.is_true().then_some(retry)
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
