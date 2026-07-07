use super::*;
use crate::DefId;
use crate::caches::db::QueryDatabase;
use crate::intern::TypeInterner;
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, MappedType, ObjectFlags, ObjectShape, TupleElement, TypeParamInfo,
};
use tsz_common::interner::Atom;

#[test]
fn test_resolve_string_index() {
    let db = TypeInterner::new();

    // Object with string index
    let obj = db.object_with_index(ObjectShape {
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

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(obj), Some(TypeId::NUMBER));
    assert_eq!(resolver.resolve_number_index(obj), None);
}

#[test]
fn test_resolve_symbol_index_dedicated_slot() {
    let db = TypeInterner::new();

    let obj = db.object_with_index(ObjectShape {
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_symbol_index(obj), Some(TypeId::NUMBER));
    assert_eq!(resolver.resolve_string_index(obj), None);
}

#[test]
fn test_resolve_symbol_index_alongside_string() {
    let db = TypeInterner::new();

    let obj = db.object_with_index(ObjectShape {
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
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

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_symbol_index(obj), Some(TypeId::BOOLEAN));
    assert_eq!(resolver.resolve_string_index(obj), Some(TypeId::NUMBER));
}

#[test]
fn test_resolve_symbol_index_from_property_key_slot() {
    let db = TypeInterner::new();
    let property_key = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    let obj = db.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: property_key,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_symbol_index(obj), Some(TypeId::STRING));
    assert_eq!(resolver.resolve_string_index(obj), Some(TypeId::STRING));
}

#[test]
fn test_resolve_symbol_index_legacy_string_slot_encoding() {
    let db = TypeInterner::new();

    let obj = db.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_symbol_index(obj), Some(TypeId::STRING));
    assert_eq!(resolver.resolve_string_index(obj), None);
}

#[test]
fn test_resolve_symbol_index_none_for_plain_string() {
    let db = TypeInterner::new();

    let obj = db.object_with_index(ObjectShape {
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

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_symbol_index(obj), None);
    assert_eq!(resolver.resolve_string_index(obj), Some(TypeId::NUMBER));
}

#[test]
fn test_resolve_number_index() {
    let db = TypeInterner::new();

    // Object with number index
    let obj = db.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(obj), None);
    assert_eq!(resolver.resolve_number_index(obj), Some(TypeId::STRING));
}

fn mapped_record(db: &TypeInterner, key_type: TypeId, value_type: TypeId) -> TypeId {
    db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(key_type),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        },
        constraint: key_type,
        name_type: None,
        template: value_type,
        readonly_modifier: None,
        optional_modifier: None,
    })
}

fn object_with_index_slots(
    db: &TypeInterner,
    string_index: Option<(TypeId, bool)>,
    number_index: Option<(TypeId, bool)>,
    symbol_index: Option<(TypeId, bool)>,
) -> TypeId {
    let index_signature = |key_type, (value_type, readonly)| IndexSignature {
        key_type,
        value_type,
        readonly,
        param_name: None,
    };

    db.object_with_index(ObjectShape {
        symbol_index: symbol_index.map(|slot| index_signature(TypeId::SYMBOL, slot)),
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: string_index.map(|slot| index_signature(TypeId::STRING, slot)),
        number_index: number_index.map(|slot| index_signature(TypeId::NUMBER, slot)),
    })
}

fn assert_index_info_matches(actual: &IndexInfo, expected: &IndexInfo) {
    fn assert_signature_matches(actual: Option<IndexSignature>, expected: Option<IndexSignature>) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => {
                assert_eq!(actual.key_type, expected.key_type);
                assert_eq!(actual.value_type, expected.value_type);
                assert_eq!(actual.readonly, expected.readonly);
                assert_eq!(actual.param_name, expected.param_name);
            }
            (None, None) => {}
            (actual, expected) => {
                panic!("index signature mismatch: actual {actual:?}, expected {expected:?}")
            }
        }
    }

    assert_signature_matches(actual.string_index, expected.string_index);
    assert_signature_matches(actual.number_index, expected.number_index);
    assert_signature_matches(actual.symbol_index, expected.symbol_index);
}

#[test]
fn test_resolve_string_index_from_mapped_record() {
    let db = TypeInterner::new();
    let record = mapped_record(&db, TypeId::STRING, TypeId::UNKNOWN);

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(
        resolver.resolve_string_index(record),
        Some(TypeId::UNKNOWN),
        "Record<string, unknown>-shaped mapped types should expose their string index"
    );
    assert_eq!(resolver.resolve_number_index(record), None);
}

#[test]
fn test_resolve_number_index_from_mapped_record() {
    let db = TypeInterner::new();
    let record = mapped_record(&db, TypeId::NUMBER, TypeId::STRING);

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(record), None);
    assert_eq!(
        resolver.resolve_number_index(record),
        Some(TypeId::STRING),
        "Record<number, string>-shaped mapped types should expose their number index"
    );
}

#[test]
fn test_resolve_string_index_from_application_mapped_record() {
    struct AliasResolver {
        def_id: DefId,
        body: TypeId,
        params: Vec<TypeParamInfo>,
    }

    impl TypeResolver for AliasResolver {
        fn resolve_ref(
            &self,
            _symbol: crate::types::SymbolRef,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(
            &self,
            def_id: DefId,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            (def_id == self.def_id).then_some(self.body)
        }

        fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
            (def_id == self.def_id).then(|| self.params.clone())
        }
    }

    let db = TypeInterner::new();
    let key_param = TypeParamInfo {
        name: db.intern_string("Key"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let value_param = TypeParamInfo {
        name: db.intern_string("Value"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let key_type = db.type_param(key_param);
    let value_type = db.type_param(value_param);
    let body = mapped_record(&db, key_type, value_type);
    let def_id = DefId(94);
    let alias = db.lazy(def_id);
    let applied_record = db.application(alias, vec![TypeId::STRING, TypeId::UNKNOWN]);
    let alias_resolver = AliasResolver {
        def_id,
        body,
        params: vec![key_param, value_param],
    };

    let resolver = IndexSignatureResolver::with_resolver(&db, &alias_resolver);
    assert_eq!(
        resolver.resolve_string_index(applied_record),
        Some(TypeId::UNKNOWN),
        "application-wrapped Record<string, unknown> aliases should expose their string index"
    );
    assert_eq!(resolver.resolve_number_index(applied_record), None);
}

#[test]
fn test_resolve_symbol_index_from_application_mapped_record() {
    struct AliasResolver {
        def_id: DefId,
        body: TypeId,
        params: Vec<TypeParamInfo>,
    }

    impl TypeResolver for AliasResolver {
        fn resolve_ref(
            &self,
            _symbol: crate::types::SymbolRef,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(
            &self,
            def_id: DefId,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            (def_id == self.def_id).then_some(self.body)
        }

        fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
            (def_id == self.def_id).then(|| self.params.clone())
        }
    }

    let db = TypeInterner::new();
    let key_param = TypeParamInfo {
        name: db.intern_string("SymbolKey"),
        constraint: Some(TypeId::SYMBOL),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let value_param = TypeParamInfo {
        name: db.intern_string("Value"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let key_type = db.type_param(key_param);
    let value_type = db.type_param(value_param);
    let body = mapped_record(&db, key_type, value_type);
    let def_id = DefId(95);
    let alias = db.lazy(def_id);
    let applied_record = db.application(alias, vec![TypeId::SYMBOL, TypeId::BOOLEAN]);
    let alias_resolver = AliasResolver {
        def_id,
        body,
        params: vec![key_param, value_param],
    };

    let resolver = IndexSignatureResolver::with_resolver(&db, &alias_resolver);
    assert_eq!(
        resolver.resolve_symbol_index(applied_record),
        Some(TypeId::BOOLEAN),
        "application-wrapped Record<symbol, boolean> aliases should expose their symbol index"
    );
    assert_eq!(resolver.resolve_string_index(applied_record), None);
}

#[test]
fn test_is_readonly() {
    let db = TypeInterner::new();

    // Readonly string index
    let obj1 = db.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
    });

    // Mutable string index
    let obj2 = db.object_with_index(ObjectShape {
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

    let resolver = IndexSignatureResolver::new(&db);
    assert!(resolver.is_readonly(obj1, IndexKind::String));
    assert!(!resolver.is_readonly(obj2, IndexKind::String));
}

#[test]
fn test_is_numeric_index_name() {
    let db = TypeInterner::new();
    let resolver = IndexSignatureResolver::new(&db);

    assert!(resolver.is_numeric_index_name("0"));
    assert!(resolver.is_numeric_index_name("42"));
    assert!(resolver.is_numeric_index_name("123"));
    assert!(!resolver.is_numeric_index_name("foo"));
    assert!(!resolver.is_numeric_index_name(""));
    assert!(!resolver.is_numeric_index_name("-1")); // Starts with minus
}

/// TS7017 vs TS7053 distinction: Object without index signatures should report
/// `has_index_signature` = false for both kinds (triggers TS7017 in checker).
#[test]
fn test_has_index_signature_plain_object() {
    use crate::types::PropertyInfo;

    let db = TypeInterner::new();
    let atom = db.intern_string("prop");
    let obj = db.object(vec![PropertyInfo {
        name: atom,
        type_id: TypeId::STRING,
        ..PropertyInfo::default()
    }]);

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        !resolver.has_index_signature(obj, IndexKind::String),
        "plain object should have no string index signature"
    );
    assert!(
        !resolver.has_index_signature(obj, IndexKind::Number),
        "plain object should have no number index signature"
    );
}

/// `ObjectWithIndex` that has a string index signature should report true for
/// string and false for number (triggers TS7053 in checker for mismatched index type).
#[test]
fn test_has_index_signature_with_string_index() {
    let db = TypeInterner::new();
    let obj = db.object_with_index(ObjectShape {
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

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        resolver.has_index_signature(obj, IndexKind::String),
        "object with string index should report has_index_signature(String) = true"
    );
    assert!(
        !resolver.has_index_signature(obj, IndexKind::Number),
        "object with only string index should report has_index_signature(Number) = false"
    );
}

/// `ObjectWithIndex` that has both string and number index signatures should
/// report true for both kinds.
#[test]
fn test_has_index_signature_with_both_indexes() {
    let db = TypeInterner::new();
    let obj = db.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert!(resolver.has_index_signature(obj, IndexKind::String));
    assert!(resolver.has_index_signature(obj, IndexKind::Number));
}

/// Callable types (class constructors) with static index signatures should
/// resolve string and number index signatures correctly.
#[test]
fn test_callable_string_index_resolution() {
    let db = TypeInterner::new();
    let callable = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(
        resolver.resolve_string_index(callable),
        Some(TypeId::NUMBER),
        "callable with string index should resolve string index"
    );
    assert_eq!(
        resolver.resolve_number_index(callable),
        None,
        "callable with only string index should not resolve number index"
    );
}

#[test]
fn test_callable_number_index_resolution() {
    let db = TypeInterner::new();
    let callable = db.callable(CallableShape {
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(
        resolver.resolve_string_index(callable),
        None,
        "callable with only number index should not resolve string index"
    );
    assert_eq!(
        resolver.resolve_number_index(callable),
        Some(TypeId::STRING),
        "callable with number index should resolve number index"
    );
}

#[test]
fn test_callable_readonly_index_signatures() {
    let db = TypeInterner::new();

    let callable_readonly = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let callable_mutable = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        resolver.is_readonly(callable_readonly, IndexKind::String),
        "readonly string index on callable should be detected"
    );
    assert!(
        resolver.is_readonly(callable_readonly, IndexKind::Number),
        "readonly number index on callable should be detected"
    );
    assert!(
        !resolver.is_readonly(callable_mutable, IndexKind::String),
        "mutable string index on callable should not be readonly"
    );
    assert!(
        !resolver.is_readonly(callable_mutable, IndexKind::Number),
        "mutable number index on callable should not be readonly"
    );
}

#[test]
fn test_callable_index_info_collection() {
    let db = TypeInterner::new();
    let callable = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    let info = resolver.get_index_info(callable);
    assert!(info.string_index.is_some(), "should have string index");
    assert!(info.number_index.is_some(), "should have number index");
    assert_eq!(
        info.string_index.as_ref().unwrap().value_type,
        TypeId::NUMBER
    );
    assert_eq!(
        info.number_index.as_ref().unwrap().value_type,
        TypeId::STRING
    );
    assert!(info.string_index.as_ref().unwrap().readonly);
    assert!(!info.number_index.as_ref().unwrap().readonly);
}

#[test]
fn test_object_index_info_collection_includes_symbol_index() {
    let db = TypeInterner::new();
    let obj = db.object_with_index(ObjectShape {
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::BOOLEAN,
            readonly: true,
            param_name: None,
        }),
        symbol: None,
        flags: ObjectFlags::empty(),
    });

    let resolver = IndexSignatureResolver::new(&db);
    let info = resolver.get_index_info(obj);
    let string_index = info.string_index.expect("string index should be present");
    let symbol_index = info.symbol_index.expect("symbol index should be present");
    assert_eq!(string_index.value_type, TypeId::NUMBER);
    assert_eq!(symbol_index.value_type, TypeId::BOOLEAN);
    assert_eq!(symbol_index.key_type, TypeId::SYMBOL);
    assert!(symbol_index.readonly);
}

#[test]
fn test_union_index_info_requires_every_member_and_unions_values() {
    let db = TypeInterner::new();
    let left = object_with_index_slots(
        &db,
        Some((TypeId::NUMBER, false)),
        Some((TypeId::STRING, true)),
        Some((TypeId::BOOLEAN, false)),
    );
    let right = object_with_index_slots(
        &db,
        Some((TypeId::STRING, true)),
        Some((TypeId::BOOLEAN, true)),
        Some((TypeId::NUMBER, true)),
    );
    let union = db.union(vec![left, right]);
    let expected_string = db.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let expected_number = db.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    let expected_symbol = db.union(vec![TypeId::BOOLEAN, TypeId::NUMBER]);

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(union), Some(expected_string));
    assert_eq!(resolver.resolve_number_index(union), Some(expected_number));
    assert_eq!(resolver.resolve_symbol_index(union), Some(expected_symbol));
    assert!(resolver.has_index_signature(union, IndexKind::String));
    assert!(resolver.has_index_signature(union, IndexKind::Number));
    assert!(
        !resolver.is_readonly(union, IndexKind::String),
        "a union index signature is readonly only when every contributor is readonly"
    );
    assert!(resolver.is_readonly(union, IndexKind::Number));

    let info = resolver.get_index_info(union);
    let string_index = info.string_index.expect("string index should be present");
    let number_index = info.number_index.expect("number index should be present");
    let symbol_index = info.symbol_index.expect("symbol index should be present");
    assert_eq!(string_index.value_type, expected_string);
    assert_eq!(number_index.value_type, expected_number);
    assert_eq!(symbol_index.value_type, expected_symbol);
    assert_eq!(symbol_index.key_type, TypeId::SYMBOL);
    assert!(!string_index.readonly);
    assert!(number_index.readonly);
    assert!(!symbol_index.readonly);
}

#[test]
fn test_union_index_info_drops_slots_missing_from_any_member() {
    let db = TypeInterner::new();
    let indexed = object_with_index_slots(
        &db,
        Some((TypeId::NUMBER, false)),
        None,
        Some((TypeId::BOOLEAN, false)),
    );
    let plain = db.object(vec![]);
    let union = db.union(vec![indexed, plain]);

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(union), None);
    assert_eq!(resolver.resolve_number_index(union), None);
    assert_eq!(resolver.resolve_symbol_index(union), None);
    assert!(!resolver.has_index_signature(union, IndexKind::String));
    assert!(!resolver.has_index_signature(union, IndexKind::Number));
    assert!(!resolver.is_readonly(union, IndexKind::String));

    let info = resolver.get_index_info(union);
    assert_eq!(info, IndexInfo::default());
}

#[test]
fn test_intersection_index_info_intersects_values_per_key() {
    let db = TypeInterner::new();
    let left = object_with_index_slots(
        &db,
        Some((TypeId::NUMBER, false)),
        Some((TypeId::STRING, false)),
        None,
    );
    let middle = db.object(vec![]);
    let right = object_with_index_slots(
        &db,
        Some((TypeId::BOOLEAN, true)),
        None,
        Some((TypeId::NUMBER, true)),
    );
    let intersection = db.intersect_types_raw(vec![left, middle, right]);
    let expected_string = db.intersect_types_raw2(TypeId::NUMBER, TypeId::BOOLEAN);

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(
        resolver.resolve_string_index(intersection),
        Some(expected_string)
    );
    assert_eq!(
        resolver.resolve_number_index(intersection),
        Some(TypeId::STRING)
    );
    assert_eq!(
        resolver.resolve_symbol_index(intersection),
        Some(TypeId::NUMBER)
    );
    assert!(!resolver.is_readonly(intersection, IndexKind::String));
    assert!(!resolver.is_readonly(intersection, IndexKind::Number));

    let info = resolver.get_index_info(intersection);
    let string_index = info.string_index.expect("string index should be present");
    let number_index = info.number_index.expect("number index should be present");
    let symbol_index = info.symbol_index.expect("symbol index should be present");
    assert_eq!(string_index.value_type, expected_string);
    assert_eq!(number_index.value_type, TypeId::STRING);
    assert_eq!(symbol_index.value_type, TypeId::NUMBER);
    assert!(!string_index.readonly);
    assert!(!number_index.readonly);
    assert!(symbol_index.readonly);
}

#[test]
fn test_resolver_get_index_info_matches_type_database_for_composites() {
    let db = TypeInterner::new();
    let first = object_with_index_slots(
        &db,
        Some((TypeId::NUMBER, false)),
        Some((TypeId::STRING, true)),
        Some((TypeId::BOOLEAN, true)),
    );
    let second = object_with_index_slots(
        &db,
        Some((TypeId::STRING, true)),
        Some((TypeId::BOOLEAN, true)),
        Some((TypeId::NUMBER, false)),
    );
    let plain = db.object(vec![]);
    let union = db.union(vec![first, second]);
    let partial_union = db.union(vec![first, plain]);
    let intersection = db.intersect_types_raw(vec![first, plain, second]);
    let resolver = IndexSignatureResolver::new(&db);

    for composite in [union, partial_union, intersection] {
        assert_index_info_matches(
            &resolver.get_index_info(composite),
            &db.get_index_signatures(composite),
        );
    }
}

/// ReadonlyType(Tuple) should have a readonly number index signature.
/// This is the fix for `readonly [T, U, ...V[]]` types where computed
/// index access (e.g., `v[0+1] = 1`) should emit TS2542.
#[test]
fn test_readonly_tuple_has_readonly_number_index() {
    let db = TypeInterner::new();

    // Create a mutable tuple [number, number]
    let tuple = db.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Mutable tuple should NOT have a readonly number index
    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        !resolver.is_readonly(tuple, IndexKind::Number),
        "mutable tuple should not have readonly number index"
    );

    // Wrap in ReadonlyType — should now have a readonly number index
    let readonly_tuple = db.readonly_type(tuple);
    assert!(
        resolver.is_readonly(readonly_tuple, IndexKind::Number),
        "readonly tuple should have readonly number index"
    );

    // String index should not be affected
    assert!(
        !resolver.is_readonly(readonly_tuple, IndexKind::String),
        "readonly tuple should not have readonly string index"
    );
}

/// ReadonlyType(Array) should have a readonly number index signature.
#[test]
fn test_readonly_array_has_readonly_number_index() {
    let db = TypeInterner::new();

    let array = db.array(TypeId::NUMBER);
    let readonly_array = db.readonly_type(array);

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        !resolver.is_readonly(array, IndexKind::Number),
        "mutable array should not have readonly number index"
    );
    assert!(
        resolver.is_readonly(readonly_array, IndexKind::Number),
        "readonly array should have readonly number index"
    );
}

/// When an object has both string and number index signatures, property access
/// for a numeric key name (e.g., "0") should prefer the number index signature.
/// This matches tsc behavior: `obj["0"]` on `{ [n: number]: number, [s: string]: string | number }`
/// resolves to `number`, not `string | number`.
#[test]
fn test_numeric_key_prefers_number_index_over_string_index() {
    use crate::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};

    let db = TypeInterner::new();

    // { [n: number]: number, [s: string]: string | number }
    let string_or_number = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj = db.object_with_index(ObjectShape {
        symbol_index: None,
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: string_or_number,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let evaluator = PropertyAccessEvaluator::new(&db);

    // Access with numeric key "0" should use number index → number
    let result = evaluator.resolve_property_access(obj, "0");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            assert_eq!(
                type_id,
                TypeId::NUMBER,
                "numeric key '0' should resolve via number index to NUMBER, not string index"
            );
        }
        other => panic!("expected Success, got {other:?}"),
    }

    // Access with non-numeric key "foo" should use string index → string | number
    let result = evaluator.resolve_property_access(obj, "foo");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            assert_eq!(
                type_id, string_or_number,
                "non-numeric key 'foo' should resolve via string index"
            );
        }
        other => panic!("expected Success, got {other:?}"),
    }
}
