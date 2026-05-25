use super::*;

impl<'a, R: TypeResolver> CompatChecker<'a, R> {
    // Extracted from `compat.rs` to keep weak-type compatibility helpers under the file-size cap.

    /// Check if a type is a "weak type" in the tsc sense.
    /// A weak type is an object type with all optional properties, no call/construct
    /// signatures, and no index signatures. For intersections, ALL members must be
    /// weak types. Non-object types (primitives, etc.) are never weak.
    pub(super) fn is_weak_type(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        // Intersections are weak only if ALL members are weak
        if let Some(TypeData::Intersection(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return !members.is_empty() && members.iter().all(|m| self.is_weak_type(*m));
        }

        // Try to extract object shape
        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let Some(shape_id) = extractor.extract(type_id) else {
            return false;
        };
        let shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(shape_id));

        // Must have properties, all optional, no index signatures
        !shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.iter().all(|p| p.optional)
    }

    pub(super) fn violates_weak_type(&self, source: TypeId, target: TypeId) -> bool {
        // For weak intersections, tsc gathers properties from ALL members before
        // testing common-property overlap — a source that shares a name with any
        // member does not violate the rule.
        if let Some(TypeData::Intersection(list_id)) = self.interner.lookup(target) {
            let members = self.interner.type_list(list_id);
            // All members must be weak for the intersection to be considered weak.
            // e.g., `string & { opt?: number }` is NOT weak because `string` is not weak.
            if members.iter().any(|m| !self.is_weak_type(*m)) {
                return false;
            }
            // Collect properties from all members. The intersection is weak iff
            // source shares no property name with any member's property set.
            let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
            let mut all_target_props: Vec<crate::types::PropertyInfo> = Vec::new();
            for &member in members.iter() {
                let Some(shape_id) = extractor.extract(member) else {
                    continue;
                };
                let shape = self
                    .interner
                    .object_shape(crate::types::ObjectShapeId(shape_id));
                all_target_props.extend_from_slice(&shape.properties);
            }
            if all_target_props.is_empty() {
                return false;
            }
            return self.violates_weak_type_with_target_props(source, &all_target_props);
        }

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);

        let Some(target_shape_id) = extractor.extract(target) else {
            return false;
        };

        let target_shape = self
            .interner
            .object_shape(crate::types::ObjectShapeId(target_shape_id));

        // ObjectWithIndex with index signatures is not a weak type
        if let Some(TypeData::ObjectWithIndex(_)) = self.interner.lookup(target)
            && (target_shape.string_index.is_some() || target_shape.number_index.is_some())
        {
            return false;
        }

        let target_props = target_shape.properties.as_slice();
        if target_props.is_empty() || target_props.iter().any(|prop| !prop.optional) {
            return false;
        }

        // Target is a weak type (all optional properties). Check source.
        // Array/Tuple types are objects (not primitives) but the ShapeExtractor
        // can't extract their shape. In tsc, arrays have properties like `length`,
        // `push`, etc. that are checked against the weak type's properties.
        // When the source is an array/tuple type, check if the weak target has
        // any property that arrays also have. If not, it's a weak type violation.
        //
        // IMPORTANT: Only apply this when the target is a standalone weak type
        // (Object/ObjectWithIndex). Intersection targets are handled above with
        // the combined-property path.
        if self.is_array_or_tuple_type(source) {
            // Only trigger the array weak-type check when the target is a
            // standalone object shape, not another compound type.
            let target_is_standalone_object = matches!(
                self.interner.lookup(target),
                Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_))
            );
            if target_is_standalone_object {
                if self.target_has_array_like_property(target_props) {
                    return false; // Target accepts arrays
                }
                // Skip for empty arrays/tuples — they have no own properties
                // to check, but tsc allows assigning empty arrays to types
                // extending empty tuples (e.g., `interface X extends [] { p?: any }`)
                let source_is_empty = match self.interner.lookup(source) {
                    Some(TypeData::Tuple(tid)) => self.interner.tuple_list(tid).is_empty(),
                    Some(TypeData::Array(elem)) => elem == TypeId::NEVER,
                    _ => false,
                };
                if source_is_empty {
                    return false;
                }
                return true; // Non-empty array, target lacks array-like props → violation
            }
            // For other compound targets, skip the array check and fall through
            // to the standard weak type check.
        }

        self.violates_weak_type_with_target_props(source, target_props)
    }

    pub(super) fn violates_weak_union(&self, source: TypeId, target: TypeId) -> bool {
        // Intrinsics never resolve to Union; skip the dyn lookup.
        if target.is_intrinsic() {
            return false;
        }
        // Don't resolve the target - check it directly for union type
        // (resolve_weak_type_ref was converting unions to objects, which is wrong)
        let target_key = match self.interner.lookup(target) {
            Some(TypeData::Union(members)) => members,
            _ => {
                return false;
            }
        };

        let members = self.interner.type_list(target_key);
        if members.is_empty() {
            return false;
        }

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let mut has_weak_member = false;

        for member in members.iter() {
            let resolved_member = self.resolve_weak_type_ref(*member);
            // Weak-union checks only apply when ALL union members are object-like.
            // If any member is primitive/non-object (e.g. `string | Function`),
            // TypeScript does not apply TS2559-style weak-type rejection.
            let Some(member_shape_id) = extractor.extract(resolved_member) else {
                return false;
            };

            let member_shape = self
                .interner
                .object_shape(crate::types::ObjectShapeId(member_shape_id));

            if member_shape.properties.is_empty()
                || member_shape.string_index.is_some()
                || member_shape.number_index.is_some()
            {
                return false;
            }

            if member_shape.properties.iter().all(|prop| prop.optional) {
                has_weak_member = true;
            }
        }

        if !has_weak_member {
            return false;
        }

        self.source_lacks_union_common_property(source, members.as_ref())
    }

    /// Returns true when assignability fails because the source has no
    /// properties in common with a weak target — covering both union targets
    /// composed of weak members (`violates_weak_union`) and single weak
    /// object targets, including primitives assigned to weak objects
    /// (`violates_weak_type`). Drives the boundary's `weak_union_violation`
    /// flag that routes the checker between TS2559 and TS2322. The
    /// historical name is kept for boundary-contract stability.
    pub fn is_weak_union_violation(&self, source: TypeId, target: TypeId) -> bool {
        self.violates_weak_union(source, target) || self.violates_weak_type(source, target)
    }

    pub(super) fn violates_weak_type_with_target_props(
        &self,
        source: TypeId,
        target_props: &[PropertyInfo],
    ) -> bool {
        // Handle Union types explicitly before visitor
        if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|member| self.violates_weak_type_with_target_props(*member, target_props));
        }

        // The global Object type is exempt from weak type checks.
        // People treat Object as equivalent to {}, even though it declares
        // properties (constructor, toString, etc.). See TypeScript PR #16047.
        if self.is_global_object_interface_target(source) {
            return false;
        }

        let Some((source_props, has_index_signature)) = self.weak_type_source_properties(source)
        else {
            // No extractable object/function-like shape. For primitive types
            // (string/number/boolean/bigint/symbol literals, enum members, etc.),
            // tsc emits TS2559 because primitives have no user-defined properties
            // in common with weak types.
            //
            // We check if the target's weak type properties could overlap with
            // well-known primitive prototype properties (e.g., `length` for strings).
            // If no overlap, it's a weak type violation.
            return self.primitive_violates_weak_type(source, target_props);
        };

        if has_index_signature {
            return false;
        }

        // Empty objects are assignable to weak types (all optional properties).
        // Only trigger weak type violation if source has properties that don't overlap.
        !source_props.is_empty() && !self.has_common_property(source_props.as_slice(), target_props)
    }

    /// Check if a primitive source type violates a weak type target by having
    /// no properties in common.
    ///
    /// In tsc, primitives have apparent types (e.g., `string` has the `String`
    /// interface with `length`, `charAt`, etc.). When checking a primitive
    /// against a weak type, tsc checks if ANY of the weak type's properties
    /// exist on the primitive's apparent type.
    ///
    /// We approximate this by checking target properties against a set of
    /// well-known primitive prototype properties.
    pub(super) fn primitive_violates_weak_type(
        &self,
        source: TypeId,
        target_props: &[PropertyInfo],
    ) -> bool {
        // Determine if source is a primitive type. Boolean literal intrinsics
        // (`BOOLEAN_TRUE`/`BOOLEAN_FALSE`) are reserved TypeIds — distinct
        // from `BOOLEAN` — but they're still primitives for weak-type checks.
        let is_primitive = if source.is_intrinsic() {
            matches!(
                source,
                TypeId::STRING
                    | TypeId::NUMBER
                    | TypeId::BOOLEAN
                    | TypeId::BIGINT
                    | TypeId::SYMBOL
                    | TypeId::BOOLEAN_TRUE
                    | TypeId::BOOLEAN_FALSE
            )
        } else {
            matches!(
                self.interner.lookup(source),
                Some(TypeData::Literal(_) | TypeData::Enum(_, _) | TypeData::Intrinsic(_))
            )
        };

        if !is_primitive {
            // Not a primitive — fall back to the original behavior (no violation
            // because we can't determine the source's properties).
            return false;
        }

        // Determine which primitive category the source belongs to
        let is_string_like = source == TypeId::STRING
            || matches!(
                self.interner.lookup(source),
                Some(TypeData::Literal(LiteralValue::String(_)))
            )
            || self.is_string_enum_member(source);

        let is_number_like = source == TypeId::NUMBER
            || matches!(
                self.interner.lookup(source),
                Some(TypeData::Literal(LiteralValue::Number(_)))
            )
            || self.is_numeric_enum_member(source);

        // Check if any target property matches a well-known primitive property.
        // If so, the primitive's apparent type shares a property with the weak
        // target and this is NOT a violation.
        for prop in target_props {
            let name = self.interner.resolve_atom_ref(prop.name);
            // Properties common to all primitives (from Object.prototype)
            if matches!(
                name.as_ref(),
                "toString"
                    | "valueOf"
                    | "constructor"
                    | "toLocaleString"
                    | "hasOwnProperty"
                    | "isPrototypeOf"
                    | "propertyIsEnumerable"
            ) {
                return false;
            }
            // String-specific properties
            if is_string_like
                && matches!(
                    name.as_ref(),
                    "length"
                        | "charAt"
                        | "charCodeAt"
                        | "concat"
                        | "indexOf"
                        | "lastIndexOf"
                        | "match"
                        | "replace"
                        | "search"
                        | "slice"
                        | "split"
                        | "substring"
                        | "toLowerCase"
                        | "toUpperCase"
                        | "trim"
                        | "trimStart"
                        | "trimEnd"
                        | "padStart"
                        | "padEnd"
                        | "startsWith"
                        | "endsWith"
                        | "includes"
                        | "repeat"
                        | "normalize"
                        | "at"
                )
            {
                return false;
            }
            // Number-specific properties
            if is_number_like
                && matches!(name.as_ref(), "toFixed" | "toExponential" | "toPrecision")
            {
                return false;
            }
        }

        // None of the target's properties match the primitive's apparent type
        true
    }

    /// Check if a type is a string enum member.
    pub(super) fn is_string_enum_member(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(TypeData::Enum(_, member_type)) = self.interner.lookup(type_id) else {
            return false;
        };
        if member_type == TypeId::STRING {
            return true;
        }
        // Other intrinsics (BOOLEAN_TRUE/FALSE etc.) never resolve to
        // `Literal(String(_))` — skip the dyn lookup.
        if member_type.is_intrinsic() {
            return false;
        }
        matches!(
            self.interner.lookup(member_type),
            Some(TypeData::Literal(LiteralValue::String(_)))
        )
    }

    /// Check if a type is a numeric enum member.
    pub(super) fn is_numeric_enum_member(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(TypeData::Enum(_, member_type)) = self.interner.lookup(type_id) else {
            return false;
        };
        if member_type == TypeId::NUMBER {
            return true;
        }
        if member_type.is_intrinsic() {
            return false;
        }
        matches!(
            self.interner.lookup(member_type),
            Some(TypeData::Literal(LiteralValue::Number(_)))
        )
    }

    pub(super) fn source_lacks_union_common_property(
        &self,
        source: TypeId,
        target_members: &[TypeId],
    ) -> bool {
        let source = self.resolve_weak_type_ref(source);

        // Handle Union explicitly
        if let Some(TypeData::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|member| self.source_lacks_union_common_property(*member, target_members));
        }

        // The global Object type is exempt from weak type checks (same as violates_weak_type).
        if self.is_global_object_interface_target(source) {
            return false;
        }

        // Handle TypeParameter explicitly
        if let Some(TypeData::TypeParameter(param)) = self.interner.lookup(source) {
            return match param.constraint {
                Some(constraint) => {
                    self.source_lacks_union_common_property(constraint, target_members)
                }
                None => false,
            };
        }

        // Use visitor for Object types
        let Some((source_props, has_index_signature)) = self.weak_type_source_properties(source)
        else {
            // Array/Tuple types are objects but not extractable. They rarely
            // share property names with arbitrary union members, so treat as
            // lacking common properties (matching tsc's getPropertiesOfType
            // behavior for arrays in weak type detection).
            if self.is_array_or_tuple_type(source) {
                return true;
            }
            return false;
        };

        if has_index_signature {
            return false;
        }

        if source_props.is_empty() {
            return false;
        }

        let mut extractor = ShapeExtractor::new(self.interner, self.subtype.resolver);
        let mut has_common = false;
        for member in target_members {
            let resolved_member = self.resolve_weak_type_ref(*member);
            let member_shape_id = match extractor.extract(resolved_member) {
                Some(id) => id,
                None => continue,
            };

            let member_shape = self
                .interner
                .object_shape(crate::types::ObjectShapeId(member_shape_id));
            if member_shape.string_index.is_some() || member_shape.number_index.is_some() {
                return false;
            }
            if self.has_common_property(source_props.as_slice(), member_shape.properties.as_slice())
            {
                has_common = true;
                break;
            }
        }

        !has_common
    }

    pub(super) fn has_common_property(
        &self,
        source_props: &[PropertyInfo],
        target_props: &[PropertyInfo],
    ) -> bool {
        crate::utils::has_common_property_name(source_props, target_props)
    }

    /// Check if a type is an Array or Tuple type.
    /// These are object types but the `ShapeExtractor` can't extract their shape.
    pub(super) fn is_array_or_tuple_type(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Array(_)) | Some(TypeData::Tuple(_))
        )
    }

    /// Check if any property in the target weak type has a name commonly found
    /// on Array types (e.g. `length`). This prevents false weak-type violations
    /// for cases like `{ length?: number } | number[]`.
    pub(super) fn target_has_array_like_property(&self, target_props: &[PropertyInfo]) -> bool {
        // Known property names that exist on Array.prototype / Array instances.
        // We only need to check the most commonly used ones that could appear
        // as optional properties on weak types intended to accept arrays.
        target_props.iter().any(|prop| {
            let name = self.interner.resolve_atom(prop.name);
            matches!(
                name.as_str(),
                "length"
                    | "push"
                    | "pop"
                    | "shift"
                    | "unshift"
                    | "concat"
                    | "join"
                    | "reverse"
                    | "slice"
                    | "sort"
                    | "splice"
                    | "indexOf"
                    | "lastIndexOf"
                    | "every"
                    | "some"
                    | "forEach"
                    | "map"
                    | "filter"
                    | "reduce"
                    | "reduceRight"
                    | "find"
                    | "findIndex"
                    | "fill"
                    | "copyWithin"
                    | "entries"
                    | "keys"
                    | "values"
                    | "includes"
                    | "flatMap"
                    | "flat"
                    | "at"
                    | "toString"
                    | "toLocaleString"
            )
        })
    }

    pub(super) fn resolve_weak_type_ref(&self, type_id: TypeId) -> TypeId {
        self.subtype.resolve_lazy_type(type_id)
    }

    /// Check if a type is an empty object target.
    /// Uses the visitor pattern from `solver::visitor`.
    pub(super) fn is_empty_object_target(&self, target: TypeId) -> bool {
        is_empty_object_type_through_type_constraints(self.interner, target)
    }

    /// Match TSC's `isUnknownLikeUnionType`: a union that contains `{}`,
    /// `null`, AND `undefined` is semantically equivalent to `unknown`,
    /// because every other constituent is necessarily a subtype of `{}`
    /// (or otherwise absorbed by it). Extra union members do not disqualify
    /// the target — e.g. `{} | { x: string } | null | undefined` is still
    /// unknown-like.
    pub(super) fn empty_object_with_nullish_target(&self, target: TypeId) -> Option<(bool, bool)> {
        let TypeData::Union(members) = self.interner.lookup(target)? else {
            return None;
        };
        let members = self.interner.type_list(members);
        if members.len() < 3 {
            return None;
        }
        let mut saw_empty_object = false;
        let mut saw_null = false;
        let mut saw_undefined = false;
        for &member in members.iter() {
            if member == TypeId::NULL {
                saw_null = true;
            } else if member == TypeId::UNDEFINED {
                saw_undefined = true;
            } else if self.is_empty_object_target(member) {
                saw_empty_object = true;
            }
        }
        (saw_empty_object && saw_null && saw_undefined).then_some((saw_null, saw_undefined))
    }

    pub(super) fn is_assignable_to_empty_object_or_nullish(
        &self,
        source: TypeId,
        allow_null: bool,
        allow_undefined: bool,
    ) -> bool {
        if allow_null && allow_undefined {
            return true;
        }
        match source {
            TypeId::NULL => return allow_null,
            TypeId::UNDEFINED => return allow_undefined,
            _ => {}
        }
        self.is_assignable_to_empty_object(source)
    }

    pub(super) fn is_assignable_to_empty_object(&self, source: TypeId) -> bool {
        if source == TypeId::ANY || source == TypeId::NEVER {
            return true;
        }
        // Error types are assignable to everything (like `any` in tsc)
        if is_error_type(self.interner, source) {
            return true;
        }
        if !self.strict_null_checks && source.is_nullish() {
            return true;
        }
        if source == TypeId::UNKNOWN
            || source == TypeId::NULL
            || source == TypeId::UNDEFINED
            || source == TypeId::VOID
        {
            return false;
        }

        // Other intrinsics (STRING/NUMBER/BOOLEAN/.../BOOLEAN_TRUE/FALSE) are
        // assignable to `{}`. They never match Union/Intersection/IndexAccess/
        // TypeParameter — the existing match falls through to `_ => true`.
        if source.is_intrinsic() {
            return true;
        }

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::Union(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .all(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|member| self.is_assignable_to_empty_object(*member))
            }
            TypeData::IndexAccess(object_type, _) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, source);
                if evaluated != source {
                    return self.is_assignable_to_empty_object(evaluated);
                }

                !crate::type_queries::is_type_parameter_like(self.interner, object_type)
            }
            TypeData::TypeParameter(param) => match param.constraint {
                Some(constraint) => self.is_assignable_to_empty_object(constraint),
                None => false,
            },
            _ => true,
        }
    }
}
