//! Index access type evaluation.
//!
//! Handles TypeScript's index access types: `T[K]`
//! Including property access, array indexing, and tuple indexing.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, IntrinsicKind, LiteralValue, MappedModifier, MappedTypeId,
    ObjectShape, ObjectShapeId, PropertyInfo, SymbolRef, TupleElement, TupleListId, TypeData,
    TypeId, TypeListId, TypeParamInfo,
};
use crate::utils;
use crate::visitor::{
    TypeVisitor, array_element_type, intersection_list_id, keyof_inner_type, literal_number,
    tuple_list_id, union_list_id,
};
use crate::{ApparentMemberKind, TypeDatabase};

use super::super::evaluate::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID, TypeEvaluator,
};
use super::apparent::make_apparent_method_type;
use crate::objects::apparent::is_member;

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

/// Standalone helper to get array member kind.
/// Extracted from `TypeEvaluator` to be usable by visitors.
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

    /// Check if the index type is generic (deferrable).
    ///
    /// When evaluating an index access during generic instantiation,
    /// if the index is still a generic type (like a type parameter),
    /// we must defer evaluation instead of returning UNDEFINED.
    fn is_generic_index(&self) -> bool {
        let key = match self.evaluator.interner().lookup(self.index_type) {
            Some(k) => k,
            None => return false,
        };

        matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::KeyOf(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::Conditional(_)
                | TypeData::TemplateLiteral(_) // Templates might resolve to generic strings
                | TypeData::Intersection(_)
        )
    }

    /// Check if the index type is an intersection that contains the mapped type's constraint.
    ///
    /// This handles cases like `string & keyof T` indexing into `{ [P in keyof T]: V }`,
    /// where the intersection is a subset of the constraint `keyof T`.
    fn intersection_contains_mapped_constraint(&self, constraint: TypeId) -> bool {
        let interner = self.evaluator.interner();
        let Some(list_id) = intersection_list_id(interner, self.index_type) else {
            return false;
        };
        let members = interner.type_list(list_id);
        members.contains(&constraint)
    }

    fn evaluate_type_param(&mut self, param: &TypeParamInfo) -> Option<TypeId> {
        if let Some(constraint) = param.constraint {
            if constraint == self.object_type {
                // Recursive constraint — defer to avoid infinite loop.
                Some(
                    self.evaluator
                        .interner()
                        .index_access(self.object_type, self.index_type),
                )
            } else if self.is_generic_index() && self.is_constraint_type_parameter(constraint) {
                // When the index is generic AND the constraint is another type parameter,
                // keep the indexed access deferred. This preserves the distinction between
                // U[K] and T[K] when U extends T — if we substituted the constraint,
                // both would collapse to T[K] and assignability would trivially pass.
                //
                // When the constraint is concrete (e.g., Record<K, number>), we still
                // substitute so T[K] properly resolves to number.
                Some(
                    self.evaluator
                        .interner()
                        .index_access(self.object_type, self.index_type),
                )
            } else {
                // Concrete constraint or concrete index — use the constraint to resolve.
                Some(
                    self.evaluator
                        .recurse_index_access(constraint, self.index_type),
                )
            }
        } else {
            // No constraint — produce a deferred IndexAccess.
            Some(
                self.evaluator
                    .interner()
                    .index_access(self.object_type, self.index_type),
            )
        }
    }

    /// Check if a constraint type is itself a type parameter.
    fn is_constraint_type_parameter(&self, constraint: TypeId) -> bool {
        matches!(
            self.evaluator.interner().lookup(constraint),
            Some(TypeData::TypeParameter(_))
        )
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

        let result = self
            .evaluator
            .evaluate_object_index(&shape.properties, self.index_type);

        // CRITICAL FIX: If we can't find the property, but the index is generic,
        // we must defer evaluation (return None) instead of returning UNDEFINED.
        // This prevents mapped type template evaluation from hardcoding UNDEFINED
        // during generic instantiation.
        if result == TypeId::UNDEFINED && self.is_generic_index() {
            return None;
        }

        Some(result)
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        let shape = self
            .evaluator
            .interner()
            .object_shape(ObjectShapeId(shape_id));

        let result = self
            .evaluator
            .evaluate_object_with_index(&shape, self.index_type);

        // CRITICAL FIX: Same deferral logic for objects with index signatures
        if result == TypeId::UNDEFINED && self.is_generic_index() {
            return None;
        }

        Some(result)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self
            .evaluator
            .interner()
            .callable_shape(CallableShapeId(shape_id));

        let result = self
            .evaluator
            .evaluate_callable_index(&shape, self.index_type);

        if result == TypeId::UNDEFINED && self.is_generic_index() {
            return None;
        }

        Some(result)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.evaluator.interner().type_list(TypeListId(list_id));
        const MAX_UNION_INDEX_SIZE: usize = 100;
        if members.len() > MAX_UNION_INDEX_SIZE {
            self.evaluator.mark_depth_exceeded();
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

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        // For intersection types, evaluate all members and combine successful lookups.
        // Returning the first non-undefined result can incorrectly lock onto `never`
        // for mapped/index-signature helper intersections.
        let members = self.evaluator.interner().type_list(TypeListId(list_id));
        let mut results = Vec::new();
        for &member in members.iter() {
            let result = self.evaluator.recurse_index_access(member, self.index_type);
            if result == TypeId::ERROR {
                return Some(TypeId::ERROR);
            }
            if result != TypeId::UNDEFINED {
                results.push(result);
            }
        }
        if results.is_empty() {
            Some(TypeId::UNDEFINED)
        } else {
            Some(self.evaluator.interner().union(results))
        }
    }

    fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
        // CRITICAL: Classes and interfaces are represented as Lazy types.
        // We must resolve them and then perform the index access lookup.
        let def_id = crate::def::DefId(def_id);
        if let Some(resolved) = self
            .evaluator
            .resolver()
            .resolve_lazy(def_id, self.evaluator.interner())
        {
            // Route through recurse_index_access (not evaluate_index_access directly)
            // so the call goes through evaluate() and its RecursionGuard. This prevents
            // stack overflow when Lazy types form cycles (e.g. DefId(1) → Lazy(DefId(1))).
            return Some(
                self.evaluator
                    .recurse_index_access(resolved, self.index_type),
            );
        }
        None
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
        let resolved = if let Some(def_id) = self.evaluator.resolver().symbol_to_def_id(symbol_ref)
        {
            self.evaluator
                .resolver()
                .resolve_lazy(def_id, self.evaluator.interner())?
        } else {
            self.evaluator
                .resolver()
                .resolve_symbol_ref(symbol_ref, self.evaluator.interner())?
        };
        if resolved == self.object_type {
            Some(
                self.evaluator
                    .interner()
                    .index_access(self.object_type, self.index_type),
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

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        let mapped = self
            .evaluator
            .interner()
            .mapped_type(MappedTypeId(mapped_id));

        // Optimization: Mapped[K] -> Template[P/K] where K matches constraint
        // This handles cases like `Ev<K>["callback"]` where Ev<K> is a mapped type
        // over K, without needing to expand the mapped type (which fails for TypeParameter K).

        // Only apply if no name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        // Direct match: index type exactly equals the constraint
        let can_substitute = mapped.constraint == self.index_type
            // Implicit index signature: when the constraint is `keyof T`,
            // string/number are valid key types because keyof T always
            // includes string | number | symbol for any T.
            // This handles for-in loops: `for (let k in obj) { result[k] = ... }`
            // where `k: string` and `result: { [K in keyof T]: V }`.
            || (matches!(self.index_type, TypeId::STRING | TypeId::NUMBER)
                && keyof_inner_type(self.evaluator.interner(), mapped.constraint).is_some())
            // Intersection index containing the constraint: when index is
            // `string & keyof T` and constraint is `keyof T`, the intersection
            // is a subset of the constraint. This handles for-in loops where the
            // key type is refined to `string & keyof T`.
            || self.intersection_contains_mapped_constraint(mapped.constraint);

        if can_substitute {
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, self.index_type);

            let mut value_type = self.evaluator.evaluate(instantiate_type(
                self.evaluator.interner(),
                mapped.template,
                &subst,
            ));

            // Handle optional modifier
            if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
                value_type = self
                    .evaluator
                    .interner()
                    .union2(value_type, TypeId::UNDEFINED);
            }

            return Some(value_type);
        }

        None
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
        get_or_init_array_member_types(&mut self.array_member_types_cache, self.db)
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
            // Explicitly handle other literals to avoid incorrect fallback
            LiteralValue::Boolean(_) | LiteralValue::BigInt(_) => Some(TypeId::UNDEFINED),
        }
    }

    /// Signal "use the default fallback" for unhandled type variants
    fn default_output() -> Self::Output {
        None
    }
}

/// Get the element type of a rest element, handling arrays and nested tuples.
///
/// For arrays, returns the element type. For tuples, returns the union of all element types.
/// Otherwise returns the type as-is.
fn rest_element_type_full(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
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
        rest_element_type_full(db, element.type_id)
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
/// Uses Option<TypeId> to signal "use default fallback" via None.
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

    /// Driver method that handles the fallback logic
    fn evaluate(&mut self, index_type: TypeId) -> TypeId {
        let result = self.visit_type(self.db, index_type);
        result.unwrap_or(TypeId::UNDEFINED)
    }

    /// Get the type of a tuple element, handling optional and rest elements
    fn tuple_element_type(&self, element: &TupleElement) -> TypeId {
        tuple_element_type_with_rest(self.db, element)
    }

    /// Get the type at a specific literal index, handling rest elements
    fn tuple_index_literal(&self, idx: usize) -> Option<TypeId> {
        for (logical_idx, element) in self.elements.iter().enumerate() {
            if element.rest {
                if let Some(rest_elements) = tuple_list_id(self.db, element.type_id) {
                    let rest_elements = self.db.tuple_list(rest_elements);
                    let inner_idx = idx.saturating_sub(logical_idx);
                    // Recursively search in rest elements
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

    /// Get all tuple element types as a union
    fn get_all_element_types(&self) -> Vec<TypeId> {
        self.elements
            .iter()
            .map(|e| self.tuple_element_type(e))
            .collect()
    }

    /// Get array member types (cached)
    fn get_array_member_types(&mut self) -> Vec<TypeId> {
        get_or_init_array_member_types(&mut self.array_member_types_cache, self.db)
    }

    /// Compute the fixed length of the tuple, resolving rest spreads to
    /// fixed-length inner tuples. Returns `None` if the length is not fixed
    /// (e.g., rest element spreads an array or variadic tuple) or exceeds
    /// the maximum tuple size.
    ///
    /// Uses an iterative approach for single-rest-element tuples (the common
    /// `[T, ...Acc]` accumulator pattern), and bounded recursion for
    /// multi-rest tuples to prevent O(2^n) traversal of branching spreads.
    fn fixed_length(&self) -> Option<usize> {
        const MAX_FIXED_LENGTH: usize = 1000;

        let mut total = 0usize;
        let mut current_type = None; // type_id of rest element to descend into

        // Process current elements
        let mut rest_count = 0;
        for element in self.elements {
            if element.rest {
                rest_count += 1;
                if rest_count > 1 {
                    // Multiple rest elements at same level — bail
                    return None;
                }
                current_type = Some(element.type_id);
            } else {
                total += 1;
                if total > MAX_FIXED_LENGTH {
                    return None;
                }
            }
        }

        // Iteratively descend into single-rest chains
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
                    total += 1;
                    if total > MAX_FIXED_LENGTH {
                        return None;
                    }
                }
            }
        }

        Some(total)
    }

    /// Check for known array members (length, methods)
    fn get_array_member_kind(&self, name: &str) -> Option<ApparentMemberKind> {
        if name == "length" {
            // For fixed-length tuples, return the literal length type (e.g., 0, 1, 2)
            // instead of generic `number`. This handles both simple tuples and tuples
            // with rest spreads that resolve to fixed-length inner tuples (e.g.,
            // `[T, ...Acc]` where `Acc` is `[any, any]` → length 3).
            // Required for patterns like `Acc["length"] extends N` in tail-recursive
            // conditional types.
            if let Some(len) = self.fixed_length() {
                let literal = self.db.literal_number(len as f64);
                return Some(ApparentMemberKind::Value(literal));
            }
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

impl<'a> TypeVisitor for TupleKeyVisitor<'a> {
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
                // Return union of all element types + array member types
                let mut types = self.get_all_element_types();
                types.extend(self.get_array_member_types());
                if types.is_empty() {
                    Some(TypeId::NEVER)
                } else {
                    Some(self.db.union(types))
                }
            }
            IntrinsicKind::Number => {
                // Return union of all element types
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
                // Check if it's a numeric property name (e.g., "0", "1", "42")
                if utils::is_numeric_property_name(self.db, *atom) {
                    let name = self.db.resolve_atom_ref(*atom);
                    if let Ok(idx) = name.as_ref().parse::<i64>()
                        && let Ok(idx) = usize::try_from(idx)
                    {
                        return self.tuple_index_literal(idx).or(Some(TypeId::UNDEFINED));
                    }
                    return Some(TypeId::UNDEFINED);
                }

                // Check for known array members
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
            // Explicitly handle other literals to avoid incorrect fallback
            LiteralValue::Boolean(_) | LiteralValue::BigInt(_) => Some(TypeId::UNDEFINED),
        }
    }

    /// Signal "use the default fallback" for unhandled type variants
    fn default_output() -> Self::Output {
        None
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Helper to recursively evaluate an index access while respecting depth limits.
    /// Creates an `IndexAccess` type and evaluates it through the main `evaluate()` method.
    pub(crate) fn recurse_index_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        let index_access = self.interner().index_access(object_type, index_type);
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
                self.mark_depth_exceeded();
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
        self.interner().index_access(object_type, index_type)
    }

    /// Evaluate property access on an object type
    pub(crate) fn evaluate_object_index(
        &self,
        props: &[PropertyInfo],
        index_type: TypeId,
    ) -> TypeId {
        // If index is a literal string or unique symbol, look up the property directly
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
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

        // If index is a literal string or unique symbol, look up the property first,
        // then fallback to string index.
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
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

    /// Evaluate index access on a callable type (class constructor / `typeof ClassName`).
    ///
    /// Callable types have static properties and index signatures, analogous to
    /// `ObjectWithIndex`. This resolves type-level indexed access like
    /// `(typeof B)["foo"]` or `(typeof B)[number]`.
    pub(crate) fn evaluate_callable_index(
        &self,
        shape: &CallableShape,
        index_type: TypeId,
    ) -> TypeId {
        // If index is a union, evaluate each member
        if let Some(members) = union_list_id(self.interner(), index_type) {
            let members = self.interner().type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_callable_index(shape, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access() {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner().union(results);
        }

        // If index is a literal string or unique symbol, look up properties first,
        // then fallback to index sigs.
        if let Some(name) =
            crate::type_queries::get_literal_property_name(self.interner(), index_type)
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
        crate::utils::optional_property_type(self.interner(), prop)
    }

    pub(crate) fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access() || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner().union2(type_id, TypeId::UNDEFINED)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        rest_element_type_full(self.interner(), type_id)
    }

    /// Evaluate index access on a tuple type
    pub(crate) fn evaluate_tuple_index(
        &self,
        elements: &[TupleElement],
        index_type: TypeId,
    ) -> TypeId {
        // Use TupleKeyVisitor to handle the index type
        // The visitor handles Union distribution internally via visit_union
        let mut visitor = TupleKeyVisitor::new(self.interner(), elements);
        let result = visitor.evaluate(index_type);

        // Add undefined if unchecked indexed access is allowed
        self.add_undefined_if_unchecked(result)
    }

    pub(crate) fn evaluate_array_index(&self, elem: TypeId, index_type: TypeId) -> TypeId {
        // Use ArrayKeyVisitor to handle the index type
        // The visitor handles Union distribution internally via visit_union
        let mut visitor = ArrayKeyVisitor::new(self.interner(), elem);
        let result = visitor.evaluate(index_type);

        // Add undefined if unchecked indexed access is allowed
        self.add_undefined_if_unchecked(result)
    }
}
