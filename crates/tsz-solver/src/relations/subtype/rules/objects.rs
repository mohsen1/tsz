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
use crate::types::{
    IntrinsicKind, ObjectFlags, ObjectShape, ObjectShapeId, PropertyInfo, SymbolRef, TypeId,
    Visibility,
};
use crate::utils;
use crate::visitor::{
    application_id, lazy_def_id, object_shape_id, object_with_index_shape_id, template_literal_id,
    union_list_id,
};
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

    fn shape_or_receiver_requires_declared_index_signature(
        &self,
        shape: &ObjectShape,
        receiver: Option<TypeId>,
    ) -> bool {
        let is_named_non_enum = |shape: &ObjectShape| {
            shape.symbol.is_some() && !shape.flags.contains(ObjectFlags::ENUM_NAMESPACE)
        };
        if is_named_non_enum(shape) {
            return true;
        }

        let Some(receiver) = receiver else {
            return false;
        };

        let receiver_shape = object_with_index_shape_id(self.interner, receiver).or_else(|| {
            let app_id = application_id(self.interner, receiver)?;
            let app = self.interner.type_application(app_id);
            object_with_index_shape_id(self.interner, app.base)
        });

        receiver_shape
            .map(|shape_id| self.interner.object_shape(shape_id))
            .is_some_and(|shape| is_named_non_enum(&shape))
    }

    /// Like [`Self::requires_explicit_declared_index_signature`], but also
    /// recovers nominal identity from the `receiver`'s type->def registration.
    ///
    /// Resolving a `Lazy(DefId)` class/interface to its structural shape (during
    /// conditional `extends` evaluation, type-predicate narrowing, etc.)
    /// re-interns a shape whose `symbol` is `None`, even though the type stays
    /// registered to its `DefId` in the definition store — the same source
    /// diagnostics use to recover the interface/class *name*. Without this, such
    /// a resolved class/interface looks anonymous and is wrongly accepted as a
    /// subtype of a `{ [k: string]: T }` record (it would satisfy the index
    /// signature structurally), dropping the requirement that a named nominal
    /// type declare an explicit index signature.
    ///
    /// Only `Class`/`Interface` def-kinds trigger the requirement: anonymous
    /// object literals and object-typed `TypeAlias`es are not registered as
    /// nominal types (so they keep their implicit-index behaviour, matching
    /// tsc), and namespace value objects / enums resolve to other def-kinds and
    /// stay structurally compatible.
    pub(crate) fn requires_explicit_declared_index_signature_for(
        &self,
        shape: &ObjectShape,
        receiver: Option<TypeId>,
    ) -> bool {
        if self.requires_explicit_declared_index_signature(shape) {
            return true;
        }
        if receiver.is_some_and(|receiver| self.receiver_is_named_class_or_interface(receiver)) {
            return true;
        }

        // Fallback for a named shape whose nominal `DefId`/`DefKind` is not
        // mapped in this resolver. When a `Lazy(DefId)` class/interface is
        // resolved to its structural shape during conditional `extends`
        // evaluation or type-predicate narrowing, the shape keeps a `symbol`
        // but neither `symbol_to_def_id` nor the type->def index resolves it in
        // the relation's resolver view, so the two checks above can't confirm it
        // is nominal. A `symbol`-bearing, non-enum shape is nevertheless a named
        // declaration (interface/class/namespace/enum/…); only `Class`/
        // `Interface` must declare an explicit index signature to satisfy a
        // `{ [k: string]: T }` target. Treat such a shape as requiring one
        // unless the resolver positively classifies it as a structurally
        // index-compatible kind (namespace value object, enum, alias, …).
        //
        // Gated on a real resolver (`!is_noop`): with the `NoopResolver`
        // sentinel there is no nominal context at all, so a symbol-bearing shape
        // stays an ordinary structural object (preserving the no-resolver
        // contract relied on by callers that intentionally check structurally).
        if self.resolver.is_noop() {
            return false;
        }
        let Some(symbol) = shape.symbol else {
            return false;
        };
        if shape.flags.contains(ObjectFlags::ENUM_NAMESPACE) {
            return false;
        }
        match self
            .resolver
            .symbol_to_def_id(SymbolRef(symbol.0))
            .and_then(|def_id| self.resolver.get_def_kind(def_id))
        {
            // Positively non-nominal kinds keep their structural index
            // compatibility (a namespace's exported members / an enum / an
            // object-typed alias can satisfy the index signature directly).
            Some(
                crate::def::DefKind::Namespace
                | crate::def::DefKind::Enum
                | crate::def::DefKind::TypeAlias
                | crate::def::DefKind::Function
                | crate::def::DefKind::Variable,
            ) => false,
            // `Class`/`Interface` (or an unmapped named declaration, which in a
            // real resolver context is an interface/class whose symbol simply
            // isn't registered here) require an explicit index signature.
            _ => true,
        }
    }

    /// True when `receiver` (or, for a generic reference, its application base)
    /// resolves through the definition store to a `Class` or `Interface` `DefId`.
    /// Used to apply the explicit-index-signature requirement to named nominal
    /// types whose resolved structural shape no longer carries a `symbol`.
    fn receiver_is_named_class_or_interface(&self, receiver: TypeId) -> bool {
        let is_class_or_interface = |candidate: TypeId| {
            self.resolver.def_for_type(candidate).is_some_and(|def_id| {
                matches!(
                    self.resolver.get_def_kind(def_id),
                    Some(crate::def::DefKind::Class | crate::def::DefKind::Interface)
                )
            })
        };
        if is_class_or_interface(receiver) {
            return true;
        }
        // For a generic reference `C<…>`, fall back to the application base `C`.
        application_id(self.interner, receiver)
            .map(|app_id| self.interner.type_application(app_id).base)
            .is_some_and(|base| base != receiver && is_class_or_interface(base))
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

    /// Look up a property in the global Object interface (Object.prototype).
    ///
    /// TypeScript treats all object interface types as implicitly having Object.prototype
    /// methods. When a structural check finds a required property absent from the source,
    /// this fallback allows the check to pass if the global Object type provides a
    /// compatible property.
    fn get_object_base_property(&mut self, name: Atom) -> Option<PropertyInfo> {
        let object_type = self.resolver.get_boxed_type(IntrinsicKind::Object)?;
        let object_type = self.evaluate_type(object_type);
        let shape_id = object_shape_id(self.interner, object_type)?;
        let shape = self.interner.object_shape(shape_id);
        self.lookup_property(&shape.properties, Some(shape_id), name)
            .cloned()
    }

    /// Decide whether a target property that is *absent* from the source's named
    /// properties is nevertheless satisfied.
    ///
    /// This is the single rule shared by every object-subtype path (plain
    /// objects, the merge-scan fast path, and objects carrying index
    /// signatures). The source's index signatures play **no** role here:
    /// `tsc`'s `getPropertyOfType` never synthesizes a named member from an
    /// index signature, so `getUnmatchedProperty` reports a required named
    /// member as missing (`TS2741`/`TS2739`/`TS2740`) even when the source
    /// declares a `[k: string]: …` / `[n: number]: …` signature, and an
    /// *optional* named member imposes no constraint at all. A source index
    /// signature participates only in index-to-index relations
    /// (`check_string_index_compatibility` / `check_number_index_compatibility`).
    ///
    /// An absent target member is therefore satisfied iff it is:
    /// - optional and public (no constraint); or
    /// - required and public but supplied — compatibly — by the implicit
    ///   `Object.prototype` members every object type carries.
    ///
    /// A private/protected member can never be satisfied while absent, so any
    /// non-public member returns `False`.
    fn check_absent_target_property(
        &mut self,
        target_prop: &PropertyInfo,
        source_receiver: Option<TypeId>,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        // Private/Protected properties cannot be satisfied while absent.
        if target_prop.visibility != Visibility::Public {
            return SubtypeResult::False;
        }
        if target_prop.optional {
            return SubtypeResult::True;
        }
        // `Object.prototype` fallback: TypeScript treats every object type as
        // implicitly carrying the global `Object` members.
        if let Some(obj_prop) = self.get_object_base_property(target_prop.name) {
            self.check_property_compatibility(
                &obj_prop,
                target_prop,
                source_receiver,
                target_receiver,
            )
        } else {
            SubtypeResult::False
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
        // Prefer the caller-provided receiver (which preserves type arguments,
        // e.g., Runtype<any>) over the shape-derived DefId reference (which loses
        // them, e.g., bare Runtype). This ensures `this` type substitution in
        // properties like `constraint: Constraint<this>` produces the correct
        // parameterized type (e.g., Constraint<Runtype<any>>).
        let source_receiver =
            source_receiver.or_else(|| self.receiver_type_from_shape_symbol(source));
        let target_receiver =
            target_receiver.or_else(|| self.receiver_type_from_shape_symbol(target));
        let target_def = self.class_relation_target_def(target_receiver, Some(target));
        if self.class_instance_extends_target_def(source, source_receiver, target_def) {
            return SubtypeResult::True;
        }
        // Private brand checking for nominal typing of classes with private fields
        if !self.check_private_brand_compatibility(&source.properties, &target.properties) {
            return SubtypeResult::False;
        }

        // Weak type check (TS2559): if the target is a "weak type" (all properties optional,
        // at least one property, no index signatures), reject if the source has properties
        // but none in common with the target. Propagated from CompatChecker via
        // `enforce_weak_types`. tsc skips when the source is ALSO a weak type.
        //
        // When checking direct intersection members (`in_intersection_member_check`),
        // suppress this check: the source may have no common properties with one
        // weak-type member but still be assignable to the combined intersection
        // (e.g., ITreeItem <: ITreeItem & { Id?: number }).
        //
        // However, when we're inside a nested property type comparison
        // (`in_property_check`), the weak type check must still apply:
        //   { x: { c: string } } <: { x: { a?: string } }
        // The inner `{ c: string } <: { a?: string }` must fail because `{ a?: string }`
        // is a weak type and `{ c: string }` has no common properties with it.
        //
        // The structural trigger (non-empty non-weak source vs weak-type target
        // with no common property names) is evaluated independently of the
        // enforcement flags. Whenever it holds, the relation result is
        // weak-enforcement-sensitive: a checker with weak checks active returns
        // `False` here while one with weak checks suppressed continues and may
        // return `True`. That enforcement state is operation-local and is NOT
        // part of the `RelationCacheKey`, so we record the sensitivity to keep
        // such results out of the shared relation cache (see
        // `note_weak_type_sensitivity`), preventing one enforcement state from
        // poisoning another via a stale cache entry.
        if !source.properties.is_empty()
            && Self::is_weak_type_shape(target)
            && !Self::is_weak_type_shape(source)
            && !self.is_global_object_shape(source)
            && !crate::utils::has_common_property_name(&source.properties, &target.properties)
        {
            crate::relations::subtype::cache::note_weak_type_sensitivity();
            // The weak-type rejection is suppressed at the direct level when the
            // shapes are the property part of an intersection member or of a
            // callable-to-callable relation (a callable target is never weak in
            // tsc's `isWeakType`). A nested property-value comparison sets
            // `in_property_check`, which re-enables the rule so genuine weak inner
            // objects still fail.
            let suppressed_at_direct_level =
                self.in_intersection_member_check || self.in_callable_property_check;
            if self.enforce_weak_types && (!suppressed_at_direct_level || self.in_property_check) {
                return SubtypeResult::False;
            }
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
                None => self.check_absent_target_property(t_prop, source_receiver, target_receiver),
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

            // Property missing - resolved by the shared absent-member rule
            // (optional, or provided by Object.prototype; source index
            // signatures are irrelevant to target named members).
            let result =
                self.check_absent_target_property(t_prop, source_receiver, target_receiver);
            if !result.is_true() {
                return result;
            }
        }

        SubtypeResult::True
    }

    /// Decide whether a source property satisfies a *nominal* (private or
    /// protected) target property, given the member name, the two
    /// declaring-class symbols (`parent_id`s), and the target member's
    /// visibility.
    ///
    /// - `private` (modifier) requires *declaration identity*: the source
    ///   property must originate from the exact same declaration.
    /// - `protected` is *hierarchical*: tsc accepts the member when the source
    ///   property is declared in the target's protected-declaring class or in a
    ///   class derived from it (`isPropertyInClassDerivedFrom`). The source may
    ///   also legally widen the member from `protected` to `public`, so its own
    ///   visibility is not consulted.
    /// - ES private identifiers (`#name`) are *hierarchical* too: tsc keys each
    ///   `#name` slot per declaring class (escaped `__#<id>@name`), so a derived
    ///   class redeclaring the same `#name` still carries the base class's slot
    ///   as a separate member and remains assignable to the base. tsz's merged
    ///   object shapes key properties by surface name, which lets the derived
    ///   redeclaration shadow the inherited brand; the declaring-class
    ///   derivation check restores the tsc verdict (the inherited slot is
    ///   always present on a derived class).
    ///
    /// Falls back to declaration identity when class symbols or the inheritance
    /// graph are unavailable, preserving the previous strict nominal behavior
    /// for shapes that carry no hierarchy information. This is the single source
    /// of truth shared by the structural property check and the `CompatChecker`
    /// nominal-brand override.
    pub(crate) fn nominal_member_origin_ok(
        &self,
        member_name: tsz_common::interner::Atom,
        source_parent: Option<tsz_binder::SymbolId>,
        target_parent: Option<tsz_binder::SymbolId>,
        target_visibility: Visibility,
    ) -> bool {
        // Same declaration satisfies both private and protected (covers the
        // inherited-without-override case, where both sides resolve to the
        // original declaring class).
        if source_parent == target_parent {
            return true;
        }
        // `private` (modifier) requires strict declaration identity, which just
        // failed — unless the member is an ES private identifier (`#name`),
        // which is a per-class slot that a derived class always inherits.
        let is_es_private_identifier = crate::utils::is_es_private_identifier_name(
            self.interner.resolve_atom_ref(member_name).as_ref(),
        );
        if target_visibility != Visibility::Protected && !is_es_private_identifier {
            return false;
        }
        // `protected` / `#name`: the source's declaring class must derive from
        // (or equal) the target's declaring class.
        match (source_parent, target_parent, self.inheritance_graph) {
            (Some(src_class), Some(tgt_class), Some(graph)) => {
                graph.is_derived_from(src_class, tgt_class)
            }
            _ => false,
        }
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
        // Rule: Private and Protected properties are nominal, but with different
        // strictness — `private` demands declaration identity, while `protected`
        // is hierarchical (a derived class may widen it to `public`). Both are
        // decided by the shared `nominal_member_origin_ok`.
        if target.visibility != Visibility::Public {
            if !self.nominal_member_origin_ok(
                target.name,
                source.parent_id,
                target.parent_id,
                target.visibility,
            ) {
                return SubtypeResult::False;
            }
        } else if source.visibility != Visibility::Public {
            // Cannot assign private/protected source to public target
            return SubtypeResult::False;
        }

        // Note: optional source vs required target is also a failure, but it is
        // resolved *after* the read-type check below so the captured failure reason
        // matches tsc. When the read types are themselves incompatible, tsc reports
        // the type-incompatibility chain (root mismatch), not the TS2327
        // optional/required line; emitting TS2327 here would hide that root.

        // Note: TypeScript does NOT reject readonly source → mutable target for
        // individual properties. `{ readonly x: number }` IS assignable to `{ x: number }`.
        // Readonly on properties is a usage constraint, not a structural typing constraint.
        // This is different from ReadonlyArray vs Array, where structural differences exist.
        //
        // Exception: when comparing types for IDENTITY (not just assignability),
        // readonly difference IS observable. This is what makes the higher-order
        // `IfEquals` pattern work — it relies on `{ readonly x: T }` and
        // `{ x: T }` being distinct types when used as the extends-clause of a
        // conditional inside `(<T>() => T extends X ? 1 : 2)`. The
        // `strict_readonly_identity` flag is toggled on by the conditional
        // extends-type equivalence helper for exactly that purpose.
        if self.strict_readonly_identity && source.readonly != target.readonly {
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
        let source_read =
            self.bind_property_receiver_this(source_receiver, self.optional_property_type(source));
        let target_read =
            self.bind_property_receiver_this(target_receiver, self.optional_property_type(target));
        let allow_bivariant = target.is_method;

        // Mark that we're inside a property comparison so nested weak type checks
        // apply to recursive structural comparisons of property types.
        let prev_in_property_check = self.in_property_check;
        self.in_property_check = true;
        let generic_args_start = self.instantiated_generic_method_args.len();
        if allow_bivariant {
            self.extend_instantiated_generic_method_args(source_receiver);
            self.extend_instantiated_generic_method_args(target_receiver);
        }
        let result = self.check_property_types(
            source,
            target,
            source_receiver,
            target_receiver,
            source_read,
            target_read,
            allow_bivariant,
        );
        self.instantiated_generic_method_args
            .truncate(generic_args_start);
        self.in_property_check = prev_in_property_check;

        // Optional source vs required target. This is only the *reported* failure
        // once the read types are known compatible: an optional source property
        // (present-but-maybe-absent) cannot satisfy a required target. If the read
        // types already failed, that type-incompatibility reason takes precedence
        // (matching tsc), so return it unchanged instead of overwriting it.
        if result.is_true() && source.optional && !target.optional {
            return SubtypeResult::False;
        }
        result
    }

    fn extend_instantiated_generic_method_args(&mut self, receiver: Option<TypeId>) {
        let Some(receiver) = receiver else {
            return;
        };
        let application = match self.interner.lookup(receiver) {
            Some(crate::types::TypeData::Application(app_id)) => Some(app_id),
            _ => self.interner.get_display_alias(receiver).and_then(|alias| {
                match self.interner.lookup(alias) {
                    Some(crate::types::TypeData::Application(app_id)) => Some(app_id),
                    _ => None,
                }
            }),
        };

        if let Some(app_id) = application {
            let app = self.interner.type_application(app_id);
            self.instantiated_generic_method_args
                .extend(app.args.iter().copied());
        }
    }

    fn class_instance_extends_target_def(
        &self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        target_def: Option<crate::def::DefId>,
    ) -> bool {
        let Some(source_def) = source_receiver
            .and_then(|type_id| self.resolver.class_def_for_instance_type(type_id))
            .or_else(|| {
                source
                    .symbol
                    .and_then(|symbol| self.resolver.symbol_to_def_id(SymbolRef(symbol.0)))
            })
        else {
            return false;
        };
        let Some(target_def) = target_def else {
            return false;
        };
        self.def_nominally_extends_target_def(source_def, target_def)
    }

    /// O(1) check for `Intersection <: ObjectLikeTarget`: does a single
    /// intersection MEMBER's checker-verified class/interface heritage chain
    /// reach `target_def`? This is sound as an unconditional early accept
    /// regardless of the intersection's other members: subtyping is
    /// transitive, so `Member <: Target` implies `(Member & Other) <: Target`
    /// for any `Other`, whatever `Other` contributes. Lets a source like
    /// `Window & { extra: number }` skip `Window`'s full DOM-lib structural
    /// walk the same way a plain `interface W extends Window {}` source
    /// already does via `class_instance_extends_target_def` (#16089).
    ///
    /// Unlike `class_instance_extends_target_def`, `member` is a bare type
    /// reference (as stored in the intersection's member list), not a
    /// receiver/`this` instance type, so it resolves through the same chain
    /// `class_relation_target_def` uses for the target side rather than
    /// through `class_def_for_instance_type` alone.
    pub(crate) fn intersection_member_nominally_extends_target(
        &self,
        member: TypeId,
        target_receiver: TypeId,
        target_shape: Option<&ObjectShape>,
    ) -> bool {
        // Prefer a bare `Lazy(DefId)` receiver derived straight from the
        // evaluated shape's symbol over the raw `target_receiver` as stored:
        // a plain interface *reference* like `Window` used as a type
        // annotation is commonly interned as an `Application(def, args)`
        // even with an empty `args` (this is exactly what `check_object_subtype`
        // sidesteps by resolving its own `target_receiver` through
        // `receiver_type_from_shape_symbol` before ever calling
        // `class_relation_target_def`). `class_relation_target_def` bails
        // outright on an `Application` receiver, so passing the raw form
        // through unconditionally would silently defeat this fast path for
        // exactly the DOM-lib targets it exists for.
        let target_receiver = target_shape
            .and_then(|shape| self.receiver_type_from_shape_symbol(shape))
            .unwrap_or(target_receiver);
        let Some(target_def) = self.class_relation_target_def(Some(target_receiver), target_shape)
        else {
            return false;
        };
        let Some(member_def) = self.def_id_for_type_reference(member) else {
            return false;
        };
        self.def_nominally_extends_target_def(member_def, target_def)
    }

    /// Shared heritage-chain walk behind both `class_instance_extends_target_def`
    /// and `intersection_member_nominally_extends_target`, once each has
    /// resolved its own `source_def`/`target_def`.
    fn def_nominally_extends_target_def(
        &self,
        source_def: crate::def::DefId,
        target_def: crate::def::DefId,
    ) -> bool {
        // #16137 widened this to interfaces; #16142 found the widening unsound
        // (an interface's heritage edge was trusted even when TS2430 rejected
        // it) and #16148 reverted to classes-only as a stopgap. This restores
        // the widening on the durable fix: `verified_interface_extends` below
        // is populated only when the checker's own TS2430 check passed, so an
        // interface source is now exactly as trustworthy as a class source.
        let source_kind = self.resolver.get_def_kind(source_def);
        if !matches!(
            source_kind,
            Some(crate::def::DefKind::Class | crate::def::DefKind::Interface)
        ) {
            return false;
        }
        if self.def_has_type_params(target_def) {
            return false;
        }
        if !self.resolver.is_actual_or_cloned_lib_def(target_def) {
            return false;
        }

        if self.resolver.defs_are_equivalent(source_def, target_def) {
            return true;
        }

        let mut current = source_def;
        for _ in 0..50 {
            // Classes use the checker-verified, generics-aware `class_extends`
            // map. Interfaces use `verified_interface_extends`, a single-parent
            // edge the checker registers only when
            // `check_interface_extension_compatibility` found no TS2430
            // ("incorrectly extends") for this declaration — trusting the raw
            // name-resolved heritage edge instead is unsound, since tsc's own
            // override check can reject a declared `extends` (#16142). Both
            // maps miss on a multi-parent `interface B extends A, C {}` (only
            // the first parent is tracked); a miss just returns `false` here,
            // which re-runs the always-correct structural walk in the caller.
            let parent = match self.resolver.get_def_kind(current) {
                Some(crate::def::DefKind::Class) => self.resolver.get_class_extends(current),
                Some(crate::def::DefKind::Interface) => {
                    self.resolver.get_interface_extends(current)
                }
                _ => None,
            };
            let Some(parent) = parent else {
                return false;
            };
            if self.resolver.defs_are_equivalent(parent, target_def) {
                return true;
            }
            current = parent;
        }
        false
    }

    fn def_has_type_params(&self, def_id: crate::def::DefId) -> bool {
        self.resolver
            .get_lazy_type_params(def_id)
            .is_some_and(|params| !params.is_empty())
    }

    fn class_relation_target_def(
        &self,
        target_receiver: Option<TypeId>,
        target: Option<&ObjectShape>,
    ) -> Option<crate::def::DefId> {
        if target_receiver.is_some_and(|type_id| self.receiver_is_application(type_id)) {
            return None;
        }

        target_receiver
            .and_then(|type_id| self.def_id_for_type_reference(type_id))
            .or_else(|| {
                target.and_then(|shape| {
                    shape
                        .symbol
                        .and_then(|symbol| self.resolver.symbol_to_def_id(SymbolRef(symbol.0)))
                })
            })
    }

    /// Resolve a `DefId` for a type used as a bare type reference (e.g. an
    /// intersection member, or the type checked against as a relation
    /// target) rather than a receiver/`this` instance type. Extracted from
    /// `class_relation_target_def` so `intersection_member_nominally_extends_target`
    /// can apply the same resolution to intersection members.
    fn def_id_for_type_reference(&self, type_id: TypeId) -> Option<crate::def::DefId> {
        lazy_def_id(self.interner, type_id)
            .or_else(|| {
                // A plain interface/class reference is commonly interned as
                // `Application(def, args)` even when `args` is empty. The
                // args are irrelevant to whether the *base* def's own
                // (arg-independent) heritage chain reaches a target def with
                // no type parameters of its own — `def_nominally_extends_target_def`
                // already requires that of the target — so unwrapping to the
                // base's `DefId` here is sound for the nominal-heritage walk
                // regardless of what the args are.
                application_id(self.interner, type_id).and_then(|app_id| {
                    lazy_def_id(self.interner, self.interner.type_application(app_id).base)
                })
            })
            .or_else(|| self.resolver.def_for_type(type_id))
            .or_else(|| self.resolver.class_def_for_instance_type(type_id))
            .or_else(|| self.def_for_receiver_shape_symbol(type_id))
    }

    fn receiver_is_application(&self, type_id: TypeId) -> bool {
        application_id(self.interner, type_id).is_some()
            || self
                .interner
                .get_display_alias(type_id)
                .is_some_and(|alias| application_id(self.interner, alias).is_some())
    }

    fn def_for_receiver_shape_symbol(&self, type_id: TypeId) -> Option<crate::def::DefId> {
        object_shape_id(self.interner, type_id)
            .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            .or_else(|| {
                object_with_index_shape_id(self.interner, type_id)
                    .and_then(|shape_id| self.interner.object_shape(shape_id).symbol)
            })
            .and_then(|symbol| self.resolver.symbol_to_def_id(SymbolRef(symbol.0)))
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

        // Rule #26 (Sound Mode only): Split Accessors - Contravariant writes
        // For mutable target properties WITH DIFFERENT READ/WRITE TYPES, check write type compatibility
        // Target write type must be subtype of source write type (contravariance)
        //
        // PARITY: tsc relates object properties through their *read* types only.
        // Setter/write types never participate in assignability, subtype, or
        // conditional-`extends` relations (TS 4.3 divergent accessors); writes
        // are validated at the actual write site instead. The contravariant
        // write check below is therefore gated behind
        // `check_split_accessor_writes`, which only Sound Mode enables.
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
        let has_split_accessor = self.check_split_accessor_writes
            && !source.readonly
            && (source.has_split_accessor() || target.has_split_accessor());

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
        // Only nominalize when the resolver can produce a real DefId.
        // Falling back to `interner.reference(symbol_ref)` here would conflate
        // `SymbolId.0` with `DefId.0` (independent ID spaces) and yield a
        // Lazy(DefId) that points at an unrelated declaration.
        self.resolver
            .symbol_to_def_id(symbol_ref)
            .map(|def_id| self.interner.lazy(def_id))
    }

    pub(in crate::relations::subtype) fn bind_property_receiver_this(
        &self,
        receiver: Option<TypeId>,
        type_id: TypeId,
    ) -> TypeId {
        if let Some(receiver) = receiver.map(|receiver| self.normalize_receiver_type(receiver))
            && crate::contains_this_type(self.interner, type_id)
        {
            crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                type_id,
                receiver,
            )
        } else {
            type_id
        }
    }

    fn normalize_receiver_type(&self, receiver: TypeId) -> TypeId {
        if receiver.is_intrinsic() {
            return receiver;
        }
        match self.interner.lookup(receiver) {
            Some(crate::types::TypeData::Object(shape_id))
            | Some(crate::types::TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if let Some(sym_id) = shape.symbol {
                    let symbol_ref = crate::SymbolRef(sym_id.0);
                    // Only nominalize when the resolver can produce a real DefId.
                    // Falling back to `interner.reference(symbol_ref)` would conflate
                    // `SymbolId.0` with `DefId.0` and produce a Lazy pointing at an
                    // unrelated declaration. When no DefId mapping exists, keep the
                    // original Object receiver — the structural shape is still a
                    // sound `this` substitution target.
                    if let Some(def_id) = self.resolver.symbol_to_def_id(symbol_ref) {
                        return self.interner.lazy(def_id);
                    }
                }
                receiver
            }
            _ => receiver,
        }
    }

    pub(crate) fn requires_explicit_declared_index_signature(&self, shape: &ObjectShape) -> bool {
        if shape.flags.contains(ObjectFlags::ENUM_NAMESPACE) {
            return false;
        }

        let Some(sym_id) = shape.symbol else {
            return false;
        };

        matches!(
            self.symbol_def_kind(crate::SymbolRef(sym_id.0)),
            Some(crate::def::DefKind::Class | crate::def::DefKind::Interface)
        )
    }

    /// Whether a target string index whose value type is `any` waives the
    /// missing string-index requirement for a source that declares none. This is
    /// `tsc`'s `indexSignaturesRelatedTo` short-circuit: a TS legacy
    /// `any`-propagation quirk, disabled alongside method bivariance in Sound
    /// Mode. A concrete value type such as `unknown` is never waived. Shared by
    /// the object-source path below and the named array/tuple-source path in
    /// `core_dispatch`.
    pub(crate) fn target_string_index_any_waives_missing_index(&self, value_type: TypeId) -> bool {
        !self.disable_method_bivariance && value_type.is_any()
    }

    /// Whether a source index signature keyed by `source_key` is applicable to a
    /// target index whose key is `target_key` — i.e. the source provides a value
    /// for every key the target index ranges over. Mirrors `tsc`'s
    /// `isApplicableIndexType(targetKey, sourceKey)` (used by `getApplicableIndexInfo`
    /// inside `typeRelatedToIndexInfo`): the target key set must be assignable to the
    /// source key set. Identical keys — the common plain-`string` index — short-circuit
    /// without a relation query so ordinary index signatures are unaffected.
    pub(crate) fn index_signature_key_covers(
        &mut self,
        source_key: TypeId,
        target_key: TypeId,
    ) -> bool {
        source_key == target_key || self.check_subtype(target_key, source_key).is_true()
    }

    /// Whether an `any`-valued number index signature on `target` imposes no
    /// requirement on a source that declares no numeric index, because `target`
    /// also carries an `any`-valued (non-symbol) string index. The numeric
    /// mirror of [`Self::target_string_index_any_waives_missing_index`]: `tsc`'s
    /// `indexSignaturesRelatedTo` short-circuit accepts a named class/interface
    /// against `{ [k: string]: any; [k: number]: any }` (e.g. `Record<any, any>`,
    /// `{ [P in any]: any }`) even though `{ [k: number]: any }` alone rejects it.
    ///
    /// Both index values must be exactly `any` — a narrower number value
    /// (`{ ...; [k: number]: string }`) or string value
    /// (`{ [k: string]: string; [k: number]: any }`) falls back to the
    /// per-property numeric check. An eagerly-merged intersection
    /// (`ObjectFlags::INTERSECTION_MERGED`, e.g. `StringTo<any> & NumberTo<any>`)
    /// is excluded: `tsc` relates each intersection member separately, so the
    /// numeric member alone still demands a numeric index. A TS legacy
    /// `any`-propagation quirk, disabled with method bivariance in Sound Mode.
    pub(crate) fn target_dual_any_index_waives_missing_number_index(
        &self,
        target: &ObjectShape,
    ) -> bool {
        !self.disable_method_bivariance
            && target
                .number_index
                .as_ref()
                .is_some_and(|idx| idx.value_type.is_any())
            && target
                .string_index
                .as_ref()
                .is_some_and(|idx| idx.key_type != TypeId::SYMBOL && idx.value_type.is_any())
            && !target.flags.contains(ObjectFlags::INTERSECTION_MERGED)
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
        let Some(t_string_idx) = target.string_index_signature() else {
            return SubtypeResult::True; // Target has no string index constraint
        };

        // tsc: `indexSignaturesRelatedTo` short-circuit (checker.ts ~24828):
        //   when the target has a string index AND the target's index info value
        //   maps to `any`, the source need not declare a matching index signature.
        //   This is the rule that allows `{ [n: number]: any }` -> `{ [s: string]: any }`
        //   even when the source is a named class/interface. We mirror it here for
        //   the assignability path (non-strict subtype).
        if self.target_string_index_any_waives_missing_index(t_string_idx.value_type) {
            return SubtypeResult::True;
        }

        // tsc `typeRelatedToIndexInfo`/`getApplicableIndexInfo`: a source string-like
        // index only satisfies the target's index when its key *covers* the target
        // key (every key the target index ranges over is assignable to the source
        // index key). A source index keyed by an unrelated branded/template/union
        // string type — e.g. `[k: TaggedString2]` against a target `[k: TaggedString1]`
        // — is NOT applicable and must fall through to the missing-index handling
        // below exactly as if the source declared no string index at all (which for a
        // named interface/class is an error, and for an inferable-index object type is
        // checked structurally). Identical keys (the common plain-`string` index) are
        // trivially applicable, so this is a no-op for ordinary index signatures.
        let target_key = t_string_idx.key_type;
        match source.string_index_signature() {
            Some(s_string_idx)
                if self.index_signature_key_covers(s_string_idx.key_type, target_key) =>
            {
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
            // No source string index at all, or one whose key does not cover the
            // target key (handled identically to a missing index per tsc).
            _ => {
                // An *optional* target string index (`[k: string]?: V`, e.g.
                // `Partial<Record<string, V>>`) imposes no requirement on a
                // property-less source. tsc accepts `object`, `{}`, an empty
                // interface, or an `object`-constrained generic `T` against it,
                // even though a *required* `Record<string, V>` rejects them all.
                // (A source WITH properties is still subject to the inferable-
                // index / explicit-index rules below — `interface Foo { x }` is
                // rejected, a `{ x }` type literal checks its members.)
                if target.string_index_is_optional() && source.properties.is_empty() {
                    return SubtypeResult::True;
                }

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

                // Class and interface instance types must declare an explicit
                // **string** index signature to satisfy a target that requires
                // one.  Having only a number index is NOT sufficient — a number
                // index constrains only numeric keys but says nothing about
                // arbitrary string keys.  tsc reports TS2345 with the message
                // "Index signature for type 'string' is missing in type …"
                // when e.g. `NumberMap<Function>` (only `[n: number]: Function`)
                // is passed where `StringMap<T>` (requires `[s: string]: T`)
                // is expected.
                //
                // Namespace-like value objects and anonymous types can still
                // satisfy the target structurally through their exported members.
                if self.requires_explicit_declared_index_signature_for(source, source_receiver) {
                    return SubtypeResult::False;
                }

                // An empty source vacuously satisfies the string index constraint.
                // tsc: `{} -> { [s: string]: T }` is assignable.
                if source.properties.is_empty() {
                    return SubtypeResult::True;
                }

                for prop in &source.properties {
                    if prop.is_symbol_named {
                        continue;
                    }
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

        // tsc: `indexSignaturesRelatedTo` short-circuit (checker.ts ~24828) — an
        // `any`-valued number index is waived by a co-present `any`-valued string
        // index on the same single object type (`Record<any, any>` accepts a
        // named source). See the helper for the full rule and intersection guard.
        if self.target_dual_any_index_waives_missing_number_index(target) {
            return SubtypeResult::True;
        }

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
            None if source
                .string_index
                .as_ref()
                .is_some_and(|idx| idx.key_type != TypeId::SYMBOL) =>
            {
                // A compatible string index can satisfy numeric index access.
                let Some(s_string_idx) = source
                    .string_index
                    .as_ref()
                    .filter(|idx| idx.key_type != TypeId::SYMBOL)
                else {
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
                // An *optional* target number index (`[k: number]?: V`, e.g.
                // `Partial<Record<number, V>>`) imposes no requirement on a
                // property-less source — the numeric mirror of the optional
                // string-index relaxation above.
                if target.number_index_is_optional() && source.properties.is_empty() {
                    return SubtypeResult::True;
                }

                // TypeScript only synthesizes an implicit numeric index signature
                // for anonymous object types and enum namespaces. Named class/interface
                // instance types must declare a real number/string index signature.
                // Check if source is a named type that ISN'T an enum namespace.
                if self.shape_or_receiver_requires_declared_index_signature(source, source_receiver)
                {
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
                    // For NUMBER index signatures, optional properties carry an implicit
                    // `| undefined` that must flow into the check (tsc: `{ 1?: string }`
                    // vs `{ [k: number]: string }` fails on `string | undefined <: string`).
                    let raw_prop_type = self.optional_property_type(prop);
                    let prop_type =
                        self.bind_property_receiver_this(source_receiver, raw_prop_type);
                    let target_value =
                        self.bind_property_receiver_this(target_receiver, t_number_idx.value_type);
                    if !self
                        .check_subtype_with_method_variance(prop_type, target_value, false)
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
    /// 4. Symbol index signature compatibility
    /// 5. All source properties must be compatible with target index signatures
    /// 6. If source has both string and number indexes, they must be compatible
    pub(crate) fn check_object_with_index_subtype(
        &mut self,
        source: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        // Prefer the caller-provided receiver (which preserves type arguments,
        // e.g., Runtype<any>) over the shape-derived DefId reference (which loses
        // them, e.g., bare Runtype). This ensures `this` type substitution in
        // properties like `constraint: Constraint<this>` produces the correct
        // parameterized type (e.g., Constraint<Runtype<any>>).
        let source_receiver =
            source_receiver.or_else(|| self.receiver_type_from_shape_symbol(source));
        let target_receiver =
            target_receiver.or_else(|| self.receiver_type_from_shape_symbol(target));
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

        // Check symbol index signature compatibility
        if !self
            .check_symbol_index_compatibility(source, source_receiver, target, target_receiver)
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
        if let (Some(s_string_idx), Some(s_number_idx)) = (
            source
                .string_index
                .as_ref()
                .filter(|idx| idx.key_type != TypeId::SYMBOL),
            &source.number_index,
        ) && !source
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
        let source_receiver =
            source_receiver.or_else(|| self.receiver_type_from_shape_symbol(source));
        let target_def = self.class_relation_target_def(target_receiver, None);
        if self.class_instance_extends_target_def(source, source_receiver, target_def) {
            return SubtypeResult::True;
        }
        for t_prop in target {
            if let Some(sp) =
                self.lookup_property(&source.properties, Some(source_shape_id), t_prop.name)
            {
                // Visibility check (Nominal) — `private` needs declaration
                // identity, `protected` is hierarchical (shared helper).
                if t_prop.visibility != Visibility::Public {
                    if !self.nominal_member_origin_ok(
                        t_prop.name,
                        sp.parent_id,
                        t_prop.parent_id,
                        t_prop.visibility,
                    ) {
                        return SubtypeResult::False;
                    }
                } else if sp.visibility != Visibility::Public {
                    // Cannot assign private/protected source to public target
                    return SubtypeResult::False;
                }

                // Check optional compatibility (see check_property_compatibility for rationale)
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
                let allow_bivariant = t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    return SubtypeResult::False;
                }
                // Sound Mode only: tsc never relates split-accessor write
                // types (see check_property_types above for the parity rule).
                if self.check_split_accessor_writes
                    && !t_prop.readonly
                    && (sp.has_split_accessor() || t_prop.has_split_accessor())
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
            } else {
                // Property absent from the source's named members. The source's
                // index signatures never supply a target *named* member (they
                // only participate in index-to-index relations), so this is
                // resolved by the shared absent-member rule exactly as for a
                // plain object source.
                let result =
                    self.check_absent_target_property(t_prop, source_receiver, target_receiver);
                if !result.is_true() {
                    return result;
                }
            }
        }

        SubtypeResult::True
    }

    fn property_name_matches_string_index_key(&mut self, name: Atom, key_type: TypeId) -> bool {
        if key_type == TypeId::STRING {
            return true;
        }

        if let Some(template_id) = template_literal_id(self.interner, key_type) {
            return self
                .check_literal_matches_template_literal(name, template_id)
                .is_true();
        }

        if let Some(union_id) = union_list_id(self.interner, key_type) {
            let members = self.interner.type_list(union_id).to_vec();
            let mut saw_template_member = false;
            for member in members {
                if member == TypeId::STRING {
                    return true;
                }
                let Some(template_id) = template_literal_id(self.interner, member) else {
                    return true;
                };
                saw_template_member = true;
                if self
                    .check_literal_matches_template_literal(name, template_id)
                    .is_true()
                {
                    return true;
                }
            }
            return !saw_template_member;
        }

        true
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
        let string_index = target.string_index_signature();
        let symbol_index = target.symbol_index_signature();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() && symbol_index.is_none() {
            return SubtypeResult::True;
        }

        for prop in source {
            // Unlike `explain_properties_against_index_signatures` (which never
            // skips), this used to `continue` here when the target declares
            // `prop.name` as a named member, on the theory that named-property
            // rules already cover it. But `tsc`'s `membersRelatedToIndexInfo`
            // checks every source property against the index regardless of a
            // same-named target member — when the target's own named property
            // doesn't satisfy its own index signature (the TS2411 shape, e.g.
            // `{ [k: string]: number; flag: boolean }`), a source property that
            // matches `target.namedProp` structurally can still violate the
            // index. When `target.namedProp <: index` holds, this check passes
            // by transitivity through the already-validated named-property
            // relation, so removing the skip only tightens the TS2411 case.

            // For NUMBER index signatures, optional properties carry an implicit
            // `| undefined` that must flow into the check (e.g. `{ 1?: string }`
            // vs `{ [k: number]: string }` fails on `string | undefined <: string`).
            // For STRING index signatures, tsc strips the implicit `| undefined`
            // so `{ b?: number }` is assignable to `{ [k: string]: number }`.
            //
            // But when the property type is itself `undefined` (e.g.
            // `k1?: undefined`), stripping yields `never`, which is
            // vacuously assignable to anything and silences a real
            // mismatch. Use the original property type in that case so
            // the check still fires (tsc emits TS2322 for
            // `{ k1?: undefined }` against `{ [key: string]: string }`).
            let string_prop_type = if prop.optional {
                let stripped =
                    crate::narrowing::utils::remove_undefined(self.interner, prop.type_id);
                if stripped == TypeId::NEVER {
                    prop.type_id
                } else {
                    stripped
                }
            } else {
                prop.type_id
            };
            let string_prop_type =
                self.bind_property_receiver_this(source_receiver, string_prop_type);
            let number_prop_type = if prop.optional {
                self.bind_property_receiver_this(source_receiver, self.optional_property_type(prop))
            } else {
                string_prop_type
            };
            let allow_bivariant = false;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, number_idx.value_type);
                if is_numeric
                    && !self
                        .check_subtype_with_method_variance(
                            number_prop_type,
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
                if prop.is_symbol_named {
                    continue;
                }
                // Non-matching keys aren't constrained: `click` ∉ `on${string}`, so
                // `{ click: number }` is fine against `{ [k: on${string}]: () => void }`.
                if !self.property_name_matches_string_index_key(prop.name, string_idx.key_type) {
                    continue;
                }
                // Note: We do NOT reject readonly source properties against writable
                // string index targets. A source with readonly properties (e.g., enum
                // namespaces, frozen objects) IS assignable to a target with a writable
                // index signature — the readonly constraint means the property can't be
                // written through the source reference, but assignability only checks
                // value type compatibility. tsc allows this pattern.
                let target_value =
                    self.bind_property_receiver_this(target_receiver, string_idx.value_type);
                if !self
                    .check_subtype_with_method_variance(
                        string_prop_type,
                        target_value,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }

            if let Some(symbol_idx) = symbol_index
                && prop.is_symbol_named
            {
                let target_value =
                    self.bind_property_receiver_this(target_receiver, symbol_idx.value_type);
                if !self
                    .check_subtype_with_method_variance(
                        string_prop_type,
                        target_value,
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
                    symbol_index: None,
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

        // Named class/interface types require an explicit string index signature to
        // satisfy a string-indexed target — compatible properties alone are not enough.
        // Symbol-keyed indices and any-value targets are exempted (same shortcircuits
        // as check_string_index_compatibility). An *optional* target string index
        // imposes no requirement on a property-less source, so it is exempt too
        // (mirrors the relaxation in `check_string_index_compatibility`).
        let optional_index_satisfied_by_empty_source =
            target.string_index_is_optional() && source_shape.properties.is_empty();
        if !optional_index_satisfied_by_empty_source
            && target.string_index.as_ref().is_some_and(|idx| {
                idx.key_type != TypeId::SYMBOL
                    && (self.disable_method_bivariance || !idx.value_type.is_any())
            })
            && self.requires_explicit_declared_index_signature_for(&source_shape, source_receiver)
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
        if !self
            .check_symbol_index_compatibility(
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
        if prop.optional && !self.exact_optional_property_types && self.strict_null_checks {
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
        if prop.optional && !self.exact_optional_property_types && self.strict_null_checks {
            self.interner.union2(write, TypeId::UNDEFINED)
        } else {
            write
        }
    }

    /// Check if an object shape is a "weak type": all properties are optional,
    /// there is at least one property, and there are no index signatures.
    /// Weak types trigger TS2559 when the source has no common properties.
    pub(crate) fn is_weak_type_shape(shape: &ObjectShape) -> bool {
        !shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.iter().all(|p| p.optional)
    }

    /// Check if an object shape is the global `Object` interface from lib.d.ts.
    ///
    /// The global `Object` type is exempt from weak type checks because in tsc,
    /// all object types implicitly inherit `Object`'s properties (`toString`,
    /// `valueOf`, `constructor`, etc.). When tsc checks `hasCommonProperties`
    /// for the weak type rule, the target type's apparent type includes these
    /// inherited members, so `Object` and any weak type always share common
    /// properties. Our shapes don't include inherited members, so we exempt
    /// `Object` explicitly to match tsc behavior (see TypeScript PR #16047).
    fn is_global_object_shape(&self, shape: &ObjectShape) -> bool {
        // Delegates to the canonical shared structural matcher in
        // `type_queries::global_interfaces` (issue #13090). Identity cannot be
        // consulted here: only the bare `ObjectShape` is available, not the
        // source `TypeId`.
        crate::type_queries::object_shape_matches_global_object_interface(self.interner, shape)
    }

    /// `ObjectWithIndex` source vs `Tuple` target.
    ///
    /// Matches tsc's behavior for array-like interfaces assigned to a tuple
    /// type, e.g.
    /// ```ts
    /// interface StrNum extends Array<string|number> {
    ///   0: string;
    ///   1: number;
    ///   length: 2;
    /// }
    /// declare let x: [string, number];
    /// declare let y: StrNum;
    /// x = y;  // OK
    /// ```
    ///
    /// Iterates the target tuple's elements and looks up each by its numeric
    /// property name (`"0"`, `"1"`, ...) on the source shape. Optional/rest
    /// elements use the source's number index signature as a fallback.
    /// `length` is also checked when the tuple has a fixed arity and the
    /// source declares a numeric `length`.
    pub(crate) fn check_object_with_index_to_tuple(
        &mut self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        t_list: crate::types::TupleListId,
        target_type: TypeId,
    ) -> SubtypeResult {
        use crate::types::PropertyInfo;
        let target_elems = self.interner.tuple_list(t_list);
        let source_receiver =
            source_receiver.or_else(|| self.receiver_type_from_shape_symbol(source));

        for (i, t_elem) in target_elems.iter().enumerate() {
            // Variadic / rest elements aren't structurally implementable by
            // a fixed-property interface — bail out conservatively.
            if t_elem.rest {
                return SubtypeResult::False;
            }
            let prop_name = self.interner.intern_string(&i.to_string());
            let s_prop_opt = PropertyInfo::find_in_slice(&source.properties, prop_name);

            // Optional tuple slot can be satisfied by either a (matching) source
            // property OR by the source's number index signature.
            let s_type = if let Some(sp) = s_prop_opt {
                self.bind_property_receiver_this(source_receiver, self.optional_property_type(sp))
            } else if let Some(idx) = &source.number_index {
                self.bind_property_receiver_this(source_receiver, idx.value_type)
            } else if t_elem.optional {
                continue;
            } else {
                return SubtypeResult::False;
            };

            let t_type = t_elem.type_id;
            if !self.check_subtype(s_type, t_type).is_true() {
                return SubtypeResult::False;
            }
        }

        // Length check: when the target tuple has a fixed arity (no rest), the
        // source's `length` property type must be assignable to the literal
        // target length. tsc applies this strictly — `length: 2` is not
        // assignable to `length: 1`, and `length: number` is not assignable
        // to `length: 1` either.
        let length_atom = self.interner.intern_string("length");
        if let Some(s_length) = PropertyInfo::find_in_slice(&source.properties, length_atom)
            && target_elems.iter().all(|e| !e.rest)
        {
            let s_length_type = self.bind_property_receiver_this(source_receiver, s_length.type_id);
            let target_len = target_elems.len();
            let target_len_type = self.interner.literal_number(target_len as f64);
            if !self.check_subtype(s_length_type, target_len_type).is_true() {
                return SubtypeResult::False;
            }
        }

        let _ = target_type;
        SubtypeResult::True
    }
}

#[cfg(test)]
#[path = "../../../../tests/objects_interface_nominal_fastpath_tests.rs"]
mod objects_interface_nominal_fastpath_tests;

#[cfg(test)]
#[path = "../../../../tests/intersection_nominal_fastpath_tests.rs"]
mod intersection_nominal_fastpath_tests;
