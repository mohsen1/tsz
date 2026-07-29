use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::DefId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeId,
    Visibility,
};

pub(crate) use super::common::{AugmentationTargetKind, classify_for_augmentation};

/// Whether one module-augmentation declaration can contribute to property
/// lookup for a receiver with the supplied declaration owner.
///
/// File-backed receivers match only the exact resolved target file. A receiver
/// without declaration identity can use an unresolved target only when the
/// binder has classified that target as an ambient module.
pub(crate) fn property_augmentation_matches_receiver(
    receiver_owner_file: Option<usize>,
    augmentation_target_file: Option<usize>,
    target_is_declared_ambient: bool,
) -> bool {
    match receiver_owner_file {
        Some(receiver_owner_file) => augmentation_target_file == Some(receiver_owner_file),
        None => augmentation_target_file.is_none() && target_is_declared_ambient,
    }
}

pub(crate) fn call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

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

pub(crate) fn empty_declaration_space_type(db: &dyn TypeDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn declaration_space_lazy_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
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

pub(crate) fn exact_path_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
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

pub(crate) fn exact_path_object_with_index_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
) -> TypeId {
    db.object_with_index(ObjectShape {
        properties,
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
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

pub(crate) fn other_target_with_augmentation_surface(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    augmentation_members: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
) -> TypeId {
    if augmentation_members.is_empty()
        && string_index.is_none()
        && number_index.is_none()
        && symbol_index.is_none()
    {
        return base_type;
    }

    let augmentation_object =
        if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
            db.object_with_index(ObjectShape {
                properties: augmentation_members,
                string_index,
                number_index,
                symbol_index,
                ..ObjectShape::default()
            })
        } else {
            db.object(augmentation_members)
        };
    db.intersection2(base_type, augmentation_object)
}

pub(crate) fn with_augmentation_index_surface_raw(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
) -> TypeId {
    if string_index.is_none() && number_index.is_none() && symbol_index.is_none() {
        return base_type;
    }

    let index_surface = db.object_with_index(ObjectShape {
        string_index,
        number_index,
        symbol_index,
        ..ObjectShape::default()
    });
    db.intersect_types_raw2(base_type, index_surface)
}

#[cfg(test)]
mod tests {
    use super::property_augmentation_matches_receiver;

    #[test]
    fn property_augmentation_selection_uses_resolved_target_identity() {
        assert!(property_augmentation_matches_receiver(
            Some(4),
            Some(4),
            false
        ));
        assert!(!property_augmentation_matches_receiver(
            Some(4),
            Some(5),
            false
        ));
        assert!(!property_augmentation_matches_receiver(Some(4), None, true));
    }

    #[test]
    fn ownerless_property_augmentation_requires_declared_ambient_target() {
        assert!(property_augmentation_matches_receiver(None, None, true));
        assert!(!property_augmentation_matches_receiver(None, None, false));
        assert!(!property_augmentation_matches_receiver(None, Some(4), true));
    }
}
