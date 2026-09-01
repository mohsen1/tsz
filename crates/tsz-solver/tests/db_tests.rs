use crate::caches::db::TypeApplicationEvalCache;
use crate::construction::{QueryCache, QueryCacheStatistics, RelationCacheProbe};
use crate::operations::property::PropertyAccessResult;
use crate::relations::relation_queries::RelationPolicy;
use crate::types::{TypeParamInfo, TypeParamOrigin};
use crate::{
    LiteralValue, ObjectFlags, PropertyInfo, QueryDatabase, RelationCacheConfig, RelationCacheKey,
    TupleElement, TypeData, TypeDatabase, TypeId, TypeInterner, Visibility,
};

impl<'a> QueryCache<'a> {
    fn eval_cache_len(&self) -> usize {
        self.eval_cache.borrow().len()
    }

    fn subtype_cache_len(&self) -> usize {
        self.subtype_cache.borrow().len()
    }

    fn assignability_cache_len(&self) -> usize {
        self.assignability_cache.borrow().len()
    }

    fn property_cache_len(&self) -> usize {
        self.property_cache.borrow().len()
    }

    fn element_access_cache_len(&self) -> usize {
        self.element_access_cache.borrow().len()
    }

    fn object_spread_properties_cache_len(&self) -> usize {
        self.object_spread_properties_cache.borrow().len()
    }

    fn intersection_merge_cache_len(&self) -> usize {
        self.intersection_merge_cache.borrow().total_entries()
    }
}

#[test]
fn type_database_interns_and_looks_up() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let hello = db.literal_string("hello");
    let key = db.lookup(hello).expect("type should be interned");

    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(db.resolve_atom(atom), "hello");
            assert_eq!(db.resolve_atom_ref(atom).as_ref(), "hello");
        }
        _ => panic!("expected string literal type"),
    }
}

#[test]
fn type_database_union_normalizes() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let union = db.union(vec![TypeId::STRING]);
    assert_eq!(union, TypeId::STRING);
}

#[test]
fn query_cache_caches_evaluate_and_subtype() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    assert_eq!(db.eval_cache_len(), 0);
    assert_eq!(db.subtype_cache_len(), 0);

    // Intrinsic types bypass the eval_cache entirely (fast path optimization).
    assert_eq!(db.evaluate_type(TypeId::STRING), TypeId::STRING);
    assert_eq!(db.eval_cache_len(), 0);
    assert_eq!(db.evaluate_type(TypeId::STRING), TypeId::STRING);
    assert_eq!(db.eval_cache_len(), 0);
    assert_eq!(db.property_cache_len(), 0);

    // Use a non-trivial pair for subtype caching: identity/top/bottom/error pairs
    // are now handled by the QueryCache fast-path and never reach the cache.
    let hello = interner.literal_string("hello");
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.subtype_cache_len(), 1);
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.subtype_cache_len(), 1);
}

#[test]
fn property_cache_skips_unresolved_lazy_result() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let prop = interner.intern_string("value");
    let unresolved = interner.lazy(crate::def::DefId(9001));

    let result = db.resolve_property_access_atom(unresolved, prop);
    assert!(
        matches!(
            result,
            PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                ..
            }
        ),
        "unresolved lazy fallback should still be returned, got {result:?}"
    );
    assert_eq!(
        db.property_cache_len(),
        0,
        "unresolved lazy fallback must not publish into the property cache"
    );
}

#[test]
fn property_cache_skips_unresolved_application_base_result() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let prop = interner.intern_string("value");
    let unresolved_base = interner.lazy(crate::def::DefId(9002));
    let app = interner.application(unresolved_base, vec![TypeId::STRING]);

    let result = db.resolve_property_access_atom(app, prop);
    assert!(
        matches!(
            result,
            PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                ..
            }
        ),
        "unresolved application fallback should still be returned, got {result:?}"
    );
    assert_eq!(
        db.property_cache_len(),
        0,
        "unresolved lazy application fallback must not publish into the property cache"
    );
}

#[test]
fn property_cache_keeps_resolved_object_property_result() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let prop = interner.intern_string("value");
    let obj = interner.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);

    let result = db.resolve_property_access_atom(obj, prop);
    assert!(
        matches!(
            result,
            PropertyAccessResult::Success {
                type_id: TypeId::NUMBER,
                ..
            }
        ),
        "resolved object property should still be returned, got {result:?}"
    );
    assert_eq!(
        db.property_cache_len(),
        1,
        "resolved object property access should still publish into the property cache"
    );
}

/// #16553: a clean-looking sibling `IndexAccess` node evaluated within the
/// same top-level walk as an unrelated `IndexAccess` into an unresolved
/// `Lazy` must not publish into the cross-file `eval_cache` either.
///
/// Two independent write paths had to agree for this to hold, and fixing
/// only one is silently insufficient (confirmed the hard way — an earlier
/// version of this fix passed every existing test while still leaking this
/// exact entry):
///  - `memo_insert`'s per-node epoch check only proves *this node's own*
///    evaluation window saw no new limit event, not that the evaluator-wide
///    `unresolved_def_seen` flag stayed clear (an unrelated sibling visited
///    earlier in the same walk can have already set it), so its persistent
///    write-through needs its own `!self.is_unresolved_def_seen()` guard.
///  - `evaluate_type_with_options`'s end-of-walk intermediate drain
///    unconditionally re-publishes *everything* `memo_insert` ever put in
///    the per-evaluator local cache (filtered only by the per-node `tainted`
///    set, which the first guard does not populate for a clean-window node)
///    — so even with the first guard in place, this drain alone republishes
///    the same entry a second, ungated way. It needs the stricter
///    `is_stable_for_run_wide_cache` check too.
///
/// Uses `union_from_sorted_vec` (which trusts caller-provided order) rather
/// than `union`, which canonically re-sorts members by `TypeId` — a `Lazy`
/// member's `TypeId` sorted after a plain `Object` member's in practice,
/// which made an earlier `union`-based version of this test visit the clean
/// sibling *first* and never exercise the intended contamination order.
/// `Intersection`'s concrete-index path was also tried and rejected: it
/// returns as soon as it hits the first unresolved member, so the sibling is
/// never reached at all regardless of this fix.
#[test]
fn eval_cache_skips_sibling_of_unresolved_lazy_index_access_in_same_union() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let tag_atom = interner.intern_string("tag");
    let tag_type = interner.literal_string("tag");
    let unresolved = interner.lazy(crate::def::DefId(9003));
    let resolved_obj = interner.object(vec![PropertyInfo::new(tag_atom, TypeId::NUMBER)]);
    let union = interner.union_from_sorted_vec(vec![unresolved, resolved_obj]);
    let union_index_access = interner.index_access(union, tag_type);
    let sibling_index_access = interner.index_access(resolved_obj, tag_type);

    db.evaluate_type(union_index_access);

    let sibling_key =
        crate::evaluation::request::EvaluationCacheKey::new(sibling_index_access, false, false);
    assert!(
        db.eval_cache.borrow().get(&sibling_key).is_none(),
        "a clean sibling IndexAccess evaluated after an unrelated union \
         member tainted this evaluator's unresolved_def_seen flag must not \
         publish into the cross-file eval_cache"
    );
}

/// Test cache poisoning prevention.
///
/// CRITICAL: This test ensures that separate caches don't interfere.
/// The assignability cache (`CompatChecker`) and subtype cache (`SubtypeChecker`)
/// are kept separate to prevent cross-contamination.
///
/// Even though both may return similar results for basic `any` checks,
/// the caches must be separate because they can diverge in complex cases.
#[test]
fn test_cache_poisoning_prevention() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Use non-trivial pairs to avoid QueryCache fast-paths (identity/top/bottom/error/any).
    let hello = interner.literal_string("hello");

    // 1. Check assignability - uses CompatChecker with TS rules
    assert!(db.is_assignable_to(hello, TypeId::STRING));
    assert_eq!(db.assignability_cache_len(), 1);
    assert_eq!(db.subtype_cache_len(), 0);

    // 2. Check subtype - uses SubtypeChecker
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.assignability_cache_len(), 1);
    assert_eq!(db.subtype_cache_len(), 1);

    // 3. Verify caches are separate - both have 1 entry proving they're independent
    assert!(db.is_assignable_to(hello, TypeId::STRING)); // Cache hit
    assert!(db.is_subtype_of(hello, TypeId::STRING)); // Cache hit

    // Check cache hit (no growth)
    assert_eq!(db.assignability_cache_len(), 1);
    assert_eq!(db.subtype_cache_len(), 1);
}

#[test]
fn relation_cache_stats_track_hits_and_misses() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    db.reset_relation_cache_stats();

    // Use non-trivial pair to avoid QueryCache fast-path (identity/top/bottom/error).
    let hello = interner.literal_string("hello");
    let key = RelationCacheKey::for_subtype(
        hello,
        TypeId::STRING,
        RelationPolicy::unflagged_compatibility().cache_config(),
    );
    let assignability_key = RelationCacheKey::for_assignability(
        hello,
        TypeId::STRING,
        RelationPolicy::unflagged_compatibility().cache_config(),
    );

    assert_eq!(
        db.probe_subtype_cache(key),
        RelationCacheProbe::MissNotCached
    );
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.probe_subtype_cache(key), RelationCacheProbe::Hit(true));
    assert_eq!(db.lookup_assignability_cache(assignability_key), None);
    assert!(db.is_assignable_to(hello, TypeId::STRING));
    assert_eq!(db.lookup_assignability_cache(assignability_key), Some(true));

    let stats = db.relation_cache_stats();
    assert!(stats.subtype_hits >= 1);
    assert!(stats.subtype_misses >= 1);
    assert!(stats.subtype_entries >= 1);
    assert!(stats.assignability_hits >= 1);
    assert!(stats.assignability_misses >= 1);
    assert!(stats.assignability_entries >= 1);
}

/// Test that `is_subtype_of` and `is_assignable_to` both handle `any` correctly.
///
/// The key difference is:
/// - `is_subtype_of`: Direct `SubtypeChecker` - structural subtyping with any propagation
/// - `is_assignable_to`: `CompatChecker` - adds weak type detection, empty object rules, etc.
///
/// For basic `any` checks, both return true (TypeScript compatibility).
#[test]
fn test_is_subtype_vs_is_assignable_any() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // For `any`, both methods handle any propagation:
    // - is_subtype_of: any is subtype of everything (SubtypeChecker)
    // - is_assignable_to: any is assignable to everything (CompatChecker)

    assert!(db.is_subtype_of(TypeId::ANY, TypeId::NUMBER));
    assert!(db.is_assignable_to(TypeId::ANY, TypeId::NUMBER));

    // Symmetric check
    assert!(db.is_subtype_of(TypeId::NUMBER, TypeId::ANY));
    assert!(db.is_assignable_to(TypeId::NUMBER, TypeId::ANY));
}

#[test]
fn query_cache_caches_element_access_type() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let tuple_type = db.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert_eq!(db.element_access_cache_len(), 0);
    let first = db.resolve_element_access_type(tuple_type, interner.literal_number(0.0), Some(0));
    assert_eq!(first, TypeId::STRING);
    assert_eq!(db.element_access_cache_len(), 1);

    let second = db.resolve_element_access_type(tuple_type, interner.literal_number(0.0), Some(0));
    assert_eq!(second, first);
    assert_eq!(db.element_access_cache_len(), 1);
}

#[test]
fn query_cache_caches_object_spread_properties() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let first_obj = db.object(vec![PropertyInfo {
        name: interner.intern_string("first"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    let second_obj = db.object_with_flags(
        vec![PropertyInfo {
            name: interner.intern_string("second"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }],
        ObjectFlags::FRESH_LITERAL,
    );

    let spread_type = db.intersection(vec![first_obj, second_obj]);
    assert_eq!(db.object_spread_properties_cache_len(), 0);

    let props = db.collect_object_spread_properties(spread_type);
    assert_eq!(props.len(), 2);
    assert!(
        props
            .iter()
            .any(|p| interner.resolve_atom_ref(p.name).as_ref() == "first")
    );
    assert!(
        props
            .iter()
            .any(|p| interner.resolve_atom_ref(p.name).as_ref() == "second")
    );
    assert_eq!(db.object_spread_properties_cache_len(), 1);

    let props_again = db.collect_object_spread_properties(spread_type);
    assert_eq!(props_again.len(), 2);
    assert_eq!(db.object_spread_properties_cache_len(), 1);
}

#[test]
fn object_spread_intersection_preserves_traversal_order_with_overrides() {
    // Structural rule: spreading intersection members follows member traversal
    // order for display/cache identity, while shared property types keep the
    // intersection semantics of the normalized source type.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let first = interner.intern_string("first");
    let shared = interner.intern_string("shared");
    let last = interner.intern_string("last");

    let left = db.object(vec![
        PropertyInfo::new(first, TypeId::STRING),
        PropertyInfo::new(shared, TypeId::NUMBER),
    ]);
    let right = db.object(vec![
        PropertyInfo::new(shared, TypeId::BOOLEAN),
        PropertyInfo::new(last, TypeId::NULL),
    ]);
    let spread_type = db.intersection(vec![left, right]);

    let props = db.collect_object_spread_properties(spread_type);
    let names: Vec<_> = props
        .iter()
        .map(|prop| interner.resolve_atom_ref(prop.name).to_string())
        .collect();

    assert_eq!(names, ["first", "shared", "last"]);
    let shared_prop = props
        .iter()
        .find(|prop| prop.name == shared)
        .expect("shared property should remain present");
    assert_eq!(
        shared_prop.type_id,
        db.intersect_types_raw2(TypeId::NUMBER, TypeId::BOOLEAN)
    );
}

#[test]
fn object_spread_union_preserves_first_seen_property_order() {
    // Structural rule: union spread combines properties in first-seen member
    // order, then marks properties optional when not every non-nullish member
    // contributes them.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let alpha = interner.intern_string("alpha");
    let shared = interner.intern_string("shared");
    let omega = interner.intern_string("omega");

    let left = db.object(vec![
        PropertyInfo::new(alpha, TypeId::STRING),
        PropertyInfo::new(shared, TypeId::NUMBER),
    ]);
    let right = db.object(vec![
        PropertyInfo::new(shared, TypeId::BOOLEAN),
        PropertyInfo::new(omega, TypeId::NULL),
    ]);
    let spread_type = db.union(vec![left, right]);

    let props = db.collect_object_spread_properties(spread_type);
    let names: Vec<_> = props
        .iter()
        .map(|prop| interner.resolve_atom_ref(prop.name).to_string())
        .collect();

    assert_eq!(names, ["alpha", "shared", "omega"]);
    let shared_prop = props
        .iter()
        .find(|prop| prop.name == shared)
        .expect("shared property should remain present");
    assert_eq!(
        shared_prop.type_id,
        db.union2(TypeId::NUMBER, TypeId::BOOLEAN)
    );
    assert!(!shared_prop.optional);
    assert!(
        props
            .iter()
            .find(|prop| prop.name == alpha)
            .expect("alpha property should remain present")
            .optional
    );
    assert!(
        props
            .iter()
            .find(|prop| prop.name == omega)
            .expect("omega property should remain present")
            .optional
    );
}

#[test]
fn object_spread_union_sibling_constraints_reenter_shared_constraint() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let shared = db.object(vec![PropertyInfo::new(
        interner.intern_string("shared"),
        TypeId::STRING,
    )]);
    let left = db.type_param(TypeParamInfo {
        name: interner.intern_string("Left"),
        constraint: Some(shared),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    let right = db.type_param(TypeParamInfo {
        name: interner.intern_string("Right"),
        constraint: Some(shared),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    let spread_type = db.union(vec![left, right]);

    let props = db.collect_object_spread_properties(spread_type);

    assert_eq!(props.len(), 1);
    let shared_prop = props
        .iter()
        .find(|prop| interner.resolve_atom_ref(prop.name).as_ref() == "shared")
        .expect("spread should retain the shared constrained property");
    assert_eq!(shared_prop.type_id, TypeId::STRING);
    assert!(
        !shared_prop.optional,
        "both non-nullish sibling constraints provide the property"
    );
    assert_eq!(db.object_spread_properties_cache_len(), 1);
}

/// `prune_impossible_object_union_members` is a pure function of the input union
/// `TypeId` (only structural predicates over the immutable interned `DAG`), so its
/// result is memoized project-wide on the interner. Object-union property access
/// re-asks the same discriminated-union `TypeId` once per property read; this pins
/// both the byte-identical pruned value and the cross-call cache reuse.
#[test]
fn prune_impossible_object_union_members_memo_is_byte_identical_and_reused() {
    let interner = TypeInterner::new();

    let kind_prop = |lit: TypeId, order: u32| PropertyInfo {
        name: interner.intern_string("kind"),
        type_id: lit,
        write_type: lit,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    };

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    // An intersection with conflicting required `kind` discriminants is a
    // structurally impossible object member, so pruning drops it. Build it with
    // the raw (un-normalized) intersection so it survives as an `Intersection`
    // node and actually reaches the prune predicate, rather than being collapsed
    // to `never` at construction time.
    let impossible = interner.intersect_types_raw2(
        interner.object(vec![kind_prop(lit_a, 0)]),
        interner.object(vec![kind_prop(lit_b, 0)]),
    );
    let valid = interner.object(vec![kind_prop(lit_c, 0)]);
    let union = interner.union_preserve_members(vec![impossible, valid]);

    // The setup must hold a real two-member `Union` so the prune path is exercised
    // (otherwise the early non-`Union` guard would short-circuit the memo).
    let Some(TypeData::Union(members)) = interner.lookup(union) else {
        panic!("expected a Union for the prune-memo setup, got {union:?}");
    };
    assert_eq!(interner.type_list(members).len(), 2);

    let db: &dyn TypeDatabase = &interner;

    // Pruning drops the structurally impossible intersection member, leaving the
    // single valid object — the byte-identical structural answer.
    let pruned = crate::type_queries::prune_impossible_object_union_members(db, union);
    assert_eq!(pruned, valid);

    // The result is memoized on the interner, so a second call returns the same
    // value from the cache rather than re-walking the union.
    assert_eq!(db.prune_union_members_memo(union), Some(pruned));
    let pruned_again = crate::type_queries::prune_impossible_object_union_members(db, union);
    assert_eq!(pruned_again, pruned);

    // Non-union inputs are returned unchanged (the early `Union` guard) and never
    // populate the union-keyed memo.
    assert_eq!(
        crate::type_queries::prune_impossible_object_union_members(db, TypeId::NUMBER),
        TypeId::NUMBER
    );
    assert_eq!(db.prune_union_members_memo(TypeId::NUMBER), None);
}

#[test]
fn prune_impossible_object_union_members_retains_required_never_property() {
    let interner = TypeInterner::new();
    let marker = interner.intern_string("marker");
    let common = interner.intern_string("common");
    let branded = interner.object(vec![
        PropertyInfo::new(marker, TypeId::NEVER),
        PropertyInfo::new(common, TypeId::STRING),
    ]);
    let ordinary = interner.object(vec![PropertyInfo::new(common, TypeId::NUMBER)]);
    let union = interner.union_preserve_members(vec![branded, ordinary]);

    let Some(TypeData::Union(members)) = interner.lookup(union) else {
        panic!("expected a Union for the required-never setup, got {union:?}");
    };
    assert_eq!(interner.type_list(members).len(), 2);

    let db: &dyn TypeDatabase = &interner;
    let pruned = crate::type_queries::prune_impossible_object_union_members(db, union);

    assert_eq!(
        pruned, union,
        "a required `never` property makes construction difficult but does not make the object member impossible"
    );
    assert_eq!(db.prune_union_members_memo(union), Some(union));
}

#[test]
fn prune_impossible_object_union_members_retains_merged_authored_never_property() {
    let interner = TypeInterner::new();
    let marker = interner.intern_string("marker");
    let common = interner.intern_string("common");
    let extra = interner.intern_string("extra");
    let branded = interner.object(vec![
        PropertyInfo::new(marker, TypeId::NEVER),
        PropertyInfo::new(common, TypeId::STRING),
    ]);
    let extra_shape = interner.object(vec![PropertyInfo::new(extra, TypeId::BOOLEAN)]);
    let merged = interner.intersection(vec![branded, extra_shape]);
    let ordinary = interner.object(vec![PropertyInfo::new(common, TypeId::NUMBER)]);
    let union = interner.union_preserve_members(vec![merged, ordinary]);

    assert!(
        interner.get_merged_intersection_origin(merged).is_some(),
        "the negative control must exercise a merged intersection"
    );

    let db: &dyn TypeDatabase = &interner;
    let pruned = crate::type_queries::prune_impossible_object_union_members(db, union);
    assert_eq!(
        pruned, union,
        "an authored required `never` remains inhabitable even after object-intersection merging"
    );
}

#[test]
fn type_interner_query_db_tracks_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;

    assert!(!db.no_unchecked_indexed_access());
    db.set_no_unchecked_indexed_access(true);
    assert!(db.no_unchecked_indexed_access());
    db.set_no_unchecked_indexed_access(false);
    assert!(!db.no_unchecked_indexed_access());
}

#[test]
fn type_interner_element_access_respects_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;

    let array = interner.array(TypeId::STRING);
    let without_flag = db.resolve_element_access_type(array, TypeId::NUMBER, None);
    assert_eq!(without_flag, TypeId::STRING);

    db.set_no_unchecked_indexed_access(true);
    let with_flag = db.resolve_element_access_type(array, TypeId::NUMBER, None);
    assert_ne!(with_flag, TypeId::STRING);
    assert!(crate::narrowing::type_contains_undefined(
        &interner, with_flag
    ));
}

#[test]
fn query_cache_set_strict_null_checks_propagates_to_wrapped_interner() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &QueryCache::new(&interner);

    // `CheckerContext::from_parts` only holds `&dyn QueryDatabase` (a
    // `QueryCache` in production) and calls `set_strict_null_checks` on it.
    // `QueryCache` used to update only its own local `Cell`, leaving the
    // wrapped `TypeInterner`'s `AtomicBool` at its `true` default -- and
    // union construction (`normalize_union`) reads that field directly via
    // `TypeInterner`'s own inherent methods, never through `QueryDatabase`.
    assert!(
        interner.strict_null_checks(),
        "interner starts strict by default"
    );
    db.set_strict_null_checks(false);
    assert!(
        !interner.strict_null_checks(),
        "QueryCache::set_strict_null_checks must propagate to the wrapped TypeInterner"
    );

    // End-to-end: the same union construction the checker's non-strict
    // member-dropping rule (`addTypeToUnion` never adds a nullish constituent
    // when a non-nullish sibling is present) depends on must observe the
    // propagated flag, not just `QueryCache`'s own local copy.
    let reduced = db.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    assert_eq!(
        reduced,
        TypeId::STRING,
        "non-strict union construction must drop nullish members once the flag is set"
    );
}

#[test]
fn query_cache_set_exact_optional_property_types_propagates_to_wrapped_interner() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &QueryCache::new(&interner);

    // Same desync family as `strict_null_checks` above: tuple-element
    // normalization (`normalize_optional_tuple_elements`) reads
    // `exact_optional_property_types` on `TypeInterner` directly.
    assert!(!interner.exact_optional_property_types());
    db.set_exact_optional_property_types(true);
    assert!(
        interner.exact_optional_property_types(),
        "QueryCache::set_exact_optional_property_types must propagate to the wrapped TypeInterner"
    );
}

#[test]
fn query_cache_statistics_reflects_cache_population() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Empty cache should have zero entries everywhere.
    let stats = cache.statistics();
    assert_eq!(stats, QueryCacheStatistics::default());

    // Use non-trivial pairs to avoid QueryCache fast-path (identity/top/bottom/error/any).
    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // Subtype check populates the subtype cache.
    let _ = cache.is_subtype_of(hello, TypeId::STRING);

    // Assignability check populates the assignability cache.
    let _ = cache.is_assignable_to(world, TypeId::STRING);

    let stats = cache.statistics();
    // Relation caches should have entries from the checks above.
    assert!(
        stats.relation.subtype_entries >= 1,
        "subtype cache should be populated: {}",
        stats.relation.subtype_entries,
    );
    assert!(
        stats.relation.assignability_entries >= 1,
        "assignability cache should be populated: {}",
        stats.relation.assignability_entries,
    );
    // Display impl should not panic.
    let display_output = format!("{stats}");
    assert!(display_output.contains("QueryCache statistics:"));
    assert!(display_output.contains("eval_cache:"));
    assert!(display_output.contains("subtype_cache:"));
    assert!(display_output.contains("assignability_cache:"));
    assert!(display_output.contains("estimated_size:"));
}

#[test]
fn query_cache_estimated_size_bytes_empty() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Empty cache should still have nonzero size (Self struct)
    let size = cache.estimated_size_bytes();
    assert!(
        size > 0,
        "empty QueryCache should have nonzero estimated size"
    );
    assert!(
        size < 4096,
        "empty QueryCache should be small, got {size} bytes"
    );

    // Statistics-based estimate should be zero for empty caches
    let stats = cache.statistics();
    assert_eq!(stats.estimated_size_bytes(), 0);
}

#[test]
fn query_cache_estimated_size_grows_with_entries() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    let empty_size = cache.estimated_size_bytes();

    // Add some eval cache entries
    let str_type = interner.literal_string("hello");
    let num_type = interner.literal_number(42.0);
    cache.evaluate_type(str_type);
    cache.evaluate_type(num_type);

    // Add subtype cache entries
    cache.insert_subtype_cache(
        RelationCacheKey::for_subtype(str_type, num_type, RelationCacheConfig::default()),
        false,
    );

    // Add assignability cache entries
    cache.insert_assignability_cache(
        RelationCacheKey::for_assignability(str_type, num_type, RelationCacheConfig::default()),
        false,
    );

    let populated_size = cache.estimated_size_bytes();
    assert!(
        populated_size > empty_size,
        "populated cache ({populated_size}) should be larger than empty ({empty_size})"
    );

    // Statistics snapshot should also show nonzero estimated size
    let stats = cache.statistics();
    assert!(
        stats.estimated_size_bytes() > 0,
        "statistics estimated_size_bytes should be nonzero after populating caches"
    );
}

#[test]
fn query_cache_estimated_size_resets_on_clear() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Populate
    let str_type = interner.literal_string("test");
    cache.evaluate_type(str_type);
    cache.insert_subtype_cache(
        RelationCacheKey::for_subtype(str_type, TypeId::NUMBER, RelationCacheConfig::default()),
        true,
    );
    cache.insert_intersection_merge(str_type, 1, Some(TypeId::STRING));

    let before_clear = cache.estimated_size_bytes();
    assert_eq!(cache.intersection_merge_cache_len(), 1);

    cache.clear();

    let after_clear = cache.estimated_size_bytes();
    assert_eq!(cache.intersection_merge_cache_len(), 0);
    assert_eq!(cache.lookup_intersection_merge(str_type, 1), None);
    // After clear, size should not exceed before_clear (maps may retain capacity).
    // The key invariant: statistics-based estimate resets to zero.
    let stats = cache.statistics();
    assert_eq!(
        stats.estimated_size_bytes(),
        0,
        "statistics estimated_size_bytes should be 0 after clear"
    );
    // Live estimate may retain capacity but should be reasonable
    assert!(
        after_clear <= before_clear,
        "live estimate should not grow after clear ({after_clear} vs {before_clear})"
    );
}

#[test]
fn query_cache_statistics_merge_preserves_estimated_size() {
    let mut stats_a = QueryCacheStatistics {
        eval_cache_entries: 10,
        property_cache_entries: 5,
        ..Default::default()
    };
    let stats_b = QueryCacheStatistics {
        eval_cache_entries: 20,
        property_cache_entries: 15,
        ..Default::default()
    };

    let size_a = stats_a.estimated_size_bytes();
    let size_b = stats_b.estimated_size_bytes();

    stats_a.merge(&stats_b);

    let merged_size = stats_a.estimated_size_bytes();
    assert_eq!(
        merged_size,
        size_a + size_b,
        "merged estimated_size_bytes should equal sum of parts"
    );
}

/// Regression for issue #10970: the closed-evaluation cache must encode
/// `exactOptionalPropertyTypes` in its key.
///
/// A closed type's evaluation can depend on `exactOptionalPropertyTypes` (a
/// homomorphic mapped type's optional-modifier stripping is gated on it). The
/// owning interner's option can change between a cache write and a later read
/// (the explicit reset boundary the issue asks for). Without the option in the
/// key, a stale result computed under the old option value would be returned.
///
/// This drives the `TypeApplicationEvalCache` boundary directly: a value is
/// stored under one option value, then the option is flipped. Before the fix
/// the lookup ignored the option and returned the stale entry; after the fix
/// the key differs and the lookup correctly misses.
#[test]
fn closed_eval_cache_keys_on_exact_optional_property_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let key_type = interner.literal_string("payload");
    let stored = TypeId::STRING;

    // Store a result under exactOptionalPropertyTypes = false.
    db.set_exact_optional_property_types(false);
    db.insert_closed_eval_cache(key_type, false, stored);
    assert_eq!(
        db.lookup_closed_eval_cache(key_type, false),
        Some(stored),
        "the entry must be visible under the option value it was written with"
    );

    // Flip the option. The entry computed under the old value must not be
    // reused: the option is part of the cache identity.
    db.set_exact_optional_property_types(true);
    assert_eq!(
        db.lookup_closed_eval_cache(key_type, false),
        None,
        "closed-eval lookup must miss after exactOptionalPropertyTypes changes"
    );

    // Restoring the original option value makes the original entry visible
    // again, proving the two option values address distinct slots.
    db.set_exact_optional_property_types(false);
    assert_eq!(db.lookup_closed_eval_cache(key_type, false), Some(stored));
}

/// Companion to the closed-eval test for the generic-application evaluation
/// cache, which shares the same option-sensitivity (a `Foo<Args>` application
/// can expand to a mapped type whose stripping depends on the option).
#[test]
fn application_eval_cache_keys_on_exact_optional_property_types() {
    use crate::def::DefId;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let def_id = DefId(7);
    let args = [TypeId::STRING];
    let stored = TypeId::NUMBER;

    db.set_exact_optional_property_types(false);
    // Disambiguate from the inherent `QueryCache::insert_application_eval_cache`
    // (which takes a pre-built tuple key) by calling the trait method directly.
    TypeApplicationEvalCache::insert_application_eval_cache(&db, def_id, &args, false, stored);
    assert_eq!(
        db.lookup_application_eval_cache(def_id, &args, false),
        Some(stored)
    );

    db.set_exact_optional_property_types(true);
    assert_eq!(
        db.lookup_application_eval_cache(def_id, &args, false),
        None,
        "application-eval lookup must miss after exactOptionalPropertyTypes changes"
    );

    db.set_exact_optional_property_types(false);
    assert_eq!(
        db.lookup_application_eval_cache(def_id, &args, false),
        Some(stored)
    );
}
