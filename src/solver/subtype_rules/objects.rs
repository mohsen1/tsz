//! Object type subtype checking.
//!
//! This module handles subtyping for TypeScript's object types:
//! - Plain objects with named properties
//! - Objects with index signatures (string and number)
//! - Property compatibility (optional, readonly, type, write_type)
//! - **Rule #26**: Split Accessors (Getter/Setter Variance)
//!   - Read types are covariant: source.read <: target.read
//!   - Write types are contravariant: target.write <: source.write
//!   - Readonly target properties only check read type (no write access)
//! - Private brand checking for nominal class typing

use crate::interner::Atom;
use crate::solver::types::*;
use crate::solver::utils;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Look up a property in a list of properties, using cached index if available.
    pub(crate) fn lookup_property<'props>(
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

    /// Check private brand compatibility for object subtyping.
    ///
    /// Private brands are used for nominal typing of classes with private fields.
    /// If both source and target have private brands, they must be the same.
    /// Returns false if brands don't match, true otherwise (including when neither has a brand).
    pub(crate) fn check_private_brand_compatibility(
        &self,
        source: &[PropertyInfo],
        target: &[PropertyInfo],
    ) -> bool {
        let source_brand = source.iter().find(|p| {
            let name = self.interner.resolve_atom(p.name);
            name.starts_with("__private_brand_")
        });
        let target_brand = target.iter().find(|p| {
            let name = self.interner.resolve_atom(p.name);
            name.starts_with("__private_brand_")
        });

        // If both have private brands (both are classes with private fields), check they match
        match (source_brand, target_brand) {
            (Some(s_brand), Some(t_brand)) => {
                let s_brand_name = self.interner.resolve_atom(s_brand.name);
                let t_brand_name = self.interner.resolve_atom(t_brand.name);
                s_brand_name == t_brand_name
            }
            _ => true, // If at least one doesn't have a brand, no conflict
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
        target: &ObjectShape,
    ) -> SubtypeResult {
        // Fast path: Nominal inheritance check for class instances
        // If both source and target are class instances with symbols,
        // check if source is derived from target (O(1) lookup)
        if let (Some(source_sym), Some(target_sym)) = (source.symbol, target.symbol) {
            if let Some(graph) = self.inheritance_graph {
                if graph.is_derived_from(source_sym, target_sym) {
                    return SubtypeResult::True;
                }
            }
        }

        // Private brand checking for nominal typing of classes with private fields
        if !self.check_private_brand_compatibility(&source.properties, &target.properties) {
            return SubtypeResult::False;
        }

        // For each property in target, source must have a compatible property
        for t_prop in &target.properties {
            let s_prop = source.properties.iter().find(|p| p.name == t_prop.name);

            let result = match s_prop {
                Some(sp) => self.check_property_compatibility(sp, t_prop),
                None => {
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
    ) -> SubtypeResult {
        // Check optional compatibility
        // Optional in source can't satisfy required in target
        if source.optional && !target.optional {
            return SubtypeResult::False;
        }

        // Readonly compatibility: source readonly cannot satisfy target mutable
        // This is required for mapped type modifier tests (readonly_add/remove)
        if source.readonly && !target.readonly {
            return SubtypeResult::False;
        }

        // Rule #26: Split Accessors (Getter/Setter Variance)
        //
        // Properties with split accessors (get/set) have different types for reading vs writing:
        // - Read type (getter): covariant - source.read must be subtype of target.read
        // - Write type (setter): contravariant - target.write must be subtype of source.write
        //
        // For readonly properties in target, we only check read type (no writes allowed)
        // For mutable properties, we check both read and write types

        // 1. Check READ type (covariant): source.read <: target.read
        let source_read = self.optional_property_type(source);
        let target_read = self.optional_property_type(target);
        let allow_bivariant = source.is_method || target.is_method;

        // Rule #26: Split Accessors - Covariant reads
        // Source read type must be subtype of target read type
        if !self
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
        let has_split_accessor =
            source.write_type != source.type_id || target.write_type != target.type_id;

        if !target.readonly && has_split_accessor {
            let source_write = self.optional_property_write_type(source);
            let target_write = self.optional_property_write_type(target);

            // Contravariant writes: target.write must be subtype of source.write
            // This ensures that anything we can write to target is also safe to write to source
            if !self
                .check_subtype_with_method_variance(target_write, source_write, allow_bivariant)
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
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
        target: &ObjectShape,
    ) -> SubtypeResult {
        let Some(ref t_string_idx) = target.string_index else {
            return SubtypeResult::True; // Target has no string index constraint
        };

        match &source.string_index {
            Some(s_string_idx) => {
                // Source string index must be subtype of target
                if s_string_idx.readonly && !t_string_idx.readonly {
                    return SubtypeResult::False;
                }
                if !self
                    .check_subtype(s_string_idx.value_type, t_string_idx.value_type)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None => {
                // Target has string index, source doesn't
                // All source properties must be compatible with target's string index
                for prop in &source.properties {
                    if !t_string_idx.readonly && prop.readonly {
                        return SubtypeResult::False;
                    }
                    let prop_type = self.optional_property_type(prop);
                    if !self
                        .check_subtype(prop_type, t_string_idx.value_type)
                        .is_true()
                    {
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
    /// - **Only target has number index**: Source properties with numeric names must be compatible
    /// - **Only source has number index**: Compatible (target accepts numeric access via index)
    /// - **Neither has number index**: Compatible (no numeric index constraint)
    pub(crate) fn check_number_index_compatibility(
        &mut self,
        source: &ObjectShape,
        target: &ObjectShape,
    ) -> SubtypeResult {
        let Some(ref t_number_idx) = target.number_index else {
            return SubtypeResult::True; // Target has no number index constraint
        };

        match &source.number_index {
            Some(s_number_idx) => {
                // Source number index must be subtype of target
                if s_number_idx.readonly && !t_number_idx.readonly {
                    return SubtypeResult::False;
                }
                if !self
                    .check_subtype(s_number_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                SubtypeResult::True
            }
            None => {
                // Target has number index but source doesn't - this is OK
                // (number indexing is optional)
                SubtypeResult::True
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
        target: &ObjectShape,
    ) -> SubtypeResult {
        // First check named properties (nominal + structural)
        // Note: We pass the full shapes to enable nominal inheritance check
        if !self.check_object_subtype(source, target).is_true() {
            return SubtypeResult::False;
        }

        // Check string index signature compatibility
        if !self
            .check_string_index_compatibility(source, target)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Check number index signature compatibility
        if !self
            .check_number_index_compatibility(source, target)
            .is_true()
        {
            return SubtypeResult::False;
        }

        if !self
            .check_properties_against_index_signatures(&source.properties, target)
            .is_true()
        {
            return SubtypeResult::False;
        }

        // If source has string index, all number-indexed properties must be compatible
        // (since number converts to string for property access)
        if let (Some(s_string_idx), Some(s_number_idx)) =
            (&source.string_index, &source.number_index)
            && !self
                .check_subtype(s_number_idx.value_type, s_string_idx.value_type)
                .is_true()
        {
            // This is a constraint violation in the source itself
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    /// Check object with index signature to plain object subtyping.
    ///
    /// Validates that a source object with an index signature can be a subtype of
    /// a target object with only named properties. For each target property:
    /// 1. Look up the property by name in source (including via index signatures)
    /// 2. Check property compatibility (optional, readonly, type, write_type)
    /// 3. If property not found in source, check if index signature can satisfy it
    pub(crate) fn check_object_with_index_to_object(
        &mut self,
        source: &ObjectShape,
        source_shape_id: ObjectShapeId,
        target: &[PropertyInfo],
    ) -> SubtypeResult {
        for t_prop in target {
            if let Some(sp) =
                self.lookup_property(&source.properties, Some(source_shape_id), t_prop.name)
            {
                // Check optional compatibility
                if sp.optional && !t_prop.optional {
                    return SubtypeResult::False;
                }
                // NOTE: TypeScript allows readonly source to satisfy mutable target
                // (readonly is a constraint on the reference, not structural compatibility)
                let source_type = self.optional_property_type(sp);
                let target_type = self.optional_property_type(t_prop);
                let allow_bivariant = sp.is_method || t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                if !t_prop.readonly
                    && (sp.write_type != sp.type_id || t_prop.write_type != t_prop.type_id)
                {
                    let source_write = self.optional_property_write_type(sp);
                    let target_write = self.optional_property_write_type(t_prop);
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
                .check_missing_property_against_index_signatures(source, t_prop)
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
        target_prop: &PropertyInfo,
    ) -> SubtypeResult {
        let mut checked = false;
        let target_type = self.optional_property_type(target_prop);

        if utils::is_numeric_property_name(self.interner, target_prop.name)
            && let Some(number_idx) = &source.number_index
        {
            checked = true;
            if number_idx.readonly && !target_prop.readonly {
                return SubtypeResult::False;
            }
            if !self
                .check_subtype_with_method_variance(
                    number_idx.value_type,
                    target_type,
                    target_prop.is_method,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        if let Some(string_idx) = &source.string_index {
            checked = true;
            if string_idx.readonly && !target_prop.readonly {
                return SubtypeResult::False;
            }
            if !self
                .check_subtype_with_method_variance(
                    string_idx.value_type,
                    target_type,
                    target_prop.is_method,
                )
                .is_true()
            {
                return SubtypeResult::False;
            }
        }

        if checked || target_prop.optional {
            SubtypeResult::True
        } else {
            SubtypeResult::False
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
        target: &ObjectShape,
    ) -> SubtypeResult {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return SubtypeResult::True;
        }

        for prop in source {
            let prop_type = self.optional_property_type(prop);
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                if is_numeric
                    && !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            number_idx.value_type,
                            allow_bivariant,
                        )
                        .is_true()
                {
                    return SubtypeResult::False;
                }
                if is_numeric && !number_idx.readonly && prop.readonly {
                    return SubtypeResult::False;
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return SubtypeResult::False;
                }
                if !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        string_idx.value_type,
                        allow_bivariant,
                    )
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
        target: &ObjectShape,
    ) -> SubtypeResult {
        // First check named properties match
        // Create temporary ObjectShape for source to enable nominal check
        let source_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: source.to_vec(),
            string_index: None,
            number_index: None,
            symbol: None, // Source doesn't have a symbol (just properties)
        };
        if !self.check_object_subtype(&source_shape, target).is_true() {
            return SubtypeResult::False;
        }

        self.check_properties_against_index_signatures(source, target)
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
    pub(crate) fn optional_property_write_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional && !self.exact_optional_property_types {
            self.interner.union2(prop.write_type, TypeId::UNDEFINED)
        } else {
            prop.write_type
        }
    }
}
