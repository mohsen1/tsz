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
use crate::solver::visitor::TypeVisitor;

#[cfg(test)]
use crate::solver::TypeInterner;

// =============================================================================
// Visitor Pattern Implementations
// =============================================================================

/// Visitor to extract the `this` type from callable types.
struct ThisTypeExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ThisTypeExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for ThisTypeExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        self.db.function_shape(FunctionShapeId(shape_id)).this_type
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        // Collect this types from all signatures
        let this_types: Vec<TypeId> = shape
            .call_signatures
            .iter()
            .filter_map(|sig| sig.this_type)
            .collect();

        if this_types.is_empty() {
            None
        } else if this_types.len() == 1 {
            Some(this_types[0])
        } else {
            Some(self.db.union(this_types))
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the return type from callable types.
struct ReturnTypeExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ReturnTypeExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for ReturnTypeExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        Some(
            self.db
                .function_shape(FunctionShapeId(shape_id))
                .return_type,
        )
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        // Collect return types from all signatures
        if shape.call_signatures.is_empty() {
            return None;
        }

        let return_types: Vec<TypeId> = shape
            .call_signatures
            .iter()
            .map(|sig| sig.return_type)
            .collect();

        if return_types.len() == 1 {
            Some(return_types[0])
        } else {
            Some(self.db.union(return_types))
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the type T from ThisType<T> utility type markers.
///
/// This handles the Vue 2 / Options API pattern where contextual types contain
/// ThisType<T> markers to override the type of 'this' in object literal methods.
///
/// Example:
/// ```typescript
/// type ObjectDescriptor<D, M> = {
///     methods?: M & ThisType<D & M>;
/// };
/// ```
struct ThisTypeMarkerExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ThisTypeMarkerExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }

    /// Check if a type application is for the ThisType utility.
    fn is_this_type_application(&self, app_id: u32) -> bool {
        let app = self.db.type_application(TypeApplicationId(app_id));

        // CRITICAL: We must NOT return true for all Lazy types!
        // Doing so would break ALL generic type aliases (Partial<T>, Readonly<T>, etc.)
        // We must check if the base type is specifically "ThisType"

        // Check TypeParameter case first (easier - has name directly)
        if let Some(TypeKey::TypeParameter(tp)) = self.db.lookup(app.base) {
            let name = self.db.resolve_atom_ref(tp.name);
            return name.as_ref() == "ThisType";
        }

        // For Lazy types (type aliases), we need to resolve the def_id to a name
        // This is harder without access to the symbol table. For now, we fail safe
        // and return false rather than breaking all type aliases.
        // TODO: When we have access to symbol resolution, check if def_id points to lib.d.ts ThisType
        if let Some(TypeKey::Lazy(_def_id)) = self.db.lookup(app.base) {
            // Cannot safely identify ThisType without symbol table access
            // Return false to avoid breaking other type aliases
            return false;
        }

        false
    }
}

impl<'a> TypeVisitor for ThisTypeMarkerExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        if self.is_this_type_application(app_id) {
            let app = self.db.type_application(TypeApplicationId(app_id));
            // ThisType<T> has exactly one type argument T
            app.args.first().copied()
        } else {
            // Not a ThisType application, recurse into base and args
            let app = self.db.type_application(TypeApplicationId(app_id));
            let base_result = self.visit_type(self.db, app.base);

            // Collect results from all arguments
            let arg_results: Vec<_> = app
                .args
                .iter()
                .filter_map(|&arg_id| self.visit_type(self.db, arg_id))
                .collect();

            // If we found ThisType in arguments, return the first one
            // (ThisType should only appear once in a given type structure)
            if let Some(result) = base_result {
                Some(result)
            } else if let Some(&first) = arg_results.first() {
                Some(first)
            } else {
                None
            }
        }
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));

        // Collect all ThisType markers from the intersection
        let this_types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member_id| self.visit_type(self.db, member_id))
            .collect();

        if this_types.is_empty() {
            None
        } else if this_types.len() == 1 {
            Some(this_types[0])
        } else {
            // Multiple ThisType markers - intersect them
            // ThisType<A> & ThisType<B> => this is A & B
            Some(self.db.intersection(this_types))
        }
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions, we distribute over members
        // (A & ThisType<X>) | (B & ThisType<Y>) should try each member
        let members = self.db.type_list(TypeListId(list_id));

        // TODO: This blindly picks the first ThisType.
        // Correct behavior requires narrowing the contextual type based on
        // the object literal shape BEFORE determining which this type to use.
        // Example: If context is (A & ThisType<X>) | (B & ThisType<Y>) and
        // the literal is { type: 'b' }, we should pick ThisType<Y>, not ThisType<X>.
        // This is acceptable for Phase 1, but should be improved in Phase 2.
        members
            .iter()
            .find_map(|&member_id| self.visit_type(self.db, member_id))
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract array element type or union of tuple element types.
struct ArrayElementExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ArrayElementExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for ArrayElementExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_array(&mut self, elem_type: TypeId) -> Self::Output {
        Some(elem_type)
    }

    fn visit_tuple(&mut self, elements_id: u32) -> Self::Output {
        let elements = self.db.tuple_list(TupleListId(elements_id));
        if elements.is_empty() {
            None
        } else {
            let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
            Some(self.db.union(types))
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract tuple element at a specific index.
struct TupleElementExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
}

impl<'a> TupleElementExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, index: usize) -> Self {
        Self { db, index }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for TupleElementExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_tuple(&mut self, elements_id: u32) -> Self::Output {
        let elements = self.db.tuple_list(TupleListId(elements_id));
        if self.index < elements.len() {
            Some(elements[self.index].type_id)
        } else if let Some(last) = elements.last() {
            if last.rest { Some(last.type_id) } else { None }
        } else {
            None
        }
    }

    fn visit_array(&mut self, elem_type: TypeId) -> Self::Output {
        Some(elem_type)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract property type from object types by name.
struct PropertyExtractor<'a> {
    db: &'a dyn TypeDatabase,
    name: String,
}

impl<'a> PropertyExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, name: String) -> Self {
        Self { db, name }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for PropertyExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        for prop in &shape.properties {
            if self.db.resolve_atom_ref(prop.name).as_ref() == self.name {
                return Some(prop.type_id);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract method type from object types by name.
/// Only returns methods (is_method = true), not regular properties.
struct MethodExtractor<'a> {
    db: &'a dyn TypeDatabase,
    name: String,
}

impl<'a> MethodExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, name: String) -> Self {
        Self { db, name }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for MethodExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        for prop in &shape.properties {
            if self.db.resolve_atom_ref(prop.name).as_ref() == self.name && prop.is_method {
                return Some(prop.type_id);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract parameter type from callable types.
struct ParameterExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
}

impl<'a> ParameterExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, index: usize) -> Self {
        Self { db, index }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }

    fn extract_from_params(&self, params: &[ParamInfo]) -> Option<TypeId> {
        // Check if there's a rest parameter at the end
        if let Some(last_param) = params.last() {
            if last_param.rest {
                // For rest parameter, any index should get the element type
                if let Some(TypeKey::Array(elem)) = self.db.lookup(last_param.type_id) {
                    return Some(elem);
                }
                // For rest parameter with tuple type, extract the element at the given index
                if let Some(TypeKey::Tuple(elements)) = self.db.lookup(last_param.type_id) {
                    let elements = self.db.tuple_list(elements);
                    // Find the tuple element at the given index
                    if self.index < elements.len() {
                        return Some(elements[self.index].type_id);
                    } else if let Some(last_elem) = elements.last()
                        && last_elem.rest
                    {
                        return Some(last_elem.type_id);
                    }
                }
                // Return the rest parameter type itself
                return Some(last_param.type_id);
            }
        }

        // For non-rest parameters, check if index is within bounds
        if self.index < params.len() {
            Some(params[self.index].type_id)
        } else {
            None
        }
    }
}

impl<'a> TypeVisitor for ParameterExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        self.extract_from_params(&shape.params)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        // For callables with multiple signatures, collect parameter types from all signatures
        if shape.call_signatures.is_empty() {
            return None;
        }

        let param_types: Vec<TypeId> = shape
            .call_signatures
            .iter()
            .filter_map(|sig| self.extract_from_params(&sig.params))
            .collect();

        if param_types.is_empty() {
            None
        } else if param_types.len() == 1 {
            Some(param_types[0])
        } else {
            Some(self.db.union(param_types))
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract parameter type from callable types for a call site.
/// Filters signatures by arity (arg_count) to handle overloaded functions.
struct ParameterForCallExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
}

impl<'a> ParameterForCallExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, index: usize, arg_count: usize) -> Self {
        Self {
            db,
            index,
            arg_count,
        }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }

    fn extract_from_params(&self, params: &[ParamInfo]) -> Option<TypeId> {
        // Check if there's a rest parameter at the end
        if let Some(last_param) = params.last() {
            if last_param.rest {
                // For rest parameter, any index should get the element type
                if let Some(TypeKey::Array(elem)) = self.db.lookup(last_param.type_id) {
                    return Some(elem);
                }
                // For rest parameter with tuple type, extract the element at the given index
                if let Some(TypeKey::Tuple(elements)) = self.db.lookup(last_param.type_id) {
                    let elements = self.db.tuple_list(elements);
                    // Find the tuple element at the given index
                    if self.index < elements.len() {
                        return Some(elements[self.index].type_id);
                    } else if let Some(last_elem) = elements.last()
                        && last_elem.rest
                    {
                        return Some(last_elem.type_id);
                    }
                }
                // Return the rest parameter type itself
                return Some(last_param.type_id);
            }
        }

        // For non-rest parameters, check if index is within bounds
        if self.index < params.len() {
            Some(params[self.index].type_id)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        // Count required (non-optional) parameters
        let required_count = params.iter().filter(|p| !p.optional).count();

        // Check if there's a rest parameter
        let has_rest = params.iter().any(|p| p.rest);

        if has_rest {
            // With rest parameter: arity must be >= required_count
            arg_count >= required_count
        } else {
            // Without rest parameter: arity must be within [required_count, total_count]
            arg_count >= required_count && arg_count <= params.len()
        }
    }
}

impl<'a> TypeVisitor for ParameterForCallExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        self.extract_from_params(&shape.params)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        // Filter signatures by arity
        let mut matched = false;
        let mut param_types: Vec<TypeId> = Vec::new();

        for sig in &shape.call_signatures {
            if self.signature_accepts_arg_count(&sig.params, self.arg_count) {
                matched = true;
                if let Some(param_type) = self.extract_from_params(&sig.params) {
                    param_types.push(param_type);
                }
            }
        }

        // If no signatures matched, fall back to all signatures
        if param_types.is_empty() && !matched {
            param_types = shape
                .call_signatures
                .iter()
                .filter_map(|sig| self.extract_from_params(&sig.params))
                .collect();
        }

        if param_types.is_empty() {
            None
        } else if param_types.len() == 1 {
            Some(param_types[0])
        } else {
            Some(self.db.union(param_types))
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the yield type from Generator<Y, R, N> applications.
struct GeneratorYieldExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> GeneratorYieldExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for GeneratorYieldExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        let app = self.db.type_application(TypeApplicationId(app_id));
        // Generator<Y, R, N> has Y as the first type argument
        if !app.args.is_empty() {
            Some(app.args[0])
        } else {
            None
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the return type from Generator<Y, R, N> applications.
struct GeneratorReturnExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> GeneratorReturnExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for GeneratorReturnExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        let app = self.db.type_application(TypeApplicationId(app_id));
        // Generator<Y, R, N> has R as the second type argument
        if app.args.len() >= 2 {
            Some(app.args[1])
        } else {
            None
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the next type from Generator<Y, R, N> applications.
struct GeneratorNextExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> GeneratorNextExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for GeneratorNextExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        let app = self.db.type_application(TypeApplicationId(app_id));
        // Generator<Y, R, N> has N as the third type argument
        if app.args.len() >= 3 {
            Some(app.args[2])
        } else {
            None
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

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

        // Handle Union explicitly - collect parameter types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_parameter_type(index)
                })
                .collect();

            return if param_types.is_empty() {
                None
            } else if param_types.len() == 1 {
                Some(param_types[0])
            } else {
                Some(self.interner.union(param_types))
            };
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeKey::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_parameter_type(index);
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterExtractor::new(self.interner, index);
        extractor.extract(expected)
    }

    /// Get the contextual type for a call argument at the given index and arity.
    pub fn get_parameter_type_for_call(&self, index: usize, arg_count: usize) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect parameter types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_parameter_type_for_call(index, arg_count)
                })
                .collect();

            return if param_types.is_empty() {
                None
            } else if param_types.len() == 1 {
                Some(param_types[0])
            } else {
                Some(self.interner.union(param_types))
            };
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeKey::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_parameter_type_for_call(index, arg_count);
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterForCallExtractor::new(self.interner, index, arg_count);
        extractor.extract(expected)
    }

    /// Get the contextual type for a `this` parameter, if present on the expected type.
    pub fn get_this_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect this types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let this_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_this_type()
                })
                .collect();

            return if this_types.is_empty() {
                None
            } else if this_types.len() == 1 {
                Some(this_types[0])
            } else {
                Some(self.interner.union(this_types))
            };
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeKey::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_this_type();
        }

        // Use visitor for Function/Callable types
        let mut extractor = ThisTypeExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the type T from a ThisType<T> marker in the contextual type.
    ///
    /// This is used for the Vue 2 / Options API pattern where object literal
    /// methods have their `this` type overridden by contextual markers.
    ///
    /// Example:
    /// ```typescript
    /// type ObjectDescriptor<D, M> = {
    ///     methods?: M & ThisType<D & M>;
    /// };
    /// const obj: ObjectDescriptor<{x: number}, {greet(): void}> = {
    ///     methods: {
    ///         greet() { console.log(this.x); } // this is D & M
    ///     }
    /// };
    /// ```
    pub fn get_this_type_from_marker(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let mut extractor = ThisTypeMarkerExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the contextual return type for a function.
    pub fn get_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect return types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let return_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_return_type()
                })
                .collect();

            return if return_types.is_empty() {
                None
            } else if return_types.len() == 1 {
                Some(return_types[0])
            } else {
                Some(self.interner.union(return_types))
            };
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeKey::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_return_type();
        }

        // Use visitor for Function/Callable types
        let mut extractor = ReturnTypeExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the contextual element type for an array.
    ///
    /// Example:
    /// ```typescript
    /// const arr: number[] = [1, 2, 3];  // elements are contextually typed as number
    /// ```
    pub fn get_array_element_type(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let mut extractor = ArrayElementExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the contextual type for a specific tuple element.
    pub fn get_tuple_element_type(&self, index: usize) -> Option<TypeId> {
        let expected = self.expected?;
        let mut extractor = TupleElementExtractor::new(self.interner, index);
        extractor.extract(expected)
    }

    /// Get the contextual type for an object property.
    ///
    /// Example:
    /// ```typescript
    /// const obj: {x: number, y: string} = {x: 1, y: "hi"};
    /// ```
    pub fn get_property_type(&self, name: &str) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect property types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let prop_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_property_type(name)
                })
                .collect();

            return if prop_types.is_empty() {
                None
            } else if prop_types.len() == 1 {
                Some(prop_types[0])
            } else {
                Some(self.interner.union(prop_types))
            };
        }

        // Use visitor for Object types
        let mut extractor = PropertyExtractor::new(self.interner, name.to_string());
        extractor.extract(expected)
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
    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

        // Handle Union explicitly - collect yield types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let yield_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_yield_type()
                })
                .collect();

            return if yield_types.is_empty() {
                None
            } else if yield_types.len() == 1 {
                Some(yield_types[0])
            } else {
                Some(self.interner.union(yield_types))
            };
        }

        // Use visitor for Generator types
        let mut extractor = GeneratorYieldExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the contextual return type for a generator function (TReturn from Generator<Y, TReturn, N>).
    ///
    /// This is used to contextually type return statements in generators.
    pub fn get_generator_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect return types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let return_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_return_type()
                })
                .collect();

            return if return_types.is_empty() {
                None
            } else if return_types.len() == 1 {
                Some(return_types[0])
            } else {
                Some(self.interner.union(return_types))
            };
        }

        // Use visitor for Generator types
        let mut extractor = GeneratorReturnExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the contextual next type for a generator function (TNext from Generator<Y, R, TNext>).
    ///
    /// This is used to determine the type of values passed to .next() and
    /// the type of the yield expression result.
    pub fn get_generator_next_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect next types from all members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let next_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_next_type()
                })
                .collect();

            return if next_types.is_empty() {
                None
            } else if next_types.len() == 1 {
                Some(next_types[0])
            } else {
                Some(self.interner.union(next_types))
            };
        }

        // Use visitor for Generator types
        let mut extractor = GeneratorNextExtractor::new(self.interner);
        extractor.extract(expected)
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
        // Use MethodExtractor to find the 'next' method (is_method = true)
        let mut method_extractor = MethodExtractor::new(self.interner, "next".to_string());
        let next_method = method_extractor.extract(gen_type)?;

        // Extract yield type from the method return type
        self.extract_yield_from_next_method(next_method)
    }

    /// Extract yield type from next method's return type.
    ///
    /// The return type is either:
    /// - IteratorResult<Y, R> for sync generators
    /// - Promise<IteratorResult<Y, R>> for async generators
    fn extract_yield_from_next_method(&self, method_type: TypeId) -> Option<TypeId> {
        // Use ReturnTypeExtractor to get the return type
        let mut ret_extractor = ReturnTypeExtractor::new(self.interner);
        let return_type = ret_extractor.extract(method_type)?;

        // Check if it's a Promise wrapper (async generator) - look for 'then' method
        let mut then_extractor = MethodExtractor::new(self.interner, "then".to_string());
        if let Some(then_type) = then_extractor.extract(return_type) {
            // Async generator: unwrap Promise and get IteratorResult value
            return self.extract_value_from_iterator_result(then_type);
        }

        // Check for union type (IteratorResult = {value: Y, done: false} | {value: R, done: true})
        if let Some(TypeKey::Union(_)) = self.interner.lookup(return_type) {
            return self.extract_value_from_iterator_result(return_type);
        }

        // Sync generator: try to extract 'value' property directly
        self.extract_value_property(return_type)
    }

    /// Extract the 'value' property from an IteratorResult<Y, R> type.
    fn extract_value_from_iterator_result(&self, result_type: TypeId) -> Option<TypeId> {
        // Handle Union explicitly
        if let Some(TypeKey::Union(list_id)) = self.interner.lookup(result_type) {
            let members = self.interner.type_list(list_id);
            // Get first member (yield result) and extract value
            if let Some(&first) = members.first() {
                return self.extract_value_property(first);
            }
            return None;
        }

        // Handle Object types
        self.extract_value_property(result_type)
    }

    /// Extract 'value' property from an object type.
    fn extract_value_property(&self, obj_type: TypeId) -> Option<TypeId> {
        // Use PropertyExtractor to find the 'value' property
        let mut prop_extractor = PropertyExtractor::new(self.interner, "value".to_string());
        prop_extractor.extract(obj_type)
    }

    /// Extract next type (N) from Generator<Y, R, N> structure.
    fn extract_next_type_from_generator(&self, gen_type: TypeId) -> Option<TypeId> {
        // Use MethodExtractor to find the 'next' method (is_method = true)
        let mut method_extractor = MethodExtractor::new(self.interner, "next".to_string());
        let next_method = method_extractor.extract(gen_type)?;

        // Extract next type from the method's first parameter
        self.extract_next_from_method(next_method)
    }

    /// Extract next type from next method's parameter.
    fn extract_next_from_method(&self, method_type: TypeId) -> Option<TypeId> {
        // Use ParameterExtractor to get the first parameter (index 0)
        let mut param_extractor = ParameterExtractor::new(self.interner, 0);
        param_extractor.extract(method_type)
    }

    /// Extract return type (R) from Generator<Y, R, N> structure.
    fn extract_return_type_from_generator(&self, gen_type: TypeId) -> Option<TypeId> {
        // Use MethodExtractor to find the 'return' method (is_method = true)
        let mut method_extractor = MethodExtractor::new(self.interner, "return".to_string());
        let return_method = method_extractor.extract(gen_type)?;

        // Extract return type from the method's first parameter
        self.extract_return_from_method(return_method)
    }

    /// Extract return type from return method's parameter.
    fn extract_return_from_method(&self, method_type: TypeId) -> Option<TypeId> {
        // Use ParameterExtractor to get the first parameter (index 0)
        let mut param_extractor = ParameterExtractor::new(self.interner, 0);
        param_extractor.extract(method_type)
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
#[path = "tests/contextual_tests.rs"]
mod tests;
