//! Property-based type narrowing.
//!
//! This module contains narrowing methods for:
//! - `in` operator narrowing (property presence check)
//! - Property type lookup for narrowing
//! - Object-like type detection for instanceof support

use super::{CachedPropertyType, NarrowingContext};
use crate::operations::property::PropertyAccessResult;
use crate::types::{
    IntrinsicKind, ObjectFlags, ObjectShapeId, PropertyInfo, TypeData, TypeId, Visibility,
};
use crate::visitor::{
    intersection_list_id, object_shape_id, object_with_index_shape_id, type_param_info,
    union_list_id,
};
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

impl<'a> NarrowingContext<'a> {
    /// Check if a type is object-like (has object structure)
    ///
    /// This is used to determine if two types can form an intersection
    /// for instanceof narrowing when they're not directly assignable.
    pub(crate) fn are_object_like(&self, type_id: TypeId) -> bool {
        use crate::types::TypeData;

        match self.db.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Function(_)
                | TypeData::Callable(_),
            ) => true,

            // Interface and class types (which are object-like)
            Some(TypeData::Application(_)) => {
                // Check if the application type has construct signatures or object structure
                use crate::type_queries::{InstanceTypeKind, classify_for_instance_type};

                matches!(
                    classify_for_instance_type(self.db, type_id),
                    InstanceTypeKind::Callable(_) | InstanceTypeKind::Function(_)
                )
            }

            // Type parameters - check their constraint
            Some(TypeData::TypeParameter(info)) => {
                // For instanceof, generics with object constraints are treated as object-like
                // This allows intersection narrowing for cases like: T & MyClass
                info.constraint.is_none_or(|c| self.are_object_like(c))
            }

            // Intersection of object types
            Some(TypeData::Intersection(members)) => {
                let members = self.db.type_list(members);
                members.iter().any(|&member| self.are_object_like(member))
            }

            _ => false,
        }
    }

    /// Narrow a type based on an `in` operator check.
    ///
    /// Example: `"a" in x` narrows `A | B` to include only types that have property `a`
    pub fn narrow_by_property_presence(
        &self,
        source_type: TypeId,
        property_name: Atom,
        present: bool,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_property_presence",
            source_type = source_type.0,
            ?property_name,
            present
        )
        .entered();

        // Handle special cases
        if source_type == TypeId::ANY {
            trace!("Source type is ANY, returning unchanged");
            return TypeId::ANY;
        }

        if source_type == TypeId::NEVER {
            trace!("Source type is NEVER, returning unchanged");
            return TypeId::NEVER;
        }

        if source_type == TypeId::UNKNOWN {
            if !present {
                // False branch: property is not present. Since unknown could be anything,
                // it remains unknown in the false branch.
                trace!("UNKNOWN in false branch for in operator, returning UNKNOWN");
                return TypeId::UNKNOWN;
            }

            // For unknown, narrow to object & { [prop]: unknown }
            // This matches TypeScript's behavior where `in` check on unknown
            // narrows to object type with the property
            let prop_type = TypeId::UNKNOWN;
            let required_prop = PropertyInfo {
                name: property_name,
                type_id: prop_type,
                write_type: prop_type,
                optional: false, // Property becomes required after `in` check
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            };
            let filter_obj = self.db.object(vec![required_prop]);
            let narrowed = self.db.intersection2(TypeId::OBJECT, filter_obj);
            trace!("Narrowing unknown to object & property = {}", narrowed.0);
            return narrowed;
        }

        // Resolve a `Lazy` alias / generic `Application` to its structural form
        // before dispatching on union-vs-type-parameter-vs-non-union: a union
        // written behind a (possibly generic) alias arrives as `Lazy`/`Application`,
        // not a `Union`, so without this the union check misses it and the receiver
        // is degraded to `source & Record<prop, unknown>`. Structural decisions use
        // the resolved view; no-op narrowings still return the original `source_type`
        // to preserve the alias display.
        let resolved_source = self.resolve_type(source_type);

        // Handle type parameters: narrow the constraint and intersect if changed
        if let Some(type_param_info) = type_param_info(self.db, resolved_source) {
            if let Some(constraint) = type_param_info.constraint
                && constraint != resolved_source
            {
                let narrowed_constraint =
                    self.narrow_by_property_presence(constraint, property_name, present);
                if narrowed_constraint != constraint {
                    trace!(
                        "Type parameter constraint narrowed from {} to {}, creating intersection",
                        constraint.0, narrowed_constraint.0
                    );
                    return self.db.intersection2(source_type, narrowed_constraint);
                }
            }
            // Bare type parameter (no constraint or unchanged constraint).
            // For positive narrowing, tsc adds `Record<prop, unknown>` so
            // chained `in` checks like `"a" in x && "b" in x` see the second
            // operand's `x` as a valid `in`-RHS. Without this, every `in x`
            // re-emits TS2322 against the unnarrowed type parameter. This is
            // strictly more informative than narrowing to `T & object`,
            // because `Record<prop, unknown>` extends `object` and also
            // records the known property.
            if present {
                trace!(
                    "Bare type parameter, intersecting with Record<prop, unknown> for present narrowing"
                );
                let record_type = self.make_record_type(property_name);
                return self.db.intersection2(source_type, record_type);
            }
            // Type parameter with no constraint (false branch) or unchanged constraint
            trace!("Type parameter unchanged, returning source");
            return source_type;
        }

        // If source is a union, filter members based on property presence
        if let Some(members_id) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members_id);
            trace!(
                "Checking property {} in union with {} members",
                self.db.resolve_atom_ref(property_name),
                members.len()
            );

            let matching: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    // CRITICAL: Resolve Lazy types for each member
                    let resolved_member = self.resolve_type(member);

                    let has_property = self.type_has_property(resolved_member, property_name);
                    if present {
                        // Positive: "prop" in member
                        if has_property {
                            // Property exists: Keep the member as-is
                            // CRITICAL: For union narrowing, we don't modify the member type
                            // We just filter to keep only members that have the property
                            Some(member)
                        } else {
                            // Property not found: Exclude member
                            // Per TypeScript: "prop in x" being true means x MUST have the property
                            // If x doesn't have it (and no index signature), narrow to never
                            None
                        }
                    } else {
                        // Negative: !("prop" in member)
                        // Exclude member ONLY if property is required
                        if self.is_property_required(resolved_member, property_name) {
                            return None;
                        }
                        // Keep member (no required property found, or property is optional)
                        Some(member)
                    }
                })
                .collect();

            if matching.is_empty() {
                if present {
                    // Positive branch with NO member declaring the property:
                    // tsc's `narrowTypeByInKeyword` sees `isKnownProperty` as
                    // false and, on the `assumeTrue` path, narrows to
                    // `Union & Record<prop, unknown>` rather than discarding the
                    // type entirely (it could structurally gain the property at
                    // runtime, having passed the `in` check).
                    trace!(
                        "No members have property, creating intersection with Record<prop, unknown>"
                    );
                    let record_type = self.make_record_type(property_name);
                    return self.db.intersect_types_raw2(source_type, record_type);
                }
                // Negative branch with every member *requiring* the property:
                // `filterType` drops all constituents because the `in` check can
                // never be false, so tsc narrows to `never`. (A member lacking
                // the property, or holding it optionally, is kept above, so an
                // empty result here means presence was guaranteed everywhere.)
                trace!("All members require property; negative `in` branch is never");
                return TypeId::NEVER;
            } else if matching.len() == 1 {
                trace!(
                    "Found single member after filtering, returning {}",
                    matching[0].0
                );
                return matching[0];
            }
            trace!("Created union with {} members", matching.len());
            return self.db.union(matching);
        }

        // For non-union types, check if the property exists. `resolved_source`
        // already resolved any Lazy/Application reference above.
        let has_property = self.type_has_property(resolved_source, property_name);

        if present {
            // Positive: "prop" in x
            //
            // `has_property` is apparent-type aware, so this covers both an
            // apparent-only member (e.g. `push` on `T[]`) and a declared own
            // property. In either case the property is already part of the
            // non-union receiver's type, so the `in` check adds nothing.
            if has_property {
                // The property is already declared on the (non-union) source
                // type — whether contributed by the apparent type (no own slot)
                // or by an own property slot. tsc's `narrowTypeByInKeyword`
                // adds no structural information in this case: the property is
                // already known, so the receiver is returned unchanged. In
                // particular an *optional* own property keeps its optionality
                // (so `delete x.p` after `"p" in x` stays legal) and its
                // declared type keeps `undefined` (so `"p" in x` does not
                // discriminate the property's value type). Synthesising a
                // required-property intersection wrongly stripped both, causing
                // false TS2790 on `delete` and unsoundly excluding `undefined`
                // from the property's type (ofetch `delete options.query`).
                source_type
            } else {
                // Property not found on a non-union type.
                // TypeScript narrows to `source_type & Record<prop, unknown>` for
                // object-like types (which is any type that can be a valid RHS of `in`).
                // This handles cases like `"a" in x` where `x: object` or `x: { b: string }`.
                trace!(
                    "Property not found on non-union type, creating intersection with Record<prop, unknown>"
                );
                let record_type = self.make_record_type(property_name);
                self.db.intersection2(source_type, record_type)
            }
        } else {
            // Negative: !("prop" in x)
            //
            // tsc's `narrowTypeByInKeyword` runs
            // `filterType(type, t => isTypePresencePossible(t, name, /*assumeTrue*/ false))`.
            // For a non-union receiver `filterType` either keeps the whole type
            // or drops it to `never`; it never rewrites the property's value
            // type. `isTypePresencePossible` reports a property as *possibly
            // absent* (keeping the receiver) for every case except a *required*
            // own property, whose presence is guaranteed and therefore makes
            // the negative branch unreachable (`never`).
            //
            // In particular an *optional* property keeps the receiver unchanged
            // even when its declared type excludes `undefined`: the property may
            // legitimately be missing at runtime, so a later `x.p = ...` write
            // must still type-check. Intersecting the receiver with a synthetic
            // `{ p: undefined }` shape (the previous behavior) collapsed `p` to
            // `never` whenever its declared type omitted `undefined`, producing
            // spurious TS2322 on the negative branch.
            if self.is_property_required(resolved_source, property_name) {
                return TypeId::NEVER;
            }
            // Optional own property, index-signature match, or no property at
            // all: the property may be absent at runtime, so the receiver is
            // returned unchanged.
            source_type
        }
    }

    /// Create a `Record<prop, unknown>` type: `{ [prop]: unknown }`.
    ///
    /// Used for `in` operator narrowing when the property doesn't exist on the
    /// source type. TypeScript narrows to `source & Record<prop, unknown>` to
    /// indicate the type must have the property after the check.
    fn make_record_type(&self, property_name: Atom) -> TypeId {
        let is_symbol_named = self
            .db
            .resolve_atom_ref(property_name)
            .starts_with("__unique_");
        let required_prop = PropertyInfo {
            name: property_name,
            type_id: TypeId::UNKNOWN,
            write_type: TypeId::UNKNOWN,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named,
            single_quoted_name: false,
        };
        self.db
            .object_with_flags(vec![required_prop], ObjectFlags::IN_OPERATOR_RECORD)
    }

    /// Check if a type has a specific property.
    ///
    /// Returns true if the type has the property (required or optional),
    /// or has an index signature that would match the property.
    pub(crate) fn type_has_property(&self, type_id: TypeId, property_name: Atom) -> bool {
        if self.get_property_type(type_id, property_name).is_some() {
            return true;
        }
        // The structural lookup above only models object/intersection shapes.
        // Array, tuple, function and primitive receivers carry their members on
        // their *apparent* type (`Array<T>`/`ReadonlyArray<T>`, the global
        // `Function`, the boxed primitive interfaces), so `tsc`'s `in`-operator
        // narrowing resolves the key against the apparent type. Mirror that for
        // parity (e.g. `"push" in arr`, `"call" in fn`).
        self.apparent_member_presence(type_id, property_name)
            .is_some()
    }

    /// Check if a property exists and is required on a type.
    ///
    /// Returns true if the property is required (not optional).
    /// This is used for negative narrowing: `!("prop" in x)` should
    /// exclude types where `prop` is required.
    pub(crate) fn is_property_required(&self, type_id: TypeId, property_name: Atom) -> bool {
        let resolved_type = self.resolve_type(type_id);
        if matches!(
            self.db.lookup(resolved_type),
            Some(TypeData::Lazy(_) | TypeData::TypeQuery(_))
        ) {
            return false;
        }

        let key = (resolved_type, self.resolver_generation(), property_name);
        if let Some(&cached) = self.cache.required_property_cache.borrow().get(&key) {
            return cached;
        }

        let required = self.is_property_required_uncached(resolved_type, property_name);
        self.cache
            .required_property_cache
            .borrow_mut()
            .insert(key, required);
        required
    }

    fn is_property_required_uncached(&self, resolved_type: TypeId, property_name: Atom) -> bool {
        // Helper to check a specific shape
        let check_shape = |shape_id: ObjectShapeId| -> bool {
            let shape = self.db.object_shape(shape_id);
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, property_name) {
                return !prop.optional;
            }
            false
        };

        // Check standard object shape
        if let Some(shape_id) = object_shape_id(self.db, resolved_type)
            && check_shape(shape_id)
        {
            return true;
        }

        // Check object with index shape (CRITICAL for interfaces/classes)
        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type)
            && check_shape(shape_id)
        {
            return true;
        }

        // Check intersection members
        // If ANY member requires it, the intersection requires it
        if let Some(members_id) = intersection_list_id(self.db, resolved_type) {
            let members = self.db.type_list(members_id);
            return members
                .iter()
                .any(|&m| self.is_property_required(m, property_name));
        }

        // Apparent-type members of arrays/tuples/functions/primitives (e.g.
        // `push`, `call`, `length`) are non-optional declarations on the boxed
        // interface, so they are *required*: `!("push" in x)` excludes a `T[]`
        // constituent. Members reached only through an index signature are not
        // guaranteed to be present, so they stay (mirroring the object-shape
        // index-signature handling, which never marks index members required).
        if let Some(from_index_signature) =
            self.apparent_member_presence(resolved_type, property_name)
        {
            return !from_index_signature;
        }

        false
    }

    /// Resolve `property_name` against the *apparent* type of receivers that the
    /// structural lookup above does not model: arrays, tuples, function types
    /// and primitives expose their members through their apparent type
    /// (`Array<T>`/`ReadonlyArray<T>`, the global `Function`, the boxed
    /// primitive interfaces) rather than through an object shape.
    ///
    /// Returns `Some(from_index_signature)` when the apparent type has the
    /// member, where `from_index_signature` reports whether it was matched via
    /// an index signature rather than a named declaration. Object, class and
    /// interface shapes are intentionally excluded because the structural
    /// lookup already covers them, and routing them through the heavier
    /// property resolver would be redundant.
    fn apparent_member_presence(&self, type_id: TypeId, property_name: Atom) -> Option<bool> {
        let resolved_type = self.resolve_type(type_id);
        let bears_apparent_members = match self.db.lookup(resolved_type) {
            Some(
                TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::ReadonlyType(_),
            ) => true,
            Some(TypeData::Intrinsic(kind)) => matches!(
                kind,
                IntrinsicKind::String
                    | IntrinsicKind::Number
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol
            ),
            _ => false,
        };
        if !bears_apparent_members {
            return None;
        }

        match self
            .db
            .resolve_property_access_atom(resolved_type, property_name)
        {
            PropertyAccessResult::Success {
                from_index_signature,
                ..
            } => Some(from_index_signature),
            _ => None,
        }
    }

    /// Get the type of a property if it exists.
    ///
    /// Returns Some(type) if the property exists, None otherwise.
    pub(crate) fn get_property_type(&self, type_id: TypeId, property_name: Atom) -> Option<TypeId> {
        self.get_property_type_entry(type_id, property_name)
            .map(|entry| entry.type_id)
    }

    fn get_property_type_entry(
        &self,
        type_id: TypeId,
        property_name: Atom,
    ) -> Option<CachedPropertyType> {
        let resolved_type = self.resolve_type(type_id);
        let key = (resolved_type, self.resolver_generation(), property_name);

        if let Some(&cached) = self.cache.property_cache.borrow().get(&key) {
            if let Some(cached_entry) = cached {
                let cached_prop_type = cached_entry.type_id;
                if cached_prop_type != TypeId::ERROR
                    && !matches!(
                        self.db.lookup(cached_prop_type),
                        Some(TypeData::Lazy(_) | TypeData::TypeQuery(_))
                    )
                {
                    return Some(cached_entry);
                }
                let resolved_cached = self.resolve_type(cached_prop_type);
                if resolved_cached != cached_prop_type {
                    let resolved_entry = CachedPropertyType {
                        type_id: resolved_cached,
                        from_index_signature: cached_entry.from_index_signature,
                    };
                    self.cache
                        .property_cache
                        .borrow_mut()
                        .insert(key, Some(resolved_entry));
                    return Some(resolved_entry);
                }
                return Some(cached_entry);
            }
            return None;
        }

        if matches!(
            self.db.lookup(resolved_type),
            Some(TypeData::Lazy(_) | TypeData::TypeQuery(_))
        ) {
            return None;
        }

        let result = self.get_property_type_uncached(resolved_type, property_name);

        // Cache the resolved property type so hot paths avoid an extra resolve pass.
        // Don't cache unresolved symbolic/error property types for the same reason as
        // discriminant lookups.
        let should_cache = match result {
            Some(entry) => {
                entry.type_id != TypeId::ERROR
                    && !matches!(
                        self.db.lookup(entry.type_id),
                        Some(TypeData::Lazy(_) | TypeData::TypeQuery(_))
                    )
            }
            None => true,
        };
        if should_cache {
            self.cache.property_cache.borrow_mut().insert(key, result);
        }
        result
    }

    fn get_property_type_uncached(
        &self,
        resolved_type: TypeId,
        property_name: Atom,
    ) -> Option<CachedPropertyType> {
        // Check intersection types - property exists if ANY member has it
        if let Some(prop) = self.get_property_info(resolved_type, property_name) {
            return Some(CachedPropertyType::explicit(prop.type_id));
        }

        if let Some(members_id) = intersection_list_id(self.db, resolved_type) {
            let members = self.db.type_list(members_id);
            // Return the type from the first member that has the property
            for &member in members.iter() {
                // Resolve each member in the intersection
                let resolved_member = self.resolve_type(member);
                if let Some(entry) = self.get_property_type_entry(resolved_member, property_name) {
                    return Some(entry);
                }
            }
            return None;
        }

        // Check object shape
        if let Some(shape_id) = object_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);

            // Check if the property exists in the object's properties
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, property_name) {
                return Some(CachedPropertyType::explicit(prop.type_id));
            }

            // Check index signatures
            // If the object has a string index signature, it has any string property
            if let Some(ref string_idx) = shape.string_index {
                // String index signature matches any string property
                return Some(CachedPropertyType::index_signature(string_idx.value_type));
            }

            // If the object has a number index signature and the property name is numeric
            if let Some(ref number_idx) = shape.number_index {
                let prop_str = self.db.resolve_atom_ref(property_name);
                if prop_str.chars().all(|c| c.is_ascii_digit()) {
                    return Some(CachedPropertyType::index_signature(number_idx.value_type));
                }
            }

            return None;
        }

        // Check object with index signature
        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);

            // Check properties first
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, property_name) {
                return Some(CachedPropertyType::explicit(prop.type_id));
            }

            // Check index signatures
            if let Some(ref string_idx) = shape.string_index {
                return Some(CachedPropertyType::index_signature(string_idx.value_type));
            }

            if let Some(ref number_idx) = shape.number_index {
                let prop_str = self.db.resolve_atom_ref(property_name);
                if prop_str.chars().all(|c| c.is_ascii_digit()) {
                    return Some(CachedPropertyType::index_signature(number_idx.value_type));
                }
            }

            return None;
        }

        // For other types (functions, classes, arrays, etc.), assume they don't have arbitrary properties
        // unless they have been handled above (object shapes, etc.)
        None
    }

    fn get_property_info(&self, type_id: TypeId, property_name: Atom) -> Option<PropertyInfo> {
        let resolved_type = self.resolve_type(type_id);

        if let Some(shape_id) = object_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, property_name) {
                return Some(prop.clone());
            }
        }

        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);
            if let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, property_name) {
                return Some(prop.clone());
            }
        }

        None
    }
}
