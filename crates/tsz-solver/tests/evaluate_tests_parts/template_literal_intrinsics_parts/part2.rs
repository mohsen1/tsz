/// String literal with special characters
#[test]
fn test_string_literal_special_chars() {
    let interner = TypeInterner::new();

    let special = interner.literal_string("hello\nworld");
    let pattern = interner.literal_string("hello\nworld");

    let cond = ConditionalType {
        check_type: special,
        extends_type: pattern,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Mapped type with Uppercase key remapping
#[test]
fn test_mapped_type_uppercase_keys() {
    let interner = TypeInterner::new();

    // { [K in "a" | "b" as Uppercase<K>]: number }
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_upper_a = interner.literal_string("A");
    let key_upper_b = interner.literal_string("B");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Map "a" -> "A", "b" -> "B" via nested conditionals
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_b,
        true_type: key_upper_b,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: key_upper_a,
        false_type: inner_cond,
        is_distributive: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { A: number; B: number }
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let expected = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::NUMBER),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    assert_eq!(result, expected);
}

/// Mapped type with template literal key transformation
#[test]
fn test_mapped_type_template_literal_keys() {
    let interner = TypeInterner::new();

    // { [K in "click" | "focus" as `on${K}`]: EventHandler }
    let key_click = interner.literal_string("click");
    let key_focus = interner.literal_string("focus");
    let keys = interner.union(vec![key_click, key_focus]);

    let on_click = interner.literal_string("onclick");
    let on_focus = interner.literal_string("onfocus");

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Map via nested conditionals
    let inner_cond = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_focus,
        true_type: on_focus,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_click,
        true_type: on_click,
        false_type: inner_cond,
        is_distributive: false,
    });

    // Event handler function type
    let handler = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: handler,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { onclick: () => void; onfocus: () => void }
    let onclick_name = interner.intern_string("onclick");
    let onfocus_name = interner.intern_string("onfocus");
    let expected = interner.object(vec![
        PropertyInfo::new(onclick_name, handler),
        PropertyInfo::new(onfocus_name, handler),
    ]);

    assert_eq!(result, expected);
}

#[test]
fn test_satisfies_basic_literal_string() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = "hello" satisfies string
    // The literal type "hello" should satisfy the string constraint
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    // "hello" satisfies string - should be true
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    // The inferred type remains "hello", not string
    assert_ne!(hello, TypeId::STRING);
}

#[test]
fn test_satisfies_basic_literal_number() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = 42 satisfies number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let forty_two = interner.literal_number(42.0);
    // 42 satisfies number - should be true
    assert!(checker.is_subtype_of(forty_two, TypeId::NUMBER));
    // The inferred type remains 42, not number
    assert_ne!(forty_two, TypeId::NUMBER);
}

#[test]
fn test_satisfies_basic_object_type() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = { a: 1, b: "hello" } satisfies { a: number, b: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let hello = interner.literal_string("hello");

    // Object with literal types (inferred type)
    let inferred = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), one),
        PropertyInfo::new(interner.intern_string("b"), hello),
    ]);

    // Constraint type (wider)
    let constraint = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    // Inferred type satisfies constraint
    assert!(checker.is_subtype_of(inferred, constraint));
    // Types are different (inferred has literal types)
    assert_ne!(inferred, constraint);
}

#[test]
fn test_satisfies_constraint_failure() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = "hello" satisfies number - should fail
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    // String literal does not satisfy number constraint
    assert!(!checker.is_subtype_of(hello, TypeId::NUMBER));
}
