//! Contextual typing (reverse inference).
//!
//! Contextual typing allows type information to flow "backwards" from
//! an expected type to an expression. This is used for:
//! - Arrow function parameters: `const f: (x: string) => void = (x) => ...`
//! - Array literals: `const arr: number[] = [1, 2, 3]`
//! - Object literals: `const obj: {x: number} = {x: 1}`
//!
//! The key insight is that when we have an expected type, we can use it
//! to infer types for parts of the expression that would otherwise be unknown.

use crate::solver::TypeDatabase;
use crate::solver::types::*;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Context for contextual typing.
/// Holds the expected type and provides methods to extract type information.
pub struct ContextualTypeContext<'a> {
    interner: &'a dyn TypeDatabase,
    /// The expected type (contextual type)
    expected: Option<TypeId>,
}

impl<'a> ContextualTypeContext<'a> {
    /// Create a new contextual type context.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        ContextualTypeContext {
            interner,
            expected: None,
        }
    }

    /// Create a context with an expected type.
    pub fn with_expected(interner: &'a dyn TypeDatabase, expected: TypeId) -> Self {
        ContextualTypeContext {
            interner,
            expected: Some(expected),
        }
    }

    /// Get the expected type.
    pub fn expected(&self) -> Option<TypeId> {
        self.expected
    }

    /// Check if we have a contextual type.
    pub fn has_context(&self) -> bool {
        self.expected.is_some()
    }

    /// Get the contextual type for a function parameter at the given index.
    ///
    /// Example:
    /// ```typescript
    /// type Handler = (e: string, i: number) => void;
    /// const h: Handler = (x, y) => {};  // x: string, y: number from context
    /// ```
    pub fn get_parameter_type(&self, index: usize) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                self.get_parameter_type_from_params(&shape.params, index)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                self.get_parameter_type_from_signatures(&shape.call_signatures, index)
            }
            // For union of function types, try to find common parameter type
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let param_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_parameter_type(index)
                    })
                    .collect();

                if param_types.is_empty() {
                    None
                } else if param_types.len() == 1 {
                    Some(param_types[0])
                } else {
                    // Union of parameter types
                    Some(self.interner.union(param_types))
                }
            }
            _ => None,
        }
    }

    /// Get the contextual type for a call argument at the given index and arity.
    pub fn get_parameter_type_for_call(&self, index: usize, arg_count: usize) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                self.get_parameter_type_from_params(&shape.params, index)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                self.get_parameter_type_from_signatures_for_call(
                    &shape.call_signatures,
                    index,
                    arg_count,
                )
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let param_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_parameter_type_for_call(index, arg_count)
                    })
                    .collect();

                if param_types.is_empty() {
                    None
                } else if param_types.len() == 1 {
                    Some(param_types[0])
                } else {
                    Some(self.interner.union(param_types))
                }
            }
            _ => None,
        }
    }

    /// Get the contextual type for a `this` parameter, if present on the expected type.
    pub fn get_this_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Function(shape_id) => self.interner.function_shape(shape_id).this_type,
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                self.get_this_type_from_signatures(&shape.call_signatures)
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let this_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_this_type()
                    })
                    .collect();

                if this_types.is_empty() {
                    None
                } else if this_types.len() == 1 {
                    Some(this_types[0])
                } else {
                    Some(self.interner.union(this_types))
                }
            }
            _ => None,
        }
    }

    /// Get the contextual return type for a function.
    pub fn get_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Function(shape_id) => Some(self.interner.function_shape(shape_id).return_type),
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                self.get_return_type_from_signatures(&shape.call_signatures)
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let return_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_return_type()
                    })
                    .collect();

                if return_types.is_empty() {
                    None
                } else if return_types.len() == 1 {
                    Some(return_types[0])
                } else {
                    Some(self.interner.union(return_types))
                }
            }
            _ => None,
        }
    }

    /// Get the contextual element type for an array.
    ///
    /// Example:
    /// ```typescript
    /// const arr: number[] = [1, 2, 3];  // elements are contextually typed as number
    /// ```
    pub fn get_array_element_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Array(elem) => Some(elem),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                if elements.is_empty() {
                    None
                } else {
                    let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                    Some(self.interner.union(types))
                }
            }
            _ => None,
        }
    }

    /// Get the contextual type for a specific tuple element.
    pub fn get_tuple_element_type(&self, index: usize) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                if index < elements.len() {
                    Some(elements[index].type_id)
                } else if let Some(last) = elements.last() {
                    if last.rest { Some(last.type_id) } else { None }
                } else {
                    None
                }
            }
            TypeKey::Array(elem) => Some(elem),
            _ => None,
        }
    }

    /// Get the contextual type for an object property.
    ///
    /// Example:
    /// ```typescript
    /// const obj: {x: number, y: string} = {x: 1, y: "hi"};
    /// ```
    pub fn get_property_type(&self, name: &str) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    if self.interner.resolve_atom_ref(prop.name).as_ref() == name {
                        return Some(prop.type_id);
                    }
                }
                None
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let prop_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_property_type(name)
                    })
                    .collect();

                if prop_types.is_empty() {
                    None
                } else if prop_types.len() == 1 {
                    Some(prop_types[0])
                } else {
                    Some(self.interner.union(prop_types))
                }
            }
            _ => None,
        }
    }

    /// Create a child context for a nested expression.
    /// This is used when checking nested structures with contextual types.
    pub fn for_property(&self, name: &str) -> ContextualTypeContext<'a> {
        match self.get_property_type(name) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for an array element.
    pub fn for_array_element(&self) -> ContextualTypeContext<'a> {
        match self.get_array_element_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a tuple element at the given index.
    pub fn for_tuple_element(&self, index: usize) -> ContextualTypeContext<'a> {
        match self.get_tuple_element_type(index) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a function parameter at the given index.
    pub fn for_parameter(&self, index: usize) -> ContextualTypeContext<'a> {
        match self.get_parameter_type(index) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a function return expression.
    pub fn for_return(&self) -> ContextualTypeContext<'a> {
        match self.get_return_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Helper to extract parameter type from a list of params.
    fn get_parameter_type_from_params(&self, params: &[ParamInfo], index: usize) -> Option<TypeId> {
        if index < params.len() {
            let param = &params[index];
            if param.rest {
                // Rest parameter - extract element type from array or tuple
                if let Some(TypeKey::Array(elem)) = self.interner.lookup(param.type_id) {
                    return Some(elem);
                }
                // For rest parameter with union type (e.g., union of tuples), extract element at index from each member
                if let Some(TypeKey::Union(members)) = self.interner.lookup(param.type_id) {
                    let members = self.interner.type_list(members);
                    let elem_types: Vec<TypeId> = members
                        .iter()
                        .filter_map(|&m| {
                            let ctx = ContextualTypeContext::with_expected(self.interner, m);
                            ctx.get_tuple_element_type(index)
                        })
                        .collect();
                    if !elem_types.is_empty() {
                        return Some(self.interner.union(elem_types));
                    }
                }
                // For rest parameter with tuple type, extract the element at the given index
                if let Some(TypeKey::Tuple(elements)) = self.interner.lookup(param.type_id) {
                    let elements = self.interner.tuple_list(elements);
                    // Find the tuple element at the given index
                    if index < elements.len() {
                        return Some(elements[index].type_id);
                    } else if let Some(last_elem) = elements.last() {
                        if last_elem.rest {
                            return Some(last_elem.type_id);
                        }
                    }
                }
            }
            Some(param.type_id)
        } else if let Some(last) = params.last() {
            // Index beyond params - check if last is rest
            if last.rest {
                // Extract element type from array or tuple
                if let Some(TypeKey::Array(elem)) = self.interner.lookup(last.type_id) {
                    return Some(elem);
                }
                // For rest parameter with union type (e.g., union of tuples), extract element at index from each member
                if let Some(TypeKey::Union(members)) = self.interner.lookup(last.type_id) {
                    let members = self.interner.type_list(members);
                    let elem_types: Vec<TypeId> = members
                        .iter()
                        .filter_map(|&m| {
                            let ctx = ContextualTypeContext::with_expected(self.interner, m);
                            ctx.get_tuple_element_type(index)
                        })
                        .collect();
                    if !elem_types.is_empty() {
                        return Some(self.interner.union(elem_types));
                    }
                }
                // For rest parameter with tuple type, extract the element at the given index
                if let Some(TypeKey::Tuple(elements)) = self.interner.lookup(last.type_id) {
                    let elements = self.interner.tuple_list(elements);
                    // Find the tuple element at the given index
                    if index < elements.len() {
                        return Some(elements[index].type_id);
                    } else if let Some(last_elem) = elements.last() {
                        if last_elem.rest {
                            return Some(last_elem.type_id);
                        }
                    }
                }
            }
            None
        } else {
            None
        }
    }

    fn get_parameter_type_from_signatures(
        &self,
        signatures: &[CallSignature],
        index: usize,
    ) -> Option<TypeId> {
        let param_types: Vec<TypeId> = signatures
            .iter()
            .filter_map(|sig| self.get_parameter_type_from_params(&sig.params, index))
            .collect();

        if param_types.is_empty() {
            None
        } else if param_types.len() == 1 {
            Some(param_types[0])
        } else {
            Some(self.interner.union(param_types))
        }
    }

    fn get_parameter_type_from_signatures_for_call(
        &self,
        signatures: &[CallSignature],
        index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let mut matched = false;
        let mut param_types: Vec<TypeId> = Vec::new();

        for sig in signatures {
            if self.signature_accepts_arg_count(&sig.params, arg_count) {
                matched = true;
                if let Some(param_type) = self.get_parameter_type_from_params(&sig.params, index) {
                    param_types.push(param_type);
                }
            }
        }

        if param_types.is_empty() && !matched {
            param_types = signatures
                .iter()
                .filter_map(|sig| self.get_parameter_type_from_params(&sig.params, index))
                .collect();
        }

        if param_types.is_empty() {
            None
        } else if param_types.len() == 1 {
            Some(param_types[0])
        } else {
            Some(self.interner.union(param_types))
        }
    }

    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        let mut min = 0usize;
        let mut max = 0usize;
        let mut has_rest = false;

        for param in params {
            if param.rest {
                has_rest = true;
                break;
            }
            max += 1;
            if !param.optional {
                min += 1;
            }
        }

        if arg_count < min {
            return false;
        }
        if has_rest {
            return true;
        }
        arg_count <= max
    }

    fn get_this_type_from_signatures(&self, signatures: &[CallSignature]) -> Option<TypeId> {
        let this_types: Vec<TypeId> = signatures.iter().filter_map(|sig| sig.this_type).collect();

        if this_types.is_empty() {
            None
        } else if this_types.len() == 1 {
            Some(this_types[0])
        } else {
            Some(self.interner.union(this_types))
        }
    }

    fn get_return_type_from_signatures(&self, signatures: &[CallSignature]) -> Option<TypeId> {
        if signatures.is_empty() {
            return None;
        }

        let return_types: Vec<TypeId> = signatures.iter().map(|sig| sig.return_type).collect();
        if return_types.len() == 1 {
            Some(return_types[0])
        } else {
            Some(self.interner.union(return_types))
        }
    }
}

/// Apply contextual type to infer a more specific type.
///
/// If the expression type is compatible with the contextual type,
/// returns the more specific type. Otherwise returns the expression type.
pub fn apply_contextual_type(
    _interner: &dyn TypeDatabase,
    expr_type: TypeId,
    contextual_type: Option<TypeId>,
) -> TypeId {
    let ctx_type = match contextual_type {
        Some(t) => t,
        None => return expr_type,
    };

    // If expression type is any or unknown, use contextual type
    if expr_type == TypeId::ANY || expr_type == TypeId::UNKNOWN {
        return ctx_type;
    }

    // If expression type is the same, just return it
    if expr_type == ctx_type {
        return expr_type;
    }

    // For now, prefer the expression type (intrinsic type wins)
    // More sophisticated: check compatibility and narrow
    expr_type
}

#[cfg(test)]
#[path = "contextual_tests.rs"]
mod tests;
