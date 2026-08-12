//! Function and callable subtype checking -- main checking methods.
//!
//! Contains the core `check_function_subtype` entry point and related
//! signature comparison logic (call signatures, constructors, params).

use crate::instantiation::instantiate::TypeSubstitution;
use crate::type_param_info;
use crate::type_queries::unpack_tuple_rest_parameter;
use crate::types::{
    CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, ObjectFlags, ObjectShape,
    ParamInfo, PropertyInfo, TypeData, TypeId, TypeParamInfo, Visibility,
};
use crate::visitor::callable_shape_id;

use super::super::super::{SubtypeChecker, SubtypeResult, TypeParamEquivalence, TypeResolver};
use super::erase_type_params_to_constraints;

mod call_signatures;
mod context_instantiation;
mod cowalk;
mod evaluation;
mod generic_constraints;
mod name_pairing;
mod nonlocal_type_params;
mod overloads;
mod params;

type HoistedTypeParams = (Vec<TypeParamInfo>, Vec<(TypeId, TypeId)>);

use name_pairing::{alpha_name_pair_enabled, name_aware_target_permutation};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(crate) fn check_function_subtype(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> SubtypeResult {
        // Consume (and clear) the construct-parameter strictness request set by
        // `check_callable_subtype` for the immediate construct-signature
        // comparison. Clearing it here means nested function comparisons reached
        // from the constructor's parameter/return types start fresh, matching
        // `tsc`, where only the construct signature whose declaration kind is a
        // class `Constructor` gets parameter bivariance; a `new (...) => T` type
        // literal or interface construct signature (`ConstructSignature`) is
        // compared strictly like a call-signature literal.
        let force_strict_construct_params = std::mem::take(&mut self.force_strict_construct_params);
        let allow_constructor_bivariance = target.is_constructor && target.is_method;
        debug_assert!(
            !force_strict_construct_params || !allow_constructor_bivariance,
            "explicit class-constructor targets must not request strict construct variance"
        );
        let callback_modes = (
            self.in_callback_param_check,
            self.in_bivariant_callback_return_check,
        );
        let rigid_rest_pair_is_strict = self.strict_function_types
            && (callback_modes.0 || !target.is_method)
            && (!target.is_constructor
                || force_strict_construct_params
                || !allow_constructor_bivariance);
        let allow_provisional_rest_union_at_this_depth =
            self.allow_provisional_rest_union && self.provisional_rest_union_function_depth == 0;
        if rigid_rest_pair_is_strict
            && !self.local_generic_rest_binders_are_alpha_paired(source, target)
            && self.rigid_bare_rest_parameter_mismatch(
                source,
                target,
                allow_provisional_rest_union_at_this_depth,
            )
        {
            // `check_function_subtype_impl` normally consumes these one-shot
            // modes at entry. This early rejection must consume them too.
            self.in_callback_param_check = false;
            self.in_bivariant_callback_return_check = false;
            return SubtypeResult::False;
        }
        self.with_provisional_rest_union_function_scope(
            |checker, allow_provisional_rest_union_at_this_depth| {
                let result = checker.check_function_subtype_impl(
                    source,
                    target,
                    allow_constructor_bivariance,
                    allow_provisional_rest_union_at_this_depth,
                );
                if result.is_true() {
                    return result;
                }
                checker
                    .retry_generic_signature_with_context_instantiation(
                        source,
                        target,
                        result,
                        callback_modes,
                        allow_provisional_rest_union_at_this_depth,
                    )
                    .unwrap_or(result)
            },
        )
    }

    /// Whether the two written rest slots are binders owned by corresponding
    /// type parameters of same-arity generic signatures.
    ///
    /// The early rigid-rest guard runs before `check_function_subtype_impl`
    /// alpha-renames generic signatures. Do not reject a pair that the normal
    /// signature normalization will turn into the same binder.
    fn local_generic_rest_binders_are_alpha_paired(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        if source.type_params.is_empty() || source.type_params.len() != target.type_params.len() {
            return false;
        }
        let Some(source_rest) = source.params.last().filter(|param| param.rest) else {
            return false;
        };
        let Some(target_rest) = target.params.last().filter(|param| param.rest) else {
            return false;
        };
        let Some(source_binder) = self.bare_rest_type_param(source_rest.type_id) else {
            return false;
        };
        let Some(target_binder) = self
            .bare_rest_type_param(target_rest.type_id)
            .or_else(|| self.single_variadic_tuple_rest_binder(target_rest.type_id))
        else {
            return false;
        };
        let Some(source_index) = source
            .type_params
            .iter()
            .position(|param| param.is_same_binder(source_binder))
        else {
            return false;
        };
        let Some(target_index) = target
            .type_params
            .iter()
            .position(|param| param.is_same_binder(target_binder))
        else {
            return false;
        };

        if alpha_name_pair_enabled()
            && let Some(permutation) =
                name_aware_target_permutation(&source.type_params, &target.type_params)
        {
            return permutation.get(source_index) == Some(&target_index);
        }
        source_index == target_index
    }

    /// True when any of `candidate_tp_ids` occurs *free* in `shape`'s parameter
    /// or return positions.
    ///
    /// Used when relating a generic signature to a non-generic one to detect a
    /// genuine type-parameter identity shared between them (from contextual
    /// seeding). tsz interns `TypeParameter`s structurally by name, so a
    /// same-named parameter *bound* by a nested generic signature in `shape`
    /// (e.g. a method `call<T>(...)` on a parameter type) shares the candidate's
    /// `TypeId` without sharing identity; restricting to *free* occurrences
    /// avoids treating that coincidence as identity-sharing, which would
    /// otherwise skip contextual instantiation and emit spurious
    /// `TS2322`/`TS2345`/`TS2416`.
    fn shape_free_type_params_overlap(
        &self,
        candidate_tp_ids: &[TypeId],
        shape: &FunctionShape,
    ) -> bool {
        if candidate_tp_ids.is_empty() {
            return false;
        }
        let free = crate::visitors::visitor_predicates::free_type_parameter_ids_in(
            self.interner,
            shape
                .params
                .iter()
                .map(|p| p.type_id)
                .chain(std::iter::once(shape.return_type)),
        );
        candidate_tp_ids.iter().any(|id| free.contains(id))
    }

    /// `TypeId` candidates that may represent `shape`'s own type parameters
    /// inside a related signature.
    ///
    /// The structural intern of each `TypeParamInfo` covers occurrences that
    /// were interned through the dedupe table. Declaration-scoped type
    /// parameters are interned fresh and keep their original `TypeId` through
    /// instantiation (#13044), so the id that actually occurs free in
    /// `shape`'s parameter/return positions can differ from that structural
    /// intern. Within `shape`, a *free* occurrence whose name matches one of
    /// the shape's own type parameters is bound by that parameter, so its id
    /// is an equally valid identity handle for it. Collect both so
    /// identity-sharing recognition works for declaration-scoped parameters
    /// as well as structurally interned ones.
    fn own_type_param_identity_ids(&self, shape: &FunctionShape) -> Vec<TypeId> {
        let mut ids: Vec<TypeId> = shape
            .type_params
            .iter()
            .map(|tp| self.interner.type_param(*tp))
            .collect();
        if shape.type_params.is_empty() {
            return ids;
        }
        let free = crate::visitors::visitor_predicates::free_type_parameter_ids_in(
            self.interner,
            shape
                .params
                .iter()
                .map(|p| p.type_id)
                .chain(shape.this_type)
                .chain(std::iter::once(shape.return_type)),
        );
        for id in free {
            if !ids.contains(&id)
                && type_param_info(self.interner, id).is_some_and(|info| {
                    shape
                        .type_params
                        .iter()
                        .any(|type_param| type_param.is_same_binder(info))
                })
            {
                ids.push(id);
            }
        }
        ids
    }

    /// #14345 scoped structural-strip flag (default-OFF, reuses
    /// `TSZ_TYPEPARAM_DECL_IDENTITY`). When OFF this strip is never applied, so
    /// the alpha-rename body comparison is byte-identical to pre-strip behavior.
    /// Gated together with the construction stamp so the two halves of the
    /// flag-flip move as one: the stamp fixes the 278 at construction, the strip
    /// clears the +53 alpha-equiv-through-Application regressions the stamp
    /// exposes in the signature relation.
    fn scoped_decl_param_strip_enabled() -> bool {
        use std::sync::OnceLock;
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var("TSZ_TYPEPARAM_DECL_IDENTITY").is_ok_and(|v| v == "1"))
    }

    /// #14345 WAVE-1 decl-origin-through-reduction flag (default-OFF, composes
    /// on top of `TSZ_TYPEPARAM_DECL_IDENTITY`).
    ///
    /// When ON (and the construction stamp is ON), the alpha-rename registration
    /// records each paired param's authoritative exact binder alongside
    /// the pre-instantiate `TypeId` pair, and the consult
    /// (`check_subtype`) additionally accepts two reduced-body `TypeParameter`
    /// leaves whose carried binders form a registered pair — the same-binder
    /// `B ≡ A` bridge that the name-keyed re-mint loses (the leaf id is a THIRD
    /// identity, but its declaration binder survives). A different-binder pair
    /// (`T`/`U` from distinct declarations that were never registered) is not
    /// accepted, which is the sound discriminator the name+surface structural
    /// strip cannot express.
    ///
    /// Requires `TSZ_TYPEPARAM_DECL_IDENTITY=1` to have any effect: without the
    /// construction stamp no leaf carries an authoritative declaration binder,
    /// so the extra match never fires. With the flag off, the registered
    /// `binders` field is always `None` and the consult is byte-identical to the
    /// id-only match.
    pub(crate) fn decl_origin_reduction_enabled() -> bool {
        use std::sync::OnceLock;
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| {
            std::env::var("TSZ_TYPEPARAM_DECL_IDENTITY").is_ok_and(|v| v == "1")
                && std::env::var("TSZ_DECL_ORIGIN_REDUCTION").is_ok_and(|v| v == "1")
        })
    }

    /// Map every free declaration-origin type parameter in `source`/`target`
    /// back to its `User`-canonical structural intern. Applying this substitution
    /// to both bodies collapses alpha-equivalent parameters to the flag-off identity.
    ///
    /// This is sound only for equal names and surfaces (`constraint`, `default`,
    /// and `is_const`). The interner keeps distinct surfaces in distinct `User`
    /// ids. If one name has multiple declaration surfaces across the bodies, a
    /// name-keyed substitution cannot represent the split, so that name remains
    /// declaration-scoped. Only uniformly surfaced names are stripped.
    pub(crate) fn build_decl_param_structural_strip(
        &self,
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> TypeSubstitution {
        let roots = source
            .params
            .iter()
            .map(|p| p.type_id)
            .chain(source.this_type)
            .chain(std::iter::once(source.return_type))
            .chain(target.params.iter().map(|p| p.type_id))
            .chain(target.this_type)
            .chain(std::iter::once(target.return_type));
        self.build_decl_param_structural_strip_for_roots(roots)
    }

    fn check_function_subtype_impl(
        &mut self,
        source: &FunctionShape,
        target: &FunctionShape,
        allow_constructor_bivariance: bool,
        allow_provisional_rest_union_at_this_depth: bool,
    ) -> SubtypeResult {
        // Capture and reset the callback-param-check flags at function entry so
        // every terminal path consumes the one-shot mode and nested sub-checks
        // cannot steal it before the parameter comparison below.
        let in_callback_param_check = self.in_callback_param_check;
        let in_bivariant_callback_return_check = self.in_bivariant_callback_return_check;
        self.in_callback_param_check = false;
        self.in_bivariant_callback_return_check = false;

        // Constructor vs non-constructor
        if source.is_constructor != target.is_constructor {
            return SubtypeResult::False;
        }

        let mut source_instantiated = source.clone();
        let mut target_instantiated = target.clone();
        // Track type param equivalences scope for cleanup at end of function.
        let equiv_start = self.type_param_equivalences.len();

        if self.erase_generics
            && source_instantiated.type_params.is_empty()
            && !target_instantiated.type_params.is_empty()
            && let Some((hoisted, replacements)) =
                self.hoist_matching_nonlocal_type_params(&source_instantiated, &target_instantiated)
        {
            source_instantiated.type_params = hoisted;
            for (from, to) in replacements {
                source_instantiated =
                    self.replace_function_type_exact(&source_instantiated, from, to);
            }
        }

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
                let local_tp_ids: rustc_hash::FxHashSet<TypeId> = shape
                    .type_params
                    .iter()
                    .map(|tp| self.interner.type_param(*tp))
                    .collect();
                let refs_nonlocal_type_param = |type_id: TypeId| {
                    crate::visitors::visitor_predicates::references_type_param_outside_id_set(
                        self.interner,
                        type_id,
                        &local_tp_ids,
                    )
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

            // #14345 Stage-3: pair the source/target type params for the
            // alpha-rename. By default this is positional (`source[i]` with
            // `target[i]`). When the name multisets are equal but the order
            // differs (`<E,A>` vs `<A,E>`), positional pairing renames the
            // target body onto the wrong source identities and produces
            // spurious mismatches; under `TSZ_ALPHA_NAME_PAIR=1`, pair by name
            // instead so same-named params line up across the reorder. Every
            // downstream consumer (constraint classification, equivalence
            // registration, fallback canonicalization) iterates this single
            // pairing so the chosen alignment is used uniformly.
            let name_aware_perm = if alpha_name_pair_enabled() {
                name_aware_target_permutation(
                    &source_instantiated.type_params,
                    &target_instantiated.type_params,
                )
            } else {
                None
            };
            // Materialize the paired `(source_tp, target_tp)` once so all sites
            // below agree on the alignment. `TypeParamInfo` is `Copy`, so own
            // the pairs rather than borrow -- `source_instantiated` /
            // `target_instantiated` are reassigned by the alpha-rename below.
            let paired_type_params: Vec<(TypeParamInfo, TypeParamInfo)> = match &name_aware_perm {
                Some(perm) => perm
                    .iter()
                    .enumerate()
                    .map(|(i, &j)| {
                        (
                            source_instantiated.type_params[i],
                            target_instantiated.type_params[j],
                        )
                    })
                    .collect(),
                None => source_instantiated
                    .type_params
                    .iter()
                    .zip(target_instantiated.type_params.iter())
                    .map(|(s, t)| (*s, *t))
                    .collect(),
            };

            let mut target_to_source_substitution =
                TypeSubstitution::for_signature_domain(&target_instantiated.type_params);
            let mut source_identity_substitution =
                TypeSubstitution::for_signature_domain(&source_instantiated.type_params);
            for (source_tp, target_tp) in &paired_type_params {
                let source_type_param_type = self.interner.type_param(*source_tp);
                target_to_source_substitution.insert(target_tp.name, source_type_param_type);
                source_identity_substitution.insert(source_tp.name, source_type_param_type);
            }

            let mapped_constraint_sensitive =
                paired_type_params
                    .iter()
                    .any(|(source_type_param, target_type_param)| {
                        source_instantiated.params.iter().any(|param| {
                            self.type_param_appears_in_mapped_context(
                                param.type_id,
                                *source_type_param,
                            )
                        }) || source_instantiated.this_type.is_some_and(|this_type| {
                            self.type_param_appears_in_mapped_context(this_type, *source_type_param)
                        }) || self.type_param_appears_in_mapped_context(
                            source_instantiated.return_type,
                            *source_type_param,
                        ) || target_instantiated.params.iter().any(|param| {
                            self.type_param_appears_in_mapped_context(
                                param.type_id,
                                *target_type_param,
                            )
                        }) || target_instantiated.this_type.is_some_and(|this_type| {
                            self.type_param_appears_in_mapped_context(this_type, *target_type_param)
                        }) || self.type_param_appears_in_mapped_context(
                            target_instantiated.return_type,
                            *target_type_param,
                        )
                    });

            // Mapped/indexed generic signatures are constraint-sensitive: a stricter
            // target constraint like `U extends string[]` must stay visible rather
            // than being alpha-renamed onto an unconstrained source parameter `T`,
            // or apparent-member facts can be erased and make the signatures look
            // spuriously compatible. Outside that lane, keep the broader one-way
            // compatibility that TypeScript uses for generic function directionality.
            // For alpha-rename to succeed, every source type parameter's bound must
            // be no stricter than the corresponding target bound (so the source's
            // requirements are at most as strict as the target's). When the source
            // is stricter we must not erase that distinction by alpha-renaming it
            // onto the target marker; the only exception is a source constraint that
            // merely wraps the target's recursive constraint in extra application
            // layers. For mapped/indexed contexts both directions must hold so
            // apparent-member facts are preserved.
            let constraints_allow_alpha_rename =
                paired_type_params.iter().all(|(source_tp, target_tp)| {
                    let relation = self.classify_generic_tp_constraint(
                        source_tp,
                        target_tp,
                        &target_to_source_substitution,
                        mapped_constraint_sensitive,
                    );
                    if relation.source_is_stricter {
                        if !mapped_constraint_sensitive && relation.wraps_recursive {
                            return true;
                        }
                        return false;
                    }
                    if mapped_constraint_sensitive {
                        relation.constraints_mutually_assignable
                    } else {
                        true
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
                for (source_tp, target_tp) in &paired_type_params {
                    let source_tp_type = self.interner.type_param(*source_tp);
                    let target_tp_type = self.interner.type_param(*target_tp);
                    if source_tp_type != target_tp_type {
                        // #14345 WAVE-1: additionally record the exact binder pair
                        // when both params carry authoritative declaration
                        // identities, so the consult can bridge
                        // reduced-body `Kind<F,A>` leaves reconstructed under a
                        // fresh `TypeId` while preserving their exact binder.
                        // Gated behind `decl_origin_reduction_enabled`
                        // (composes with `TSZ_TYPEPARAM_DECL_IDENTITY`); OFF ->
                        // `binders: None`, byte-identical id-only behavior.
                        let binders = if Self::decl_origin_reduction_enabled() {
                            source_tp
                                .declaration_binder_key()
                                .zip(target_tp.declaration_binder_key())
                        } else {
                            None
                        };
                        self.type_param_equivalences.push(TypeParamEquivalence {
                            source: source_tp_type,
                            target: target_tp_type,
                            binders,
                        });
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

                // #14345 WAVE-1 register-through-reduction co-walk (flag-gated,
                // byte-parity-inert OFF). Registers the DEEPER corresponding leaf
                // binder pairs the top-level registration misses; must run before
                // the strip below (which erases declaration origins). See the
                // `cowalk` module.
                self.register_cowalk_leaf_binders(&source_instantiated, &target_instantiated);
                // #14345 scoped structural-strip (flag-gated, byte-parity-inert
                // OFF). The construction stamp gives every user-written type
                // parameter an authoritative declaration origin so two distinct
                // declarations sharing an identical surface intern to distinct
                // ids — fixing the self-ref-guard over-collapse at
                // construction/registration (the 278 fp-ts fixes). But that
                // stamp also makes two ALPHA-EQUIVALENT signature bodies (e.g.
                // `<A>(r: Record<string, A>) => number` vs
                // `<A>(r: ReadonlyRecord<string, A>) => number` after their
                // aliases reduce to the same shape) fail to relate: their `A`s
                // carry distinct declaration origins that are re-minted to a
                // THIRD id through the name-keyed substitution + per-body
                // `instantiate_type`, so the id-keyed equivalence registered
                // above (lines ~289-300) never bridges them — the +53
                // equiv-through-Application regressions.
                //
                // Flag-OFF, those same params intern to ONE structural (`User`)
                // id and relate trivially (zero +53). This strip reproduces that
                // flag-OFF identity SCOPED to the two cloned bodies used for the
                // structural compare only: each free declaration-stamped
                // parameter is mapped back to its `User`-canonical structural
                // intern (origin erased, surface preserved), so alpha-equivalent
                // params from distinct decls collapse to the SAME id and unify. It does NOT
                // touch construction-time stamping (the 278 fire at a different
                // level — registration/`is_identity_for`), so it is additive to
                // the construction fix.
                if Self::scoped_decl_param_strip_enabled() {
                    let mut strip = self.build_decl_param_structural_strip(
                        &source_instantiated,
                        &target_instantiated,
                    );
                    // `instantiate_function_shape` clears the quantifier lists,
                    // so retain their exact domains through the pairs captured
                    // above. The strip may canonicalize the alpha-paired local
                    // binders, but a captured same-named class binder is foreign
                    // and must remain declaration-scoped.
                    for (source_type_param, target_type_param) in &paired_type_params {
                        strip.protect_type_parameters(std::slice::from_ref(source_type_param));
                        strip.protect_type_parameters(std::slice::from_ref(target_type_param));
                    }
                    if !strip.is_empty() {
                        source_instantiated =
                            self.instantiate_function_shape(&source_instantiated, &strip);
                        target_instantiated =
                            self.instantiate_function_shape(&target_instantiated, &strip);
                    }
                }
            } else if mapped_constraint_sensitive {
                // When mapped/indexed types are involved, constraint differences are
                // semantically significant and cannot be erased safely. Reject immediately.
                self.type_param_equivalences.truncate(equiv_start);
                return SubtypeResult::False;
            } else {
                // Strategy 2: alpha-rename was refused because at least one source
                // type parameter carries a strictly stronger constraint than its
                // target counterpart (e.g. `<T extends object>` vs `<T>`).
                //
                // tsc handles this by keeping the target's type parameters as
                // canonical opaque markers (`getCanonicalSignature`) and
                // instantiating the *source* in the context of the target
                // (`instantiateSignatureInContextOf`): each source type parameter is
                // inferred from the target marker and clamped to its declared
                // constraint when the marker cannot satisfy it. For the same-arity
                // case this is exactly:
                //   - a stricter source parameter never satisfies its bound from the
                //     looser target marker, so it clamps to its own constraint;
                //   - a non-stricter source parameter is satisfied by the target
                //     marker, so it keeps the marker (an alpha-rename onto it).
                // The target keeps its parameters as free opaque markers (their
                // embedded constraints survive) for the structural comparison below.
                //
                // This is critical for soundness: erasing *both* sides to their
                // constraints loses the target markers, after which a method-bivariant
                // or covariant comparison silently accepts unsound overrides such as
                //   source: <T extends object>(x: T) => T   (derived)
                //   target: <T>(x: T) => T                  (base)
                // because `object`/`unknown` compare loosely. Clamping keeps the
                // distinction (`T` becomes `object`, compared against the opaque base
                // marker `U`, which fails) while still accepting cases where the
                // constraint difference is reconciled by parameter usage, e.g.
                //   source: <T extends {p: string}>(x: T[]) => void
                //   target: <S extends {p: string}[]>(x: S) => void
                // (`T` clamps to `{p: string}`, source param becomes `{p: string}[]`,
                // which the opaque `S extends {p: string}[]` marker accepts).
                let mut source_substitution =
                    TypeSubstitution::for_signature_domain(&source_instantiated.type_params);
                for (source_tp, target_tp) in &paired_type_params {
                    let relation = self.classify_generic_tp_constraint(
                        source_tp,
                        target_tp,
                        &target_to_source_substitution,
                        false,
                    );
                    if relation.source_is_stricter && !relation.wraps_recursive {
                        // Clamp the stricter source parameter to its own constraint.
                        source_substitution.insert(
                            source_tp.name,
                            source_tp.constraint.unwrap_or(TypeId::UNKNOWN),
                        );
                    } else {
                        // Reconcilable: alpha-rename onto the target's opaque marker.
                        source_substitution
                            .insert(source_tp.name, self.interner.type_param(*target_tp));
                    }
                }
                source_instantiated =
                    self.instantiate_function_shape(&source_instantiated, &source_substitution);
                // Drop the target quantifier so its parameters become free opaque
                // markers (retaining their embedded constraints) for comparison.
                target_instantiated.type_params.clear();
            }
        }

        let source_mentions_nonlocal_type_params = {
            let local_source_tp_ids: rustc_hash::FxHashSet<TypeId> = source_instantiated
                .type_params
                .iter()
                .map(|tp| self.interner.type_param(*tp))
                .collect();
            let refs_nonlocal_type_param = |type_id: TypeId| {
                crate::visitors::visitor_predicates::references_type_param_outside_id_set(
                    self.interner,
                    type_id,
                    &local_source_tp_ids,
                )
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
                    allow_provisional_rest_union_at_this_depth,
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
            //
            // Identity here must be a *free* occurrence of the source parameter in
            // the target. tsz interns `TypeParameter`s structurally by name, so an
            // unrelated same-named parameter bound by a nested generic signature in
            // the target (e.g. a method `$call<T>(...)` on the target's parameter
            // type) shares the source `T`'s `TypeId` without sharing its identity.
            // Counting those bound occurrences would wrongly skip instantiation and
            // leave the source parameter free, producing spurious TS2322/TS2345
            // (`'X' is not assignable to 'T'`). Restrict the check to free
            // occurrences so only genuine contextual-seeding shares identity.
            let source_tp_ids = self.own_type_param_identity_ids(&source_instantiated);
            let target_refs_source_params =
                self.shape_free_type_params_overlap(&source_tp_ids, &target_instantiated);
            tracing::trace!(
                ?source_tp_ids,
                target_refs_source_params,
                target_params = ?target_instantiated.params.iter().map(|p| p.type_id).collect::<Vec<_>>(),
                target_return = ?target_instantiated.return_type,
                "generic source vs non-generic target: identity-sharing check"
            );

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
        //
        // As with the generic-source branch above, only a *free* occurrence of the
        // target parameter in the source signifies shared identity. A same-named
        // parameter bound by a nested generic signature inside the source shares the
        // interned `TypeId` without sharing identity and must not be counted, or a
        // concrete source member is spuriously rejected as a subtype of the
        // universally quantified target (false TS2416/TS2430).
        if source_instantiated.type_params.is_empty() && !target_instantiated.type_params.is_empty()
        {
            let target_tp_ids = self.own_type_param_identity_ids(&target_instantiated);
            let source_refs_target_params =
                self.shape_free_type_params_overlap(&target_tp_ids, &source_instantiated);

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
            } else if self.erase_generics {
                // Standard path: tsc's `getBaseSignature` for the subset of
                // target's type parameters observed only through a generic
                // application (e.g. alias `A` in `AliasedRawBuilder<O, A>`),
                // not bare in a value position. Each erasable parameter is
                // instantiated to its constraint so a concrete implementation
                // whose result satisfies the constraint is accepted (matches
                // tsc for overloaded generic builder methods). Bare or
                // unconstrained parameters stay opaque.
                //
                // `type_param_appears_bare` matches the parameter by name as
                // well as by `TypeId`, because the erase substitution below is
                // keyed on `tp.name` and a signature's `type_params` list can
                // carry a different `TypeId` for the same logical parameter than
                // its body does (the list is re-interned while the return keeps
                // its original reference). Without the name match a bare `T`,
                // `T[]`, or `T | null` in the return would read as "absent",
                // and the name-keyed substitution would erase it anyway — a
                // covariant leak that wrongly accepts a concrete member for a
                // universally-quantified one (issue #10812).
                let mut target_canonical =
                    TypeSubstitution::for_signature_domain(&target_instantiated.type_params);
                for tp in &target_instantiated.type_params {
                    let tp_id = self.interner.type_param(*tp);
                    if let Some(constraint) = tp.constraint
                        && constraint != TypeId::UNKNOWN
                    {
                        let appears_bare = target_instantiated
                            .params
                            .iter()
                            .any(|p| self.type_param_appears_bare(p.type_id, tp_id))
                            || target_instantiated
                                .this_type
                                .is_some_and(|t| self.type_param_appears_bare(t, tp_id))
                            || self.type_param_appears_bare(target_instantiated.return_type, tp_id);
                        // Even when the parameter never appears bare, an
                        // application-mediated covariant (or invariant) occurrence
                        // in the return -- `Box<T>`, `Cell<T>`, `T[]`, `T | null`
                        // -- keeps the parameter observable to a caller, so it must
                        // stay opaque to match tsc's per-signature variance
                        // comparison. Only purely contravariant or phantom
                        // occurrences remain erasable. The variance walk runs only
                        // when the cheaper syntactic check already permits erasure.
                        // (Issue #10812.)
                        if !appears_bare
                            && !self.type_param_covariant_in_return(
                                target_instantiated.return_type,
                                tp.name,
                            )
                        {
                            target_canonical.insert(tp.name, constraint);
                        }
                    }
                }
                target_instantiated =
                    self.instantiate_function_shape(&target_instantiated, &target_canonical);
            } else {
                // Strict member-compatibility (TS2416/TS2430): tsc's
                // `compareSignaturesRelated` only canonicalizes target's
                // method-local type parameters when source has its own.
                // With a non-generic source, target stays universally
                // quantified — comparing target's `T` opaquely naturally
                // enforces variance (a covariant `Box<T>` rejects the
                // implementation, a contravariant `FBox<T>` accepts it).
                // Overloaded-builder overrides that need the erasure escape
                // hatch are short-circuited upstream by
                // `implementation_signature_covers_interface_overloads`.
                let mentions =
                    |ty: TypeId| crate::visitor::contains_type_parameters(self.interner, ty);
                if source_instantiated
                    .params
                    .iter()
                    .any(|p| mentions(p.type_id))
                    || source_instantiated.this_type.is_some_and(mentions)
                    || mentions(source_instantiated.return_type)
                {
                    self.type_param_equivalences.truncate(equiv_start);
                    return SubtypeResult::False;
                }
            }
        }

        let raw_rest_types = source_instantiated
            .params
            .last()
            .filter(|param| param.rest)
            .zip(target_instantiated.params.last().filter(|param| param.rest))
            .map(|(source_rest, target_rest)| (source_rest.type_id, target_rest.type_id));
        let provisional_rest_union = allow_provisional_rest_union_at_this_depth
            && raw_rest_types.is_some_and(|(source_rest, target_rest)| {
                self.is_bare_rest_type_param(source_rest)
                    && self.rest_type_has_union_surface(target_rest)
            });

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
                            if !s_tp.is_same_binder(t_tp) {
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

        // Return type is normally covariant. In tsc's BivariantCallback mode
        // (the callback signature comparison reached from a bivariant method
        // parameter), callback returns are accepted in either direction.
        let return_result = self.check_return_compat(
            source_instantiated.return_type,
            target_instantiated.return_type,
        );
        let return_result = if in_bivariant_callback_return_check && !return_result.is_true() {
            self.check_subtype(
                target_instantiated.return_type,
                source_instantiated.return_type,
            )
        } else {
            return_result
        };
        if !return_result.is_true() {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }
        if !self.are_this_parameters_compatible(
            source_instantiated.this_type,
            target_instantiated.this_type,
            in_callback_param_check || target_instantiated.is_method,
        ) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        // Type predicates check
        if !self.are_type_predicates_compatible(&source_instantiated, &target_instantiated) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

        // Target methods and explicit class constructors are bivariant;
        // function properties and construct-signature types remain strict.
        //
        // tsc's StrictCallback/BivariantCallback override: when the immediately-
        // enclosing comparison was a callback parameter check, methods do *not*
        // get bivariance-loosening for this signature comparison. The flag was
        // captured and cleared at the top of this function (see above) so that
        // nested sub-checks performed before this point cannot consume it; we
        // read the captured local here.
        let constructor_param_bivariance =
            allow_constructor_bivariance && target_instantiated.is_constructor;
        let is_method = !in_callback_param_check
            && (target_instantiated.is_method || constructor_param_bivariance);
        let raw_bare_source_rest_compatibility = {
            // Use the same one-shot strict-callback variance mode as the fixed
            // and expanded-rest parameter comparisons below.
            let saved_force_strict_callback_params = self.force_strict_callback_param_variance;
            self.force_strict_callback_param_variance = in_callback_param_check;
            let result = raw_rest_types.and_then(|(source_rest, target_rest)| {
                self.bare_source_rest_compatibility(
                    source_rest,
                    target_rest,
                    is_method,
                    provisional_rest_union,
                )
            });
            self.force_strict_callback_param_variance = saved_force_strict_callback_params;
            result
        };
        if raw_bare_source_rest_compatibility == Some(false) {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }

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
        let source_params_unpacked = self.unpack_normalized_params(&source_instantiated.params);
        let target_params_unpacked = self.unpack_normalized_params(&target_instantiated.params);

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
                let require_all_variants = !is_method;
                let mut matched_any_variant = false;
                for member_type_id in union_members.iter() {
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
                        if require_all_variants {
                            self.type_param_equivalences.truncate(equiv_start);
                            return SubtypeResult::False;
                        }
                        continue;
                    }

                    // Try unpacking this union member as a tuple
                    let member_param = ParamInfo {
                        suppress_display_optional: false,
                        type_id: *member_type_id,
                        rest: true,
                        ..*last_target_param
                    };
                    let member_unpacked = unpack_tuple_rest_parameter(self.interner, &member_param);

                    // Build full param list for this variant
                    let mut variant_params: Vec<ParamInfo> = prefix_params.to_vec();
                    variant_params.extend(member_unpacked);

                    let matched = self
                        .check_params_compatible(
                            &source_params_unpacked,
                            &variant_params,
                            is_method,
                            provisional_rest_union
                                || raw_bare_source_rest_compatibility == Some(true),
                        )
                        .is_true();
                    if require_all_variants && !matched {
                        self.type_param_equivalences.truncate(equiv_start);
                        return SubtypeResult::False;
                    }
                    matched_any_variant |= matched;
                }
                self.type_param_equivalences.truncate(equiv_start);
                return if require_all_variants || matched_any_variant {
                    SubtypeResult::True
                } else {
                    SubtypeResult::False
                };
            }
        }

        // Check rest parameter handling (after unpacking)
        let target_has_rest = target_params_unpacked.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params_unpacked.last().is_some_and(|p| p.rest);
        if target_has_rest
            && source_has_rest
            && raw_bare_source_rest_compatibility != Some(true)
            && let (Some(source_rest), Some(target_rest)) =
                (source_params_unpacked.last(), target_params_unpacked.last())
            && matches!(
                self.bare_source_rest_compatibility(
                    source_rest.type_id,
                    target_rest.type_id,
                    is_method,
                    false,
                ),
                Some(false)
            )
        {
            self.type_param_equivalences.truncate(equiv_start);
            return SubtypeResult::False;
        }
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
        let allow_bivariant_param_count = self.allows_bivariant_param_count(is_method);
        if (!target_has_rest || guard_target_rest_arity)
            && !allow_bivariant_param_count
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
        let saved_force_strict_callback_params = self.force_strict_callback_param_variance;
        self.force_strict_callback_param_variance = in_callback_param_check;
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
                    return SubtypeResult::False;
                }
            }

            // If target has rest parameter, check source's extra params against the rest type
            if target_has_rest {
                let Some(rest_elem_type) = rest_elem_type else {
                    return SubtypeResult::False;
                };
                if rest_elem_type.is_any_or_unknown()
                    && self
                        .first_top_rest_unassignable_source_param(&source_params_unpacked)
                        .is_some()
                {
                    return SubtypeResult::False;
                }
                if rest_is_top {
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
                if target_fixed_count > source_fixed_count
                    && self.is_bare_rest_type_param(rest_param.type_id)
                {
                    return SubtypeResult::False;
                }
                if self.is_tuple_list_rest_type(rest_param.type_id)
                    && target_fixed_count > source_fixed_count
                {
                    let tuple_elements: Vec<crate::types::TupleElement> = target_params_unpacked
                        .iter()
                        .skip(source_fixed_count)
                        .take(target_fixed_count.saturating_sub(source_fixed_count))
                        .map(|param| crate::types::TupleElement {
                            type_id: param.type_id,
                            name: param.name,
                            optional: param.optional,
                            rest: false,
                        })
                        .collect();
                    let target_rest_tuple = self.interner.tuple(tuple_elements);
                    if !self.are_parameters_compatible_impl(
                        rest_param.type_id,
                        target_rest_tuple,
                        is_method,
                    ) {
                        return SubtypeResult::False;
                    }
                    return SubtypeResult::True;
                }
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
        self.force_strict_callback_param_variance = saved_force_strict_callback_params;

        // If the inference-based comparison failed and we used inference for the
        // generic source → non-generic target case, retry with constraint erasure.
        // This matches tsc's `getErasedSignature` behavior for interface extension
        // checks (TS2430) where inference over-constrains type parameters by
        // intersecting inferred types with their constraints.
        let source_before_has_mapped_type_param_context =
            source_before_generic_instantiation
                .as_ref()
                .is_some_and(|source_before| {
                    source_before.type_params.iter().any(|tp| {
                        source_before.params.iter().any(|param| {
                            self.type_param_appears_in_mapped_context(param.type_id, *tp)
                        }) || source_before.this_type.is_some_and(|this_type| {
                            self.type_param_appears_in_mapped_context(this_type, *tp)
                        }) || self
                            .type_param_appears_in_mapped_context(source_before.return_type, *tp)
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
            if has_multiple_target_sigs
                && self.erased_fn_to_sig_return_variance_rejects(&s_fn, t_sig)
            {
                return SubtypeResult::False;
            }
            if !self.check_call_signature_subtype_fn(&s_fn, t_sig).is_true() {
                // tsc N×M path: when the target has multiple call signatures, try
                // erasing type params to `any` before rejecting. This matches tsc's
                // `signaturesRelatedTo` which uses `erase = true` for the N×M case.
                if has_multiple_target_sigs {
                    if !self.check_erased_fn_subtype_to_sig(&s_fn, t_sig).is_true()
                        && !self
                            .check_erased_fn_params_to_sig_with_matching_return_base(&s_fn, t_sig)
                            .is_true()
                    {
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
            let mut source_props = Vec::new();
            for t_prop in &target_props {
                let prop_name = self.interner.resolve_atom(t_prop.name);
                if matches!(prop_name.as_str(), "call" | "apply")
                    && !source_props
                        .iter()
                        .any(|p: &PropertyInfo| p.name == t_prop.name)
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
                        is_symbol_named: false,
                        single_quoted_name: false,
                        non_widening: false,
                    });
                }
            }
            let source_shape = ObjectShape {
                flags: ObjectFlags::empty(),
                properties: source_props,
                string_index: None,
                number_index: None,
                symbol_index: None,
                symbol: None,
            };
            let target_shape = ObjectShape {
                flags: ObjectFlags::empty(),
                properties: target_props,
                string_index: t_callable.string_index,
                number_index: t_callable.number_index,
                symbol_index: None,
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
        let mut substitution = TypeSubstitution::for_signature_domain(&s_sig.type_params);

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
                suppress_display_optional: false,
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
            if source.call_signatures.len() > 1
                && t_sig.is_method
                && self
                    .method_overloads_cover_tuple_union_rest_target(&source.call_signatures, t_sig)
            {
                found_match = true;
            }
            for s_sig in &source.call_signatures {
                if (!is_multi_sig || !self.erased_call_sig_return_variance_rejects(s_sig, t_sig))
                    && self.check_call_signature_subtype(s_sig, t_sig).is_true()
                {
                    found_match = true;
                    break;
                }
            }
            // tsc N×M path: when either side has multiple signatures, try erasing
            // type params to `any` (matching tsc's `getErasedSignature` behavior).
            if !found_match && is_multi_sig {
                for s_sig in &source.call_signatures {
                    if self.erased_call_sig_return_variance_rejects(s_sig, t_sig) {
                        continue;
                    }
                    if self
                        .check_erased_call_signature_subtype(s_sig, t_sig)
                        .is_true()
                        || self
                            .check_erased_call_signature_params_with_matching_return_base(
                                s_sig, t_sig,
                            )
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
        // Constructor-parameter bivariance is reserved for class-derived
        // constructor functions (`typeof Class`). A `new (...) => T` type literal
        // and an interface construct signature compare parameters strictly
        // (contravariantly under `strict_function_types`), exactly like a
        // call-signature literal — `tsc` keys this on whether the construct
        // signature's declaration is a class `Constructor`. The strictness is
        // driven by the *target* signature's kind, mirroring `tsc`'s
        // `compareSignaturesRelated`, so it is computed from the target callable.
        // Short-circuit on the empty case so a callable with only call
        // signatures pays no resolver lookup.
        for t_sig in &target.construct_signatures {
            let force_strict = !t_sig.is_method;
            let mut found_match = false;
            for s_sig in &source.construct_signatures {
                self.force_strict_construct_params = force_strict;
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
                    self.force_strict_construct_params = force_strict;
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
        // Defensive reset: ensure no leftover request leaks into the property
        // comparison below (and any later sibling relation) if the final
        // construct-signature comparison short-circuited before reaching
        // `check_function_subtype`.
        self.force_strict_construct_params = false;

        // Check properties (excluding private `#` fields), sorted by name to match
        // check_object_subtype's merge scan. When both callables have construct
        // signatures, skip `prototype` (validated by construct-signature compatibility;
        // checking it separately fails under signature-level generic erasure).
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
                        is_symbol_named: false,
                        single_quoted_name: false,
                        non_widening: false,
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
            symbol_index: None,
            symbol: source.symbol,
        };
        let target_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: target_props,
            string_index: target.string_index,
            number_index: target.number_index,
            symbol_index: None,
            symbol: target.symbol,
        };
        // Weak-type rule off for this stripped property part: a callable target is never weak in tsc. See `in_callable_property_check`.
        let prev_callable = self.in_callable_property_check;
        self.in_callable_property_check = true;
        let props_ok = self
            .check_object_subtype(&source_shape, None, None, &target_shape, None)
            .is_true();
        self.in_callable_property_check = prev_callable;
        if !props_ok {
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }
}
