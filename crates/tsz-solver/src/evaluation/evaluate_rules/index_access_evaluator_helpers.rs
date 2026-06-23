//! `TypeEvaluator` helper methods shared by indexed-access rules.

use crate::relations::subtype::TypeResolver;
use crate::types::{ObjectShape, PropertyInfo, TupleElement, TypeId};

use super::super::evaluate::TypeEvaluator;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Evaluate property access on an object type with index signatures.
    pub(crate) fn evaluate_object_with_index(
        &self,
        shape: &ObjectShape,
        index_type: TypeId,
    ) -> TypeId {
        super::index_access_object_with_index::evaluate_object_with_index(self, shape, index_type)
    }

    pub(crate) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        crate::utils::optional_property_type(self.interner(), prop)
    }

    pub(crate) fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access() || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner().union2(type_id, TypeId::UNDEFINED)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        super::index_access_keys::rest_element_type(self.interner(), type_id)
    }

    /// Evaluate index access on a tuple type.
    pub(crate) fn evaluate_tuple_index(
        &self,
        elements: &[TupleElement],
        index_type: TypeId,
    ) -> TypeId {
        super::index_access_keys::evaluate_tuple_index(
            self.interner(),
            elements,
            index_type,
            self.no_unchecked_indexed_access(),
        )
    }

    pub(crate) fn evaluate_array_index(&self, elem: TypeId, index_type: TypeId) -> TypeId {
        super::index_access_keys::evaluate_array_index(
            self.interner(),
            elem,
            index_type,
            self.no_unchecked_indexed_access(),
        )
    }
}
