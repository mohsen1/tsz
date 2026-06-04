#[test]
fn test_mapped_type_optional_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: Some(MappedModifier::Remove),
    });

    let name_a = interner.intern_string("a");
    let mutable_required_target = interner.object(vec![PropertyInfo::new(name_a, TypeId::NUMBER)]);
    let readonly_optional_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
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

    assert!(checker.is_subtype_of(mapped, mutable_required_target));
    assert!(checker.is_subtype_of(mapped, readonly_optional_target));
    assert!(!checker.is_subtype_of(readonly_optional_target, mapped));
}

#[test]
fn test_mapped_type_optional_modifier_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let name_a = interner.intern_string("a");
    let required_target = interner.object(vec![PropertyInfo::new(name_a, TypeId::NUMBER)]);
    let optional_target = interner.object(vec![PropertyInfo::opt(name_a, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, required_target));
    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(optional_target, mapped));
}

#[test]
fn test_mapped_type_optional_remove_from_optional_keyof() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let source_obj = interner.object(vec![PropertyInfo::opt(key_a, TypeId::NUMBER)]);
    let keys = interner.intern(TypeData::KeyOf(source_obj));

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_target = interner.object(vec![PropertyInfo::new(key_a, TypeId::STRING)]);
    let optional_target = interner.object(vec![PropertyInfo::opt(key_a, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, required_target));
    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(optional_target, mapped));
}

#[test]
fn test_mapped_type_readonly_remove_from_readonly_keyof() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let source_obj = interner.object(vec![PropertyInfo::readonly(key_a, TypeId::STRING)]);
    let keys = interner.intern(TypeData::KeyOf(source_obj));

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let mutable_target = interner.object(vec![PropertyInfo::new(key_a, TypeId::NUMBER)]);
    let readonly_target = interner.object(vec![PropertyInfo::readonly(key_a, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, mutable_target));
    assert!(checker.is_subtype_of(mapped, readonly_target));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(readonly_target, mapped));
}

#[test]
fn test_mapped_type_readonly_modifier_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let name_a = interner.intern_string("a");
    let mutable_target = interner.object(vec![PropertyInfo::new(name_a, TypeId::NUMBER)]);
    let readonly_target = interner.object(vec![PropertyInfo::readonly(name_a, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, mutable_target));
    assert!(checker.is_subtype_of(mapped, readonly_target));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(readonly_target, mapped));
}

#[test]
fn test_mapped_type_key_remap_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let expected = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let requires_a = interner.object(vec![PropertyInfo::new(prop_a.name, TypeId::STRING)]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, requires_a));
}

#[test]
fn test_mapped_type_key_remap_optional_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    let optional_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);
    let required_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, optional_b));
    assert!(!checker.is_subtype_of(mapped, required_b));
}

#[test]
fn test_mapped_type_key_remap_optional_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::opt(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let optional_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, required_b));
    assert!(checker.is_subtype_of(mapped, optional_b));
}
