//! Callable-shape index access evaluation.

use crate::relations::subtype::TypeResolver;
use crate::types::{CallableShape, TypeData, TypeId};
use crate::utils;
use crate::visitor::{literal_number, union_list_id};

use super::super::evaluate::TypeEvaluator;
use super::string_index_helpers::string_index_signature_applies;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate index access on a callable type (class constructor / `typeof ClassName`).
    ///
    /// Callable types have static properties and index signatures, analogous to
    /// `ObjectWithIndex`. This resolves type-level indexed access like
    /// `(typeof B)["foo"]` or `(typeof B)[number]`.
    pub(crate) fn evaluate_callable_index(
        &self,
        shape: &CallableShape,
        index_type: TypeId,
    ) -> TypeId {
        let string_index = shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL);
        let symbol_index = shape
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type == TypeId::SYMBOL);

        // If index is a union, evaluate each member.
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_callable_index(shape, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is a literal string or unique symbol, look up properties first,
        // then fallback to index sigs.
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
        {
            let is_symbol_key = matches!(
                self.interner().lookup(index_type),
                Some(TypeData::UniqueSymbol(_))
            );
            for prop in &shape.properties {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            if utils::is_numeric_property_name(self.interner(), name)
                && let Some(number_index) = shape.number_index.as_ref()
            {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if is_symbol_key && let Some(symbol_index) = symbol_index {
                return self.add_undefined_if_unchecked(symbol_index.value_type);
            }
            // Symbol-keyed properties must NOT fall through to string index signatures.
            if !is_symbol_key
                && let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // If index is a literal number, prefer number index, then string index.
        if literal_number(self.interner(), index_type).is_some() {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // Bare `string`/`number`/`symbol` indices that match no applicable index
        // signature are a TS2536/TS2537 failure: tsc resolves the access to the error
        // type rather than the union of all member value types, so downstream checks
        // are suppressed. A numeric index still falls back to a string index signature
        // (numeric keys are string keys).
        if index_type == TypeId::STRING {
            if let Some(string_index) = string_index
                && string_index_signature_applies(self, string_index, index_type)
            {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::ERROR;
        }

        if index_type == TypeId::NUMBER {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = string_index {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::ERROR;
        }

        if index_type == TypeId::SYMBOL {
            if let Some(symbol_index) = symbol_index {
                return self.add_undefined_if_unchecked(symbol_index.value_type);
            }
            return TypeId::ERROR;
        }

        // String-like index types (template literals, string intrinsics, branded strings)
        // should use the string index signature when available.
        if let Some(string_index) = string_index
            && string_index_signature_applies(self, string_index, index_type)
        {
            return self.add_undefined_if_unchecked(string_index.value_type);
        }

        TypeId::UNDEFINED
    }
}
