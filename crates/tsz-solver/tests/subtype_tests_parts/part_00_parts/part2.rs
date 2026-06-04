#[test]
fn test_readonly_property_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name = interner.intern_string("x");
    let readonly_obj = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    let mutable_obj = interner.object(vec![PropertyInfo {
        name,
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
    }]);

    // TypeScript allows readonly property → mutable property assignment
    assert!(checker.is_subtype_of(readonly_obj, mutable_obj));
    assert!(checker.is_subtype_of(mutable_obj, readonly_obj));
}

#[test]
fn test_readonly_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let mutable_array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(mutable_array));

    assert!(checker.is_subtype_of(mutable_array, readonly_array));
    assert!(!checker.is_subtype_of(readonly_array, mutable_array));
}

struct ReadonlyArrayDefResolver {
    def_id: DefId,
}

impl TypeResolver for ReadonlyArrayDefResolver {
    fn resolve_ref(
        &self,
        _symbol: SymbolRef,
        _interner: &dyn crate::construction::TypeDatabase,
    ) -> Option<TypeId> {
        None
    }

    fn is_builtin_readonly_array_def(&self, def_id: DefId) -> bool {
        def_id == self.def_id
    }
}

#[test]
fn test_readonly_array_application_matches_readonly_array_syntax() {
    let interner = TypeInterner::new();
    let resolver = ReadonlyArrayDefResolver { def_id: DefId(1) };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let readonly_array_def = interner.lazy(DefId(1));
    let readonly_array_app = interner.application(readonly_array_def, vec![TypeId::STRING]);
    let readonly_string_array = interner.readonly_type(interner.array(TypeId::STRING));
    let readonly_number_array = interner.readonly_type(interner.array(TypeId::NUMBER));
    let app_or_null = interner.union(vec![readonly_array_app, TypeId::NULL]);
    let syntax_or_null = interner.union(vec![readonly_string_array, TypeId::NULL]);

    assert!(checker.is_subtype_of(readonly_array_app, readonly_string_array));
    assert!(checker.is_subtype_of(readonly_string_array, readonly_array_app));
    assert!(checker.is_subtype_of(app_or_null, syntax_or_null));
    assert!(!checker.is_subtype_of(readonly_array_app, readonly_number_array));

    let shadow_resolver = ReadonlyArrayDefResolver { def_id: DefId(99) };
    let mut shadow_checker = SubtypeChecker::with_resolver(&interner, &shadow_resolver);
    assert!(!shadow_checker.is_subtype_of(readonly_array_app, readonly_string_array));
}

#[test]
fn test_readonly_tuple_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    assert!(checker.is_subtype_of(tuple, readonly_tuple));
    assert!(!checker.is_subtype_of(readonly_tuple, tuple));
}

#[test]
fn test_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let any_array = interner.array(TypeId::ANY);

    // Same element type
    assert!(checker.is_subtype_of(string_array, string_array));

    // Different element type
    assert!(!checker.is_subtype_of(string_array, number_array));

    // Covariance with any
    assert!(checker.is_subtype_of(string_array, any_array));
}

#[test]
fn test_array_to_iterable_protocol_subtyping() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);
    let mut checker = SubtypeChecker::with_resolver(&interner, &cache).with_query_db(&cache);

    let array_length = interner.intern_string("length");
    let array_base = interner.object(vec![PropertyInfo::readonly(array_length, TypeId::NUMBER)]);
    interner.set_array_base_type(array_base, vec![]);

    let iterator_name = interner.intern_string("[Symbol.iterator]");
    let next_name = interner.intern_string("next");
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iterator_result_type = |value_ty| {
        interner.object(vec![
            PropertyInfo::new(value_name, value_ty),
            PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
        ])
    };

    let iterator_type = |value_ty| {
        let next = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: iterator_result_type(value_ty),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        interner.object(vec![PropertyInfo::method(next_name, next)])
    };

    let iterable_of = |value_ty| {
        let iter_fn = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: iterator_type(value_ty),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        interner.object(vec![PropertyInfo::method(iterator_name, iter_fn)])
    };

    let iterable_number = iterable_of(TypeId::NUMBER);
    let iterable_string = iterable_of(TypeId::STRING);
    let iterator_info =
        crate::operations::iterators::get_iterator_info(&cache, iterable_number, false)
            .expect("iterable target should expose iterable info");
    assert_eq!(iterator_info.yield_type, TypeId::NUMBER);
    let source = interner.array(TypeId::NUMBER);

    assert!(!checker.is_subtype_of(array_base, iterable_number));
    let interface_result = checker
        .check_array_interface_subtype(TypeId::NUMBER, iterable_number)
        .expect("array interface check should apply");
    assert!(interface_result.is_true());
    assert!(checker.is_subtype_of(source, iterable_number));
    assert!(!checker.is_subtype_of(source, iterable_string));
}

#[test]
fn test_array_covariant_mutable_unsoundness() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(string_or_number);

    assert!(checker.is_subtype_of(string_array, union_array));
    assert!(!checker.is_subtype_of(union_array, string_array));
}

#[test]
fn test_type_environment() {
    let _interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Register some types
    let sym1 = SymbolRef(1);
    let sym2 = SymbolRef(2);
    env.insert(sym1, TypeId::STRING);
    env.insert(sym2, TypeId::NUMBER);

    // Check retrieval
    assert_eq!(env.get(sym1), Some(TypeId::STRING));
    assert_eq!(env.get(sym2), Some(TypeId::NUMBER));
    assert_eq!(env.get(SymbolRef(999)), None);

    // Check contains
    assert!(env.contains(sym1));
    assert!(!env.contains(SymbolRef(999)));
}

#[test]
fn test_ref_resolution_with_environment() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Create a Ref type for symbol 1
    let ref_type = interner.lazy(DefId(1));

    // Without resolution, Ref to anything should fail (no noop resolution)
    let mut checker = SubtypeChecker::new(&interner);
    // Ref to intrinsic - can't resolve, so falls back to false
    assert!(!checker.is_subtype_of(ref_type, TypeId::STRING));

    // Add resolution: symbol 1 = string
    env.insert_def(DefId(1), TypeId::STRING);

    // With environment, Ref(1) resolves to string
    let mut checker_with_env = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker_with_env.is_subtype_of(ref_type, TypeId::STRING));
    assert!(!checker_with_env.is_subtype_of(ref_type, TypeId::NUMBER));
}

#[test]
fn test_reference_lazy_fallback_uses_symbol_to_def_mapping() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Register a real DefId and map a raw SymbolId back to it.
    let real_def = DefId(100);
    env.insert_def(real_def, TypeId::STRING);
    env.register_def_symbol_mapping(real_def, SymbolId(5));

    let raw_reference = interner.reference(SymbolRef(5));

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(raw_reference, TypeId::STRING));
    assert!(!checker.is_subtype_of(raw_reference, TypeId::NUMBER));
}

#[test]
fn test_lazy_type_params_falls_back_from_symbol_based_lazy_ref() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let generic_def = DefId(200);
    env.insert_def_with_params(generic_def, TypeId::STRING, vec![t_param]);
    env.register_def_symbol_mapping(generic_def, SymbolId(42));

    let raw_lazy = env
        .get_lazy_type_params(DefId(42))
        .expect("fallback should resolve params");
    assert_eq!(raw_lazy.len(), 1);
    assert_eq!(raw_lazy[0], t_param);

    let symbol_reference = interner.reference(SymbolRef(42));
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(symbol_reference, TypeId::STRING));
}
