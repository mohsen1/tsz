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
        // IndexAccess types (e.g., S["a"] where S is a type parameter) are deferred
        // types that the interner cannot reason about structurally. Without this,
        // intersections like `S["a"] & (T | undefined)` get distributed into
        // `(S["a"] & T) | (S["a"] & undefined)`, losing the deferred constraint
        // and making `T` incorrectly assignable to `(S & State<T>)["a"]`.
        let has_unresolved = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(
                    TypeData::Lazy(_)
                        | TypeData::Application(_)
                        | TypeData::Mapped(_)
                        | TypeData::IndexAccess(_, _)
                )
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

        // TS2590: Cross-product union size check for intersections containing unions.
        // When an intersection has union members (e.g., `(A|B) & (C|D) & ...`), the
        // cross-product can grow exponentially.  tsc's checkCrossProductUnion in
        // getIntersectionType bails at 100,000 regardless of whether distribution
        // will actually occur.  We must check BEFORE the distribution guard so that
        // all-union intersections (where distribution is skipped) still trigger the flag.
        //
        // Skip the check when any union member contains type parameters, lazy types,
        // or indexed access types — the actual cross-product depends on instantiation
        // and counting them at definition time would cause false positives (e.g.,
        // `("a" | T[0]) & ("b" | T[1]) & ...` in divideAndConquerIntersections.ts).
        {
            let mut cross_product_size: u64 = 1;
            let mut has_generic_union = false;
            for &id in flat.iter() {
                if let Some(TypeData::Union(members)) = self.lookup(id) {
                    let member_types = self.type_list(members);
                    // Check if any union member is non-concrete
                    let is_concrete = member_types.iter().all(|&m| {
                        !matches!(
                            self.lookup(m),
                            Some(
                                TypeData::TypeParameter(_)
                                    | TypeData::Lazy(_)
                                    | TypeData::IndexAccess(_, _)
                                    | TypeData::Conditional(_)
                                    | TypeData::Application(_)
                            )
                        )
                    });
                    if !is_concrete {
                        has_generic_union = true;
                        break;
                    }
                    cross_product_size =
                        cross_product_size.saturating_mul(member_types.len() as u64);
                    if cross_product_size >= 100_000 {
                        self.set_union_too_complex();
                        break;
                    }
                }
            }
            // Don't flag when generics are present — actual size depends on instantiation
            if has_generic_union {
                // Clear the flag if it was set during partial iteration
            }
        }

        // Distributivity: A & (B | C) → (A & B) | (A & C)
        // When there are non-union members, always distribute (the non-union
        // member anchors each alternative). When ALL members are unions,
        // only use distribution if the result genuinely simplifies (no
        // remaining intersection types in the result). This matches tsc:
        // `(string|boolean) & (boolean|null)` → `boolean`, but
        // `(A|B) & (C|D)` with interfaces stays as intersection.
        let has_non_union = flat
            .iter()
            .any(|&id| !matches!(self.lookup(id), Some(TypeData::Union(_))));
        if has_non_union {
            if let Some(distributed) = self.distribute_intersection_over_unions(&flat) {
                return distributed;
            }
        } else {
            // All-union: pre-check cross-product size to avoid triggering
            // set_union_too_complex side-effect for large intersections.
            let cross_product: usize = flat
                .iter()
                .filter_map(|&id| match self.lookup(id) {
                    Some(TypeData::Union(members)) => Some(self.type_list(members).len()),
                    _ => None,
                })
                .fold(1usize, |acc, n| acc.saturating_mul(n));
            if cross_product <= 25
                && let Some(distributed) = self.distribute_intersection_over_unions(&flat)
            {
                let is_simpler = match self.lookup(distributed) {
                    Some(TypeData::Union(members)) => {
                        let list = self.type_list(members);
                        !list
                            .iter()
                            .any(|&m| matches!(self.lookup(m), Some(TypeData::Intersection(_))))
                    }
                    _ => true,
                };
                if is_simpler {
                    return distributed;
                }
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

        // Capture original object members before merging so we can store a
        // display alias that preserves the `{ a: T; } & { b: U; }` form tsc
        // uses in error messages.
        let original_objects: SmallVec<[TypeId; 4]> = flat
            .iter()
            .filter(|&&id| {
                matches!(
                    self.lookup(id),
                    Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
                )
            })
            .copied()
            .collect();

        // Step 1: Extract and merge objects from mixed intersection
        let (merged_object, remaining_after_objects) = self.extract_and_merge_objects(&flat);

        // When 2+ objects are merged into one, store a display alias from the
        // merged object back to the raw (un-normalized) intersection of the
        // original members.  This lets the formatter show `{ a: T; } & { b: U; }`
        // instead of the flattened `{ a: T; b: U; }`, matching tsc behavior.
        if let Some(merged_id) = merged_object
            && original_objects.len() >= 2
        {
            let raw_list = self.intern_type_list_from_slice(&original_objects);
            let raw_intersection = self.intern(TypeData::Intersection(raw_list));
            // Only alias when the merged result is a genuinely new type (not one of
            // the original members).  If merged_id equals an original member it means
            // the objects were structurally identical; aliasing would make that type
            // display as `{ x: string; } & { x: string; }` everywhere.
            if raw_intersection != merged_id && !original_objects.contains(&merged_id) {
                self.store_display_alias(merged_id, raw_intersection);
            }
        }

        // Capture original callable members before merging so we can store a
        // display alias that preserves the `((a: T) => R1) & ((b: U) => R2)` form
        // tsc uses in error messages.
        let original_callables: SmallVec<[TypeId; 4]> = remaining_after_objects
            .iter()
            .filter(|&&id| crate::type_queries::is_callable_type(self, id))
            .copied()
            .collect();

        // Step 2: Extract and merge callables from remaining members
        let (merged_callable, remaining_after_callables) =
            self.extract_and_merge_callables(&remaining_after_objects);

        // When 2+ callables are merged into one, store a display alias from the
        // merged callable back to the raw intersection of the original members.
        // This lets the formatter show `((a: T) => R1) & ((b: U) => R2)` instead
        // of the merged callable object notation, matching tsc behavior.
        if let Some(merged_id) = merged_callable
            && original_callables.len() >= 2
            && !original_callables.contains(&merged_id)
        {
            let raw_list = self.intern_type_list_from_slice(&original_callables);
            let raw_intersection = self.intern(TypeData::Intersection(raw_list));
            if raw_intersection != merged_id {
                self.store_display_alias(merged_id, raw_intersection);
            }
        }

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
        // Don't merge when multiple members contribute construct signatures.
        // This preserves the intersection structure so that `resolve_intersection_new`
        // can properly intersect the instance types (mixin pattern). Merging would
        // collapse `(new => A) & (new => B)` into a single callable with two
        // construct signatures treated as overloads, losing the intersection semantics.
        let mut construct_source_count = 0;
        for &member in members {
            let has_construct = match self.lookup(member) {
                Some(TypeData::Function(func_id)) => self.function_shape(func_id).is_constructor,
                Some(TypeData::Callable(callable_id)) => !self
                    .callable_shape(callable_id)
                    .construct_signatures
                    .is_empty(),
                _ => false,
            };
            if has_construct {
                construct_source_count += 1;
                if construct_source_count > 1 {
                    return None; // Don't merge: keep intersection of constructors
                }
            }
        }

        let mut call_signatures: Vec<CallSignature> = Vec::new();
        let mut construct_signatures: Vec<CallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;
        let mut is_abstract = false;

        // Collect all call/construct signatures and properties.
        for &member in members {
            match self.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let func = self.function_shape(func_id);
                    let signature = CallSignature {
                        type_params: func.type_params.clone(),
                        params: func.params.clone(),
                        this_type: func.this_type,
                        return_type: func.return_type,
                        type_predicate: func.type_predicate,
                        is_method: func.is_method,
                    };
                    if func.is_constructor {
                        construct_signatures.push(signature);
                    } else {
                        call_signatures.push(signature);
                    }
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.callable_shape(callable_id);
                    // Add all call signatures
                    for sig in &callable.call_signatures {
                        call_signatures.push(sig.clone());
                    }
                    // Add all construct signatures
                    for sig in &callable.construct_signatures {
                        construct_signatures.push(sig.clone());
                    }
                    is_abstract |= callable.is_abstract;
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

        if call_signatures.is_empty() && construct_signatures.is_empty() {
            return None;
        }

        // Sort properties by name for consistent hashing
        properties.sort_by_key(|p| p.name.0);

        let callable_shape = CallableShape {
            call_signatures,
            construct_signatures,
            properties,
            string_index,
            number_index,
            symbol: None,
            is_abstract,
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
                    // Write type: handle readonly vs writable merging and divergent accessors.
                    // - NONE sentinel means "readonly, no setter". Skip it when merging.
                    // - If both have real write_types, intersect them for divergent accessor support.
                    // - Use normalizing `intersection2` to distribute over unions and reduce subtypes.
                    if existing.write_type != prop.write_type {
                        if prop.write_type == TypeId::NONE {
                            // prop is readonly, keep existing.write_type unchanged
                        } else if existing.write_type == TypeId::NONE {
                            // existing was readonly, use prop's write_type
                            existing.write_type = prop.write_type;
                        } else {
                            // Both have real write_types, intersect them
                            existing.write_type =
                                self.intersection2(existing.write_type, prop.write_type);
                        }
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
        let result = if self.object_shape(shape_id).string_index.is_some()
            || self.object_shape(shape_id).number_index.is_some()
        {
            self.intern(TypeData::ObjectWithIndex(shape_id))
        } else {
            self.intern(TypeData::Object(shape_id))
        };

        // Propagate display properties from input objects to the merged result.
        let display_vec = crate::types::merge_display_properties_for_intersection(self, members);
        if !display_vec.is_empty() {
            self.store_display_properties(result, display_vec);
        }

        Some(result)
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
