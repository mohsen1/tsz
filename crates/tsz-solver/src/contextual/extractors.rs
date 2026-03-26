//! Visitor-based type extractors for contextual typing.
//!
//! These [`TypeVisitor`] implementations extract specific type information
//! (return types, parameter types, property types, etc.) from contextual types.
//! They are used by [`super::ContextualTypeContext`] to implement
//! bidirectional type inference.

use crate::TypeDatabase;
use crate::diagnostics::format::TypeFormatter;
use crate::types::{
    CallableShapeId, FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShapeId, ParamInfo,
    TupleElement, TupleListId, TypeApplicationId, TypeData, TypeId, TypeListId,
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

/// Like [`collect_single_or_union`] but uses literal-only union reduction
/// (no subtype reduction). Use this when subtype reduction would incorrectly
/// discard contextual type information, e.g. when unioning callback types
/// from union callee members where contravariant parameter subtyping
/// would absorb the more specific variant.
pub(crate) fn collect_single_or_union_no_reduce(
    db: &dyn TypeDatabase,
    types: Vec<TypeId>,
) -> Option<TypeId> {
    match types.len() {
        0 => None,
        1 => Some(types[0]),
        _ => Some(db.union_literal_reduce(types)),
    }
}

/// Merge contextual candidates gathered from intersection members.
///
/// Intersections frequently mix a precise member with a broad index-signature or
/// `any`-like fallback. In those cases, the broad `any` candidate should not erase
/// the more precise contextual information.
pub(crate) fn collect_from_intersection(
    db: &dyn TypeDatabase,
    mut types: Vec<TypeId>,
    combine: impl FnOnce(&dyn TypeDatabase, Vec<TypeId>) -> TypeId,
) -> Option<TypeId> {
    if types.len() > 1 && types.contains(&TypeId::ANY) && types.iter().any(|&t| t != TypeId::ANY) {
        types.retain(|&t| t != TypeId::ANY);
    }

    match types.len() {
        0 => None,
        1 => Some(types[0]),
        _ if types.windows(2).all(|pair| pair[0] == pair[1]) => Some(types[0]),
        _ => Some(combine(db, types)),
    }
}

/// Extract the element type from a rest element's stored type for contextual typing.
///
/// Rest elements in tuples store the full array/tuple type (e.g., `string[]` for
/// `...string[]`). When used as a contextual type for individual element positions,
/// we need the element type (e.g., `string`), not the array type.
fn rest_element_contextual_type(db: &dyn TypeDatabase, rest_type: TypeId) -> TypeId {
    // PERF: Single lookup for ReadonlyType/Array/Tuple checks
    match db.lookup(rest_type) {
        Some(TypeData::ReadonlyType(inner)) => {
            return rest_element_contextual_type(db, inner);
        }
        Some(TypeData::Array(elem)) => {
            return elem;
        }
        Some(TypeData::Tuple(_)) => {
            let expansion = crate::utils::expand_tuple_rest(db, rest_type);
            if let Some(variadic) = expansion.variadic {
                return variadic;
            }
        }
        _ => {}
    }
    // Fallback: return the type as-is (e.g., type parameters)
    rest_type
}

fn add_undefined_if_missing(db: &dyn TypeDatabase, ty: TypeId) -> TypeId {
    if crate::narrowing::type_contains_undefined(db, ty) {
        ty
    } else {
        db.union(vec![ty, TypeId::UNDEFINED])
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

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        let app = self.db.type_application(TypeApplicationId(app_id));
        let base_result = self.visit_type(self.db, app.base);
        if base_result.is_some() {
            return base_result;
        }

        app.args
            .iter()
            .find_map(|&arg| self.visit_type(self.db, arg))
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // For intersections, try members in order and return the first
        // callable-compatible return type.
        let members = self.db.type_list(TypeListId(list_id));
        for &member in members.iter() {
            let mut extractor = ReturnTypeExtractor::new(self.db);
            if let Some(ty) = extractor.extract(member) {
                return Some(ty);
            }
        }
        None
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

        // For Lazy types (type aliases/interfaces), check if the DefId was registered
        // as the ThisType marker interface during lib.d.ts setup.
        if let Some(TypeData::Lazy(def_id)) = self.db.lookup(app.base) {
            return self.db.is_this_type_marker_def_id(def_id);
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
    /// Total number of elements in the source array/tuple literal.
    /// When provided, enables correct mapping for variadic tuple types.
    element_count: Option<usize>,
}

impl<'a> TupleElementExtractor<'a> {
    pub(crate) fn new(
        db: &'a dyn TypeDatabase,
        index: usize,
        element_count: Option<usize>,
    ) -> Self {
        Self {
            db,
            index,
            element_count,
        }
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

        // Check for variadic tuple pattern: rest element with tail elements after it.
        let rest_pos = elements.iter().position(|e| e.rest);
        let has_tail_after_rest = rest_pos.is_some_and(|pos| pos + 1 < elements.len());

        if has_tail_after_rest && let Some(element_count) = self.element_count {
            return variadic_tuple_element_type(self.db, &elements, self.index, element_count);
        }

        if self.index < elements.len() {
            let elem = &elements[self.index];
            if elem.rest {
                // Rest elements store the array/tuple type (e.g., `string[]`).
                // Extract the element type for contextual typing of individual positions.
                Some(rest_element_contextual_type(self.db, elem.type_id))
            } else {
                let mut ty = elem.type_id;
                if elem.optional {
                    ty = add_undefined_if_missing(self.db, ty);
                }
                Some(ty)
            }
        } else if let Some(last) = elements.last() {
            if last.rest {
                Some(rest_element_contextual_type(self.db, last.type_id))
            } else {
                None
            }
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
                let mut extractor =
                    TupleElementExtractor::new(self.db, self.index, self.element_count);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // For intersections, collect element types from ALL members and intersect them.
        // This ensures that when the contextual type is e.g. `[{data: T}] & [{error: E}]`,
        // the element contextual type is `{data: T} & {error: E}`, not just `{data: T}`.
        // Without intersecting, callbacks in the second member lose contextual typing.
        let members = self.db.type_list(TypeListId(list_id));
        let elem_types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    TupleElementExtractor::new(self.db, self.index, self.element_count);
                extractor.extract(member)
            })
            .collect();
        match elem_types.len() {
            0 => None,
            1 => Some(elem_types[0]),
            _ => {
                // Filter out `any` types to preserve specificity (e.g., `[any] & [1]` → `1`)
                let non_any: Vec<TypeId> = elem_types
                    .iter()
                    .copied()
                    .filter(|&t| t != TypeId::ANY)
                    .collect();
                if non_any.is_empty() {
                    Some(TypeId::ANY)
                } else if non_any.len() == 1 {
                    Some(non_any[0])
                } else {
                    Some(self.db.intersection(non_any))
                }
            }
        }
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
                // Contextual property lookup is a read-side query, so optional
                // properties expose `T | undefined`. Callers that specifically
                // need the declared/raw property type should use the dedicated
                // raw-property query helpers instead of this contextual API.
                return Some(crate::utils::optional_property_type(self.db, prop));
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

    fn visit_array(&mut self, elem_type: TypeId) -> Self::Output {
        // For numeric property names (e.g., "0", "1"), the contextual type for
        // array elements applies. This matches tsc's behavior where `{ 0: expr }`
        // with contextual type `T[]` gets contextual element type `T`.
        if self.is_numeric_name {
            Some(elem_type)
        } else {
            None
        }
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        // For numeric property names, extract the specific tuple element type.
        // E.g., `{ 1: expr }` with contextual type `[string, boolean]` gets `boolean`.
        if self.is_numeric_name {
            let name_str = self.db.resolve_atom(self.name_atom);
            let index: usize = name_str.parse().ok()?;
            let elements = self.db.tuple_list(crate::types::TupleListId(list_id));
            if let Some(elem) = elements.get(index) {
                return Some(elem.type_id);
            }
            // Index out of bounds for the tuple — no contextual type
            None
        } else {
            None
        }
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
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    PropertyExtractor::from_atom(self.db, self.name_atom, self.is_numeric_name);
                extractor.extract(member)
            })
            .collect();
        collect_from_intersection(self.db, types, |db, tys| db.intersection(tys))
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
    extract_param_type_at_inner(db, params, index, None)
}

/// Extract the parameter type at `index` with knowledge of the total argument count.
/// When `arg_count` is provided, variadic tuple rest parameters are resolved correctly
/// by mapping argument positions through prefix/variadic/tail structure.
pub(crate) fn extract_param_type_at_for_call(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
    index: usize,
    arg_count: usize,
) -> Option<TypeId> {
    extract_param_type_at_inner(db, params, index, Some(arg_count))
}

fn repair_array_callback_value_param(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
    index: usize,
    ty: TypeId,
) -> TypeId {
    if index != 0 || params.len() < 3 {
        return ty;
    }

    let Some(array_elem) = crate::type_queries::get_array_element_type(db, params[2].type_id)
    else {
        return ty;
    };

    if ty != array_elem
        && crate::is_subtype_of(db, ty, array_elem)
        && !crate::is_subtype_of(db, array_elem, ty)
    {
        array_elem
    } else {
        ty
    }
}

/// Extract the contextual type for a **rest** callback parameter at position `index`.
///
/// Unlike `extract_param_type_at` which returns the individual element type at that position,
/// this collects all remaining parameter types from the contextual function into a tuple.
///
/// ## Examples:
/// - Contextual `(...values: [A, B, C]) => void`, index 0 → `[A, B, C]`
/// - Contextual `(a: A, b: B, c: C) => void`, index 0 → `[A, B, C]`
/// - Contextual `(a: A, b: B, c: C) => void`, index 1 → `[B, C]`
/// - Contextual `(a: A, ...rest: B[]) => void`, index 1 → `B[]` (the rest type)
pub(crate) fn extract_rest_param_type_at(
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

    if index >= rest_start {
        if let Some(rp) = rest_param {
            let mut normalized = rp.clone();
            if let Some(evaluated) = evaluate_rest_like_type(db, normalized.type_id) {
                normalized.type_id = evaluated;
            }
            let expanded = crate::type_queries::unpack_tuple_rest_parameter(db, &normalized);
            let tuple_unpacked = expanded.len() > 1
                || expanded.first().is_some_and(|param| {
                    param.type_id != normalized.type_id
                        || param.rest != normalized.rest
                        || param.optional != normalized.optional
                });
            if tuple_unpacked {
                let mut expanded_params = params[..rest_start].to_vec();
                expanded_params.extend(expanded);
                return extract_rest_param_type_at(db, &expanded_params, index);
            }

            // The callback rest param aligns with the contextual function's rest param.
            // Return the rest param's type directly (it's already a tuple or array type).
            return Some(normalized.type_id);
        }
        // Past the end with no rest param — no contextual type.
        return None;
    }

    // The callback rest param starts before the contextual function's rest param.
    // Collect remaining fixed params + rest param into a tuple.
    let remaining_fixed: Vec<TupleElement> = params[index..rest_start]
        .iter()
        .map(|p| TupleElement {
            type_id: p.type_id,
            name: p.name,
            optional: p.optional,
            rest: false,
        })
        .collect();

    if let Some(rp) = rest_param {
        // Has a rest param — build tuple with fixed elements + spread of rest type.
        let mut elements = remaining_fixed;
        elements.push(TupleElement {
            type_id: rp.type_id,
            name: rp.name,
            optional: false,
            rest: true,
        });
        Some(db.tuple(elements))
    } else {
        // No rest param — just build a tuple from the remaining fixed params.
        if remaining_fixed.is_empty() {
            // Empty tuple
            Some(db.tuple(vec![]))
        } else {
            Some(db.tuple(remaining_fixed))
        }
    }
}

fn evaluate_rest_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    let result = match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
        Some(
            TypeData::Lazy(_)
            | TypeData::Mapped(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Application(_),
        ) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            (evaluated != type_id).then_some(evaluated)
        }
        _ => None,
    };
    if std::env::var_os("TSZ_DEBUG_CONTEXTUAL_REST_EVAL").is_some()
        && matches!(db.lookup(type_id), Some(TypeData::IndexAccess(_, _)))
    {
        let mut fmt = TypeFormatter::new(db);
        let raw = fmt.format(type_id).into_owned();
        let evaluated = result
            .map(|ty| {
                let mut fmt = TypeFormatter::new(db);
                fmt.format(ty).into_owned()
            })
            .unwrap_or_else(|| "<none>".to_string());
        eprintln!("contextual-rest-eval raw={raw} evaluated={evaluated}");
    }
    result
}

/// Check if `index` falls at a rest parameter position in the given parameter list.
pub(crate) fn is_rest_position(params: &[ParamInfo], index: usize) -> bool {
    let has_rest = params.last().is_some_and(|p| p.rest);
    if !has_rest {
        return false;
    }
    let rest_start = params.len().saturating_sub(1);
    index >= rest_start
}

pub(crate) fn is_rest_or_optional_tail_position(params: &[ParamInfo], index: usize) -> bool {
    if is_rest_position(params, index) {
        return true;
    }
    if index >= params.len() {
        return false;
    }
    params[index..]
        .iter()
        .all(|param| param.optional || param.rest)
}

fn extract_param_type_at_inner(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
    index: usize,
    arg_count: Option<usize>,
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
        if let Some(evaluated) = evaluate_rest_like_type(db, last_param.type_id) {
            let mut mock_params = params.to_vec();
            mock_params
                .last_mut()
                .expect("mock_params is non-empty after to_vec from non-empty params")
                .type_id = evaluated;
            return extract_param_type_at_inner(db, &mock_params, index, arg_count);
        }
        if let Some(TypeData::Array(elem)) = db.lookup(last_param.type_id) {
            return Some(elem);
        }
        if let Some(TypeData::Union(members)) = db.lookup(last_param.type_id) {
            let members = db.type_list(members);
            let types: Vec<TypeId> = members
                .iter()
                .rev()
                .filter_map(|&member| {
                    let mut mock_params = params.to_vec();
                    mock_params
                        .last_mut()
                        .expect("mock_params is non-empty after to_vec from non-empty params")
                        .type_id = member;
                    extract_param_type_at_inner(db, &mock_params, index, arg_count)
                })
                .collect();
            if std::env::var_os("TSZ_DEBUG_CONTEXTUAL_REST_EVAL").is_some() {
                let mut fmt = TypeFormatter::new(db);
                let rest = fmt.format(last_param.type_id).into_owned();
                let parts: Vec<_> = types
                    .iter()
                    .map(|&ty| {
                        let mut fmt = TypeFormatter::new(db);
                        fmt.format(ty).into_owned()
                    })
                    .collect();
                eprintln!(
                    "contextual-rest-union index={} arg_count={:?} rest={} members={parts:?}",
                    index, arg_count, rest
                );
            }
            return collect_single_or_union_no_reduce(db, types);
        }
        if let Some(TypeData::Tuple(elements_id)) = db.lookup(last_param.type_id) {
            let elements = db.tuple_list(elements_id);
            // Check for variadic tuple pattern: a rest element with non-rest elements after it.
            // e.g., [...T[], U] has rest at index 0 with tail element U at index 1.
            // Only use the expensive variadic expansion path when there are tail elements
            // after the rest position, which is the signature of variadic tuple types.
            let rest_pos = elements.iter().position(|e| e.rest);
            let has_tail_after_rest = rest_pos.is_some_and(|pos| pos + 1 < elements.len());

            if has_tail_after_rest && let Some(count) = arg_count {
                // Use variadic-aware mapping: expand the tuple rest structure
                // and map argument position through prefix/variadic/tail.
                let rest_arg_count = count.saturating_sub(rest_start);
                return variadic_tuple_element_type(db, &elements, rest_index, rest_arg_count);
            }

            // Non-variadic tuple or no arg_count: direct indexing
            if rest_index < elements.len() {
                let elem = &elements[rest_index];
                if elem.rest {
                    // Rest elements store the array type; extract element type
                    return Some(rest_element_contextual_type(db, elem.type_id));
                }
                return Some(elem.type_id);
            } else if let Some(last_elem) = elements.last()
                && last_elem.rest
            {
                return Some(rest_element_contextual_type(db, last_elem.type_id));
            }
            // If out of bounds of the tuple constraint without rest, return undefined/unknown?
            // Fall through
        } else if let Some(TypeData::TypeParameter(param_info)) = db.lookup(last_param.type_id) {
            if arg_count.is_some() {
                return Some(last_param.type_id);
            }
            if let Some(constraint) = param_info.constraint {
                let mut mock_params = params.to_vec();
                mock_params
                    .last_mut()
                    .expect("mock_params is non-empty after to_vec from non-empty params")
                    .type_id = constraint;
                return extract_param_type_at_inner(db, &mock_params, index, arg_count);
            }
        } else if let Some(TypeData::Intersection(members)) = db.lookup(last_param.type_id) {
            let members = db.type_list(members);
            for &m in members.iter() {
                let mut mock_params = params.to_vec();
                mock_params
                    .last_mut()
                    .expect("mock_params is non-empty after to_vec from non-empty params")
                    .type_id = m;
                if let Some(param_type) =
                    extract_param_type_at_inner(db, &mock_params, index, arg_count)
                {
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
        }
        // If we still didn't extract a specific type, check constraint
        if arg_count.is_some()
            && (crate::type_queries::is_type_parameter_like(db, last_param.type_id)
                || crate::type_queries::contains_type_parameters_db(db, last_param.type_id))
        {
            return Some(last_param.type_id);
        }
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(db, last_param.type_id)
        {
            let mut mock_params = params.to_vec();
            mock_params
                .last_mut()
                .expect("mock_params is non-empty after to_vec from non-empty params")
                .type_id = constraint;
            if let Some(param_type) =
                extract_param_type_at_inner(db, &mock_params, index, arg_count)
            {
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

/// Map an argument position through a variadic tuple's prefix/variadic/tail structure.
///
/// For a tuple like `[A, ...B[], C, D]` with 5 rest arguments:
/// - Positions `0..prefix_len` map to prefix elements
/// - Positions after (`rest_arg_count` - `tail_len`) map to tail elements
/// - Positions in between map to the variadic element type
fn variadic_tuple_element_type(
    db: &dyn TypeDatabase,
    elements: &[crate::TupleElement],
    offset: usize,
    rest_arg_count: usize,
) -> Option<TypeId> {
    let rest_index = elements.iter().position(|elem| elem.rest)?;

    let (prefix, rest_and_tail) = elements.split_at(rest_index);
    let rest_elem = &rest_and_tail[0];
    let outer_tail = &rest_and_tail[1..];

    let expansion = crate::utils::expand_tuple_rest(db, rest_elem.type_id);
    let prefix_len = prefix.len();
    let rest_fixed_len = expansion.fixed.len();
    let expansion_tail_len = expansion.tail.len();
    let outer_tail_len = outer_tail.len();
    let total_suffix_len = expansion_tail_len + outer_tail_len;

    if let Some(variadic) = expansion.variadic {
        let suffix_start = rest_arg_count.saturating_sub(total_suffix_len);
        if offset >= suffix_start {
            let suffix_index = offset - suffix_start;
            if suffix_index < expansion_tail_len {
                return Some(expansion.tail[suffix_index].type_id);
            }
            let outer_index = suffix_index - expansion_tail_len;
            if let Some(elem) = outer_tail.get(outer_index) {
                return Some(elem.type_id);
            }
            // Past the outer tail — still in the variadic region.
            // This can happen when probing at a very large index to detect
            // whether a rest parameter exists. Return the variadic element
            // type so the probe correctly reports "has rest param".
            return Some(variadic);
        }
        if offset < prefix_len {
            return Some(prefix[offset].type_id);
        }
        let fixed_end = prefix_len + rest_fixed_len;
        if offset < fixed_end {
            return Some(expansion.fixed[offset - prefix_len].type_id);
        }
        return Some(variadic);
    }

    // No variadic: prefix + expansion.fixed + expansion.tail + outer_tail
    let mut idx = offset;
    if idx < prefix_len {
        return Some(prefix[idx].type_id);
    }
    idx -= prefix_len;
    if idx < rest_fixed_len {
        return Some(expansion.fixed[idx].type_id);
    }
    idx -= rest_fixed_len;
    if idx < expansion_tail_len {
        return Some(expansion.tail[idx].type_id);
    }
    idx -= expansion_tail_len;
    outer_tail.get(idx).map(|elem| elem.type_id)
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
            .map(|ty| repair_array_callback_value_param(self.db, &shape.params, self.index, ty))
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        // tsc's getIntersectedSignatures returns undefined when multiple
        // signatures are present and ANY is generic. A single generic signature
        // still provides contextual types (type params act as contextual types).
        if shape.call_signatures.len() > 1
            && shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty())
        {
            return None;
        }

        // Collect parameter types from all signatures at the given index
        let param_types: Vec<TypeId> = shape
            .call_signatures
            .iter()
            .filter_map(|sig| {
                extract_param_type_at(self.db, &sig.params, self.index).map(|ty| {
                    repair_array_callback_value_param(self.db, &sig.params, self.index, ty)
                })
            })
            .collect();

        if param_types.is_empty() {
            None
        } else if param_types.len() == 1 {
            Some(param_types[0])
        } else {
            // Multiple call signatures with potentially different parameter types.
            // If all signatures agree on the same type, use it.
            let first = param_types[0];
            if param_types.iter().all(|&t| t == first) {
                return Some(first);
            }
            // When signatures disagree, filter out `any` parameters.
            // This handles intersection evaluation artifacts where
            // `T & ((arg: string) => any)` produces a Callable with a degraded
            // `(any?) => any` signature from the unresolved T alongside the real
            // `(arg: string) => any` signature. The `any`-parameterized signatures
            // should not block contextual typing from more specific signatures.
            let non_any: Vec<TypeId> = param_types
                .iter()
                .copied()
                .filter(|&t| t != TypeId::ANY)
                .collect();
            if non_any.is_empty() {
                // All signatures have `any` at this position — provide `any`
                // as contextual type to suppress TS7006.
                Some(TypeId::ANY)
            } else if non_any.len() == 1 {
                Some(non_any[0])
            } else {
                let first_non_any = non_any[0];
                if non_any.iter().all(|&t| t == first_non_any) {
                    Some(first_non_any)
                } else {
                    // Genuinely different non-any types: union them.
                    // tsc creates intersected parameter types across overloaded
                    // signatures; we union them which also suppresses false TS7006
                    // (e.g., Callback with (null, T) | (Error, null) overloads).
                    collect_single_or_union(self.db, non_any)
                }
            }
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
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    ParameterExtractor::new(self.db, self.index, self.no_implicit_any);
                extractor.extract(member)
            })
            .collect();
        collect_from_intersection(self.db, types, |db, tys| db.union(tys))
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract the **rest parameter type** from callable types.
///
/// Unlike `ParameterExtractor` which returns the individual element type at a position,
/// this returns the full tuple/array type for a rest callback parameter.
pub(crate) struct RestParameterExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
}

impl<'a> RestParameterExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize) -> Self {
        Self { db, index }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for RestParameterExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        extract_rest_param_type_at(self.db, &shape.params, self.index)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        // Use the first call signature that provides a rest type
        for sig in &shape.call_signatures {
            if let Some(ty) = extract_rest_param_type_at(self.db, &sig.params, self.index) {
                return Some(ty);
            }
        }
        None
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor = RestParameterExtractor::new(self.db, self.index);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union(self.db, types)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor = RestParameterExtractor::new(self.db, self.index);
                extractor.extract(member)
            })
            .collect();
        collect_from_intersection(self.db, types, |db, tys| db.union(tys))
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

        extract_param_type_at_for_call(self.db, &shape.params, self.index, self.arg_count)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        let mut matched = false;
        let mut param_types: Vec<TypeId> = Vec::new();

        let mut matching_call_signatures: Vec<_> = shape
            .call_signatures
            .iter()
            .filter(|sig| self.signature_accepts_arg_count(&sig.params, self.arg_count))
            .collect();
        if matching_call_signatures
            .iter()
            .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
        {
            matching_call_signatures
                .retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
        }

        // tsc's getIntersectedSignatures returns undefined when multiple
        // signatures are present and ANY is generic. This prevents contextual
        // typing when assigning arrow functions to overloaded types that have
        // both generic and non-generic call signatures.
        // Only apply when there's a MIX of generic and non-generic signatures
        // (genuine overloads). When ALL signatures are generic, they likely
        // come from union member merging and should still provide contextual types.
        if matching_call_signatures.len() > 1 {
            let has_generic = matching_call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
            let has_non_generic = matching_call_signatures
                .iter()
                .any(|sig| sig.type_params.is_empty());
            if has_generic && has_non_generic {
                return None;
            }
        }

        for sig in matching_call_signatures {
            matched = true;
            if let Some(param_type) =
                extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
            {
                param_types.push(param_type);
            }
        }

        if param_types.is_empty() && !matched {
            param_types = shape
                .call_signatures
                .iter()
                .filter_map(|sig| {
                    extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
                })
                .collect();
        }

        // If no call signatures matched, check construct signatures.
        // This handles super() calls and new expressions where the callee
        // is a Callable with construct signatures (not call signatures).
        // NOTE: Generic construct signatures still provide useful contextual
        // types for callback arguments (possibly involving type parameters),
        // and suppressing them causes false TS7006 in constructor calls.
        if param_types.is_empty() {
            matched = false;
            let mut matching_construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .filter(|sig| self.signature_accepts_arg_count(&sig.params, self.arg_count))
                .collect();
            if matching_construct_signatures
                .iter()
                .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
            {
                matching_construct_signatures
                    .retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
            }
            for sig in matching_construct_signatures {
                matched = true;
                if let Some(param_type) =
                    extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
                {
                    param_types.push(param_type);
                }
            }
            if param_types.is_empty() && !matched {
                param_types = shape
                    .construct_signatures
                    .iter()
                    .filter_map(|sig| {
                        extract_param_type_at_for_call(
                            self.db,
                            &sig.params,
                            self.index,
                            self.arg_count,
                        )
                    })
                    .collect();
            }
        }

        // Avoid contextual-type poisoning from catch-all `any` signatures
        // (e.g. implementation signatures like `(...args: any[])` on overloaded
        // constructors). If at least one non-`any` contextual type exists, prefer
        // those and drop `any` contributors.
        if param_types.len() > 1 {
            let has_non_any = param_types.iter().any(|&ty| ty != TypeId::ANY);
            if has_non_any {
                param_types.retain(|&ty| ty != TypeId::ANY);
            }
        }

        collect_single_or_union_no_reduce(self.db, param_types)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions, extract parameter types from each member and combine.
        // Use no-reduce union to preserve all callback type variants — see
        // collect_single_or_union_no_reduce doc comment for rationale.
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    ParameterForCallExtractor::new(self.db, self.index, self.arg_count);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union_no_reduce(self.db, types)
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

// =============================================================================
// RestPositionCheckExtractor
// =============================================================================

/// Visitor to check if a given argument index falls at a rest parameter position
/// for a callable type. Used by TS2556 checking: non-tuple array spreads must
/// only land on rest parameter positions.
///
/// For overloaded callables, returns `true` only if ALL matching signatures
/// have the index at a rest position. This is conservative — if any signature
/// treats the position as non-rest, the spread is invalid.
pub(crate) struct RestPositionCheckExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
}

impl<'a> RestPositionCheckExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize, arg_count: usize) -> Self {
        Self {
            db,
            index,
            arg_count,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> bool {
        self.visit_type(self.db, type_id).unwrap_or(false)
    }

    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            arg_count >= required_count
        } else {
            arg_count >= required_count && arg_count <= params.len()
        }
    }
}

impl<'a> TypeVisitor for RestPositionCheckExtractor<'a> {
    type Output = Option<bool>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        Some(is_rest_position(&shape.params, self.index))
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        // Check both call and construct signatures (super() uses construct sigs)
        let all_sigs: Vec<&[ParamInfo]> = shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .map(|sig| sig.params.as_slice())
            .collect();

        if all_sigs.is_empty() {
            return None;
        }

        // Check matching signatures first
        let mut any_matched = false;
        let mut all_rest = true;
        for &params in &all_sigs {
            if self.signature_accepts_arg_count(params, self.arg_count) {
                any_matched = true;
                if !is_rest_position(params, self.index) {
                    all_rest = false;
                }
            }
        }

        if !any_matched {
            // Fall back to all signatures
            for &params in &all_sigs {
                if !is_rest_position(params, self.index) {
                    return Some(false);
                }
            }
            return Some(true);
        }

        Some(all_rest)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        // If any member says non-rest, the spread is invalid
        for &m in members.iter() {
            let mut extractor =
                RestPositionCheckExtractor::new(self.db, self.index, self.arg_count);
            if !extractor.extract(m) {
                return Some(false);
            }
        }
        Some(true)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &m in members.iter() {
            let mut extractor =
                RestPositionCheckExtractor::new(self.db, self.index, self.arg_count);
            if let Some(result) = extractor.visit_type(self.db, m) {
                return Some(result);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}

pub(crate) struct RestOrOptionalTailPositionExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
}

impl<'a> RestOrOptionalTailPositionExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize, arg_count: usize) -> Self {
        Self {
            db,
            index,
            arg_count,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> bool {
        self.visit_type(self.db, type_id).unwrap_or(false)
    }

    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            arg_count >= required_count
        } else {
            arg_count >= required_count && arg_count <= params.len()
        }
    }
}

impl<'a> TypeVisitor for RestOrOptionalTailPositionExtractor<'a> {
    type Output = Option<bool>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        Some(is_rest_or_optional_tail_position(&shape.params, self.index))
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        let all_sigs: Vec<&[ParamInfo]> = shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .map(|sig| sig.params.as_slice())
            .collect();

        if all_sigs.is_empty() {
            return None;
        }

        let mut any_matched = false;
        let mut all_allowed = true;
        for &params in &all_sigs {
            if self.signature_accepts_arg_count(params, self.arg_count) {
                any_matched = true;
                if !is_rest_or_optional_tail_position(params, self.index) {
                    all_allowed = false;
                }
            }
        }

        if !any_matched {
            for &params in &all_sigs {
                if !is_rest_or_optional_tail_position(params, self.index) {
                    return Some(false);
                }
            }
            return Some(true);
        }

        Some(all_allowed)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &m in members.iter() {
            let mut extractor =
                RestOrOptionalTailPositionExtractor::new(self.db, self.index, self.arg_count);
            if !extractor.extract(m) {
                return Some(false);
            }
        }
        Some(true)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &m in members.iter() {
            let mut extractor =
                RestOrOptionalTailPositionExtractor::new(self.db, self.index, self.arg_count);
            if let Some(result) = extractor.visit_type(self.db, m) {
                return Some(result);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}
