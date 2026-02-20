//! Intersection type normalization and merging.

use super::*;

impl TypeInterner {
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
        // FIX: Do not blindly sort all members. Callables must preserve order
        // for correct overload resolution. Non-callables should be sorted for
        // canonicalization.

        // 1. Check if we have any callables (fast path optimization)
        let has_callables = flat.iter().any(|&id| self.is_callable_type(id));

        if !has_callables {
            // Fast path: No callables, sort everything for canonicalization
            flat.sort_by_key(|id| id.0);
            flat.dedup();
        } else {
            // Slow path: Separate callables and others without heap allocation
            // Use SmallVec to keep stack allocation benefits
            let mut callables = SmallVec::<[TypeId; 4]>::new();

            // Retain only non-callables in 'flat', move callables to 'callables'
            // This preserves the order of callables as they are extracted
            let mut i = 0;
            while i < flat.len() {
                if self.is_callable_type(flat[i]) {
                    callables.push(flat.remove(i));
                } else {
                    i += 1;
                }
            }

            // 2. Sort non-callables (which are left in 'flat')
            flat.sort_by_key(|id| id.0);
            flat.dedup();

            // 3. Deduplicate callables (preserving order)
            // Using a set for O(1) lookups while maintaining insertion order
            let mut seen = FxHashSet::default();
            callables.retain(|id| seen.insert(*id));

            // 4. Merge: Put non-callables first (canonical), then callables (ordered)
            // This creates a canonical form where structural types appear before signatures
            flat.extend(callables);
        }

        // Handle special cases
        if flat.contains(&TypeId::ERROR) {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        // If any member is `never`, the intersection is `never`
        if flat.contains(&TypeId::NEVER) {
            return TypeId::NEVER;
        }
        // If any member is `any`, the intersection is `any`
        if flat.contains(&TypeId::ANY) {
            return TypeId::ANY;
        }
        // Remove `unknown` from intersections (identity element)
        flat.retain(|id| *id != TypeId::UNKNOWN);

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

        // Abort reduction if any member is a Lazy type.
        // The interner (Judge) cannot resolve symbols, so if we have unresolved types,
        // we must preserve the intersection as-is without attempting to merge or reduce.
        // This prevents incorrect reductions on type aliases like `type A = { x: number }`.
        let has_unresolved = flat
            .iter()
            .any(|&id| matches!(self.lookup(id), Some(TypeData::Lazy(_))));
        if has_unresolved {
            let list_id = self.intern_type_list(flat.into_vec());
            return self.intern(TypeData::Intersection(list_id));
        }

        // NOTE: narrow_literal_primitive_intersection was removed (Task #43) because it was too aggressive.
        // It caused incorrect behavior in mixed intersections like "a" & string & { x: 1 }.
        // The reduce_intersection_subtypes() at the end correctly handles literal/primitive narrowing
        // via is_subtype_shallow checks without losing other intersection members.

        if self.intersection_has_disjoint_primitives(&flat) {
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

        // Distributivity: A & (B | C) → (A & B) | (A & C)
        // This enables better normalization and is required for soundness
        // Must be done before object/callable merging to ensure we operate on distributed members
        if let Some(distributed) = self.distribute_intersection_over_unions(&flat) {
            return distributed;
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

        // Step 3: Rebuild flat with merged results in canonical form
        // Canonical form: [non-callables sorted, callables ordered]
        let mut final_flat: TypeListBuffer = SmallVec::new();

        // Add remaining non-object, non-callable members (these are non-callables)
        final_flat.extend(remaining_after_callables.iter().copied());

        // Add merged object if present (objects are non-callables)
        if let Some(obj_id) = merged_object {
            final_flat.push(obj_id);
        }

        // Sort all non-callables for canonicalization
        final_flat.sort_by_key(|id| id.0);
        final_flat.dedup();

        // Add merged callable if present (callables must come after non-callables)
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

        let list_id = self.intern_type_list(flat.into_vec());
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
                        type_predicate: func.type_predicate.clone(),
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
                            existing.write_type =
                                self.intersect_types_raw2(existing.write_type, prop.write_type);
                            existing.optional = existing.optional && prop.optional;
                            // Intersection: readonly if ANY constituent is readonly (cumulative)
                            existing.readonly = existing.readonly || prop.readonly;
                        } else {
                            properties.push(prop.clone());
                        }
                    }
                    // Merge index signatures
                    match (&callable.string_index, &string_index) {
                        (Some(idx), None) => string_index = Some(idx.clone()),
                        (Some(idx), Some(existing)) => {
                            string_index = Some(IndexSignature {
                                key_type: existing.key_type,
                                value_type: self
                                    .intersect_types_raw2(existing.value_type, idx.value_type),
                                // Intersection: readonly if ANY constituent is readonly (cumulative)
                                readonly: existing.readonly || idx.readonly,
                            });
                        }
                        _ => {}
                    }
                    match (&callable.number_index, &number_index) {
                        (Some(idx), None) => number_index = Some(idx.clone()),
                        (Some(idx), Some(existing)) => {
                            number_index = Some(IndexSignature {
                                key_type: existing.key_type,
                                value_type: self
                                    .intersect_types_raw2(existing.value_type, idx.value_type),
                                // Intersection: readonly if ANY constituent is readonly (cumulative)
                                readonly: existing.readonly || idx.readonly,
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
        let mut merged_flags = ObjectFlags::empty();

        for obj in &objects {
            // Propagate FRESH_LITERAL flag if any constituent has it
            merged_flags |= obj.flags & ObjectFlags::FRESH_LITERAL;
            // Merge properties
            for prop in &obj.properties {
                // Check if property already exists using HashMap for O(1) lookup
                if let Some(&idx) = prop_index.get(&prop.name) {
                    let existing = &mut merged_props[idx];
                    // Property exists - intersect the types for stricter checking
                    // In TypeScript, if same property has different types, use intersection
                    // Use raw intersection to avoid infinite recursion
                    if existing.type_id != prop.type_id {
                        existing.type_id =
                            self.intersect_types_raw2(existing.type_id, prop.type_id);
                    }
                    if existing.write_type != prop.write_type {
                        existing.write_type =
                            self.intersect_types_raw2(existing.write_type, prop.write_type);
                    }
                    // Merge flags: required wins over optional, readonly is cumulative
                    // For optional: only optional if ALL are optional (required wins)
                    existing.optional = existing.optional && prop.optional;
                    // For readonly: readonly if ANY is readonly (readonly is cumulative)
                    // { readonly a: number } & { a: number } = { readonly a: number }
                    existing.readonly = existing.readonly || prop.readonly;
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
                    });
                }
                (Some(idx), Some(existing)) => {
                    merged_string_index = Some(IndexSignature {
                        key_type: existing.key_type,
                        value_type: self.intersect_types_raw2(existing.value_type, idx.value_type),
                        // Intersection: readonly if ANY constituent is readonly (cumulative)
                        readonly: existing.readonly || idx.readonly,
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
                    });
                }
                (Some(idx), Some(existing)) => {
                    merged_number_index = Some(IndexSignature {
                        key_type: existing.key_type,
                        value_type: self.intersect_types_raw2(existing.value_type, idx.value_type),
                        // Intersection: readonly if ANY constituent is readonly (cumulative)
                        readonly: existing.readonly || idx.readonly,
                    });
                }
                _ => {}
            }
        }

        // Sort properties by name for consistent hashing
        merged_props.sort_by_key(|p| p.name.0);

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
            if self.is_callable_type(member) {
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
