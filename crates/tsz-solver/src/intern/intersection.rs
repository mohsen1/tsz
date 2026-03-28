//! Intersection type normalization and merging.

use super::*;

impl TypeInterner {
    fn object_property_names(&self, type_id: TypeId) -> smallvec::SmallVec<[Atom; 4]> {
        let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
            self.lookup(type_id)
        else {
            return smallvec::SmallVec::new();
        };
        let shape = self.object_shape(shape_id);
        shape.properties.iter().map(|prop| prop.name).collect()
    }

    fn union_has_matching_object_discriminant(
        &self,
        union_type: TypeId,
        candidate_names: &[Atom],
    ) -> bool {
        let Some(TypeData::Union(list_id)) = self.lookup(union_type) else {
            return false;
        };
        let members = self.type_list(list_id);
        if members.len() < 2 {
            return false;
        }

        candidate_names.iter().copied().any(|name| {
            let mut seen = smallvec::SmallVec::<[TypeId; 4]>::new();
            for &member in members.iter() {
                let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                    self.lookup(member)
                else {
                    return false;
                };
                let shape = self.object_shape(shape_id);
                let Some(prop) = shape.properties.iter().find(|prop| prop.name == name) else {
                    return false;
                };
                if !crate::type_queries::is_unit_type(self, prop.type_id) {
                    return false;
                }
                if !seen.contains(&prop.type_id) {
                    seen.push(prop.type_id);
                }
            }
            seen.len() > 1
        })
    }

    fn should_preserve_discriminated_object_intersection(&self, flat: &TypeListBuffer) -> bool {
        let candidate_names: smallvec::SmallVec<[Atom; 8]> = flat
            .iter()
            .filter(|&&id| !matches!(self.lookup(id), Some(TypeData::Union(_))))
            .flat_map(|&id| self.object_property_names(id))
            .collect();
        if candidate_names.is_empty() {
            return false;
        }

        flat.iter().copied().any(|member| {
            self.union_has_matching_object_discriminant(member, candidate_names.as_slice())
        })
    }

    /// Check if a type is an empty object type (no properties, no index signatures).
    ///
    /// Empty objects like `{}` represent "any non-nullish value" in TypeScript.
    /// In intersections like `string & {}`, the empty object is redundant and can be removed.
    fn is_empty_object(&self, id: TypeId) -> bool {
        match self.lookup(id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            _ => false,
        }
    }

    /// Check if a type is non-nullish (i.e., not null, undefined, void, or never).
    ///
    /// This is used to determine if an intersection has non-nullish members that
    /// make empty objects redundant.
    ///
    /// For unions: returns true only if ALL members are non-nullish (conservative).
    /// For intersections: returns true if ANY member is non-nullish (permissive).
    fn is_non_nullish_type(&self, id: TypeId) -> bool {
        match id {
            TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID | TypeId::NEVER => false,
            TypeId::STRING
            | TypeId::NUMBER
            | TypeId::BOOLEAN
            | TypeId::BIGINT
            | TypeId::SYMBOL
            | TypeId::OBJECT => true,
            _ => match self.lookup(id) {
                Some(
                    TypeData::Literal(_)
                    | TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::TemplateLiteral(_)
                    | TypeData::UniqueSymbol(_),
                ) => true,

                // Union is non-nullish only if ALL members are non-nullish
                // (conservative: don't remove {} if any member might be nullish)
                Some(TypeData::Union(list_id)) => {
                    let members = self.type_list(list_id);
                    members.iter().all(|&m| self.is_non_nullish_type(m))
                }

                // Intersection is non-nullish if ANY member is non-nullish
                // (permissive: string & T is non-nullish regardless of T)
                Some(TypeData::Intersection(list_id)) => {
                    let members = self.type_list(list_id);
                    members.iter().any(|&m| self.is_non_nullish_type(m))
                }

                _ => false,
            },
        }
    }

    pub(super) fn normalize_intersection(&self, mut flat: TypeListBuffer) -> TypeId {
        // Single-pass scan for special sentinel types instead of multiple contains() calls.
        let mut has_error = false;
        let mut has_never = false;
        let mut has_any = false;
        let mut has_unknown = false;
        for &id in flat.iter() {
            if id == TypeId::ERROR {
                has_error = true;
                break;
            }
            if id == TypeId::NEVER {
                has_never = true;
            } else if id == TypeId::ANY {
                has_any = true;
            } else if id == TypeId::UNKNOWN {
                has_unknown = true;
            }
        }
        if has_error {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        if has_never {
            return TypeId::NEVER;
        }
        if has_any {
            return TypeId::ANY;
        }
        // Remove `unknown` from intersections (identity element), only if present
        if has_unknown {
            flat.retain(|id| *id != TypeId::UNKNOWN);
        }

        // =========================================================
        // Task #48: Empty Object Rule for Intersections
        // =========================================================
        // Remove {} from intersections if other non-nullish types are present.
        // In TypeScript, {} represents "any non-nullish value", which is redundant
        // when we already have a non-nullish type like string, number, etc.
        // Example: `string & {}` → `string` (correct)
        // Note: This is INTERSECTION-SPECIFIC. For unions, we DO NOT remove {}
        //       because `string | {}` should stay as `string | {}` (weak type rule).
        if flat.len() > 1 && flat.iter().any(|&id| self.is_empty_object(id)) {
            let has_non_nullish = flat
                .iter()
                .any(|&id| !self.is_empty_object(id) && self.is_non_nullish_type(id));

            if has_non_nullish {
                flat.retain(|id| !self.is_empty_object(*id));
            }
        }

        // Preserve source/declaration order of intersection members to match tsc.
        // tsc does not sort intersection members — it preserves the order from the
        // source declaration. We only perform order-preserving dedup (remove exact
        // duplicates while keeping the first occurrence).
        {
            let mut seen = FxHashSet::default();
            flat.retain(|id| seen.insert(*id));
        }

        // Re-check length after dedup
        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Some intersections with Lazy enum-member refs are still safe to
        // reduce immediately: incompatible unit values should collapse to never
        // before the unresolved-type bailout below.
        if self.intersection_has_disjoint_unit_values(&flat) {
            return TypeId::NEVER;
        }

        // Abort reduction if any member is a meta-type the interner cannot safely
        // reason about structurally on its own.
        //
        // Lazy/Application/Mapped members may later evaluate to object types with
        // index signatures or other structure that is invisible here. If we merge
        // concrete object members around them too early, we can collapse
        // intersections like:
        //   { [K in keyof Source]: string } & Pick<Tgt, Exclude<keyof Tgt, keyof Source>>
        // into an object that incorrectly loses Tgt's string index signature after
        // generic instantiation (`Source = {}`).
        //
        // Preserve these intersections as-is so the checker's resolver/evaluator can
        // expand them with full symbol information later.
        let has_unresolved = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::Lazy(_) | TypeData::Application(_) | TypeData::Mapped(_))
            )
        });
        if has_unresolved {
            let list_id = self.intern_type_list_from_slice(&flat);
            return self.intern(TypeData::Intersection(list_id));
        }

        // NOTE: narrow_literal_primitive_intersection was removed (Task #43) because it was too aggressive.
        // It caused incorrect behavior in mixed intersections like "a" & string & { x: 1 }.
        // The reduce_intersection_subtypes() at the end correctly handles literal/primitive narrowing
        // via is_subtype_shallow checks without losing other intersection members.

        if self.intersection_has_disjoint_primitives(&flat) {
            return TypeId::NEVER;
        }
        if self.intersection_has_conflicting_private_brands(&flat) {
            return TypeId::NEVER;
        }
        if self.intersection_has_disjoint_object_literals(&flat) {
            return TypeId::NEVER;
        }
        // Check if null/undefined intersects with any object type
        // null & object = never, undefined & object = never
        // Note: This is different from branded types like string & { __brand: T }
        // which are valid, but null/undefined are ALWAYS disjoint from object types
        if self.intersection_has_null_undefined_with_object(&flat) {
            return TypeId::NEVER;
        }
        // Check if the `object` intrinsic (non-primitive type) is intersected with a primitive.
        // `object & string = never`, `object & number = never`, etc.
        // Branded types like `string & { __brand: T }` use structural objects, not `object`.
        if self.intersection_has_object_intrinsic_with_primitive(&flat) {
            return TypeId::NEVER;
        }
        // Check if a TypeParameter with a non-nullable constraint is intersected with
        // null/undefined/void. For example, `T & undefined` where `T extends string`
        // is `never` because `string` and `undefined` are disjoint.
        // This handles the common pattern from type predicate narrowing where
        // property types like `(T | undefined) & T` distribute to `T | (T & undefined)`,
        // and the `T & undefined` branch must reduce to `never`.
        if self.intersection_has_type_param_disjoint_with_nullish(&flat) {
            return TypeId::NEVER;
        }

        // Merge same-named type parameters before distribution.
        // When Application evaluation produces unconstrained copies of type parameters
        // (e.g., `DatafulFoo<T>` produces `T` without constraint from the interface,
        // while the class has `T extends string`), the intersection may contain both.
        // Replace unconstrained type parameters with their constrained counterparts
        // (same name) so distribution can properly simplify (e.g., `undefined & T` → never).
        // Also check inside union members for constrained type parameters.
        self.merge_same_name_type_params(&mut flat);

        // Distributivity: A & (B | C) → (A & B) | (A & C)
        // Only distribute when there are non-union members to intersect with each
        // union alternative. When ALL members are unions (e.g., `(A|B) & (C|D)`),
        // TSC preserves the intersection form rather than distributing into
        // `(A&C) | (A&D) | (B&C) | (B&D)`.
        let has_non_union = flat
            .iter()
            .any(|&id| !matches!(self.lookup(id), Some(TypeData::Union(_))));
        if has_non_union {
            if self.should_preserve_discriminated_object_intersection(&flat) {
                let list_id = self.intern_type_list_from_slice(&flat);
                return self.intern(TypeData::Intersection(list_id));
            }
            if let Some(distributed) = self.distribute_intersection_over_unions(&flat) {
                return distributed;
            }
        }

        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // =========================================================
        // Task #43: Partial Merging Strategy
        // =========================================================
        // Instead of all-or-nothing merging, extract objects and callables
        // from mixed intersections, merge them separately, then combine.
        //
        // Example: { a: string } & { b: number } & ((x: number) => void)
        // → Merge objects: { a: string; b: number }
        // → Merge callables: (x: number) => void
        // → Result: Callable with properties (merging both)

        // Step 1: Extract and merge objects from mixed intersection
        let (merged_object, remaining_after_objects) = self.extract_and_merge_objects(&flat);

        // Step 2: Extract and merge callables from remaining members
        let (merged_callable, remaining_after_callables) =
            self.extract_and_merge_callables(&remaining_after_objects);

        // Step 3: Rebuild flat with merged results, preserving declaration order.
        // tsc preserves the original order of intersection members.
        let mut final_flat: TypeListBuffer = SmallVec::new();

        // Add remaining non-object, non-callable members
        final_flat.extend(remaining_after_callables.iter().copied());

        // Add merged object if present
        if let Some(obj_id) = merged_object {
            final_flat.push(obj_id);
        }

        // Add merged callable if present
        if let Some(call_id) = merged_callable {
            final_flat.push(call_id);
        }

        // Early exit if simplified to single type
        if final_flat.len() == 1 {
            return final_flat[0];
        }

        // Update flat reference for subsequent checks
        flat = final_flat;

        // Reduce intersection using subtype checks (e.g., {a: 1} & {a: 1 | number} => {a: 1})
        // Skip reduction if intersection contains complex types (TypeParameters, Lazy, etc.)
        let has_complex = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::TypeParameter(_) | TypeData::Lazy(_))
            )
        });
        if !has_complex {
            self.reduce_intersection_subtypes(&mut flat);
        }

        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Intersection(list_id))
    }

    fn try_merge_callables_in_intersection(&self, members: &[TypeId]) -> Option<TypeId> {
        let mut call_signatures: Vec<CallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;

        // Collect all call signatures and properties
        for &member in members {
            match self.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let func = self.function_shape(func_id);
                    call_signatures.push(CallSignature {
                        type_params: func.type_params.clone(),
                        params: func.params.clone(),
                        this_type: func.this_type,
                        return_type: func.return_type,
                        type_predicate: func.type_predicate,
                        is_method: func.is_method,
                    });
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.callable_shape(callable_id);
                    // Add all call signatures
                    for sig in &callable.call_signatures {
                        call_signatures.push(sig.clone());
                    }
                    // Merge properties
                    for prop in &callable.properties {
                        if let Some(existing) = properties.iter_mut().find(|p| p.name == prop.name)
                        {
                            // Intersect property types using raw intersection to avoid infinite recursion
                            existing.type_id =
                                self.intersect_types_raw2(existing.type_id, prop.type_id);
                            existing.optional = existing.optional && prop.optional;
                            // Intersection: readonly only if ALL are readonly (writable wins)
                            existing.readonly = existing.readonly && prop.readonly;
                            // Write type: if writable, use read type to avoid NONE sentinels
                            if !existing.readonly {
                                existing.write_type = existing.type_id;
                            } else {
                                existing.write_type =
                                    self.intersect_types_raw2(existing.write_type, prop.write_type);
                            }
                        } else {
                            properties.push(prop.clone());
                        }
                    }
                    // Merge index signatures
                    match (&callable.string_index, &string_index) {
                        (Some(idx), None) => string_index = Some(*idx),
                        (Some(idx), Some(existing)) => {
                            string_index = Some(IndexSignature {
                                key_type: existing.key_type,
                                value_type: self
                                    .intersect_types_raw2(existing.value_type, idx.value_type),
                                // Intersection: readonly only if ALL constituents are readonly
                                readonly: existing.readonly && idx.readonly,
                                param_name: None,
                            });
                        }
                        _ => {}
                    }
                    match (&callable.number_index, &number_index) {
                        (Some(idx), None) => number_index = Some(*idx),
                        (Some(idx), Some(existing)) => {
                            number_index = Some(IndexSignature {
                                key_type: existing.key_type,
                                value_type: self
                                    .intersect_types_raw2(existing.value_type, idx.value_type),
                                // Intersection: readonly only if ALL constituents are readonly
                                readonly: existing.readonly && idx.readonly,
                                param_name: None,
                            });
                        }
                        _ => {}
                    }
                }
                _ => return None, // Not all callables, can't merge
            }
        }

        if call_signatures.is_empty() {
            return None;
        }

        // Sort properties by name for consistent hashing
        properties.sort_by_key(|p| p.name.0);

        let callable_shape = CallableShape {
            call_signatures,
            construct_signatures: Vec::new(),
            properties,
            string_index,
            number_index,
            symbol: None,
            is_abstract: false,
        };

        let shape_id = self.intern_callable_shape(callable_shape);
        Some(self.intern(TypeData::Callable(shape_id)))
    }

    fn try_merge_objects_in_intersection(&self, members: &[TypeId]) -> Option<TypeId> {
        let mut objects: Vec<Arc<ObjectShape>> = Vec::new();

        // Check if all members are objects
        for &member in members {
            match self.lookup(member) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    objects.push(self.object_shape(shape_id));
                }
                _ => return None, // Not all objects, can't merge
            }
        }

        // Merge all object properties using HashMap index for O(N) total instead of O(N²)
        let mut merged_props: Vec<PropertyInfo> = Vec::new();
        let mut prop_index: rustc_hash::FxHashMap<Atom, usize> = rustc_hash::FxHashMap::default();
        let mut merged_string_index: Option<IndexSignature> = None;
        let mut merged_number_index: Option<IndexSignature> = None;
        let mut merged_fresh = true;

        for obj in &objects {
            // Freshness is only preserved when *all* intersected object members are fresh.
            if !obj.flags.contains(ObjectFlags::FRESH_LITERAL) {
                merged_fresh = false;
            }
            // Merge properties
            for prop in &obj.properties {
                // Check if property already exists using HashMap for O(1) lookup
                if let Some(&idx) = prop_index.get(&prop.name) {
                    let existing = &mut merged_props[idx];
                    // Property exists - intersect the types for stricter checking
                    // In TypeScript, if same property has different types, use intersection
                    // Use raw intersection to avoid infinite recursion.
                    //
                    // CRITICAL: When one property is optional and the other is required,
                    // the optional property's type implicitly includes `undefined`.
                    // We must include this before intersecting, otherwise:
                    //   `a?: { aProp: string }` & `a: undefined`
                    //   incorrectly computes `{ aProp: string } & undefined = never`
                    //   instead of `({ aProp: string } | undefined) & undefined = undefined`
                    if existing.type_id != prop.type_id {
                        let lhs = if existing.optional && !prop.optional {
                            self.union2(existing.type_id, TypeId::UNDEFINED)
                        } else {
                            existing.type_id
                        };
                        let rhs = if prop.optional && !existing.optional {
                            self.union2(prop.type_id, TypeId::UNDEFINED)
                        } else {
                            prop.type_id
                        };
                        existing.type_id = self.intersect_types_raw2(lhs, rhs);
                    }
                    // Merge flags: required wins over optional, readonly only if ALL readonly
                    // For optional: only optional if ALL are optional (required wins)
                    existing.optional = existing.optional && prop.optional;
                    // For readonly: readonly only if ALL members have it readonly
                    // { readonly a: number } & { a: number } = { a: number } (writable)
                    // This matches tsc: if any member says writable, the intersection is writable
                    existing.readonly = existing.readonly && prop.readonly;
                    // Write type: if the merged property is writable, use the merged read
                    // type as write_type to avoid NONE sentinels from readonly members
                    // polluting the result (e.g., intersecting NONE & number = "error & number").
                    // When writable, write_type == type_id (no divergent getter/setter).
                    if !existing.readonly {
                        existing.write_type = existing.type_id;
                    } else if existing.write_type != prop.write_type {
                        existing.write_type =
                            self.intersect_types_raw2(existing.write_type, prop.write_type);
                    }
                    // For visibility: most restrictive wins (Private > Protected > Public)
                    // { private a: number } & { public a: number } = { private a: number }
                    existing.visibility = match (existing.visibility, prop.visibility) {
                        (Visibility::Private, _) | (_, Visibility::Private) => Visibility::Private,
                        (Visibility::Protected, _) | (_, Visibility::Protected) => {
                            Visibility::Protected
                        }
                        (Visibility::Public, Visibility::Public) => Visibility::Public,
                    };
                } else {
                    let new_idx = merged_props.len();
                    prop_index.insert(prop.name, new_idx);
                    merged_props.push(prop.clone());
                }
            }

            // Merge index signatures
            match (&obj.string_index, &merged_string_index) {
                (Some(idx), None) => {
                    merged_string_index = Some(IndexSignature {
                        key_type: idx.key_type,
                        value_type: idx.value_type,
                        readonly: idx.readonly,
                        param_name: None,
                    });
                }
                (Some(idx), Some(existing)) => {
                    merged_string_index = Some(IndexSignature {
                        key_type: existing.key_type,
                        value_type: self.intersect_types_raw2(existing.value_type, idx.value_type),
                        // Intersection: readonly only if ALL constituents are readonly
                        readonly: existing.readonly && idx.readonly,
                        param_name: None,
                    });
                }
                _ => {}
            }

            match (&obj.number_index, &merged_number_index) {
                (Some(idx), None) => {
                    merged_number_index = Some(IndexSignature {
                        key_type: idx.key_type,
                        value_type: idx.value_type,
                        readonly: idx.readonly,
                        param_name: None,
                    });
                }
                (Some(idx), Some(existing)) => {
                    merged_number_index = Some(IndexSignature {
                        key_type: existing.key_type,
                        value_type: self.intersect_types_raw2(existing.value_type, idx.value_type),
                        // Intersection: readonly only if ALL constituents are readonly
                        readonly: existing.readonly && idx.readonly,
                        param_name: None,
                    });
                }
                _ => {}
            }
        }

        // Sort properties by name for consistent hashing
        merged_props.sort_by_key(|p| p.name.0);

        let merged_flags = if merged_fresh {
            ObjectFlags::FRESH_LITERAL
        } else {
            ObjectFlags::empty()
        };

        let shape = ObjectShape {
            flags: merged_flags,
            properties: merged_props,
            string_index: merged_string_index,
            number_index: merged_number_index,
            symbol: None,
        };

        let shape_id = self.intern_object_shape(shape);
        // Preserve index signatures when present.
        if self.object_shape(shape_id).string_index.is_some()
            || self.object_shape(shape_id).number_index.is_some()
        {
            Some(self.intern(TypeData::ObjectWithIndex(shape_id)))
        } else {
            Some(self.intern(TypeData::Object(shape_id)))
        }
    }

    /// Task #43: Extract objects from a mixed intersection, merge them, and return
    /// the merged object along with remaining non-object members.
    ///
    /// This implements partial merging for intersections like:
    /// `{ a: string } & { b: number } & string`
    /// → Extracts: `{ a: string }`, `{ b: number }`
    /// → Merges to: `{ a: string; b: number }`
    /// → Returns: (Some({ a: string; b: number }), [string])
    fn extract_and_merge_objects(
        &self,
        members: &[TypeId],
    ) -> (Option<TypeId>, SmallVec<[TypeId; 4]>) {
        let mut objects: Vec<TypeId> = Vec::new();
        let mut remaining: SmallVec<[TypeId; 4]> = SmallVec::new();

        // Separate objects from non-objects
        for &member in members {
            match self.lookup(member) {
                Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_)) => {
                    objects.push(member);
                }
                _ => {
                    remaining.push(member);
                }
            }
        }

        // If no objects, return early
        if objects.is_empty() {
            return (None, remaining);
        }

        // If only one object, return it as-is
        if objects.len() == 1 {
            return (Some(objects[0]), remaining);
        }

        // Merge all objects using existing merge logic
        if let Some(merged) = self.try_merge_objects_in_intersection(&objects) {
            (Some(merged), remaining)
        } else {
            // Merge failed (shouldn't happen), return objects as-is
            remaining.extend(objects);
            (None, remaining)
        }
    }

    /// Task #43: Extract callables from a mixed intersection, merge them, and return
    /// the merged callable along with remaining non-callable members.
    ///
    /// This implements partial merging for intersections like:
    /// `((x: string) => void) & ((x: number) => void) & { a: number }`
    /// → Extracts: `(x: string) => void`, `(x: number) => void`
    /// → Merges to: Callable with overloads
    /// → Returns: (Some(Callable), [{ a: number }])
    fn extract_and_merge_callables(
        &self,
        members: &[TypeId],
    ) -> (Option<TypeId>, SmallVec<[TypeId; 4]>) {
        let mut callables: Vec<TypeId> = Vec::new();
        let mut remaining: SmallVec<[TypeId; 4]> = SmallVec::new();

        // Separate callables from non-callables
        for &member in members {
            if crate::type_queries::is_callable_type(self, member) {
                callables.push(member);
            } else {
                remaining.push(member);
            }
        }

        // If no callables, return early
        if callables.is_empty() {
            return (None, remaining);
        }

        // If only one callable, return it as-is
        if callables.len() == 1 {
            return (Some(callables[0]), remaining);
        }

        // Merge all callables using existing merge logic
        if let Some(merged) = self.try_merge_callables_in_intersection(&callables) {
            (Some(merged), remaining)
        } else {
            // Merge failed, return callables as-is
            remaining.extend(callables);
            (None, remaining)
        }
    }
}
