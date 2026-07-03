#[test]
fn test_narrowing_subtype_probe_publishes_shared_relation_cache() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let first_cache = NarrowingCache::new();
    let first_ctx = NarrowingContext::with_cache(&db, &first_cache);
    assert_eq!(first_ctx.narrow_to_type(dog, animal), dog);
    let after_first = db.statistics();
    assert_eq!(after_first.relation.subtype_entries, 1);

    let second_cache = NarrowingCache::new();
    let second_ctx = NarrowingContext::with_cache(&db, &second_cache);
    assert_eq!(second_ctx.narrow_to_type(dog, animal), dog);
    let after_second = db.statistics();
    assert_eq!(after_second.relation.subtype_entries, 1);
    assert!(
        after_second.relation.subtype_hits > after_first.relation.subtype_hits,
        "a fresh narrowing cache should still reuse the shared subtype relation answer",
    );
}

#[test]
fn test_instanceof_member_filter_reuses_shared_relation_cache() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let source = interner.union2(dog, TypeId::NUMBER);

    let first_cache = NarrowingCache::new();
    let first_ctx = NarrowingContext::with_cache(&db, &first_cache);
    assert_eq!(first_ctx.narrow_by_instance_type(source, animal), dog);
    let after_first = db.statistics();
    assert!(after_first.relation.subtype_entries > 0);

    let second_cache = NarrowingCache::new();
    let second_ctx = NarrowingContext::with_cache(&db, &second_cache);
    assert_eq!(second_ctx.narrow_by_instance_type(source, animal), dog);
    let after_second = db.statistics();
    assert_eq!(
        after_second.relation.subtype_entries,
        after_first.relation.subtype_entries
    );
    assert!(
        after_second.relation.subtype_hits > after_first.relation.subtype_hits,
        "fresh instanceof narrowing should reuse shared subtype relation answers",
    );
}

#[test]
fn test_structural_narrowing_visitor_reuses_shared_relation_cache() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let first_cache = NarrowingCache::new();
    let first_ctx = NarrowingContext::with_cache(&db, &first_cache);
    assert_eq!(first_ctx.narrow(animal, dog), dog);
    let after_first = db.statistics();
    assert!(after_first.relation.subtype_entries > 0);

    let second_cache = NarrowingCache::new();
    let second_ctx = NarrowingContext::with_cache(&db, &second_cache);
    assert_eq!(second_ctx.narrow(animal, dog), dog);
    let after_second = db.statistics();
    assert_eq!(
        after_second.relation.subtype_entries,
        after_first.relation.subtype_entries
    );
    assert!(
        after_second.relation.subtype_hits > after_first.relation.subtype_hits,
        "fresh structural narrowing visitor should reuse shared subtype relation answers",
    );
}
