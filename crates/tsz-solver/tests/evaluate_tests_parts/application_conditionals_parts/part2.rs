/// Test Application expansion with array element type in body.
///
/// `type ArrayOf<T> = T[]` with `ArrayOf<string>` should expand to `string[]`
#[test]
fn test_application_ref_expansion_with_array_body() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type ArrayOf<T> = T[]
    let array_body = interner.array(t_type);

    // Create Ref(1) for ArrayOf type alias
    let array_of_ref = interner.lazy(DefId(1));

    // Create Application: ArrayOf<string>
    let array_of_string = interner.application(array_of_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), array_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(array_of_string);

    // Expected: string[]
    let expected = interner.array(TypeId::STRING);

    assert_eq!(
        result, expected,
        "ArrayOf<string> should expand to string[]"
    );
}

/// Test Application expansion with readonly property in body.
///
/// `type ReadonlyBox<T> = { readonly value: T }` with `ReadonlyBox<number>`
/// should expand to `{ readonly value: number }`
#[test]
fn test_application_ref_expansion_with_readonly_property() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type ReadonlyBox<T> = { readonly value: T }
    let value_name = interner.intern_string("value");
    let readonly_box_body = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: true, // readonly modifier
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Create Ref(1) for ReadonlyBox type alias
    let readonly_box_ref = interner.lazy(DefId(1));

    // Create Application: ReadonlyBox<number>
    let readonly_box_number = interner.application(readonly_box_ref, vec![TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), readonly_box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(readonly_box_number);

    // Expected: { readonly value: number }
    let expected = interner.object(vec![PropertyInfo::readonly(value_name, TypeId::NUMBER)]);

    assert_eq!(
        result, expected,
        "ReadonlyBox<number> should expand to {{ readonly value: number }}"
    );
}

/// Test Application expansion with optional property in body.
///
/// `type OptionalBox<T> = { value?: T }` with `OptionalBox<string>`
/// should expand to `{ value?: string }`
#[test]
fn test_application_ref_expansion_with_optional_property() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define: type OptionalBox<T> = { value?: T }
    let value_name = interner.intern_string("value");
    let optional_box_body = interner.object(vec![PropertyInfo {
        name: value_name,
        type_id: t_type,
        write_type: t_type,
        optional: true, // optional modifier
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

    // Create Ref(1) for OptionalBox type alias
    let optional_box_ref = interner.lazy(DefId(1));

    // Create Application: OptionalBox<string>
    let optional_box_string = interner.application(optional_box_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), optional_box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(optional_box_string);

    // Expected: { value?: string }
    let expected = interner.object(vec![PropertyInfo::opt(value_name, TypeId::STRING)]);

    assert_eq!(
        result, expected,
        "OptionalBox<string> should expand to {{ value?: string }}"
    );
}

/// Test Application expansion with method in body.
///
/// `type WithMethod<T> = { get(): T }` with `WithMethod<boolean>`
/// should expand to `{ get(): boolean }`
#[test]
fn test_application_ref_expansion_with_method() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameter T
    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // Define method type: () => T
    let method_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Define: type WithMethod<T> = { get(): T }
    let get_name = interner.intern_string("get");
    let with_method_body = interner.object(vec![PropertyInfo::method(get_name, method_type)]);

    // Create Ref(1) for WithMethod type alias
    let with_method_ref = interner.lazy(DefId(1));

    // Create Application: WithMethod<boolean>
    let with_method_boolean = interner.application(with_method_ref, vec![TypeId::BOOLEAN]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), with_method_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(with_method_boolean);

    // Expected method type: () => boolean
    let expected_method_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Expected: { get(): boolean }
    let expected = interner.object(vec![PropertyInfo::method(get_name, expected_method_type)]);

    assert_eq!(
        result, expected,
        "WithMethod<boolean> should expand to {{ get(): boolean }}"
    );
}
