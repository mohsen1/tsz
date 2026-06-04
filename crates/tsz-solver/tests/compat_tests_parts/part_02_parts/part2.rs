#[test]
fn test_private_brand_lazy_cycle_does_not_recurse() {
    struct CyclicLazyResolver {
        first_def: DefId,
        first_type: TypeId,
        second_def: DefId,
        second_type: TypeId,
    }

    impl TypeResolver for CyclicLazyResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            if def_id == self.first_def {
                Some(self.second_type)
            } else if def_id == self.second_def {
                Some(self.first_type)
            } else {
                None
            }
        }
    }

    let interner = TypeInterner::new();
    let first_def = DefId(42);
    let second_def = DefId(43);
    let first_type = interner.intern(TypeData::Lazy(first_def));
    let second_type = interner.intern(TypeData::Lazy(second_def));
    let resolver = CyclicLazyResolver {
        first_def,
        first_type,
        second_def,
        second_type,
    };
    let checker = CompatChecker::with_resolver(&interner, &resolver);
    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    assert_eq!(
        checker.private_brand_assignability_override(first_type, target),
        None
    );
}

#[test]
fn test_private_brand_same_brand_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Two types with the same private brand should be assignable
    let brand = interner.intern_string("__private_brand_Foo");
    let source = interner.object(vec![PropertyInfo::new(brand, TypeId::NEVER)]);
    let target = interner.object(vec![PropertyInfo::new(brand, TypeId::NEVER)]);

    // Same brand = same class declaration = assignable
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_different_brand_not_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Two types with different private brands should NOT be assignable
    let brand1 = interner.intern_string("__private_brand_Foo");
    let brand2 = interner.intern_string("__private_brand_Bar");

    let source = interner.object(vec![PropertyInfo::new(brand1, TypeId::NEVER)]);
    let target = interner.object(vec![PropertyInfo::new(brand2, TypeId::NEVER)]);

    // Different brands = different class declarations = not assignable
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_source_without_brand_not_assignable_to_target_with_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Source without brand cannot satisfy target's private requirements
    let brand = interner.intern_string("__private_brand_Foo");
    let name = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo {
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
    let target = interner.object(vec![
        PropertyInfo::new(brand, TypeId::NEVER),
        PropertyInfo {
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
        },
    ]);

    // Source without brand cannot be assigned to target with brand
    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_source_with_brand_assignable_to_target_without_brand() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Source with brand CAN be assigned to target without brand (e.g., interface)
    let brand = interner.intern_string("__private_brand_Foo");
    let name = interner.intern_string("value");

    let source = interner.object(vec![
        PropertyInfo::new(brand, TypeId::NEVER),
        PropertyInfo {
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
        },
    ]);
    let target = interner.object(vec![PropertyInfo {
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

    // A class can implement an interface (source with brand -> target without brand)
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_private_brand_neither_has_brand_falls_through() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // When neither has a brand, fall through to structural checking
    let name = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo {
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
    let target = interner.object(vec![PropertyInfo {
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

    // Structural check passes
    assert!(checker.is_assignable(source, target));
}
