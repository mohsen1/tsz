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
            // For Application types (e.g., generic type aliases like Destructuring<TFuncs1, T>),
            // unwrap to the base type and get parameter type from it
            // This fixes TS2571 false positives where arrow function parameters are typed as UNKNOWN
            // instead of the actual type from the Application
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                // Recursively get parameter type from the base type
                let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
                ctx.get_parameter_type(index)
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
            // For Application types, unwrap to the base type
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
                ctx.get_parameter_type_for_call(index, arg_count)
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
            // For Application types, unwrap to the base type
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
                ctx.get_this_type()
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
            // For Application types, unwrap to the base type
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
                ctx.get_return_type()
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
                    } else if let Some(last_elem) = elements.last()
                        && last_elem.rest
                    {
                        return Some(last_elem.type_id);
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
                    } else if let Some(last_elem) = elements.last()
                        && last_elem.rest
                    {
                        return Some(last_elem.type_id);
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

    /// Get the contextual yield type for a generator function.
    ///
    /// If the expected type is `Generator<Y, R, N>`, this returns Y.
    /// This is used to contextually type yield expressions.
    ///
    /// Example:
    /// ```typescript
    /// function* gen(): Generator<number, void, unknown> {
    ///     yield 1;  // 1 is contextually typed as number
    /// }
    /// ```
    pub fn get_generator_yield_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                // Generator<Y, R, N> has Y as the first type argument
                if !app.args.is_empty() {
                    Some(app.args[0])
                } else {
                    None
                }
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let yield_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_generator_yield_type()
                    })
                    .collect();

                if yield_types.is_empty() {
                    None
                } else if yield_types.len() == 1 {
                    Some(yield_types[0])
                } else {
                    Some(self.interner.union(yield_types))
                }
            }
            _ => None,
        }
    }

    /// Get the contextual return type for a generator function (TReturn from Generator<Y, TReturn, N>).
    ///
    /// This is used to contextually type return statements in generators.
    pub fn get_generator_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                // Generator<Y, R, N> has R as the second type argument
                if app.args.len() >= 2 {
                    Some(app.args[1])
                } else {
                    None
                }
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let return_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_generator_return_type()
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

    /// Get the contextual next type for a generator function (TNext from Generator<Y, R, TNext>).
    ///
    /// This is used to determine the type of values passed to .next() and
    /// the type of the yield expression result.
    pub fn get_generator_next_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let key = self.interner.lookup(expected)?;

        match key {
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                // Generator<Y, R, N> has N as the third type argument
                if app.args.len() >= 3 {
                    Some(app.args[2])
                } else {
                    None
                }
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let next_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_generator_next_type()
                    })
                    .collect();

                if next_types.is_empty() {
                    None
                } else if next_types.len() == 1 {
                    Some(next_types[0])
                } else {
                    Some(self.interner.union(next_types))
                }
            }
            _ => None,
        }
    }

    /// Create a child context for a yield expression in a generator.
    pub fn for_yield(&self) -> ContextualTypeContext<'a> {
        match self.get_generator_yield_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }
}

/// Apply contextual type to infer a more specific type.
///
/// This implements bidirectional type inference:
/// 1. If expr_type is any/unknown/error, use contextual type
/// 2. If expr_type is a literal and contextual type is a union containing that literal's base type, preserve literal
/// 3. If expr_type is assignable to contextual type and is more specific, use expr_type
/// 4. Otherwise, prefer expr_type (don't widen to contextual type)
pub fn apply_contextual_type(
    interner: &dyn TypeDatabase,
    expr_type: TypeId,
    contextual_type: Option<TypeId>,
) -> TypeId {
    let ctx_type = match contextual_type {
        Some(t) => t,
        None => return expr_type,
    };

    // If expression type is any, unknown, or error, use contextual type
    if expr_type == TypeId::ANY || expr_type == TypeId::UNKNOWN || expr_type == TypeId::ERROR {
        return ctx_type;
    }

    // If expression type is the same, just return it
    if expr_type == ctx_type {
        return expr_type;
    }

    // Check if expr_type is a literal type that should be preserved
    // When contextual type is a union like string | number, we should preserve literal types
    if let Some(expr_key) = interner.lookup(expr_type) {
        // Literal types should be preserved when context is a union
        if matches!(expr_key, TypeKey::Literal(_)) {
            if let Some(ctx_key) = interner.lookup(ctx_type) {
                if matches!(ctx_key, TypeKey::Union(_)) {
                    // Preserve the literal type - it's more specific than the union
                    return expr_type;
                }
            }
        }
    }

    // Check if contextual type is a union
    if let Some(TypeKey::Union(members)) = interner.lookup(ctx_type) {
        let members = interner.type_list(members);
        // If expr_type is in the union, it's valid - use the more specific expr_type
        for &member in members.iter() {
            if member == expr_type {
                return expr_type;
            }
        }
        // If expr_type is assignable to any union member, use expr_type
        for &member in members.iter() {
            if is_subtype_of(interner, expr_type, member) {
                return expr_type;
            }
        }
    }

    // If expr_type is assignable to contextual type, use expr_type (it's more specific)
    if is_subtype_of(interner, expr_type, ctx_type) {
        return expr_type;
    }

    // If contextual type is assignable to expr_type, use contextual type (it's more specific)
    if is_subtype_of(interner, ctx_type, expr_type) {
        return ctx_type;
    }

    // Default: prefer the expression type (don't widen to contextual type)
    // This prevents incorrectly widening concrete types to generic type parameters
    expr_type
}

/// Check if a type is a subtype of another type.
fn is_subtype_of(interner: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    use crate::solver::subtype::is_subtype_of;
    is_subtype_of(interner, source, target)
}

/// Context for yield expression contextual typing in generators.
///
/// When checking a yield expression in a generator function, we need
/// to extract the expected yield type from the function's return type.
pub struct GeneratorContextualType<'a> {
    interner: &'a dyn TypeDatabase,
    /// The generator return type (e.g., Generator<number, void, unknown>)
    generator_type: Option<TypeId>,
}

impl<'a> GeneratorContextualType<'a> {
    /// Create a new generator contextual type context.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        GeneratorContextualType {
            interner,
            generator_type: None,
        }
    }

    /// Create a context with a generator type.
    pub fn with_generator(interner: &'a dyn TypeDatabase, generator_type: TypeId) -> Self {
        GeneratorContextualType {
            interner,
            generator_type: Some(generator_type),
        }
    }

    /// Get the contextual yield type from the generator.
    ///
    /// For Generator<Y, R, N> or AsyncGenerator<Y, R, N>, this returns Y.
    /// This is used for contextual typing of yield expressions.
    pub fn get_yield_type(&self) -> Option<TypeId> {
        let gen_type = self.generator_type?;
        self.extract_yield_type_from_generator(gen_type)
    }

    /// Get the contextual next type from the generator.
    ///
    /// For Generator<Y, R, N> or AsyncGenerator<Y, R, N>, this returns N.
    /// This is used for the type of the yield expression result.
    pub fn get_next_type(&self) -> Option<TypeId> {
        let gen_type = self.generator_type?;
        self.extract_next_type_from_generator(gen_type)
    }

    /// Get the contextual return type from the generator.
    ///
    /// For Generator<Y, R, N> or AsyncGenerator<Y, R, N>, this returns R.
    /// This is used for contextual typing of return statements.
    pub fn get_return_type(&self) -> Option<TypeId> {
        let gen_type = self.generator_type?;
        self.extract_return_type_from_generator(gen_type)
    }

    /// Extract yield type (Y) from Generator<Y, R, N> structure.
    ///
    /// We look for the 'next' method's return type, which should be
    /// IteratorResult<Y, R> or Promise<IteratorResult<Y, R>>.
    fn extract_yield_type_from_generator(&self, gen_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(gen_type)?;

        match key {
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    let prop_name = self.interner.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "next" && prop.is_method {
                        // Extract yield type from the method return type
                        return self.extract_yield_from_next_method(prop.type_id);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Extract yield type from next method's return type.
    ///
    /// The return type is either:
    /// - IteratorResult<Y, R> for sync generators
    /// - Promise<IteratorResult<Y, R>> for async generators
    fn extract_yield_from_next_method(&self, method_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(method_type)?;

        if let TypeKey::Function(func_id) = key {
            let func = self.interner.function_shape(func_id);
            let return_type = func.return_type;

            // Check if it's a Promise wrapper (async generator)
            if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(return_type) {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    let prop_name = self.interner.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "then" {
                        // Async generator: unwrap Promise and get IteratorResult value
                        return self.extract_value_from_iterator_result(prop.type_id);
                    }
                    if prop_name.as_ref() == "value" {
                        // Sync generator: directly get value
                        return Some(prop.type_id);
                    }
                }
            }

            // Check for union type (IteratorResult = {value: Y, done: false} | {value: R, done: true})
            if let Some(TypeKey::Union(_)) = self.interner.lookup(return_type) {
                return self.extract_value_from_iterator_result(return_type);
            }
        }
        None
    }

    /// Extract the 'value' property from an IteratorResult<Y, R> type.
    fn extract_value_from_iterator_result(&self, result_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(result_type)?;

        match key {
            TypeKey::Union(list_id) => {
                // Get first member (yield result) and extract value
                let members = self.interner.type_list(list_id);
                if let Some(&first) = members.first() {
                    return self.extract_value_property(first);
                }
                None
            }
            TypeKey::Object(_) => self.extract_value_property(result_type),
            _ => None,
        }
    }

    /// Extract 'value' property from an object type.
    fn extract_value_property(&self, obj_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(obj_type)?;

        if let TypeKey::Object(shape_id) = key {
            let shape = self.interner.object_shape(shape_id);
            for prop in &shape.properties {
                let prop_name = self.interner.resolve_atom_ref(prop.name);
                if prop_name.as_ref() == "value" {
                    return Some(prop.type_id);
                }
            }
        }
        None
    }

    /// Extract next type (N) from Generator<Y, R, N> structure.
    fn extract_next_type_from_generator(&self, gen_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(gen_type)?;

        if let TypeKey::Object(shape_id) = key {
            let shape = self.interner.object_shape(shape_id);
            for prop in &shape.properties {
                let prop_name = self.interner.resolve_atom_ref(prop.name);
                if prop_name.as_ref() == "next" && prop.is_method {
                    // Extract next type from the method's first parameter
                    return self.extract_next_from_method(prop.type_id);
                }
            }
        }
        None
    }

    /// Extract next type from next method's parameter.
    fn extract_next_from_method(&self, method_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(method_type)?;

        if let TypeKey::Function(func_id) = key {
            let func = self.interner.function_shape(func_id);
            if let Some(first_param) = func.params.first() {
                return Some(first_param.type_id);
            }
        }
        None
    }

    /// Extract return type (R) from Generator<Y, R, N> structure.
    fn extract_return_type_from_generator(&self, gen_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(gen_type)?;

        if let TypeKey::Object(shape_id) = key {
            let shape = self.interner.object_shape(shape_id);
            for prop in &shape.properties {
                let prop_name = self.interner.resolve_atom_ref(prop.name);
                if prop_name.as_ref() == "return" && prop.is_method {
                    // Extract return type from the method's parameter
                    return self.extract_return_from_method(prop.type_id);
                }
            }
        }
        None
    }

    /// Extract return type from return method's parameter.
    fn extract_return_from_method(&self, method_type: TypeId) -> Option<TypeId> {
        let key = self.interner.lookup(method_type)?;

        if let TypeKey::Function(func_id) = key {
            let func = self.interner.function_shape(func_id);
            if let Some(first_param) = func.params.first() {
                return Some(first_param.type_id);
            }
        }
        None
    }
}

/// Check if a type is an async generator type.
///
/// An async generator has a 'next' method that returns Promise<IteratorResult<Y, R>>.
pub fn is_async_generator_type(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(key) = interner.lookup(type_id) else {
        return false;
    };

    if let TypeKey::Object(shape_id) = key {
        let shape = interner.object_shape(shape_id);
        for prop in &shape.properties {
            let prop_name = interner.resolve_atom_ref(prop.name);
            if prop_name.as_ref() == "next" && prop.is_method {
                // Check if return type is a Promise (has 'then' property)
                if let Some(TypeKey::Function(func_id)) = interner.lookup(prop.type_id) {
                    let func = interner.function_shape(func_id);
                    if let Some(TypeKey::Object(ret_shape_id)) = interner.lookup(func.return_type) {
                        let ret_shape = interner.object_shape(ret_shape_id);
                        for ret_prop in &ret_shape.properties {
                            let ret_prop_name = interner.resolve_atom_ref(ret_prop.name);
                            if ret_prop_name.as_ref() == "then" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Check if a type is a sync generator type.
///
/// A sync generator has a 'next' method that returns IteratorResult<Y, R> directly.
pub fn is_sync_generator_type(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(key) = interner.lookup(type_id) else {
        return false;
    };

    if let TypeKey::Object(shape_id) = key {
        let shape = interner.object_shape(shape_id);
        for prop in &shape.properties {
            let prop_name = interner.resolve_atom_ref(prop.name);
            if prop_name.as_ref() == "next" && prop.is_method {
                // Check if return type is NOT a Promise (no 'then' property)
                if let Some(TypeKey::Function(func_id)) = interner.lookup(prop.type_id) {
                    let func = interner.function_shape(func_id);
                    // If return type is a union or object without 'then', it's a sync generator
                    if let Some(TypeKey::Object(ret_shape_id)) = interner.lookup(func.return_type) {
                        let ret_shape = interner.object_shape(ret_shape_id);
                        let has_then = ret_shape
                            .properties
                            .iter()
                            .any(|p| interner.resolve_atom_ref(p.name).as_ref() == "then");
                        if !has_then {
                            return true;
                        }
                    }
                    if let Some(TypeKey::Union(_)) = interner.lookup(func.return_type) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
#[path = "contextual_tests.rs"]
mod tests;
