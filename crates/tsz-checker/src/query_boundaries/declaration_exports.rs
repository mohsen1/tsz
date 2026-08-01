//! Declaration export surface construction boundary.
//!
//! Declaration checkers collect AST, binder, visibility, duplicate-diagnostic,
//! and source-order facts. This module owns the solver types those facts become
//! for namespace/module value surfaces.

use tsz_binder::SymbolId;
use tsz_common::{Atom, Visibility};
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{CallableShape, ObjectFlags, PropertyInfo, TypeId};

pub(crate) const fn declaration_export_property(
    name: Atom,
    type_id: TypeId,
    declaration_order: u32,
) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method: false,
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

pub(crate) fn declaration_lazy_export_type(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn module_export_augmented_type(
    db: &dyn TypeDatabase,
    existing_type: TypeId,
    augmentation_type: TypeId,
) -> TypeId {
    db.intersection2(existing_type, augmentation_type)
}

pub(crate) fn dynamic_import_module_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn dynamic_import_promise_type(
    db: &dyn TypeDatabase,
    promise_base: TypeId,
    inner_type: TypeId,
) -> TypeId {
    db.application(promise_base, vec![inner_type])
}

pub(crate) fn empty_namespace_object_type(db: &dyn TypeDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn namespace_object_placeholder_type(db: &dyn TypeDatabase) -> TypeId {
    empty_namespace_object_type(db)
}

pub(crate) fn namespace_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: SymbolId,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, ObjectFlags::empty(), Some(symbol))
}

pub(crate) fn namespace_merged_constructor_callable_type(
    db: &dyn TypeDatabase,
    shape: &CallableShape,
    properties: Vec<PropertyInfo>,
    symbol: SymbolId,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: shape.call_signatures.clone(),
        construct_signatures: shape.construct_signatures.clone(),
        properties,
        string_index: shape.string_index,
        number_index: shape.number_index,
        symbol: Some(symbol),
        is_abstract: false,
    })
}

/// `symbol` is `Some` only for an *instantiated* module merge (`ValueModule`),
/// which is what `tsc` renders as `typeof f`; an empty or type-only namespace
/// passes `None` and keeps the structural rendering.
pub(crate) fn namespace_merged_function_callable_type(
    db: &dyn TypeDatabase,
    shape: &CallableShape,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: shape.call_signatures.clone(),
        construct_signatures: shape.construct_signatures.clone(),
        properties,
        string_index: shape.string_index,
        number_index: shape.number_index,
        symbol,
        is_abstract: false,
    })
}
