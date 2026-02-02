//! Index access type evaluation.
//!
//! Handles TypeScript's index access types: `T[K]`
//! Including property access, array indexing, and tuple indexing.

use crate::solver::{ApparentMemberKind, TypeDatabase};
use crate::solver::subtype::TypeResolver;
use crate::solver::types::*;
use crate::solver::utils;
use crate::solver::visitor::{
    TypeVisitor, array_element_type, literal_number, literal_string, tuple_list_id, union_list_id,
};

use super::apparent::make_apparent_method_type;
use super::super::evaluate::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID, TypeEvaluator,
};

fn is_member(name: &str, list: &[&str]) -> bool {
    list.contains(&name)
}

/// Standalone helper to get array member kind.
/// Extracted from TypeEvaluator to be usable by visitors.
pub(crate) fn get_array_member_kind(name: &str) -> Option<ApparentMemberKind> {
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

struct IndexAccessVisitor<'a, 'b, R: TypeResolver> {
    evaluator: &'b mut TypeEvaluator<'a, R>,
    object_type: TypeId,
    index_type: TypeId,
}

impl<'a, 'b, R: TypeResolver> IndexAccessVisitor<'a, 'b, R> {
    fn evaluate_apparent_primitive(&mut self, kind: IntrinsicKind) -> Option<TypeId> {
        match kind {
            IntrinsicKind::String
            | IntrinsicKind::Number
            | IntrinsicKind::Boolean
            | IntrinsicKind::Bigint
            | IntrinsicKind::Symbol => {
                let shape = self.evaluator.apparent_primitive_shape(kind);
                Some(
                    self.evaluator
                        .evaluate_object_with_index(&shape, self.index_type),
                )
            }
            _ => None,
        }
    }

    fn evaluate_type_param(&mut self, param: &TypeParamInfo) -> Option<TypeId> {
        if let Some(constraint) = param.constraint {
            if constraint == self.object_type {
                Some(
                    self.evaluator
                        .interner()
                        .intern(TypeKey::IndexAccess(self.object_type, self.index_type)),
                )
            } else {
                Some(
                    self.evaluator
                        .recurse_index_access(constraint, self.index_type),
                )
            }
        } else {
            Some(
                self.evaluator
                    .interner()
                    .intern(TypeKey::IndexAccess(self.object_type, self.index_type)),
            )
        }
    }
}

impl<'a, 'b, R: TypeResolver> TypeVisitor for IndexAccessVisitor<'a, 'b, R> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        self.evaluate_apparent_primitive(kind)
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        self.evaluator
            .apparent_literal_kind(value)
            .and_then(|kind| self.evaluate_apparent_primitive(kind))
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        let shape = self
            .evaluator
            .interner()
            .object_shape(ObjectShapeId(shape_id));
        Some(
            self.evaluator
                .evaluate_object_index(&shape.properties, self.index_type),
        )
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self
            .evaluator
            .interner()
            .object_shape(ObjectShapeId(shape_id));
        Some(
            self.evaluator
                .evaluate_object_with_index(&shape, self.index_type),
        )
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.evaluator.interner().type_list(TypeListId(list_id));
        const MAX_UNION_INDEX_SIZE: usize = 100;
        if members.len() > MAX_UNION_INDEX_SIZE {
            self.evaluator.set_depth_exceeded(true);
            return Some(TypeId::ERROR);
        }
        let mut results = Vec::new();
        for &member in members.iter() {
            if self.evaluator.is_depth_exceeded() {
                return Some(TypeId::ERROR);
            }
            let result = self.evaluator.recurse_index_access(member, self.index_type);
            if result == TypeId::ERROR && self.evaluator.is_depth_exceeded() {
                return Some(TypeId::ERROR);
            }
            if result != TypeId::UNDEFINED || self.evaluator.no_unchecked_indexed_access() {
                results.push(result);
            }
        }
        if results.is_empty() {
            return Some(TypeId::UNDEFINED);
        }
        Some(self.evaluator.interner().union(results))
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        Some(
            self.evaluator
                .evaluate_array_index(element_type, self.index_type),
        )
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        let elements = self.evaluator.interner().tuple_list(TupleListId(list_id));
        Some(
            self.evaluator
                .evaluate_tuple_index(&elements, self.index_type),
        )
    }

    fn visit_ref(&mut self, symbol_ref: u32) -> Self::Output {
        let symbol_ref = SymbolRef(symbol_ref);
        let resolved = self
            .evaluator
            .resolver()
            .resolve_ref(symbol_ref, self.evaluator.interner())?;
        if resolved == self.object_type {
            Some(
                self.evaluator
                    .interner()
                    .intern(TypeKey::IndexAccess(self.object_type, self.index_type)),
            )
        } else {
            Some(
                self.evaluator
                    .recurse_index_access(resolved, self.index_type),
            )
        }
    }

    fn visit_type_parameter(&mut self, param_info: &TypeParamInfo) -> Self::Output {
        self.evaluate_type_param(param_info)
    }

    fn visit_infer(&mut self, param_info: &TypeParamInfo) -> Self::Output {
        self.evaluate_type_param(param_info)
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        Some(
            self.evaluator
                .recurse_index_access(inner_type, self.index_type),
        )
    }

    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        self.evaluate_apparent_primitive(IntrinsicKind::String)
    }

    fn default_output() -> Self::Output {
        None
    }
}

// =============================================================================
// Visitor Pattern Implementations for Index Type Evaluation
// =============================================================================

/// Visitor to handle array index access: `Array[K]`
///
/// Evaluates what type is returned when indexing an array with various key types.
/// Uses Option<TypeId> to signal "use default fallback" via None.
struct ArrayKeyVisitor<'a> {
    db: &'a dyn TypeDatabase,
    element_type: TypeId,
    array_member_types_cache: Option<Vec<TypeId>>,
}

impl<'a> ArrayKeyVisitor<'a> {
    fn new(db: &'a dyn TypeDatabase, element_type: TypeId) -> Self {
        Self {
            db,
            element_type,
            array_member_types_cache: None,
        }
    }

    /// Driver method that handles the fallback logic
    fn evaluate(&mut self, index_type: TypeId) -> TypeId {
        let result = self.visit_type(self.db, index_type);
        result.unwrap_or(self.element_type)
    }

    fn get_array_member_types(&mut self) -> Vec<TypeId> {
        self.array_member_types_cache.get_or_insert_with(|| {
            vec![
                TypeId::NUMBER,
                make_apparent_method_type(self.db, TypeId::ANY),
                make_apparent_method_type(self.db, TypeId::BOOLEAN),
                make_apparent_method_type(self.db, TypeId::NUMBER),
                make_apparent_method_type(self.db, TypeId::VOID),
                make_apparent_method_type(self.db, TypeId::STRING),
            ]
        })
        .clone()
    }
}

impl<'a> TypeVisitor for ArrayKeyVisitor<'a> {
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
            IntrinsicKind::Number => Some(self.element_type),
            IntrinsicKind::String => Some(self.db.union(self.get_array_member_types())),
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
                // Check for known array members
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
            // For other literals, signal "use default fallback"
            _ => None,
        }
    }

    /// Signal "use the default fallback" for unhandled type variants
    fn default_output() -> Self::Output {
        None
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Helper to recursively evaluate an index access while respecting depth limits.
    /// Creates an IndexAccess type and evaluates it through the main evaluate() method.
    pub(crate) fn recurse_index_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        let index_access = self
            .interner()
            .intern(TypeKey::IndexAccess(object_type, index_type));
        self.evaluate(index_access)
    }

    /// Evaluate an index access type: T[K]
    ///
    /// This resolves property access on object types.
    pub fn evaluate_index_access(&mut self, object_type: TypeId, index_type: TypeId) -> TypeId {
        let evaluated_object = self.evaluate(object_type);
        let evaluated_index = self.evaluate(index_type);
        if evaluated_object != object_type || evaluated_index != index_type {
            // Use recurse_index_access to respect depth limits
            return self.recurse_index_access(evaluated_object, evaluated_index);
        }
        // Match tsc: index access involving `any` produces `any`.
        // (e.g. `any[string]` is `any`, not an error)
        if evaluated_object == TypeId::ANY || evaluated_index == TypeId::ANY {
            return TypeId::ANY;
        }

        // Rule #38: Distribute over index union at the top level (Cartesian product expansion)
        // T[A | B] -> T[A] | T[B]
        // This must happen before checking the object type to ensure full cross-product expansion
        // when both object and index are unions: (X | Y)[A | B] -> X[A] | X[B] | Y[A] | Y[B]
        if let Some(members_id) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members_id);
            // Limit to prevent OOM with large unions
            const MAX_UNION_INDEX_SIZE: usize = 100;
            if members.len() > MAX_UNION_INDEX_SIZE {
                self.set_depth_exceeded(true);
                return TypeId::ERROR;
            }
            let mut results = Vec::new();
            for &member in members.iter() {
                if self.is_depth_exceeded() {
                    return TypeId::ERROR;
                }
                let result = self.recurse_index_access(object_type, member);
                if result == TypeId::ERROR && self.is_depth_exceeded() {
                    return TypeId::ERROR;
                }
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        let interner = self.interner();
        let mut visitor = IndexAccessVisitor {
            evaluator: self,
            object_type,
            index_type,
        };
        if let Some(result) = visitor.visit_type(interner, object_type) {
            return result;
        }

        // For other types, keep as IndexAccess (deferred)
        self.interner()
            .intern(TypeKey::IndexAccess(object_type, index_type))
    }

    /// Evaluate property access on an object type
    pub(crate) fn evaluate_object_index(
        &self,
        props: &[PropertyInfo],
        index_type: TypeId,
    ) -> TypeId {
        // If index is a literal string, look up the property directly
        if let Some(name) = literal_string(self.interner(), index_type) {
            for prop in props {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            // Property not found
            return TypeId::UNDEFINED;
        }

        // If index is a union of literals, return union of property types
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_object_index(props, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is string, return union of all property types (index signature behavior)
        if index_type == TypeId::STRING {
            let union = self.union_property_types(props);
            return self.add_undefined_if_unchecked(union);
        }

        TypeId::UNDEFINED
    }

    /// Evaluate property access on an object type with index signatures.
    pub(crate) fn evaluate_object_with_index(
        &self,
        shape: &ObjectShape,
        index_type: TypeId,
    ) -> TypeId {
        // If index is a union, evaluate each member
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_object_with_index(shape, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is a literal string, look up the property first, then fallback to string index.
        if let Some(name) = literal_string(self.interner(), index_type) {
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
            if let Some(string_index) = shape.string_index.as_ref() {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // If index is a literal number, prefer number index, then string index.
        if literal_number(self.interner(), index_type).is_some() {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = shape.string_index.as_ref() {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        if index_type == TypeId::STRING {
            let result = if let Some(string_index) = shape.string_index.as_ref() {
                string_index.value_type
            } else {
                self.union_property_types(&shape.properties)
            };
            return self.add_undefined_if_unchecked(result);
        }

        if index_type == TypeId::NUMBER {
            let result = if let Some(number_index) = shape.number_index.as_ref() {
                number_index.value_type
            } else if let Some(string_index) = shape.string_index.as_ref() {
                string_index.value_type
            } else {
                self.union_property_types(&shape.properties)
            };
            return self.add_undefined_if_unchecked(result);
        }

        TypeId::UNDEFINED
    }

    pub(crate) fn union_property_types(&self, props: &[PropertyInfo]) -> TypeId {
        let all_types: Vec<TypeId> = props
            .iter()
            .map(|prop| self.optional_property_type(prop))
            .collect();
        if all_types.is_empty() {
            TypeId::UNDEFINED
        } else {
            self.interner().union(all_types)
        }
    }

    pub(crate) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner().union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    pub(crate) fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access() || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner().union2(type_id, TypeId::UNDEFINED)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        if let Some(elem) = array_element_type(self.interner(), type_id) {
            return elem;
        }
        if let Some(elements) = tuple_list_id(self.interner(), type_id) {
            let elements = self.interner().tuple_list(elements);
            let types: Vec<TypeId> = elements
                .iter()
                .map(|e| self.tuple_element_type(e))
                .collect();
            if types.is_empty() {
                TypeId::NEVER
            } else {
                self.interner().union(types)
            }
        } else {
            type_id
        }
    }

    pub(crate) fn tuple_element_type(&self, element: &TupleElement) -> TypeId {
        let mut type_id = if element.rest {
            self.rest_element_type(element.type_id)
        } else {
            element.type_id
        };

        if element.optional {
            type_id = self.interner().union2(type_id, TypeId::UNDEFINED);
        }

        type_id
    }

    fn tuple_index_literal(&self, elements: &[TupleElement], idx: usize) -> Option<TypeId> {
        for (logical_idx, element) in elements.iter().enumerate() {
            if element.rest {
                if let Some(rest_elements) = tuple_list_id(self.interner(), element.type_id) {
                    let rest_elements = self.interner().tuple_list(rest_elements);
                    let inner_idx = idx.saturating_sub(logical_idx);
                    return self.tuple_index_literal(&rest_elements, inner_idx);
                }
                return Some(self.tuple_element_type(element));
            }

            if logical_idx == idx {
                return Some(self.tuple_element_type(element));
            }
        }

        None
    }

    /// Evaluate index access on a tuple type
    pub(crate) fn evaluate_tuple_index(
        &self,
        elements: &[TupleElement],
        index_type: TypeId,
    ) -> TypeId {
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_tuple_index(elements, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is a literal number, return the specific element
        if let Some(n) = literal_number(self.interner(), index_type) {
            let value = n.0;
            if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                return TypeId::UNDEFINED;
            }
            let idx = value as usize;
            return self
                .tuple_index_literal(elements, idx)
                .unwrap_or(TypeId::UNDEFINED);
        }

        if index_type == TypeId::STRING {
            let mut types: Vec<TypeId> = elements
                .iter()
                .map(|e| self.tuple_element_type(e))
                .collect();
            types.extend(self.array_member_types());
            if types.is_empty() {
                return TypeId::NEVER;
            }
            let union = self.interner().union(types);
            return self.add_undefined_if_unchecked(union);
        }

        if let Some(name) = literal_string(self.interner(), index_type) {
            if utils::is_numeric_property_name(self.interner(), name) {
                let name_str = self.interner().resolve_atom_ref(name);
                if let Ok(idx) = name_str.as_ref().parse::<i64>()
                    && let Ok(idx) = usize::try_from(idx)
                {
                    return self
                        .tuple_index_literal(elements, idx)
                        .unwrap_or(TypeId::UNDEFINED);
                }
                return TypeId::UNDEFINED;
            }

            let name_str = self.interner().resolve_atom_ref(name);
            if let Some(member) = self.array_member_kind(name_str.as_ref()) {
                return match member {
                    ApparentMemberKind::Value(type_id) => type_id,
                    ApparentMemberKind::Method(return_type) => {
                        self.apparent_method_type(return_type)
                    }
                };
            }

            return TypeId::UNDEFINED;
        }

        // If index is number, return union of all element types
        if index_type == TypeId::NUMBER {
            let all_types: Vec<TypeId> = elements
                .iter()
                .map(|e| self.tuple_element_type(e))
                .collect();
            if all_types.is_empty() {
                return TypeId::NEVER;
            }
            let union = self.interner().union(all_types);
            return self.add_undefined_if_unchecked(union);
        }

        TypeId::UNDEFINED
    }

    /// Check if a type is number-like (number or numeric literal)
    pub(crate) fn is_number_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::NUMBER {
            return true;
        }
        literal_number(self.interner(), type_id).is_some()
    }

    pub(crate) fn evaluate_array_index(&self, elem: TypeId, index_type: TypeId) -> TypeId {
        // Handle Union explicitly (distributes over union members)
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_array_index(elem, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // Use ArrayKeyVisitor to handle the index type
        let mut visitor = ArrayKeyVisitor::new(self.interner(), elem);
        let result = visitor.evaluate(index_type);

        // Add undefined if unchecked indexed access is allowed
        self.add_undefined_if_unchecked(result)
    }

    pub(crate) fn array_member_types(&self) -> Vec<TypeId> {
        vec![
            TypeId::NUMBER,
            self.apparent_method_type(TypeId::ANY),
            self.apparent_method_type(TypeId::BOOLEAN),
            self.apparent_method_type(TypeId::NUMBER),
            self.apparent_method_type(TypeId::UNDEFINED),
            self.apparent_method_type(TypeId::STRING),
        ]
    }

    pub(crate) fn array_member_kind(&self, name: &str) -> Option<ApparentMemberKind> {
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
}
