//! Context-dependent class-check cache tests.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::SubtypeChecker;
use crate::types::{IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeId};

#[test]
fn subtype_cache_skips_class_check_context() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source_symbol = SymbolId(42);
    let class_ref = crate::SymbolRef(source_symbol.0);
    let is_class = |symbol: crate::SymbolRef| symbol == class_ref;

    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
            PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(source_symbol),
    );
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let mut class_uncached = SubtypeChecker::new(&interner).with_class_check(&is_class);
    assert!(
        !class_uncached.is_subtype_of(source, target),
        "named class/interface sources need an explicit string index signature",
    );

    let mut structural_uncached = SubtypeChecker::new(&interner);
    assert!(
        structural_uncached.is_subtype_of(source, target),
        "without class-symbol context the same shape is an ordinary structural object",
    );

    let mut class_cached = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_class_check(&is_class);
    let class_key = class_cached.debug_cache_key_for(source, target);
    assert!(
        !class_cached.is_subtype_of(source, target),
        "cached class-context relation should preserve the uncached class-context answer",
    );
    assert_eq!(
        db.lookup_subtype_cache(class_key),
        None,
        "class-check context is behavior-affecting and must not populate a shared class-agnostic slot",
    );

    let mut structural_cached = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(
        structural_cached.is_subtype_of(source, target),
        "a class-context result must not be reused by a structural checker without class context",
    );
}

#[test]
fn class_check_context_verdict_uses_instance_local_memo_not_shared_cache() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source_symbol = SymbolId(44);
    let class_ref = crate::SymbolRef(source_symbol.0);
    let is_class = |symbol: crate::SymbolRef| symbol == class_ref;

    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
            PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(source_symbol),
    );
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let mut checker = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_class_check(&is_class);
    let key = checker.debug_cache_key_for(source, target);

    // A class-context verdict is excluded from the cross-checker shared cache
    // (it depends on the `is_class_symbol` closure), so the first check
    // populates the instance-local fallback memo instead (issue #13828).
    assert!(!checker.is_subtype_of(source, target));
    assert_eq!(
        db.lookup_subtype_cache(key),
        None,
        "class-context verdict must not populate the shared class-agnostic slot",
    );
    assert_eq!(
        checker.local_relation_cache.get(&key),
        Some(&false),
        "class-context verdict must be memoized in the instance-local fallback",
    );

    // A repeat comparison on the same instance is served from the local memo
    // and preserves the verdict.
    assert!(!checker.is_subtype_of(source, target));

    // `reset` clears the instance-local memo so a reused checker never carries a
    // context-bound verdict into a fresh context.
    checker.reset();
    assert!(
        checker.local_relation_cache.is_empty(),
        "reset must clear the instance-local relation memo",
    );

    // A separate structural checker (no class context) computes the ordinary
    // structural answer, unaffected by the per-instance memo — the verdict is
    // never shared across instances.
    let mut structural = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(structural.is_subtype_of(source, target));
}

#[test]
fn assignability_relation_context_propagates_class_check_without_shared_cache() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source_symbol = SymbolId(43);
    let class_ref = crate::SymbolRef(source_symbol.0);
    let is_class = |symbol: crate::SymbolRef| symbol == class_ref;

    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
            PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(source_symbol),
    );
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });
    let class_context = RelationContext {
        query_db: Some(&db),
        class_check: Some(&is_class),
        ..RelationContext::default()
    };

    let class_key = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_class_check(&is_class)
        .debug_cache_key_for(source, target);

    assert!(
        !query_relation(
            &interner,
            source,
            target,
            RelationKind::Assignable,
            RelationPolicy::default(),
            class_context,
        )
        .is_related(),
        "assignability relation context must preserve class/interface index-signature rules",
    );
    assert_eq!(
        db.lookup_subtype_cache(class_key),
        None,
        "class-check assignability context must not populate a shared class-agnostic subtype slot",
    );

    assert!(
        query_relation(
            &interner,
            source,
            target,
            RelationKind::Assignable,
            RelationPolicy::default(),
            RelationContext {
                query_db: Some(&db),
                ..RelationContext::default()
            },
        )
        .is_related(),
        "without class-symbol context the same shape remains structurally assignable",
    );
}
