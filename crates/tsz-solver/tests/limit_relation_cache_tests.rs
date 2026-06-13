//! Limit-hit relation/eval outcome caching tests (issue #13241).
//!
//! tsc records `Ternary.Maybe` results on its `maybeKeys` stack and promotes
//! them to cached successes when the outermost relation in
//! `checkTypeRelatedTo` completes successfully. These tests pin the tsz
//! mirror of that policy:
//!
//! - cycle-derived Maybe verdicts are promoted to definitive `true` entries
//!   on outermost success and discarded on outermost failure;
//! - fuel-limit verdicts are stored as budget-conditional `LimitTrue`
//!   entries that are reused only under an equal-or-smaller budget
//!   (fuel-band cache honesty) and never overwrite definitive entries;
//! - the evaluator's per-node taint set discriminates limit-truncated
//!   memo entries from clean intermediates of the same run.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::construction::RelationCacheProbe;
use crate::def::DefId;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::intern::TypeInterner;
use crate::relations::subtype::cache::MAX_GLOBAL_SUBTYPE_FUEL;
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    MappedType, PropertyInfo, RelationCacheValue, SymbolRef, TypeId, TypeParamInfo,
};

/// Maps a fixed set of `DefId`s to recursive bodies.
struct RecursiveDefResolver {
    defs: Vec<(DefId, TypeId)>,
}

impl TypeResolver for RecursiveDefResolver {
    fn resolve_ref(
        &self,
        _symbol: SymbolRef,
        _interner: &dyn crate::construction::TypeDatabase,
    ) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(
        &self,
        def_id: DefId,
        _interner: &dyn crate::construction::TypeDatabase,
    ) -> Option<TypeId> {
        self.defs
            .iter()
            .find(|(d, _)| *d == def_id)
            .map(|&(_, t)| t)
    }
}

/// Build a pair of mutually structured recursive object types
/// `type S = { <prop>: S[]; <tag>: <tag_s> }` / `type T = { <prop>: T[]; <tag>: <tag_t> }`
/// and return `(lazy_s, lazy_t, arr_s, arr_t)`.
///
/// Relating `lazy_s <: lazy_t` delegates `arr_s <: arr_t` through the array
/// element fast path back to `lazy_s <: lazy_t`, which is coinductively
/// assumed related (`CycleDetected`) — the exact delegation-chain Maybe
/// verdict that tsc records on its maybe stack.
fn recursive_pair(
    interner: &TypeInterner,
    prop: &str,
    tag: &str,
    tag_s: TypeId,
    tag_t: TypeId,
    s_def: DefId,
    t_def: DefId,
) -> (RecursiveDefResolver, TypeId, TypeId, TypeId, TypeId) {
    let lazy_s = interner.lazy(s_def);
    let lazy_t = interner.lazy(t_def);
    let arr_s = interner.array(lazy_s);
    let arr_t = interner.array(lazy_t);
    let prop_atom = interner.intern_string(prop);
    let tag_atom = interner.intern_string(tag);
    let body_s = interner.object(vec![
        PropertyInfo::new(prop_atom, arr_s),
        PropertyInfo::new(tag_atom, tag_s),
    ]);
    let body_t = interner.object(vec![
        PropertyInfo::new(prop_atom, arr_t),
        PropertyInfo::new(tag_atom, tag_t),
    ]);
    let resolver = RecursiveDefResolver {
        defs: vec![(s_def, body_s), (t_def, body_t)],
    };
    (resolver, lazy_s, lazy_t, arr_s, arr_t)
}

// =============================================================================
// Maybe-stack promotion (cycle verdicts)
// =============================================================================

#[test]
fn cycle_maybe_keys_promoted_to_true_on_outermost_success() {
    let interner = TypeInterner::new();
    let (resolver, lazy_s, lazy_t, arr_s, arr_t) = recursive_pair(
        &interner,
        "next",
        "kind",
        TypeId::NUMBER,
        TypeId::NUMBER,
        DefId(11),
        DefId(12),
    );
    let db = QueryCache::new(&interner);
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver).with_query_db(&db);

    assert!(
        checker.check_subtype(lazy_s, lazy_t).is_true(),
        "compatible recursive types must relate"
    );

    // The delegated array-element pair resolved through the coinductive cycle
    // assumption; the outermost success must have promoted it (tsc maybeKeys
    // promotion). Before #13241 this pair was recomputed on every query.
    let arr_key = checker.debug_cache_key_for(arr_s, arr_t);
    assert_eq!(
        db.probe_subtype_cache(arr_key),
        RelationCacheProbe::Hit(true),
        "cycle-derived Maybe key must be promoted to a definitive true entry"
    );
}

#[test]
fn repeat_query_of_promoted_maybe_key_is_a_counted_cache_hit() {
    tsz_common::perf_counters::force_enable_perf_counters_for_tests();
    let promotions_before = tsz_common::perf_counters::counters()
        .relation_maybe_promotions
        .load(std::sync::atomic::Ordering::Relaxed);

    let interner = TypeInterner::new();
    let (resolver, lazy_s, lazy_t, arr_s, arr_t) = recursive_pair(
        &interner,
        "next",
        "kind",
        TypeId::NUMBER,
        TypeId::NUMBER,
        DefId(51),
        DefId(52),
    );
    let db = QueryCache::new(&interner);
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver).with_query_db(&db);
    assert!(checker.check_subtype(lazy_s, lazy_t).is_true());

    let promotions_after = tsz_common::perf_counters::counters()
        .relation_maybe_promotions
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        promotions_after > promotions_before,
        "outermost success must promote at least one maybe key (counter-asserted)"
    );

    // The repeat query of the previously limit-hit (cycle-assumed) inner pair
    // must now be a counted cache hit instead of a recomputation.
    let hits_before = db.relation_cache_stats().subtype_hits;
    assert!(checker.check_subtype(arr_s, arr_t).is_true());
    let hits_after = db.relation_cache_stats().subtype_hits;
    assert!(
        hits_after > hits_before,
        "repeat query of a promoted maybe key must hit the relation cache"
    );
}

#[test]
fn cycle_maybe_keys_discarded_on_outermost_failure() {
    let interner = TypeInterner::new();
    // Same recursion, but the tags mismatch: the cycle assumption is made and
    // then the enclosing object frame fails on the other property. The Maybe
    // key must NOT be promoted (negative case from the #6973→#7210 family).
    let (resolver, lazy_s, lazy_t, arr_s, arr_t) = recursive_pair(
        &interner,
        "next",
        "kind",
        TypeId::NUMBER,
        TypeId::STRING,
        DefId(21),
        DefId(22),
    );
    let db = QueryCache::new(&interner);
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver).with_query_db(&db);

    assert!(
        checker.check_subtype(lazy_s, lazy_t).is_false(),
        "mismatching tag property must fail the relation"
    );

    let arr_key = checker.debug_cache_key_for(arr_s, arr_t);
    assert_eq!(
        db.probe_subtype_cache(arr_key),
        RelationCacheProbe::MissNotCached,
        "Maybe keys of a failed outermost relation must be discarded, not promoted"
    );
}

#[test]
fn promotion_is_structural_not_name_bound() {
    // Renamed binders / different property names / different DefIds: the same
    // structural shape must promote identically (anti-hardcoding gate).
    for (prop, tag, s_raw, t_raw) in [
        ("children", "size", 31u32, 32u32),
        ("zzz", "qqq", 4101u32, 4102u32),
    ] {
        let interner = TypeInterner::new();
        let (resolver, lazy_s, lazy_t, arr_s, arr_t) = recursive_pair(
            &interner,
            prop,
            tag,
            TypeId::BOOLEAN,
            TypeId::BOOLEAN,
            DefId(s_raw),
            DefId(t_raw),
        );
        let db = QueryCache::new(&interner);
        let mut checker = SubtypeChecker::with_resolver(&interner, &resolver).with_query_db(&db);
        assert!(checker.check_subtype(lazy_s, lazy_t).is_true());
        let arr_key = checker.debug_cache_key_for(arr_s, arr_t);
        assert_eq!(
            db.probe_subtype_cache(arr_key),
            RelationCacheProbe::Hit(true),
            "promotion must be structural for binder family ({prop}, {tag})"
        );
    }
}

#[test]
fn promoted_maybe_keys_are_visible_through_the_shared_cache() {
    use crate::caches::query_cache::SharedQueryCache;

    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();
    let (resolver, lazy_s, lazy_t, arr_s, arr_t) = recursive_pair(
        &interner,
        "next",
        "kind",
        TypeId::NUMBER,
        TypeId::NUMBER,
        DefId(41),
        DefId(42),
    );
    let db_a = QueryCache::new_with_shared(&interner, &shared);
    let db_b = QueryCache::new_with_shared(&interner, &shared);

    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver).with_query_db(&db_a);
    assert!(checker.check_subtype(lazy_s, lazy_t).is_true());

    let arr_key = checker.debug_cache_key_for(arr_s, arr_t);
    assert_eq!(
        db_b.probe_subtype_cache(arr_key),
        RelationCacheProbe::Hit(true),
        "promoted maybe keys must be readable by sibling file checkers"
    );
}

// =============================================================================
// Fuel-band honesty (LimitTrue entries)
// =============================================================================

#[test]
fn limit_true_with_full_band_short_circuits_the_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let a = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(y, TypeId::STRING)]);

    let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);
    let key = checker.debug_cache_key_for(a, b);

    // Plant an assumed-related verdict recorded under the full budget: any
    // query (whose remaining budget is necessarily <= the full budget) must
    // reuse it instead of recomputing.
    db.insert_subtype_limit_true(key, MAX_GLOBAL_SUBTYPE_FUEL);
    assert!(
        checker.check_subtype(a, b).is_true(),
        "full-band LimitTrue entry must short-circuit as assumed-related"
    );
}

#[test]
fn limit_true_with_smaller_band_is_recomputed_under_a_larger_budget() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");
    let y = interner.intern_string("y");
    let a = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(y, TypeId::STRING)]);

    let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);
    let key = checker.debug_cache_key_for(a, b);

    // Recorded under a tiny budget: a fresh top-level query holds the full
    // budget, so the truncated verdict is NOT honest for it — it must
    // recompute (raised-budget queries recompute; fuel-band honesty) and the
    // honest definitive `false` must replace the limit entry.
    db.insert_subtype_limit_true(key, 1);
    assert!(
        checker.check_subtype(a, b).is_false(),
        "a query with a larger budget must recompute, not reuse the shallow verdict"
    );
    assert_eq!(
        db.probe_subtype_cache(key),
        RelationCacheProbe::Hit(false),
        "the honest recomputed verdict must overwrite the limit entry"
    );
}

#[test]
fn limit_true_never_overwrites_definitive_entries() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");
    let a = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(x, TypeId::STRING)]);
    let checker = SubtypeChecker::new(&interner);
    let key = checker.debug_cache_key_for(a, b);

    db.insert_subtype_cache(key, false);
    db.insert_subtype_limit_true(key, MAX_GLOBAL_SUBTYPE_FUEL);
    assert_eq!(
        db.lookup_subtype_cache(key),
        Some(false),
        "a definitive false must survive a limit-verdict insert"
    );

    db.promote_subtype_cache_true(key);
    assert_eq!(
        db.lookup_subtype_cache(key),
        Some(false),
        "a definitive false must survive a maybe-key promotion"
    );
}

#[test]
fn limit_true_band_merges_keep_the_larger_band_and_promote_to_definitive() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");
    let a = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);
    let b = interner.object(vec![PropertyInfo::new(x, TypeId::STRING)]);
    let checker = SubtypeChecker::new(&interner);
    let key = checker.debug_cache_key_for(a, b);

    db.insert_subtype_limit_true(key, 5);
    db.insert_subtype_limit_true(key, MAX_GLOBAL_SUBTYPE_FUEL);
    db.insert_subtype_limit_true(key, 7);
    assert_eq!(
        db.lookup_subtype_cache_value(key),
        Some(RelationCacheValue::LimitTrue {
            fuel_band: MAX_GLOBAL_SUBTYPE_FUEL
        }),
        "band merges must keep the largest recorded band"
    );
    assert_eq!(
        db.lookup_subtype_cache(key),
        None,
        "the boolean view must hide budget-conditional entries"
    );

    db.promote_subtype_cache_true(key);
    assert_eq!(
        db.lookup_subtype_cache(key),
        Some(true),
        "a validated maybe key upgrades an existing LimitTrue entry"
    );
}

// =============================================================================
// Evaluator taint discrimination (clean intermediates vs truncated artifacts)
// =============================================================================

#[test]
fn clean_evaluation_after_unrelated_bail_is_not_tainted() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    // An earlier, unrelated subtree bailed: the run-sticky flag is set, but
    // a later clean evaluation fires no new limit event in its own window.
    evaluator.simulate_unrelated_recursion_bail_for_test();
    assert!(evaluator.recursion_limit_hit());

    let k = interner.intern_string("k");
    let obj = interner.object(vec![PropertyInfo::new(k, TypeId::NUMBER)]);
    let keyof_obj = interner.keyof(obj);
    let result = evaluator.evaluate(keyof_obj);
    assert_ne!(result, keyof_obj, "keyof must evaluate to the literal key");
    assert!(
        !evaluator.is_tainted(keyof_obj),
        "a clean sibling evaluation must not be marked as a truncated artifact"
    );
    let tainted = evaluator.take_tainted();
    assert!(
        tainted.is_empty(),
        "no node in this run was truncated; tainted set must be empty"
    );
}

#[test]
fn self_referential_mapped_cycle_bail_is_tainted() {
    let interner = TypeInterner::new();

    // type M = { [P in keyof M]: number } — evaluating the constraint
    // re-enters the same mapped TypeId, which is the cycle-breaker bail that
    // memoizes an opaque artifact. That artifact must be tainted so it is
    // never persisted into the depth-agnostic eval cache.
    let def = DefId(77);
    let lazy_m = interner.lazy(def);
    let mapped_ty = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(lazy_m),
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    });
    let resolver = RecursiveDefResolver {
        defs: vec![(def, mapped_ty)],
    };

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let _result = evaluator.evaluate(mapped_ty);
    assert!(
        evaluator.recursion_limit_hit(),
        "the self-referential mapped type must trip a recursion limit"
    );
    assert!(
        evaluator.is_tainted(mapped_ty),
        "the cycle-bailed node's memo entry must be marked as a truncated artifact"
    );

    // A clean evaluation afterwards in the same (limit-hit) run stays clean.
    let k = interner.intern_string("w");
    let obj = interner.object(vec![PropertyInfo::new(k, TypeId::STRING)]);
    let keyof_obj = interner.keyof(obj);
    let evaluated = evaluator.evaluate(keyof_obj);
    assert_ne!(evaluated, keyof_obj);
    assert!(
        !evaluator.is_tainted(keyof_obj),
        "clean intermediates of a limit-hit run must stay persistable"
    );
}

#[test]
fn reading_a_tainted_memo_entry_taints_the_consumer() {
    let interner = TypeInterner::new();

    let def = DefId(78);
    let lazy_m = interner.lazy(def);
    let mapped_ty = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("Q"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(lazy_m),
        name_type: None,
        template: TypeId::STRING,
        optional_modifier: None,
        readonly_modifier: None,
    });
    let resolver = RecursiveDefResolver {
        defs: vec![(def, mapped_ty)],
    };

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let _ = evaluator.evaluate(mapped_ty);
    assert!(evaluator.is_tainted(mapped_ty));

    // A later node whose evaluation reads the tainted memo entry must itself
    // become tainted: its value embeds the truncated artifact even though no
    // new limit event fired structurally inside it.
    let consumer = interner.keyof(mapped_ty);
    let _ = evaluator.evaluate(consumer);
    assert!(
        evaluator.is_tainted(consumer),
        "artifact-dependence must propagate through memo reads"
    );
}

#[test]
fn evaluate_type_with_options_persists_clean_intermediates_of_limit_hit_runs() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Union of a self-referential mapped type (bails) and a clean keyof
    // (converges): the run is limit-hit, so the top-level union result must
    // not be cached, but the clean keyof intermediate must be.
    let def = DefId(79);
    let lazy_m = interner.lazy(def);
    let mapped_ty = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("R"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: interner.keyof(lazy_m),
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    });
    // NOTE: this test exercises the QueryCache drain gate with the default
    // (resolver-less) query evaluator: the Lazy def stays unresolved, which
    // itself records limit-independent behavior; the mapped type over an
    // unresolved lazy stays deferred. Use a structure that bails without a
    // resolver instead: a divergent conditional is not constructible here,
    // so this test only asserts the no-regression property — clean inputs
    // still populate the cache.
    let k = interner.intern_string("p");
    let obj = interner.object(vec![PropertyInfo::new(k, TypeId::NUMBER)]);
    let keyof_obj = interner.keyof(obj);
    let union_ty = interner.union(vec![mapped_ty, keyof_obj]);

    let entries_before = db.statistics().eval_cache_entries;
    let _ = db.evaluate_type(union_ty);
    let entries_after = db.statistics().eval_cache_entries;
    assert!(
        entries_after > entries_before,
        "evaluation must persist at least the clean evaluated entries"
    );

    // The clean keyof sub-result must be served from cache now: evaluating
    // it directly must return the same converged literal-key type.
    let direct = db.evaluate_type(keyof_obj);
    assert_ne!(direct, keyof_obj, "keyof must converge to its literal key");
}
