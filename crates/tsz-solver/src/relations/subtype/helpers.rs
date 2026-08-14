//! Subtype checker helper methods.
//!
//! Contains intersection optimization, cache key construction,
//! public entry points, and special-case subtype checks
//! (Object contract, generic index access).

use crate::def::resolver::TypeResolver;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::objects::PropertyCollectionResult;
use crate::relations::relation_queries::RelationPolicy;
use crate::relations::subtype::{
    AnyPropagationMode, INTERSECTION_OBJECT_FAST_PATH_THRESHOLD, SubtypeChecker, SubtypeResult,
};
use crate::types::{
    CachedAnyMode, ObjectFlags, ObjectShape, RelationCacheKey, RelationFlags, TypeData, TypeId,
    TypeParamInfo, TypeParamOrigin, Visibility,
};
use crate::visitor::{
    callable_shape_id, function_shape_id, index_access_parts, intersection_list_id,
    keyof_inner_type, literal_string, mapped_type_id, object_shape_id, object_with_index_shape_id,
    type_param_info, union_list_id,
};
use rustc_hash::FxHashMap;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    pub(crate) const fn allows_bivariant_param_count(&self, is_method_like: bool) -> bool {
        self.allow_bivariant_param_count
            && (!self.strict_function_types || (is_method_like && !self.disable_method_bivariance))
    }

    /// tsc's `someTypeRelatedToType` fast path: whether a source intersection —
    /// interned directly, or recovered from a *merged*-intersection object — is
    /// related to `target` because one constituent already is.
    ///
    /// A source object built by merging an intersection keeps a
    /// `merged_intersection_origin` back to that intersection. The global
    /// `window` value's type is a materialized `Window & typeof globalThis`
    /// surface whose own `window`/`self`/`frames` members are again
    /// `Window & typeof globalThis`, so a structural walk against `Window`
    /// re-mints `this`-bound `Window` instantiations without converging and
    /// exhausts the relation budget — TS2859 for a direct assignment, or a
    /// spurious TS2322 when a property/argument context turns the depth-exceeded
    /// verdict into `False` (issue #17390). Recovering the origin intersection
    /// and short-circuiting on the `Window` constituent matches tsc and
    /// sidesteps the walk. The caller runs this early, before the target is
    /// resolved through its `Lazy` reference: the origin's `Window` constituent
    /// and a `Lazy(Window)` target both carry the lib def, whereas the merged
    /// surface's own materialized `Window` members and a resolved-to-body target
    /// carry no def/symbol, so a later check could not match them.
    ///
    /// A constituent qualifies when it literally IS the target (`A & T <: T`,
    /// sound for any target) or when its verified nominal heritage reaches the
    /// target's def, so this only ever returns `true` for a genuine subtype.
    pub(crate) fn intersection_or_merged_source_satisfies_target(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if source == target || source.is_intrinsic() {
            return false;
        }
        let members = intersection_list_id(self.interner, source).or_else(|| {
            self.interner
                .get_merged_intersection_origin(source)
                .filter(|&origin| origin != source)
                .and_then(|origin| intersection_list_id(self.interner, origin))
        });
        let Some(members) = members else {
            return false;
        };
        let member_list = self.interner.type_list(members);
        let target_shape = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target))
            .map(|id| self.interner.object_shape(id));
        member_list.iter().any(|&member| {
            member == target
                || self.intersection_member_nominally_extends_target(
                    member,
                    target,
                    target_shape.as_deref(),
                )
        })
    }

    /// Build a name-keyed substitution that erases authoritative declaration
    /// stamps for genuinely alpha-equivalent free type parameters in `roots`.
    ///
    /// Only same-name, same-surface (`constraint`/`default`/`is_const`) params
    /// collapse to the same `User`-canonical id. If the same name appears with
    /// conflicting surfaces, that name is poisoned and left unstripped.
    pub(crate) fn build_decl_param_structural_strip_for_roots(
        &self,
        roots: impl IntoIterator<Item = TypeId>,
    ) -> TypeSubstitution {
        let free_ids =
            crate::visitors::visitor_predicates::free_type_parameter_ids_in(self.interner, roots);

        let mut by_name: FxHashMap<tsz_common::interner::Atom, Option<TypeId>> =
            FxHashMap::default();
        for id in free_ids {
            let Some(info) = type_param_info(self.interner, id) else {
                continue;
            };
            if !info.origin.is_decl_scoped() {
                continue;
            }

            let canonical = self.interner.type_param(TypeParamInfo {
                origin: TypeParamOrigin::User,
                ..info
            });
            match by_name.get_mut(&info.name) {
                None => {
                    by_name.insert(info.name, Some(canonical));
                }
                Some(slot @ Some(_)) if *slot == Some(canonical) => {}
                Some(slot) => {
                    *slot = None;
                }
            }
        }

        let mut strip = TypeSubstitution::new();
        for (name, canonical) in by_name {
            if let Some(canonical) = canonical {
                strip.insert(name, canonical);
            }
        }
        strip
    }

    pub(crate) fn check_decl_stripped_lazy_application_index_access_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeResult> {
        let (source_object, _source_key) = index_access_parts(self.interner, source)?;
        let (target_object, _target_key) = index_access_parts(self.interner, target)?;
        if !self.same_lazy_application_base(source_object, target_object) {
            return None;
        }

        let strip = self.build_decl_param_structural_strip_for_roots([source, target]);
        if strip.is_empty() {
            return None;
        }

        let stripped_source = instantiate_type(self.interner, source, &strip);
        let stripped_target = instantiate_type(self.interner, target, &strip);
        if stripped_source == source && stripped_target == target {
            return None;
        }

        Some(self.check_subtype(stripped_source, stripped_target))
    }

    fn same_lazy_application_base(&self, source_object: TypeId, target_object: TypeId) -> bool {
        let Some(TypeData::Application(source_app_id)) = self.interner.lookup(source_object) else {
            return false;
        };
        let Some(TypeData::Application(target_app_id)) = self.interner.lookup(target_object) else {
            return false;
        };

        let source_app = self.interner.type_application(source_app_id);
        let target_app = self.interner.type_application(target_app_id);
        if source_app.args.len() != target_app.args.len() {
            return false;
        }

        matches!(
            (
                self.interner.lookup(source_app.base),
                self.interner.lookup(target_app.base),
            ),
            (Some(TypeData::Lazy(source_def)), Some(TypeData::Lazy(target_def)))
                if source_def == target_def
        )
    }

    pub(crate) fn resolved_type_param_info(
        &self,
        type_id: TypeId,
    ) -> Option<crate::types::TypeParamInfo> {
        type_param_info(self.interner, type_id).or_else(|| {
            let resolved = self.resolve_lazy_type(type_id);
            (resolved != type_id)
                .then(|| type_param_info(self.interner, resolved))
                .flatten()
        })
    }

    pub(crate) fn resolved_type_param_type_id(&self, type_id: TypeId) -> Option<TypeId> {
        if type_param_info(self.interner, type_id).is_some() {
            return Some(type_id);
        }

        let resolved = self.resolve_lazy_type(type_id);
        (resolved != type_id && type_param_info(self.interner, resolved).is_some())
            .then_some(resolved)
    }

    pub(crate) fn index_accesses_have_same_object_distinct_type_param_keys(
        &self,
        source_object: TypeId,
        source_key: TypeId,
        target_object: TypeId,
        target_key: TypeId,
    ) -> bool {
        let resolved_source_object = self.resolve_lazy_type(source_object);
        let resolved_target_object = self.resolve_lazy_type(target_object);
        if source_object != target_object && resolved_source_object != resolved_target_object {
            return false;
        }

        self.index_accesses_have_distinct_type_param_keys(source_key, target_key)
    }

    pub(crate) fn index_accesses_have_distinct_type_param_keys(
        &self,
        source_key: TypeId,
        target_key: TypeId,
    ) -> bool {
        let Some(source_param) = self.resolved_type_param_type_id(source_key) else {
            return false;
        };
        let Some(target_param) = self.resolved_type_param_type_id(target_key) else {
            return false;
        };

        !self.type_params_equivalent_in_current_relation(source_param, target_param)
    }

    pub(crate) fn type_params_equivalent_in_current_relation(
        &self,
        source_param: TypeId,
        target_param: TypeId,
    ) -> bool {
        source_param == target_param
            || self
                .type_param_equivalences
                .iter()
                .any(|eq| eq.matches_ids(source_param, target_param))
    }

    /// Build the elaboration carrier for two distinct type-parameter keys
    /// of an index access mismatch.
    ///
    /// Returns `None` when either key does not resolve to a `TypeParameter`
    /// kind — callers must fall back to the generic mismatch reason.
    /// Surface keys (not the underlying resolved types) are used so the
    /// rendered message matches what the user wrote at the use site.
    pub(crate) fn index_access_distinct_type_param_keys_failure_reason(
        &self,
        source_key: TypeId,
        target_key: TypeId,
    ) -> Option<crate::diagnostics::SubtypeFailureReason> {
        if !self.index_accesses_have_distinct_type_param_keys(source_key, target_key) {
            return None;
        }
        let target_info = self.resolved_type_param_info(target_key)?;
        Some(
            crate::diagnostics::SubtypeFailureReason::IndexAccessTypeParameterMismatch {
                source_param: source_key,
                target_param: target_key,
                target_constraint: target_info.constraint,
            },
        )
    }

    pub(crate) fn can_use_object_intersection_fast_path(&self, members: &[TypeId]) -> bool {
        let has_finite_mapped_member = members.iter().any(|&member| {
            let resolved = self.resolve_lazy_type(member);
            mapped_type_id(self.interner, resolved).is_some_and(|mapped_id| {
                crate::type_queries::collect_finite_mapped_property_names(self.interner, mapped_id)
                    .is_some()
            })
        });

        if members.len() < INTERSECTION_OBJECT_FAST_PATH_THRESHOLD && !has_finite_mapped_member {
            return false;
        }

        for &member in members {
            let resolved = self.resolve_lazy_type(member);

            // Callable requirements must remain explicit intersection members.
            // Collapsing to a merged object target would drop call signatures.
            if callable_shape_id(self.interner, resolved).is_some()
                || function_shape_id(self.interner, resolved).is_some()
            {
                return false;
            }

            if mapped_type_id(self.interner, resolved).is_some_and(|mapped_id| {
                crate::type_queries::collect_finite_mapped_property_names(self.interner, mapped_id)
                    .is_some()
            }) {
                continue;
            }

            let Some(shape_id) = object_shape_id(self.interner, resolved)
                .or_else(|| object_with_index_shape_id(self.interner, resolved))
            else {
                return false;
            };

            let shape = self.interner.object_shape(shape_id);
            if !shape.flags.is_empty() {
                return false;
            }
            if shape
                .properties
                .iter()
                .any(|prop| prop.visibility != Visibility::Public)
            {
                return false;
            }
        }

        true
    }

    pub(crate) fn build_object_intersection_target(
        &self,
        target_intersection: TypeId,
    ) -> Option<TypeId> {
        let resolver_generation = self.resolver.resolver_generation();
        // Check the shared QueryCache first to avoid expensive property collection
        // for large intersections checked across multiple SubtypeChecker instances.
        if let Some(db) = self.query_db
            && let Some(cached) =
                db.lookup_intersection_merge(target_intersection, resolver_generation)
        {
            return cached.into_result();
        }

        use crate::objects::collect_properties_cached;

        let result = match collect_properties_cached(
            target_intersection,
            self.interner,
            self.resolver,
            self.query_db,
        ) {
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
                symbol_index,
            } => {
                let shape = ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties,
                    string_index,
                    number_index,
                    symbol_index,
                    symbol: None,
                };

                if shape.string_index.is_some()
                    || shape.number_index.is_some()
                    || shape.symbol_index.is_some()
                {
                    Some(self.interner.object_with_index(shape))
                } else {
                    Some(self.interner.object(shape.properties))
                }
            }
            PropertyCollectionResult::Any => Some(TypeId::ANY),
            PropertyCollectionResult::NonObject => None,
        };

        // Cache the result for subsequent SubtypeChecker instances.
        if let Some(db) = self.query_db {
            db.insert_intersection_merge(target_intersection, resolver_generation, result);
        }
        result
    }

    /// Check if two object types have overlapping properties.
    ///
    /// Returns false if any common property has non-overlapping types.
    /// Construct a `RelationCacheKey` for the current checker configuration.
    ///
    /// Produces a fully behavior-complete [`RelationCacheConfig`] so that
    /// results computed under different rules (strict vs non-strict, sound
    /// vs lax, with/without weak-type suppression, etc.) cannot
    /// contaminate each other.
    pub(crate) fn make_cache_key(&self, source: TypeId, target: TypeId) -> RelationCacheKey {
        self.make_cache_key_with_this_context(
            source,
            target,
            self.this_relation_context(source, target),
        )
    }

    /// Build the relation cache key for `(source, target)` with an
    /// already-resolved polymorphic-`this` discriminator.
    ///
    /// Callers on the hot cache-lookup path pass the discriminator they already
    /// computed for the gate decision (see `check_subtype`'s cache section), so
    /// the [`crate::contains_this_type`] walk in [`Self::this_relation_context`]
    /// is not repeated per lookup. Passing [`TypeId::NONE`] produces a key
    /// byte-identical to the legacy undiscriminated form.
    pub(crate) fn make_cache_key_with_this_context(
        &self,
        source: TypeId,
        target: TypeId,
        this_context: TypeId,
    ) -> RelationCacheKey {
        let (inheritance_graph_id, inheritance_graph_generation) = self
            .inheritance_graph
            .map_or((0, 0), |graph| (graph.identity(), graph.generation()));
        RelationCacheKey::for_subtype(
            source,
            target,
            self.cache_policy()
                .cache_config_with_cached_any_mode(self.effective_cached_any_mode()),
        )
        .with_this_context(this_context)
        .with_inheritance_graph_context(inheritance_graph_id, inheritance_graph_generation)
    }

    /// Resolve the polymorphic-`this` discriminator for a `(source, target)`
    /// pair (issue #13828).
    ///
    /// Returns the resolver's current `this` binding when the pair carries a
    /// polymorphic `this` (so its verdict depends on that binding), else
    /// [`TypeId::NONE`]. A `None` binding also yields [`TypeId::NONE`]: with no
    /// receiver to resolve `ThisType` against, the pair stays on the
    /// instance-local memo rather than the shared cache.
    pub(crate) fn this_relation_context(&self, source: TypeId, target: TypeId) -> TypeId {
        match self.resolver.resolve_this_type(self.interner) {
            Some(this_ty)
                if crate::contains_this_type(self.interner, source)
                    || crate::contains_this_type(self.interner, target) =>
            {
                this_ty
            }
            _ => TypeId::NONE,
        }
    }

    /// Project this checker's behavior-affecting relation modes to a policy.
    fn cache_policy(&self) -> RelationPolicy {
        let mut flags = RelationFlags::empty();
        if self.strict_null_checks {
            flags |= RelationFlags::STRICT_NULL_CHECKS;
        }
        if self.strict_function_types {
            flags |= RelationFlags::STRICT_FUNCTION_TYPES;
        }
        if self.exact_optional_property_types {
            flags |= RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES;
        }
        if self.strict_readonly_identity {
            flags |= RelationFlags::STRICT_READONLY_IDENTITY;
        }
        if self.no_unchecked_indexed_access {
            flags |= RelationFlags::NO_UNCHECKED_INDEXED_ACCESS;
        }
        if self.disable_method_bivariance {
            flags |= RelationFlags::DISABLE_METHOD_BIVARIANCE;
        }
        if self.in_callback_param_check {
            flags |= RelationFlags::IN_CALLBACK_PARAM_CHECK;
        }
        if self.allow_void_return {
            flags |= RelationFlags::ALLOW_VOID_RETURN;
        }
        if self.allow_bivariant_rest {
            flags |= RelationFlags::ALLOW_BIVARIANT_REST;
        }
        if self.allow_bivariant_param_count {
            flags |= RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT;
        }
        if !self.erase_generics {
            flags |= RelationFlags::NO_ERASE_GENERICS;
        }
        if self.allow_erased_generic_signature_retry {
            flags |= RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY;
        }
        if self.allow_provisional_rest_union && self.provisional_rest_union_function_depth == 0 {
            flags |= RelationFlags::PROVISIONAL_REST_UNION;
        }
        if self.assume_related_on_cycle {
            flags |= RelationFlags::ASSUME_RELATED_ON_CYCLE;
        }
        if self.assume_related_on_depth {
            flags |= RelationFlags::ASSUME_RELATED_ON_DEPTH;
        }
        // The class-symbol classifier is behavior-affecting (it can make a
        // no-`DefId` class-flagged symbol nominal), so discriminate verdicts
        // computed with it active from class-agnostic ones (issue #13828). The
        // classifier is a pure function of the program binder, fixed for the
        // whole compilation, so this single bit fully partitions the regimes.
        if self.is_class_symbol.is_some() {
            flags |= RelationFlags::CLASS_CHECK_CONTEXT;
        }

        RelationPolicy::from_relation_flags(flags)
            .with_any_propagation_mode(self.any_propagation)
            .with_assume_related_on_cycle(self.assume_related_on_cycle)
            .with_assume_related_on_depth(self.assume_related_on_depth)
    }

    /// Resolve depth-sensitive `any` propagation to the cache-key mode.
    const fn effective_cached_any_mode(&self) -> CachedAnyMode {
        // If `any_propagation` is `TopLevelOnly` but `depth > 0`, the
        // effective mode is nested so top-level checks cannot hit cached
        // nested answers.
        match self.any_propagation {
            AnyPropagationMode::All => CachedAnyMode::All,
            AnyPropagationMode::TopLevelOnly if self.guard.depth() == 0 => {
                CachedAnyMode::TopLevelOnlyAtTop
            }
            AnyPropagationMode::TopLevelOnly => CachedAnyMode::TopLevelOnlyNested,
            // Depth-independent mode: same behavior at every nesting level.
            AnyPropagationMode::AnySourceNotRelated => CachedAnyMode::AnySourceNotRelated,
            AnyPropagationMode::IdenticalOnly => CachedAnyMode::IdenticalOnly,
        }
    }

    /// Test-only accessor that exposes the cache key this checker would use
    /// for a given `(source, target)` pair. External crates should never call
    /// this — use the query boundary helpers instead.
    #[doc(hidden)]
    pub fn debug_cache_key_for(&self, source: TypeId, target: TypeId) -> RelationCacheKey {
        self.make_cache_key(source, target)
    }

    /// Check if `source` is a subtype of `target`.
    /// This is the main entry point for subtype checking.
    ///
    /// When a `QueryDatabase` is available (via `with_query_db`), fast-path checks
    /// (identity, any, unknown, never) are done locally, then the full structural
    /// check is delegated to the internal `check_subtype` which may use Salsa
    /// memoization for `evaluate_type` calls.
    pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        self.check_subtype(source, target).is_true()
    }

    /// Check if `source` is assignable to `target`.
    /// This is a strict structural check; use `CompatChecker` for TypeScript assignability rules.
    pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of(source, target)
    }

    /// Internal subtype check with cycle detection
    ///
    /// # Cycle Detection Strategy (Coinductive Semantics)
    ///
    /// This function implements coinductive cycle handling for recursive types.
    /// The key insight is that we must check for cycles BEFORE evaluation to handle
    /// "expansive" types like `type Deep<T> = { next: Deep<Box<T>> }` that produce
    /// fresh `TypeIds` on each evaluation.
    ///
    /// The algorithm:
    /// 1. Fast paths (identity, any, unknown, never)
    /// 2. **Cycle detection FIRST** (before evaluation!)
    /// 3. Meta-type evaluation (keyof, conditional, mapped, etc.)
    /// 4. Structural comparison
    ///
    /// Check if source satisfies the Object contract (conflicting properties check).
    ///
    /// The `Object` interface allows assignment from almost anything, but if the source
    /// provides properties that overlap with `Object` (e.g. `toString`), they must be compatible.
    pub(crate) fn check_object_contract(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> SubtypeResult {
        use crate::visitor::{object_shape_id, object_with_index_shape_id};

        // Type parameters must NOT short-circuit here: an unconstrained T could be
        // instantiated with null/undefined/void, which are NOT assignable to Object.
        // For constrained T, check if the constraint is assignable to Object.
        let source_eval = self.evaluate_type(source);
        if let Some(info) = crate::visitor::type_param_info(self.interner, source_eval) {
            return match info.constraint {
                Some(constraint) => self.check_object_contract(constraint, target),
                None => SubtypeResult::False,
            };
        }

        // Resolve source shape first - if not an object, it's valid (primitives match Object)
        let s_shape_id = match object_shape_id(self.interner, source_eval)
            .or_else(|| object_with_index_shape_id(self.interner, source_eval))
        {
            Some(id) => id,
            None => return SubtypeResult::True,
        };
        let s_shape = self.interner.object_shape(s_shape_id);

        // Resolve Object shape (target)
        let target_eval = self.evaluate_type(target);
        let t_shape_id = match object_shape_id(self.interner, target_eval)
            .or_else(|| object_with_index_shape_id(self.interner, target_eval))
        {
            Some(id) => id,
            None => return SubtypeResult::True, // Should not happen for Object interface
        };
        let t_shape = self.interner.object_shape(t_shape_id);

        // Check for conflicting properties
        for s_prop in &s_shape.properties {
            // Find property in Object interface (target)
            if let Some(t_prop) =
                self.lookup_property(&t_shape.properties, Some(t_shape_id), s_prop.name)
            {
                // Found potential conflict: check compatibility
                let result = self.check_property_compatibility(s_prop, t_prop, None, None);
                if !result.is_true() {
                    return result;
                }
            }
        }

        SubtypeResult::True
    }

    /// Check if source is a subtype of an `IndexAccess` target where the index is generic.
    ///
    /// If `Target` is `Obj[K]` where `K` is generic, we check if `Source <: Obj[C]`
    /// where `C` is the **effective index bound** of `K`.
    /// Specifically, if `C` is a union of string literals `"a" | "b"`, we verify
    /// `Source <: Obj["a"]` AND `Source <: Obj["b"]` (every key must accept the
    /// source, since `K` is universally quantified over its bound).
    ///
    /// ## Effective index bound
    /// The bound is `K`'s declared constraint when it is attached to the
    /// type-parameter node. When it is *not* attached at this relation site
    /// (`constraint == None`), the bound falls back to `keyof Obj`: for the
    /// deferred access `Obj[K]` to be well-formed at all, `K` must range over
    /// `keyof Obj`, so `keyof Obj` is the soundest upper bound to distribute
    /// over. Bailing to `false` here instead would fabricate a spurious
    /// `TS2322` — most visibly `{}` assigned to `JSX.IntrinsicElements[T]`,
    /// where every element's prop type is all-optional so `{}` is in fact
    /// assignable for any `T` (issue #12450). This fallback only widens *which*
    /// keys are checked; the all-keys-must-pass loop below still rejects a
    /// source that fails against any single value type, so required-property
    /// and distinct-key mismatches keep their correct `TS2322`.
    pub(crate) fn check_generic_index_access_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((t_obj, t_idx)) = index_access_parts(self.interner, target) else {
            return false;
        };

        // Special case: if source is also an index access with the same object type
        // but a different type parameter key, they are not subtypes even if they have
        // the same constraint. This prevents `T1[K] <: T2[K]` when T1 != T2.
        if let Some((s_obj, s_idx)) = index_access_parts(self.interner, source)
            && self.index_accesses_have_same_object_distinct_type_param_keys(
                s_obj, s_idx, t_obj, t_idx,
            )
        {
            return false;
        }

        // Nested generic indexed-access target `O[K]["p"]`: the outer index
        // `"p"` is concrete but the inner object `O[K]` is a deferred generic
        // indexed access (e.g. `MyObj[K]["name"]`). tsc relates the source to the
        // constraint of such a target — the value union reachable through `K`'s
        // constraint, indexed by the outer key — so `string | number <=
        // MyObj[K]["name"]` holds (correlatedUnions). Compute the inner upper
        // bounds, index each by the outer key, and check the source against the
        // resulting constraint.
        if type_param_info(self.interner, t_idx).is_none()
            && index_access_parts(self.interner, t_obj).is_some()
            && self.source_fits_nested_index_access_constraint(source, t_obj, t_idx)
        {
            return true;
        }

        // Check if index is a generic type parameter
        let Some(t_param) = type_param_info(self.interner, t_idx) else {
            return false;
        };

        // Use the declared constraint when present; otherwise fall back to
        // `keyof Obj` as the effective bound (see the doc comment). `Obj[K]`
        // only type-checks when `K extends keyof Obj`, so this is sound and
        // never widens the source acceptance — only the set of keys checked.
        let constraint = t_param
            .constraint
            .unwrap_or_else(|| self.interner.keyof(t_obj));

        // Evaluate the constraint to resolve any type aliases/applications
        let constraint = self.evaluate_type(constraint);

        // Collect all literal types from the constraint (if it's a union of literals)
        // If constraint is a single literal, treat as union of 1.
        let mut literals = Vec::new();

        if let Some(s) = literal_string(self.interner, constraint) {
            literals.push(self.interner.literal_string_atom(s));
        } else if let Some(union_id) = union_list_id(self.interner, constraint) {
            let members = self.interner.type_list(union_id);
            for &m in members.iter() {
                if let Some(s) = literal_string(self.interner, m) {
                    literals.push(self.interner.literal_string_atom(s));
                } else {
                    // Constraint contains non-string-literal (e.g. number, or generic).
                    // Can't distribute.
                    return false;
                }
            }
        } else {
            // Constraint is not a literal or union of literals.
            return false;
        }

        if literals.is_empty() {
            return false;
        }

        // Check source <: Obj[L] for all L in literals
        for lit_type in literals {
            // Create IndexAccess(Obj, L)
            // We use evaluate_type here to potentially resolve it to a concrete property type
            // (e.g. Obj["a"] -> string)
            let indexed_access = self.interner.index_access(t_obj, lit_type);
            let evaluated = self.evaluate_type(indexed_access);

            if !self.check_subtype(source, evaluated).is_true() {
                return false;
            }
        }

        true
    }

    /// Relate `source` to a nested deferred indexed-access target
    /// `inner_obj[outer_key]` where `inner_obj` is itself a generic indexed
    /// access (`O[K]`) and `outer_key` is concrete (`"name"`).
    ///
    /// tsc keeps `O[K]["name"]` deferred but relates a source to its constraint:
    /// the value union reachable through `K`'s constraint (`O[keyof O]`), indexed
    /// by the outer key — `({ name: string } | { name: number })["name"]` =
    /// `string | number`. The source is accepted when it is assignable to that
    /// constraint. This is the target-side analogue of the source-side
    /// constraint widening; it only relaxes the target via its declared upper
    /// bound, so an unrelated source (e.g. `boolean`) is still rejected.
    fn source_fits_nested_index_access_constraint(
        &mut self,
        source: TypeId,
        inner_obj: TypeId,
        outer_key: TypeId,
    ) -> bool {
        let Some((object_type, key_type)) = index_access_parts(self.interner, inner_obj) else {
            return false;
        };
        let original = self.interner.index_access(object_type, key_type);
        let mut candidates = Vec::new();
        self.collect_index_access_upper_bound_candidates(
            object_type,
            key_type,
            original,
            &mut candidates,
        );

        let mut indexed_constraints = Vec::new();
        for candidate in candidates {
            if candidate == original || candidate == TypeId::ERROR {
                continue;
            }
            let indexed = self.evaluate_type(self.interner.index_access(candidate, outer_key));
            // Skip an indexed result that is still a deferred index access — it
            // carries no concrete constraint to compare against and would loop.
            if indexed == TypeId::ERROR || index_access_parts(self.interner, indexed).is_some() {
                continue;
            }
            if !indexed_constraints.contains(&indexed) {
                indexed_constraints.push(indexed);
            }
        }

        if indexed_constraints.is_empty() {
            return false;
        }
        let constraint = crate::utils::union_or_single(self.interner, indexed_constraints);
        self.check_subtype(source, constraint).is_true()
    }

    pub(crate) fn check_index_access_source_upper_bound_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((object_type, key_type)) = index_access_parts(self.interner, source) else {
            return false;
        };

        // Special case: if target is also an index access with the same object type
        // but a different type parameter key, they are not subtypes even if they have
        // the same constraint. This prevents `T1[K] <: T2[K]` when T1 != T2.
        if let Some((t_obj, t_idx)) = index_access_parts(self.interner, target)
            && self.index_accesses_have_same_object_distinct_type_param_keys(
                object_type,
                key_type,
                t_obj,
                t_idx,
            )
        {
            return false;
        }

        let original = self.interner.index_access(object_type, key_type);
        let mut candidates = Vec::new();
        self.collect_index_access_upper_bound_candidates(
            object_type,
            key_type,
            original,
            &mut candidates,
        );

        candidates.into_iter().any(|candidate| {
            candidate != original
                && candidate != TypeId::ERROR
                && self.check_subtype(candidate, target).is_true()
        })
    }

    fn collect_index_access_upper_bound_candidates(
        &mut self,
        object_type: TypeId,
        key_type: TypeId,
        original: TypeId,
        candidates: &mut Vec<TypeId>,
    ) {
        let evaluated = self.evaluate_type(self.interner.index_access(object_type, key_type));
        if evaluated != original && !candidates.contains(&evaluated) {
            candidates.push(evaluated);
        }

        if let Some(info) = type_param_info(self.interner, object_type) {
            if let Some(constraint) = info.constraint
                && !crate::visitor::is_type_parameter(self.interner, constraint)
                && !crate::visitor::is_this_type(self.interner, constraint)
            {
                self.collect_index_access_upper_bound_candidates(
                    constraint, key_type, original, candidates,
                );
            } else if info.constraint.is_none() {
                // Unconstrained type parameters have implicit constraint `unknown`.
                // T[K] for unconstrained T has upper bound `unknown` because T
                // could be any type and its properties could have any value.
                if !candidates.contains(&TypeId::UNKNOWN) {
                    candidates.push(TypeId::UNKNOWN);
                }
            }
        }

        if let Some(info) = type_param_info(self.interner, key_type)
            && let Some(constraint) = info.constraint
        {
            let constrained =
                self.evaluate_type(self.interner.index_access(object_type, constraint));
            if constrained != original && !candidates.contains(&constrained) {
                candidates.push(constrained);
            }
        }

        if self.index_key_is_constrained_to_keyof_object(key_type, object_type)
            && let Some(value_union) = self.collect_keyof_index_value_union(object_type)
            && value_union != original
            && !candidates.contains(&value_union)
        {
            candidates.push(value_union);
        }

        if let Some(intersection_id) =
            crate::visitor::intersection_list_id(self.interner, object_type)
        {
            let members = self.interner.type_list(intersection_id);
            for &member in members.iter() {
                self.collect_index_access_upper_bound_candidates(
                    member, key_type, original, candidates,
                );
            }
        }
    }

    fn index_key_is_constrained_to_keyof_object(
        &mut self,
        key_type: TypeId,
        object_type: TypeId,
    ) -> bool {
        let key_constraint = type_param_info(self.interner, key_type)
            .and_then(|info| info.constraint)
            .unwrap_or(key_type);
        let Some(keyof_object) = keyof_inner_type(self.interner, key_constraint) else {
            return false;
        };

        self.same_after_evaluation(keyof_object, object_type)
    }

    fn same_after_evaluation(&mut self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        let evaluated_left = self.evaluate_type(left);
        let evaluated_right = self.evaluate_type(right);
        evaluated_left == right || left == evaluated_right || evaluated_left == evaluated_right
    }

    fn collect_keyof_index_value_union(&mut self, object_type: TypeId) -> Option<TypeId> {
        let collected = match crate::objects::collect_properties_cached(
            object_type,
            self.interner,
            self.resolver,
            self.query_db,
        ) {
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
                symbol_index: _,
            } => (properties, string_index, number_index),
            PropertyCollectionResult::Any => return Some(TypeId::ANY),
            PropertyCollectionResult::NonObject => {
                let evaluated = self.evaluate_type(object_type);
                if evaluated == object_type {
                    return None;
                }
                match crate::objects::collect_properties_cached(
                    evaluated,
                    self.interner,
                    self.resolver,
                    self.query_db,
                ) {
                    PropertyCollectionResult::Properties {
                        properties,
                        string_index,
                        number_index,
                        symbol_index: _,
                    } => (properties, string_index, number_index),
                    PropertyCollectionResult::Any => return Some(TypeId::ANY),
                    PropertyCollectionResult::NonObject => return None,
                }
            }
        };

        let (properties, string_index, number_index) = collected;
        let mut values: Vec<TypeId> = properties
            .into_iter()
            .map(|property| property.type_id)
            .collect();
        if let Some(index) = string_index {
            values.push(index.value_type);
        }
        if let Some(index) = number_index {
            values.push(index.value_type);
        }
        if values.is_empty() {
            None
        } else {
            Some(crate::utils::union_or_single(self.interner, values))
        }
    }
}
