use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeId,
    Visibility,
};

pub(crate) const fn augmentation_member_property(
    name: Atom,
    type_id: TypeId,
    optional: bool,
    readonly: bool,
    is_method: bool,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional,
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
        declared_location: tsz_binder::StableLocation::NONE,
    }
}

pub(crate) const fn augmentation_any_member_property(
    name: Atom,
    readonly: bool,
    is_method: bool,
    declaration_order: u32,
) -> PropertyInfo {
    augmentation_member_property(
        name,
        TypeId::ANY,
        false,
        readonly,
        is_method,
        declaration_order,
    )
}

pub(crate) const fn augmentation_value_member_property(
    name: Atom,
    type_id: TypeId,
) -> PropertyInfo {
    augmentation_member_property(name, type_id, false, false, false, 0)
}

pub(crate) const fn augmentation_any_value_member_property(name: Atom) -> PropertyInfo {
    augmentation_any_member_property(name, false, false, 0)
}

pub(crate) const fn augmentation_any_method_member_property(name: Atom) -> PropertyInfo {
    augmentation_any_member_property(name, false, true, 0)
}

pub(crate) fn self_reference_application_type(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    type_args: Vec<TypeId>,
) -> TypeId {
    db.application(base_type, type_args)
}

pub(crate) fn augmented_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    flags: ObjectFlags,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, flags, symbol)
}

pub(crate) fn augmented_object_with_index_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    flags: ObjectFlags,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_index(ObjectShape {
        flags,
        properties,
        string_index,
        number_index,
        symbol_index,
        symbol,
    })
}

pub(crate) fn augmented_callable_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
    is_abstract: bool,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        construct_signatures,
        properties,
        string_index,
        number_index,
        symbol,
        is_abstract,
    })
}

pub(crate) fn other_target_with_augmentation_members(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    augmentation_members: Vec<PropertyInfo>,
) -> TypeId {
    if augmentation_members.is_empty() {
        return base_type;
    }

    let augmentation_object = db.object(augmentation_members);
    db.intersection2(base_type, augmentation_object)
}
