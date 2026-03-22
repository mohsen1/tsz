use super::*;
use tsz_solver::{
    DefId, FunctionShape, ParamInfo, PropertyInfo, TupleElement, TypeInterner, Visibility,
};

#[test]
fn exposes_property_access_boundary_queries() {
    let types = TypeInterner::new();

    let function = types.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let callable = types.callable(tsz_solver::CallableShape {
        call_signatures: vec![tsz_solver::CallSignature {
            type_params: vec![],
            params: vec![ParamInfo::unnamed(TypeId::BOOLEAN)],
            this_type: None,
            return_type: TypeId::STRING,
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
    let tuple = types.tuple(vec![
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
    let readonly_tuple = types.readonly_type(tuple);
    let lazy = types.lazy(DefId(77));
    let app_base = types.lazy(DefId(81));
    let application = types.application(app_base, vec![TypeId::BOOLEAN, TypeId::NUMBER]);

    assert!(is_function_type(&types, function));
    assert_eq!(unwrap_readonly(&types, readonly_tuple), tuple);
    assert_eq!(
        tuple_element_type_union(&types, tuple),
        Some(types.union(vec![TypeId::NUMBER, TypeId::STRING]))
    );
    assert_eq!(
        application_first_arg(&types, application),
        Some(TypeId::BOOLEAN)
    );
    assert!(is_boolean_type(&types, TypeId::BOOLEAN));
    assert!(is_number_type(&types, TypeId::NUMBER));
    assert!(is_string_type(&types, TypeId::STRING));
    assert!(is_symbol_type(&types, TypeId::SYMBOL));
    assert!(is_bigint_type(&types, TypeId::BIGINT));
    assert_eq!(def_id(&types, lazy), Some(DefId(77)));
    assert_eq!(
        function_shape(&types, function).map(|shape| shape.params.len()),
        Some(1)
    );
    assert_eq!(
        callable_shape(&types, callable).map(|shape| shape.call_signatures.len()),
        Some(1)
    );
    assert!(array_element_type(&types, tuple).is_none());
}

#[test]
fn this_type_query_replaces_direct_type_data_inspection() {
    let types = TypeInterner::new();

    let this_type = types.this_type();
    // The is_this_type query replaces direct ThisType variant matching
    assert!(tsz_solver::is_this_type(&types, this_type));
    // Primitives and other types are not ThisType
    assert!(!tsz_solver::is_this_type(&types, TypeId::STRING));
    assert!(!tsz_solver::is_this_type(&types, TypeId::NUMBER));
    assert!(!tsz_solver::is_this_type(&types, TypeId::ANY));
    assert!(!tsz_solver::is_this_type(&types, TypeId::NEVER));
    // Type parameters are not ThisType
    let tp = types.type_param(tsz_solver::TypeParamInfo {
        name: tsz_common::interner::Atom::none(),
        constraint: None,
        default: None,
        is_const: false,
    });
    assert!(!tsz_solver::is_this_type(&types, tp));
}

#[test]
fn type_has_property_on_objects_and_unions() {
    let types = TypeInterner::new();

    let name_atom = types.intern_string("name");
    let age_atom = types.intern_string("age");

    let obj_with_name = types.object(vec![PropertyInfo {
        name: name_atom,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
    }]);
    let obj_with_name_and_age = types.object(vec![
        PropertyInfo {
            name: name_atom,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        },
        PropertyInfo {
            name: age_atom,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        },
    ]);

    // Single object: property present
    assert!(type_has_property(&types, obj_with_name, "name"));
    assert!(!type_has_property(&types, obj_with_name, "age"));
    assert!(!type_has_property(&types, obj_with_name, "missing"));

    // Union: property must be on ALL members
    let union_type = types.union(vec![obj_with_name, obj_with_name_and_age]);
    assert!(type_has_property(&types, union_type, "name"));
    assert!(!type_has_property(&types, union_type, "age")); // only on second member
    assert!(!type_has_property(&types, union_type, "missing"));
}

#[test]
fn indexed_access_via_tuple_element_union() {
    let types = TypeInterner::new();

    // Single-element tuple
    let single_tuple = types.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(
        tuple_element_type_union(&types, single_tuple),
        Some(TypeId::BOOLEAN)
    );

    // Empty tuple returns Some(NEVER) — union of zero elements is bottom type
    let empty_tuple = types.tuple(vec![]);
    assert_eq!(
        tuple_element_type_union(&types, empty_tuple),
        Some(TypeId::NEVER)
    );

    // Non-tuple types return None
    assert_eq!(tuple_element_type_union(&types, TypeId::STRING), None);
    assert_eq!(tuple_element_type_union(&types, TypeId::ANY), None);

    // Array element type distinguishes arrays from tuples
    let array_type = types.array(TypeId::NUMBER);
    assert_eq!(array_element_type(&types, array_type), Some(TypeId::NUMBER));
    assert_eq!(array_element_type(&types, single_tuple), None);
}

#[test]
fn def_id_and_application_queries_for_indexed_access() {
    let types = TypeInterner::new();

    // Lazy types expose their DefId
    let lazy1 = types.lazy(DefId(100));
    let lazy2 = types.lazy(DefId(200));
    assert_eq!(def_id(&types, lazy1), Some(DefId(100)));
    assert_eq!(def_id(&types, lazy2), Some(DefId(200)));

    // Non-lazy types have no DefId
    assert_eq!(def_id(&types, TypeId::STRING), None);
    assert_eq!(def_id(&types, TypeId::NUMBER), None);

    // Application types expose their first arg for indexed access
    let app = types.application(lazy1, vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(application_first_arg(&types, app), Some(TypeId::STRING));

    // Application with single arg
    let app_single = types.application(lazy1, vec![TypeId::BOOLEAN]);
    assert_eq!(
        application_first_arg(&types, app_single),
        Some(TypeId::BOOLEAN)
    );

    // Non-application types return None
    assert_eq!(application_first_arg(&types, TypeId::ANY), None);
    assert_eq!(application_first_arg(&types, lazy1), None);
}
