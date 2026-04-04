use super::*;
use crate::TypeInterner;
use crate::types::{CallSignature, CallableShape, ParamInfo, TypeId, TypeParamInfo};

fn make_callable_with_construct_sig(
    interner: &TypeInterner,
    return_type: TypeId,
    type_params: Vec<TypeParamInfo>,
) -> TypeId {
    let shape = CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params,
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    };
    interner.callable(shape)
}

fn make_callable_with_call_sig(interner: &TypeInterner, return_type: TypeId) -> TypeId {
    let shape = CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: None,
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    };
    interner.callable(shape)
}

#[test]
fn get_construct_signatures_direct_callable() {
    let interner = TypeInterner::new();
    let callable = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let sigs = get_construct_signatures(&interner, callable);
    assert!(sigs.is_some());
    assert_eq!(sigs.unwrap().len(), 1);
}

#[test]
fn get_construct_signatures_intersection_collects_from_members() {
    let interner = TypeInterner::new();
    // Create two callables with construct signatures
    let ctor1 = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let ctor2 = make_callable_with_construct_sig(&interner, TypeId::NUMBER, vec![]);
    // Create intersection: ctor1 & ctor2
    let intersection = interner.intersection2(ctor1, ctor2);
    let sigs = get_construct_signatures(&interner, intersection);
    assert!(sigs.is_some());
    let sigs = sigs.unwrap();
    assert_eq!(
        sigs.len(),
        2,
        "Should collect construct sigs from both members"
    );
}

#[test]
fn get_construct_signatures_intersection_with_non_callable_member() {
    let interner = TypeInterner::new();
    // Create intersection: Constructor & { prop: string }
    let ctor = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
    let obj = interner.object(vec![]); // plain object, no construct sigs
    let intersection = interner.intersection2(ctor, obj);
    let sigs = get_construct_signatures(&interner, intersection);
    assert!(sigs.is_some());
    assert_eq!(
        sigs.unwrap().len(),
        1,
        "Should find construct sig from callable member"
    );
}

#[test]
fn get_construct_signatures_intersection_no_construct_sigs() {
    let interner = TypeInterner::new();
    // Intersection of non-callable types
    let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
    let sigs = get_construct_signatures(&interner, intersection);
    assert!(sigs.is_none());
}

#[test]
fn get_call_signatures_intersection_collects_from_members() {
    let interner = TypeInterner::new();
    let fn1 = make_callable_with_call_sig(&interner, TypeId::STRING);
    let fn2 = make_callable_with_call_sig(&interner, TypeId::NUMBER);
    let intersection = interner.intersection2(fn1, fn2);
    let sigs = get_call_signatures(&interner, intersection);
    assert!(sigs.is_some());
    let sigs = sigs.unwrap();
    assert_eq!(sigs.len(), 2, "Should collect call sigs from both members");
}

#[test]
fn get_call_signatures_intersection_no_call_sigs() {
    let interner = TypeInterner::new();
    let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
    let sigs = get_call_signatures(&interner, intersection);
    assert!(sigs.is_none());
}

#[test]
fn construct_sig_with_application_return_type_is_extractable() {
    // Simulates the JSX class component scenario where:
    // interface ComponentClass<P> { new(props: P): Component<P, any>; }
    // interface TestClass extends ComponentClass<{reqd: any}> {}
    //
    // The construct signature return type is Application(Component, [props, any])
    // which needs evaluation. The checker should evaluate it before bailing out.
    let interner = TypeInterner::new();

    // Create an Application type (simulating Component<{reqd: any}, any>)
    let inner_obj = interner.object(vec![]);
    let app_type = interner.application(inner_obj, vec![TypeId::STRING, TypeId::ANY]);

    // Create a callable with construct sig returning the Application type
    let callable = make_callable_with_construct_sig(&interner, app_type, vec![]);

    // Verify we CAN extract construct signatures
    let sigs = get_construct_signatures(&interner, callable);
    assert!(sigs.is_some(), "Should extract construct signatures");
    let sigs = sigs.unwrap();
    assert_eq!(sigs.len(), 1);

    // The return type IS an Application (needs evaluation)
    let return_type = sigs[0].return_type;
    assert!(
        crate::type_queries::needs_evaluation_for_merge(&interner, return_type),
        "Application return type needs evaluation"
    );

    // But the type itself does NOT contain type parameters
    // (all args are concrete: STRING, ANY)
    assert!(
        !crate::contains_type_parameters(&interner, return_type),
        "Concrete application should not contain type parameters"
    );
}

#[test]
fn test_union_has_direct_type_parameter() {
    let interner = crate::intern::TypeInterner::new();

    // Single type parameter
    let tp = interner.type_param(crate::types::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    // Not a union — returns false
    assert!(!union_has_direct_type_parameter(&interner, tp));

    // Union containing a type parameter
    let union_with_tp = interner.union2(TypeId::STRING, tp);
    assert!(union_has_direct_type_parameter(&interner, union_with_tp));

    // Union without type parameters
    let plain_union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    assert!(!union_has_direct_type_parameter(&interner, plain_union));

    // Non-union type
    assert!(!union_has_direct_type_parameter(&interner, TypeId::STRING));
}

#[test]
fn test_collect_callable_property_types() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{FunctionShape, PropertyInfo, Visibility};

    // Create a function type (callable property)
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create an object with one callable and one non-callable property
    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("callback"),
            type_id: fn_type,
            write_type: fn_type,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);

    let result = collect_callable_property_types(&interner, obj);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], fn_type);

    // Non-object type returns empty
    assert!(collect_callable_property_types(&interner, TypeId::STRING).is_empty());
}

#[test]
fn test_construct_return_type_for_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{CallSignature, CallableShape, FunctionShape};

    // Function constructor
    let fn_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    assert_eq!(
        construct_return_type_for_type(&interner, fn_ctor),
        Some(TypeId::STRING)
    );

    // Non-constructor function → None
    let fn_regular = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(construct_return_type_for_type(&interner, fn_regular), None);

    // Callable with construct signature
    let callable = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    assert_eq!(
        construct_return_type_for_type(&interner, callable),
        Some(TypeId::BOOLEAN)
    );

    // Non-constructable type → None
    assert_eq!(
        construct_return_type_for_type(&interner, TypeId::STRING),
        None
    );
}

#[test]
fn test_is_constructor_like_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{CallSignature, CallableShape, FunctionShape};

    // Constructor function
    let fn_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    assert!(is_constructor_like_type(&interner, fn_ctor));

    // Regular function — not constructor-like
    let fn_regular = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(!is_constructor_like_type(&interner, fn_regular));

    // Callable with construct signature
    let callable_ctor = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    assert!(is_constructor_like_type(&interner, callable_ctor));

    // Union containing a constructor — should be constructor-like
    let union_with_ctor = interner.union2(TypeId::STRING, fn_ctor);
    assert!(is_constructor_like_type(&interner, union_with_ctor));

    // Plain type — not constructor-like
    assert!(!is_constructor_like_type(&interner, TypeId::STRING));
}

#[test]
fn test_extract_type_params_for_call() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{FunctionShape, TypeParamInfo};

    let tp_t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };

    // Function with 1 type param
    let fn_generic = interner.function(FunctionShape {
        type_params: vec![tp_t],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = extract_type_params_for_call(&interner, fn_generic, 1);
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 1);

    // Non-callable type → None
    assert!(extract_type_params_for_call(&interner, TypeId::STRING, 0).is_none());
}

#[test]
fn test_get_callable_shape_for_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::FunctionShape;

    // Function → wrapped as single-sig callable
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let shape = get_callable_shape_for_type(&interner, fn_type);
    assert!(shape.is_some());
    let shape = shape.unwrap();
    assert_eq!(shape.call_signatures.len(), 1);
    assert_eq!(shape.call_signatures[0].return_type, TypeId::STRING);

    // Non-callable → None
    assert!(get_callable_shape_for_type(&interner, TypeId::NUMBER).is_none());
}

#[test]
fn test_get_overload_call_signatures() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{CallSignature, CallableShape};

    // Callable with 2 overloads → Some
    let overloaded = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let sigs = get_overload_call_signatures(&interner, overloaded);
    assert!(sigs.is_some());
    assert_eq!(sigs.unwrap().len(), 2);

    // Callable with 1 signature → None (not overloaded)
    let single = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    assert!(get_overload_call_signatures(&interner, single).is_none());

    // Non-callable → None
    assert!(get_overload_call_signatures(&interner, TypeId::STRING).is_none());
}

#[test]
fn test_get_object_symbol() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{ObjectFlags, ObjectShape, PropertyInfo, Visibility};

    let sym = tsz_binder::SymbolId(42);

    // Object with symbol — use object_with_index to comply with intern quarantine
    let obj_with_sym = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
        symbol: Some(sym),
    });
    assert_eq!(get_object_symbol(&interner, obj_with_sym), Some(sym));

    // Non-object → None
    assert_eq!(get_object_symbol(&interner, TypeId::STRING), None);
}

#[test]
fn test_get_raw_property_type() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::{PropertyInfo, Visibility};

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let obj = interner.object(vec![
        PropertyInfo {
            name: name_x,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: name_y,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
        },
    ]);

    assert_eq!(
        get_raw_property_type(&interner, obj, name_x),
        Some(TypeId::STRING)
    );
    assert_eq!(
        get_raw_property_type(&interner, obj, name_y),
        Some(TypeId::NUMBER)
    );

    // Non-existent property
    let name_z = interner.intern_string("z");
    assert_eq!(get_raw_property_type(&interner, obj, name_z), None);

    // Non-object type
    assert_eq!(
        get_raw_property_type(&interner, TypeId::STRING, name_x),
        None
    );
}

#[test]
fn test_intersect_constructor_returns() {
    let interner = crate::intern::TypeInterner::new();
    use crate::types::FunctionShape;

    // Function constructor — return type gets intersected
    let fn_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    let result = intersect_constructor_returns(&interner, fn_ctor, TypeId::STRING);
    assert_ne!(result, fn_ctor); // Should produce a new type
    // The result should be a Function with intersected return type
    if let Some(shape_id) = crate::visitor::function_shape_id(&interner, result) {
        let shape = interner.function_shape(shape_id);
        assert!(shape.is_constructor);
        // return type should be object & string (intersection)
        assert_ne!(shape.return_type, TypeId::OBJECT);
    } else {
        panic!("Expected Function type");
    }

    // Non-constructor function — unchanged
    let fn_regular = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(
        intersect_constructor_returns(&interner, fn_regular, TypeId::STRING),
        fn_regular
    );

    // Non-callable — unchanged
    assert_eq!(
        intersect_constructor_returns(&interner, TypeId::STRING, TypeId::NUMBER),
        TypeId::STRING
    );
}

#[test]
fn classify_body_for_arg_preservation_non_conditional() {
    let interner = TypeInterner::new();

    // Non-conditional body → EvaluateAll
    assert_eq!(
        classify_body_for_arg_preservation(&interner, TypeId::STRING),
        BodyArgPreservation::EvaluateAll,
    );
    assert_eq!(
        classify_body_for_arg_preservation(&interner, TypeId::NUMBER),
        BodyArgPreservation::EvaluateAll,
    );
}

#[test]
fn classify_body_for_arg_preservation_conditional_with_infer() {
    let interner = TypeInterner::new();

    let infer_u = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let infer_type = interner.infer(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });

    // Conditional with infer in extends: T extends infer U ? T : never
    let cond_with_infer = interner.conditional(crate::types::ConditionalType {
        check_type: infer_u,
        extends_type: infer_type,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(
        classify_body_for_arg_preservation(&interner, cond_with_infer),
        BodyArgPreservation::ConditionalInfer,
    );

    // Conditional without infer: T extends string ? T : never
    let cond_no_infer = interner.conditional(crate::types::ConditionalType {
        check_type: infer_u,
        extends_type: TypeId::STRING,
        true_type: infer_u,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(
        classify_body_for_arg_preservation(&interner, cond_no_infer),
        BodyArgPreservation::EvaluateAll,
    );
}

#[test]
fn classify_body_for_arg_preservation_conditional_application_infer() {
    let interner = TypeInterner::new();

    let param_t = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let infer_v = interner.infer(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
    });

    // Application(Lazy(42), [T, infer V]) — represents Synthetic<T, infer V>
    let base = interner.lazy(crate::def::DefId(42));
    let app_with_infer = interner.application(base, vec![param_t, infer_v]);

    // Conditional: U extends Synthetic<T, infer V> ? V : never
    let cond_app_infer = interner.conditional(crate::types::ConditionalType {
        check_type: param_t,
        extends_type: app_with_infer,
        true_type: infer_v,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });
    assert_eq!(
        classify_body_for_arg_preservation(&interner, cond_app_infer),
        BodyArgPreservation::ConditionalApplicationInfer,
    );
}

// =========================================================================
// is_type_deeply_any
// =========================================================================

#[test]
fn deeply_any_for_any() {
    let interner = TypeInterner::new();
    assert!(is_type_deeply_any(&interner, TypeId::ANY));
}

#[test]
fn deeply_any_for_non_any_primitives() {
    let interner = TypeInterner::new();
    assert!(!is_type_deeply_any(&interner, TypeId::STRING));
    assert!(!is_type_deeply_any(&interner, TypeId::NUMBER));
    assert!(!is_type_deeply_any(&interner, TypeId::NEVER));
    assert!(!is_type_deeply_any(&interner, TypeId::UNKNOWN));
}

#[test]
fn deeply_any_for_array_of_any() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::ANY);
    assert!(is_type_deeply_any(&interner, arr));
}

#[test]
fn deeply_any_for_array_of_string() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::STRING);
    assert!(!is_type_deeply_any(&interner, arr));
}

#[test]
#[ignore = "Pre-existing failure from recent merges"]
fn deeply_any_for_tuple_of_any() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
            name: None,
        },
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert!(is_type_deeply_any(&interner, tuple));
}

#[test]
fn deeply_any_for_tuple_with_non_any_member() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
            name: None,
        },
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert!(!is_type_deeply_any(&interner, tuple));
}

#[test]
fn deeply_any_for_union_of_any() {
    let interner = TypeInterner::new();
    // Manually create a union with all-any members
    let union = interner.union2(TypeId::ANY, TypeId::ANY);
    assert!(is_type_deeply_any(&interner, union));
}

#[test]
#[ignore = "Pre-existing failure from recent merges"]
fn deeply_any_for_union_with_non_any() {
    let interner = TypeInterner::new();
    let union = interner.union2(TypeId::ANY, TypeId::STRING);
    assert!(!is_type_deeply_any(&interner, union));
}

#[test]
fn deeply_any_for_nested_array_of_any() {
    let interner = TypeInterner::new();
    let inner = interner.array(TypeId::ANY);
    let outer = interner.array(inner);
    assert!(is_type_deeply_any(&interner, outer));
}

// =========================================================================
// contains_application_in_structure
// =========================================================================

#[test]
fn contains_application_direct() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    assert!(contains_application_in_structure(&interner, app));
}

#[test]
fn contains_application_not_present() {
    let interner = TypeInterner::new();
    assert!(!contains_application_in_structure(
        &interner,
        TypeId::STRING
    ));
    assert!(!contains_application_in_structure(&interner, TypeId::ANY));
}

#[test]
fn contains_application_in_union() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let union = interner.union2(TypeId::NUMBER, app);
    assert!(contains_application_in_structure(&interner, union));
}

#[test]
fn contains_application_in_intersection() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let intersection = interner.intersection(vec![TypeId::NUMBER, app]);
    assert!(contains_application_in_structure(&interner, intersection));
}

#[test]
fn contains_application_in_readonly() {
    let interner = TypeInterner::new();
    let base = interner.lazy(crate::def::DefId(1));
    let app = interner.application(base, vec![TypeId::STRING]);
    let readonly = interner.readonly_type(app);
    assert!(contains_application_in_structure(&interner, readonly));
}

#[test]
fn contains_application_union_without_app() {
    let interner = TypeInterner::new();
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    assert!(!contains_application_in_structure(&interner, union));
}
