//! Cross-call `instantiate_type` cache wiring tests.
//!
//! These tests exercise the wiring of `InstantiationCache` into the
//! cache-aware instantiation entry points (the `_cached` variants). They verify
//! that:
//!
//! 1. Two back-to-back calls with the same `(type_id, subst, mode_bits, this_type)`
//!    produce a cache hit (recorded via `instantiation_cache_hits`).
//! 2. Different `this_type` values for `substitute_this_type_cached` do NOT
//!    alias even though the substitution is empty.
//! 3. The leaf fast paths (`TypeParameter` direct hit, `IndexAccess`) are NOT
//!    cached — they remain allocation-free.
//! 4. The empty / concrete-identity short-circuit runs BEFORE cache-key
//!    construction, leaving the cache untouched on no-op substitutions.
//! 5. A `depth_exceeded` walk's result is never cached in the PER-FILE
//!    `InstantiationCache` (`query_db=Some`'s own gate reads the raw
//!    termination flag). The PROJECT-WIDE proto cache is more precise: a
//!    depth-exceeded verdict from the per-walk local depth cap (which always
//!    starts fresh at 0, so it is a pure function of the request) IS cached
//!    there; only a bail through the ambient cross-operation solver-frame
//!    budget is excluded (see `InstantiationResult::is_ambient_limited`).
//! 6. Semantically equal substitutions hit the same cache slot even if their
//!    `FxHashMap` insertion order differs.
//!
//! Tests use `TypeInterner` + `QueryCache` and route the cache parameter
//! explicitly through the `_cached` overloads, mirroring how the hot evaluator
//! / subtype-checker paths thread `self.query_db`.

use crate::caches::query_cache::QueryCache;
use crate::def::{DefId, DefinitionStore};
use crate::instantiation::instantiate::flags::InstResolverRereduceFlagGuard;
use crate::instantiation::instantiate::{
    MAX_INSTANTIATION_DEPTH, ProjectInstCacheDisabledGuard, TypeSubstitution,
    instantiate_generic_cached, instantiate_type, instantiate_type_cached,
    instantiate_type_preserving_cached, substitute_this_type_at_return_position,
    substitute_this_type_cached,
};
use crate::intern::TypeInterner;
use crate::types::{
    ConditionalType, PropertyInfo, TypeId, TypeParamInfo, TypeParamOrigin, Visibility,
};

fn param_info(atom: tsz_common::interner::Atom) -> TypeParamInfo {
    TypeParamInfo {
        name: atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }
}

fn type_param(interner: &TypeInterner, name: &str) -> (tsz_common::interner::Atom, TypeId) {
    let atom = interner.intern_string(name);
    let id = interner.type_param(param_info(atom));
    (atom, id)
}

/// Build an object type `{ a: T }` over a given type-id.
fn object_with(interner: &TypeInterner, t_id: TypeId) -> TypeId {
    let a = interner.intern_string("a");
    let prop = PropertyInfo {
        name: a,
        type_id: t_id,
        write_type: t_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: true,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    };
    interner.object(vec![prop])
}

/// Build an object type `{ a: T; b: U }` over two given type-ids.
fn object_with_pair(interner: &TypeInterner, t_id: TypeId, u_id: TypeId) -> TypeId {
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let prop_a = PropertyInfo {
        name: a,
        type_id: t_id,
        write_type: t_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: true,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    };
    let prop_b = PropertyInfo {
        name: b,
        type_id: u_id,
        write_type: u_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 1,
        is_string_named: true,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    };
    interner.object(vec![prop_a, prop_b])
}

#[test]
fn cache_hit_after_first_instantiate_type() {
    // Two back-to-back instantiate_type_cached calls with the same key must
    // produce exactly one miss followed by one hit.
    // Per-file tier in isolation (#14345): disable the project-wide cache so the
    // second call's hit is observed on the per-file QueryCache statistics.
    let _g = ProjectInstCacheDisabledGuard::new();
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let stats0 = db.statistics();

    let r1 = instantiate_type_cached(&interner, Some(&db), body, &subst);
    let r2 = instantiate_type_cached(&interner, Some(&db), body, &subst);

    assert_eq!(r1, r2, "cached result must equal recomputed result");

    let stats1 = db.statistics();
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first call should record at least one miss"
    );
    assert!(
        stats1.instantiation_cache_hits > stats0.instantiation_cache_hits,
        "second call should record a hit (got {} hits)",
        stats1.instantiation_cache_hits
    );
    assert!(
        stats1.instantiation_cache_entries >= 1,
        "cache must contain at least one entry after first call"
    );
}

#[test]
fn cache_distinct_substitutions_do_not_alias() {
    // {"T": string} and {"T": number} on the same body produce different
    // results and different cache entries.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);

    let mut subst_string = TypeSubstitution::new();
    subst_string.insert(t_atom, TypeId::STRING);

    let mut subst_number = TypeSubstitution::new();
    subst_number.insert(t_atom, TypeId::NUMBER);

    let r_string = instantiate_type_cached(&interner, Some(&db), body, &subst_string);
    let r_number = instantiate_type_cached(&interner, Some(&db), body, &subst_number);

    assert_ne!(
        r_string, r_number,
        "different substitutions must produce different results"
    );

    let entries = db.statistics().instantiation_cache_entries;
    assert!(
        entries >= 2,
        "expected >= 2 distinct cache entries, got {entries}"
    );
}

#[test]
fn cache_canonicalizes_substitution_insertion_order() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // The cache key is the canonical substitution payload, not the source
    // FxHashMap's insertion order. Rebuilding the same semantic substitution in
    // reverse order must hit the first cache entry instead of creating a second.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let (u_atom, u_id) = type_param(&interner, "U");
    let body = object_with_pair(&interner, t_id, u_id);

    let mut subst_tu = TypeSubstitution::new();
    subst_tu.insert(t_atom, TypeId::STRING);
    subst_tu.insert(u_atom, TypeId::NUMBER);

    let mut subst_ut = TypeSubstitution::new();
    subst_ut.insert(u_atom, TypeId::NUMBER);
    subst_ut.insert(t_atom, TypeId::STRING);

    let r1 = instantiate_type_cached(&interner, Some(&db), body, &subst_tu);
    let stats_after_first = db.statistics();
    assert_eq!(
        stats_after_first.instantiation_cache_entries, 1,
        "first non-leaf instantiation should create one cache entry"
    );

    let r2 = instantiate_type_cached(&interner, Some(&db), body, &subst_ut);
    let stats_after_second = db.statistics();

    assert_eq!(r1, r2, "equal substitutions must produce equal results");
    assert_eq!(
        stats_after_second.instantiation_cache_entries,
        stats_after_first.instantiation_cache_entries,
        "reversed insertion order must reuse the canonical cache slot"
    );
    assert!(
        stats_after_second.instantiation_cache_hits > stats_after_first.instantiation_cache_hits,
        "reversed insertion order should register a cache hit"
    );
}

#[test]
fn exact_domain_cache_hits_equivalent_content_and_separates_distinct_owners() {
    let _g = ProjectInstCacheDisabledGuard::new();
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("T");
    let file = interner.intern_string("cache-domain.ts");
    let owned = TypeParamInfo {
        origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        ..TypeParamInfo::simple(name)
    };
    let foreign = TypeParamInfo {
        origin: TypeParamOrigin::DeclScoped { file, node: 2 },
        ..owned
    };
    let body = object_with(&interner, interner.fresh_type_param(owned));

    let first = TypeSubstitution::from_signature_args(&interner, &[owned], &[TypeId::STRING]);
    let equivalent = TypeSubstitution::from_signature_args(&interner, &[owned], &[TypeId::STRING]);
    let distinct = TypeSubstitution::from_signature_args(&interner, &[foreign], &[TypeId::STRING]);

    let owned_result = instantiate_type_cached(&interner, Some(&db), body, &first);
    let stats_after_first = db.statistics();
    let equivalent_result = instantiate_type_cached(&interner, Some(&db), body, &equivalent);
    let stats_after_equivalent = db.statistics();

    assert_eq!(owned_result, equivalent_result);
    assert!(
        stats_after_equivalent.instantiation_cache_hits
            > stats_after_first.instantiation_cache_hits,
        "an independently rebuilt equivalent domain must hit"
    );

    let distinct_result = instantiate_type_cached(&interner, Some(&db), body, &distinct);
    let stats_after_distinct = db.statistics();
    assert_eq!(distinct_result, body, "a foreign domain must not rewrite T");
    assert_ne!(owned_result, distinct_result);
    assert!(
        stats_after_distinct.instantiation_cache_entries
            > stats_after_equivalent.instantiation_cache_entries,
        "the same name map under a different owner needs a separate entry"
    );
}

#[test]
fn substitute_this_type_caches_per_this() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // substitute_this_type_cached with the same (type_id, this_type) hits
    // the cache; different this_type values miss.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Build a body that contains ThisType so substitution actually walks.
    let this_t = interner.this_type();
    let body = object_with(&interner, this_t);

    let class_a = interner.literal_string("ClassA"); // distinct TypeId, opaque
    let class_b = interner.literal_string("ClassB");

    let stats0 = db.statistics();

    let _ = substitute_this_type_cached(&interner, Some(&db), body, class_a);
    let _ = substitute_this_type_cached(&interner, Some(&db), body, class_a); // hit
    let _ = substitute_this_type_cached(&interner, Some(&db), body, class_b); // miss

    let stats1 = db.statistics();
    let prior = stats0.instantiation_cache_hits;
    let after = stats1.instantiation_cache_hits;
    assert!(
        after > prior,
        "second call with same this_type must hit the cache (hits: {prior} -> {after})"
    );
    let entries = stats1.instantiation_cache_entries;
    assert!(
        entries >= 2,
        "different this_type values must occupy distinct cache slots ({entries} entries)"
    );
}

#[test]
fn shallow_this_return_position_caches_with_distinct_mode() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // The shallow return-position variant uses a different walk shape than
    // deep substitute_this_type_cached, so it must cache under a distinct
    // mode bit while still hitting for repeated shallow calls.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let this_t = interner.this_type();
    let body = object_with(&interner, this_t);
    let receiver = interner.literal_string("Receiver");

    let _ = substitute_this_type_cached(&interner, Some(&db), body, receiver);
    let entries_after_deep = db.statistics().instantiation_cache_entries;
    let hits_after_deep = db.statistics().instantiation_cache_hits;

    let shallow_1 = substitute_this_type_at_return_position(&interner, Some(&db), body, receiver);
    let entries_after_shallow = db.statistics().instantiation_cache_entries;
    assert!(
        entries_after_shallow > entries_after_deep,
        "shallow-this substitution must not alias the deep-this cache slot"
    );

    let shallow_2 = substitute_this_type_at_return_position(&interner, Some(&db), body, receiver);
    assert_eq!(shallow_1, shallow_2);
    assert!(
        db.statistics().instantiation_cache_hits > hits_after_deep,
        "second shallow-this substitution should hit the cache"
    );
}

#[test]
fn leaf_fast_path_typeparameter_is_not_cached() {
    // The TypeParameter direct-hit fast path runs BEFORE any cache-key
    // construction. After many leaf calls, the cache should remain empty.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let stats0 = db.statistics();

    // Each call hits the TypeParameter fast path and returns immediately.
    for _ in 0..32 {
        let r = instantiate_type_cached(&interner, Some(&db), t_id, &subst);
        assert_eq!(r, TypeId::STRING);
    }

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "leaf TypeParameter fast path must NOT populate the cache"
    );
    assert_eq!(
        stats1.instantiation_cache_misses, stats0.instantiation_cache_misses,
        "leaf TypeParameter fast path must NOT probe the cache (no miss either)"
    );
}

#[test]
fn empty_substitution_short_circuits_before_cache() {
    // Empty substitution returns the input directly without touching the
    // cache. (Design: the empty/identity short-circuit runs before cache
    // construction.) Note: substitute_this_type still caches because it
    // carries this_type — this test exercises instantiate_type only.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (_, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let empty = TypeSubstitution::new();

    let stats0 = db.statistics();

    for _ in 0..16 {
        let r = instantiate_type_cached(&interner, Some(&db), body, &empty);
        assert_eq!(r, body, "empty substitution must be identity");
    }

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "empty substitution must NOT populate the cache"
    );
    assert_eq!(
        stats1.instantiation_cache_misses, stats0.instantiation_cache_misses,
        "empty substitution must NOT probe the cache"
    );
}

#[test]
fn concrete_body_short_circuits_before_cache_with_non_empty_substitution() {
    // A non-empty substitution cannot affect a body that contains no
    // TypeParameter/Infer roots. The instantiator should return identity before
    // probing the cross-call instantiation cache.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, _) = type_param(&interner, "T");
    let concrete_body = object_with(&interner, TypeId::STRING);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::NUMBER);

    let stats0 = db.statistics();

    for _ in 0..16 {
        let direct = instantiate_type_cached(&interner, Some(&db), concrete_body, &subst);
        assert_eq!(direct, concrete_body);
    }

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "concrete identity must NOT populate the instantiation cache"
    );
    assert_eq!(
        stats1.instantiation_cache_misses, stats0.instantiation_cache_misses,
        "concrete identity must NOT probe the instantiation cache"
    );
}

#[test]
fn concrete_body_with_meta_type_still_walks_and_caches() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // `instantiate_type_cached` can only skip concrete bodies that are true
    // identity walks. Concrete meta-types still need the instantiator's
    // normalization pass; otherwise an unrelated substitution would leave
    // nested `{ a: string }["a"]` raw.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, _) = type_param(&interner, "T");
    let source = object_with(&interner, TypeId::STRING);
    let indexed = interner.index_access(source, interner.literal_string("a"));
    let concrete_body = object_with(&interner, indexed);
    let expected = object_with(&interner, TypeId::STRING);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::NUMBER);

    let stats0 = db.statistics();

    let first = instantiate_type_cached(&interner, Some(&db), concrete_body, &subst);
    let second = instantiate_type_cached(&interner, Some(&db), concrete_body, &subst);

    assert_eq!(first, expected);
    assert_eq!(second, expected);

    let stats1 = db.statistics();
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first concrete meta-type normalization must probe the cache"
    );
    assert!(
        stats1.instantiation_cache_hits > stats0.instantiation_cache_hits,
        "second concrete meta-type normalization must hit the cache"
    );
}

#[test]
fn instantiate_generic_cached_keeps_concrete_meta_type_normalization() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // `instantiate_generic_cached` is also the entry point for alias/application
    // body normalization. Even when a body contains no type parameters, it must
    // still run the staged instantiator so concrete meta-types such as
    // `{ a: string }["a"]` reduce the same way as the non-short-circuited path.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, _) = type_param(&interner, "T");
    let source = object_with(&interner, TypeId::STRING);
    let concrete_body = interner.index_access(source, interner.literal_string("a"));
    let type_params = [param_info(t_atom)];
    let type_args = [TypeId::NUMBER];

    let stats0 = db.statistics();

    let first = instantiate_generic_cached(
        &interner,
        Some(&db),
        concrete_body,
        &type_params,
        &type_args,
    );
    let second = instantiate_generic_cached(
        &interner,
        Some(&db),
        concrete_body,
        &type_params,
        &type_args,
    );

    assert_eq!(first, TypeId::STRING);
    assert_eq!(second, TypeId::STRING);

    let stats1 = db.statistics();
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first generic concrete meta-type normalization must probe the cache"
    );
    assert!(
        stats1.instantiation_cache_hits > stats0.instantiation_cache_hits,
        "second generic concrete meta-type normalization must hit the cache"
    );
}

#[test]
fn no_query_db_disables_cache() {
    // Calling instantiate_type_cached with query_db=None still computes the
    // correct result but never touches the cache. Used to verify that the
    // backwards-compat path (no QueryDatabase) is preserved.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let stats0 = db.statistics();

    // Call with query_db=None — cache must NOT see this.
    let r1 = instantiate_type_cached(&interner, None, body, &subst);
    let r2 = instantiate_type_cached(&interner, None, body, &subst);
    assert_eq!(r1, r2);

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "calls with query_db=None must NOT populate the cache"
    );
    assert_eq!(
        stats1.instantiation_cache_hits, stats0.instantiation_cache_hits,
        "calls with query_db=None must NOT register hits"
    );
}

#[test]
fn mode_bits_isolate_preserving_from_default() {
    // instantiate_type_cached and instantiate_type_preserving_cached must
    // not collide in the cache because their mode_bits differ.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let _ = instantiate_type_cached(&interner, Some(&db), body, &subst);
    let entries_after_default = db.statistics().instantiation_cache_entries;

    let _ = instantiate_type_preserving_cached(&interner, Some(&db), body, &subst);
    let entries_after_preserving = db.statistics().instantiation_cache_entries;

    assert!(
        entries_after_preserving > entries_after_default,
        "preserving variant must produce a distinct cache entry ({entries_after_default} -> {entries_after_preserving})"
    );
}

#[test]
fn depth_exceeded_result_is_not_cached_per_file_but_short_circuits_project_wide() {
    // A depth-overflow walk returns a relation-preserving partial type (no
    // longer the `TypeId::ERROR` sentinel; see #13652). The PER-FILE
    // `InstantiationCache` never stores it (`query_db=Some`'s own gate reads
    // the raw `depth_exceeded()` termination flag, unconditionally). But the
    // walk's local depth cap always starts fresh at 0 per instance, so its
    // depth-exceeded verdict is a pure function of `(type_id, subst,
    // mode_bits, this_type)` and IS stored in the project-wide proto cache
    // (#14345) — so the second call short-circuits there, before ever
    // reaching (and re-missing) the per-file cache the first call probed.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");

    // Wrap T many times: Array<Array<...<T>...>>
    let depth = (MAX_INSTANTIATION_DEPTH as usize) + 2;
    let mut body = t_id;
    for _ in 0..depth {
        body = interner.array(body);
    }

    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let stats0 = db.statistics();

    let r1 = instantiate_type_cached(&interner, Some(&db), body, &subst);
    let r2 = instantiate_type_cached(&interner, Some(&db), body, &subst);

    // The bail no longer surfaces the ERROR sentinel, and repeating the
    // request is deterministic.
    assert_ne!(r1, TypeId::ERROR);
    assert_eq!(r1, r2);

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "depth-overflow results must not populate the PER-FILE instantiation cache"
    );
    assert_eq!(
        stats1.instantiation_cache_hits, stats0.instantiation_cache_hits,
        "the second request never reaches the per-file cache to hit it — \
         it short-circuits at the project-wide proto cache first"
    );
    assert_eq!(
        stats1.instantiation_cache_misses,
        stats0.instantiation_cache_misses + 1,
        "only the FIRST depth-overflow request probes and misses the \
         per-file cache; the second is served by the project-wide proto \
         cache before the per-file cache is ever consulted"
    );
}

#[test]
fn instantiate_generic_cached_hits_cache_on_repeat() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // Mirrors `cache_hit_after_first_instantiate_type` but exercises the
    // substitution-building entry that recursive utility expansion uses.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let param = param_info(t_atom);

    let stats0 = db.statistics();

    let r1 = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[TypeId::STRING]);
    let r2 = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[TypeId::STRING]);

    assert_eq!(r1, r2, "cached generic instantiation must equal recomputed");

    let stats1 = db.statistics();
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first call must record at least one miss"
    );
    assert!(
        stats1.instantiation_cache_hits > stats0.instantiation_cache_hits,
        "second call must record a hit (got {} hits)",
        stats1.instantiation_cache_hits
    );
}

#[test]
fn instantiate_generic_cached_shares_slot_with_instantiate_type_cached() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // The two entry points share the canonical-substitution cache slot, so
    // recursive utility expansion benefits from the cache regardless of which
    // entry point the calling site uses.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let param = param_info(t_atom);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let stats0 = db.statistics();

    let r_type = instantiate_type_cached(&interner, Some(&db), body, &subst);
    let r_generic =
        instantiate_generic_cached(&interner, Some(&db), body, &[param], &[TypeId::STRING]);

    assert_eq!(
        r_type, r_generic,
        "both entry points must produce the same result"
    );

    let stats1 = db.statistics();
    assert!(
        stats1.instantiation_cache_hits > stats0.instantiation_cache_hits,
        "instantiate_generic_cached must hit the slot populated by instantiate_type_cached"
    );
}

#[test]
fn instantiate_generic_cached_identity_short_circuits() {
    // `is_identity_for` returns the body untouched without probing the cache.
    // This preserves the pre-existing fast path that the uncached
    // `instantiate_generic` had and keeps the cache cheap for no-op
    // substitutions.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let param = param_info(t_atom);

    let stats0 = db.statistics();

    // Identity substitution: T -> T.
    let r = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[t_id]);
    assert_eq!(r, body);

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "identity substitution must not populate the cache"
    );
    assert_eq!(
        stats1.instantiation_cache_misses, stats0.instantiation_cache_misses,
        "identity substitution must not probe the cache"
    );
}

#[test]
fn unchanged_conditional_instantiation_skips_conditional_reintern() {
    // Conditional types are meta-types, so they still need to be walked under a
    // non-empty substitution. If all four arms remain unchanged, though, the
    // walk should return the original `TypeId` without probing the conditional
    // interner again.
    //
    // Scoped: `conditional_intern_calls` is a process-wide atomic, and this
    // test reads a before/after delta on it. Without `ScopedPerfCounters`, a
    // sibling thread interning an unrelated conditional between the two reads
    // would inflate `after` and mask a real regression here just as easily as
    // it would produce a false failure (see #16017's writeup of this counter
    // class under a shared-process runner).
    let _scope = tsz_common::perf_counters::ScopedPerfCounters::new();

    let interner = TypeInterner::new();
    let (u_atom, _u_id) = type_param(&interner, "U");
    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(u_atom, TypeId::STRING);

    let before = tsz_common::perf_counters::PerfCounters::snapshot()
        .interner
        .conditional_intern_calls;
    let result = instantiate_type(&interner, conditional, &subst);
    let after = tsz_common::perf_counters::PerfCounters::snapshot()
        .interner
        .conditional_intern_calls;

    assert_eq!(
        result, conditional,
        "unchanged conditional instantiation should preserve identity"
    );
    assert_eq!(
        after, before,
        "unchanged conditional instantiation should not re-intern the conditional"
    );
}

#[test]
fn changed_conditional_instantiation_still_rebuilds_conditional() {
    // Scoped for the same reason as `unchanged_conditional_instantiation_
    // skips_conditional_reintern` above: `conditional_intern_calls` is a
    // process-wide atomic and this test reads a before/after delta on it.
    let _scope = tsz_common::perf_counters::ScopedPerfCounters::new();

    let interner = TypeInterner::new();
    let (t_atom, t_id) = type_param(&interner, "T");
    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: t_id,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::NUMBER);

    let before = tsz_common::perf_counters::PerfCounters::snapshot()
        .interner
        .conditional_intern_calls;
    let result = instantiate_type(&interner, conditional, &subst);
    let after = tsz_common::perf_counters::PerfCounters::snapshot()
        .interner
        .conditional_intern_calls;

    assert_ne!(
        result, conditional,
        "changed conditional instantiation must not preserve the old identity"
    );
    assert!(
        after > before,
        "changed conditional instantiation should re-intern the rebuilt conditional"
    );
    let result_id = match interner.lookup(result) {
        Some(crate::types::TypeData::Conditional(id)) => id,
        other => panic!("expected rebuilt conditional, got {other:?}"),
    };
    let rebuilt = interner.get_conditional(result_id);
    assert_eq!(rebuilt.true_type, TypeId::NUMBER);
    assert_eq!(rebuilt.false_type, TypeId::BOOLEAN);
}

#[test]
fn instantiate_generic_cached_no_query_db_disables_cache() {
    // Backwards compat: calling with query_db=None preserves the existing
    // semantics — the result is correct but the cache is never consulted.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let param = param_info(t_atom);

    let stats0 = db.statistics();

    let r1 = instantiate_generic_cached(&interner, None, body, &[param], &[TypeId::STRING]);
    let r2 = instantiate_generic_cached(&interner, None, body, &[param], &[TypeId::STRING]);
    assert_eq!(r1, r2);

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "calls with query_db=None must not populate the cache"
    );
    assert_eq!(
        stats1.instantiation_cache_hits, stats0.instantiation_cache_hits,
        "calls with query_db=None must not register hits"
    );
}

/// Build the project-wide instantiation `InstantiationCacheKey` for a
/// `(body, param -> arg)` single-substitution request, matching what
/// `instantiate_generic_cached` consults internally.
fn proto_key_for(
    interner: &TypeInterner,
    body: TypeId,
    param: &TypeParamInfo,
    arg: TypeId,
) -> crate::caches::instantiation_cache::InstantiationCacheKey {
    let subst = TypeSubstitution::from_args(interner, std::slice::from_ref(param), &[arg]);
    crate::instantiation::request::InstantiationRequest::new(body, &subst).cache_key()
}

/// #14345 limit gate (positive control): a non-limited instantiation IS stored
/// project-wide, so the negative tests below are not vacuously passing. Pins the
/// positive path: a clean `(body, subst)` populates the project-wide cache for
/// the `query_db=None` callers to reuse.
#[test]
fn clean_instantiation_is_cached_project_wide() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let param = param_info(t_atom);
    let body = object_with(&interner, t_id);
    let key = proto_key_for(&interner, body, &param, TypeId::BOOLEAN);

    assert!(interner.proto_instantiation_memo(&key).is_none());
    let r = instantiate_generic_cached(
        &interner,
        Some(&db),
        body,
        std::slice::from_ref(&param),
        &[TypeId::BOOLEAN],
    );
    assert_eq!(
        interner.proto_instantiation_memo(&key),
        Some(r),
        "a clean instantiation must be stored project-wide for query_db=None reuse"
    );
}

/// #14345 limit gate (refined): a depth-exceeded walk from the per-instance
/// LOCAL depth cap DOES enter the project-wide cache. That cap always starts
/// fresh at 0 for every `TypeInstantiator` (`run_instantiator` builds a new
/// one per call), so its verdict is a pure, reproducible function of
/// `(type_id, subst, mode_bits, this_type)` alone — caching it is sound, and
/// necessary: without it, a self-referential/deeply-nested shape (e.g. DOM
/// lib interfaces, #16089) that legitimately overflows the walk on every
/// request recomputes the same bounded-but-expensive walk from scratch every
/// single time, since a truncated result was never memoized to short-circuit
/// the repeat. Only a bail through the AMBIENT cross-operation solver-frame
/// budget stays excluded (`InstantiationResult::is_ambient_limited`), because
/// that budget is shared state that can make the identical request bail or
/// succeed depending on unrelated concurrent recursion — see the unit tests
/// on `InstantiationMemoStability` in `instantiation::result` for that half.
#[test]
fn locally_depth_exceeded_instantiation_is_cached_project_wide() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let param = param_info(t_atom);
    let depth = (MAX_INSTANTIATION_DEPTH as usize) + 2;
    let mut body = t_id;
    for _ in 0..depth {
        body = interner.array(body);
    }
    let key = proto_key_for(&interner, body, &param, TypeId::STRING);

    let r = instantiate_generic_cached(
        &interner,
        Some(&db),
        body,
        std::slice::from_ref(&param),
        &[TypeId::STRING],
    );
    assert_eq!(
        interner.proto_instantiation_memo(&key),
        Some(r),
        "a purely-local depth-exceeded instantiation is a pure function of \
         the request and must be stored project-wide so a repeat short-\
         circuits instead of re-walking the same bounded-but-expensive shape"
    );
}

/// #14345 limit gate (before/after correctness): a sticky limit flag left set by
/// an EARLIER sibling instantiation must NOT block caching an unrelated clean
/// result. The store-gate snapshots `tuple_too_large` / `union_too_complex`
/// before the walk and refuses only a NEWLY-tripped result — mirroring
/// `closed_eval_cache`'s `union_too_complex_before` snapshot. Without this, one
/// pathological sibling would poison the cache for every later instantiation.
#[test]
fn pre_existing_limit_flag_does_not_block_clean_instantiation() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let param = param_info(t_atom);
    let body = object_with(&interner, t_id);
    let key = proto_key_for(&interner, body, &param, TypeId::NUMBER);

    // A sibling already tripped both sticky flags; this clean walk does not.
    interner.set_tuple_too_large();
    interner.set_union_too_complex();

    let r = instantiate_generic_cached(
        &interner,
        Some(&db),
        body,
        std::slice::from_ref(&param),
        &[TypeId::NUMBER],
    );
    assert_eq!(
        interner.proto_instantiation_memo(&key),
        Some(r),
        "a pre-existing sibling limit flag must not block a clean instantiation"
    );
}

#[test]
fn instantiate_generic_cached_depth_overflow_short_circuits_project_wide_on_repeat() {
    // A depth-overflow walk returns a relation-preserving partial type (no
    // longer the `TypeId::ERROR` sentinel; see #13652). The PER-FILE
    // `InstantiationCache` never stores it, matching
    // `depth_exceeded_result_is_not_cached_per_file_but_short_circuits_project_wide`
    // above. But the local depth cap's verdict is a pure function of the
    // request (see `locally_depth_exceeded_instantiation_is_cached_project_wide`),
    // so the second identical request is served by the project-wide proto
    // cache and never reaches (or re-misses) the per-file cache at all.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let param = param_info(t_atom);

    let depth = (MAX_INSTANTIATION_DEPTH as usize) + 2;
    let mut body = t_id;
    for _ in 0..depth {
        body = interner.array(body);
    }

    let stats0 = db.statistics();

    let r1 = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[TypeId::STRING]);
    let r2 = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[TypeId::STRING]);

    assert_ne!(r1, TypeId::ERROR);
    assert_eq!(r1, r2);

    let stats1 = db.statistics();
    assert_eq!(
        stats1.instantiation_cache_entries, stats0.instantiation_cache_entries,
        "depth-overflow results must not populate the PER-FILE cache"
    );
    assert_eq!(
        stats1.instantiation_cache_hits, stats0.instantiation_cache_hits,
        "the second request never reaches the per-file cache to hit it — \
         it short-circuits at the project-wide proto cache first"
    );
    assert_eq!(
        stats1.instantiation_cache_misses,
        stats0.instantiation_cache_misses + 1,
        "only the FIRST depth-overflow request probes and misses the \
         per-file cache; the second is served by the project-wide proto \
         cache before the per-file cache is ever consulted"
    );
}

#[test]
fn instantiate_generic_cached_is_invariant_to_type_param_renaming() {
    let _g = ProjectInstCacheDisabledGuard::new();
    // The cache key uses canonical (Atom, TypeId) pairs, so two callers whose
    // `TypeParamInfo` differ only in non-name metadata (constraint, default)
    // must hit the same cache slot.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Body uses a TypeParameter atom shared with the param we'll instantiate
    // with. (Same atom => same substitution payload.)
    let (shared_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);

    // Two callers, two different `TypeParamInfo` values but the same name atom.
    // Different constraint metadata must not perturb the cache key.
    let param_a = param_info(shared_atom);
    let param_b = TypeParamInfo {
        name: shared_atom,
        constraint: Some(TypeId::UNKNOWN),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };

    let stats0 = db.statistics();
    let r_a = instantiate_generic_cached(&interner, Some(&db), body, &[param_a], &[TypeId::STRING]);
    let r_b = instantiate_generic_cached(&interner, Some(&db), body, &[param_b], &[TypeId::STRING]);

    assert_eq!(
        r_a, r_b,
        "same substitution payload must produce the same result"
    );

    let stats1 = db.statistics();
    assert!(
        stats1.instantiation_cache_hits > stats0.instantiation_cache_hits,
        "second caller must reuse the cache slot populated by the first"
    );
}

#[test]
fn instantiate_generic_cached_reuses_alpha_equivalent_independent_args() {
    // Issue #13394: utility pipelines often instantiate the same alias body
    // with structurally identical object arguments whose leaf type parameters
    // only differ by local binder name. When the instantiated result erases
    // those binders (here, `keyof T`), the cache should reuse the first walk.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = interner.keyof(t_id);
    let param = param_info(t_atom);

    let (_, source_a) = type_param(&interner, "SourceA");
    let (_, source_b) = type_param(&interner, "SourceB");
    let arg_a = object_with(&interner, source_a);
    let arg_b = object_with(&interner, source_b);

    let stats0 = db.statistics();
    let r_a = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[arg_a]);
    let stats1 = db.statistics();
    let r_b = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[arg_b]);
    let stats2 = db.statistics();

    let expected_b = instantiate_generic_cached(&interner, None, body, &[param], &[arg_b]);
    assert_ne!(
        r_a, expected_b,
        "the witness should require restoring the current binder, not returning the first result"
    );
    assert_eq!(
        r_b, expected_b,
        "alpha-equivalent cache hits must restore the current binder identity"
    );
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first alpha-equivalent request must still miss and populate"
    );
    assert!(
        stats2.instantiation_cache_hits > stats1.instantiation_cache_hits,
        "second alpha-equivalent request should hit instead of re-walking"
    );
}

#[test]
fn instantiate_type_cached_does_not_alpha_reuse_independent_args() {
    // Request scope matters: the alpha-equivalent cache is only for generic
    // alias/application instantiation. Ordinary instantiate_type_cached calls
    // keep exact substitution keys so broad substitution walks cannot perturb
    // diagnostic shape on already-wrong programs.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = interner.keyof(t_id);

    let (_, source_a) = type_param(&interner, "SourceA");
    let (_, source_b) = type_param(&interner, "SourceB");
    let arg_a = object_with(&interner, source_a);
    let arg_b = object_with(&interner, source_b);

    let mut subst_a = TypeSubstitution::new();
    subst_a.insert(t_atom, arg_a);
    let mut subst_b = TypeSubstitution::new();
    subst_b.insert(t_atom, arg_b);

    let stats0 = db.statistics();
    let _ = instantiate_type_cached(&interner, Some(&db), body, &subst_a);
    let stats1 = db.statistics();
    let cached_b = instantiate_type_cached(&interner, Some(&db), body, &subst_b);
    let stats2 = db.statistics();
    let expected_b = instantiate_type_cached(&interner, None, body, &subst_b);

    assert_eq!(
        cached_b, expected_b,
        "exact-key instantiate_type_cached should still compute the current result"
    );
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first request should miss"
    );
    assert_eq!(
        stats2.instantiation_cache_hits, stats1.instantiation_cache_hits,
        "instantiate_type_cached must not use alpha-equivalent cache slots"
    );
}

#[test]
fn instantiate_generic_cached_keeps_constrained_type_param_args_distinct() {
    // Alpha-cache reuse is only sound for simple local binders. Constrained
    // type parameters carry semantic information and must keep their ordinary
    // exact cache keys.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = interner.keyof(t_id);
    let param = param_info(t_atom);

    let source_a_atom = interner.intern_string("SourceA");
    let source_b_atom = interner.intern_string("SourceB");
    let source_a = interner.type_param(TypeParamInfo {
        constraint: Some(TypeId::STRING),
        ..param_info(source_a_atom)
    });
    let source_b = interner.type_param(TypeParamInfo {
        constraint: Some(TypeId::NUMBER),
        ..param_info(source_b_atom)
    });
    let arg_a = object_with(&interner, source_a);
    let arg_b = object_with(&interner, source_b);

    let stats0 = db.statistics();
    let _ = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[arg_a]);
    let stats1 = db.statistics();
    let cached_b = instantiate_generic_cached(&interner, Some(&db), body, &[param], &[arg_b]);
    let stats2 = db.statistics();
    let expected_b = instantiate_generic_cached(&interner, None, body, &[param], &[arg_b]);

    assert_eq!(
        cached_b, expected_b,
        "constrained arg instantiation must still compute the current result"
    );
    assert!(
        stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "first constrained request should miss"
    );
    assert_eq!(
        stats2.instantiation_cache_hits, stats1.instantiation_cache_hits,
        "constrained type parameters must not use the simple alpha cache"
    );
}

#[test]
fn evaluator_recursive_utility_application_populates_cache() {
    // End-to-end wiring evidence for issue #10851: route an alias body that
    // contains the type parameter twice through the actual `TypeEvaluator`
    // with a `QueryCache` attached. The known-params instantiation path goes
    // through `instantiate_generic_cached`, which must populate the cross-
    // call `InstantiationCache`. Guards the wiring at the callsite, not just
    // the helper in isolation.
    use crate::def::{DefId, DefKind};
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();
    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with_pair(&interner, t_id, t_id);

    let mut env = TypeEnvironment::new();
    let def_id = DefId(908_510);
    env.insert_def_with_params(def_id, body, vec![param_info(t_atom)]);
    env.insert_def_kind(def_id, DefKind::TypeAlias);

    let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);
    let qc = QueryCache::new(&interner);

    let stats0 = qc.statistics();
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);
    let _ = evaluator.evaluate(app);
    let stats1 = qc.statistics();

    assert!(
        stats1.instantiation_cache_entries > stats0.instantiation_cache_entries
            || stats1.instantiation_cache_misses > stats0.instantiation_cache_misses,
        "evaluator must reach instantiation cache through instantiate_generic_cached \
         (entries: {} -> {}, misses: {} -> {})",
        stats0.instantiation_cache_entries,
        stats1.instantiation_cache_entries,
        stats0.instantiation_cache_misses,
        stats1.instantiation_cache_misses,
    );
}

#[test]
fn cache_clear_drops_all_instantiation_entries() {
    // QueryCache::clear() must drop the instantiation cache too.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let (t_atom, t_id) = type_param(&interner, "T");
    let body = object_with(&interner, t_id);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, TypeId::STRING);

    let _ = instantiate_type_cached(&interner, Some(&db), body, &subst);
    assert!(db.statistics().instantiation_cache_entries >= 1);

    db.clear();
    assert_eq!(db.statistics().instantiation_cache_entries, 0);
}

#[test]
fn query_database_evaluate_entry_points_preserve_results() {
    // #12021: `QueryCache` overrides `evaluate_conditional` / `evaluate_keyof` /
    // `evaluate_mapped` / `evaluate_index_access_with_options` to thread `self`
    // as the `query_db` so recursive utility expansion through those entry
    // points reaches the cross-call instantiation cache (instead of the
    // trait-default `query_db = None` evaluator built by the free functions).
    // The override must change ONLY caching behavior — never the computed type.
    use crate::caches::db::QueryDatabase;
    use crate::types::{ConditionalType, MappedType};

    let interner = TypeInterner::new();
    let qc = QueryCache::new(&interner);

    // keyof { a: string; b: number }
    let source = object_with_pair(&interner, TypeId::STRING, TypeId::NUMBER);
    let keyof_src = interner.keyof(source);
    assert_eq!(
        QueryDatabase::evaluate_keyof(&qc, source),
        crate::evaluation::evaluate::evaluate_keyof(&interner, source),
        "evaluate_keyof override must match the uncached result",
    );

    // { a: string; b: number }["a"]
    let key_a = interner.literal_string("a");
    assert_eq!(
        QueryDatabase::evaluate_index_access_with_options(&qc, source, key_a, false),
        crate::evaluation::evaluate::evaluate_index_access_with_options(
            &interner, source, key_a, false,
        ),
        "evaluate_index_access override must match the uncached result",
    );

    // string extends string ? number : boolean
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };
    assert_eq!(
        QueryDatabase::evaluate_conditional(&qc, &cond),
        crate::evaluation::evaluate::evaluate_conditional(&interner, &cond),
        "evaluate_conditional override must match the uncached result",
    );

    // Homomorphic `{ [P in keyof T]: T[P] }` instantiated with T = source.
    let iter_atom = interner.intern_string("P");
    let outer_t = interner.type_param(param_info(interner.intern_string("T")));
    let iter_param = interner.type_param(param_info(iter_atom));
    let template = interner.index_access(source, iter_param);
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: iter_atom,
            constraint: Some(interner.keyof(outer_t)),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: keyof_src,
        name_type: None,
        template,
        readonly_modifier: None,
        optional_modifier: None,
    };
    assert_eq!(
        QueryDatabase::evaluate_mapped(&qc, &mapped),
        crate::evaluation::evaluate::evaluate_mapped(&interner, &mapped),
        "evaluate_mapped override must match the uncached result",
    );
}

#[test]
fn query_database_store_backed_rereduce_resolves_published_lazy_body() {
    use crate::caches::db::QueryDatabase;

    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let def_id = DefId(43_451);
    let body = object_with(&interner, TypeId::STRING);
    let lazy = interner.lazy(def_id);
    let key_a = interner.literal_string("a");
    store.set_body(def_id, body);

    let qc = QueryCache::new(&interner).with_definition_store(&store);
    let deferred_index = interner.index_access(lazy, key_a);
    let deferred_keyof = interner.keyof(lazy);

    {
        let _flag = InstResolverRereduceFlagGuard::new(false);
        assert_eq!(
            QueryDatabase::evaluate_index_access(&qc, lazy, key_a),
            deferred_index,
            "flag-off QueryCache evaluation must keep the historical resolver-less deferred index",
        );
        assert_eq!(
            QueryDatabase::evaluate_keyof(&qc, lazy),
            deferred_keyof,
            "flag-off QueryCache evaluation must keep the historical resolver-less deferred keyof",
        );
    }

    let _flag = InstResolverRereduceFlagGuard::new(true);
    assert_eq!(
        QueryDatabase::evaluate_index_access(&qc, lazy, key_a),
        TypeId::STRING,
        "flag-on store-backed QueryCache evaluation must resolve the published Lazy body",
    );
    assert_eq!(
        QueryDatabase::evaluate_keyof(&qc, lazy),
        key_a,
        "flag-on store-backed QueryCache evaluation must compute keys from the published body",
    );
}
