//! Index access type evaluation.
//!
//! Handles TypeScript's index access types: `T[K]`
//! Including property access, array indexing, and tuple indexing.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::objects::{PropertyCollectionResult, collect_properties};
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
    fn index_is_symbolic_key_space(&self, constraint: TypeId) -> bool {
        if self.index_type != constraint {
            return false;
        }

        !matches!(
            self.evaluator.interner().lookup(self.index_type),
            Some(
                TypeData::Literal(_)
                    | TypeData::Intrinsic(
                        IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Symbol
                    )
            )
        )
    }

    fn instantiate_mapped_template_with_constraint_param(
        &mut self,
        mapped: &crate::types::MappedType,
    ) -> TypeId {
        let constrained_key = self.evaluator.interner().type_param(TypeParamInfo {
            name: mapped.type_param.name,
            constraint: Some(mapped.constraint),
            default: mapped.type_param.default,
            is_const: mapped.type_param.is_const,
        });

        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, constrained_key);

        let mut value_type = self.evaluator.evaluate(instantiate_type(
            self.evaluator.interner(),
            mapped.template,
            &subst,
        ));

        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            value_type = self
                .evaluator
                .interner()
                .union2(value_type, TypeId::UNDEFINED);
        }

        value_type
    }

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
    ///
    /// Also handles the case where `keyof Boxified<T>` appears in the intersection
    /// and evaluates to `keyof T` (the constraint). This occurs with homomorphic mapped
    /// types: `keyof { [P in keyof T]: V }` = `keyof T`, but the unevaluated form
    /// `keyof Application(...)` has a different TypeId than `keyof T`.
    fn intersection_contains_mapped_constraint(&mut self, constraint: TypeId) -> bool {
        let members_arc = {
            let interner = self.evaluator.interner();
            let Some(list_id) = intersection_list_id(interner, self.index_type) else {
                return false;
            };
            interner.type_list(list_id)
        };

        if members_arc.contains(&constraint) {
            return true;
        }

        // Evaluate each intersection member and check if any evaluates to the constraint.
        // This handles `keyof Boxified<T>` matching `keyof T` when Boxified<T> is a
        // homomorphic mapped type `{ [P in keyof T]: ... }`.
        for &member in members_arc.iter() {
            let evaluated = self.evaluator.evaluate(member);
            if evaluated == constraint {
                return true;
            }

            // When the evaluator lacks a resolver (e.g., during solver-only evaluation),
            // `keyof Application(Boxified, [T])` can't be expanded to `keyof T`.
            // Handle this by comparing inner KeyOf operands structurally: if both the
            // member and constraint are KeyOf types, and their inner operands are
            // type parameters with the same name, they're semantically equivalent.
            // This occurs with for-in loops where flow narrowing produces
            // `keyof Boxified<T> & string` but the mapped type uses `keyof T`.
            let interner = self.evaluator.interner();
            if let (Some(TypeData::KeyOf(member_inner)), Some(TypeData::KeyOf(constraint_inner))) =
                (interner.lookup(member), interner.lookup(constraint))
            {
                // Direct inner match
                if member_inner == constraint_inner {
                    return true;
                }
                // If the member's inner type is an Application whose type argument
                // is a type parameter matching the constraint's inner type parameter,
                // they're equivalent: keyof Boxified<T> ≡ keyof T for homomorphic types.
                if let Some(TypeData::Application(app_id)) = interner.lookup(member_inner) {
                    let app = interner.type_application(app_id);
                    if app.args.len() == 1 && app.args[0] == constraint_inner {
                        return true;
                    }
                }
                // Same-name type parameter match (different TypeIds, same Atom name)
                if let (
                    Some(TypeData::TypeParameter(member_tp)),
                    Some(TypeData::TypeParameter(constraint_tp)),
                ) = (
                    interner.lookup(member_inner),
                    interner.lookup(constraint_inner),
                ) && member_tp.name == constraint_tp.name
                {
                    return true;
                }
            }
        }

        false
    }

    fn mapped_constraint_contains_index_type(&mut self, constraint: TypeId) -> bool {
        if constraint == self.index_type {
            return true;
        }

        let interner = self.evaluator.interner();
        let same_type_param_name = match (
            interner.lookup(constraint),
            interner.lookup(self.index_type),
        ) {
            (
                Some(TypeData::TypeParameter(constraint_tp)),
                Some(TypeData::TypeParameter(index_tp)),
            ) => constraint_tp.name == index_tp.name,
            _ => false,
        };
        if same_type_param_name {
            return true;
        }

        let members = union_list_id(interner, constraint)
            .or_else(|| intersection_list_id(interner, constraint))
            .map(|list_id| interner.type_list(list_id));
        members.is_some_and(|members| {
            members
                .iter()
                .any(|&member| self.mapped_constraint_contains_index_type(member))
        })
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

    fn can_fast_path_large_union_index(&self) -> bool {
        crate::type_queries::get_literal_property_name(self.evaluator.interner(), self.index_type)
            .is_some()
            || literal_number(self.evaluator.interner(), self.index_type).is_some()
            || matches!(self.index_type, TypeId::STRING | TypeId::NUMBER)
    }

    fn try_fast_index_large_union_member(&mut self, member: TypeId) -> Option<TypeId> {
        match self.evaluator.interner().lookup(member) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.evaluator.interner().object_shape(shape_id);
                Some(
                    self.evaluator
                        .evaluate_object_index(&shape.properties, self.index_type),
                )
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.evaluator.interner().object_shape(shape_id);
                Some(
                    self.evaluator
                        .evaluate_object_with_index(&shape, self.index_type),
                )
            }
            Some(TypeData::Array(element_type)) => Some(
                self.evaluator
                    .evaluate_array_index(element_type, self.index_type),
            ),
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.evaluator.interner().tuple_list(list_id);
                Some(
                    self.evaluator
                        .evaluate_tuple_index(&elements, self.index_type),
                )
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.evaluator.interner().callable_shape(shape_id);
                Some(
                    self.evaluator
                        .evaluate_callable_index(&shape, self.index_type),
                )
            }
            Some(TypeData::ReadonlyType(inner_type)) => {
                self.try_fast_index_large_union_member(inner_type)
            }
            Some(TypeData::Lazy(def_id)) => {
                let resolved = self
                    .evaluator
                    .resolver()
                    .resolve_lazy(def_id, self.evaluator.interner())?;
                if resolved == member {
                    None
                } else {
                    self.try_fast_index_large_union_member(resolved)
                }
            }
            _ => None,
        }
    }

    fn try_fast_index_large_union(&mut self, members: &[TypeId]) -> Option<TypeId> {
        if !self.can_fast_path_large_union_index() {
            return None;
        }

        let mut results = Vec::with_capacity(members.len());
        for &member in members {
            let result = self.try_fast_index_large_union_member(member)?;
            if result != TypeId::UNDEFINED || self.evaluator.no_unchecked_indexed_access() {
                results.push(result);
            }
        }

        if results.is_empty() {
            Some(TypeId::UNDEFINED)
        } else {
            Some(self.evaluator.interner().union(results))
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
            if let Some(result) = self.try_fast_index_large_union(&members) {
                return Some(result);
            }
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
        // When the index is generic (type parameter, keyof, etc.), distributing the
        // index access over intersection members creates incorrect deferred types.
        // For example: ({ a: string } & { b: string })[K] where K extends "a" | "b"
        // would become Union(IndexAccess({a:string}, K), IndexAccess({b:string}, K)),
        // causing false TS2322 because {a:string}["b"] doesn't exist.
        // Fix: merge the intersection into a single object first, then index into it.
        if self.is_generic_index() {
            let members = self.evaluator.interner().type_list(TypeListId(list_id));
            let mut concrete_results = Vec::new();
            let mut deferred_results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluator.recurse_index_access(member, self.index_type);
                if result == TypeId::ERROR {
                    return Some(TypeId::ERROR);
                }
                if result == TypeId::UNDEFINED {
                    continue;
                }
                if crate::type_queries::is_index_access_type(self.evaluator.interner(), result) {
                    deferred_results.push(result);
                } else {
                    concrete_results.push(result);
                }
            }

            if !concrete_results.is_empty() {
                // Include deferred IndexAccess results so unresolvable
                // intersection members still constrain the result type.
                concrete_results.extend(deferred_results);
                return Some(crate::utils::intersection_or_single(
                    self.evaluator.interner(),
                    concrete_results,
                ));
            }

            let intersection_type = self.object_type;
            match collect_properties(
                intersection_type,
                self.evaluator.interner(),
                self.evaluator.resolver(),
            ) {
                PropertyCollectionResult::Properties {
                    properties,
                    string_index,
                    number_index,
                } => {
                    let merged = if string_index.is_some() || number_index.is_some() {
                        let shape = ObjectShape {
                            flags: crate::types::ObjectFlags::empty(),
                            properties,
                            string_index,
                            number_index,
                            symbol: None,
                        };
                        self.evaluator.interner().object_with_index(shape)
                    } else {
                        self.evaluator.interner().object(properties)
                    };
                    // Index access on merged object will defer (generic index),
                    // but the merged object has all properties accessible.
                    return Some(self.evaluator.recurse_index_access(merged, self.index_type));
                }
                PropertyCollectionResult::Any => return Some(TypeId::ANY),
                PropertyCollectionResult::NonObject => {
                    // Fall through to existing distribution logic
                }
            }
        }

        // For concrete indexes, distribute over intersection members and combine results.
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
        let result = self
            .evaluator
            .evaluate_tuple_index(&elements, self.index_type);

        // CRITICAL FIX: If we can't find the element, but the index is generic,
        // we must defer evaluation (return None) instead of returning UNDEFINED.
        // This prevents false TS2344 errors when a tuple is indexed by a type
        // parameter (e.g., `[-1, 0, 1, ...][Depth]` where `Depth extends number`).
        // Without this, the evaluator resolves the IndexAccess to `undefined`,
        // which then fails the constraint check against `number`.
        if result == TypeId::UNDEFINED && self.is_generic_index() {
            return None;
        }

        Some(result)
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
            .get_mapped(MappedTypeId(mapped_id));

        // Optimization: Mapped[K] -> Template[P/K] where K matches constraint
        // This handles cases like `Ev<K>["callback"]` where Ev<K> is a mapped type
        // over K, without needing to expand the mapped type (which fails for TypeParameter K).

        tracing::trace!(
            mapped_constraint = mapped.constraint.0,
            mapped_constraint_key = ?self.evaluator.interner().lookup(mapped.constraint),
            index_type = self.index_type.0,
            index_type_key = ?self.evaluator.interner().lookup(self.index_type),
            "visit_mapped index access"
        );

        // Only apply if no name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        // Same-name TypeParameter match: handle the case where the mapped constraint and
        // the index type are both TypeParameters with the same name but different TypeIds.
        //
        // This occurs with `T extends Record<K, number>, K extends string` where
        // `T[K]` should resolve to `number`. After Application expansion:
        // - `Record<K, number>` → `{ [P in K_inner]: number }` where K_inner (TypeId A)
        //   was created before K's `extends string` constraint was recorded.
        // - The function's K has a different TypeId (TypeId B) with the constraint.
        // - Both have the same Atom name (e.g., Atom("K")).
        //
        // By name-matching TypeParams we correctly identify that the index K is the
        // same parameter as the mapped constraint K, enabling substitution.
        let same_type_param_name = {
            let interner = self.evaluator.interner();
            match (
                interner.lookup(mapped.constraint),
                interner.lookup(self.index_type),
            ) {
                (
                    Some(TypeData::TypeParameter(constraint_tp)),
                    Some(TypeData::TypeParameter(index_tp)),
                ) => constraint_tp.name == index_tp.name,
                _ => false,
            }
        };

        // TypeParameter index whose constraint matches the mapped constraint:
        // When the index is `K extends "one" | "two"` and the mapped constraint is
        // `"one" | "two"`, K is a valid key into the mapped type. Substituting K into
        // the template preserves the generic relationship, e.g., `{ [P in "one" | "two"]: F<P> }[K]`
        // becomes `F<K>`. This matches tsc's behavior for indexed access on mapped types
        // with generic key types.
        let type_param_constraint_matches = {
            let raw_constraint = {
                let interner = self.evaluator.interner();
                if let Some(TypeData::TypeParameter(index_tp)) = interner.lookup(self.index_type) {
                    index_tp.constraint
                } else {
                    None
                }
            };
            if let Some(constraint) = raw_constraint {
                if constraint == mapped.constraint {
                    true
                } else {
                    // The constraint on the type parameter may be an unevaluated form
                    // (e.g., IndexAccess(Options, "kind")) that evaluates to the same
                    // type as the mapped constraint (e.g., "one" | "two"). Evaluate it
                    // before comparing to handle cases like:
                    //   type OptionHandlers = { [K in Options['kind']]: ... }
                    //   function handleOption<K extends Options['kind']>(...)
                    // where K's constraint is stored as Options['kind'] but the mapped
                    // constraint is the evaluated union "one" | "two".
                    let evaluated_constraint = self.evaluator.evaluate(constraint);
                    evaluated_constraint == mapped.constraint
                }
            } else {
                false
            }
        };

        // Direct match: index type exactly equals the constraint
        let can_substitute = mapped.constraint == self.index_type
            // Same-named TypeParameters with different TypeIds (see above)
            || same_type_param_name
            // Union/intersection constraints that directly include the index type
            || self.mapped_constraint_contains_index_type(mapped.constraint)
            // TypeParameter whose constraint matches the mapped constraint
            || type_param_constraint_matches
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
            // `{ [K in Keys]: F<K> }[Keys]` is a union over each key, not `F<Keys>`.
            // When the index is the whole symbolic key space (typically `keyof T`),
            // substituting `K := Keys` collapses per-key conditionals like
            // `{ [K in keyof T]: T[K] extends U ? K : never }[keyof T]` into
            // `T[keyof T] extends U ? keyof T : never`, which is unsound.
            // Preserve the per-key relationship by evaluating the template against a
            // constrained iteration variable instead of the whole key-space type.
            if self.index_is_symbolic_key_space(mapped.constraint) {
                return Some(self.instantiate_mapped_template_with_constraint_param(&mapped));
            }

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
}

impl<'a> ArrayKeyVisitor<'a> {
    fn new(db: &'a dyn TypeDatabase, element_type: TypeId) -> Self {
        Self { db, element_type }
    }

    /// Driver method that handles the fallback logic
    fn evaluate(&mut self, index_type: TypeId) -> TypeId {
        let result = self.visit_type(self.db, index_type);
        result.unwrap_or(self.element_type)
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
    /// Pre-evaluation check for mapped type + type parameter index access.
    ///
    /// When the object is a mapped type like `{ [P in C]: Template<P> }` and the
    /// index is a type parameter `K extends C`, substitute K into the template
    /// to produce `Template<K>`. This must happen before `evaluate(object_type)`
    /// because evaluation expands mapped types with concrete constraints into
    /// Object types, losing the template relationship.
    fn try_mapped_type_param_substitution(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        // Check if object is a mapped type
        let mapped_id = match self.interner().lookup(object_type) {
            Some(TypeData::Mapped(id)) => id,
            _ => return None,
        };

        // Check if index is a type parameter
        let index_constraint = match self.interner().lookup(index_type) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint?,
            _ => return None,
        };

        let mapped = self.interner().get_mapped(MappedTypeId(mapped_id.0));

        // Skip if there's a name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        // Check if the type parameter's constraint matches the mapped constraint.
        // The constraint may be stored in an unevaluated form (e.g., IndexAccess)
        // that evaluates to the same type as the mapped constraint.
        let constraint_matches = index_constraint == mapped.constraint || {
            let evaluated = self.evaluate(index_constraint);
            evaluated == mapped.constraint
        };

        if !constraint_matches {
            return None;
        }

        // Substitute K into the mapped template
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, index_type);

        let mut value_type =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        // Handle optional modifier
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            value_type = self.interner().union2(value_type, TypeId::UNDEFINED);
        }

        Some(value_type)
    }

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
        // Pre-evaluation check: if the object is a mapped type and the index is a type
        // parameter whose constraint matches the mapped constraint, substitute K into
        // the mapped template directly. This MUST happen before evaluate(object_type)
        // because evaluation expands mapped types with concrete constraints into Object
        // types, losing the template relationship. Without this, `MappedType[K]` where
        // K extends the mapped constraint would produce a deferred IndexAccess(Object, K)
        // that resolves to a union of concrete types instead of a single generic type.
        // Example: `{ [P in "one"|"two"]: (option: T & {kind:P}) => string }[K]`
        // should produce `(option: T & {kind:K}) => string`, not a union of functions.
        if let Some(mapped_result) =
            self.try_mapped_type_param_substitution(object_type, index_type)
        {
            return mapped_result;
        }

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

        // Error type propagation: if the object or index type is ERROR (e.g., from
        // a failed module import), return ERROR to suppress cascading diagnostics.
        // Without this, `Out[T]` where `Out` comes from a missing module would
        // produce false TS2322 errors instead of silently propagating the error.
        if evaluated_object == TypeId::ERROR || evaluated_index == TypeId::ERROR {
            return TypeId::ERROR;
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
            let name_str = self.interner().resolve_atom(name);
            let is_symbol_key = name_str.starts_with("__unique_");
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
            // Symbol-keyed properties must NOT fall through to string index
            // signatures — tsc treats symbol keys as distinct from string keys.
            if !is_symbol_key && let Some(string_index) = shape.string_index.as_ref() {
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

        // Template literal types (e.g., `foo${string}`), string intrinsic types
        // (e.g., Lowercase<T>), and intersections containing string (e.g., string & { brand: any })
        // are all subtypes of string. When the object has a string index signature,
        // these index types should resolve to the string index signature's value type,
        // just like TypeId::STRING does.
        if let Some(string_index) = shape.string_index.as_ref()
            && self.is_string_like_index(index_type)
        {
            return self.add_undefined_if_unchecked(string_index.value_type);
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
            let name_str = self.interner().resolve_atom(name);
            let is_symbol_key = name_str.starts_with("__unique_");
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
            // Symbol-keyed properties must NOT fall through to string index signatures
            if !is_symbol_key && let Some(string_index) = shape.string_index.as_ref() {
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

        // String-like index types (template literals, string intrinsics, branded strings)
        // should use the string index signature when available.
        if let Some(string_index) = shape.string_index.as_ref()
            && self.is_string_like_index(index_type)
        {
            return self.add_undefined_if_unchecked(string_index.value_type);
        }

        TypeId::UNDEFINED
    }

    /// Check if an index type is a subtype of string for index signature resolution.
    ///
    /// Template literal types, string intrinsic types (Lowercase, Uppercase, etc.),
    /// and intersections that contain string or a string literal are all subtypes
    /// of string. When used as an index on an object with a string index signature,
    /// they should resolve to the string index signature's value type.
    fn is_string_like_index(&self, index_type: TypeId) -> bool {
        match self.interner().lookup(index_type) {
            Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => true,
            Some(TypeData::Intersection(list_id)) => {
                // An intersection is string-like if any member is string or a string literal
                let members = self.interner().type_list(list_id);
                members.iter().any(|&m| {
                    m == TypeId::STRING
                        || matches!(
                            self.interner().lookup(m),
                            Some(
                                TypeData::Literal(LiteralValue::String(_))
                                    | TypeData::TemplateLiteral(_)
                                    | TypeData::StringIntrinsic { .. }
                            )
                        )
                })
            }
            _ => false,
        }
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

        // Under noUncheckedIndexedAccess, add `| undefined` only when the
        // accessed position is not guaranteed to exist.  Fixed tuple elements
        // that are within the minimum guaranteed length never need it.
        if self.no_unchecked_indexed_access() {
            // For literal numeric indices, check against the minimum guaranteed
            // length (count of required non-rest elements).
            let min_guaranteed = elements.iter().filter(|e| e.is_required()).count();
            if let Some(n) = literal_number(self.interner(), index_type)
                && (n.0 as usize) < min_guaranteed
            {
                // Position is guaranteed to exist — no undefined needed.
                return result;
            }

            // For non-literal indices (string, number, etc.), or indices
            // beyond the guaranteed range, add undefined.
            return self.add_undefined_if_unchecked(result);
        }

        result
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
