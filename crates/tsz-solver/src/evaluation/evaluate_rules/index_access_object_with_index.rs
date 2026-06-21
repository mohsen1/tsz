//! Object indexed-access evaluation for object shapes with index signatures.

use crate::relations::subtype::TypeResolver;
use crate::types::{ObjectShape, TypeId};
use crate::utils;
use crate::visitor::{literal_number, union_list_id};

use super::super::evaluate::TypeEvaluator;
use super::string_index_helpers::{
    index_signature_accepts_symbol, number_index_signature_applies, string_index_signature_applies,
};

pub(super) fn evaluate_object_with_index<R: TypeResolver>(
    evaluator: &TypeEvaluator<'_, R>,
    shape: &ObjectShape,
    index_type: TypeId,
) -> TypeId {
    // The `string_index` slot carries the object's non-numeric index signature,
    // which may be string-keyed, symbol-keyed, or span both (`string | symbol` /
    // `PropertyKey`). Route it to the string path unless it is a *symbol-only*
    // signature, and to the symbol path whenever its key space accepts symbols —
    // a structural test, not the former `key_type == symbol` slot heuristic
    // (which dropped union/alias symbol keys, see #14315).
    let string_index = shape
        .string_index
        .as_ref()
        .filter(|idx| idx.key_type != TypeId::SYMBOL);
    let symbol_index = shape
        .string_index
        .as_ref()
        .filter(|idx| index_signature_accepts_symbol(evaluator, idx));

    // If index is a union, evaluate each member.
    if let Some(members) = union_list_id(evaluator.interner(), index_type) {
        let members = evaluator.interner().type_list(members);
        let mut results = Vec::new();
        for &member in members.iter() {
            let result = evaluator.evaluate_object_with_index(shape, member);
            if result != TypeId::UNDEFINED || evaluator.no_unchecked_indexed_access() {
                results.push(result);
            }
        }
        if results.is_empty() {
            return TypeId::UNDEFINED;
        }
        return evaluator.interner().union(results);
    }

    // If index is a literal string or unique symbol, look up the property first,
    // then fallback to string index.
    if let Some(name) = evaluator.literal_property_lookup_atom(index_type) {
        let is_symbol_key = evaluator.index_type_is_symbol_key(index_type);
        for prop in &shape.properties {
            if prop.name == name {
                return evaluator.optional_property_type(prop);
            }
        }
        if utils::is_numeric_property_name(evaluator.interner(), name)
            && let Some(number_index) = shape.number_index.as_ref()
        {
            return evaluator.add_undefined_if_unchecked(number_index.value_type);
        }
        if is_symbol_key && let Some(symbol_index) = symbol_index {
            return evaluator.add_undefined_if_unchecked(symbol_index.value_type);
        }
        // Symbol-keyed properties must not fall through to string index signatures.
        if !is_symbol_key
            && let Some(string_index) = string_index
            && string_index_signature_applies(evaluator, string_index, index_type)
        {
            return evaluator.add_undefined_if_unchecked(string_index.value_type);
        }
        return TypeId::UNDEFINED;
    }

    // If index is a literal number, prefer number index, then string index.
    if literal_number(evaluator.interner(), index_type).is_some() {
        if let Some(number_index) = shape.number_index.as_ref() {
            return evaluator.add_undefined_if_unchecked(number_index.value_type);
        }
        if let Some(string_index) = string_index
            && string_index_signature_applies(evaluator, string_index, index_type)
        {
            return evaluator.add_undefined_if_unchecked(string_index.value_type);
        }
        return TypeId::UNDEFINED;
    }

    // Bare `string`/`number`/`symbol` indices that match no applicable index
    // signature are a TS2536/TS2537 failure: tsc resolves the access to the
    // error type rather than the union of all member value types, so downstream
    // checks are suppressed. A numeric index still falls back to a string index
    // signature (numeric keys are string keys).
    if index_type == TypeId::STRING {
        if let Some(string_index) = string_index
            && string_index_signature_applies(evaluator, string_index, index_type)
        {
            return evaluator.add_undefined_if_unchecked(string_index.value_type);
        }
        return TypeId::ERROR;
    }

    if index_type == TypeId::NUMBER {
        if let Some(number_index) = shape.number_index.as_ref() {
            return evaluator.add_undefined_if_unchecked(number_index.value_type);
        }
        if let Some(string_index) = string_index {
            return evaluator.add_undefined_if_unchecked(string_index.value_type);
        }
        return TypeId::ERROR;
    }

    if index_type == TypeId::SYMBOL {
        if let Some(symbol_index) = symbol_index {
            return evaluator.add_undefined_if_unchecked(symbol_index.value_type);
        }
        return TypeId::ERROR;
    }

    // Template literal types, string intrinsic types, and intersections
    // containing string are all subtypes of string. When the object has a
    // string index signature, these resolve to that index signature's value.
    if let Some(string_index) = string_index
        && string_index_signature_applies(evaluator, string_index, index_type)
    {
        return evaluator.add_undefined_if_unchecked(string_index.value_type);
    }

    // Number subtypes that are neither a bare `number` nor a numeric literal
    // must resolve through the numeric index signature, then fall back to the
    // string index signature because numeric keys coerce to string keys.
    if number_index_signature_applies(evaluator, index_type) {
        if let Some(number_index) = shape.number_index.as_ref() {
            return evaluator.add_undefined_if_unchecked(number_index.value_type);
        }
        if let Some(string_index) = string_index {
            return evaluator.add_undefined_if_unchecked(string_index.value_type);
        }
    }

    TypeId::UNDEFINED
}
