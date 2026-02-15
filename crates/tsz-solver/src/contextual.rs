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

use crate::TypeDatabase;
#[cfg(test)]
use crate::types::*;
use crate::types::{
    CallableShapeId, FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShapeId, ParamInfo,
    TupleListId, TypeApplicationId, TypeData, TypeId, TypeListId,
};
use crate::visitor::TypeVisitor;

// =============================================================================
// Helper Functions
// =============================================================================

/// Helper to collect types and return None, single type, or union.
///
/// This pattern appears frequently in visitor implementations:
/// - If no types collected: return None
/// - If one type collected: return Some(that type)
/// - If multiple types: return Some(union of types)
fn collect_single_or_union(db: &dyn TypeDatabase, types: Vec<TypeId>) -> Option<TypeId> {
    match types.len() {
        0 => None,
        1 => Some(types[0]),
        _ => Some(db.union(types)),
    }
}

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

        collect_single_or_union(self.db, this_types)
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
        let return_types: Vec<TypeId> = shape
            .call_signatures
            .iter()
            .map(|sig| sig.return_type)
            .collect();

        collect_single_or_union(self.db, return_types)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the type T from `ThisType`<T> utility type markers.
///
/// This handles the Vue 2 / Options API pattern where contextual types contain
/// `ThisType`<T> markers to override the type of 'this' in object literal methods.
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

    /// Check if a type application is for the `ThisType` utility.
    fn is_this_type_application(&self, app_id: u32) -> bool {
        let app = self.db.type_application(TypeApplicationId(app_id));

        // CRITICAL: We must NOT return true for all Lazy types!
        // Doing so would break ALL generic type aliases (Partial<T>, Readonly<T>, etc.)
        // We must check if the base type is specifically "ThisType"

        // Check TypeParameter case first (easier - has name directly)
        if let Some(TypeData::TypeParameter(tp)) = self.db.lookup(app.base) {
            let name = self.db.resolve_atom_ref(tp.name);
            return name.as_ref() == "ThisType";
        }

        // For Lazy types (type aliases), we need to resolve the def_id to a name
        // This is harder without access to the symbol table. For now, we fail safe
        // and return false rather than breaking all type aliases.
        // TODO: When we have access to symbol resolution, check if def_id points to lib.d.ts ThisType
        if let Some(TypeData::Lazy(_def_id)) = self.db.lookup(app.base) {
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
            last.rest.then_some(last.type_id)
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
        // Fall back to index signatures for Object types too
        // This handles cases where interfaces/types have index signatures
        // but are stored as Object rather than ObjectWithIndex
        if let Some(ref idx) = shape.string_index {
            return Some(idx.value_type);
        }
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        // First try named properties
        if let Some(ty) = self.visit_object(shape_id) {
            return Some(ty);
        }
        // Fall back to string index signature value type
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        if let Some(ref idx) = shape.string_index {
            return Some(idx.value_type);
        }
        None
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        let resolved = crate::evaluate::evaluate_type(self.db, TypeId(def_id));
        if resolved != TypeId(def_id) {
            self.visit_type(self.db, resolved)
        } else {
            None
        }
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &member in members.iter() {
            let mut extractor = PropertyExtractor::new(self.db, self.name.clone());
            if let Some(ty) = extractor.extract(member) {
                return Some(ty);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Extract the parameter type at `index` from a parameter list, handling rest params.
fn extract_param_type_at(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
    index: usize,
) -> Option<TypeId> {
    let rest_param = params.last().filter(|p| p.rest);
    let rest_start = if rest_param.is_some() {
        params.len().saturating_sub(1)
    } else {
        params.len()
    };

    // Regular (non-rest) parameters: return directly by index
    if index < rest_start {
        return Some(params[index].type_id);
    }

    // Rest parameter handling
    if let Some(last_param) = rest_param {
        // Adjust index relative to rest parameter start
        let rest_index = index - rest_start;
        if let Some(TypeData::Array(elem)) = db.lookup(last_param.type_id) {
            return Some(elem);
        }
        if let Some(TypeData::Tuple(elements)) = db.lookup(last_param.type_id) {
            let elements = db.tuple_list(elements);
            if rest_index < elements.len() {
                return Some(elements[rest_index].type_id);
            } else if let Some(last_elem) = elements.last()
                && last_elem.rest
            {
                return Some(last_elem.type_id);
            }
        }
        return Some(last_param.type_id);
    }

    // Index within non-rest params
    (index < params.len()).then(|| params[index].type_id)
}

/// Visitor to extract parameter type from callable types.
struct ParameterExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    no_implicit_any: bool,
}

impl<'a> ParameterExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, index: usize, no_implicit_any: bool) -> Self {
        Self {
            db,
            index,
            no_implicit_any,
        }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
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
        extract_param_type_at(self.db, &shape.params, self.index)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        // Collect parameter types from all signatures at the given index
        let param_types: Vec<TypeId> = shape
            .call_signatures
            .iter()
            .filter_map(|sig| extract_param_type_at(self.db, &sig.params, self.index))
            .collect();

        if param_types.is_empty() {
            None
        } else if param_types.len() == 1 {
            Some(param_types[0])
        } else {
            // Multiple different parameter types
            // If noImplicitAny is false, fall back to `any` (return None)
            // If noImplicitAny is true, create a union type
            self.no_implicit_any.then(|| self.db.union(param_types))
        }
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract parameter type from callable types for a call site.
/// Filters signatures by arity (`arg_count`) to handle overloaded functions.
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

        if !self.signature_accepts_arg_count(&shape.params, self.arg_count) {
            return None;
        }

        extract_param_type_at(self.db, &shape.params, self.index)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        let mut matched = false;
        let mut param_types: Vec<TypeId> = Vec::new();

        for sig in &shape.call_signatures {
            if self.signature_accepts_arg_count(&sig.params, self.arg_count) {
                matched = true;
                if let Some(param_type) = extract_param_type_at(self.db, &sig.params, self.index) {
                    param_types.push(param_type);
                }
            }
        }

        if param_types.is_empty() && !matched {
            param_types = shape
                .call_signatures
                .iter()
                .filter_map(|sig| extract_param_type_at(self.db, &sig.params, self.index))
                .collect();
        }

        collect_single_or_union(self.db, param_types)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract a type argument at a given index from an Application type.
///
/// Used for `Generator<Y, R, N>` and similar generic types where we need to
/// pull out a specific type parameter by position.
struct ApplicationArgExtractor<'a> {
    db: &'a dyn TypeDatabase,
    arg_index: usize,
}

impl<'a> ApplicationArgExtractor<'a> {
    fn new(db: &'a dyn TypeDatabase, arg_index: usize) -> Self {
        Self { db, arg_index }
    }

    fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for ApplicationArgExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        let app = self.db.type_application(TypeApplicationId(app_id));
        app.args.get(self.arg_index).copied()
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
    /// Whether noImplicitAny is enabled (affects contextual typing for multi-signature functions)
    no_implicit_any: bool,
}

impl<'a> ContextualTypeContext<'a> {
    /// Create a new contextual type context.
    /// Defaults to `no_implicit_any: false` for compatibility.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        ContextualTypeContext {
            interner,
            expected: None,
            no_implicit_any: false,
        }
    }

    /// Create a context with an expected type.
    /// Defaults to `no_implicit_any: false` for compatibility.
    pub fn with_expected(interner: &'a dyn TypeDatabase, expected: TypeId) -> Self {
        ContextualTypeContext {
            interner,
            expected: Some(expected),
            no_implicit_any: false,
        }
    }

    /// Create a context with an expected type and explicit noImplicitAny setting.
    pub fn with_expected_and_options(
        interner: &'a dyn TypeDatabase,
        expected: TypeId,
        no_implicit_any: bool,
    ) -> Self {
        ContextualTypeContext {
            interner,
            expected: Some(expected),
            no_implicit_any,
        }
    }

    /// Get the expected type.
    pub const fn expected(&self) -> Option<TypeId> {
        self.expected
    }

    /// Check if we have a contextual type.
    pub const fn has_context(&self) -> bool {
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
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected_and_options(
                        self.interner,
                        m,
                        self.no_implicit_any,
                    );
                    ctx.get_parameter_type(index)
                })
                .collect();

            return collect_single_or_union(self.interner, param_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected_and_options(
                self.interner,
                app.base,
                self.no_implicit_any,
            );
            return ctx.get_parameter_type(index);
        }

        // Handle Intersection explicitly - pick the first callable member's parameter type
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            for &m in members.iter() {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    m,
                    self.no_implicit_any,
                );
                if let Some(param_type) = ctx.get_parameter_type(index) {
                    return Some(param_type);
                }
            }
            return None;
        }

        // Handle Mapped and Conditional types by evaluating them first
        if let Some(TypeData::Mapped(_) | TypeData::Conditional(_)) = self.interner.lookup(expected)
        {
            let evaluated = crate::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    evaluated,
                    self.no_implicit_any,
                );
                return ctx.get_parameter_type(index);
            }
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterExtractor::new(self.interner, index, self.no_implicit_any);
        extractor.extract(expected)
    }

    /// Get the contextual type for a call argument at the given index and arity.
    pub fn get_parameter_type_for_call(&self, index: usize, arg_count: usize) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect parameter types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_parameter_type_for_call(index, arg_count)
                })
                .collect();

            return collect_single_or_union(self.interner, param_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_parameter_type_for_call(index, arg_count);
        }

        // Handle Intersection explicitly - pick the first callable member's parameter type
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            for &m in members.iter() {
                let ctx = ContextualTypeContext::with_expected(self.interner, m);
                if let Some(param_type) = ctx.get_parameter_type_for_call(index, arg_count) {
                    return Some(param_type);
                }
            }
            return None;
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterForCallExtractor::new(self.interner, index, arg_count);
        extractor.extract(expected)
    }

    /// Get the contextual type for a `this` parameter, if present on the expected type.
    pub fn get_this_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect this types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let this_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_this_type()
                })
                .collect();

            return collect_single_or_union(self.interner, this_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_this_type();
        }

        // Use visitor for Function/Callable types
        let mut extractor = ThisTypeExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the type T from a `ThisType`<T> marker in the contextual type.
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
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let return_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_return_type()
                })
                .collect();

            return collect_single_or_union(self.interner, return_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
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
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
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
                // CRITICAL: Use union_preserve_members to keep literal types intact
                // For discriminated unions like `{ success: false } | { success: true }`,
                // the property type should be `false | true`, NOT widened to `boolean`.
                // This preserves literal types for contextual typing.
                Some(self.interner.union_preserve_members(prop_types))
            };
        }

        // Handle Mapped, Conditional, and Application types.
        // These complex types need to be resolved to concrete object types before
        // property extraction can work.
        match self.interner.lookup(expected) {
            Some(TypeData::Mapped(mapped_id)) => {
                // First try evaluating the mapped type directly
                let evaluated = crate::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type(name);
                }
                // If evaluation deferred (e.g. { [K in keyof P]: P[K] } where P is a type
                // parameter), fall back to the constraint of the mapped type's source.
                // For `keyof P` where `P extends Props`, use `Props` as the contextual type.
                let mapped = self.interner.mapped_type(mapped_id);
                if let Some(TypeData::KeyOf(operand)) = self.interner.lookup(mapped.constraint) {
                    // The operand may be a Lazy type wrapping a type parameter — resolve it
                    let resolved_operand = crate::evaluate::evaluate_type(self.interner, operand);
                    if let Some(constraint) = crate::type_queries::get_type_parameter_constraint(
                        self.interner,
                        resolved_operand,
                    ) {
                        let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                        return ctx.get_property_type(name);
                    }
                    // Also try the original operand (may already be a TypeParameter)
                    if let Some(constraint) =
                        crate::type_queries::get_type_parameter_constraint(self.interner, operand)
                    {
                        let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                        return ctx.get_property_type(name);
                    }
                }
            }
            Some(TypeData::Conditional(_) | TypeData::Application(_)) => {
                let evaluated = crate::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type(name);
                }
            }
            _ => {}
        }

        // Handle TypeParameter - use its constraint for property extraction
        // Example: Actions extends ActionsObject<State>
        // When getting property from Actions, use ActionsObject<State> instead
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_property_type(name);
        }

        // Use visitor for Object types
        let mut extractor = PropertyExtractor::new(self.interner, name.to_string());
        extractor.extract(expected)
    }

    /// Create a child context for a nested expression.
    /// This is used when checking nested structures with contextual types.
    pub fn for_property(&self, name: &str) -> Self {
        match self.get_property_type(name) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for an array element.
    pub fn for_array_element(&self) -> Self {
        match self.get_array_element_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a tuple element at the given index.
    pub fn for_tuple_element(&self, index: usize) -> Self {
        match self.get_tuple_element_type(index) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a function parameter at the given index.
    pub fn for_parameter(&self, index: usize) -> Self {
        match self.get_parameter_type(index) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a function return expression.
    pub fn for_return(&self) -> Self {
        match self.get_return_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
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
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let yield_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_yield_type()
                })
                .collect();

            return collect_single_or_union(self.interner, yield_types);
        }

        // Generator<Y, R, N> — yield type is arg 0
        let mut extractor = ApplicationArgExtractor::new(self.interner, 0);
        extractor.extract(expected)
    }

    /// Get the contextual return type for a generator function (`TReturn` from Generator<Y, `TReturn`, N>).
    ///
    /// This is used to contextually type return statements in generators.
    pub fn get_generator_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect return types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let return_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_return_type()
                })
                .collect();

            return collect_single_or_union(self.interner, return_types);
        }

        // Generator<Y, R, N> — return type is arg 1
        let mut extractor = ApplicationArgExtractor::new(self.interner, 1);
        extractor.extract(expected)
    }

    /// Get the contextual next type for a generator function (`TNext` from Generator<Y, R, `TNext`>).
    ///
    /// This is used to determine the type of values passed to .`next()` and
    /// the type of the yield expression result.
    pub fn get_generator_next_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect next types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let next_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_next_type()
                })
                .collect();

            return collect_single_or_union(self.interner, next_types);
        }

        // Generator<Y, R, N> — next type is arg 2
        let mut extractor = ApplicationArgExtractor::new(self.interner, 2);
        extractor.extract(expected)
    }

    /// Create a child context for a yield expression in a generator.
    pub fn for_yield(&self) -> Self {
        match self.get_generator_yield_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }
}

/// Apply contextual type to infer a more specific type.
///
/// This implements bidirectional type inference:
/// 1. If `expr_type` is any/unknown/error, use contextual type
/// 2. If `expr_type` is a literal and contextual type is a union containing that literal's base type, preserve literal
/// 3. If `expr_type` is assignable to contextual type and is more specific, use `expr_type`
/// 4. Otherwise, prefer `expr_type` (don't widen to contextual type)
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
    if expr_type.is_any_or_unknown() || expr_type.is_error() {
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
        if matches!(expr_key, TypeData::Literal(_))
            && let Some(ctx_key) = interner.lookup(ctx_type)
            && matches!(ctx_key, TypeData::Union(_))
        {
            // Preserve the literal type - it's more specific than the union
            return expr_type;
        }
    }

    // PERF: Reuse a single SubtypeChecker across all subtype checks in this function
    let mut checker = crate::subtype::SubtypeChecker::new(interner);

    // Check if contextual type is a union
    if let Some(TypeData::Union(members)) = interner.lookup(ctx_type) {
        let members = interner.type_list(members);
        // If expr_type is in the union, it's valid - use the more specific expr_type
        for &member in members.iter() {
            if member == expr_type {
                return expr_type;
            }
        }
        // If expr_type is assignable to any union member, use expr_type
        for &member in members.iter() {
            checker.reset();
            if checker.is_subtype_of(expr_type, member) {
                return expr_type;
            }
        }
    }

    // If expr_type is assignable to contextual type, use expr_type (it's more specific)
    checker.reset();
    if checker.is_subtype_of(expr_type, ctx_type) {
        return expr_type;
    }

    // If contextual type is assignable to expr_type, use contextual type (it's more specific)
    // BUT: Skip for function/callable types — the solver's bivariant SubtypeChecker can
    // incorrectly say that a wider function type (e.g. (x: number|string) => void) is a
    // subtype of a narrower one (e.g. (x: number) => void), which would widen the property
    // type and suppress valid TS2322 errors under strict function types.
    let is_function_type = matches!(
        interner.lookup(expr_type),
        Some(TypeData::Function(_) | TypeData::Object(_))
    );
    if !is_function_type {
        checker.reset();
        if checker.is_subtype_of(ctx_type, expr_type) {
            return ctx_type;
        }
    }

    // Default: prefer the expression type (don't widen to contextual type)
    // This prevents incorrectly widening concrete types to generic type parameters
    expr_type
}

#[cfg(test)]
#[path = "../tests/contextual_tests.rs"]
mod tests;
