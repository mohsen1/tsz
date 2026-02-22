//! Visitor-based type extractors for contextual typing.
//!
//! These [`TypeVisitor`] implementations extract specific type information
//! (return types, parameter types, property types, etc.) from contextual types.
//! They are used by [`super::contextual::ContextualTypeContext`] to implement
//! bidirectional type inference.

use crate::TypeDatabase;
use crate::types::{
    CallableShapeId, FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShapeId, ParamInfo,
    TupleListId, TypeApplicationId, TypeData, TypeId, TypeListId,
};
use crate::visitor::TypeVisitor;
use tsz_common::interner::Atom;

// =============================================================================
// Helper Functions
// =============================================================================

/// Helper to collect types and return None, single type, or union.
///
/// This pattern appears frequently in visitor implementations:
/// - If no types collected: return None
/// - If one type collected: return Some(that type)
/// - If multiple types: return Some(union of types)
pub(crate) fn collect_single_or_union(db: &dyn TypeDatabase, types: Vec<TypeId>) -> Option<TypeId> {
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
pub(crate) struct ThisTypeExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ThisTypeExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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
pub(crate) struct ReturnTypeExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ReturnTypeExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions of callable types, extract return type from each member
        // and create a union of the results.
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor = ReturnTypeExtractor::new(self.db);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
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
pub(crate) struct ThisTypeMarkerExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ThisTypeMarkerExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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
        // This is a conservative heuristic and could be improved.
        members
            .iter()
            .find_map(|&member_id| self.visit_type(self.db, member_id))
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract array element type or union of tuple element types.
pub(crate) struct ArrayElementExtractor<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ArrayElementExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        if let Some(ref idx) = shape.number_index {
            return Some(idx.value_type);
        }
        if let Some(ref idx) = shape.string_index {
            return Some(idx.value_type);
        }
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        self.visit_object(shape_id)
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
pub(crate) struct TupleElementExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
}

impl<'a> TupleElementExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize) -> Self {
        Self { db, index }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        if let Some(ref idx) = shape.number_index {
            return Some(idx.value_type);
        }
        if let Some(ref idx) = shape.string_index {
            return Some(idx.value_type);
        }
        None
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        self.visit_object(shape_id)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions of tuple/array types, extract the element type from each member
        // and create a union of the results.
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor = TupleElementExtractor::new(self.db, self.index);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // For intersections, try each member and return the first match.
        let members = self.db.type_list(TypeListId(list_id));
        for &member in members.iter() {
            let mut extractor = TupleElementExtractor::new(self.db, self.index);
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

/// Visitor to extract property type from object types by name.
pub(crate) struct PropertyExtractor<'a> {
    db: &'a dyn TypeDatabase,
    name_atom: Atom,
    is_numeric_name: bool,
}

impl<'a> PropertyExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, name: &str) -> Self {
        Self {
            db,
            name_atom: db.intern_string(name),
            is_numeric_name: name.parse::<f64>().is_ok(),
        }
    }

    pub(crate) fn from_atom(
        db: &'a dyn TypeDatabase,
        name_atom: Atom,
        is_numeric_name: bool,
    ) -> Self {
        Self {
            db,
            name_atom,
            is_numeric_name,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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
            if prop.name == self.name_atom {
                return Some(prop.type_id);
            }
        }
        // Fall back to index signatures for Object types too
        // This handles cases where interfaces/types have index signatures
        // but are stored as Object rather than ObjectWithIndex
        // For numeric property names (e.g., "1"), check number index signature first
        if self.is_numeric_name
            && let Some(ref idx) = shape.number_index
        {
            return Some(idx.value_type);
        }
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
        let shape = self.db.object_shape(ObjectShapeId(shape_id));
        // For numeric property names, check number index signature first
        if self.is_numeric_name
            && let Some(ref idx) = shape.number_index
        {
            return Some(idx.value_type);
        }
        // Fall back to string index signature value type
        if let Some(ref idx) = shape.string_index {
            return Some(idx.value_type);
        }
        None
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        let resolved = crate::evaluation::evaluate::evaluate_type(self.db, TypeId(def_id));
        if resolved != TypeId(def_id) {
            self.visit_type(self.db, resolved)
        } else {
            None
        }
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions, extract the property from each member and combine as a union.
        // e.g., for { foo: ... } with contextual type A | B,
        // the contextual type of `foo` is A["foo"] | B["foo"].
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    PropertyExtractor::from_atom(self.db, self.name_atom, self.is_numeric_name);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &member in members.iter() {
            let mut extractor =
                PropertyExtractor::from_atom(self.db, self.name_atom, self.is_numeric_name);
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
pub(crate) fn extract_param_type_at(
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
            // If out of bounds of the tuple constraint without rest, return undefined/unknown?
            // Fall through
        } else if let Some(TypeData::TypeParameter(param_info)) = db.lookup(last_param.type_id) {
            if let Some(constraint) = param_info.constraint {
                let mut mock_params = params.to_vec();
                mock_params.last_mut().unwrap().type_id = constraint;
                return extract_param_type_at(db, &mock_params, index);
            }
        } else if let Some(TypeData::Intersection(members)) = db.lookup(last_param.type_id) {
            let members = db.type_list(members);
            for &m in members.iter() {
                let mut mock_params = params.to_vec();
                mock_params.last_mut().unwrap().type_id = m;
                if let Some(param_type) = extract_param_type_at(db, &mock_params, index) {
                    // Try to evaluate it if it's a generic type or placeholder to see if it yields a concrete type
                    if !matches!(
                        db.lookup(param_type),
                        Some(TypeData::TypeParameter(_) | TypeData::Intersection(_))
                    ) {
                        return Some(param_type);
                    }
                }
            }
            // If all returned generic types, just fall through
        } else if let Some(TypeData::Application(_app_id)) = db.lookup(last_param.type_id) {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, last_param.type_id);
            if evaluated != last_param.type_id {
                let mut mock_params = params.to_vec();
                mock_params.last_mut().unwrap().type_id = evaluated;
                return extract_param_type_at(db, &mock_params, index);
            }
        }

        // If we still didn't extract a specific type, check constraint
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(db, last_param.type_id)
        {
            let mut mock_params = params.to_vec();
            mock_params.last_mut().unwrap().type_id = constraint;
            if let Some(param_type) = extract_param_type_at(db, &mock_params, index) {
                // If it yielded something different than the constraint itself, use it
                if param_type != constraint {
                    return Some(param_type);
                }
            }
        }

        return Some(last_param.type_id);
    }

    // Index within non-rest params
    (index < params.len()).then(|| params[index].type_id)
}

/// Visitor to extract parameter type from callable types.
pub(crate) struct ParameterExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    no_implicit_any: bool,
}

impl<'a> ParameterExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize, no_implicit_any: bool) -> Self {
        Self {
            db,
            index,
            no_implicit_any,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions of callable types, extract the parameter type from each member
        // and create a union of the results.
        // e.g., ((a: number) => void) | ((a: string) => void) at index 0 => number | string
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    ParameterExtractor::new(self.db, self.index, self.no_implicit_any);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // For intersections, try each member and return the first match.
        let members = self.db.type_list(TypeListId(list_id));
        for &member in members.iter() {
            let mut extractor = ParameterExtractor::new(self.db, self.index, self.no_implicit_any);
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

/// Visitor to extract parameter type from callable types for a call site.
/// Filters signatures by arity (`arg_count`) to handle overloaded functions.
pub(crate) struct ParameterForCallExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
}

impl<'a> ParameterForCallExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize, arg_count: usize) -> Self {
        Self {
            db,
            index,
            arg_count,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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

        // If no call signatures matched, check non-generic construct signatures.
        // This handles super() calls and new expressions where the callee
        // is a Callable with construct signatures (not call signatures).
        // Skip generic construct signatures: their type parameters must be
        // inferred by the solver, not used as contextual types for arguments.
        if param_types.is_empty() {
            matched = false;
            for sig in &shape.construct_signatures {
                if !sig.type_params.is_empty() {
                    continue;
                }
                if self.signature_accepts_arg_count(&sig.params, self.arg_count) {
                    matched = true;
                    if let Some(param_type) =
                        extract_param_type_at(self.db, &sig.params, self.index)
                    {
                        param_types.push(param_type);
                    }
                }
            }
            if param_types.is_empty() && !matched {
                param_types = shape
                    .construct_signatures
                    .iter()
                    .filter(|sig| sig.type_params.is_empty())
                    .filter_map(|sig| extract_param_type_at(self.db, &sig.params, self.index))
                    .collect();
            }
        }

        collect_single_or_union(self.db, param_types)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions, extract parameter types from each member and combine.
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    ParameterForCallExtractor::new(self.db, self.index, self.arg_count);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract a type argument at a given index from an Application type.
///
/// Used for `Generator<Y, R, N>` and similar generic types where we need to
/// pull out a specific type parameter by position.
pub(crate) struct ApplicationArgExtractor<'a> {
    db: &'a dyn TypeDatabase,
    arg_index: usize,
}

impl<'a> ApplicationArgExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, arg_index: usize) -> Self {
        Self { db, arg_index }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
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
