//! Object type subtype checking.
//!
//! This module handles subtyping for TypeScript's object types:
//! - Plain objects with named properties
//! - Objects with index signatures (string and number)
//! - Property compatibility (optional, readonly, type, `write_type`)
//! - **Rule #26**: Split Accessors (Getter/Setter Variance)
//!   - Read types are covariant: source.read <: target.read
//!   - Write types are contravariant: target.write <: source.write
//!   - Readonly target properties only check read type (no write access)
//! - Private brand checking for nominal class typing

use crate::operations::iterators::get_iterator_info;
use crate::type_queries::get_return_type;
use crate::types::{ObjectFlags, ObjectShape, ObjectShapeId, PropertyInfo, TypeId, Visibility};
use crate::utils;
use crate::visitor::application_id;
use tsz_common::interner::Atom;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Look up a property in a list of properties, using cached index if available.
    pub(crate) fn lookup_property<'props>(
        &self,
        props: &'props [PropertyInfo],
        shape_id: Option<ObjectShapeId>,
        name: Atom,
    ) -> Option<&'props PropertyInfo> {
        crate::utils::lookup_property(self.interner, props, shape_id, name)
    }

    fn extract_iterator_like_yield_type(&self, type_id: TypeId) -> Option<TypeId> {
        let app_id = application_id(self.interner, type_id)?;
        let app = self.interner.type_application(app_id);
        app.args.first().copied()
    }

    fn has_compatible_symbol_iterator_methods(
        &mut self,
        source: &PropertyInfo,
        target: &PropertyInfo,
        source_method_type: TypeId,
        target_method_type: TypeId,
    ) -> bool {
        let symbol_iterator = self.interner.intern_string("[Symbol.iterator]");
        let internal_iterator = self.interner.intern_string("__@iterator");
        let is_iterator_name = |name: Atom| name == symbol_iterator || name == internal_iterator;
        if !is_iterator_name(source.name) || !is_iterator_name(target.name) {
            return false;
        }

        let Some(query_db) = self.query_db else {
            return false;
        };

        let Some(source_return_type) = get_return_type(query_db, source_method_type) else {
            return false;
        };
        let Some(target_return_type) = get_return_type(query_db, target_method_type) else {
            return false;
        };

        let source_yield_type = get_iterator_info(query_db, source_return_type, false)
            .map(|info| info.yield_type)
            .or_else(|| self.extract_iterator_like_yield_type(source_return_type));
        let target_yield_type = get_iterator_info(query_db, target_return_type, false)
            .map(|info| info.yield_type)
            .or_else(|| self.extract_iterator_like_yield_type(target_return_type));

        source_yield_type
            .zip(target_yield_type)
            .is_some_and(|(source_yield, target_yield)| {
                self.check_subtype(source_yield, target_yield).is_true()
            })
    }

    /// Check private brand compatibility for object subtyping.
    ///
    /// Private brands are used for nominal typing of classes with private fields.
    /// If both source and target have private brands, they must be the same.
    /// If target has a brand but source doesn't (e.g., object literal), this fails.
    /// Returns false if brands don't match, true otherwise (including when neither has a brand).
    pub(crate) fn check_private_brand_compatibility(
        &self,
        source: &[PropertyInfo],
        target: &[PropertyInfo],
    ) -> bool {
        // Fast path: if neither side has non-public properties, there can't be any
        // private brands. This avoids the expensive resolve_atom + starts_with scan
        // on every property.
        let target_has_nonpublic = target.iter().any(|p| p.visibility != Visibility::Public);
        if !target_has_nonpublic {
            // No non-public target properties → no brand to check against
            return true;
        }

        let source_brand = source.iter().find(|p| {
            p.visibility != Visibility::Public && {
                let name = self.interner.resolve_atom(p.name);
                name.starts_with("__private_brand_")
            }
        });
        let target_brand = target.iter().find(|p| {
            p.visibility != Visibility::Public && {
                let name = self.interner.resolve_atom(p.name);
                name.starts_with("__private_brand_")
            }
        });

        // Check private brand compatibility
        match (source_brand, target_brand) {
            (Some(s_brand), Some(t_brand)) => {
                // Both have private brands - they must match exactly
                let s_brand_name = self.interner.resolve_atom(s_brand.name);
                let t_brand_name = self.interner.resolve_atom(t_brand.name);
                s_brand_name == t_brand_name
            }
            (None, Some(_)) => {
                // Target has a private brand but source doesn't
                // This happens when assigning object literal to class with private members
                // Object literals can never have private brands, so this fails
                false
            }
            _ => {
                // Neither has a brand, or source has brand but target doesn't - both OK
                true
            }
        }
    }

    /// Check object subtyping (structural with nominal optimization).
    ///
    /// Validates that source object is a subtype of target object by checking:
    /// 1. **Fast path**: Nominal inheritance check (O(1) for class instances)
    /// 2. Private brand compatibility (for nominal class typing with private fields)
    /// 3. For each target property, source must have a compatible property
    pub(crate) fn check_object_subtype(
        &mut self,
        source: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let source_receiver = self
            .receiver_type_from_shape_symbol(source)
            .or(source_receiver);
        let target_receiver = self
            .receiver_type_from_shape_symbol(target)
            .or(target_receiver);
        // Private brand checking for nominal typing of classes with private fields
        if !self.check_private_brand_compatibility(&source.properties, &target.properties) {
            return SubtypeResult::False;
        }

        // Weak type check (TS2559): if the target is a "weak type" (all properties optional,
        // at least one property, no index signatures), reject if the source has properties
        // but none in common with the target. This check is propagated from CompatChecker
        // via the `enforce_weak_types` flag so union-member structural comparisons cannot
        // bypass weak-type rejection by jumping directly into the subtype kernel.
        // Top-level compat still owns the richer TS2559 diagnostics and exemptions; this
        // shared relation path only enforces the semantic incompatibility.
        // Check ordering: O(1) flag/length guards first, then O(n) shape scan, then O(m+n) merge.
        if self.enforce_weak_types
            && !source.properties.is_empty()
            && Self::is_weak_type_shape(target)
            && !crate::utils::has_common_property_name(&source.properties, &target.properties)
        {
            return SubtypeResult::False;
        }

        // Fast fail for private/protected members: check these first so unrelated
        // class instances can fail before expensive public method comparison.
        for t_prop in &target.properties {
            if t_prop.visibility == Visibility::Public {
                continue;
            }

            let Some(s_prop) =
                self.lookup_property(&source.properties, source_shape_id, t_prop.name)
            else {
                return SubtypeResult::False;
            };

            let result =
                self.check_property_compatibility(s_prop, t_prop, source_receiver, target_receiver);
            if !result.is_true() {
                return result;
            }
        }

        let source_len = source.properties.len();
        let target_len = target.properties.len();
        let use_merge_scan =
            source_shape_id.is_none() || source_len <= target_len.saturating_mul(4);

        if use_merge_scan {
            return self.check_object_subtype_merge_scan(
                source,
                target,
                source_receiver,
                target_receiver,
            );
        }

        // For each property in target, source must have a compatible property
        for t_prop in &target.properties {
            // Private/protected members were handled in the fast-fail prepass.
            if t_prop.visibility != Visibility::Public {
                continue;
            }
            let s_prop = self.lookup_property(&source.properties, source_shape_id, t_prop.name);

            let result = match s_prop {
                Some(sp) => {
                    self.check_property_compatibility(sp, t_prop, source_receiver, target_receiver)
                }
                None => {
                    // Private/Protected properties cannot be missing, even if optional
                    if t_prop.visibility != Visibility::Public {
                        return SubtypeResult::False;
                    }

                    // Property missing - only OK if target property is optional
                    if t_prop.optional {
                        SubtypeResult::True
                    } else {
                        SubtypeResult::False
                    }
                }
            };

            if !result.is_true() {
                return result;
            }
        }

        SubtypeResult::True
    }

    fn check_object_subtype_merge_scan(
        &mut self,
        source: &ObjectShape,
        target: &ObjectShape,
        source_receiver: Option<TypeId>,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let s_props = &source.properties;
        let t_props = &target.properties;

        let mut s_idx = 0;
        for t_prop in t_props {
            if t_prop.visibility != Visibility::Public {
                continue;
            }

            while s_idx < s_props.len() && s_props[s_idx].name < t_prop.name {
                s_idx += 1;
            }

            if s_idx < s_props.len() && s_props[s_idx].name == t_prop.name {
                let result = self.check_property_compatibility(
                    &s_props[s_idx],
                    t_prop,
                    source_receiver,
                    target_receiver,
                );
                if !result.is_true() {
                    return result;
                }
                s_idx += 1;
                continue;
            }

            // Property missing - only OK if target property is optional and public
            if t_prop.visibility != Visibility::Public {
                return SubtypeResult::False;
            }
            if !t_prop.optional {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check if a source property is compatible with a target property.
    ///
    /// This validates property compatibility for structural object subtyping:
    ///
    /// ## Rules:
    /// 1. **Optional compatibility**: Optional in source can't satisfy required in target
    ///    - `{ x?: number }` ≤ `{ x: number }` ❌
    ///    - `{ x: number }` ≤ `{ x?: number }` ✅
    ///
    /// 2. **Readonly compatibility**: TypeScript allows readonly source to satisfy mutable target
    ///    - `{ readonly x: number }` ≤ `{ x: number }` ✅ (readonly is on the reference)
    ///    - `{ x: number }` ≤ `{ readonly x: number }` ✅
    ///
    /// 3. **Type compatibility**: Source type must be subtype of target type
    ///    - Methods use bivariant checking (both directions)
    ///    - Properties use contravariant checking
    ///
    /// 4. **Write type compatibility**: For mutable properties with different write types,
    ///    target's write type must be subtype of source's (contravariance for writes)
    ///
    /// Check property compatibility between source and target properties.
    ///
    /// This implements **Rule #26: Split Accessors (Getter/Setter Variance)**.
    ///
    /// ## Split Accessor Variance
    ///
    /// Properties can have different types for reading (getter) vs writing (setter):
    /// ```typescript
    /// class C {
    ///   private _x: string | number;
    ///   get x(): string { return this._x as string; }
    ///   set x(v: string | number) { this._x = v; }
    /// }
    /// ```
    ///
    /// In this example, reading `x` yields `string`, but writing accepts `string | number`.
    ///
    /// ## Subtyping Rules
    ///
    /// For `source_prop <: target_prop`:
    ///
    /// 1. **Read types are COVARIANT**: `source.read <: target.read`
    ///    - When reading from source, we get something that's safe to use as target's read type
    ///
    /// 2. **Write types are CONTRAVARIANT**: `target.write <: source.write`
    ///    - When writing to target, we accept something that's also safe for source
    ///    - This ensures source can accept everything target can write
    ///
    /// 3. **Readonly properties**: If target property is readonly, we only check read types
    ///    - You can't write to a readonly target, so write type doesn't matter
    ///
    /// ## Example
    ///
    /// ```typescript
    /// class Base {
    ///   get x(): string { return "hello"; }
    ///   set x(v: string | number) {}
    /// }
    ///
    /// class Derived extends Base {
    ///   get x(): string { return "world"; }  // OK: string <: string
    ///   set x(v: string) {}  // OK: string <: string | number (contravariant)
    /// }
    /// ```
    ///
    /// ## Additional Checks
    ///
    /// - Optional properties: source optional can't satisfy target required
    /// - Readonly properties: source readonly can't satisfy target mutable
    pub(crate) fn check_property_compatibility(
        &mut self,
        source: &PropertyInfo,
        target: &PropertyInfo,
        source_receiver: Option<TypeId>,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        // Rule: Private and Protected properties are nominal.
        if target.visibility != Visibility::Public {
            if source.parent_id != target.parent_id {
                // Trace: Property nominal mismatch
                if let Some(tracer) = &mut self.tracer
                    && !tracer.on_mismatch_dyn(
                        crate::diagnostics::SubtypeFailureReason::PropertyNominalMismatch {
                            property_name: source.name,
                        },
                    )
                {
                    return SubtypeResult::False;
                }
                return SubtypeResult::False;
            }
        } else if source.visibility != Visibility::Public {
            // Cannot assign private/protected source to public target
            // Trace: Property visibility mismatch
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(
                    crate::diagnostics::SubtypeFailureReason::PropertyVisibilityMismatch {
                        property_name: source.name,
                        source_visibility: source.visibility,
                        target_visibility: target.visibility,
                    },
                )
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Check optional compatibility
        // Optional in source can't satisfy required in target
        if source.optional && !target.optional {
            // Trace: Optional property cannot satisfy required property
            if let Some(tracer) = &mut self.tracer
                && !tracer.on_mismatch_dyn(
                    crate::diagnostics::SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: source.name,
                    },
                )
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::False;
        }

        // Note: TypeScript does NOT reject readonly source → mutable target for
        // individual properties. `{ readonly x: number }` IS assignable to `{ x: number }`.
        // Readonly on properties is a usage constraint, not a structural typing constraint.
        // This is different from ReadonlyArray vs Array, where structural differences exist.

        // Rule #26: Split Accessors (Getter/Setter Variance)
        //
        // Properties with split accessors (get/set) have different types for reading vs writing:
        // - Read type (getter): covariant - source.read must be subtype of target.read
        // - Write type (setter): contravariant - target.write must be subtype of source.write
        //
        // For readonly properties in target, we only check read type (no writes allowed)
        // For mutable properties, we check both read and write types

        // 1. Check READ type (covariant): source.read <: target.read
        let source_read =
            self.bind_property_receiver_this(source_receiver, self.optional_property_type(source));
        let target_read =
            self.bind_property_receiver_this(target_receiver, self.optional_property_type(target));
        let allow_bivariant = source.is_method || target.is_method;

        // Mark that we're inside a property comparison so nested weak type checks
        // apply to recursive structural comparisons of property types.
        let prev_in_property_check = self.in_property_check;
        self.in_property_check = true;
        let result = self.check_property_types(
            source,
            target,
            source_receiver,
            target_receiver,
            source_read,
            target_read,
            allow_bivariant,
        );
        self.in_property_check = prev_in_property_check;
        result
    }

    /// Inner property type comparison with `in_property_check` already set.
    /// Separated to ensure `in_property_check` is always restored via the caller.
    fn check_property_types(
        &mut self,
        source: &PropertyInfo,
        target: &PropertyInfo,
        source_receiver: Option<TypeId>,
        target_receiver: Option<TypeId>,
        source_read: TypeId,
        target_read: TypeId,
        allow_bivariant: bool,
    ) -> SubtypeResult {
        if self.has_compatible_symbol_iterator_methods(source, target, source_read, target_read) {
            return SubtypeResult::True;
        }

        // Rule #26: Split Accessors - Covariant reads
        // Source read type must be subtype of target read type
        if source_read != target_read
            && !self
                .check_subtype_with_method_variance(source_read, target_read, allow_bivariant)
                .is_true()
        {
            return SubtypeResult::False;
        }

        // Rule #26: Split Accessors - Contravariant writes
        // For mutable target properties WITH DIFFERENT READ/WRITE TYPES, check write type compatibility
        // Target write type must be subtype of source write type (contravariance)
        //
        // IMPORTANT: This contravariant check only applies to "split accessors" where the
        // property has different types for reading vs writing (e.g., getter returns `string`,
        // setter accepts `string | number`). For regular properties where read and write types
        // are the same, TypeScript uses covariant checking for both.
        //
        // Without this condition, we would incorrectly reject valid assignments like:
        // - { a: string } <: { a?: string } (required to optional)
        // - { x: undefined } <: { x?: number } (undefined to optional)
        // TypeScript treats readonly as a usage constraint, not a structural one:
        // `{ readonly x: T }` IS assignable to `{ x: T }`. When the source
        // property is readonly, its write_type is irrelevant (it may be NONE or
        // a sentinel value), so skip the write check entirely.
        // TypeId::NONE is also a sentinel for "no distinct write type" (used by
        // readonly properties from the lowering pass). Treat NONE as "same as
        // type_id" so it doesn't falsely trigger split-accessor detection.
        let has_split_accessor = if source.readonly {
            false
        } else {
            (source.write_type != TypeId::NONE && source.write_type != source.type_id)
                || (target.write_type != TypeId::NONE && target.write_type != target.type_id)
        };

        if !target.readonly && has_split_accessor {
            let source_write = self.bind_property_receiver_this(
                source_receiver,
                self.optional_property_write_type(source),
            );
            let target_write = self.bind_property_receiver_this(
                target_receiver,
                self.optional_property_write_type(target),
            );

            // Contravariant writes: target.write must be subtype of source.write
            // This ensures that anything we can write to target is also safe to write to source
            if target_write != source_write
                && !self
                    .check_subtype_with_method_variance(target_write, source_write, allow_bivariant)
                    .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    fn receiver_type_from_shape_symbol(&self, shape: &ObjectShape) -> Option<TypeId> {
        let sym_id = shape.symbol?;
        let symbol_ref = crate::SymbolRef(sym_id.0);
        Some(
            self.resolver
                .symbol_to_def_id(symbol_ref)
                .map(|def_id| self.interner.lazy(def_id))
                .unwrap_or_else(|| self.interner.reference(symbol_ref)),
        )
    }

    fn bind_property_receiver_this(&self, receiver: Option<TypeId>, type_id: TypeId) -> TypeId {
        if let Some(receiver) = receiver.map(|receiver| self.normalize_receiver_type(receiver))
            && crate::contains_this_type(self.interner, type_id)
        {
            crate::substitute_this_type(self.interner, type_id, receiver)
        } else {
            type_id
        }
    }

    fn normalize_receiver_type(&self, receiver: TypeId) -> TypeId {
        match self.interner.lookup(receiver) {
            Some(crate::types::TypeData::Object(shape_id))
            | Some(crate::types::TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if let Some(sym_id) = shape.symbol {
                    let symbol_ref = crate::SymbolRef(sym_id.0);
                    return self
                        .resolver
                        .symbol_to_def_id(symbol_ref)
                        .map(|def_id| self.interner.lazy(def_id))
                        .unwrap_or_else(|| self.interner.reference(symbol_ref));
                }
                receiver
            }
            _ => receiver,
        }
    }

    /// Check string index signature compatibility between source and target.
    ///
    /// Validates that string index signatures are compatible, handling:
    /// - **Both have string index**: Source index must be subtype of target index
    /// - **Only target has string index**: All source properties must be compatible with target's index
    /// - **Only source has string index**: Compatible (target accepts string access via index)
    /// - **Neither has string index**: Compatible (no string index constraint)
    ///
    /// ## Readonly Constraints:
    /// - If target index is readonly, source index can be readonly or mutable
    /// - If target index is mutable, source index must be mutable (readonly source not compatible)
    pub(crate) fn check_string_index_compatibility(
        &mut self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let Some(ref t_string_idx) = target.string_index else {
            return SubtypeResult::True; // Target has no string index constraint
        };

        match &source.string_index {
            Some(s_string_idx) => {
                // Note: tsc does NOT enforce readonly on index signatures during
                // assignability. A readonly source index IS assignable to a writable
                // target index — readonly only prevents mutation through the reference.
                let source_value =
                    self.bind_property_receiver_this(source_receiver, s_string_idx.value_type);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, t_string_idx.value_type);
                if !self.check_subtype(source_value, target_value).is_true() {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None => {
                // Target has string index, source doesn't have a string index.
                // Check if source has a number index — in TypeScript, a numeric index
                // signature implies a string index (JS converts numeric keys to strings).
                // So `{ [n: number]: T }` is assignable to `{ [s: string]: T }` when
                // the value types are compatible.
                if let Some(s_number_idx) = &source.number_index {
                    // Note: We intentionally do NOT enforce readonly here. When a
                    // source type has a readonly number index (e.g., enum reverse
                    // mappings like `typeof E1`), it should still satisfy a writable
                    // string index target. The readonly constraint is about mutability,
                    // not value type compatibility. tsc allows `typeof E1` (with
                    // readonly number index for reverse mappings) to be assigned to
                    // `{ [x: string]: T }` (writable string index) when the value
                    // types are compatible.
                    let source_value =
                        self.bind_property_receiver_this(source_receiver, s_number_idx.value_type);
                    let target_value =
                        self.bind_property_receiver_this(target_receiver, t_string_idx.value_type);
                    if !self.check_subtype(source_value, target_value).is_true() {
                        return SubtypeResult::False;
                    }
                    // Don't return here — fall through to also check named properties
                    // against the target string index (implicit index signature path).
                }

                // An empty source vacuously satisfies the string index constraint.
                // tsc: `{} -> { [s: string]: T }` is assignable.
                if source.properties.is_empty() {
                    return SubtypeResult::True;
                }

                for prop in &source.properties {
                    // Note: We do NOT check property readonly vs target index readonly
                    // here. A source with readonly properties (e.g., enum namespaces)
                    // IS assignable to a target with a writable index signature. The
                    // readonly constraint means the property can't be written through
                    // the source, but assignability only checks value types. tsc
                    // allows `{ readonly A: E1 } <: { [k: string]: E1 }`.
                    //
                    // The inverse (writable source property vs readonly target index)
                    // is checked elsewhere via index signature compatibility.
                    //
                    // Strip `undefined` from optional property types when checking against
                    // index signatures. In tsc, `{ a: string, b?: number }` is assignable to
                    // `{ [s: string]: string | number }` because `b?` contributes `number`,
                    // not `number | undefined`.
                    let raw_prop_type = if prop.optional {
                        crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
                    } else {
                        prop.type_id
                    };
                    let prop_type =
                        self.bind_property_receiver_this(source_receiver, raw_prop_type);
                    let target_value =
                        self.bind_property_receiver_this(target_receiver, t_string_idx.value_type);
                    if !self.check_subtype(prop_type, target_value).is_true() {
                        return SubtypeResult::False;
                    }
                }
                SubtypeResult::True
            }
        }
    }

    /// Check number index signature compatibility between source and target objects.
    ///
    /// Validates that number index signatures (`[key: number]: T`) are compatible
    /// when checking if source is a subtype of target.
    ///
    /// ## TypeScript Soundness:
    /// - **Both have number index**: Source index must be subtype of target index
    /// - **Only target has number index**: Source must provide a compatible number/string index
    /// - **Only source has number index**: Compatible (target accepts numeric access via index)
    /// - **Neither has number index**: Source must have compatible numeric property names
    ///   (for index-less source objects assigned to indexed targets)
    pub(crate) fn check_number_index_compatibility(
        &mut self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let Some(ref t_number_idx) = target.number_index else {
            return SubtypeResult::True; // Target has no number index constraint
        };

        match &source.number_index {
            Some(s_number_idx) => {
                // Note: tsc does NOT enforce readonly on index signatures during
                // assignability. Readonly source index IS assignable to writable target.
                let source_value =
                    self.bind_property_receiver_this(source_receiver, s_number_idx.value_type);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, t_number_idx.value_type);
                if !self.check_subtype(source_value, target_value).is_true() {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None if source.string_index.is_some() => {
                // A compatible string index can satisfy numeric index access.
                let Some(s_string_idx) = source.string_index.as_ref() else {
                    return SubtypeResult::False;
                };
                // Note: tsc does NOT enforce readonly on index signatures during
                // assignability. Readonly source index IS assignable to writable target.
                let source_value =
                    self.bind_property_receiver_this(source_receiver, s_string_idx.value_type);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, t_number_idx.value_type);
                if !self.check_subtype(source_value, target_value).is_true() {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None => {
                // TypeScript only synthesizes an implicit numeric index signature
                // for anonymous object types and enum namespaces. Named class/interface
                // instance types must declare a real number/string index signature.
                // Check if source is a named type that ISN'T an enum namespace.
                if source.symbol.is_some() && !source.flags.contains(ObjectFlags::ENUM_NAMESPACE) {
                    return SubtypeResult::False;
                }

                // A truly empty anonymous source vacuously satisfies the numeric
                // index signature. tsc accepts `{}`-like object literal types here.
                if source.properties.is_empty() {
                    return SubtypeResult::True;
                }

                // Check any numeric-keyed source properties against the target's
                // number index type. If a numeric property has an incompatible type,
                // the assignment fails.
                //
                // Implicit Index Signature Rule:
                // If the source has no index signature, it is considered to have one
                // implicitly IF AND ONLY IF it has properties that match the index key type.
                // If there are NO numeric properties, the source does NOT satisfy the
                // numeric index signature requirement.
                let mut found_numeric_prop = false;
                for prop in &source.properties {
                    if !utils::is_numeric_property_name(self.interner, prop.name) {
                        continue;
                    }
                    found_numeric_prop = true;

                    // Note: tsc does NOT reject readonly properties against writable
                    // number index targets during assignability checks.
                    // Strip undefined from optional property types (same as string index).
                    let raw_prop_type = if prop.optional {
                        crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
                    } else {
                        prop.type_id
                    };
                    let prop_type =
                        self.bind_property_receiver_this(source_receiver, raw_prop_type);
                    let target_value =
                        self.bind_property_receiver_this(target_receiver, t_number_idx.value_type);
                    if !self
                        .check_subtype_with_method_variance(prop_type, target_value, prop.is_method)
                        .is_true()
                    {
                        return SubtypeResult::False;
                    }
                }

                if found_numeric_prop {
                    SubtypeResult::True
                } else {
                    // TypeScript treats number index signatures as constraining only
                    // numerically named members for anonymous object types. If the
                    // source has no numeric members, the numeric index constraint is
                    // vacuously satisfied.
                    SubtypeResult::True
                }
            }
        }
    }

    /// Check object with index signature subtyping.
    ///
    /// Validates subtype compatibility between two objects that both have index signatures.
    /// This requires:
    /// 1. Named property compatibility (all target properties must exist in source)
    /// 2. String index signature compatibility
    /// 3. Number index signature compatibility
    /// 4. All source properties must be compatible with target index signatures
    /// 5. If source has both string and number indexes, they must be compatible
    pub(crate) fn check_object_with_index_subtype(
        &mut self,
        source: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let source_receiver = self
            .receiver_type_from_shape_symbol(source)
            .or(source_receiver);
        let target_receiver = self
            .receiver_type_from_shape_symbol(target)
            .or(target_receiver);
        // First check named properties (nominal + structural)
        // Note: We pass the full shapes to enable nominal inheritance check
        if !self
            .check_object_subtype(
                source,
                source_shape_id,
                source_receiver,
                target,
                target_receiver,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check string index signature compatibility
        if !self
            .check_string_index_compatibility(source, source_receiver, target, target_receiver)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check number index signature compatibility
        if !self
            .check_number_index_compatibility(source, source_receiver, target, target_receiver)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self
            .check_properties_against_index_signatures(
                &source.properties,
                source_receiver,
                target,
                target_receiver,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }

        // For declared source types, if source has both string and number indexes,
        // the number index value type must be compatible with the string index value
        // type. Fresh object literals can transiently infer different string/number
        // index value unions during generic contextual typing, and tsc does not reject
        // assignment on that basis when the target index type already accepts both.
        if let (Some(s_string_idx), Some(s_number_idx)) =
            (&source.string_index, &source.number_index)
            && !source
                .flags
                .contains(crate::types::ObjectFlags::FRESH_LITERAL)
            && !self
                .check_subtype(
                    self.bind_property_receiver_this(source_receiver, s_number_idx.value_type),
                    self.bind_property_receiver_this(source_receiver, s_string_idx.value_type),
                )
                .is_true()
        {
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    /// Check object with index signature to plain object subtyping.
    ///
    /// Validates that a source object with an index signature can be a subtype of
    /// a target object with only named properties. For each target property:
    /// 1. Look up the property by name in source (including via index signatures)
    /// 2. Check property compatibility (optional, readonly, type, `write_type`)
    /// 3. If property not found in source, check if index signature can satisfy it
    pub(crate) fn check_object_with_index_to_object(
        &mut self,
        source: &ObjectShape,
        source_shape_id: ObjectShapeId,
        source_receiver: Option<TypeId>,
        target: &[PropertyInfo],
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let source_receiver = self
            .receiver_type_from_shape_symbol(source)
            .or(source_receiver);
        for t_prop in target {
            if let Some(sp) =
                self.lookup_property(&source.properties, Some(source_shape_id), t_prop.name)
            {
                // Visibility check (Nominal)
                if t_prop.visibility != Visibility::Public {
                    if sp.parent_id != t_prop.parent_id {
                        return SubtypeResult::False;
                    }
                } else if sp.visibility != Visibility::Public {
                    // Cannot assign private/protected source to public target
                    return SubtypeResult::False;
                }

                // Check optional compatibility
                if sp.optional && !t_prop.optional {
                    return SubtypeResult::False;
                }
                // NOTE: TypeScript allows readonly source to satisfy mutable target
                // (readonly is a constraint on the reference, not structural compatibility)
                let source_type = self
                    .bind_property_receiver_this(source_receiver, self.optional_property_type(sp));
                let target_type = self.bind_property_receiver_this(
                    target_receiver,
                    self.optional_property_type(t_prop),
                );
                let allow_bivariant = sp.is_method || t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                if !t_prop.readonly
                    && (sp.write_type != TypeId::NONE && sp.write_type != sp.type_id
                        || t_prop.write_type != TypeId::NONE && t_prop.write_type != t_prop.type_id)
                {
                    let source_write = self.bind_property_receiver_this(
                        source_receiver,
                        self.optional_property_write_type(sp),
                    );
                    let target_write = self.bind_property_receiver_this(
                        target_receiver,
                        self.optional_property_write_type(t_prop),
                    );
                    if !self
                        .check_subtype_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        return SubtypeResult::False;
                    }
                }
            } else if !self
                .check_missing_property_against_index_signatures(
                    source,
                    source_receiver,
                    t_prop,
                    target_receiver,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check if a missing target property can be satisfied by source index signatures.
    ///
    /// When a target property doesn't exist in the source object, the source's index
    /// signatures can potentially satisfy it:
    /// - If property name is numeric, check against number index signature
    /// - Always check against string index signature (since numbers convert to strings)
    pub(crate) fn check_missing_property_against_index_signatures(
        &mut self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        target_prop: &PropertyInfo,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        // Required properties cannot be satisfied by index signatures (soundness).
        // An index signature { [k: string]: V } admits the empty object {},
        // but a required property means the target does NOT admit {}.
        // This is a fundamental set-theoretic mismatch, not just a TS compatibility rule.
        if !target_prop.optional {
            return SubtypeResult::False;
        }

        // Private/Protected properties cannot be satisfied by index signatures.
        // They must be explicitly present in the source and match nominally.
        if target_prop.visibility != Visibility::Public {
            return SubtypeResult::False;
        }

        // Check if property name matches index signatures
        // Numeric property names can match number index signatures
        // All property names can match string index signatures (numbers convert to strings)
        let target_type = self
            .bind_property_receiver_this(target_receiver, self.optional_property_type(target_prop));

        if utils::is_numeric_property_name(self.interner, target_prop.name)
            && let Some(number_idx) = &source.number_index
        {
            if number_idx.readonly && !target_prop.readonly {
                return SubtypeResult::False;
            }
            let source_value =
                self.bind_property_receiver_this(source_receiver, number_idx.value_type);
            if !self
                .check_subtype_with_method_variance(
                    source_value,
                    target_type,
                    target_prop.is_method,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::True;
        }

        if let Some(string_idx) = &source.string_index {
            if string_idx.readonly && !target_prop.readonly {
                return SubtypeResult::False;
            }
            let source_value =
                self.bind_property_receiver_this(source_receiver, string_idx.value_type);
            if !self
                .check_subtype_with_method_variance(
                    source_value,
                    target_type,
                    target_prop.is_method,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
            return SubtypeResult::True;
        }

        // No matching index signature
        // For optional properties, this is OK
        // For required properties, this is an error
        if !target_prop.optional {
            SubtypeResult::False
        } else {
            SubtypeResult::True
        }
    }

    /// Check that source properties are compatible with target index signatures.
    ///
    /// When a target has an index signature, all source properties must satisfy it:
    /// - String index: All string-named properties must be compatible with index type
    /// - Number index: All numerically-named properties must be compatible with index type
    pub(crate) fn check_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return SubtypeResult::True;
        }

        for prop in source {
            // If target declares this property explicitly, its compatibility is
            // checked via named-property rules. Don't also force it through the
            // index signature value type (tsc behavior for intersections like
            // `{ a: X } & { [k: string]: Y }` where `a` is validated against `X`).
            if target
                .properties
                .binary_search_by_key(&prop.name, |p| p.name)
                .is_ok()
            {
                continue;
            }

            // Strip `undefined` from optional property types when checking against
            // index signatures. In tsc, optional properties are compatible with index
            // signatures that don't include `undefined`.
            let raw_prop_type = if prop.optional {
                crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
            } else {
                prop.type_id
            };
            let prop_type = self.bind_property_receiver_this(source_receiver, raw_prop_type);
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, number_idx.value_type);
                if is_numeric
                    && !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            target_value,
                            allow_bivariant,
                        )
                        .is_true()
                {
                    return SubtypeResult::False;
                }
                // Note: tsc does NOT reject readonly properties against writable
                // number index targets during assignability checks.
            }

            if let Some(string_idx) = string_index {
                // Note: We do NOT reject readonly source properties against writable
                // string index targets. A source with readonly properties (e.g., enum
                // namespaces, frozen objects) IS assignable to a target with a writable
                // index signature — the readonly constraint means the property can't be
                // written through the source reference, but assignability only checks
                // value type compatibility. tsc allows this pattern.
                let target_value =
                    self.bind_property_receiver_this(target_receiver, string_idx.value_type);
                if !self
                    .check_subtype_with_method_variance(prop_type, target_value, allow_bivariant)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }
        }

        SubtypeResult::True
    }

    /// Check simple object to object with index signature.
    ///
    /// Validates that a source object with only named properties is a subtype of
    /// a target object with an index signature. This requires:
    /// 1. All target named properties must have compatible source properties
    /// 2. All source properties must be compatible with the index signature type
    pub(crate) fn check_object_to_indexed(
        &mut self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        // Preserve the original shape identity when available. Named interface/class
        // types follow different index-signature rules than anonymous object types,
        // and rebuilding them as anonymous shapes loses that distinction.
        let source_shape = source_shape_id
            .map(|id| self.interner.object_shape(id))
            .unwrap_or_else(|| {
                ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties: source.to_vec(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                }
                .into()
            });
        let source_receiver = self
            .receiver_type_from_shape_symbol(&source_shape)
            .or(source_receiver);
        let target_receiver = self
            .receiver_type_from_shape_symbol(target)
            .or(target_receiver);
        if !self
            .check_object_subtype(
                &source_shape,
                source_shape_id,
                source_receiver,
                target,
                target_receiver,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }

        // A target number index signature requires the source to provide
        // number-compatible indexing via a number or string index signature.
        // A plain object with only named properties cannot satisfy arbitrary
        // numeric index access.
        if !self
            .check_number_index_compatibility(
                &source_shape,
                source_receiver,
                target,
                target_receiver,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }
        self.check_properties_against_index_signatures(
            source,
            source_receiver,
            target,
            target_receiver,
        )
    }

    /// Get the effective type of an optional property for reading.
    ///
    /// Optional properties in TypeScript can be undefined even if their type doesn't
    /// explicitly include undefined. This function adds undefined to the type unless
    /// exactOptionalPropertyTypes is enabled.
    pub(crate) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional && !self.exact_optional_property_types {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    /// Get the effective write type of an optional property.
    /// Falls back to `type_id` when `write_type` is `NONE` (readonly sentinel).
    pub(crate) fn optional_property_write_type(&self, prop: &PropertyInfo) -> TypeId {
        let write = if prop.write_type == TypeId::NONE {
            prop.type_id
        } else {
            prop.write_type
        };
        if prop.optional && !self.exact_optional_property_types {
            self.interner.union2(write, TypeId::UNDEFINED)
        } else {
            write
        }
    }

    /// Check if an object shape is a "weak type": all properties are optional,
    /// there is at least one property, and there are no index signatures.
    /// Weak types trigger TS2559 when the source has no common properties.
    fn is_weak_type_shape(shape: &ObjectShape) -> bool {
        !shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.iter().all(|p| p.optional)
    }
}
