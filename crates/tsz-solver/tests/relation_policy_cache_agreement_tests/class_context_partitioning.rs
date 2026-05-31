//! Context-dependent class-check cache tests.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
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
