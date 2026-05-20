use super::*;
use tsz_solver::{DefId, FunctionShape, ParamInfo, TupleElement, TypeInterner};

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
fn this_type_query_via_boundary() {
    let types = TypeInterner::new();

    let this_ty = types.this_type();
    assert!(is_this_type(&types, this_ty));

    // Non-ThisType types should return false
    assert!(!is_this_type(&types, TypeId::ANY));
    assert!(!is_this_type(&types, TypeId::STRING));
    assert!(!is_this_type(&types, TypeId::NUMBER));
    assert!(!is_this_type(&types, TypeId::NEVER));
}

#[test]
fn index_access_type_query_via_boundary() {
    let types = TypeInterner::new();

    // Create T[K] where T = string[], K = number
    let array_ty = types.array(TypeId::STRING);
    let idx_access = types.index_access(array_ty, TypeId::NUMBER);

    let result = index_access_types(&types, idx_access);
    assert_eq!(result, Some((array_ty, TypeId::NUMBER)));

    // Non-IndexAccess types should return None
    assert_eq!(index_access_types(&types, TypeId::STRING), None);
    assert_eq!(index_access_types(&types, array_ty), None);
}

#[test]
fn type_has_property_query() {
    let types = TypeInterner::new();

    let name_atom = types.intern_string("x");
    let obj = types.object(vec![tsz_solver::PropertyInfo {
        name: name_atom,
        type_id: TypeId::NUMBER,
        optional: false,
        readonly: false,
        visibility: tsz_common::common::Visibility::Public,
        is_method: false,
        ..Default::default()
    }]);

    assert!(type_has_property(&types, obj, "x"));
    assert!(!type_has_property(&types, obj, "y"));
}

#[test]
fn type_has_property_union_requires_all_members() {
    let types = TypeInterner::new();

    let name_atom = types.intern_string("name");
    let age_atom = types.intern_string("age");

    let obj_with_name = types.object(vec![tsz_solver::PropertyInfo {
        name: name_atom,
        type_id: TypeId::STRING,
        ..Default::default()
    }]);
    let obj_with_both = types.object(vec![
        tsz_solver::PropertyInfo {
            name: name_atom,
            type_id: TypeId::STRING,
            ..Default::default()
        },
        tsz_solver::PropertyInfo {
            name: age_atom,
            type_id: TypeId::NUMBER,
            ..Default::default()
        },
    ]);

    let union_type = types.union(vec![obj_with_name, obj_with_both]);
    assert!(type_has_property(&types, union_type, "name"));
    assert!(!type_has_property(&types, union_type, "age"));
    assert!(!type_has_property(&types, union_type, "missing"));
}

#[test]
fn tuple_element_union_and_array_element_type() {
    let types = TypeInterner::new();

    let single = types.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(
        tuple_element_type_union(&types, single),
        Some(TypeId::BOOLEAN)
    );

    let empty = types.tuple(vec![]);
    assert_eq!(tuple_element_type_union(&types, empty), Some(TypeId::NEVER));

    assert_eq!(tuple_element_type_union(&types, TypeId::STRING), None);

    let arr = types.array(TypeId::NUMBER);
    assert_eq!(array_element_type(&types, arr), Some(TypeId::NUMBER));
    assert_eq!(array_element_type(&types, single), None);
}

#[test]
fn def_id_and_application_queries() {
    let types = TypeInterner::new();

    let lazy1 = types.lazy(DefId(100));
    let lazy2 = types.lazy(DefId(200));
    assert_eq!(def_id(&types, lazy1), Some(DefId(100)));
    assert_eq!(def_id(&types, lazy2), Some(DefId(200)));
    assert_eq!(def_id(&types, TypeId::STRING), None);

    let app = types.application(lazy1, vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(application_first_arg(&types, app), Some(TypeId::STRING));

    let app_single = types.application(lazy1, vec![TypeId::BOOLEAN]);
    assert_eq!(
        application_first_arg(&types, app_single),
        Some(TypeId::BOOLEAN)
    );

    assert_eq!(application_first_arg(&types, TypeId::ANY), None);
    assert_eq!(application_first_arg(&types, lazy1), None);
}
