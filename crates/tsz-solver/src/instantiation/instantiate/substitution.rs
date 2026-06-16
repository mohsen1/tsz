use super::api::{instantiate_type, maybe_evaluate_concrete_conditional, type_references_param};
use crate::construction::TypeDatabase;
use crate::types::{TypeId, TypeParamInfo};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tsz_common::interner::Atom;

/// A substitution map from type parameter names to concrete types.
#[derive(Clone, Debug, Default)]
pub struct TypeSubstitution {
    /// Maps type parameter names to their substituted types.
    pub(super) map: FxHashMap<Atom, TypeId>,
}

impl TypeSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
        }
    }

    /// Create a substitution containing a single `name -> type_id` binding.
    /// Equivalent to `let mut s = TypeSubstitution::new(); s.insert(name, type_id);`.
    pub fn single(name: Atom, type_id: TypeId) -> Self {
        let mut map = FxHashMap::with_capacity_and_hasher(1, Default::default());
        map.insert(name, type_id);
        Self { map }
    }

    /// Clear the substitution for reuse, preserving allocated capacity.
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Create a substitution from type parameters and arguments.
    ///
    /// `type_params` - The declared type parameters (e.g., `<T, U>`)
    /// `type_args` - The provided type arguments (e.g., `<string, number>`)
    ///
    /// When `type_args` has fewer elements than `type_params`, default values
    /// from the type parameters are used for the remaining parameters.
    ///
    /// IMPORTANT: Defaults may reference earlier type parameters, so they need
    /// to be instantiated with the substitution built so far.
    pub fn from_args(
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
    ) -> Self {
        let mut map = FxHashMap::with_capacity_and_hasher(type_params.len(), Default::default());

        // Phase 1: Insert explicitly-provided type arguments.
        //
        // A `TypeId::ERROR` argument is the internal cycle/fuel sentinel, never a
        // real type. Binding a type parameter directly to it would collapse that
        // parameter to `error` everywhere it appears in the instantiated body —
        // the cross-arena base-class poison cycle (#13044/#13484): when a generic
        // base class (`QueryCreator<DB>`) is resolved transitively while a derived
        // subclass chain is mid-resolution, the in-progress base instance is the
        // `ERROR` sentinel and substituting it as the `DB` argument bakes
        // `SelectFrom<error, ...>` into the inherited members.
        //
        // Treat the sentinel exactly like an unsupplied argument: fall through to
        // Phase 2, which binds the parameter to `any` (its no-candidate fallback).
        // This neither bakes `error` into the body (fixing the cross-arena poison
        // cycle) nor leaves the parameter free to leak into a contextual signature
        // (e.g. a generic call where inference produced no real candidate for a
        // type parameter), which would degrade contextual checking of the
        // remaining arguments. `tsc` never collapses a type parameter to an error
        // sentinel; an uninferred parameter resolves to its no-candidate fallback.
        for (i, param) in type_params.iter().enumerate() {
            if i < type_args.len() && type_args[i] != TypeId::ERROR {
                map.insert(param.name, type_args[i]);
            }
        }

        // Phase 2: Pre-fill unsupplied type parameters with `any` so that
        // circular and forward references in defaults become any-like instead
        // of leaking unresolved placeholders into the instantiated type.
        // Parameters whose supplied argument was the `ERROR` sentinel (skipped
        // above) are treated as unsupplied here and likewise bound to `any`.
        let supplied_real_arg = |i: usize| i < type_args.len() && type_args[i] != TypeId::ERROR;
        for (i, param) in type_params.iter().enumerate() {
            if !supplied_real_arg(i) {
                map.insert(param.name, TypeId::ANY);
            }
        }

        // Phase 3: Process defaults in declaration order. Each default is
        // instantiated with the substitution built so far (which includes
        // explicitly provided args, already-resolved defaults, and `any` for
        // not-yet-resolved params). This means a forward reference like `U = V`
        // where V hasn't been processed yet resolves to an any-like type.
        //
        // The working map is wrapped in a `TypeSubstitution` up front so each
        // default is instantiated against the substitution built so far without
        // cloning the entire map per parameter. `instantiate_type` borrows the
        // substitution immutably and does not retain it, so the in-place
        // `insert`/`remove` that follows each call observes exactly the same map
        // state a fresh per-iteration clone did — only the per-default
        // allocation churn is removed. This matters for deeply-defaulted generic
        // and recursive utility shapes, where the previous clone was O(map) work
        // repeated once for every defaulted parameter.
        let mut subst = Self { map };
        for (i, param) in type_params.iter().enumerate() {
            if i < type_args.len() {
                continue; // already provided explicitly
            }
            match param.default {
                Some(default) => {
                    let resolved = instantiate_type(interner, default, &subst);
                    // When the instantiated default is a conditional type whose check_type
                    // and extends_type are both concrete (no remaining type parameters),
                    // evaluate it immediately. This ensures that defaults like
                    // `K extends string ? Map<K, V> : Map<string, V>` (with K=string, V=number)
                    // become `Map<string, number>` in the substitution rather than remaining
                    // as a deferred conditional, which would later compare as a different
                    // `TypeId` from the inferred `Map<string, number>`.
                    let resolved = maybe_evaluate_concrete_conditional(interner, resolved);
                    // Circular default detection: if the resolved default is (or
                    // contains) the type parameter itself, fall back to `any`.
                    // This matches tsc behavior for `type T<X extends C = X>`.
                    let final_type = if type_references_param(interner, resolved, param.name) {
                        TypeId::ANY
                    } else {
                        resolved
                    };
                    subst.insert(param.name, final_type);
                }
                None => {
                    // No default and no argument - remove the error placeholder
                    // so this parameter remains unsubstituted.
                    subst.remove(param.name);
                }
            }
        }

        subst
    }

    /// Add a single substitution.
    pub fn insert(&mut self, name: Atom, type_id: TypeId) {
        self.map.insert(name, type_id);
    }

    /// Remove a single substitution.
    pub fn remove(&mut self, name: Atom) -> Option<TypeId> {
        self.map.remove(&name)
    }

    /// Look up a substitution.
    pub fn get(&self, name: Atom) -> Option<TypeId> {
        self.map.get(&name).copied()
    }

    /// Check if substitution is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Number of substitutions.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if this substitution is an identity mapping against specific type parameters.
    ///
    /// Compares the interned `TypeId` of each declared type parameter against
    /// its substituted value, which is the only sound identity check when the
    /// body being instantiated may use different `TypeId`s for same-named
    /// `TypeParameter`s (declaration-scoped fresh params share name but not
    /// `TypeId`).
    pub fn is_identity_for(
        &self,
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
    ) -> bool {
        type_params.iter().all(|param| {
            match self.map.get(&param.name) {
                Some(&type_id) => interner.type_param(*param) == type_id,
                None => true, // unmapped params don't change anything
            }
        })
    }

    /// Get a reference to the internal substitution map.
    ///
    /// This is useful for building new substitutions based on existing ones.
    pub const fn map(&self) -> &FxHashMap<Atom, TypeId> {
        &self.map
    }

    /// Produce the canonical, content-hashable form of this substitution.
    ///
    /// Returns a `SmallVec` of `(name, type_id)` pairs sorted by `Atom`.
    /// Sorting removes insertion-order dependence - the underlying
    /// `FxHashMap` does not iterate in a deterministic order, so two maps
    /// with the same contents but different insertion sequences would
    /// otherwise produce different iteration shapes.
    ///
    /// The returned form is the substitution component of
    /// `InstantiationCacheKey`. Substitution *interning* (for example, a
    /// global `u32` handle) intentionally does not live here: the cache
    /// lifetime is owned by `QueryCache`, not the global `TypeInterner`.
    ///
    /// Most substitutions have 1-4 entries (matching the shape of the
    /// existing `application_eval_cache`), so the `SmallVec<[_; 4]>`
    /// inline buffer avoids a heap allocation for the common case.
    #[must_use]
    pub fn canonical_pairs(&self) -> SmallVec<[(Atom, TypeId); 4]> {
        let mut pairs: SmallVec<[(Atom, TypeId); 4]> = self
            .map
            .iter()
            .map(|(&name, &type_id)| (name, type_id))
            .collect();
        // Keys are unique (`FxHashMap`), so sorting by `Atom` alone is
        // enough to canonicalize. `Atom` is `Ord` (u32 newtype).
        pairs.sort_unstable_by_key(|(name, _)| *name);
        pairs
    }
}

#[cfg(test)]
mod tests {
    use super::TypeSubstitution;
    use crate::TypeInterner;
    use crate::types::{TypeId, TypeParamInfo};
    use tsz_common::interner::Atom;

    fn param(name: Atom, default: Option<TypeId>) -> TypeParamInfo {
        TypeParamInfo {
            is_const: false,
            name,
            constraint: None,
            default,
            origin: crate::types::TypeParamOrigin::User,
        }
    }

    /// When every type parameter has a corresponding argument, `from_args`
    /// must map each name to the supplied argument and never enter the
    /// default-resolution phase.
    #[test]
    fn from_args_all_supplied_maps_directly() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        let params = vec![param(t, None), param(u, None)];

        let subst =
            TypeSubstitution::from_args(&interner, &params, &[TypeId::NUMBER, TypeId::STRING]);

        assert_eq!(subst.get(t), Some(TypeId::NUMBER));
        assert_eq!(subst.get(u), Some(TypeId::STRING));
        assert_eq!(subst.len(), 2);
    }

    /// A supplied argument that is the `TypeId::ERROR` cycle/fuel sentinel must
    /// never be baked into the substitution as `error` (the cross-arena
    /// base-class poison cycle #13044/#13484), nor left free (which leaks the
    /// raw parameter into a contextual signature and degrades checking of the
    /// remaining arguments, regressing `thislessFunctionsNotContextSensitive2`).
    /// It is treated exactly like an unsupplied argument: bound to `any`, the
    /// no-candidate fallback. Real arguments in other positions are unaffected.
    #[test]
    fn from_args_error_sentinel_arg_falls_back_to_any() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        let params = vec![param(t, None), param(u, None)];

        let subst =
            TypeSubstitution::from_args(&interner, &params, &[TypeId::ERROR, TypeId::STRING]);

        // The ERROR-sentinel position resolves to `any`, never `error`.
        assert_eq!(subst.get(t), Some(TypeId::ANY));
        assert_ne!(subst.get(t), Some(TypeId::ERROR));
        // A genuine argument in another position is bound normally.
        assert_eq!(subst.get(u), Some(TypeId::STRING));
    }

    /// A parameter with neither an argument nor a default must be left
    /// unsubstituted: the `any` placeholder seeded in phase 2 is removed in
    /// phase 3 so the body keeps the raw parameter.
    #[test]
    fn from_args_unsupplied_without_default_is_removed() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let params = vec![param(t, None)];

        let subst = TypeSubstitution::from_args(&interner, &params, &[]);

        assert_eq!(subst.get(t), None);
        assert!(subst.is_empty());
    }

    /// A default that references an earlier parameter must be instantiated
    /// against the argument supplied for that earlier parameter. This is the
    /// case the in-place (clone-free) accumulation must preserve.
    #[test]
    fn from_args_default_references_earlier_supplied_param() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        // U defaults to the type parameter `T`.
        let t_param_ty = interner.type_param(param(t, None));
        let params = vec![param(t, None), param(u, Some(t_param_ty))];

        let subst = TypeSubstitution::from_args(&interner, &params, &[TypeId::NUMBER]);

        assert_eq!(subst.get(t), Some(TypeId::NUMBER));
        // U's default `T` resolves through the substitution built so far.
        assert_eq!(subst.get(u), Some(TypeId::NUMBER));
    }

    /// A chain of defaults (`U = T`, `V = U`) must propagate the supplied
    /// argument all the way down. This exercises the in-place accumulation
    /// across multiple iterations: each default observes the resolved value of
    /// the previous one, exactly as the prior per-iteration map clone did.
    #[test]
    fn from_args_default_chain_propagates_through_in_place_map() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let t_param_ty = interner.type_param(param(t, None));
        let u_param_ty = interner.type_param(param(u, None));
        let params = vec![
            param(t, None),
            param(u, Some(t_param_ty)),
            param(v, Some(u_param_ty)),
        ];

        let subst = TypeSubstitution::from_args(&interner, &params, &[TypeId::BOOLEAN]);

        assert_eq!(subst.get(t), Some(TypeId::BOOLEAN));
        assert_eq!(subst.get(u), Some(TypeId::BOOLEAN));
        assert_eq!(subst.get(v), Some(TypeId::BOOLEAN));
    }

    /// A self-referential default (`X = X`) resolves to `any`: phase 2 seeds
    /// `X -> any`, and instantiating the default against that map substitutes
    /// the self-reference away, matching tsc's any-fallback for circular
    /// defaults.
    #[test]
    fn from_args_self_referential_default_falls_back_to_any() {
        let interner = TypeInterner::new();
        let x = interner.intern_string("X");
        let x_param_ty = interner.type_param(param(x, None));
        let params = vec![param(x, Some(x_param_ty))];

        let subst = TypeSubstitution::from_args(&interner, &params, &[]);

        assert_eq!(subst.get(x), Some(TypeId::ANY));
    }

    /// A forward reference (`U = V` where `V` is a later, unsupplied parameter)
    /// must resolve to an any-like type rather than leaking an unresolved
    /// placeholder, because phase 2 pre-seeds every unsupplied parameter with
    /// `any` before defaults are processed in declaration order.
    #[test]
    fn from_args_forward_reference_default_is_any_like() {
        let interner = TypeInterner::new();
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let v_param_ty = interner.type_param(param(v, None));
        // U defaults to the *later* parameter V; V has no default/arg.
        let params = vec![param(u, Some(v_param_ty)), param(v, None)];

        let subst = TypeSubstitution::from_args(&interner, &params, &[]);

        // U sees V's phase-2 `any` seed.
        assert_eq!(subst.get(u), Some(TypeId::ANY));
        // V itself has no default and no arg, so it is removed.
        assert_eq!(subst.get(v), None);
    }
}
