/// Test Application expansion with rest parameter in function body.
///
/// `type VarArgs<T> = (...args: T[]) => void` with `VarArgs<string>`
/// should expand to `(...args: string[]) => void`
#[test]
fn test_application_ref_expansion_with_rest_param() {
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

    // Define: type VarArgs<T> = (...args: T[]) => void
    let args_name = interner.intern_string("args");
    let t_array = interner.array(t_type);

    let varargs_body = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(args_name),
            type_id: t_array,
            optional: false,
            rest: true, // rest parameter
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create Ref(1) for VarArgs type alias
    let varargs_ref = interner.lazy(DefId(1));

    // Create Application: VarArgs<string>
    let varargs_string = interner.application(varargs_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), varargs_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(varargs_string);

    // Expected: (...args: string[]) => void
    let string_array = interner.array(TypeId::STRING);
    let expected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::rest(args_name, string_array)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert_eq!(
        result, expected,
        "VarArgs<string> should expand to (...args: string[]) => void"
    );
}

/// Test Application expansion with index signature in body.
///
/// `type Dict<T> = { [key: string]: T }` with `Dict<number>`
/// should expand to `{ [key: string]: number }`
#[test]
fn test_application_ref_expansion_with_index_signature() {
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

    // Define: type Dict<T> = { [key: string]: T }
    let dict_body = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Create Ref(1) for Dict type alias
    let dict_ref = interner.lazy(DefId(1));

    // Create Application: Dict<number>
    let dict_number = interner.application(dict_ref, vec![TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), dict_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(dict_number);

    // Expected: { [key: string]: number }
    let expected = interner.object_with_index(ObjectShape {
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

    assert_eq!(
        result, expected,
        "Dict<number> should expand to {{ [key: string]: number }}"
    );
}

/// Test Application expansion with number index signature in body.
///
/// `type NumericDict<T> = { [index: number]: T }` with `NumericDict<string>`
/// should expand to `{ [index: number]: string }`
#[test]
fn test_application_ref_expansion_with_number_index_signature() {
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

    // Define: type NumericDict<T> = { [index: number]: T }
    let numeric_dict_body = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
    });

    // Create Ref(1) for NumericDict type alias
    let numeric_dict_ref = interner.lazy(DefId(1));

    // Create Application: NumericDict<string>
    let numeric_dict_string = interner.application(numeric_dict_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), numeric_dict_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(numeric_dict_string);

    // Expected: { [index: number]: string }
    let expected = interner.object_with_index(ObjectShape {
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

    assert_eq!(
        result, expected,
        "NumericDict<string> should expand to {{ [index: number]: string }}"
    );
}

/// Test Application expansion with literal type argument.
///
/// `type Box<T> = { value: T }` with `Box<"hello">`
/// should expand to `{ value: "hello" }`
#[test]
fn test_application_ref_expansion_with_literal_arg() {
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

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create literal type "hello"
    let hello_literal = interner.literal_string("hello");

    // Create Application: Box<"hello">
    let box_hello = interner.application(box_ref, vec![hello_literal]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_hello);

    // Expected: { value: "hello" }
    let expected = interner.object(vec![PropertyInfo::new(value_name, hello_literal)]);

    assert_eq!(
        result, expected,
        "Box<\"hello\"> should expand to {{ value: \"hello\" }}"
    );
}

/// Test Application expansion with numeric literal type argument.
///
/// `type Box<T> = { value: T }` with `Box<42>`
/// should expand to `{ value: 42 }`
#[test]
fn test_application_ref_expansion_with_numeric_literal_arg() {
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

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create literal type 42
    let lit_42 = interner.literal_number(42.0);

    // Create Application: Box<42>
    let box_42 = interner.application(box_ref, vec![lit_42]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_42);

    // Expected: { value: 42 }
    let expected = interner.object(vec![PropertyInfo::new(value_name, lit_42)]);

    assert_eq!(result, expected, "Box<42> should expand to {{ value: 42 }}");
}

/// Test Application expansion with multiple properties referencing same type param.
///
/// `type Pair<T> = { first: T; second: T }` with `Pair<string>`
/// should expand to `{ first: string; second: string }`
#[test]
fn test_application_ref_expansion_with_multiple_refs_to_same_param() {
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

    // Define: type Pair<T> = { first: T; second: T }
    let first_name = interner.intern_string("first");
    let second_name = interner.intern_string("second");
    let pair_body = interner.object(vec![
        PropertyInfo::new(first_name, t_type),
        PropertyInfo::new(second_name, t_type),
    ]);

    // Create Ref(1) for Pair type alias
    let pair_ref = interner.lazy(DefId(1));

    // Create Application: Pair<string>
    let pair_string = interner.application(pair_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), pair_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(pair_string);

    // Expected: { first: string; second: string }
    let expected = interner.object(vec![
        PropertyInfo::new(first_name, TypeId::STRING),
        PropertyInfo::new(second_name, TypeId::STRING),
    ]);

    assert_eq!(
        result, expected,
        "Pair<string> should expand to {{ first: string; second: string }}"
    );
}

/// Test Application expansion with boolean literal type argument.
///
/// `type Box<T> = { value: T }` with `Box<true>`
/// should expand to `{ value: true }`
#[test]
fn test_application_ref_expansion_with_boolean_literal_arg() {
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

    // Define: type Box<T> = { value: T }
    let value_name = interner.intern_string("value");
    let box_body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Create Ref(1) for Box type alias
    let box_ref = interner.lazy(DefId(1));

    // Create literal type true
    let lit_true = interner.literal_boolean(true);

    // Create Application: Box<true>
    let box_true = interner.application(box_ref, vec![lit_true]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), box_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(box_true);

    // Expected: { value: true }
    let expected = interner.object(vec![PropertyInfo::new(value_name, lit_true)]);

    assert_eq!(
        result, expected,
        "Box<true> should expand to {{ value: true }}"
    );
}

/// Test Application expansion with union type in body.
///
/// `type Either<L, R> = L | R` with `Either<string, number>`
/// should expand to `string | number`
#[test]
fn test_application_ref_expansion_with_union_body() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameters L and R
    let l_name = interner.intern_string("L");
    let r_name = interner.intern_string("R");
    let l_param = TypeParamInfo {
        name: l_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let r_param = TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let l_type = interner.intern(TypeData::TypeParameter(l_param));
    let r_type = interner.intern(TypeData::TypeParameter(r_param));

    // Define: type Either<L, R> = L | R
    let either_body = interner.union(vec![l_type, r_type]);

    // Create Ref(1) for Either type alias
    let either_ref = interner.lazy(DefId(1));

    // Create Application: Either<string, number>
    let either_string_number =
        interner.application(either_ref, vec![TypeId::STRING, TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), either_body, vec![l_param, r_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(either_string_number);

    // Expected: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(
        result, expected,
        "Either<string, number> should expand to string | number"
    );
}

/// Test Application expansion with intersection type in body.
///
/// `type Both<A, B> = A & B` with `Both<{x: number}, {y: string}>`
/// should expand to `{x: number} & {y: string}`
#[test]
fn test_application_ref_expansion_with_intersection_body() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameters A and B
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let a_param = TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let b_param = TypeParamInfo {
        name: b_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let a_type = interner.intern(TypeData::TypeParameter(a_param));
    let b_type = interner.intern(TypeData::TypeParameter(b_param));

    // Define: type Both<A, B> = A & B
    let both_body = interner.intersection(vec![a_type, b_type]);

    // Create Ref(1) for Both type alias
    let both_ref = interner.lazy(DefId(1));

    // Create object types: {x: number} and {y: string}
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let obj_x = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);
    let obj_y = interner.object(vec![PropertyInfo::new(y_name, TypeId::STRING)]);

    // Create Application: Both<{x: number}, {y: string}>
    let both_xy = interner.application(both_ref, vec![obj_x, obj_y]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), both_body, vec![a_param, b_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(both_xy);

    // Expected: {x: number} & {y: string}
    let expected = interner.intersection(vec![obj_x, obj_y]);

    assert_eq!(
        result, expected,
        "Both<{{x: number}}, {{y: string}}> should expand to {{x: number}} & {{y: string}}"
    );
}

/// Test Application expansion with this-parameter in function body.
///
/// `type BoundMethod<T> = (this: T) => void` with `BoundMethod<{x: number}>`
/// should expand to `(this: {x: number}) => void`
#[test]
fn test_application_ref_expansion_with_this_param() {
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

    // Define: type BoundMethod<T> = (this: T) => void
    let bound_method_body = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(t_type), // this parameter
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create Ref(1) for BoundMethod type alias
    let bound_method_ref = interner.lazy(DefId(1));

    // Create object type: {x: number}
    let x_name = interner.intern_string("x");
    let obj_x = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    // Create Application: BoundMethod<{x: number}>
    let bound_method_obj = interner.application(bound_method_ref, vec![obj_x]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), bound_method_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(bound_method_obj);

    // Expected: (this: {x: number}) => void
    let expected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(obj_x),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert_eq!(
        result, expected,
        "BoundMethod<{{x: number}}> should expand to (this: {{x: number}}) => void"
    );
}

/// Test Application expansion with optional parameter in function body.
///
/// `type OptionalFn<T> = (x?: T) => T` with `OptionalFn<string>`
/// should expand to `(x?: string) => string`
#[test]
fn test_application_ref_expansion_with_optional_param() {
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

    // Define: type OptionalFn<T> = (x?: T) => T
    let x_name = interner.intern_string("x");
    let optional_fn_body = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(x_name),
            type_id: t_type,
            optional: true, // optional parameter
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create Ref(1) for OptionalFn type alias
    let optional_fn_ref = interner.lazy(DefId(1));

    // Create Application: OptionalFn<string>
    let optional_fn_string = interner.application(optional_fn_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), optional_fn_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(optional_fn_string);

    // Expected: (x?: string) => string
    let expected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::optional(x_name, TypeId::STRING)],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert_eq!(
        result, expected,
        "OptionalFn<string> should expand to (x?: string) => string"
    );
}

/// Test Application expansion with readonly array in body.
///
/// `type ReadonlyArray<T> = readonly T[]` with `ReadonlyArray<number>`
/// should expand to `readonly number[]`
#[test]
fn test_application_ref_expansion_with_readonly_array_body() {
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

    // Define: type ReadonlyArrayOf<T> = readonly T[]
    let t_array = interner.array(t_type);
    let readonly_array_body = interner.intern(TypeData::ReadonlyType(t_array));

    // Create Ref(1) for ReadonlyArrayOf type alias
    let readonly_array_ref = interner.lazy(DefId(1));

    // Create Application: ReadonlyArrayOf<number>
    let readonly_array_number = interner.application(readonly_array_ref, vec![TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), readonly_array_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(readonly_array_number);

    // Expected: readonly number[]
    let number_array = interner.array(TypeId::NUMBER);
    let expected = interner.intern(TypeData::ReadonlyType(number_array));

    assert_eq!(
        result, expected,
        "ReadonlyArrayOf<number> should expand to readonly number[]"
    );
}

/// Test Application expansion with mixed readonly and optional properties.
///
/// `type Config<T> = { readonly id: string; value?: T }` with `Config<number>`
/// should expand to `{ readonly id: string; value?: number }`
#[test]
fn test_application_ref_expansion_with_mixed_modifiers() {
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

    // Define: type Config<T> = { readonly id: string; value?: T }
    let id_name = interner.intern_string("id");
    let value_name = interner.intern_string("value");
    let config_body = interner.object(vec![
        PropertyInfo {
            name: id_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true, // readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: value_name,
            type_id: t_type,
            write_type: t_type,
            optional: true, // optional
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

    // Create Ref(1) for Config type alias
    let config_ref = interner.lazy(DefId(1));

    // Create Application: Config<number>
    let config_number = interner.application(config_ref, vec![TypeId::NUMBER]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), config_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(config_number);

    // Expected: { readonly id: string; value?: number }
    let expected = interner.object(vec![
        PropertyInfo::readonly(id_name, TypeId::STRING),
        PropertyInfo::opt(value_name, TypeId::NUMBER),
    ]);

    assert_eq!(
        result, expected,
        "Config<number> should expand to {{ readonly id: string; value?: number }}"
    );
}

/// Test Application expansion with callable type in body.
///
/// `type Callback<T, R> = { (arg: T): R }` with `Callback<string, boolean>`
/// should expand to `{ (arg: string): boolean }`
#[test]
fn test_application_ref_expansion_with_callable_body() {
    use crate::evaluation::evaluate::TypeEvaluator;
    use crate::relations::subtype::TypeEnvironment;

    let interner = TypeInterner::new();

    // Define type parameters T and R
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let r_param = TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let r_type = interner.intern(TypeData::TypeParameter(r_param));

    // Define: type Callback<T, R> = { (arg: T): R }
    let arg_name = interner.intern_string("arg");
    let call_sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo::required(arg_name, t_type)],
        this_type: None,
        return_type: r_type,
        type_predicate: None,
        is_method: false,
    };
    let callback_body = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![call_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    // Create Ref(1) for Callback type alias
    let callback_ref = interner.lazy(DefId(1));

    // Create Application: Callback<string, boolean>
    let callback_string_bool =
        interner.application(callback_ref, vec![TypeId::STRING, TypeId::BOOLEAN]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), callback_body, vec![t_param, r_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(callback_string_bool);

    // Expected: { (arg: string): boolean }
    let expected_call_sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo::required(arg_name, TypeId::STRING)],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_method: false,
    };
    let expected = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![expected_call_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert_eq!(
        result, expected,
        "Callback<string, boolean> should expand to {{ (arg: string): boolean }}"
    );
}

/// Test Application expansion with construct signature in body.
///
/// `type Constructor<T> = { new (): T }` with `Constructor<{x: number}>`
/// should expand to `{ new (): {x: number} }`
#[test]
fn test_application_ref_expansion_with_construct_signature() {
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

    // Define: type Constructor<T> = { new (): T }
    let construct_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_method: false,
    };
    let constructor_body = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![construct_sig],
        properties: vec![],
        ..Default::default()
    });

    // Create Ref(1) for Constructor type alias
    let constructor_ref = interner.lazy(DefId(1));

    // Create object type: {x: number}
    let x_name = interner.intern_string("x");
    let obj_x = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    // Create Application: Constructor<{x: number}>
    let constructor_obj = interner.application(constructor_ref, vec![obj_x]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), constructor_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(constructor_obj);

    // Expected: { new (): {x: number} }
    let expected_construct_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_x,
        type_predicate: None,
        is_method: false,
    };
    let expected = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![expected_construct_sig],
        properties: vec![],
        ..Default::default()
    });

    assert_eq!(
        result, expected,
        "Constructor<{{x: number}}> should expand to {{ new (): {{x: number}} }}"
    );
}

/// Test Application expansion with deeply nested type params.
///
/// `type Wrapper<T> = { inner: { value: T } }` with `Wrapper<string>`
/// should expand to `{ inner: { value: string } }`
#[test]
fn test_application_ref_expansion_with_deeply_nested_param() {
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

    // Define inner object: { value: T }
    let value_name = interner.intern_string("value");
    let inner_obj = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // Define: type Wrapper<T> = { inner: { value: T } }
    let inner_name = interner.intern_string("inner");
    let wrapper_body = interner.object(vec![PropertyInfo::new(inner_name, inner_obj)]);

    // Create Ref(1) for Wrapper type alias
    let wrapper_ref = interner.lazy(DefId(1));

    // Create Application: Wrapper<string>
    let wrapper_string = interner.application(wrapper_ref, vec![TypeId::STRING]);

    // Set up resolver with type parameters
    let mut env = TypeEnvironment::new();
    env.insert_def_with_params(DefId(1), wrapper_body, vec![t_param]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(wrapper_string);

    // Expected inner object: { value: string }
    let expected_inner = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);

    // Expected: { inner: { value: string } }
    let expected = interner.object(vec![PropertyInfo::new(inner_name, expected_inner)]);

    assert_eq!(
        result, expected,
        "Wrapper<string> should expand to {{ inner: {{ value: string }} }}"
    );
}

// =============================================================================
// Conditional Type Edge Cases
// =============================================================================

/// Test conditional with `unknown` as check type.
///
/// `unknown extends string ? true : false` should evaluate to `false`
/// because `unknown` is not assignable to `string`.
#[test]
fn test_conditional_unknown_check_type() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // unknown extends string ? true : false
    let cond = ConditionalType {
        check_type: TypeId::UNKNOWN,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // unknown is not assignable to string, so false branch
    assert_eq!(result, lit_false, "unknown extends string should be false");
}

/// Test conditional with `unknown` extends `unknown`.
///
/// `unknown extends unknown ? true : false` should evaluate to `true`
/// because `unknown` is assignable to itself.
#[test]
fn test_conditional_unknown_extends_unknown() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // unknown extends unknown ? true : false
    let cond = ConditionalType {
        check_type: TypeId::UNKNOWN,
        extends_type: TypeId::UNKNOWN,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    assert_eq!(result, lit_true, "unknown extends unknown should be true");
}

/// Test distributive conditional over intersection type.
///
/// `(string & { length: number }) extends string ? true : false`
/// The intersection should extend string, so result is true.
#[test]
fn test_conditional_intersection_check_type() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // Create intersection: string & { length: number }
    let length_name = interner.intern_string("length");
    let length_obj = interner.object(vec![PropertyInfo::new(length_name, TypeId::NUMBER)]);
    let string_intersection = interner.intersection(vec![TypeId::STRING, length_obj]);

    // (string & { length: number }) extends string ? true : false
    let cond = ConditionalType {
        check_type: string_intersection,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // string & {...} extends string should be true (intersection is more specific)
    assert_eq!(
        result, lit_true,
        "string intersection extends string should be true"
    );
}

/// Test conditional with `never` as check type (non-distributive).
///
/// `never extends T ? A : B` should be `never` when distributive is false
/// and check type is exactly `never`.
#[test]
fn test_conditional_never_check_type_non_distributive() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // never extends string ? true : false (non-distributive)
    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // With non-distributive, never extends T should evaluate normally
    // never is assignable to everything, so true branch
    assert_eq!(
        result, lit_true,
        "never extends string (non-distributive) should be true"
    );
}

/// Test conditional with `never` extends type.
///
/// `string extends never ? true : false` should be `false`
/// because string is not assignable to never.
#[test]
fn test_conditional_extends_never() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // string extends never ? true : false
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NEVER,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // string is not assignable to never, so false branch
    assert_eq!(result, lit_false, "string extends never should be false");
}

/// Test conditional with `never` extends `never`.
///
/// `never extends never ? true : false` should be `true`
/// because never is assignable to never.
#[test]
fn test_conditional_never_extends_never() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // never extends never ? true : false
    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::NEVER,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // never extends never should be true
    assert_eq!(result, lit_true, "never extends never should be true");
}

/// Test multiple `infer` in tuple pattern.
///
/// `[string, number] extends [infer A, infer B] ? [B, A] : never`
/// Should extract both elements and swap them.
#[test]
fn test_conditional_infer_tuple_multiple_positions() {
    let interner = TypeInterner::new();

    // Create tuple: [string, number]
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    // Create infer placeholders for A and B
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Create extends pattern: [infer A, infer B]
    let pattern = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Create true branch: [B, A] - swapped
    // We reference the inferred types using their positions
    let swapped = interner.tuple(vec![
        TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: tuple_type,
        extends_type: pattern,
        true_type: swapped,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Expected: [number, string] (swapped)
    let expected = interner.tuple(vec![
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

    assert_eq!(
        result, expected,
        "[string, number] with [infer A, infer B] should swap to [number, string]"
    );
}

/// Test nested conditional types (conditional in true branch).
///
/// `T extends string ? (T extends "hello" ? "greeting" : "other") : "not string"`
#[test]
fn test_conditional_nested_in_true_branch() {
    let interner = TypeInterner::new();

    let hello_lit = interner.literal_string("hello");
    let greeting_lit = interner.literal_string("greeting");
    let other_lit = interner.literal_string("other");
    let not_string_lit = interner.literal_string("not string");

    // Inner conditional: "hello" extends "hello" ? "greeting" : "other"
    let inner_cond = interner.conditional(ConditionalType {
        check_type: hello_lit,
        extends_type: hello_lit,
        true_type: greeting_lit,
        false_type: other_lit,
        is_distributive: false,
    });

    // Outer: "hello" extends string ? <inner> : "not string"
    let outer_cond = ConditionalType {
        check_type: hello_lit,
        extends_type: TypeId::STRING,
        true_type: inner_cond,
        false_type: not_string_lit,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &outer_cond);

    // "hello" extends string -> true, then "hello" extends "hello" -> "greeting"
    assert_eq!(
        result, greeting_lit,
        "nested conditional should resolve to 'greeting'"
    );
}

/// Test distributive conditional with literal union.
///
/// `("a" | "b" | "c") extends "a" ? "yes" : "no"`
/// Distributes to: ("a" extends "a" ? "yes" : "no") | ("b" extends "a" ? "yes" : "no") | ...
#[test]
fn test_conditional_distributive_literal_union() {
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let yes_lit = interner.literal_string("yes");
    let no_lit = interner.literal_string("no");

    let abc_union = interner.union(vec![lit_a, lit_b, lit_c]);

    // ("a" | "b" | "c") extends "a" ? "yes" : "no"
    let cond = ConditionalType {
        check_type: abc_union,
        extends_type: lit_a,
        true_type: yes_lit,
        false_type: no_lit,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // "a" -> "yes", "b" -> "no", "c" -> "no"
    // Result: "yes" | "no"
    let expected = interner.union(vec![yes_lit, no_lit]);
    assert_eq!(
        result, expected,
        "distributive over literal union should produce 'yes' | 'no'"
    );
}

/// Test conditional with `any` in extends position.
///
/// `string extends any ? true : false` should be `true`
/// because everything extends any.
#[test]
fn test_conditional_extends_any() {
    let interner = TypeInterner::new();

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);

    // string extends any ? true : false
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::ANY,
        true_type: lit_true,
        false_type: lit_false,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // string extends any is always true
    assert_eq!(result, lit_true, "string extends any should be true");
}

/// Test infer with constraint that doesn't match.
///
/// `{ x: number } extends { x: infer T extends string } ? T : never`
/// The constraint `T extends string` doesn't match `number`, so false branch.
#[test]
fn test_conditional_infer_constraint_mismatch_edge() {
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");
    let t_name = interner.intern_string("T");

    // Create { x: number }
    let obj_number = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    // Create infer T extends string
    let infer_t = interner.intern(TypeData::Infer(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // Create pattern { x: infer T extends string }
    let pattern = interner.object(vec![PropertyInfo::new(x_name, infer_t)]);

    let cond = ConditionalType {
        check_type: obj_number,
        extends_type: pattern,
        true_type: infer_t,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // number doesn't satisfy constraint `extends string`, so false branch
    assert_eq!(
        result,
        TypeId::NEVER,
        "infer with mismatched constraint should produce never"
    );
}

// =========================================================================
// Template Literal Type Inference - Hyphen Pattern Tests
// =========================================================================
// Tests for template literal patterns using hyphen separators like `hello-${string}`

#[test]
fn test_template_literal_hyphen_prefix_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `hello-${infer R}` ? R : never
    // Input: "hello-world" => R = "world"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `hello-${infer R}` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello-")),
        TemplateSpan::Type(infer_r),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("hello-world"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("world");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_two_part_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `${infer First}-${infer Rest}` ? [First, Rest] : never
    // Input: "foo-bar-baz" => First = "foo", Rest = "bar-baz"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_first = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("First"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_rest = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("Rest"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `${infer First}-${infer Rest}` ? [First, Rest] : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_first),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_rest),
    ]);

    let true_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_first,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: infer_rest,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("foo-bar-baz"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // TypeScript uses first-match semantics: First = "foo", Rest = "bar-baz"
    let expected = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("foo"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("bar-baz"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_suffix_pattern() {
    let interner = TypeInterner::new();

    // Pattern: T extends `${infer R}-handler` ? R : never
    // Input: "click-handler" => R = "click"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `${infer R}-handler` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("-handler")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("click-handler"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("click");
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_distributive_union() {
    let interner = TypeInterner::new();

    // Pattern: T extends `event-${infer R}` ? R : never (distributive)
    // Input: "event-click" | "event-load" => "click" | "load"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `event-${infer R}` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("event-")),
        TemplateSpan::Type(infer_r),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let lit_click = interner.literal_string("event-click");
    let lit_load = interner.literal_string("event-load");
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![lit_click, lit_load]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("click"),
        interner.literal_string("load"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_template_literal_hyphen_no_match_returns_never() {
    let interner = TypeInterner::new();

    // Pattern: T extends `prefix-${infer R}` ? R : never
    // Input: "other-value" (doesn't start with "prefix-") => never

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `prefix-${infer R}` ? R : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(infer_r),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("other-value"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // "other-value" doesn't match pattern "prefix-${infer R}", so returns never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_template_literal_prefix_infer_suffix_extraction() {
    let interner = TypeInterner::new();

    // Pattern: T extends `start-${infer M}-end` ? M : never
    // Input: "start-middle-end" => M = "middle"

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("M");
    let infer_m = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `start-${infer M}-end` ? M : never
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("start-")),
        TemplateSpan::Type(infer_m),
        TemplateSpan::Text(interner.intern_string("-end")),
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_m,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.literal_string("start-middle-end"));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("middle");
    assert_eq!(result, expected);
}
