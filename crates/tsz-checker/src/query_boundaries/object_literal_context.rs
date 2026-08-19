use tsz_common::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{CallSignature, CallableShape, ParamInfo, PropertyInfo, TypeId, Visibility};

pub(crate) fn contextual_union_preserve_members(
    db: &dyn TypeDatabase,
    members: Vec<TypeId>,
) -> TypeId {
    db.union_preserve_members(members)
}

pub(crate) fn contextual_intersection(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn mapped_contextual_property_number_key_type(
    db: &dyn TypeDatabase,
    value: f64,
) -> TypeId {
    db.literal_number(value)
}

pub(crate) fn mapped_contextual_property_string_key_type(
    db: &dyn TypeDatabase,
    value: &str,
) -> TypeId {
    db.literal_string(value)
}

pub(crate) fn synthetic_this_method_callable(
    db: &dyn TypeDatabase,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: true,
            declaration_group: 0,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

pub(crate) const fn synthetic_this_method_property(
    name: Atom,
    method_type: TypeId,
    write_type: TypeId,
    readonly: bool,
    declaration_order: u32,
) -> PropertyInfo {
    synthetic_this_property(
        name,
        method_type,
        write_type,
        readonly,
        true,
        declaration_order,
    )
}

pub(crate) const fn synthetic_this_property(
    name: Atom,
    type_id: TypeId,
    write_type: TypeId,
    readonly: bool,
    is_method: bool,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type,
        optional: false,
        readonly,
        is_method,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

pub(crate) fn synthetic_this_object(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}
