use crate::construction::TypeDatabase;

use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_type, instantiate_type_preserving_meta_cached,
};

use crate::objects::PropertyCollectionResult;

use crate::relations::subtype::TypeResolver;

use crate::types::{
    CallableShape, CallableShapeId, IntrinsicKind, LiteralValue, MappedModifier, MappedType,
    MappedTypeId, ObjectShape, ObjectShapeId, PropertyInfo, SymbolRef, TupleElement, TupleListId,
    TypeData, TypeId, TypeListId, TypeParamInfo,
};

use crate::utils;

use crate::visitor::{
    TypeVisitor, intersection_list_id, keyof_inner_type, literal_number, type_param_info,
    union_list_id,
};

use super::super::evaluate::TypeEvaluator;

use super::string_index_helpers::string_index_signature_applies;

use crate::objects::apparent::literal_value_intrinsic_kind;

const MAX_UNION_INDEX_SIZE: usize = 500;

/// Threshold at which `O[T]` with a generic `T extends keyof O` index should
/// be left deferred instead of distributed into the per-key value-type union.
///
/// Below this many properties, the eager expansion is cheap and downstream
/// callers (property/method lookup, contextual typing, narrowing) rely on the
/// resolved value-type union to find members like `Array<O[T]>.push`. At or
/// above this many properties — `JSX.IntrinsicElements` from `react16.d.ts`
/// (~150 keys, each a complex generic `DetailedHTMLProps<...>` application) is
/// the canonical case — the expansion becomes quadratic in `|keyof O|`,
/// hitting tens of seconds on a single relation and ballooning the type
/// graph at every relation site. tsc keeps these accesses deferred; this
/// matches the pre-evaluation key-identity rejection that the upper layers
/// apply for the same shape, so downstream relation diagnostics see the
/// unevaluated `IndexAccess` and can emit the canonical TS2322 + TS5075
/// elaboration on it instead of comparing two identical value-type unions.
const LARGE_OBJECT_DEFERRAL_THRESHOLD: usize = 60;

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
        if let Some(TypeData::IndexAccess(template_obj, template_idx)) =
            self.evaluator.interner().lookup(mapped.template)
            && matches!(
                self.evaluator.interner().lookup(template_idx),
                Some(TypeData::TypeParameter(tp)) if tp.name == mapped.type_param.name
            )
        {
            let mut value_type = self
                .evaluator
                .interner()
                .index_access(template_obj, mapped.constraint);
            if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
                value_type = self
                    .evaluator
                    .interner()
                    .union2(value_type, TypeId::UNDEFINED);
            }
            return value_type;
        }

        let constrained_key = self.evaluator.interner().type_param(TypeParamInfo {
            name: mapped.type_param.name,
            constraint: Some(mapped.constraint),
            default: mapped.type_param.default,
            is_const: mapped.type_param.is_const,
        });

        let subst = TypeSubstitution::single(mapped.type_param.name, constrained_key);

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
        if self.index_type.is_intrinsic() {
            return false;
        }
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

    /// Check whether the index is a type parameter whose effective constraint
    /// is structurally `keyof <this_object>` — the same object whose property
    /// table the visitor is about to walk. When that holds, `O[T]` must stay
    /// deferred: distributing T's constraint over every key of O would expand
    /// `O[T]` to the full value-type union of O at every relation site, which
    /// is quadratic in `|keyof O|` for large interfaces (e.g. JSX.IntrinsicElements
    /// with ~150 keys mapped to generic Applications) and erases the per-call-site
    /// type-parameter identity that diagnostics like TS2322 + TS5075 require.
    fn index_is_type_param_constrained_by_keyof_of_this_object(&mut self) -> bool {
        let Some(info) = type_param_info(self.evaluator.interner(), self.index_type) else {
            return false;
        };
        let Some(constraint) = info.constraint else {
            return false;
        };
        self.constraint_is_keyof_of_object(constraint)
    }

    /// True iff `constraint` (possibly nested in an intersection) is structurally
    /// `keyof X` where `X` is the same as `self.object_type` (modulo evaluation).
    /// We accept either form: the raw `KeyOf(X)` TypeData or its evaluated form
    /// that still resolves back to `self.object_type` once we strip the `keyof`.
    fn constraint_is_keyof_of_object(&mut self, constraint: TypeId) -> bool {
        if let Some(list_id) = intersection_list_id(self.evaluator.interner(), constraint) {
            let members: Vec<_> = self
                .evaluator
                .interner()
                .type_list(list_id)
                .iter()
                .copied()
                .collect();
            return members
                .into_iter()
                .any(|member| self.constraint_is_keyof_of_object(member));
        }
        let inner = keyof_inner_type(self.evaluator.interner(), constraint).or_else(|| {
            let evaluated = self.evaluator.evaluate(constraint);
            (evaluated != constraint)
                .then(|| keyof_inner_type(self.evaluator.interner(), evaluated))
                .flatten()
        });
        let Some(inner) = inner else {
            return false;
        };
        self.evaluator
            .constraints_semantically_match(inner, self.object_type)
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
        if self.index_type.is_intrinsic() {
            return false;
        }
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
        if constraint.is_intrinsic() {
            return false;
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

    fn same_type_parameter_key(&mut self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }

        let (left_name, left_constraint, right_name, right_constraint) = {
            let interner = self.evaluator.interner();
            match (interner.lookup(left), interner.lookup(right)) {
                (
                    Some(TypeData::TypeParameter(left_info)),
                    Some(TypeData::TypeParameter(right_info)),
                ) => (
                    left_info.name,
                    left_info.constraint,
                    right_info.name,
                    right_info.constraint,
                ),
                _ => return false,
            }
        };

        if left_name != right_name {
            return false;
        }

        match (left_constraint, right_constraint) {
            (Some(left_constraint), Some(right_constraint)) => self
                .evaluator
                .constraints_semantically_match(left_constraint, right_constraint),
            (None, None) => true,
            _ => false,
        }
    }

    fn intersection_generic_key_part(&mut self, type_id: TypeId) -> Option<(TypeId, TypeId)> {
        let members = {
            let interner = self.evaluator.interner();
            let list_id = intersection_list_id(interner, type_id)?;
            interner.type_list(list_id)
        };

        let mut type_param = None;
        let mut key_parts = Vec::new();

        for &member in members.iter() {
            if matches!(
                self.evaluator.interner().lookup(member),
                Some(TypeData::TypeParameter(_))
            ) {
                if type_param
                    .is_some_and(|existing| !self.same_type_parameter_key(existing, member))
                {
                    return None;
                }
                type_param = Some(member);
            } else {
                if crate::type_queries::contains_type_parameters_db(
                    self.evaluator.interner(),
                    member,
                ) {
                    return None;
                }
                key_parts.push(member);
            }
        }

        let type_param = type_param?;
        let key_part = match key_parts.len() {
            0 => return None,
            1 => key_parts[0],
            _ => self.evaluator.interner().intersection(key_parts),
        };

        Some((type_param, key_part))
    }

    fn generic_index_covering_mapped_constraint(&mut self, constraint: TypeId) -> Option<TypeId> {
        let members = {
            let interner = self.evaluator.interner();
            let list_id = union_list_id(interner, self.index_type)?;
            interner.type_list(list_id)
        };

        let mut type_param = None;
        let mut covered_keys = Vec::with_capacity(members.len());

        for &member in members.iter() {
            let (member_type_param, key_part) = self.intersection_generic_key_part(member)?;
            if type_param
                .is_some_and(|existing| !self.same_type_parameter_key(existing, member_type_param))
            {
                return None;
            }
            type_param = Some(member_type_param);
            covered_keys.push(key_part);
        }

        let type_param = type_param?;
        let covered_key_space = match covered_keys.len() {
            0 => return None,
            1 => covered_keys[0],
            _ => self.evaluator.interner().union(covered_keys),
        };

        self.evaluator
            .constraints_semantically_match(covered_key_space, constraint)
            .then_some(type_param)
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
        if constraint.is_intrinsic() {
            return false;
        }
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
        // Intrinsics are never Object/ObjectWithIndex/Array/Tuple — skip lookup.
        if member.is_intrinsic() {
            return None;
        }
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
        self.evaluate_apparent_primitive(literal_value_intrinsic_kind(value))
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        let shape = self
            .evaluator
            .interner()
            .object_shape(ObjectShapeId(shape_id));

        // Defer `O[T]` when the index is a type parameter whose constraint is
        // `keyof O` (the object currently being indexed) AND distributing T's
        // constraint over every key of O would produce an unmanageable value-type
        // union — i.e. O has many properties (think `JSX.IntrinsicElements` with
        // ~150 keys mapped to complex generic Applications). tsc keeps `O[T]`
        // deferred for any generic key, but the cost of eager expansion is what
        // matters at scale: small Os can be expanded safely (and downstream code
        // still relies on the eager value-type union for property/method lookup
        // on Array-of-O[T] etc.). The threshold is the smallest property count
        // above which the quadratic expansion becomes noticeable in CI.
        if shape.properties.len() >= LARGE_OBJECT_DEFERRAL_THRESHOLD
            && self.index_is_type_param_constrained_by_keyof_of_this_object()
        {
            return None;
        }

        let result = self
            .evaluator
            .evaluate_object_index_from_constraint(&shape.properties, self.index_type)
            .unwrap_or_else(|| {
                self.evaluator
                    .evaluate_object_index(&shape.properties, self.index_type)
            });

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

        if shape.properties.len() >= LARGE_OBJECT_DEFERRAL_THRESHOLD
            && self.index_is_type_param_constrained_by_keyof_of_this_object()
        {
            return None;
        }

        let result = self
            .evaluator
            .evaluate_object_with_index_from_constraint(&shape, self.index_type)
            .unwrap_or_else(|| {
                self.evaluator
                    .evaluate_object_with_index(&shape, self.index_type)
            });

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
                    // Check if the member is a type parameter without a meaningful constraint.
                    // If so, create a deferred IndexAccess to preserve the constraint.
                    if let Some(TypeData::TypeParameter(param_info)) =
                        self.evaluator.interner().lookup(member)
                    {
                        let has_meaningful_constraint = param_info
                            .constraint
                            .is_some_and(|c| c != TypeId::UNKNOWN && c != TypeId::ANY);
                        if !has_meaningful_constraint {
                            let deferred = self
                                .evaluator
                                .interner()
                                .index_access(member, self.index_type);
                            deferred_results.push(deferred);
                        }
                    }
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

            // This handles cases like `(S & State<T>)["a"]` where S is a type parameter
            // without a meaningful constraint - we need to preserve S["a"] as a deferred
            if !deferred_results.is_empty() {
                return Some(crate::utils::intersection_or_single(
                    self.evaluator.interner(),
                    deferred_results,
                ));
            }

            let intersection_type = self.object_type;
            match crate::objects::collect_properties_cached(
                intersection_type,
                self.evaluator.interner(),
                self.evaluator.resolver(),
                self.evaluator.query_db(),
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
                    return Some(self.evaluator.recurse_index_access(merged, self.index_type));
                }
                PropertyCollectionResult::Any => return Some(TypeId::ANY),
                PropertyCollectionResult::NonObject => {
                    // Fall through to existing distribution logic
                }
            }
        }

        // For concrete indexes, distribute over intersection members and intersect results.
        // Members that don't have the property (returning UNDEFINED) are excluded.
        //
        // CRITICAL: Deferred IndexAccess types (from type parameters without constraints)
        // must be preserved even if the property access returns UNDEFINED. For example,
        // (S & State<T>)["a"] where S is unconstrained should produce S["a"] & (T | undefined),
        // not just T | undefined. The deferred S["a"] provides a constraint that must be
        // checked for correct assignability.
        //
        // Both concrete and deferred IndexAccess results are included in the intersection.
        // Deferred types (e.g., S["a"] where S is an unconstrained type parameter) represent
        // unknown constraints that must be preserved for correct assignability checking.
        // For example, (S & State<T>)["a"] must produce S["a"] & (T | undefined), not
        // just T | undefined — otherwise T would incorrectly be assignable to the result.
        let members = self.evaluator.interner().type_list(TypeListId(list_id));
        let mut results = Vec::new();
        for &member in members.iter() {
            let result = self.evaluator.recurse_index_access(member, self.index_type);
            if result == TypeId::ERROR {
                return Some(TypeId::ERROR);
            }
            if result == TypeId::UNDEFINED {
                // Check if the member is a type parameter without a meaningful constraint.
                // A constraint is "meaningful" if it provides actual structural information
                // beyond just `unknown` or `any`. TypeScript 6.0+ gives unconstrained type
                // parameters an implicit constraint of `unknown`, but for indexed access
                // purposes, we should still treat them as deferred to preserve assignability
                // constraints like `(S & State<T>)["a"] = S["a"] & (T | undefined)`.
                if let Some(TypeData::TypeParameter(param_info)) =
                    self.evaluator.interner().lookup(member)
                {
                    let has_meaningful_constraint = param_info
                        .constraint
                        .is_some_and(|c| c != TypeId::UNKNOWN && c != TypeId::ANY);
                    if !has_meaningful_constraint {
                        let deferred = self
                            .evaluator
                            .interner()
                            .index_access(member, self.index_type);
                        results.push(deferred);
                    }
                }
                continue;
            }
            results.push(result);
        }
        if results.is_empty() {
            Some(TypeId::UNDEFINED)
        } else {
            Some(crate::utils::intersection_or_single(
                self.evaluator.interner(),
                results,
            ))
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

        // Generic tuple indexes defer instead of becoming `undefined`, avoiding
        // false constraint errors for patterns like `Tuple[Depth]`.
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

    fn visit_this_type(&mut self) -> Self::Output {
        let concrete_this = self
            .evaluator
            .resolver()
            .resolve_this_type(self.evaluator.interner())?;
        if concrete_this == self.object_type {
            return Some(
                self.evaluator
                    .interner()
                    .index_access(self.object_type, self.index_type),
            );
        }
        Some(
            self.evaluator
                .recurse_index_access(concrete_this, self.index_type),
        )
    }

    fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
        Some(
            self.evaluator
                .recurse_index_access(inner_type, self.index_type),
        )
    }

    fn visit_enum(&mut self, def_id: u32, _member_type: TypeId) -> Self::Output {
        let ns_type = self
            .evaluator
            .resolver()
            .get_enum_namespace_type(crate::def::DefId(def_id))?;
        let result = self
            .evaluator
            .recurse_index_access(ns_type, self.index_type);
        if result == TypeId::UNDEFINED && self.is_generic_index() {
            return None;
        }
        Some(result)
    }

    fn visit_mapped(&mut self, mapped_id: u32) -> Self::Output {
        let mapped = self
            .evaluator
            .interner()
            .get_mapped(MappedTypeId(mapped_id));

        if mapped.name_type.is_some() {
            if let Some(result) =
                super::mapped_template_index::try_evaluate_remapped_mapped_template_for_index(
                    self.evaluator,
                    &mapped,
                    self.index_type,
                )
            {
                return Some(result);
            }
            return None;
        }

        // Name-match TypeParams so expanded `Record<K, V>` constraints still
        // substitute for the caller's distinct-but-same-name `K`.
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
                    self.evaluator
                        .constraints_semantically_match(constraint, mapped.constraint)
                }
            } else {
                false
            }
        };
        let generic_covering_index =
            self.generic_index_covering_mapped_constraint(mapped.constraint);

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
            // This handles for-in loops: `for (let k in obj) { result[k] = ... }`.
            || (matches!(self.index_type, TypeId::STRING | TypeId::NUMBER)
                && keyof_inner_type(self.evaluator.interner(), mapped.constraint).is_some())
            // Intersection index containing the constraint: when index is
            // `string & keyof T` and constraint is `keyof T`, the intersection
            // is a subset of the constraint. This handles for-in loops where the
            || self.intersection_contains_mapped_constraint(mapped.constraint)
            // Union of `(K & key)` members covering the mapped constraint preserves
            // the original generic key. This occurs when reading a discriminant
            // property from `Union & { kind: K }`.
            || generic_covering_index.is_some()
            || self
                .evaluator
                .constraints_semantically_match(self.index_type, mapped.constraint);

        if can_substitute {
            // `{ [K in Keys]: F<K> }[Keys]` is a per-key union, not `F<Keys>`.
            // Preserve that relationship for symbolic key-space indexes.
            if self.index_is_symbolic_key_space(mapped.constraint) {
                if let Some(per_key_result) =
                    super::mapped_template_index::try_evaluate_mapped_template_per_concrete_key(
                        self.evaluator,
                        &mapped,
                    )
                {
                    return Some(per_key_result);
                }
                return Some(self.instantiate_mapped_template_with_constraint_param(&mapped));
            }

            let substitution_index = generic_covering_index.unwrap_or(self.index_type);
            let subst = TypeSubstitution::single(mapped.type_param.name, substitution_index);

            let value_type = self.evaluator.evaluate(instantiate_type(
                self.evaluator.interner(),
                mapped.template,
                &subst,
            ));

            return Some(self.evaluator.apply_mapped_optional_read_semantics(
                self.object_type,
                &mapped,
                substitution_index,
                value_type,
            ));
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
