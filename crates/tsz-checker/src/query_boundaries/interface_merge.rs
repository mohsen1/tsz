//! Interface merge construction boundary.
//!
//! Interface merging resolves heritage, decides override/order policy, and
//! gathers final surface facts in checker. This module owns the solver shape
//! literals used to intern the merged callable, object, and intersection types.

use tsz_binder::SymbolId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{CallSignature, CallableShape, IndexSignature, ObjectFlags, ObjectShape};
use tsz_solver::{PropertyInfo, TypeId};

pub(crate) struct MergedCallableSurface {
    pub(crate) call_signatures: Vec<CallSignature>,
    pub(crate) construct_signatures: Vec<CallSignature>,
    pub(crate) properties: Vec<PropertyInfo>,
    pub(crate) string_index: Option<IndexSignature>,
    pub(crate) number_index: Option<IndexSignature>,
    pub(crate) symbol: Option<SymbolId>,
    pub(crate) is_abstract: bool,
}

impl MergedCallableSurface {
    pub(crate) const fn new(
        call_signatures: Vec<CallSignature>,
        construct_signatures: Vec<CallSignature>,
        properties: Vec<PropertyInfo>,
        string_index: Option<IndexSignature>,
        number_index: Option<IndexSignature>,
        symbol: Option<SymbolId>,
        is_abstract: bool,
    ) -> Self {
        Self {
            call_signatures,
            construct_signatures,
            properties,
            string_index,
            number_index,
            symbol,
            is_abstract,
        }
    }
}

pub(crate) fn merged_callable_type(
    db: &dyn TypeDatabase,
    surface: MergedCallableSurface,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: surface.call_signatures,
        construct_signatures: surface.construct_signatures,
        properties: surface.properties,
        string_index: surface.string_index,
        number_index: surface.number_index,
        symbol: surface.symbol,
        is_abstract: surface.is_abstract,
    })
}

pub(crate) struct MergedObjectSurface {
    pub(crate) properties: Vec<PropertyInfo>,
    pub(crate) string_index: Option<IndexSignature>,
    pub(crate) number_index: Option<IndexSignature>,
    pub(crate) symbol_index: Option<IndexSignature>,
    pub(crate) symbol: Option<SymbolId>,
}

impl MergedObjectSurface {
    pub(crate) const fn new(
        properties: Vec<PropertyInfo>,
        string_index: Option<IndexSignature>,
        number_index: Option<IndexSignature>,
        symbol_index: Option<IndexSignature>,
        symbol: Option<SymbolId>,
    ) -> Self {
        Self {
            properties,
            string_index,
            number_index,
            symbol_index,
            symbol,
        }
    }
}

pub(crate) fn merged_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
    symbol: Option<SymbolId>,
) -> TypeId {
    db.object_with_flags_and_symbol(properties, ObjectFlags::empty(), symbol)
}

pub(crate) fn merged_object_with_index_type(
    db: &dyn TypeDatabase,
    surface: MergedObjectSurface,
) -> TypeId {
    db.object_with_index(ObjectShape {
        properties: surface.properties,
        string_index: surface.string_index,
        number_index: surface.number_index,
        symbol_index: surface.symbol_index,
        symbol: surface.symbol,
        ..ObjectShape::default()
    })
}

pub(crate) fn merged_intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn merged_intersection_pair_type(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> TypeId {
    db.intersection2(left, right)
}
