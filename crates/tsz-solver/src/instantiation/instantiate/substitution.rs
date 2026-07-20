use super::api::instantiate_type;
use super::api_lazy::{maybe_evaluate_concrete_conditional, type_references_param};
use crate::construction::TypeDatabase;
use crate::types::{TypeId, TypeParamBinderKey, TypeParamInfo};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tsz_common::interner::Atom;

/// A substitution map from type parameters to concrete types.
///
/// Most substitutions use the legacy name map. A substitution for a
/// declaration-scoped generic signature additionally records the exact
/// declaration origins owned by that signature. Once a name has that exact
/// domain, a same-named foreign binder is protected from both substitution and
/// constraint fallback.
#[derive(Clone, Debug, Default)]
pub struct TypeSubstitution {
    /// Maps type parameter names to their substituted types.
    pub(super) map: FxHashMap<Atom, TypeId>,
    /// Rare exact-identity overlay. Reference-counted out of line so the legacy
    /// substitution stays one name map plus one nullable pointer, while scratch
    /// substitutions can share the immutable domain without copying its maps.
    identity_domain: Option<Arc<IdentitySubstitutionDomain>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct IdentitySubstitutionDomain {
    /// Exact declaration binders owned by this substitution. The declared name
    /// is part of the key because sibling JSDoc parameters share one owner
    /// node/comment position.
    identity_binders: FxHashSet<TypeParamBinderKey>,
    /// Names for which name-only fallback is forbidden. Kept separately so a
    /// same-named foreign `TypeId` can be rejected in O(1).
    identity_names: FxHashSet<Atom>,
    /// Canonical content used by cross-call cache keys. Kept sorted as the
    /// domain is built so hashing a cache probe does not allocate or sort.
    canonical_binders: SmallVec<[TypeParamBinderKey; 4]>,
}

impl PartialEq for IdentitySubstitutionDomain {
    fn eq(&self, other: &Self) -> bool {
        self.canonical_binders == other.canonical_binders
    }
}

impl Eq for IdentitySubstitutionDomain {}

impl Hash for IdentitySubstitutionDomain {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.canonical_binders.hash(state);
    }
}

impl IdentitySubstitutionDomain {
    /// Heap retained by one exact-identity domain allocation.
    ///
    /// The `Arc` allocation owns the domain value itself plus both hash-table
    /// buffers and, for unusually wide generic signatures, the spilled
    /// canonical binder buffer. Callers deduplicate this amount by `Arc`
    /// pointer when multiple cache keys share one immutable domain.
    pub(crate) fn estimated_heap_bytes(&self, bucket_overhead: usize) -> usize {
        let mut size = std::mem::size_of::<Self>();
        size += self.identity_binders.capacity()
            * (bucket_overhead + std::mem::size_of::<TypeParamBinderKey>());
        size += self.identity_names.capacity() * (bucket_overhead + std::mem::size_of::<Atom>());
        if self.canonical_binders.spilled() {
            size += self.canonical_binders.capacity() * std::mem::size_of::<TypeParamBinderKey>();
        }
        size
    }
}

impl TypeSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
            identity_domain: None,
        }
    }

    /// Create an empty substitution whose ownership domain is the supplied
    /// callable signature's type parameters.
    ///
    /// This is used by contextual-inference collectors before they have chosen
    /// concrete bindings. It stays allocation-equivalent to [`Self::new`] for
    /// ordinary unstamped signatures; only declaration-scoped signatures add
    /// the rare exact-identity overlay.
    pub fn for_signature_domain(type_params: &[TypeParamInfo]) -> Self {
        let mut substitution = Self::new();
        substitution.protect_type_parameters(type_params);
        substitution
    }

    /// Create a substitution containing a single `name -> type_id` binding.
    /// Equivalent to `let mut s = TypeSubstitution::new(); s.insert(name, type_id);`.
    pub fn single(name: Atom, type_id: TypeId) -> Self {
        let mut map = FxHashMap::with_capacity_and_hasher(1, Default::default());
        map.insert(name, type_id);
        Self {
            map,
            identity_domain: None,
        }
    }

    /// Clear the substitution for reuse, preserving the name-map capacity and
    /// releasing any rare exact-identity overlay.
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
        self.identity_domain = None;
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
        Self::from_args_impl(interner, type_params, type_args, false)
    }

    fn from_args_impl(
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
        protect_signature_domain: bool,
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
        let mut subst = Self {
            map,
            identity_domain: None,
        };
        if protect_signature_domain {
            subst.protect_type_parameters(type_params);
        }
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
                    let final_type = if type_references_param(interner, resolved, *param) {
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

    /// Create an explicit-argument substitution for a callable signature.
    ///
    /// Unlike general application substitution, a callable's locally-owned
    /// declaration-scoped parameters must not rewrite same-named binders
    /// captured in its parameter or return shapes. The rare identity overlay is
    /// installed up front so defaults and every later signature component share
    /// the same exact domain.
    pub fn from_signature_args(
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
    ) -> Self {
        Self::from_args_impl(interner, type_params, type_args, true)
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

    /// Restrict declaration-scoped parameter names to the exact origins owned
    /// by a generic signature.
    ///
    /// The overlay is absent for the common legacy `User` origin. Selectively
    /// stamped signatures allocate it once, then every reconstructed occurrence
    /// of the same declaration resolves through the one name-map slot.
    pub(crate) fn protect_type_parameters(&mut self, type_params: &[TypeParamInfo]) {
        let mut scoped = type_params
            .iter()
            .filter(|type_param| type_param.origin.is_decl_scoped());
        let Some(first) = scoped.next() else {
            return;
        };
        let domain = self
            .identity_domain
            .get_or_insert_with(|| Arc::new(IdentitySubstitutionDomain::default()));
        let domain = Arc::make_mut(domain);
        for type_param in std::iter::once(first).chain(scoped) {
            let binder = type_param
                .declaration_binder_key()
                .expect("filtered declaration-scoped parameter must have a binder key");
            if domain.identity_binders.insert(binder) {
                domain.canonical_binders.push(binder);
            }
            domain.identity_names.insert(type_param.name);
        }
        domain.canonical_binders.sort_unstable();
    }

    /// Look up a concrete type-parameter occurrence under the hybrid domain.
    /// Exact declaration origins win; a protected same-named foreign identity
    /// never falls through to the legacy name map.
    pub(crate) fn get_for_type_parameter(&self, info: &TypeParamInfo) -> Option<TypeId> {
        if let Some(domain) = self.identity_domain.as_deref() {
            if info
                .declaration_binder_key()
                .is_some_and(|binder| domain.identity_binders.contains(&binder))
            {
                return self.map.get(&info.name).copied();
            }
            if domain.identity_names.contains(&info.name) {
                return None;
            }
        }
        self.get(info.name)
    }

    /// Whether this substitution owns a concrete type-parameter occurrence.
    /// Used by generic-call fast-path classification so a captured same-named
    /// binder cannot masquerade as the called signature's local parameter.
    pub(crate) fn binds_type_parameter(&self, info: &TypeParamInfo) -> bool {
        if let Some(domain) = self.identity_domain.as_deref() {
            if info
                .declaration_binder_key()
                .is_some_and(|binder| domain.identity_binders.contains(&binder))
            {
                return self.map.contains_key(&info.name);
            }
            if domain.identity_names.contains(&info.name) {
                return false;
            }
        }
        self.map.contains_key(&info.name)
    }

    /// Whether `info` belongs to the signature domain represented by
    /// `fallback_names` and this substitution's optional exact overlay.
    ///
    /// The fallback set preserves the common unstamped name-keyed path. For a
    /// protected declaration name, only the exact declaration origin is owned;
    /// a captured same-spelled binder is foreign even before the substitution
    /// has collected a value for the owned parameter.
    pub fn domain_contains_type_parameter(
        &self,
        info: &TypeParamInfo,
        fallback_names: &FxHashSet<Atom>,
    ) -> bool {
        if let Some(domain) = self.identity_domain.as_deref() {
            if info
                .declaration_binder_key()
                .is_some_and(|binder| domain.identity_binders.contains(&binder))
            {
                return true;
            }
            if domain.identity_names.contains(&info.name) {
                return false;
            }
        }
        fallback_names.contains(&info.name)
    }

    /// Start an empty collection with the same exact signature domain.
    ///
    /// Return-context union probing needs isolated candidate maps, but binder
    /// ownership must remain identical in every probe.
    pub fn empty_with_same_domain(&self) -> Self {
        Self {
            map: FxHashMap::default(),
            identity_domain: self.identity_domain.clone(),
        }
    }

    /// Whether `name` is excluded from the unmapped-parameter constraint
    /// fallback. Exact-domain misses must remain their original binder rather
    /// than silently widening through a constraint.
    pub(crate) fn protects_type_parameter_name(&self, name: Atom) -> bool {
        self.identity_domain
            .as_deref()
            .is_some_and(|domain| domain.identity_names.contains(&name))
    }

    /// Share the immutable exact binder domain with a cache key.
    pub(crate) fn identity_domain_for_cache(&self) -> Option<Arc<IdentitySubstitutionDomain>> {
        self.identity_domain.clone()
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
    /// Exact-identity domains are intentionally absent from this pair vector:
    /// request key construction attaches the substitution's shared immutable
    /// domain as a separate content-hashable key component.
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
    use super::instantiate_type;
    use crate::TypeInterner;
    use crate::types::{
        FunctionShape, TupleElement, TypeData, TypeId, TypeParamInfo, TypeParamOrigin,
    };
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

    fn tuple_members(interner: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
        let Some(TypeData::Tuple(list_id)) = interner.lookup(type_id) else {
            panic!("expected tuple, got {:?}", interner.lookup(type_id));
        };
        interner
            .tuple_list(list_id)
            .iter()
            .map(|element| element.type_id)
            .collect()
    }

    #[test]
    fn exact_domain_substitutes_only_the_owned_same_surface_binder() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("U");
        let file = interner.intern_string("identity.ts");
        let local_info = TypeParamInfo {
            name,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..local_info
        };
        let local = interner.fresh_type_param(local_info);
        let foreign = interner.fresh_type_param(foreign_info);
        assert_ne!(local, foreign);
        let root = interner.tuple(vec![
            TupleElement::fixed(local),
            TupleElement::fixed(foreign),
        ]);

        let mut substitution = TypeSubstitution::new();
        substitution.insert(name, TypeId::NUMBER);
        substitution.protect_type_parameters(&[local_info]);
        let result = instantiate_type(&interner, root, &substitution);

        assert_eq!(
            tuple_members(&interner, result),
            vec![TypeId::NUMBER, foreign]
        );

        // The name/value cache component is identical, but changing the exact
        // owner must produce a distinct result rather than hitting a name-only
        // project-cache entry.
        let mut other_owner = TypeSubstitution::new();
        other_owner.insert(name, TypeId::NUMBER);
        other_owner.protect_type_parameters(&[foreign_info]);
        let other_result = instantiate_type(&interner, root, &other_owner);
        assert_eq!(
            tuple_members(&interner, other_result),
            vec![local, TypeId::NUMBER],
        );
    }

    #[test]
    fn jsdoc_exact_domain_uses_comment_position_and_origin_kind() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("Value");
        let file = interner.intern_string("identity.js");
        let owned_info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::JsdocCommentScoped { file, pos: 10 },
        };
        let reconstructed_info = TypeParamInfo {
            constraint: Some(TypeId::STRING),
            ..owned_info
        };
        let foreign_jsdoc_info = TypeParamInfo {
            origin: TypeParamOrigin::JsdocCommentScoped { file, pos: 20 },
            ..owned_info
        };
        let ast_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 10 },
            ..owned_info
        };
        let legacy_info = TypeParamInfo::simple(name);

        assert!(owned_info.is_same_binder(reconstructed_info));
        assert!(!owned_info.is_same_binder(foreign_jsdoc_info));
        assert!(!owned_info.is_same_binder(ast_info));
        assert!(legacy_info.is_same_binder(TypeParamInfo {
            constraint: Some(TypeId::NUMBER),
            ..legacy_info
        }));

        let reconstructed = interner.fresh_type_param(reconstructed_info);
        let foreign_jsdoc = interner.fresh_type_param(foreign_jsdoc_info);
        let ast = interner.fresh_type_param(ast_info);
        let root = interner.tuple(vec![
            TupleElement::fixed(reconstructed),
            TupleElement::fixed(foreign_jsdoc),
            TupleElement::fixed(ast),
        ]);
        let substitution =
            TypeSubstitution::from_signature_args(&interner, &[owned_info], &[TypeId::NUMBER]);

        assert_eq!(
            tuple_members(&interner, instantiate_type(&interner, root, &substitution)),
            vec![TypeId::NUMBER, foreign_jsdoc, ast],
        );
    }

    #[test]
    fn exact_domain_distinguishes_sibling_binders_at_one_jsdoc_site() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("siblings.js");
        let t_name = interner.intern_string("T");
        let u_name = interner.intern_string("U");

        for origin in [
            TypeParamOrigin::DeclScoped { file, node: 10 },
            TypeParamOrigin::JsdocOwnerScoped { file, node: 10 },
            TypeParamOrigin::JsdocCommentScoped { file, pos: 20 },
        ] {
            let t_info = TypeParamInfo {
                origin,
                ..TypeParamInfo::simple(t_name)
            };
            let u_info = TypeParamInfo {
                origin,
                ..TypeParamInfo::simple(u_name)
            };
            assert!(!t_info.is_same_binder(u_info));

            let reconstructed_t = interner.fresh_type_param(TypeParamInfo {
                constraint: Some(TypeId::OBJECT),
                ..t_info
            });
            let reconstructed_u = interner.fresh_type_param(TypeParamInfo {
                default: Some(TypeId::UNKNOWN),
                ..u_info
            });
            let tuple = interner.tuple(vec![
                TupleElement::fixed(reconstructed_t),
                TupleElement::fixed(reconstructed_u),
            ]);
            let substitution = TypeSubstitution::from_signature_args(
                &interner,
                &[t_info, u_info],
                &[TypeId::NUMBER, TypeId::STRING],
            );

            assert_eq!(
                tuple_members(&interner, instantiate_type(&interner, tuple, &substitution)),
                vec![TypeId::NUMBER, TypeId::STRING],
            );
        }
    }

    #[test]
    fn exact_domain_scratch_shares_the_out_of_line_domain() {
        let interner = TypeInterner::new();
        let info = TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("scratch-domain.ts"),
                node: 1,
            },
        };
        let substitution = TypeSubstitution::for_signature_domain(&[info]);
        let scratch = substitution.empty_with_same_domain();

        assert!(std::sync::Arc::ptr_eq(
            substitution
                .identity_domain
                .as_ref()
                .expect("scoped signature must have an exact domain"),
            scratch
                .identity_domain
                .as_ref()
                .expect("scratch substitution must preserve the exact domain"),
        ));
        assert!(
            TypeSubstitution::for_signature_domain(&[TypeParamInfo::simple(info.name)])
                .identity_domain
                .is_none(),
            "the common unstamped path must not allocate an exact domain",
        );
    }

    #[test]
    fn exact_domain_foreign_binder_skips_constraint_fallback() {
        let interner = TypeInterner::new();
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let file = interner.intern_string("identity.ts");
        let dependency = interner.fresh_type_param(TypeParamInfo {
            name: v,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        });
        let local_info = TypeParamInfo {
            name: u,
            constraint: Some(dependency),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(TypeParamInfo {
            name: u,
            constraint: Some(dependency),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
        });

        let mut substitution = TypeSubstitution::new();
        substitution.insert(u, TypeId::STRING);
        substitution.insert(v, TypeId::NUMBER);
        substitution.protect_type_parameters(&[local_info]);

        assert_eq!(instantiate_type(&interner, foreign, &substitution), foreign);
    }

    #[test]
    fn legacy_unstamped_substitution_remains_name_keyed() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("T");
        let info = param(name, None);
        let first = interner.fresh_type_param(info);
        let second = interner.fresh_type_param(info);
        let root = interner.tuple(vec![
            TupleElement::fixed(first),
            TupleElement::fixed(second),
        ]);

        let substitution = TypeSubstitution::single(name, TypeId::BOOLEAN);
        let result = instantiate_type(&interner, root, &substitution);

        assert_eq!(
            tuple_members(&interner, result),
            vec![TypeId::BOOLEAN, TypeId::BOOLEAN],
        );
    }

    #[test]
    fn exact_domain_does_not_affect_a_renamed_foreign_binder() {
        let interner = TypeInterner::new();
        let local_name = interner.intern_string("Local");
        let foreign_name = interner.intern_string("Foreign");
        let file = interner.intern_string("identity.ts");
        let local_info = TypeParamInfo {
            name: local_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let local = interner.fresh_type_param(local_info);
        let foreign = interner.fresh_type_param(TypeParamInfo {
            name: foreign_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
        });
        let root = interner.tuple(vec![
            TupleElement::fixed(local),
            TupleElement::fixed(foreign),
        ]);

        let mut substitution = TypeSubstitution::new();
        substitution.insert(local_name, TypeId::STRING);
        substitution.protect_type_parameters(&[local_info]);

        assert_eq!(
            tuple_members(&interner, instantiate_type(&interner, root, &substitution)),
            vec![TypeId::STRING, foreign],
        );
    }

    #[test]
    fn signature_exact_domain_descends_into_nested_generic_return() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nested.ts");
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let owned_info = TypeParamInfo {
            name: u,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned_info
        });
        // A reconstructed occurrence of the owned declaration deliberately has
        // a distinct `TypeId`; its declaration origin is the stable identity.
        let reconstructed_owned = interner.fresh_type_param(owned_info);
        let nested_param = TypeParamInfo {
            name: v,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        };
        let nested = interner.function(FunctionShape {
            type_params: vec![nested_param],
            params: Vec::new(),
            this_type: None,
            return_type: interner.tuple(vec![
                TupleElement::fixed(foreign),
                TupleElement::fixed(reconstructed_owned),
            ]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let substitution =
            TypeSubstitution::from_signature_args(&interner, &[owned_info], &[TypeId::NUMBER]);
        let result = instantiate_type(&interner, nested, &substitution);
        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!(
                "expected nested function, got {:?}",
                interner.lookup(result)
            );
        };
        let shape = interner.function_shape(shape_id);

        assert_eq!(shape.type_params, vec![nested_param]);
        assert_eq!(
            tuple_members(&interner, shape.return_type),
            vec![foreign, TypeId::NUMBER],
        );
    }

    #[test]
    fn nested_same_named_binder_shadows_only_its_own_identity() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nested-shadow.ts");
        let name = interner.intern_string("U");
        let owned_info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let nested_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned_info
        };
        let captured_outer = interner.fresh_type_param(owned_info);
        let nested_local = interner.fresh_type_param(nested_info);
        let nested = interner.function(FunctionShape {
            type_params: vec![nested_info],
            params: vec![crate::types::ParamInfo::unnamed(nested_local)],
            this_type: None,
            return_type: interner.tuple(vec![
                TupleElement::fixed(captured_outer),
                TupleElement::fixed(nested_local),
            ]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let substitution =
            TypeSubstitution::from_signature_args(&interner, &[owned_info], &[TypeId::NUMBER]);
        let result = instantiate_type(&interner, nested, &substitution);
        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!(
                "expected nested function, got {:?}",
                interner.lookup(result)
            );
        };
        let shape = interner.function_shape(shape_id);

        assert_eq!(shape.type_params, vec![nested_info]);
        assert_eq!(shape.params[0].type_id, nested_local);
        assert_eq!(
            tuple_members(&interner, shape.return_type),
            vec![TypeId::NUMBER, nested_local],
        );
    }

    #[test]
    fn rewritten_nested_local_lookup_is_declaration_aware() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nested-local.ts");
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let owned_u = TypeParamInfo {
            name: u,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let owned_v = TypeParamInfo {
            name: v,
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned_u
        };
        let owned_v_occurrence = interner.fresh_type_param(owned_v);
        let nested_u = TypeParamInfo {
            name: u,
            constraint: Some(owned_v_occurrence),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        };
        let captured_u = interner.fresh_type_param(owned_u);
        let nested_u_occurrence = interner.fresh_type_param(nested_u);
        let nested = interner.function(FunctionShape {
            type_params: vec![nested_u],
            params: vec![crate::types::ParamInfo::unnamed(nested_u_occurrence)],
            this_type: None,
            return_type: interner.tuple(vec![
                TupleElement::fixed(captured_u),
                TupleElement::fixed(nested_u_occurrence),
            ]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let substitution = TypeSubstitution::from_signature_args(
            &interner,
            &[owned_u, owned_v],
            &[TypeId::NUMBER, TypeId::STRING],
        );
        let result = instantiate_type(&interner, nested, &substitution);
        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!(
                "expected nested function, got {:?}",
                interner.lookup(result)
            );
        };
        let shape = interner.function_shape(shape_id);
        let rewritten_local = shape.type_params[0];

        assert_eq!(rewritten_local.origin, nested_u.origin);
        assert_eq!(rewritten_local.constraint, Some(TypeId::STRING));
        assert_eq!(
            shape.params[0].type_id,
            interner.type_param(rewritten_local)
        );
        assert_eq!(
            tuple_members(&interner, shape.return_type),
            vec![TypeId::NUMBER, interner.type_param(rewritten_local)],
        );
    }

    #[test]
    fn signature_default_can_capture_foreign_same_named_binder() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("default-capture.ts");
        let name = interner.intern_string("U");
        let foreign_info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(foreign_info);
        let owned_info = TypeParamInfo {
            default: Some(foreign),
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..foreign_info
        };

        let substitution = TypeSubstitution::from_signature_args(&interner, &[owned_info], &[]);

        assert_eq!(substitution.get(name), Some(foreign));
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
