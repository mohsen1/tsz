#[test]
fn test_narrow_excluding_positive_subset_memoizes_reduced_result() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let positive = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);

    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive),
        Some(TypeId::NUMBER)
    );
    assert_eq!(cache.narrow_positive_subset_cache.borrow().len(), 1);
    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        cache.narrow_positive_subset_cache.borrow().len(),
        1,
        "repeated shallow predicate exclusion should hit without growing the cache"
    );
}

#[test]
fn test_narrow_excluding_positive_subset_memoizes_no_reduction() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, TypeId::BOOLEAN),
        None
    );
    assert_eq!(cache.narrow_positive_subset_cache.borrow().len(), 1);
    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, TypeId::BOOLEAN),
        None
    );
    assert_eq!(
        cache.narrow_positive_subset_cache.borrow().len(),
        1,
        "no-reduction predicate exclusion results should be cached too"
    );
}

#[test]
fn test_narrow_excluding_positive_subset_memoizes_structural_filter() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let member_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let member_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let source = interner.union(vec![member_a, member_b]);
    let positive_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive_b),
        Some(member_a)
    );
    assert_eq!(cache.narrow_positive_subset_cache.borrow().len(), 1);
    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive_b),
        Some(member_a)
    );
    assert_eq!(
        cache.narrow_positive_subset_cache.borrow().len(),
        1,
        "structural positive-subset filters should reuse their memo entry"
    );
}

#[test]
fn test_narrow_excluding_positive_subset_filters_inert_structural_member_inside_positive_union() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let tag_name = interner.intern_string("tag");
    let value_name = interner.intern_string("value");
    let count_name = interner.intern_string("count");
    let tag_a = interner.literal_string("a");
    let tag_b = interner.literal_string("b");
    let member_a = interner.object(vec![
        PropertyInfo::new(tag_name, tag_a),
        PropertyInfo::new(value_name, TypeId::STRING),
    ]);
    let member_b = interner.object(vec![
        PropertyInfo::new(tag_name, tag_b),
        PropertyInfo::new(count_name, TypeId::NUMBER),
    ]);
    let positive_b = interner.object(vec![PropertyInfo::new(tag_name, tag_b)]);
    let source = interner.union(vec![TypeId::STRING, member_a, member_b]);
    let positive = interner.union(vec![TypeId::STRING, positive_b]);

    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive),
        Some(member_a)
    );
    assert_eq!(
        cache.cache_statistics().narrow_assignable_cache_entries,
        2,
        "only inert object members should be structurally probed against the positive object"
    );
}

#[test]
fn test_narrow_excluding_positive_subset_defers_recursive_structural_probe_after_identity_drop() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let tag_name = interner.intern_string("kind");
    let next_name = interner.intern_string("next");
    let extra_name = interner.intern_string("count");
    let tag_value = interner.literal_string("branch");
    let recursive = interner.recursive(0);
    let recursive_member = interner.object(vec![
        PropertyInfo::new(tag_name, tag_value),
        PropertyInfo::new(next_name, recursive),
        PropertyInfo::new(extra_name, TypeId::NUMBER),
    ]);
    let positive_recursive = interner.object(vec![
        PropertyInfo::new(tag_name, tag_value),
        PropertyInfo::new(next_name, recursive),
    ]);
    let source = interner.union(vec![TypeId::STRING, recursive_member]);
    let positive = interner.union(vec![TypeId::STRING, positive_recursive]);

    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive),
        Some(recursive_member)
    );
    let stats = cache.cache_statistics();
    assert_eq!(
        stats.narrow_assignable_cache_entries, 0,
        "recursive/evaluator-sensitive survivors should not enter structural assignability"
    );
    assert_eq!(
        stats.narrow_subtype_cache_entries, 0,
        "recursive/evaluator-sensitive survivors should stay on the tsc subset path"
    );
}

#[test]
fn test_narrow_excluding_positive_subset_deferred_no_reduction_returns_source() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let tag_name = interner.intern_string("shape");
    let next_name = interner.intern_string("child");
    let extra_name = interner.intern_string("rank");
    let tag_value = interner.literal_string("node");
    let recursive = interner.recursive(0);
    let recursive_member = interner.object(vec![
        PropertyInfo::new(tag_name, tag_value),
        PropertyInfo::new(next_name, recursive),
        PropertyInfo::new(extra_name, TypeId::NUMBER),
    ]);
    let unrelated_member = interner.object(vec![PropertyInfo::new(
        interner.intern_string("other"),
        TypeId::BOOLEAN,
    )]);
    let source = interner.union(vec![recursive_member, unrelated_member]);
    let positive_recursive = interner.object(vec![
        PropertyInfo::new(tag_name, tag_value),
        PropertyInfo::new(next_name, recursive),
    ]);

    assert_eq!(
        ctx.narrow_excluding_positive_subset(source, positive_recursive),
        Some(source),
        "deferred structural repair is a terminal unchanged predicate false branch"
    );
    let stats = cache.cache_statistics();
    assert_eq!(stats.narrow_positive_subset_cache_entries, 1);
    assert_eq!(stats.narrow_assignable_cache_entries, 0);
    assert_eq!(stats.narrow_subtype_cache_entries, 0);
}

#[test]
fn test_predicate_false_branch_publishes_positive_subset_memo() {
    let interner = TypeInterner::new();
    let cache = NarrowingCache::new();
    let ctx = NarrowingContext::with_cache(&interner, &cache);
    let member_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("left"),
        TypeId::STRING,
    )]);
    let member_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("right"),
        TypeId::NUMBER,
    )]);
    let source = interner.union(vec![member_a, member_b]);
    let predicate_target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("right"),
        TypeId::NUMBER,
    )]);
    let guard = TypeGuard::Predicate {
        type_id: Some(predicate_target),
        asserts: false,
    };

    assert_eq!(
        ctx.narrow_type(source, &guard, GuardSense::Negative),
        member_a
    );
    assert_eq!(
        cache.narrow_positive_subset_cache.borrow().len(),
        1,
        "predicate false-branch should memoize the shallow positive-subset exclusion"
    );
}
