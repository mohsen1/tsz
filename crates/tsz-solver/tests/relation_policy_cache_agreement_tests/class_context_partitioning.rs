//! Class-check relation cache partitioning tests.
//!
//! A class-symbol classifier is behavior-affecting: it can make a class-flagged
//! symbol that has no resolvable `DefId` behave nominally (it then needs an
//! explicit declared index signature) where, absent the classifier, the same
//! shape is judged purely structurally. Those verdicts must never share a cache
//! slot, but they *can* both live in the cross-checker shared cache once the key
//! is discriminated by `RelationFlags::CLASS_CHECK_CONTEXT` (issue #13828). The
//! classifier is a pure function of the program binder, fixed for the whole
//! compilation, so a single discriminating bit fully partitions the regimes.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::SubtypeChecker;
use crate::types::{IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeId};

/// Build the `{a: number, b: number}` (symbol-tagged) source and the
/// `{[s: string]: number}` index-signature target shared by these tests.
fn nominal_vs_index_pair(
    interner: &TypeInterner,
    source_symbol: tsz_binder::SymbolId,
) -> (TypeId, TypeId) {
    let source = interner.object_with_flags_and_symbol(
        vec![
            PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
            PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        ],
        ObjectFlags::empty(),
        Some(source_symbol),
    );
    let target = interner.object_with_index(ObjectShape {
        base_types: Vec::new(),
        symbol_index: None,
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
    (source, target)
}

#[test]
fn class_and_structural_contexts_use_distinct_cache_keys() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let source_symbol = SymbolId(42);
    let class_ref = crate::SymbolRef(source_symbol.0);
    let is_class = |symbol: crate::SymbolRef| symbol == class_ref;
    let (source, target) = nominal_vs_index_pair(&interner, source_symbol);

    // The two regimes still disagree on the verdict: the class-tagged source
    // needs an explicit string index signature, the structural one does not.
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

    // Because the verdicts differ, the keys must differ so they never collide.
    let class_key = SubtypeChecker::new(&interner)
        .with_class_check(&is_class)
        .debug_cache_key_for(source, target);
    let structural_key = SubtypeChecker::new(&interner).debug_cache_key_for(source, target);
    assert_ne!(
        class_key, structural_key,
        "class-context and class-agnostic keys must occupy different cache slots",
    );
    assert!(
        class_key
            .config
            .flags
            .contains(crate::types::RelationFlags::CLASS_CHECK_CONTEXT),
        "the class-context key must carry the CLASS_CHECK_CONTEXT discriminator",
    );
    assert!(
        !structural_key
            .config
            .flags
            .contains(crate::types::RelationFlags::CLASS_CHECK_CONTEXT),
        "the class-agnostic key must keep the CLASS_CHECK_CONTEXT bit clear",
    );
}

#[test]
fn class_context_verdict_is_shared_cross_checker_in_its_own_partition() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source_symbol = SymbolId(44);
    let class_ref = crate::SymbolRef(source_symbol.0);
    let is_class = |symbol: crate::SymbolRef| symbol == class_ref;
    let (source, target) = nominal_vs_index_pair(&interner, source_symbol);

    // First class-context checker populates the shared cache under the
    // class-context-discriminated key — not the instance-local memo (the
    // class-check bypass is gone, issue #13828).
    let mut first = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_class_check(&is_class);
    let class_key = first.debug_cache_key_for(source, target);
    assert!(!first.is_subtype_of(source, target));
    assert_eq!(
        db.lookup_subtype_cache(class_key),
        Some(false),
        "class-context verdict must be recorded in the shared cache under its discriminated key",
    );
    assert!(
        first.local_relation_cache.is_empty(),
        "class-context verdict must no longer route to the instance-local memo",
    );

    // A second, independent class-context checker reads that shared verdict —
    // the cross-checker reuse #13828 is about.
    let mut second = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_class_check(&is_class);
    assert!(!second.is_subtype_of(source, target));

    // A class-agnostic checker keys to a *different* slot (bit clear), so it is
    // never served the nominal verdict — it computes the structural answer.
    let structural_key = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .debug_cache_key_for(source, target);
    let mut structural = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(
        structural.is_subtype_of(source, target),
        "a class-context verdict must never poison the class-agnostic regime",
    );
    assert_eq!(
        db.lookup_subtype_cache(structural_key),
        Some(true),
        "the structural regime records its own verdict in its own slot",
    );
}

#[test]
fn assignability_relation_context_partitions_class_check_in_shared_cache() {
    use tsz_binder::SymbolId;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source_symbol = SymbolId(43);
    let class_ref = crate::SymbolRef(source_symbol.0);
    let is_class = |symbol: crate::SymbolRef| symbol == class_ref;
    let (source, target) = nominal_vs_index_pair(&interner, source_symbol);

    // Class-context assignability preserves the nominal index-signature rule.
    let class_context = RelationContext {
        query_db: Some(&db),
        class_check: Some(&is_class),
        ..RelationContext::default()
    };
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

    // The class-agnostic assignability of the same pair is unaffected by any
    // shared-cache entry the class-context run left behind: it keys to a
    // different slot and computes the structural answer.
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
