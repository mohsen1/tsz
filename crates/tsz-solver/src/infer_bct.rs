//! Best Common Type (BCT) inference.
//!
//! Implements Rule #32: Best Common Type algorithm for determining the most
//! specific type that is a supertype of all candidates. Used by array literal
//! type inference, conditional expression type inference, etc.
//!
//! Algorithm:
//! 1. Filter out duplicates and never types
//! 2. Try to find a single candidate that is a supertype of all others
//! 3. Try to find a common base class (e.g., Dog + Cat -> Animal)
//! 4. If not found, create a union of all candidates

use crate::types::{
    CallSignature, CallableShape, FunctionShape, LiteralValue, ObjectShape, ObjectShapeId,
    ParamInfo, PropertyInfo, PropertyLookup, TupleElement, TypeData, TypeId,
};
use crate::utils;
use crate::visitor;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;

use super::InferenceContext;

struct TupleRestExpansion {
    /// Fixed elements before the variadic portion (prefix)
    fixed: Vec<TupleElement>,
    /// The variadic element type (e.g., T for ...T[])
    variadic: Option<TypeId>,
    /// Fixed elements after the variadic portion (suffix/tail)
    tail: Vec<TupleElement>,
}

impl<'a> InferenceContext<'a> {
    // =========================================================================
    // Best Common Type
    // =========================================================================

    /// Calculate the best common type from a set of types.
    /// This implements Rule #32: Best Common Type (BCT) Inference.
    ///
    /// Algorithm:
    /// 1. Filter out duplicates and never types
    /// 2. Try to find a single candidate that is a supertype of all others
    /// 3. Try to find a common base class (e.g., Dog + Cat -> Animal)
    /// 4. If not found, create a union of all candidates
    pub fn best_common_type(&self, types: &[TypeId]) -> TypeId {
        if types.is_empty() {
            return TypeId::UNKNOWN;
        }
        if types.len() == 1 {
            return types[0];
        }

        // HOMOGENEOUS FAST PATH: Zero-allocation check for arrays with identical types
        // This is the most common case for array literals like [1, 2, 3] or ["a", "b", "c"]
        let first = types[0];
        if types.iter().all(|&t| t == first) {
            return first;
        }

        // Filter out duplicates and special types
        let mut seen = FxHashSet::default();
        let mut unique: Vec<TypeId> = Vec::new();
        let mut has_any = false;
        for &ty in types {
            if ty == TypeId::ANY {
                has_any = true;
            }
            if ty == TypeId::NEVER {
                continue; // never doesn't contribute to union
            }
            if seen.insert(ty) {
                unique.push(ty);
            }
        }

        // Rule: If any type is 'any', the best common type is 'any'
        if has_any {
            return TypeId::ANY;
        }

        if unique.is_empty() {
            return TypeId::NEVER;
        }
        if unique.len() == 1 {
            return unique[0];
        }

        // Step 1: Try to find a common base type for primitives/literals
        // For example, [string, "hello"] -> string
        let common_base = self.find_common_base_type(&unique);
        if let Some(base) = common_base {
            // All types share a common base type (e.g. all are strings or derived from Animal).
            // Using the common base is more specific than a full union only when there's
            // more than one unique candidate.
            if unique.len() > 1 {
                return base;
            }
        }

        // Step 2: Tournament reduction — O(N) to find potential supertype candidate.
        // Instead of O(N²) pairwise comparison, we find the "winner" of a tournament
        // and then verify if it's truly a supertype of all in a second O(N) pass.
        let mut best = unique[0];
        for &candidate in &unique[1..] {
            if self.is_subtype(best, candidate) {
                best = candidate;
            }
        }
        if self.is_suitable_common_type(best, &unique) {
            return best;
        }

        // Step 3: Try to find a common base class for object types
        // This handles cases like [Dog, Cat] -> Animal (if both extend Animal)
        if let Some(common_class) = self.find_common_base_class(&unique) {
            return common_class;
        }

        // Step 4: Create union of all types
        self.interner.union(unique)
    }

    /// Find a common base type for a set of types.
    /// For example, [string, "hello"] -> Some(string)
    fn find_common_base_type(&self, types: &[TypeId]) -> Option<TypeId> {
        if types.is_empty() {
            return None;
        }

        // Get the base type of the first element
        let first_base = self.get_base_type(types[0])?;

        // Check if all other types have the same base
        for &ty in types.iter().skip(1) {
            let base = self.get_base_type(ty)?;
            if base != first_base {
                return None;
            }
        }

        Some(first_base)
    }

    /// Get the base type of a type.
    ///
    /// This handles both:
    /// 1. Literal widening: `"hello"` -> `string`, `42` -> `number`
    /// 2. Nominal hierarchy: `Dog` -> `Animal` (via resolver)
    pub(crate) fn get_base_type(&self, ty: TypeId) -> Option<TypeId> {
        match self.interner.lookup(ty) {
            // Literal widening: extract intrinsic type
            Some(TypeData::Literal(_)) => {
                match ty {
                    TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT => Some(ty),
                    _ => {
                        // For literal values, extract their base type
                        if let Some(TypeData::Literal(lit)) = self.interner.lookup(ty) {
                            match lit {
                                LiteralValue::String(_) => Some(TypeId::STRING),
                                LiteralValue::Number(_) => Some(TypeId::NUMBER),
                                LiteralValue::Boolean(_) => Some(TypeId::BOOLEAN),
                                LiteralValue::BigInt(_) => Some(TypeId::BIGINT),
                            }
                        } else {
                            Some(ty)
                        }
                    }
                }
            }
            // Nominal hierarchy: use resolver to get base class
            Some(TypeData::Lazy(_)) => {
                // For class/interface types, try to get base class from resolver
                if let Some(resolver) = self.resolver {
                    resolver.get_base_type(ty, self.interner)
                } else {
                    // No resolver available - return type as-is
                    Some(ty)
                }
            }
            _ => Some(ty),
        }
    }

    /// Find a common base class for object types.
    /// This implements the optimization for BCT where [Dog, Cat] -> Animal
    /// instead of Dog | Cat, if both Dog and Cat extend Animal.
    ///
    /// Returns None if no common base class exists or if types are not class types.
    fn find_common_base_class(&self, types: &[TypeId]) -> Option<TypeId> {
        if types.len() < 2 {
            return None;
        }

        // 1. Initialize candidates from the FIRST type only.
        // This is the only time we generate a full hierarchy.
        let mut base_candidates = self.get_class_hierarchy(types[0])?;

        // 2. For subsequent types, filter using is_subtype (cached and fast).
        // No allocations, no hierarchy traversal - just subtype checks.
        // This reduces complexity from O(N·Alloc(D)) to O(N·|Candidates|).
        for &ty in types.iter().skip(1) {
            // Optimization: If we run out of candidates, stop immediately.
            if base_candidates.is_empty() {
                return None;
            }

            // Filter: Keep base if 'ty' is a subtype of 'base'
            // This preserves semantic correctness while being much faster.
            base_candidates.retain(|&base| self.is_subtype(ty, base));
        }

        // Return the most specific base (first remaining candidate after filtering)
        base_candidates.first().copied()
    }

    /// Get the class hierarchy for a type, from most derived to most base.
    /// Returns None if the type is not a class/interface type.
    fn get_class_hierarchy(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        let mut hierarchy = Vec::new();
        self.collect_class_hierarchy(ty, &mut hierarchy);
        if hierarchy.is_empty() {
            None
        } else {
            Some(hierarchy)
        }
    }

    /// Recursively collect the class hierarchy for a type.
    fn collect_class_hierarchy(&self, ty: TypeId, hierarchy: &mut Vec<TypeId>) {
        // Prevent infinite recursion
        if hierarchy.contains(&ty) {
            return;
        }

        // Add current type to hierarchy
        hierarchy.push(ty);

        // Get the type key
        let Some(type_key) = self.interner.lookup(ty) else {
            return;
        };

        match type_key {
            // Intersection types: recurse into all members to extract commonality
            // This enables BCT to find common members from intersections
            // Example: [A & B, A & C] -> A (common member)
            TypeData::Intersection(members_id) => {
                let members = self.interner.type_list(members_id);
                for &member in members.iter() {
                    self.collect_class_hierarchy(member, hierarchy);
                }
            }
            // Lazy types: add the type itself, then follow extends chain
            // This enables BCT to work with classes/interfaces defined as Lazy(DefId)
            TypeData::Lazy(_) => {
                if let Some(base_type) = self.get_extends_clause(ty) {
                    self.collect_class_hierarchy(base_type, hierarchy);
                }
            }
            // For class/interface types, collect extends clauses
            TypeData::Callable(shape_id) => {
                let _shape = self.interner.callable_shape(shape_id);

                // Check for base class (extends clause)
                // In callable shapes, this is stored in the base_class property
                if let Some(base_type) = self.get_extends_clause(ty) {
                    self.collect_class_hierarchy(base_type, hierarchy);
                }
            }
            TypeData::Object(shape_id) => {
                let _shape = self.interner.object_shape(shape_id);

                // Check for base class (extends clause)
                if let Some(base_type) = self.get_extends_clause(ty) {
                    self.collect_class_hierarchy(base_type, hierarchy);
                }
            }
            _ => {
                // Not a class/interface type, no hierarchy
            }
        }
    }

    /// Get the extends clause (base class) for a class/interface type.
    ///
    /// This uses the `TypeResolver` to bridge to the Binder's extends clause information.
    /// For example, given Dog that extends Animal, this returns the Animal type.
    fn get_extends_clause(&self, ty: TypeId) -> Option<TypeId> {
        // If we have a resolver, use it to get the base type
        if let Some(resolver) = self.resolver {
            resolver.get_base_type(ty, self.interner)
        } else {
            // No resolver available - can't determine base class
            None
        }
    }

    /// Check if a candidate type is a suitable common type for all types.
    /// A suitable common type must be a supertype of all types in the list.
    fn is_suitable_common_type(&self, candidate: TypeId, types: &[TypeId]) -> bool {
        types.iter().all(|&ty| self.is_subtype(ty, candidate))
    }

    /// Simple subtype check for bounds validation.
    /// Uses a simplified check - for full checking, use `SubtypeChecker`.
    pub(crate) fn is_subtype(&self, source: TypeId, target: TypeId) -> bool {
        let key = (source, target);
        if let Some(&cached) = self.subtype_cache.borrow().get(&key) {
            return cached;
        }

        let result = self.is_subtype_uncached(source, target);
        self.subtype_cache.borrow_mut().insert(key, result);
        result
    }

    fn is_subtype_uncached(&self, source: TypeId, target: TypeId) -> bool {
        // Same type
        if source == target {
            return true;
        }

        // never <: T for all T
        if source == TypeId::NEVER {
            return true;
        }

        // T <: unknown for all T
        if target == TypeId::UNKNOWN {
            return true;
        }

        // any <: T and T <: any (only if both are any)
        if source == TypeId::ANY || target == TypeId::ANY {
            return source == target;
        }

        // STRICT_ANY matches itself or unknown/any (only at top level)
        if source == TypeId::STRICT_ANY || target == TypeId::STRICT_ANY {
            return source == target
                || target == TypeId::UNKNOWN
                || target == TypeId::ANY
                || source == TypeId::ANY;
        }

        // object keyword accepts any non-primitive type
        if target == TypeId::OBJECT {
            return self.is_object_keyword_type(source);
        }

        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        // OPTIMIZATION: Enum member disjointness fast-path
        // Two different enum members are guaranteed disjoint (neither is subtype of the other).
        // Since we already checked source == target at the top, reaching here means source != target.
        // This avoids O(n²) structural recursion in enumLiteralsSubtypeReduction.ts
        if let (Some(TypeData::Enum(..)), Some(TypeData::Enum(..))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            // Different enum members (or different enums) are always disjoint
            return false;
        }

        // Check if source is literal of target intrinsic
        if let Some(TypeData::Literal(lit)) = source_key.as_ref() {
            match (lit, target) {
                (LiteralValue::String(_), t) if t == TypeId::STRING => return true,
                (LiteralValue::Number(_), t) if t == TypeId::NUMBER => return true,
                (LiteralValue::Boolean(_), t) if t == TypeId::BOOLEAN => return true,
                (LiteralValue::BigInt(_), t) if t == TypeId::BIGINT => return true,
                _ => {}
            }
        }

        // Array and tuple structural checks
        if let (Some(TypeData::Array(s_elem)), Some(TypeData::Array(t_elem))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            return self.is_subtype(*s_elem, *t_elem);
        }

        if let (Some(TypeData::Tuple(_)), Some(TypeData::Tuple(_))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            // OPTIMIZATION: Unit-tuple disjointness fast-path
            // Two different unit tuples (tuples of literals/enums only) are guaranteed disjoint.
            // Since we already checked source == target at the top and returned true,
            // reaching here means source != target. If both are unit tuples, they're disjoint.
            // This avoids O(N) structural recursion for each comparison.
            if visitor::is_unit_type(self.interner, source)
                && visitor::is_unit_type(self.interner, target)
            {
                return false;
            }
            // Fall through to structural check for non-unit tuples
            let (Some(TypeData::Tuple(s_elems)), Some(TypeData::Tuple(t_elems))) =
                (source_key.as_ref(), target_key.as_ref())
            else {
                panic!("invariant violation: tuple subtype check expected tuple operands")
            };
            let s_elems = self.interner.tuple_list(*s_elems);
            let t_elems = self.interner.tuple_list(*t_elems);
            return self.tuple_subtype_of(&s_elems, &t_elems);
        }

        if let (Some(TypeData::Tuple(s_elems)), Some(TypeData::Array(t_elem))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_elems = self.interner.tuple_list(*s_elems);
            return self.tuple_subtype_array(&s_elems, *t_elem);
        }

        if let (Some(TypeData::Object(s_props)), Some(TypeData::Object(t_props))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_props);
            let t_shape = self.interner.object_shape(*t_props);
            return self.object_subtype_of(
                &s_shape.properties,
                Some(*s_props),
                &t_shape.properties,
            );
        }

        if let (
            Some(TypeData::ObjectWithIndex(s_shape_id)),
            Some(TypeData::ObjectWithIndex(t_shape_id)),
        ) = (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_shape_id);
            let t_shape = self.interner.object_shape(*t_shape_id);
            return self.object_with_index_subtype_of(&s_shape, Some(*s_shape_id), &t_shape);
        }

        if let (Some(TypeData::Object(s_props)), Some(TypeData::ObjectWithIndex(t_shape))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_props);
            let t_shape = self.interner.object_shape(*t_shape);
            return self.object_props_subtype_index(&s_shape.properties, Some(*s_props), &t_shape);
        }

        if let (Some(TypeData::ObjectWithIndex(s_shape_id)), Some(TypeData::Object(t_props))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_shape_id);
            let t_shape = self.interner.object_shape(*t_props);
            return self.object_subtype_of(
                &s_shape.properties,
                Some(*s_shape_id),
                &t_shape.properties,
            );
        }

        if let (Some(TypeData::Function(s_fn)), Some(TypeData::Function(t_fn))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_fn = self.interner.function_shape(*s_fn);
            let t_fn = self.interner.function_shape(*t_fn);
            return self.function_subtype_of(&s_fn, &t_fn);
        }

        if let (Some(TypeData::Callable(s_callable)), Some(TypeData::Callable(t_callable))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_callable = self.interner.callable_shape(*s_callable);
            let t_callable = self.interner.callable_shape(*t_callable);
            return self.callable_subtype_of(&s_callable, &t_callable);
        }

        if let (Some(TypeData::Function(s_fn)), Some(TypeData::Callable(t_callable))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_fn = self.interner.function_shape(*s_fn);
            let t_callable = self.interner.callable_shape(*t_callable);
            return self.function_subtype_callable(&s_fn, &t_callable);
        }

        if let (Some(TypeData::Callable(s_callable)), Some(TypeData::Function(t_fn))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_callable = self.interner.callable_shape(*s_callable);
            let t_fn = self.interner.function_shape(*t_fn);
            return self.callable_subtype_function(&s_callable, &t_fn);
        }

        if let (Some(TypeData::Application(s_app)), Some(TypeData::Application(t_app))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_app = self.interner.type_application(*s_app);
            let t_app = self.interner.type_application(*t_app);
            if s_app.args.len() != t_app.args.len() {
                return false;
            }
            if !self.is_subtype(s_app.base, t_app.base) {
                return false;
            }
            for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                if !self.is_subtype(*s_arg, *t_arg) {
                    return false;
                }
            }
            return true;
        }

        // Intersection: A & B <: T if either member is a subtype of T
        if let Some(TypeData::Intersection(members)) = source_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .any(|&member| self.is_subtype(member, target));
        }

        // Union: A | B <: T if both A <: T and B <: T
        if let Some(TypeData::Union(members)) = source_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .all(|&member| self.is_subtype(member, target));
        }

        // Target intersection: S <: (A & B) if S <: A and S <: B
        if let Some(TypeData::Intersection(members)) = target_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .all(|&member| self.is_subtype(source, member));
        }

        // Target union: S <: (A | B) if S <: A or S <: B
        if let Some(TypeData::Union(members)) = target_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .any(|&member| self.is_subtype(source, member));
        }

        // Object vs Object comparison
        if let (Some(TypeData::Object(s_props)), Some(TypeData::Object(t_props))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_props);
            let t_shape = self.interner.object_shape(*t_props);
            return self.object_subtype_of(
                &s_shape.properties,
                Some(*s_props),
                &t_shape.properties,
            );
        }

        false
    }

    fn is_object_keyword_type(&self, source: TypeId) -> bool {
        match source {
            TypeId::NEVER | TypeId::ERROR | TypeId::OBJECT => return true,
            TypeId::ANY => {
                // In BCT context, we want strict matching for ANY
                return false;
            }
            TypeId::UNKNOWN
            | TypeId::VOID
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::BOOLEAN
            | TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BIGINT
            | TypeId::SYMBOL => return false,
            _ => {}
        }

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Mapped(_)
            | TypeData::Application(_)
            | TypeData::ThisType => true,
            TypeData::ReadonlyType(inner) => self.is_subtype(inner, TypeId::OBJECT),
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_subtype(constraint, TypeId::OBJECT)),
            _ => false,
        }
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn optional_property_write_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.write_type, TypeId::UNDEFINED)
        } else {
            prop.write_type
        }
    }

    fn is_subtype_with_method_variance(
        &self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> bool {
        if !allow_bivariant {
            return self.is_subtype(source, target);
        }

        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        match (source_key.as_ref(), target_key.as_ref()) {
            (Some(TypeData::Function(s_fn)), Some(TypeData::Function(t_fn))) => {
                let s_fn = self.interner.function_shape(*s_fn);
                let t_fn = self.interner.function_shape(*t_fn);
                return self.function_like_subtype_of_with_variance(
                    &s_fn.params,
                    s_fn.return_type,
                    &t_fn.params,
                    t_fn.return_type,
                    true,
                );
            }
            (Some(TypeData::Callable(s_callable)), Some(TypeData::Callable(t_callable))) => {
                let s_callable = self.interner.callable_shape(*s_callable);
                let t_callable = self.interner.callable_shape(*t_callable);
                return self.callable_subtype_of_with_variance(&s_callable, &t_callable, true);
            }
            (Some(TypeData::Function(s_fn)), Some(TypeData::Callable(t_callable))) => {
                let s_fn = self.interner.function_shape(*s_fn);
                let t_callable = self.interner.callable_shape(*t_callable);
                return self.function_subtype_callable_with_variance(&s_fn, &t_callable, true);
            }
            (Some(TypeData::Callable(s_callable)), Some(TypeData::Function(t_fn))) => {
                let s_callable = self.interner.callable_shape(*s_callable);
                let t_fn = self.interner.function_shape(*t_fn);
                return self.callable_subtype_function_with_variance(&s_callable, &t_fn, true);
            }
            _ => {}
        }

        self.is_subtype(source, target)
    }

    fn lookup_property<'props>(
        &self,
        props: &'props [PropertyInfo],
        shape_id: Option<ObjectShapeId>,
        name: Atom,
    ) -> Option<&'props PropertyInfo> {
        if let Some(shape_id) = shape_id {
            match self.interner.object_property_index(shape_id, name) {
                PropertyLookup::Found(idx) => return props.get(idx),
                PropertyLookup::NotFound => return None,
                PropertyLookup::Uncached => {}
            }
        }
        props.iter().find(|p| p.name == name)
    }

    fn object_subtype_of(
        &self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target: &[PropertyInfo],
    ) -> bool {
        for t_prop in target {
            let s_prop = self.lookup_property(source, source_shape_id, t_prop.name);
            match s_prop {
                Some(sp) => {
                    if sp.optional && !t_prop.optional {
                        return false;
                    }
                    // NOTE: TypeScript allows readonly source to satisfy mutable target
                    // (readonly is a constraint on the reference, not structural compatibility)
                    let source_type = self.optional_property_type(sp);
                    let target_type = self.optional_property_type(t_prop);
                    if !self.is_subtype_with_method_variance(
                        source_type,
                        target_type,
                        t_prop.is_method,
                    ) {
                        return false;
                    }
                    // Check write type compatibility for mutable targets
                    // A readonly source cannot satisfy a mutable target (can't write to readonly)
                    if !t_prop.readonly {
                        // If source is readonly but target is mutable, this is a mismatch
                        if sp.readonly {
                            return false;
                        }
                        // If source is non-optional and target is optional, skip write type check
                        // Non-optional source can always satisfy optional target for writing
                        if !sp.optional && t_prop.optional {
                            // Skip write type check - non-optional source satisfies optional target
                        } else {
                            let source_write = self.optional_property_write_type(sp);
                            let target_write = self.optional_property_write_type(t_prop);
                            if !self.is_subtype_with_method_variance(
                                target_write,
                                source_write,
                                t_prop.is_method,
                            ) {
                                return false;
                            }
                        }
                    }
                }
                None => {
                    if !t_prop.optional {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn object_props_subtype_index(
        &self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target: &ObjectShape,
    ) -> bool {
        if !self.object_subtype_of(source, source_shape_id, &target.properties) {
            return false;
        }
        self.check_properties_against_index_signatures(source, target)
    }

    fn object_with_index_subtype_of(
        &self,
        source: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target: &ObjectShape,
    ) -> bool {
        if !self.object_subtype_of(&source.properties, source_shape_id, &target.properties) {
            return false;
        }

        if let Some(t_string_idx) = &target.string_index
            && let Some(s_string_idx) = &source.string_index
        {
            if s_string_idx.readonly && !t_string_idx.readonly {
                return false;
            }
            if !self.is_subtype(s_string_idx.value_type, t_string_idx.value_type) {
                return false;
            }
        }

        if let Some(t_number_idx) = &target.number_index
            && let Some(s_number_idx) = &source.number_index
        {
            if s_number_idx.readonly && !t_number_idx.readonly {
                return false;
            }
            if !self.is_subtype(s_number_idx.value_type, t_number_idx.value_type) {
                return false;
            }
        }

        if let (Some(s_string_idx), Some(s_number_idx)) =
            (&source.string_index, &source.number_index)
            && !self.is_subtype(s_number_idx.value_type, s_string_idx.value_type)
        {
            return false;
        }

        self.check_properties_against_index_signatures(&source.properties, target)
    }

    fn check_properties_against_index_signatures(
        &self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> bool {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return true;
        }

        for prop in source {
            let prop_type = self.optional_property_type(prop);

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                if !number_idx.readonly && prop.readonly {
                    return false;
                }
                if !self.is_subtype(prop_type, number_idx.value_type) {
                    return false;
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return false;
                }
                if !self.is_subtype(prop_type, string_idx.value_type) {
                    return false;
                }
            }
        }

        true
    }

    fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => elem,
            _ => type_id,
        }
    }

    fn are_parameters_compatible(&self, source: TypeId, target: TypeId, bivariant: bool) -> bool {
        if bivariant {
            self.is_subtype(target, source) || self.is_subtype(source, target)
        } else {
            self.is_subtype(target, source)
        }
    }

    fn are_this_parameters_compatible(
        &self,
        source: Option<TypeId>,
        target: Option<TypeId>,
        bivariant: bool,
    ) -> bool {
        if source.is_none() && target.is_none() {
            return true;
        }
        // If target has no explicit `this` parameter, always compatible.
        // TypeScript only checks `this` when the target declares one.
        if target.is_none() {
            return true;
        }
        let source = source.unwrap_or(TypeId::UNKNOWN);
        let target = target.unwrap();
        self.are_parameters_compatible(source, target, bivariant)
    }

    fn function_like_subtype_of(
        &self,
        source_params: &[ParamInfo],
        source_return: TypeId,
        target_params: &[ParamInfo],
        target_return: TypeId,
    ) -> bool {
        self.function_like_subtype_of_with_variance(
            source_params,
            source_return,
            target_params,
            target_return,
            false,
        )
    }

    fn function_like_subtype_of_with_variance(
        &self,
        source_params: &[ParamInfo],
        source_return: TypeId,
        target_params: &[ParamInfo],
        target_return: TypeId,
        bivariant: bool,
    ) -> bool {
        if !self.is_subtype(source_return, target_return) {
            return false;
        }

        let target_has_rest = target_params.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params.last().is_some_and(|p| p.rest);
        let target_fixed = if target_has_rest {
            target_params.len().saturating_sub(1)
        } else {
            target_params.len()
        };
        let source_fixed = if source_has_rest {
            source_params.len().saturating_sub(1)
        } else {
            source_params.len()
        };

        if !target_has_rest && source_params.len() > target_params.len() {
            return false;
        }

        let fixed_compare = std::cmp::min(source_fixed, target_fixed);
        for i in 0..fixed_compare {
            let s_param = &source_params[i];
            let t_param = &target_params[i];
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id, bivariant) {
                return false;
            }
        }

        if target_has_rest {
            let rest_param = match target_params.last() {
                Some(param) => param,
                None => return false,
            };
            let rest_elem = self.rest_element_type(rest_param.type_id);

            for s_param in source_params
                .iter()
                .skip(target_fixed)
                .take(source_fixed - target_fixed)
            {
                if !self.are_parameters_compatible(s_param.type_id, rest_elem, bivariant) {
                    return false;
                }
            }

            if source_has_rest {
                let s_rest = match source_params.last() {
                    Some(param) => param,
                    None => return false,
                };
                let s_rest_elem = self.rest_element_type(s_rest.type_id);
                if !self.are_parameters_compatible(s_rest_elem, rest_elem, bivariant) {
                    return false;
                }
            }
        }

        true
    }

    fn function_subtype_of(&self, source: &FunctionShape, target: &FunctionShape) -> bool {
        if source.is_constructor != target.is_constructor {
            return false;
        }
        if !self.are_this_parameters_compatible(source.this_type, target.this_type, false) {
            return false;
        }

        self.function_like_subtype_of(
            &source.params,
            source.return_type,
            &target.params,
            target.return_type,
        )
    }

    fn call_signature_subtype_of(
        &self,
        source: &CallSignature,
        target: &CallSignature,
        bivariant: bool,
    ) -> bool {
        if !self.are_this_parameters_compatible(source.this_type, target.this_type, bivariant) {
            return false;
        }
        self.function_like_subtype_of_with_variance(
            &source.params,
            source.return_type,
            &target.params,
            target.return_type,
            bivariant,
        )
    }

    fn callable_subtype_of(&self, source: &CallableShape, target: &CallableShape) -> bool {
        self.callable_subtype_of_with_variance(source, target, false)
    }

    fn callable_subtype_of_with_variance(
        &self,
        source: &CallableShape,
        target: &CallableShape,
        bivariant: bool,
    ) -> bool {
        for t_sig in &target.call_signatures {
            let mut found = false;
            for s_sig in &source.call_signatures {
                if self.call_signature_subtype_of(s_sig, t_sig, bivariant) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }

        for t_sig in &target.construct_signatures {
            let mut found = false;
            for s_sig in &source.construct_signatures {
                if self.call_signature_subtype_of(s_sig, t_sig, bivariant) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }

        self.object_subtype_of(&source.properties, None, &target.properties)
    }

    fn function_subtype_callable(&self, source: &FunctionShape, target: &CallableShape) -> bool {
        self.function_subtype_callable_with_variance(source, target, false)
    }

    fn function_subtype_callable_with_variance(
        &self,
        source: &FunctionShape,
        target: &CallableShape,
        bivariant: bool,
    ) -> bool {
        for t_sig in &target.call_signatures {
            if !self.function_like_subtype_of_with_variance(
                &source.params,
                source.return_type,
                &t_sig.params,
                t_sig.return_type,
                bivariant,
            ) {
                return false;
            }
        }
        true
    }

    fn callable_subtype_function(&self, source: &CallableShape, target: &FunctionShape) -> bool {
        self.callable_subtype_function_with_variance(source, target, false)
    }

    fn callable_subtype_function_with_variance(
        &self,
        source: &CallableShape,
        target: &FunctionShape,
        bivariant: bool,
    ) -> bool {
        for s_sig in &source.call_signatures {
            if self.function_like_subtype_of_with_variance(
                &s_sig.params,
                s_sig.return_type,
                &target.params,
                target.return_type,
                bivariant,
            ) {
                return true;
            }
        }
        false
    }

    fn tuple_subtype_array(&self, source: &[TupleElement], target_elem: TypeId) -> bool {
        for elem in source {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    if !self.is_subtype(fixed.type_id, target_elem) {
                        return false;
                    }
                }
                if let Some(variadic) = expansion.variadic
                    && !self.is_subtype(variadic, target_elem)
                {
                    return false;
                }
                // Check tail elements from nested tuple spreads
                for tail_elem in expansion.tail {
                    if !self.is_subtype(tail_elem.type_id, target_elem) {
                        return false;
                    }
                }
            } else if !self.is_subtype(elem.type_id, target_elem) {
                return false;
            }
        }
        true
    }

    fn tuple_subtype_of(&self, source: &[TupleElement], target: &[TupleElement]) -> bool {
        let source_required = source.iter().filter(|e| !e.optional && !e.rest).count();
        let target_required = target.iter().filter(|e| !e.optional && !e.rest).count();

        if source_required < target_required {
            return false;
        }

        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail
                let combined_suffix: Vec<_> = expansion
                    .tail
                    .iter()
                    .chain(outer_tail.iter())
                    .cloned()
                    .collect();

                // Match combined suffix from the end
                let mut source_end = source.len();
                for tail_elem in combined_suffix.iter().rev() {
                    if source_end <= i {
                        if !tail_elem.optional {
                            return false;
                        }
                        break;
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return false;
                        }
                        break;
                    }
                    if !self.is_subtype(s_elem.type_id, tail_elem.type_id) {
                        if tail_elem.optional {
                            break;
                        }
                        return false;
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some(s_elem) => {
                            if s_elem.rest {
                                return false;
                            }
                            if !self.is_subtype(s_elem.type_id, t_fixed.type_id) {
                                return false;
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return false;
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_array = self.interner.array(variadic);
                    for s_elem in source_iter {
                        if s_elem.rest {
                            if !self.is_subtype(s_elem.type_id, variadic_array) {
                                return false;
                            }
                        } else if !self.is_subtype(s_elem.type_id, variadic) {
                            return false;
                        }
                    }
                    return true;
                }

                if source_iter.next().is_some() {
                    return false;
                }
                return true;
            }

            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    return false;
                }
                if !self.is_subtype(s_elem.type_id, t_elem.type_id) {
                    return false;
                }
            } else if !t_elem.optional {
                return false;
            }
        }

        if source.len() > target.len() {
            return false;
        }

        if source.iter().any(|elem| elem.rest) {
            return false;
        }

        true
    }

    fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
                tail: Vec::new(),
            },
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let mut fixed = Vec::new();
                for (i, elem) in elements.iter().enumerate() {
                    if elem.rest {
                        let inner = self.expand_tuple_rest(elem.type_id);
                        fixed.extend(inner.fixed);
                        // Capture tail elements: inner.tail + elements after the rest
                        let mut tail = inner.tail;
                        tail.extend(elements[i + 1..].iter().cloned());
                        return TupleRestExpansion {
                            fixed,
                            variadic: inner.variadic,
                            tail,
                        };
                    }
                    fixed.push(elem.clone());
                }
                TupleRestExpansion {
                    fixed,
                    variadic: None,
                    tail: Vec::new(),
                }
            }
            _ => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(type_id),
                tail: Vec::new(),
            },
        }
    }
}
