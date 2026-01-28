//! Index access type evaluation.
//!
//! Handles TypeScript's index access types: `T[K]`
//! Including property access, array indexing, and tuple indexing.

use crate::solver::ApparentMemberKind;
use crate::solver::subtype::TypeResolver;
use crate::solver::types::*;
use crate::solver::utils;

use super::super::evaluate::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID, TypeEvaluator,
};

fn is_member(name: &str, list: &[&str]) -> bool {
    list.contains(&name)
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Helper to recursively evaluate an index access while respecting depth limits.
    /// Creates an IndexAccess type and evaluates it through the main evaluate() method.
    pub(crate) fn recurse_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        let index_access = self
            .interner()
            .intern(TypeKey::IndexAccess(object_type, index_type));
        self.evaluate(index_access)
    }

    /// Evaluate an index access type: T[K]
    ///
    /// This resolves property access on object types.
    pub fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
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
        if let Some(TypeKey::Union(members_id)) = self.interner().lookup(index_type) {
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

        // Get the object structure
        let obj_key = match self.interner().lookup(object_type) {
            Some(k) => k,
            None => return TypeId::ERROR,
        };

        if let Some(shape) = self.apparent_primitive_shape_for_key(&obj_key) {
            return self.evaluate_object_with_index(&shape, index_type);
        }

        match obj_key {
            TypeKey::ReadonlyType(inner) => self.recurse_index_access(inner, index_type),
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver().resolve_ref(sym, self.interner()) {
                    if resolved == object_type {
                        self.interner()
                            .intern(TypeKey::IndexAccess(object_type, index_type))
                    } else {
                        self.recurse_index_access(resolved, index_type)
                    }
                } else {
                    TypeId::ERROR
                }
            }
            TypeKey::TypeParameter(param) | TypeKey::Infer(param) => {
                if let Some(constraint) = param.constraint {
                    if constraint == object_type {
                        self.interner()
                            .intern(TypeKey::IndexAccess(object_type, index_type))
                    } else {
                        self.recurse_index_access(constraint, index_type)
                    }
                } else {
                    self.interner()
                        .intern(TypeKey::IndexAccess(object_type, index_type))
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                self.evaluate_object_index(&shape.properties, index_type)
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                self.evaluate_object_with_index(&shape, index_type)
            }
            TypeKey::Union(members) => {
                let members = self.interner().type_list(members);
                // Limit to prevent OOM with large unions
                const MAX_UNION_INDEX_SIZE: usize = 100;
                if members.len() > MAX_UNION_INDEX_SIZE {
                    self.set_depth_exceeded(true);
                    return TypeId::ERROR;
                }
                let mut results = Vec::new();
                for &member in members.iter() {
                    // Check if depth was exceeded during evaluation
                    if self.is_depth_exceeded() {
                        return TypeId::ERROR;
                    }
                    // Use recurse_index_access to respect depth limits
                    let result = self.recurse_index_access(member, index_type);
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
                self.interner().union(results)
            }
            TypeKey::Array(elem) => self.evaluate_array_index(elem, index_type),
            TypeKey::Tuple(elements) => {
                let elements = self.interner().tuple_list(elements);
                self.evaluate_tuple_index(&elements, index_type)
            }
            // For other types, keep as IndexAccess (deferred)
            _ => self
                .interner()
                .intern(TypeKey::IndexAccess(object_type, index_type)),
        }
    }

    /// Evaluate property access on an object type
    pub(crate) fn evaluate_object_index(
        &self,
        props: &[PropertyInfo],
        index_type: TypeId,
    ) -> TypeId {
        // If index is a literal string, look up the property directly
        if let Some(TypeKey::Literal(LiteralValue::String(name))) =
            self.interner().lookup(index_type)
        {
            for prop in props {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            // Property not found
            return TypeId::UNDEFINED;
        }

        // If index is a union of literals, return union of property types
        if let Some(TypeKey::Union(members)) = self.interner().lookup(index_type) {
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
        if let Some(TypeKey::Union(members)) = self.interner().lookup(index_type) {
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
        if let Some(TypeKey::Literal(LiteralValue::String(name))) =
            self.interner().lookup(index_type)
        {
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
        if let Some(TypeKey::Literal(LiteralValue::Number(_))) = self.interner().lookup(index_type)
        {
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
        match self.interner().lookup(type_id) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(elements)) => {
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
            }
            _ => type_id,
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
                match self.interner().lookup(element.type_id) {
                    Some(TypeKey::Tuple(rest_elements)) => {
                        let rest_elements = self.interner().tuple_list(rest_elements);
                        let inner_idx = idx.saturating_sub(logical_idx);
                        return self.tuple_index_literal(&rest_elements, inner_idx);
                    }
                    _ => {
                        return Some(self.tuple_element_type(element));
                    }
                }
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
        if let Some(TypeKey::Union(members)) = self.interner().lookup(index_type) {
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
        if let Some(TypeKey::Literal(LiteralValue::Number(n))) = self.interner().lookup(index_type)
        {
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

        if let Some(TypeKey::Literal(LiteralValue::String(name))) =
            self.interner().lookup(index_type)
        {
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
        if let Some(TypeKey::Literal(LiteralValue::Number(_))) = self.interner().lookup(type_id) {
            return true;
        }
        false
    }

    pub(crate) fn evaluate_array_index(&self, elem: TypeId, index_type: TypeId) -> TypeId {
        if let Some(TypeKey::Union(members)) = self.interner().lookup(index_type) {
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

        if self.is_number_like(index_type) {
            return self.add_undefined_if_unchecked(elem);
        }

        if index_type == TypeId::STRING {
            let union = self.interner().union(self.array_member_types());
            return self.add_undefined_if_unchecked(union);
        }

        if let Some(TypeKey::Literal(LiteralValue::String(name))) =
            self.interner().lookup(index_type)
        {
            if utils::is_numeric_property_name(self.interner(), name) {
                return self.add_undefined_if_unchecked(elem);
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

        elem
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
