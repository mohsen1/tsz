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
        for (i, param) in type_params.iter().enumerate() {
            if i < type_args.len() {
                map.insert(param.name, type_args[i]);
            }
        }

        // Phase 2: Pre-fill unsupplied type parameters with `any` so that
        // circular and forward references in defaults become any-like instead
        // of leaking unresolved placeholders into the instantiated type.
        for (i, param) in type_params.iter().enumerate() {
            if i >= type_args.len() {
                map.insert(param.name, TypeId::ANY);
            }
        }

        // Phase 3: Process defaults in declaration order. Each default is
        // instantiated with the substitution built so far (which includes
        // explicitly provided args, already-resolved defaults, and `any` for
        // not-yet-resolved params). This means a forward reference like `U = V`
        // where V hasn't been processed yet resolves to an any-like type.
        for (i, param) in type_params.iter().enumerate() {
            if i < type_args.len() {
                continue; // already provided explicitly
            }
            match param.default {
                Some(default) => {
                    let subst = Self { map: map.clone() };
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
                    map.insert(param.name, final_type);
                }
                None => {
                    // No default and no argument - remove the error placeholder
                    // so this parameter remains unsubstituted.
                    map.remove(&param.name);
                }
            }
        }

        Self { map }
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
