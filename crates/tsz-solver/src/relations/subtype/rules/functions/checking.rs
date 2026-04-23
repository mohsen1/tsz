//! Function and callable subtype checking -- main checking methods.
//!
//! Contains the core `check_function_subtype` entry point and related
//! signature comparison logic (call signatures, constructors, params).

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::type_param_info;
use crate::types::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, ObjectFlags,
    ObjectShape, ParamInfo, PropertyInfo, TypeData, TypeId, Visibility,
};
use crate::visitor::callable_shape_id;

use super::super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::{erase_call_sig_to_any, erase_fn_shape_to_any, erase_type_params_to_constraints};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(crate) fn check_function_subtype(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> SubtypeResult {
        let allow_constructor_bivariance =
            !Self::constructor_signatures_need_strict_params(source, target);
        self.check_function_subtype_impl(source, target, allow_constructor_bivariance)
    }

    fn check_function_subtype_impl(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
        allow_constructor_bivariance: bool,
    ) -> SubtypeResult {
        // Constructor vs non-constructor
        if source.is_constructor != target.is_constructor {
            return SubtypeResult::False;
        }

        let mut source_instantiated = source.clone();
        let mut target_instantiated = target.clone();
        // Track type param equivalences scope for cleanup at end of function.
        let equiv_start = self.type_param_equivalences.len();

        // Generic source vs generic target (same arity): normalize both signatures so they
        // can be compared structurally.
        //
        // Two strategies are used depending on constraint compatibility:
        // 1. Alpha-renaming: map target type params to source type params, check constraints
        //    bidirectionally. Works when constraints are related (especially outer-scope type
        //    parameters like `T` vs `T1 extends T`).
        // 2. Canonicalization (tsc-like): replace target type params with their constraints,
        //    then infer source type params from the concrete target. Handles cases where
        //    constraints differ structurally but are semantically equivalent through parameter
        //    usage (e.g., `<S extends {p:string}[]>(x: S)` vs `<T extends {p:string}>(x: T[])`).
        let signature_mentions_nonlocal_type_params =
            |shape: &crate::types::FunctionShape| -> bool {
                let local_tp_ids: Vec<TypeId> = shape
                    .type_params
                    .iter()
                    .map(|tp| self.interner.type_param(*tp))
                    .collect();
                let refs_nonlocal_type_param = |type_id: TypeId| {
                    crate::visitor::collect_all_types(self.interner, type_id)
                        .into_iter()
                        .any(|ty| {
                            type_param_info(self.interner, ty).is_some()
                                && !local_tp_ids.contains(&ty)
                        })
                };

                shape
                    .params
                    .iter()
                    .any(|param| refs_nonlocal_type_param(param.type_id))
                    || shape.this_type.is_some_and(refs_nonlocal_type_param)
                    || refs_nonlocal_type_param(shape.return_type)
            };

        if !source_instantiated.type_params.is_empty()
            && source_instantiated.type_params.len() == target_instantiated.type_params.len()
            && !target_instantiated.type_params.is_empty()
        {
            if !self.erase_generics {
                let source_mentions_nonlocal =
                    signature_mentions_nonlocal_type_params(&source_instantiated);
                let target_mentions_nonlocal =
                    signature_mentions_nonlocal_type_params(&target_instantiated);
                if source_mentions_nonlocal != target_mentions_nonlocal {
                    self.type_param_equivalences.truncate(equiv_start);
                    return SubtypeResult::False;
                }
            }

            let mut target_to_source_substitution = TypeSubstitution::new();
            let mut source_identity_substitution = TypeSubstitution::new();
            for (source_tp, target_tp) in source_instantiated
                .type_params
                .iter()
                .zip(target_instantiated.type_params.iter())
            {
                let source_type_param_type = self.interner.type_param(*source_tp);
                target_to_source_substitution.insert(target_tp.name, source_type_param_type);
                source_identity_substitution.insert(source_tp.name, source_type_param_type);
            }

            let mapped_constraint_sensitive =
                source_instantiated.type_params.iter().any(|tp| {
                    source_instantiated.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, tp.name)
                    }) || source_instantiated.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, tp.name)
                    }) || self.type_param_appears_in_mapped_context(
                        source_instantiated.return_type,
                        tp.name,
                    ) || target_instantiated.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, tp.name)
                    }) || target_instantiated.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, tp.name)
                    }) || self.type_param_appears_in_mapped_context(
                        target_instantiated.return_type,
                        tp.name,
                    )
                });

            // Mapped/indexed generic signatures are constraint-sensitive: a stricter
            // target constraint like `U extends string[]` must stay visible rather
            // than being alpha-renamed onto an unconstrained source parameter `T`,
            // or apparent-member facts can be erased and make the signatures look
            // spuriously compatible. Outside that lane, keep the broader one-way
            // compatibility that TypeScript uses for generic function directionality.
            let constraints_allow_alpha_rename = source_instantiated
                .type_params
                .iter()
                .zip(target_instantiated.type_params.iter())
                .all(|(source_tp, target_tp)| {
                    let source_constraint = source_tp.constraint.unwrap_or(TypeId::UNKNOWN);
                    let target_constraint =
                        target_tp.constraint.map_or(TypeId::UNKNOWN, |constraint| {
                            instantiate_type(
                                self.interner,
                                constraint,
                                &target_to_source_substitution,
                            )
                        });

                    // For alpha-rename to succeed, the target's constraint must be
                    // at least as strict as the source's constraint. This ensures the source
                    // function's constraint requirements are at most as strict
                    // as the target's.
                    //
                    // The key check: target_constraint ≤ source_constraint
                    // If target's constraint is not assignable to source's constraint,
                    // then source is stricter and we should NOT allow alpha-rename.
                    //
                    // Example:
                    //   source: <S extends T> (constraint: T)
                    //   target: <S> (constraint: unknown)
                    //   check: unknown ≤ T → true (unknown is assignable to T)
                    //   But wait, this means target is LOOSER, so source is STRICTER!
                    //   We should NOT allow alpha-rename when source is stricter.
                    //
                    // The issue: when source has S extends T and target has S (no constraint),
                    // the source is stricter. Alpha-rename would erase this distinction.
                    // We need to detect when source has a constraint that target doesn't.
                    //
                    // Correction: check if source has a constraint that target doesn't.
                    // If source has constraint and target doesn't (or target's constraint
                    // is looser), then alpha-rename should fail.
                    //
                    // We check: does source have a constraint that makes it stricter?
                    // Source is stricter if:
                    // - source has constraint, target doesn't: always stricter
                    // - both have constraints: source is stricter if its constraint is narrower
                    let source_has_constraint = source_tp.constraint.is_some();
                    let target_has_constraint = target_tp.constraint.is_some();

                    let source_is_stricter = if source_has_constraint && !target_has_constraint {
                        // Source has constraint, target doesn't → source is stricter
                        true
                    } else if !source_has_constraint && target_has_constraint {
                        // Target has constraint, source doesn't → source is looser (OK)
                        false
                    } else if source_has_constraint && target_has_constraint {
                        // Both have constraints: check if source's is stricter
                        // If target's constraint is NOT assignable to source's constraint,
                        // then source is stricter
                        !self
                            .check_subtype(target_constraint, source_constraint)
                            .is_true()
                    } else {
                        // Neither has constraint → equal
                        false
                    };

                    if source_is_stricter {
                        return false; // Don't allow alpha-rename
                    }

                    // For mapped/indexed contexts, both directions must hold
                    // to preserve constraint information.
                    if mapped_constraint_sensitive {
                        let target_to_source = self
                            .check_subtype(target_constraint, source_constraint)
                            .is_true();
                        let source_to_target = self
                            .check_subtype(source_constraint, target_constraint)
                            .is_true();
                        target_to_source && source_to_target
                    } else {
                        true // Constraints are compatible, allow alpha-rename
                    }
                });

            if constraints_allow_alpha_rename {
                // Strategy 1: alpha-rename — both shapes use source type param identities.
                //
                // Establish type parameter equivalences for structural comparison.
                // When return types are pre-evaluated Object types (e.g., IList<D> already
                // expanded to an Object shape), name-based substitution may fail to penetrate
                // inner functions with same-named type params (shadowing). The equivalences
                // allow structural comparison to treat the original source/target type params
                // as identical, fixing false mismatches for structurally identical generic
                // method signatures with different type param names.
                for (source_tp, target_tp) in source_instantiated
                    .type_params
                    .iter()
                    .zip(target_instantiated.type_params.iter())
                {
                    let source_tp_type = self.interner.type_param(*source_tp);
                    let target_tp_type = self.interner.type_param(*target_tp);
                    if source_tp_type != target_tp_type {
                        self.type_param_equivalences
                            .push((source_tp_type, target_tp_type));
                    }
                }

                source_instantiated = self.instantiate_function_shape(
                    &source_instantiated,
                    &source_identity_substitution,
                );
                target_instantiated = self.instantiate_function_shape(
                    &target_instantiated,
                    &target_to_source_substitution,
                );
            } else if mapped_constraint_sensitive {
                // When mapped/indexed types are involved, constraint differences are
                // semantically significant and cannot be erased safely. Reject immediately.
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            } else {
                // Strategy 2: When alpha-rename fails due to incompatible constraints
                // in non-mapped contexts, fall through to constraint erasure (tsc's
                // `getErasedSignature` behavior). Both signatures have their type params
                // replaced with their constraints (or `unknown` if unconstrained) and
                // then compared structurally.
                //
                // Example that should PASS:
                //   source: <T extends {p: string}>(x: T[]) => void
                //   target: <S extends {p: string}[]>(x: S) => void
                //   erased: (x: {p: string}[]) => void vs (x: {p: string}[]) => void → OK
                //
                // Example that should FAIL:
                //   source: <T extends string>(x: T[]) => void
                //   target: <S extends number[]>(x: S) => void
                //   erased: (x: string[]) => void vs (x: number[]) => void → FAIL
                let source_canonical =
                    erase_type_params_to_constraints(&source_instantiated.type_params);
                source_instantiated =
                    self.instantiate_function_shape(&source_instantiated, &source_canonical);

                let target_canonical =
                    erase_type_params_to_constraints(&target_instantiated.type_params);
                target_instantiated =
                    self.instantiate_function_shape(&target_instantiated, &target_canonical);
            }
        }

        let source_mentions_nonlocal_type_params = {
            let local_source_tp_ids: Vec<TypeId> = source_instantiated
                .type_params
                .iter()
                .map(|tp| self.interner.type_param(*tp))
                .collect();
            let refs_nonlocal_type_param = |type_id: TypeId| {
                crate::visitor::collect_all_types(self.interner, type_id)
                    .into_iter()
                    .any(|ty| {
                        type_param_info(self.interner, ty).is_some()
                            && !local_source_tp_ids.contains(&ty)
                    })
            };
            source_instantiated
                .params
                .iter()
                .any(|p| refs_nonlocal_type_param(p.type_id))
                || source_instantiated
                    .this_type
                    .is_some_and(refs_nonlocal_type_param)
                || refs_nonlocal_type_param(source_instantiated.return_type)
        };

        // When both sides are generic but have different type parameter counts,
        // erase both signatures by replacing type params with their constraints
        // (or `unknown` if unconstrained). This matches tsc's `getCanonicalSignature`
        // behavior in `signatureRelatedTo` when `eraseGenerics` is true.
        // Example: `<T, U>(x: T, y: U) => void` vs `<T>(x: T, y: T) => void`
        //   → erased: `(x: unknown, y: unknown) => void` vs `(x: unknown, y: unknown) => void`
        if !source_instantiated.type_params.is_empty()
            && !target_instantiated.type_params.is_empty()
            && source_instantiated.type_params.len() != target_instantiated.type_params.len()
        {
            if !self.erase_generics && source_mentions_nonlocal_type_params {
                // Strict member-compatibility checks must not erase away the distinction
                // between a source signature's own type parameters and type parameters it
                // captured from an outer declaration. Otherwise signatures like
                //   `<U>(x: T, y: U) => string`
                // are incorrectly accepted as subtypes of
                //   `<T, U>(x: T, y: U) => string`
                // during TS2416/TS2430 comparison.
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            }

            if self.has_conflicting_contextual_param_candidates(
                &source_instantiated,
                &target_instantiated,
            ) {
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            }

            if let Ok(substitution) = self
                .infer_source_type_param_substitution(&source_instantiated, &target_instantiated)
            {
                let inferred_source =
                    self.instantiate_function_shape(&source_instantiated, &substitution);
                let result = self.check_function_subtype_impl(
                    &inferred_source,
                    &target_instantiated,
                    allow_constructor_bivariance,
                );
                if result.is_true() {
                    self.type_param_equivalences.truncate(equiv_start);
                    return result;
                }
                if !self.allow_erased_generic_signature_retry {
                    self.type_param_equivalences.truncate(equiv_start);
                    return result;
                }
            }

            let source_canonical =
                erase_type_params_to_constraints(&source_instantiated.type_params);
            source_instantiated =
                self.instantiate_function_shape(&source_instantiated, &source_canonical);

            let target_canonical =
                erase_type_params_to_constraints(&target_instantiated.type_params);
            target_instantiated =
                self.instantiate_function_shape(&target_instantiated, &target_canonical);
        }

        // Contextual signature instantiation for generic source -> non-generic target.
        // This is key for non-strict assignability cases where a generic function expression
        // is contextually typed by a concrete callback/function type.
        //
        // Two strategies exist and we try inference first (needed for contextual callback
        // typing where return types must be precisely inferred), then fall back to tsc's
        // `getErasedSignature` (constraint erasure) if the inference-based comparison fails.
        // This fallback is essential for interface-extends checks (TS2430) where inference
        // over-constrains by intersecting inferred types with constraints.
        let mut used_inference_for_generic_source = false;
        let source_before_generic_instantiation = if !source_instantiated.type_params.is_empty()
            && target_instantiated.type_params.is_empty()
        {
            Some(source_instantiated.clone())
        } else {
            None
        };
        if !source_instantiated.type_params.is_empty() && target_instantiated.type_params.is_empty()
        {
            // When a generic callback is inferred as an argument (e.g., `fn(function<T>(a: Foo<T>) {})`),
            // the outer function's type parameter (e.g., `Args`) gets inferred as a tuple containing
            // the callback's own type parameter TypeIds (e.g., `[Foo<T>, T]`). The target signature
            // is then instantiated with these inferred types, making it non-generic but containing
            // the source's type parameter TypeIds. In this case, the source and target already share
            // the same type parameter identity — no erasure or inference is needed; just clear the
            // source type params so structural comparison proceeds with matching TypeIds.
            let source_tp_ids: Vec<TypeId> = source_instantiated
                .type_params
                .iter()
                .map(|tp| self.interner.type_param(*tp))
                .collect();
            let target_refs_source_params = target_instantiated.params.iter().any(|p| {
                source_tp_ids.contains(&p.type_id)
                    || source_tp_ids.iter().any(|&tp_id| {
                        crate::visitor::collect_all_types(self.interner, p.type_id).contains(&tp_id)
                    })
            }) || source_tp_ids.iter().any(|&tp_id| {
                crate::visitor::collect_all_types(self.interner, target_instantiated.return_type)
                    .contains(&tp_id)
            });

            if target_refs_source_params {
                // Target references source's type params — they share identity.
                // Just clear source type params; no instantiation needed.
                source_instantiated.type_params.clear();
            } else {
                if self.has_conflicting_contextual_param_candidates(
                    &source_instantiated,
                    &target_instantiated,
                ) {
                    return SubtypeResult::False;
                }
                let substitution = match self.infer_source_type_param_substitution(
                    &source_instantiated,
                    &target_instantiated,
                ) {
                    Ok(sub) => {
                        used_inference_for_generic_source = true;
                        sub
                    }
                    Err(_) => {
                        // Inference failed (e.g., bounds violation). Fall back to tsc's
                        // `getErasedSignature` behavior: replace type params with their
                        // constraints (or `unknown` if unconstrained).
                        erase_type_params_to_constraints(&source_instantiated.type_params)
                    }
                };
                source_instantiated =
                    self.instantiate_function_shape(&source_instantiated, &substitution);
            }
        }

        // Non-generic source → generic target: check if the source references the same
        // TypeParam TypeIds as the target's bound type parameters. This happens when
        // contextual type seeding resolves inference variables to the contextual type's
        // bound TypeParams (e.g., `wrap(list)` produces `(a: A) => A[]` where A is the
        // same TypeParam as in the contextual type `<A>(x: A) => A[]`).
        // In this case, treat the source as effectively generic with the same type params.
        // Otherwise, fall back to erasing target type params to constraints.
        if source_instantiated.type_params.is_empty() && !target_instantiated.type_params.is_empty()
        {
            let target_tp_ids: Vec<TypeId> = target_instantiated
                .type_params
                .iter()
                .map(|tp| self.interner.type_param(*tp))
                .collect();
            let source_refs_target_params = source_instantiated.params.iter().any(|p| {
                target_tp_ids.contains(&p.type_id)
                    || target_tp_ids.iter().any(|&tp_id| {
                        crate::visitor::collect_all_types(self.interner, p.type_id).contains(&tp_id)
                    })
            }) || target_tp_ids.iter().any(|&tp_id| {
                crate::visitor::collect_all_types(self.interner, source_instantiated.return_type)
                    .contains(&tp_id)
            });

            let source_mentions_outer_type_params = source_instantiated
                .params
                .iter()
                .any(|p| crate::visitor::contains_type_parameters(self.interner, p.type_id))
                || source_instantiated.this_type.is_some_and(|this_type| {
                    crate::visitor::contains_type_parameters(self.interner, this_type)
                })
                || crate::visitor::contains_type_parameters(
                    self.interner,
                    source_instantiated.return_type,
                );

            if source_refs_target_params {
                if !self.erase_generics {
                    // In strict member-compatibility checks (TS2416/TS2430), a
                    // non-generic source must never be promoted to "effectively
                    // generic", even when it appears to reference the target's
                    // type-parameter identities. That identity-sharing can arise
                    // from contextual seeding and would incorrectly accept concrete
                    // members as subtypes of universally quantified ones, e.g.:
                    //   `(x: T) => T[]` <= `<U>(x: U) => U[]`
                    //   `new (x: T) => T[]` <= `new <U>(x: U) => U[]`
                    self.type_param_equivalences.truncate(equiv_start);
                    return SubtypeResult::False;
                }
                // Source references target's bound TypeParams — promote source to generic
                // and use the same-arity alpha-renaming path above
                source_instantiated.type_params = target_instantiated.type_params.clone();
                // Both now have the same type params with the same TypeIds, so
                // alpha-renaming is an identity operation and structural comparison
                // will match correctly.
                target_instantiated.type_params.clear();
                source_instantiated.type_params.clear();
            } else if !self.erase_generics {
                if source_mentions_outer_type_params {
                    // Strict member-compatibility checks (TS2416/TS2430) must reject a
                    // non-generic source that only works for some outer-scope type
                    // parameter when the target is genuinely generic. Otherwise shapes like
                    // `new (x: T) => T[]` are incorrectly accepted as subtypes of
                    // `new <U>(x: U) => U[]` in interface/base compatibility.
                    self.type_param_equivalences.truncate(equiv_start);
                    return SubtypeResult::False;
                }

                // When erase_generics is false (strict mode, used for implements/extends
                // member type checking), a non-generic function is NOT assignable to a
                // generic function. This matches tsc's compareSignaturesRelated with
                // eraseGenerics=false: the comparison proceeds with raw TypeParameter
                // types in the target, and the SubtypeChecker rejects concrete types
                // against opaque type parameters (e.g., string ≤ T returns False).
                // This ensures TS2416 is correctly emitted for incompatible overrides.
                target_instantiated.type_params.clear();
            } else {
                // erase_generics=true path: erase target type params to their
                // constraints so a concrete source signature can match the target's
                // structural shape through constraint-erasure (tsc's
                // `getErasedSignature` behavior). This is what single-signature
                // assignability uses for base-type structural compatibility checks.
                let target_canonical =
                    erase_type_params_to_constraints(&target_instantiated.type_params);
                target_instantiated =
                    self.instantiate_function_shape(&target_instantiated, &target_canonical);
            }
        }

        self.normalize_rest_param_types(&mut source_instantiated);
        self.normalize_rest_param_types(&mut target_instantiated);

        // When both functions have no type parameters but their return types
        // contain type parameters, we need to ensure those type parameters are
        // properly compared. This handles cases like:
        //   () => T  vs  () => U  (T and U are different type parameters)
        // where T should NOT be assignable to U.
        if source_instantiated.type_params.is_empty() && target_instantiated.type_params.is_empty()
        {
            // Check if return types contain type parameters that need explicit comparison
            let s_return = source_instantiated.return_type;
            let t_return = target_instantiated.return_type;

            // If return types are different function types, check their return types too
            if let Some(s_shape) = callable_shape_id(self.interner, s_return)
                && let Some(t_shape) = callable_shape_id(self.interner, t_return)
            {
                let s_callable = self.interner.callable_shape(s_shape);
                let t_callable = self.interner.callable_shape(t_shape);

                // Get the first call signature from each callable (if any)
                if let (Some(s_sig), Some(t_sig)) = (
                    s_callable.call_signatures.first(),
                    t_callable.call_signatures.first(),
                ) {
                    // If both inner functions also have no type params, check their returns
                    if s_sig.type_params.is_empty() && t_sig.type_params.is_empty() {
                        let s_inner_return = s_sig.return_type;
                        let t_inner_return = t_sig.return_type;

                        // Check if both inner returns are type parameters
                        if let Some(s_tp) = type_param_info(self.interner, s_inner_return)
                            && let Some(t_tp) = type_param_info(self.interner, t_inner_return)
                        {
                            // Different type parameters should not be assignable
                            if s_tp.name != t_tp.name {
                                // Check if there's a constraint relationship
                                let s_constrained_to_t = s_tp.constraint == Some(t_inner_return);
                                let t_constrained_to_s = t_tp.constraint == Some(s_inner_return);

                                if !s_constrained_to_t && !t_constrained_to_s {
                                    // Different unconstrained type parameters - not assignable
                                    self.type_param_equivalences.truncate(equiv_start);
                                    return SubtypeResult::False;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Return type is covariant
        let return_result = self.check_return_compat(
            source_instantiated.return_type,
            target_instantiated.return_type,
        );
        if !return_result.is_true() {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        if !self.are_this_parameters_compatible(
            source_instantiated.this_type,
            target_instantiated.this_type,
        ) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        // Type predicates check
        if !self.are_type_predicates_compatible(&source_instantiated, &target_instantiated) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        // Method/constructor bivariance: strictFunctionTypes only applies to function
        // type literals, not to methods or construct signatures (new (...) => T).
        let constructor_param_bivariance = allow_constructor_bivariance
            && (source_instantiated.is_constructor || target_instantiated.is_constructor);
        let is_method = source_instantiated.is_method
            || target_instantiated.is_method
            || constructor_param_bivariance;

        // The lib iterator/generator declarations encode `next(value?)` as a single
        // rest parameter with tuple-list type `[] | [TNext]`. Compare that whole
        // tuple-list type directly before the generic rest-element machinery kicks in;
        // otherwise we lose the contravariant relation between the tuple variants and
        // incorrectly accept incompatible `TNext` values.
        if let (Some(s_param), Some(t_param)) = (
            source_instantiated.params.first(),
            target_instantiated.params.first(),
        ) && source_instantiated.params.len() == 1
            && target_instantiated.params.len() == 1
            && s_param.rest
            && t_param.rest
            && self.is_tuple_list_rest_type(s_param.type_id)
            && self.is_tuple_list_rest_type(t_param.type_id)
        {
            self.type_param_equivalences.truncate(equiv_start);
            return if self.are_parameters_compatible_impl(
                s_param.type_id,
                t_param.type_id,
                is_method,
            ) {
                SubtypeResult::True
            } else {
                SubtypeResult::False
            };
        }

        // Unpack tuple rest parameters before comparison.
        // In TypeScript, `(...args: [A, B]) => R` is equivalent to `(a: A, b: B) => R`.
        // We unpack tuple rest parameters into individual fixed parameters for proper matching.
        // Before unpacking, evaluate Application types in rest params (e.g., MappedType<T>
        // that evaluates to a tuple) so unpack_tuple_rest_parameter can detect the tuple.
        use crate::type_queries::unpack_tuple_rest_parameter;
        let source_params_unpacked: Vec<ParamInfo> = source_instantiated
            .params
            .iter()
            .flat_map(|p| {
                if p.rest
                    && matches!(
                        self.interner.lookup(p.type_id),
                        Some(TypeData::Application(_))
                    )
                {
                    let evaluated = self.evaluate_type(p.type_id);
                    if evaluated != p.type_id {
                        let mut ep = *p;
                        ep.type_id = evaluated;
                        return unpack_tuple_rest_parameter(self.interner, &ep);
                    }
                }
                unpack_tuple_rest_parameter(self.interner, p)
            })
            .collect();
        let target_params_unpacked: Vec<ParamInfo> = target_instantiated
            .params
            .iter()
            .flat_map(|p| {
                if p.rest
                    && matches!(
                        self.interner.lookup(p.type_id),
                        Some(TypeData::Application(_))
                    )
                {
                    let evaluated = self.evaluate_type(p.type_id);
                    if evaluated != p.type_id {
                        let mut ep = *p;
                        ep.type_id = evaluated;
                        return unpack_tuple_rest_parameter(self.interner, &ep);
                    }
                }
                unpack_tuple_rest_parameter(self.interner, p)
            })
            .collect();

        if source_params_unpacked.len() == target_params_unpacked.len()
            && source_params_unpacked
                .iter()
                .zip(target_params_unpacked.iter())
                .all(|(source_param, target_param)| {
                    source_param.type_id == target_param.type_id
                        && source_param.optional == target_param.optional
                        && source_param.rest == target_param.rest
                })
        {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::True;
        }

        // Handle union-of-tuple rest parameters in target.
        // When target has `...args: [A] | [B, C] | [D]`, try each union member separately.
        // Source matches if its params are compatible with ANY of the union member tuple shapes.
        // This handles patterns like:
        //   interface I { set(...args: [Record<string, unknown>] | [string, unknown]): void }
        //   class C implements I { set(option: Record<string, unknown>): void; set(name: string, value: unknown): void; }
        if let Some(last_target_param) = target_instantiated.params.last()
            && last_target_param.rest
        {
            use crate::type_queries::data::get_union_members;
            if let Some(union_members) = get_union_members(self.interner, last_target_param.type_id)
            {
                // Get non-rest prefix params from target
                let prefix_count = target_params_unpacked.len().saturating_sub(1);
                let prefix_params: &[ParamInfo] = &target_params_unpacked[..prefix_count];

                let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
                for member_type_id in &union_members {
                    // When the union member is a readonly tuple and the source has
                    // individual (non-rest) parameters (forming a mutable tuple),
                    // the readonly tuple cannot be assigned to the mutable param tuple
                    // under contravariance.  Skip this member — it cannot match.
                    // This mirrors tsc's behavior where `readonly [A, B]` is not
                    // assignable to `[A, B]`.
                    if !source_has_rest
                        && matches!(
                            self.interner.lookup(*member_type_id),
                            Some(TypeData::ReadonlyType(_))
                        )
                    {
                        continue;
                    }

                    // Try unpacking this union member as a tuple
                    let member_param = ParamInfo {
                        type_id: *member_type_id,
                        rest: true,
                        ..*last_target_param
                    };
                    let member_unpacked = unpack_tuple_rest_parameter(self.interner, &member_param);

                    // Build full param list for this variant
                    let mut variant_params: Vec<ParamInfo> = prefix_params.to_vec();
                    variant_params.extend(member_unpacked);

                    // Try the comparison with this variant
                    if self
                        .check_params_compatible(
                            &source_params_unpacked,
                            &variant_params,
                            is_method,
                        )
                        .is_true()
                    {
                        self.type_param_equivalences.truncate(equiv_start);
                        return SubtypeResult::True;
                    }
                }
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            }
        }

        // Check rest parameter handling (after unpacking)
        let target_has_rest = target_params_unpacked.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params_unpacked
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        // Count non-rest parameters (needed for arity check below)
        let target_fixed_count = if target_has_rest {
            target_params_unpacked.len().saturating_sub(1)
        } else {
            target_params_unpacked.len()
        };
        let source_fixed_count = if source_has_rest {
            source_params_unpacked.len().saturating_sub(1)
        } else {
            source_params_unpacked.len()
        };

        // Check parameter arity: source's required params must not exceed
        // the target's total non-rest params (including optional ones).
        // When target has a rest parameter, skip the arity check entirely —
        // the rest parameter can accept any number of arguments, and type
        // compatibility of extra params is checked later against the rest element type.
        //
        // Special case: parameters of type `void` are effectively optional in TypeScript.
        // A function like `(a: void) => void` is assignable to `() => void` because
        // void parameters can be called without arguments.
        let source_required = self.required_param_count(&source_params_unpacked);
        let target_rest_min_required = if target_has_rest {
            target_params_unpacked
                .last()
                .map(|param| self.rest_param_min_required_arg_count(param.type_id))
                .unwrap_or(0)
        } else {
            0
        };
        let guard_target_rest_arity = target_has_rest
            && target_params_unpacked
                .last()
                .is_some_and(|param| self.rest_param_needs_min_arity_guard(param.type_id));
        if (!target_has_rest || guard_target_rest_arity)
            && source_required
                > target_fixed_count
                    + if target_has_rest {
                        target_rest_min_required
                    } else {
                        0
                    }
        {
            let extra_are_void = source_params_unpacked
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count + target_rest_min_required))
                .all(|param| self.param_type_contains_void(param.type_id));
            if !extra_are_void {
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            }
        }

        // Check parameter types
        let result = (|| -> SubtypeResult {
            // Compare fixed parameters (using unpacked params)
            let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
            for i in 0..fixed_compare_count {
                let s_param = &source_params_unpacked[i];
                let t_param = &target_params_unpacked[i];

                // Compute effective parameter types, matching tsc's `getTypeAtPosition`:
                // optional parameters are widened to `T | undefined` under strictNullChecks.
                // When both parameters are optional, strip `undefined` so that
                // `(x?: T)` and `(x?: T | undefined)` compare as equivalent.
                let (s_effective, t_effective) = self.effective_param_type_pair(s_param, t_param);
                if !self.are_parameters_compatible_impl(s_effective, t_effective, is_method) {
                    // Trace: Parameter type mismatch
                    if let Some(tracer) = &mut self.tracer
                        && !tracer.on_mismatch_dyn(
                            crate::diagnostics::SubtypeFailureReason::ParameterTypeMismatch {
                                param_index: i,
                                source_param: s_effective,
                                target_param: t_effective,
                            },
                        )
                    {
                        return SubtypeResult::False;
                    }
                    return SubtypeResult::False;
                }
            }

            // If target has rest parameter, check source's extra params against the rest type
            if target_has_rest {
                let Some(rest_elem_type) = rest_elem_type else {
                    return SubtypeResult::False;
                };
                if rest_is_top {
                    if self
                        .first_top_rest_unassignable_source_param(&source_params_unpacked)
                        .is_some()
                    {
                        return SubtypeResult::False;
                    }
                    return SubtypeResult::True;
                }

                for s_param in source_params_unpacked
                    .iter()
                    .skip(target_fixed_count)
                    .take(source_fixed_count.saturating_sub(target_fixed_count))
                {
                    if !self.are_parameters_compatible_impl(
                        s_param.type_id,
                        rest_elem_type,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }

                if source_has_rest {
                    let Some(s_rest_param) = source_params_unpacked.last() else {
                        return SubtypeResult::False;
                    };

                    // After unpacking, tuple rest parameters are already expanded into fixed params.
                    // Only non-tuple rest parameters (like ...args: string[]) remain as rest.
                    // Check the rest element type against target's rest element type.
                    let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                    if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method)
                    {
                        return SubtypeResult::False;
                    }
                }
            }

            if source_has_rest {
                let Some(rest_param) = source_params_unpacked.last() else {
                    return SubtypeResult::False;
                };
                let rest_elem_type = self.get_array_element_type(rest_param.type_id);
                let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

                if !rest_is_top {
                    for t_param in target_params_unpacked
                        .iter()
                        .skip(source_fixed_count)
                        .take(target_fixed_count.saturating_sub(source_fixed_count))
                    {
                        if !self.are_parameters_compatible_impl(
                            rest_elem_type,
                            t_param.type_id,
                            is_method,
                        ) {
                            return SubtypeResult::False;
                        }
                    }
                }
            }

            SubtypeResult::True
        })();

        // If the inference-based comparison failed and we used inference for the
        // generic source → non-generic target case, retry with constraint erasure.
        // This matches tsc's `getErasedSignature` behavior for interface extension
        // checks (TS2430) where inference over-constrains type parameters by
        // intersecting inferred types with their constraints.
        let source_before_has_mapped_type_param_context = source_before_generic_instantiation
            .as_ref()
            .is_some_and(|source_before| {
                source_before.type_params.iter().any(|tp| {
                    source_before.params.iter().any(|param| {
                        self.type_param_appears_in_mapped_context(param.type_id, tp.name)
                    }) || source_before.this_type.is_some_and(|this_type| {
                        self.type_param_appears_in_mapped_context(this_type, tp.name)
                    }) || self
                        .type_param_appears_in_mapped_context(source_before.return_type, tp.name)
                })
            });
        if !result.is_true()
            && used_inference_for_generic_source
            && !source_before_has_mapped_type_param_context
            && let Some(source_before) = source_before_generic_instantiation
        {
            let erasure_sub = erase_type_params_to_constraints(&source_before.type_params);
            let erased_source = self.instantiate_function_shape(&source_before, &erasure_sub);
            let retry = self.check_function_subtype(&erased_source, &target_instantiated);
            self.type_param_equivalences.truncate(equiv_start);
            return retry;
        }

        // Clean up type parameter equivalences established in this scope.
        self.type_param_equivalences.truncate(equiv_start);
        result
    }

    fn is_tuple_list_rest_type(&mut self, type_id: TypeId) -> bool {
        use crate::type_queries::{get_tuple_elements, union_contains_tuple};

        get_tuple_elements(self.interner, type_id).is_some()
            || union_contains_tuple(self.interner, type_id)
    }

    /// Check if a single function type is a subtype of a callable type with overloads.
    pub(crate) fn check_function_to_callable_subtype(
        &mut self,
        s_fn_id: FunctionShapeId,
        t_callable_id: CallableShapeId,
    ) -> SubtypeResult {
        let s_fn = self.interner.function_shape(s_fn_id);
        let t_callable = self.interner.callable_shape(t_callable_id);

        let has_multiple_target_sigs = t_callable.call_signatures.len() > 1;

        for t_sig in &t_callable.call_signatures {
            if s_fn.is_constructor {
                return SubtypeResult::False;
            }
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
                // tsc N×M path: when the target has multiple call signatures, try
                // erasing type params to `any` before rejecting. This matches tsc's
                // `signaturesRelatedTo` which uses `erase = true` for the N×M case.
                if has_multiple_target_sigs {
                    if !self.check_erased_fn_subtype_to_sig(&s_fn, t_sig).is_true() {
                        return SubtypeResult::False;
                    }
                } else {
                    return SubtypeResult::False;
                }
            }
        }

        for t_sig in &t_callable.construct_signatures {
            if !s_fn.is_constructor {
                return SubtypeResult::False;
            }
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
                return SubtypeResult::False;
            }
        }

        // Check properties: a plain function has no user-defined properties,
        // so if the target callable has non-optional properties (e.g., from a
        // namespace merge), the function is NOT a subtype. This matches tsc's
        // behavior where `typeof Point` (function + namespace exports) is not
        // assignable to a bare function type.
        let should_skip_prop = |name: crate::intern::Atom| {
            let resolved = self.interner.resolve_atom(name);
            resolved.starts_with('#')
        };
        let target_props: Vec<_> = t_callable
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        if !target_props.is_empty() {
            // The function type has no properties to match against the target's
            // required properties. Delegate to check_object_subtype with an
            // empty source shape to properly handle optional vs required props.
            let source_shape = ObjectShape {
                flags: ObjectFlags::empty(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
            };
            let target_shape = ObjectShape {
                flags: ObjectFlags::empty(),
                properties: target_props,
                string_index: t_callable.string_index,
                number_index: t_callable.number_index,
                symbol: t_callable.symbol,
            };
            if !self
                .check_object_subtype(&source_shape, None, None, &target_shape, None)
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check if an overloaded callable type is a subtype of a single function type.
    pub(crate) fn check_callable_to_function_subtype(
        &mut self,
        s_callable_id: CallableShapeId,
        t_fn_id: FunctionShapeId,
    ) -> SubtypeResult {
        let s_callable = self.interner.callable_shape(s_callable_id);
        let t_fn = self.interner.function_shape(t_fn_id);

        if t_fn.is_constructor {
            let has_multiple_source_construct_sigs = s_callable.construct_signatures.len() > 1;
            for s_sig in &s_callable.construct_signatures {
                let direct = self
                    .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                    .is_true();
                if direct {
                    return SubtypeResult::True;
                }
            }

            // tsc N×M path: when the source has multiple constructor signatures,
            // retry by erasing type parameters to `any`.
            if has_multiple_source_construct_sigs {
                for s_sig in &s_callable.construct_signatures {
                    let erased = self
                        .check_erased_signature_subtype_to_fn(s_sig, &t_fn)
                        .is_true();
                    if erased {
                        return SubtypeResult::True;
                    }
                }
            }
            return SubtypeResult::False;
        }

        if s_callable.call_signatures.is_empty() {
            return SubtypeResult::False;
        }

        // Check source call signatures against the target function.
        // A single compatible source signature is enough to establish the relation.
        for s_sig in &s_callable.call_signatures {
            if self
                .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                .is_true()
            {
                return SubtypeResult::True;
            }

            if !s_sig.type_params.is_empty()
                && t_fn.type_params.is_empty()
                && self
                    .try_instantiate_generic_callable_to_function(s_sig, &t_fn)
                    .is_true()
            {
                return SubtypeResult::True;
            }
        }

        // tsc N×M path: when a callable has multiple signatures and the direct
        // comparison above fails, try erasing type parameters to `any`
        // comparison above fails, try erasing type parameters to `any`
        // (matching tsc's `getErasedSignature` / `createTypeEraser`). In tsc's
        // `signaturesRelatedTo`, the N×M case (source.length > 1 || target.length > 1)
        // always uses `erase = true`, which maps type params to `any`. This allows
        // overloaded callables with constrained generics (e.g., `{ <T extends A>(x: T): T;
        // <T extends B>(x: T): T }`) to be assignable to unconstrained generic functions
        // (e.g., `<T>(x: T) => T`), because after erasure both become `(x: any) => any`.
        if s_callable.call_signatures.len() > 1 {
            for s_sig in &s_callable.call_signatures {
                if self
                    .check_erased_signature_subtype_to_fn(s_sig, &t_fn)
                    .is_true()
                {
                    return SubtypeResult::True;
                }
            }
        }

        SubtypeResult::False
    }

    /// Compare a function type against a call signature after erasing both signatures'
    /// type parameters to `any`. Matches tsc's N×M `signaturesRelatedTo` path.
    fn check_erased_fn_subtype_to_sig(
        &mut self,
        s_fn: &FunctionShape,
        t_sig: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_fn_shape_to_any(s_fn, self.interner);
        let t_erased = erase_call_sig_to_any(t_sig, self.interner);
        self.check_function_subtype(&s_erased, &t_erased)
    }

    /// Compare a call signature against a function type after erasing both signatures'
    /// type parameters to `any`. Matches tsc's N×M `signaturesRelatedTo` path.
    fn check_erased_signature_subtype_to_fn(
        &mut self,
        s_sig: &CallSignature,
        t_fn: &FunctionShape,
    ) -> SubtypeResult {
        let mut s_erased = erase_call_sig_to_any(s_sig, self.interner);
        // Preserve constructor-vs-callable intent from the target function shape.
        // `erase_call_sig_to_any` always returns `is_constructor = false`, which
        // would immediately fail `check_function_subtype` on constructor targets.
        s_erased.is_constructor = t_fn.is_constructor;
        let t_erased = erase_fn_shape_to_any(t_fn, self.interner);
        self.check_function_subtype(&s_erased, &t_erased)
    }

    /// Try to instantiate a generic callable signature to match a concrete function type.
    /// This handles cases like: `declare function box<V>(x: V): {value: V}; const f: (x: number) => {value: number} = box;`
    fn try_instantiate_generic_callable_to_function(
        &mut self,
        s_sig: &crate::types::CallSignature,
        t_fn: &crate::types::FunctionShape,
    ) -> SubtypeResult {
        use crate::TypeData;
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

        // Create a substitution mapping type parameters to the target's parameter types
        // This is a simplified instantiation - we map each source type param to the corresponding target param type
        let mut substitution = TypeSubstitution::new();

        // For a simple case like <V>(x: V) => R vs (x: T) => S, map V to T
        // This handles the common case where type parameters flow through from parameters to return type
        for (s_param, t_param) in s_sig.params.iter().zip(t_fn.params.iter()) {
            // If source param is a type parameter, map it to target param type
            if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(s_param.type_id) {
                substitution.insert(tp.name, t_param.type_id);
            }
        }

        // If we couldn't infer any type parameters, fall back to checking with unknown
        // This handles cases where type params aren't directly in parameters
        if substitution.is_empty() {
            for tp in &s_sig.type_params {
                substitution.insert(tp.name, crate::TypeId::UNKNOWN);
            }
        }

        // Instantiate the source signature
        let instantiated_params: Vec<_> = s_sig
            .params
            .iter()
            .map(|p| crate::types::ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        let instantiated_return = instantiate_type(self.interner, s_sig.return_type, &substitution);

        let instantiated_sig = crate::types::CallSignature {
            type_params: Vec::new(), // No type params after instantiation
            params: instantiated_params,
            this_type: s_sig.this_type,
            return_type: instantiated_return,
            type_predicate: s_sig.type_predicate,
            is_method: s_sig.is_method,
        };

        // Check if instantiated signature is compatible with target
        self.check_call_signature_subtype_to_fn(&instantiated_sig, t_fn)
    }

    /// Check callable subtyping with overloaded signatures.
    pub(crate) fn check_callable_subtype(
        &mut self,
        source: &CallableShape,
        target: &CallableShape,
    ) -> SubtypeResult {
        // For each target call signature, at least one source call signature must match.
        // Unlike call-site overload resolution (which uses only the implementation/last
        // signature), structural subtype checking uses ALL source signatures — matching
        // tsc's signaturesRelatedTo N×M comparison.
        let is_multi_sig = source.call_signatures.len() > 1 || target.call_signatures.len() > 1;
        for t_sig in &target.call_signatures {
            let mut found_match = false;
            for s_sig in &source.call_signatures {
                if self.check_call_signature_subtype(s_sig, t_sig).is_true() {
                    found_match = true;
                    break;
                }
            }
            // tsc N×M path: when either side has multiple signatures, try erasing
            // type params to `any` (matching tsc's `getErasedSignature` behavior).
            if !found_match && is_multi_sig {
                for s_sig in &source.call_signatures {
                    if self
                        .check_erased_call_signature_subtype(s_sig, t_sig)
                        .is_true()
                    {
                        found_match = true;
                        break;
                    }
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // For each target construct signature, at least one source signature must match.
        // Callable-object construct signatures come from property values such as
        // `{ ctor: new <T>(x: T) => T }`, not from method syntax, so they should
        // follow the regular property-function relation instead of method-style
        // bivariance. Standalone constructor function types still flow through
        // `check_function_subtype` with `is_constructor = true`.
        for t_sig in &target.construct_signatures {
            let mut found_match = false;
            for s_sig in &source.construct_signatures {
                let result = self.check_call_signature_subtype_as_constructor(s_sig, t_sig);
                if result.is_true() {
                    found_match = true;
                    break;
                }
            }
            if !found_match
                && (source.construct_signatures.len() > 1 || target.construct_signatures.len() > 1)
            {
                for s_sig in &source.construct_signatures {
                    if self
                        .check_erased_call_signature_subtype_as_constructor(s_sig, t_sig)
                        .is_true()
                    {
                        found_match = true;
                        break;
                    }
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // Check properties (if any), excluding private fields.
        // Sort by name (Atom) to match the merge scan's expectation in check_object_subtype.
        //
        // When both callables have construct signatures (class constructors), skip the
        // `prototype` property. Its type is the instance type which is already validated
        // by construct signature compatibility — checking it separately can fail when
        // the target has generic type params that were erased only at the signature level.
        let has_construct_sigs =
            !source.construct_signatures.is_empty() && !target.construct_signatures.is_empty();
        let should_skip_prop = |name| {
            let resolved = self.interner.resolve_atom(name);
            resolved.starts_with('#') || (has_construct_sigs && resolved == "prototype")
        };
        let mut source_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        // Function-like sources (with call signatures) are expected to have Function members
        // such as `call` and `apply`, even if those properties are not materialized on the
        // callable shape. Add synthetic members to align assignability behavior.
        if !source.call_signatures.is_empty() {
            for t_prop in &target.properties {
                let prop_name = self.interner.resolve_atom(t_prop.name);
                if (prop_name == "call" || prop_name == "apply")
                    && !source_props.iter().any(|p| p.name == t_prop.name)
                {
                    source_props.push(PropertyInfo {
                        name: t_prop.name,
                        type_id: t_prop.type_id,
                        write_type: t_prop.write_type,
                        optional: false,
                        readonly: false,
                        is_method: true,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                        is_string_named: false,
                    });
                }
            }
        }
        source_props.sort_by_key(|a| a.name);
        let mut target_props: Vec<_> = target
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        target_props.sort_by_key(|a| a.name);
        // Create temporary ObjectShape instances for the property check
        let source_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: source_props,
            string_index: source.string_index,
            number_index: source.number_index,
            symbol: source.symbol,
        };
        let target_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: target_props,
            string_index: target.string_index,
            number_index: target.number_index,
            symbol: target.symbol,
        };
        if !self
            .check_object_subtype(&source_shape, None, None, &target_shape, None)
            .is_true()
        {
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    /// Check call signature subtyping.
    pub(crate) fn check_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, false)
    }

    /// Compare two call signatures after erasing both signatures' type parameters
    /// to `any`. Used in the N×M callable subtype path to match tsc's behavior.
    fn check_erased_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        let s_erased = erase_call_sig_to_any(source, self.interner);
        let t_erased = erase_call_sig_to_any(target, self.interner);
        self.check_function_subtype(&s_erased, &t_erased)
    }

    /// Compare constructor signatures after erasing type parameters to `any`.
    /// Used in N×M constructor-signature comparison to match tsc behavior.
    fn check_erased_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        // Guard against over-erasing higher-order callable modality differences.
        // If a source parameter is constructor-like while the target parameter is
        // call-like (or vice versa), erasing both sides to `any` would mask a real
        // incompatibility (e.g. `{ new (...) }` vs `(x) => ...`).
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
        self.check_function_subtype(&s_erased, &t_erased)
    }

    pub(crate) fn callable_modality_flags_for_type(&self, type_id: TypeId) -> (bool, bool) {
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

    fn constructor_signatures_need_strict_params(
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
            // constructor bivariance. Without this, the bivariant check
            // would pass (`number <: number | undefined`) and incorrectly
            // allow `new (x: number) => number` as a subtype.
            let has_optionality_mismatch = source
                .params
                .iter()
                .zip(target.params.iter())
                .any(|(sp, tp)| sp.optional != tp.optional);
            return has_optionality_mismatch;
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

    /// Check if source params are compatible with target params.
    /// Extracted to support union-of-tuple rest parameter handling,
    /// where we need to try multiple target param variants.
    fn check_params_compatible(
        &mut self,
        source_params: &[ParamInfo],
        target_params: &[ParamInfo],
        is_method: bool,
    ) -> SubtypeResult {
        let target_has_rest = target_params.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params.last().is_some_and(|p| p.rest);
        let rest_elem_type = if target_has_rest {
            target_params
                .last()
                .map(|param| self.get_array_element_type(param.type_id))
        } else {
            None
        };
        let rest_is_top = self.allow_bivariant_rest
            && matches!(rest_elem_type, Some(TypeId::ANY | TypeId::UNKNOWN));

        let target_fixed_count = if target_has_rest {
            target_params.len().saturating_sub(1)
        } else {
            target_params.len()
        };
        let source_fixed_count = if source_has_rest {
            source_params.len().saturating_sub(1)
        } else {
            source_params.len()
        };

        let source_required = self.required_param_count(source_params);
        let target_rest_min_required = if target_has_rest {
            target_params
                .last()
                .map(|param| self.rest_param_min_required_arg_count(param.type_id))
                .unwrap_or(0)
        } else {
            0
        };
        let guard_target_rest_arity = target_has_rest
            && target_params
                .last()
                .is_some_and(|param| self.rest_param_needs_min_arity_guard(param.type_id));
        if (!target_has_rest || guard_target_rest_arity)
            && source_required
                > target_fixed_count
                    + if target_has_rest {
                        target_rest_min_required
                    } else {
                        0
                    }
        {
            let extra_are_void = source_params
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count + target_rest_min_required))
                .all(|param| self.param_type_contains_void(param.type_id));
            if !extra_are_void {
                return SubtypeResult::False;
            }
        }

        // Compare fixed parameters
        let fixed_compare_count = std::cmp::min(source_fixed_count, target_fixed_count);
        for i in 0..fixed_compare_count {
            let s_param = &source_params[i];
            let t_param = &target_params[i];

            // Compute effective types — optional params widened to `T | undefined`
            // under strictNullChecks (matching tsc's `getTypeAtPosition`).
            // When both parameters are optional, strip `undefined` so
            // `(x?: T)` and `(x?: T | undefined)` compare as equivalent.
            let (s_effective, t_effective) = self.effective_param_type_pair(s_param, t_param);
            if !self.are_parameters_compatible_impl(s_effective, t_effective, is_method) {
                return SubtypeResult::False;
            }
        }

        // If target has rest parameter, check source's extra params against the rest type
        if target_has_rest {
            let Some(rest_elem_type) = rest_elem_type else {
                return SubtypeResult::False;
            };
            if rest_is_top {
                return SubtypeResult::True;
            }

            for s_param in source_params
                .iter()
                .skip(target_fixed_count)
                .take(source_fixed_count.saturating_sub(target_fixed_count))
            {
                if !self.are_parameters_compatible_impl(s_param.type_id, rest_elem_type, is_method)
                {
                    return SubtypeResult::False;
                }
            }

            if source_has_rest {
                let Some(s_rest_param) = source_params.last() else {
                    return SubtypeResult::False;
                };
                let s_rest_elem = self.get_array_element_type(s_rest_param.type_id);
                if !self.are_parameters_compatible_impl(s_rest_elem, rest_elem_type, is_method) {
                    return SubtypeResult::False;
                }
            }
        }

        if source_has_rest {
            let Some(rest_param) = source_params.last() else {
                return SubtypeResult::False;
            };
            let rest_elem_type = self.get_array_element_type(rest_param.type_id);
            let rest_is_top = self.allow_bivariant_rest && rest_elem_type.is_any_or_unknown();

            if !rest_is_top {
                for t_param in target_params
                    .iter()
                    .skip(source_fixed_count)
                    .take(target_fixed_count.saturating_sub(source_fixed_count))
                {
                    if !self.are_parameters_compatible_impl(
                        rest_elem_type,
                        t_param.type_id,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                }
            }
        }

        SubtypeResult::True
    }

    /// Evaluate a meta-type (conditional, index access, mapped, keyof, etc.) to its
    /// concrete form. Uses `TypeEvaluator` with the resolver to correctly resolve
    /// Lazy(DefId) types at all nesting levels (e.g., KeyOf(Lazy(DefId))).
    ///
    /// Always uses `TypeEvaluator` with the resolver instead of `query_db.evaluate_type()`
    /// because the checker populates DefId→TypeId mappings in the `TypeEnvironment` that
    /// the `query_db`'s resolver-less evaluator cannot access.
    ///
    /// Results are cached in `eval_cache` to avoid re-evaluating the same type across
    /// multiple subtype checks. This turns O(n²) evaluate calls into O(n).
    pub(crate) fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types (number, string, boolean, void, null, etc.)
        // never need evaluation. Skip cache lookup entirely.
        if type_id.is_intrinsic() {
            return type_id;
        }
        // Check local evaluation cache first.
        // Key includes no_unchecked_indexed_access since with that flag evaluation results can vary.
        let cache_key = (type_id, self.no_unchecked_indexed_access);
        if let Some(&cached) = self.eval_cache.get(&cache_key) {
            return cached;
        }
        use crate::evaluation::evaluate::TypeEvaluator;
        let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
        evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access);
        // Pass query_db to share the application evaluation cache across evaluations.
        // This ensures that Application(Lazy(DefId), args) evaluated multiple times produces
        // the same ObjectShapeId, preventing spurious structural subtype failures when two
        // independent evaluations of the same generic type (e.g., AsyncGenerator<string, string, string[]>)
        // produce different shape IDs.
        if let Some(db) = self.query_db {
            evaluator = evaluator.with_query_db(db);
        }
        let result = evaluator.evaluate(type_id);
        self.eval_cache.insert(cache_key, result);
        result
    }
}
