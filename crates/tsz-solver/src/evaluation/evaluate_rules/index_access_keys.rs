//! Array and tuple key evaluation helpers for indexed access.

use crate::construction::TypeDatabase;
use crate::objects::ApparentMemberKind;
use crate::objects::apparent::is_member;
use crate::types::{IntrinsicKind, LiteralValue, TupleElement, TypeId, TypeListId};
use crate::utils;
use crate::visitor::{TypeVisitor, array_element_type, literal_number, tuple_list_id};

use super::super::evaluate::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID,
};
use super::apparent::make_apparent_method_type;

/// Lazily compute and cache array member types (length + apparent methods).
/// Shared between `ArrayKeyVisitor` and `TupleKeyVisitor`.
fn get_or_init_array_member_types(
    cache: &mut Option<Vec<TypeId>>,
    db: &dyn TypeDatabase,
) -> Vec<TypeId> {
    cache
        .get_or_insert_with(|| {
            vec![
                TypeId::NUMBER,
                make_apparent_method_type(db, TypeId::ANY),
                make_apparent_method_type(db, TypeId::BOOLEAN),
                make_apparent_method_type(db, TypeId::NUMBER),
                make_apparent_method_type(db, TypeId::VOID),
                make_apparent_method_type(db, TypeId::STRING),
            ]
        })
        .clone()
}

fn get_array_member_kind(name: &str) -> Option<ApparentMemberKind> {
    if name == "length" {
        return Some(ApparentMemberKind::Value(TypeId::NUMBER));
    }
    if is_member(name, ARRAY_METHODS_RETURN_ANY) {
        return Some(ApparentMemberKind::Method(TypeId::ANY));
    }
    if is_member(name, ARRAY_METHODS_RETURN_BOOLEAN) {
        return Some(ApparentMemberKind::Method(TypeId::BOOLEAN));
    }
    if is_member(name, ARRAY_METHODS_RETURN_NUMBER) {
        return Some(ApparentMemberKind::Method(TypeId::NUMBER));
    }
    if is_member(name, ARRAY_METHODS_RETURN_VOID) {
        return Some(ApparentMemberKind::Method(TypeId::VOID));
    }
    if is_member(name, ARRAY_METHODS_RETURN_STRING) {
        return Some(ApparentMemberKind::Method(TypeId::STRING));
    }
    None
}

fn add_undefined_if_unchecked(
    db: &dyn TypeDatabase,
    no_unchecked_indexed_access: bool,
    type_id: TypeId,
) -> TypeId {
    if !no_unchecked_indexed_access || type_id == TypeId::UNDEFINED {
        return type_id;
    }
    db.union2(type_id, TypeId::UNDEFINED)
}

/// Visitor to handle array index access: `Array[K]`
///
/// Evaluates what type is returned when indexing an array with various key types.
/// Uses `Option<TypeId>` to signal "use default fallback" via `None`.
struct ArrayKeyVisitor<'a> {
    db: &'a dyn TypeDatabase,
    element_type: TypeId,
}

impl<'a> ArrayKeyVisitor<'a> {
    fn new(db: &'a dyn TypeDatabase, element_type: TypeId) -> Self {
        Self { db, element_type }
    }

    /// Driver method that handles the fallback logic.
    fn evaluate(&mut self, index_type: TypeId) -> TypeId {
        let result = self.visit_type(self.db, index_type);
        result.unwrap_or(self.element_type)
    }
}

impl TypeVisitor for ArrayKeyVisitor<'_> {
    type Output = Option<TypeId>;

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        let mut results = Vec::new();
        for &member in members.iter() {
            let result = self.evaluate(member);
            if result != TypeId::UNDEFINED {
                results.push(result);
            }
        }
        if results.is_empty() {
            Some(TypeId::UNDEFINED)
        } else {
            Some(self.db.union(results))
        }
    }

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        match kind {
            // tsc: Array<T>[number] and Array<T>[string] both return T (the
            // element type). For string indexing, the numeric index signature
            // (returning T) is implicitly available under string keys, so the
            // numeric index type is returned.
            IntrinsicKind::Number | IntrinsicKind::String => Some(self.element_type),
            _ => Some(TypeId::UNDEFINED),
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        match value {
            LiteralValue::Number(_) => Some(self.element_type),
            LiteralValue::String(atom) => {
                let name = self.db.resolve_atom_ref(*atom);
                if utils::is_numeric_property_name(self.db, *atom) {
                    return Some(self.element_type);
                }
                // Check for known array members.
                if let Some(member) = get_array_member_kind(name.as_ref()) {
                    return match member {
                        ApparentMemberKind::Value(type_id) => Some(type_id),
                        ApparentMemberKind::Method(return_type) => {
                            Some(make_apparent_method_type(self.db, return_type))
                        }
                    };
                }
                Some(TypeId::UNDEFINED)
            }
            // Explicitly handle other literals to avoid incorrect fallback.
            LiteralValue::Boolean(_) | LiteralValue::BigInt(_) => Some(TypeId::UNDEFINED),
        }
    }

    /// Signal "use the default fallback" for unhandled type variants.
    fn default_output() -> Self::Output {
        None
    }
}

/// Get the element type of a rest element, handling arrays and nested tuples.
///
/// For arrays, returns the element type. For tuples, returns the union of all element types.
/// Otherwise returns the type as-is.
pub(super) fn rest_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if let Some(elem) = array_element_type(db, type_id) {
        return elem;
    }
    if let Some(elements) = tuple_list_id(db, type_id) {
        let elements = db.tuple_list(elements);
        let types: Vec<TypeId> = elements
            .iter()
            .map(|e| tuple_element_type_with_rest(db, e))
            .collect();
        if types.is_empty() {
            TypeId::NEVER
        } else {
            db.union(types)
        }
    } else {
        type_id
    }
}

/// Get the type of a tuple element, handling optional and rest elements.
fn tuple_element_type_with_rest(db: &dyn TypeDatabase, element: &TupleElement) -> TypeId {
    let mut type_id = if element.rest {
        rest_element_type(db, element.type_id)
    } else {
        element.type_id
    };

    if element.optional {
        type_id = db.union2(type_id, TypeId::UNDEFINED);
    }

    type_id
}

/// Visitor to handle tuple index access: `Tuple[K]`
///
/// Evaluates what type is returned when indexing a tuple with various key types.
/// Uses `Option<TypeId>` to signal "use default fallback" via `None`.
struct TupleKeyVisitor<'a> {
    db: &'a dyn TypeDatabase,
    elements: &'a [TupleElement],
    array_member_types_cache: Option<Vec<TypeId>>,
}

impl<'a> TupleKeyVisitor<'a> {
    fn new(db: &'a dyn TypeDatabase, elements: &'a [TupleElement]) -> Self {
        Self {
            db,
            elements,
            array_member_types_cache: None,
        }
    }

    /// Driver method that handles the fallback logic.
    fn evaluate(&mut self, index_type: TypeId) -> TypeId {
        let result = self.visit_type(self.db, index_type);
        result.unwrap_or(TypeId::UNDEFINED)
    }

    /// Get the type of a tuple element, handling optional and rest elements.
    fn tuple_element_type(&self, element: &TupleElement) -> TypeId {
        tuple_element_type_with_rest(self.db, element)
    }

    /// Get the type at a specific literal index, handling rest elements.
    fn tuple_index_literal(&self, idx: usize) -> Option<TypeId> {
        for (logical_idx, element) in self.elements.iter().enumerate() {
            if element.rest {
                if let Some(rest_elements) = tuple_list_id(self.db, element.type_id) {
                    let rest_elements = self.db.tuple_list(rest_elements);
                    let inner_idx = idx.saturating_sub(logical_idx);
                    // Recursively search in rest elements.
                    let inner_visitor = TupleKeyVisitor::new(self.db, &rest_elements);
                    return inner_visitor.tuple_index_literal(inner_idx);
                }
                return Some(self.tuple_element_type(element));
            }

            if logical_idx == idx {
                return Some(self.tuple_element_type(element));
            }
        }

        None
    }

    /// Get all tuple element types as a union.
    fn get_all_element_types(&self) -> Vec<TypeId> {
        self.elements
            .iter()
            .map(|e| self.tuple_element_type(e))
            .collect()
    }

    /// Get array member types (cached).
    fn get_array_member_types(&mut self) -> Vec<TypeId> {
        get_or_init_array_member_types(&mut self.array_member_types_cache, self.db)
    }

    fn length_type(&self) -> Option<TypeId> {
        let (min, max) = self.length_bounds()?;
        if min == max {
            return Some(self.db.literal_number(max as f64));
        }

        let members = (min..=max)
            .map(|len| self.db.literal_number(len as f64))
            .collect();
        Some(self.db.union(members))
    }

    fn length_bounds(&self) -> Option<(usize, usize)> {
        const MAX_FIXED_LENGTH: usize = 1000;

        let mut min = 0usize;
        let mut max = 0usize;
        let mut current_type = None;

        let mut rest_count = 0;
        for element in self.elements {
            if element.rest {
                rest_count += 1;
                if rest_count > 1 {
                    return None;
                }
                current_type = Some(element.type_id);
            } else {
                if !element.optional {
                    min += 1;
                }
                max += 1;
                if max > MAX_FIXED_LENGTH {
                    return None;
                }
            }
        }

        while let Some(rest_type_id) = current_type.take() {
            let inner_list_id = tuple_list_id(self.db, rest_type_id)?;
            let inner_elements = self.db.tuple_list(inner_list_id);

            let mut inner_rest_count = 0;
            for element in inner_elements.iter() {
                if element.rest {
                    inner_rest_count += 1;
                    if inner_rest_count > 1 {
                        return None;
                    }
                    current_type = Some(element.type_id);
                } else {
                    if !element.optional {
                        min += 1;
                    }
                    max += 1;
                    if max > MAX_FIXED_LENGTH {
                        return None;
                    }
                }
            }
        }

        Some((min, max))
    }

    /// Check for known array members (length, methods).
    fn get_array_member_kind(&self, name: &str) -> Option<ApparentMemberKind> {
        if name == "length" {
            // Return literal tuple lengths, including optional-element ranges
            // like `[T?]["length"]` -> `0 | 1`.
            if let Some(length_type) = self.length_type() {
                return Some(ApparentMemberKind::Value(length_type));
            }
            return Some(ApparentMemberKind::Value(TypeId::NUMBER));
        }
        get_array_member_kind(name)
    }
}

impl TypeVisitor for TupleKeyVisitor<'_> {
    type Output = Option<TypeId>;

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        let mut results = Vec::new();
        for &member in members.iter() {
            let result = self.evaluate(member);
            if result != TypeId::UNDEFINED {
                results.push(result);
            }
        }
        if results.is_empty() {
            Some(TypeId::UNDEFINED)
        } else {
            Some(self.db.union(results))
        }
    }

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        match kind {
            IntrinsicKind::String => {
                // Return union of all element types + array member types.
                let mut types = self.get_all_element_types();
                types.extend(self.get_array_member_types());
                if types.is_empty() {
                    Some(TypeId::NEVER)
                } else {
                    Some(self.db.union(types))
                }
            }
            IntrinsicKind::Number => {
                // Return union of all element types.
                let all_types = self.get_all_element_types();
                if all_types.is_empty() {
                    Some(TypeId::NEVER)
                } else {
                    Some(self.db.union(all_types))
                }
            }
            _ => Some(TypeId::UNDEFINED),
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        match value {
            LiteralValue::Number(n) => {
                let value = n.0;
                if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                    return Some(TypeId::UNDEFINED);
                }
                let idx = value as usize;
                self.tuple_index_literal(idx).or(Some(TypeId::UNDEFINED))
            }
            LiteralValue::String(atom) => {
                // Check if it's a numeric property name (e.g., "0", "1", "42").
                if utils::is_numeric_property_name(self.db, *atom) {
                    let name = self.db.resolve_atom_ref(*atom);
                    if let Ok(idx) = name.as_ref().parse::<i64>()
                        && let Ok(idx) = usize::try_from(idx)
                    {
                        return self.tuple_index_literal(idx).or(Some(TypeId::UNDEFINED));
                    }
                    return Some(TypeId::UNDEFINED);
                }

                // Check for known array members.
                let name = self.db.resolve_atom_ref(*atom);
                if let Some(member) = self.get_array_member_kind(name.as_ref()) {
                    return match member {
                        ApparentMemberKind::Value(type_id) => Some(type_id),
                        ApparentMemberKind::Method(return_type) => {
                            Some(make_apparent_method_type(self.db, return_type))
                        }
                    };
                }

                Some(TypeId::UNDEFINED)
            }
            // Explicitly handle other literals to avoid incorrect fallback.
            LiteralValue::Boolean(_) | LiteralValue::BigInt(_) => Some(TypeId::UNDEFINED),
        }
    }

    /// Signal "use the default fallback" for unhandled type variants.
    fn default_output() -> Self::Output {
        None
    }
}

pub(super) fn evaluate_tuple_index(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
    index_type: TypeId,
    no_unchecked_indexed_access: bool,
) -> TypeId {
    let mut visitor = TupleKeyVisitor::new(db, elements);
    let result = visitor.evaluate(index_type);

    // Under noUncheckedIndexedAccess, add `| undefined` only when the
    // accessed position is not guaranteed to exist. Fixed tuple elements
    // that are within the minimum guaranteed length never need it.
    if no_unchecked_indexed_access {
        // For literal numeric indices, check against the minimum guaranteed
        // length (count of required non-rest elements).
        let min_guaranteed = elements.iter().filter(|e| e.is_required()).count();
        if let Some(n) = literal_number(db, index_type)
            && (n.0 as usize) < min_guaranteed
        {
            // Position is guaranteed to exist; no undefined needed.
            return result;
        }

        // For non-literal indices (string, number, etc.), or indices
        // beyond the guaranteed range, add undefined.
        return add_undefined_if_unchecked(db, no_unchecked_indexed_access, result);
    }

    result
}

pub(super) fn evaluate_array_index(
    db: &dyn TypeDatabase,
    elem: TypeId,
    index_type: TypeId,
    no_unchecked_indexed_access: bool,
) -> TypeId {
    // Use `ArrayKeyVisitor` to handle the index type.
    let mut visitor = ArrayKeyVisitor::new(db, elem);
    let result = visitor.evaluate(index_type);

    add_undefined_if_unchecked(db, no_unchecked_indexed_access, result)
}
