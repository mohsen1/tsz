//! Property access resolution (`PropertyAccessEvaluator`) for resolving
//! property access on types (obj.prop, obj["key"], etc.).

use crate::caches::db::QueryDatabase;
use crate::relations::subtype::TypeResolver;
use crate::types::{IntrinsicKind, LiteralValue, ObjectShapeId, TypeData, TypeId};
use crate::{ApparentMemberKind, TypeDatabase, apparent_object_member_kind};
use std::cell::{Cell, RefCell};
use tsz_common::interner::Atom;

// Re-export readonly helpers
pub(crate) use super::property_readonly::property_is_readonly;
pub use super::property_readonly::{
    is_mapped_type_with_readonly_modifier, is_readonly_index_signature,
    is_readonly_tuple_fixed_element,
};

// Child module: resolution helpers (mapped types, primitives, arrays, applications, etc.)
#[path = "property_helpers.rs"]
mod property_helpers;

// =============================================================================
// Property Access Resolution
// =============================================================================

/// Result of attempting to access a property on a type.
#[derive(Clone, Copy, Debug)]
pub enum PropertyAccessResult {
    /// Property exists, returns its type
    Success {
        type_id: TypeId,
        /// The write type (setter parameter type) when different from read type.
        /// Used for assignment checking with divergent accessors (TS 4.3+).
        /// `None` means `write_type` == `type_id` (no divergence).
        write_type: Option<TypeId>,
        /// True if this property was resolved via an index signature
        /// (not an explicit property declaration). Used for error 4111.
        from_index_signature: bool,
    },

    /// Property does not exist on this type
    PropertyNotFound {
        type_id: TypeId,
        property_name: Atom,
    },

    /// Type is possibly null or undefined.
    /// Contains the type of the property from non-nullable members (if any),
    /// and the specific nullable type causing the error.
    PossiblyNullOrUndefined {
        /// Type from valid non-nullable members (for recovery/optional chaining)
        property_type: Option<TypeId>,
        /// The nullable type causing the issue: NULL, UNDEFINED, or union of both
        cause: TypeId,
    },

    /// Type is unknown
    IsUnknown,
}

impl PropertyAccessResult {
    /// Convenience constructor: successful access returning the given type.
    /// Shorthand for `Success { type_id, write_type: None, from_index_signature: false }`.
    #[inline]
    pub const fn simple(type_id: TypeId) -> Self {
        Self::Success {
            type_id,
            write_type: None,
            from_index_signature: false,
        }
    }

    /// Convenience constructor: successful access resolved via an index signature.
    /// Shorthand for `Success { type_id, write_type: None, from_index_signature: true }`.
    #[inline]
    pub const fn from_index(type_id: TypeId) -> Self {
        Self::Success {
            type_id,
            write_type: None,
            from_index_signature: true,
        }
    }

    /// Convenience constructor: successful access with divergent read/write types.
    /// Shorthand for `Success { type_id, write_type: Some(write), from_index_signature: false }`.
    #[inline]
    pub const fn with_write_type(type_id: TypeId, write_type: TypeId) -> Self {
        Self::Success {
            type_id,
            write_type: Some(write_type),
            from_index_signature: false,
        }
    }

    /// Returns true if this is a successful property access.
    #[inline]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Returns true if the property was not found.
    #[inline]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::PropertyNotFound { .. })
    }

    /// Returns true if the type is possibly null or undefined.
    #[inline]
    pub const fn is_possibly_null_or_undefined(&self) -> bool {
        matches!(self, Self::PossiblyNullOrUndefined { .. })
    }

    /// Returns true if the type is unknown.
    #[inline]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::IsUnknown)
    }

    /// Extracts the `type_id` from a Success result, or None otherwise.
    pub const fn success_type(&self) -> Option<TypeId> {
        match self {
            Self::Success { type_id, .. } => Some(*type_id),
            _ => None,
        }
    }

    /// Extracts both `type_id` and `from_index_signature` from a Success result.
    pub const fn success_info(&self) -> Option<(TypeId, bool)> {
        match self {
            Self::Success {
                type_id,
                from_index_signature,
                ..
            } => Some((*type_id, *from_index_signature)),
            _ => None,
        }
    }

    /// Extracts the `property_type` from a `PossiblyNullOrUndefined` result.
    pub const fn nullable_property_type(&self) -> Option<TypeId> {
        match self {
            Self::PossiblyNullOrUndefined { property_type, .. } => *property_type,
            _ => None,
        }
    }
}

/// Evaluates property access.
///
/// Uses `QueryDatabase` which provides both `TypeDatabase` and `TypeResolver` functionality,
/// enabling proper resolution of Lazy types and type aliases.
pub struct PropertyAccessEvaluator<'a> {
    pub(crate) db: &'a dyn QueryDatabase,
    resolver: Option<&'a dyn TypeResolver>,
    pub(crate) no_unchecked_indexed_access: bool,
    pub(crate) exact_optional_property_types: bool,
    /// Unified recursion guard for cycle detection and depth limiting.
    pub(crate) guard: RefCell<crate::recursion::RecursionGuard<TypeId>>,
    // Context for visitor pattern (set during property access resolution)
    // We store both the str (for immediate use) and Atom (for interned comparisons)
    pub(crate) current_prop_name: RefCell<Option<String>>,
    pub(crate) current_prop_atom: RefCell<Option<Atom>>,
    /// When true, `bind_object_receiver_this` is a no-op. Set when resolving
    /// properties through a type parameter's constraint so that `this` is
    /// preserved for the checker to substitute with the correct receiver type.
    skip_this_binding: Cell<bool>,
}

struct PropertyAccessGuard<'a> {
    evaluator: &'a PropertyAccessEvaluator<'a>,
    obj_type: TypeId,
}

impl<'a> Drop for PropertyAccessGuard<'a> {
    fn drop(&mut self) {
        self.evaluator.guard.borrow_mut().leave(self.obj_type);
    }
}

impl<'a> PropertyAccessEvaluator<'a> {
    pub fn new(db: &'a dyn QueryDatabase) -> Self {
        PropertyAccessEvaluator {
            db,
            resolver: None,
            no_unchecked_indexed_access: false,
            exact_optional_property_types:
                crate::caches::db::QueryDatabase::exact_optional_property_types(db),
            guard: RefCell::new(crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::PropertyAccess,
            )),
            current_prop_name: RefCell::new(None),
            current_prop_atom: RefCell::new(None),
            skip_this_binding: Cell::new(false),
        }
    }

    pub fn with_resolver(db: &'a dyn QueryDatabase, resolver: &'a dyn TypeResolver) -> Self {
        let mut evaluator = Self::new(db);
        evaluator.resolver = Some(resolver);
        evaluator
    }

    pub const fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    pub const fn set_exact_optional_property_types(&mut self, enabled: bool) {
        self.exact_optional_property_types = enabled;
    }

    /// Skip `this` binding during property resolution. When set, raw `ThisType`
    /// is preserved in the result so the caller can substitute it with the
    /// correct nominal receiver type.
    pub fn set_skip_this_binding(&self, skip: bool) {
        self.skip_this_binding.set(skip);
    }

    /// Returns the current state of the `skip_this_binding` flag.
    pub(crate) const fn is_skip_this_binding(&self) -> bool {
        self.skip_this_binding.get()
    }

    /// Helper to access the underlying `TypeDatabase`
    pub(crate) fn interner(&self) -> &dyn TypeDatabase {
        self.db.as_type_database()
    }

    fn resolver(&self) -> &dyn TypeResolver {
        self.resolver.unwrap_or_else(|| self.db.as_type_resolver())
    }

    pub(crate) fn bind_object_receiver_this(&self, receiver: TypeId, type_id: TypeId) -> TypeId {
        if self.skip_this_binding.get() {
            return type_id;
        }
        let new_receiver = self.nominalize_object_receiver(receiver);
        if crate::contains_this_type(self.interner(), type_id) {
            // Use the shallow variant: at property-access binding, we want to
            // substitute `this` references at structural positions but NOT
            // walk into stored nominal Object/Function/Callable internals.
            // Walking into Label's stored `extend` method here bakes
            // `this -> Label` into Label's stored bodies, poisoning later
            // intersection wrapping (chained `extend({a}).extend({b})`).
            crate::substitute_this_type_at_return_position(
                self.interner(),
                None,
                type_id,
                new_receiver,
            )
        } else {
            type_id
        }
    }

    fn nominalize_object_receiver(&self, receiver: TypeId) -> TypeId {
        // Fast path: intrinsics aren't `Object(_)` / `ObjectWithIndex(_)`;
        // the match falls through to `_ => receiver`.
        if receiver.is_intrinsic() {
            return receiver;
        }
        match self.interner().lookup(receiver) {
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                if let Some(sym_id) = shape.symbol {
                    let symbol_ref = crate::SymbolRef(sym_id.0);
                    // Only nominalize when the resolver can produce a real DefId.
                    // Falling back to `interner().reference(symbol_ref)` here would
                    // conflate `SymbolId.0` with `DefId.0` (independent ID spaces),
                    // producing a Lazy(DefId) that points at a *different* declaration.
                    // When no DefId mapping exists, keep the original object shape —
                    // structural substitution is still correct, and the type formatter
                    // can recover the interface name from `shape.symbol`.
                    if let Some(def_id) = self.resolver().symbol_to_def_id(symbol_ref) {
                        return self.interner().lazy(def_id);
                    }
                }
                receiver
            }
            _ => receiver,
        }
    }

    /// Try to resolve a member from the global `Object` type, returning
    /// `PropertyNotFound` if no such member exists.
    fn resolve_object_member_or_not_found(
        &self,
        obj_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        self.resolve_object_member(prop_name, prop_atom).unwrap_or(
            PropertyAccessResult::PropertyNotFound {
                type_id: obj_type,
                property_name: prop_atom,
            },
        )
    }

    pub(crate) fn is_deferred_any_fallback_member(&self, type_id: TypeId) -> bool {
        matches!(
            self.interner().lookup(type_id),
            Some(
                TypeData::IndexAccess(_, _)
                    | TypeData::Mapped(_)
                    | TypeData::Conditional(_)
                    | TypeData::TypeQuery(_)
            )
        )
    }
}

impl<'a> PropertyAccessEvaluator<'a> {
    /// Resolve property access: obj.prop -> type
    pub fn resolve_property_access(
        &self,
        obj_type: TypeId,
        prop_name: &str,
    ) -> PropertyAccessResult {
        self.resolve_property_access_inner(obj_type, prop_name, None)
    }

    fn enter_property_access_guard(&self, obj_type: TypeId) -> Option<PropertyAccessGuard<'_>> {
        use crate::recursion::RecursionResult;

        let mut guard = self.guard.borrow_mut();
        match guard.enter(obj_type) {
            RecursionResult::Entered => {}
            RecursionResult::Cycle
            | RecursionResult::DepthExceeded
            | RecursionResult::IterationExceeded => {
                return None;
            }
        }
        drop(guard);

        Some(PropertyAccessGuard {
            evaluator: self,
            obj_type,
        })
    }

    pub(crate) fn resolve_property_access_inner(
        &self,
        obj_type: TypeId,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> PropertyAccessResult {
        // Milestone 2: Visitor Bridge Pattern
        // Set context for visitor methods
        {
            let mut current_name = self.current_prop_name.borrow_mut();
            if let Some(name) = current_name.as_mut() {
                name.clear();
                name.push_str(prop_name);
            } else {
                *current_name = Some(prop_name.to_owned());
            }
        }
        *self.current_prop_atom.borrow_mut() = prop_atom;

        // Single-lookup dispatch: resolve property access based on type data.
        // All type variants are handled in one match to avoid redundant interner lookups.
        let Some(key) = self.interner().lookup(obj_type) else {
            let prop_atom = prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
            return PropertyAccessResult::PropertyNotFound {
                type_id: obj_type,
                property_name: prop_atom,
            };
        };

        match key {
            TypeData::Error => {
                // Error types propagate silently (like any) — property access
                // succeeds with ERROR to prevent cascading diagnostics.
                PropertyAccessResult::simple(TypeId::ERROR)
            }

            TypeData::Object(shape_id) => self
                .visit_object_impl(shape_id.0, prop_name, prop_atom)
                .unwrap_or_else(|| PropertyAccessResult::simple(TypeId::ANY)),

            TypeData::ObjectWithIndex(shape_id) => self
                .visit_object_with_index_impl(shape_id.0, prop_name, prop_atom)
                .unwrap_or_else(|| PropertyAccessResult::simple(TypeId::ANY)),

            TypeData::Array(_) | TypeData::Tuple(_) => self
                .visit_array_impl(obj_type, prop_name, prop_atom)
                .unwrap_or_else(|| PropertyAccessResult::simple(TypeId::ANY)),

            TypeData::Union(list_id) => self
                .visit_union_impl(list_id.0, prop_name, prop_atom)
                .unwrap_or_else(|| PropertyAccessResult::simple(TypeId::ANY)),

            TypeData::Intrinsic(kind) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                match kind {
                    IntrinsicKind::Any => PropertyAccessResult::simple(TypeId::ANY),
                    IntrinsicKind::Unknown => PropertyAccessResult::IsUnknown,
                    IntrinsicKind::Void => {
                        // In tsc, accessing a property on `void` produces TS2339
                        // ("Property 'X' does not exist on type 'void'"), NOT TS2532
                        // ("Object is possibly 'undefined'"). `void` is a distinct type
                        // from `undefined` for property access purposes.
                        PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type,
                            property_name: prop_atom,
                        }
                    }
                    IntrinsicKind::Null | IntrinsicKind::Undefined => {
                        let cause = if kind == IntrinsicKind::Undefined {
                            TypeId::UNDEFINED
                        } else {
                            TypeId::NULL
                        };
                        PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: None,
                            cause,
                        }
                    }
                    IntrinsicKind::Symbol => {
                        self.resolve_symbol_primitive_property(prop_name, prop_atom)
                    }
                    IntrinsicKind::Never => PropertyAccessResult::simple(TypeId::NEVER),
                    IntrinsicKind::String => self.resolve_string_property(prop_name, prop_atom),
                    IntrinsicKind::Number => self.resolve_number_property(prop_name, prop_atom),
                    IntrinsicKind::Boolean => self.resolve_boolean_property(prop_name, prop_atom),
                    IntrinsicKind::Bigint => self.resolve_bigint_property(prop_name, prop_atom),
                    IntrinsicKind::Object => {
                        self.resolve_object_member_or_not_found(obj_type, prop_name, prop_atom)
                    }
                    // Other intrinsic kinds: try apparent members
                    _ => {
                        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                            result
                        } else {
                            PropertyAccessResult::simple(TypeId::ANY)
                        }
                    }
                }
            }

            TypeData::Function(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            TypeData::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                for prop in &shape.properties {
                    if prop.name == prop_atom {
                        let read_type = self
                            .bind_object_receiver_this(obj_type, self.optional_property_type(prop));
                        let write_type = self.bind_object_receiver_this(
                            obj_type,
                            self.optional_property_write_type(prop),
                        );
                        let write = (write_type != read_type).then_some(write_type);
                        return PropertyAccessResult::Success {
                            type_id: read_type,
                            write_type: write,
                            from_index_signature: false,
                        };
                    }
                }
                // Check numeric index signature first for numeric property names
                use crate::objects::index_signatures::IndexSignatureResolver;
                let resolver = IndexSignatureResolver::new(self.interner());
                if resolver.is_numeric_index_name(prop_name)
                    && let Some(ref idx) = shape.number_index
                {
                    return PropertyAccessResult::from_index(
                        self.add_undefined_if_unchecked(idx.value_type),
                    );
                }
                // Check string index signature (for static index signatures on class constructors)
                if let Some(ref idx) = shape.string_index {
                    return PropertyAccessResult::from_index(
                        self.add_undefined_if_unchecked(idx.value_type),
                    );
                }
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                let mut results = Vec::new();
                let mut write_results = Vec::new();
                let mut any_from_index = false;
                let mut saw_deferred_any_fallback = false;
                let mut nullable_causes = Vec::new();
                let mut saw_unknown = false;
                let mut not_found_members: Vec<TypeId> = Vec::new();

                // Suppress `this` binding during intersection member resolution.
                // Each member would otherwise bind `ThisType` to itself (e.g. Thing1),
                // but the correct receiver is the full intersection (Thing1 & Thing2).
                // The checker substitutes `this` with the nominal receiver type afterward.
                let prev_skip = self.skip_this_binding.get();
                self.skip_this_binding.set(true);

                for &member in members.iter() {
                    match self.resolve_property_access_inner(member, prop_name, Some(prop_atom)) {
                        PropertyAccessResult::Success {
                            type_id,
                            write_type,
                            from_index_signature,
                        } => {
                            if type_id == TypeId::ANY
                                && !from_index_signature
                                && self.is_deferred_any_fallback_member(member)
                            {
                                saw_deferred_any_fallback = true;
                                continue;
                            }
                            results.push(type_id);
                            // For write types, use the explicit write_type if present, otherwise
                            // use type_id (non-divergent accessor).
                            write_results.push(write_type.unwrap_or(type_id));
                            if from_index_signature {
                                any_from_index = true;
                            }
                        }
                        PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type,
                            cause,
                        } => {
                            if let Some(t) = property_type {
                                results.push(t);
                            }
                            nullable_causes.push(cause);
                        }
                        PropertyAccessResult::IsUnknown => {
                            saw_unknown = true;
                        }
                        PropertyAccessResult::PropertyNotFound { .. } => {
                            // Track members that didn't have the property for fallback apparent type resolution.
                            // Some members like type parameters or lazy types may have the property on their apparent type.
                            not_found_members.push(member);
                        }
                    }
                }

                // Second pass: for members that didn't have the property, try their apparent types.
                // This handles cases like `Window & typeof globalThis` where `Window` has the property
                // but `typeof globalThis` needs apparent type resolution to find it.
                for &member in &not_found_members {
                    // Try apparent type for type parameters and primitives
                    let apparent = self.try_resolve_apparent_type(member);
                    if apparent != member
                        && apparent != TypeId::ANY
                        && let PropertyAccessResult::Success {
                            type_id,
                            write_type,
                            from_index_signature,
                        } =
                            self.resolve_property_access_inner(apparent, prop_name, Some(prop_atom))
                        && type_id != TypeId::ANY
                    {
                        results.push(type_id);
                        write_results.push(write_type.unwrap_or(type_id));
                        if from_index_signature {
                            any_from_index = true;
                        }
                    }
                }

                // Restore `this` binding state after per-member resolution.
                self.skip_this_binding.set(prev_skip);

                if results.is_empty() {
                    if !nullable_causes.is_empty() {
                        let cause = if nullable_causes.len() == 1 {
                            nullable_causes[0]
                        } else {
                            self.interner().union(nullable_causes)
                        };
                        return PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: None,
                            cause,
                        };
                    }
                    if saw_unknown {
                        return PropertyAccessResult::IsUnknown;
                    }
                    if saw_deferred_any_fallback {
                        return PropertyAccessResult::simple(TypeId::ANY);
                    }

                    // Before giving up, check if any member has an index signature
                    // For intersections, if ANY member has an index signature, the property access should succeed
                    use crate::objects::index_signatures::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.interner());

                    // Check string index signature on all members
                    for &member in members.iter() {
                        if resolver.has_index_signature(member, IndexKind::String)
                            && let Some(value_type) = resolver.resolve_string_index(member)
                        {
                            return PropertyAccessResult::from_index(
                                self.add_undefined_if_unchecked(value_type),
                            );
                        }
                    }

                    // Check numeric index signature if property name looks numeric
                    if resolver.is_numeric_index_name(prop_name) {
                        for &member in members.iter() {
                            if let Some(value_type) = resolver.resolve_number_index(member) {
                                return PropertyAccessResult::from_index(
                                    self.add_undefined_if_unchecked(value_type),
                                );
                            }
                        }
                    }

                    // Before giving up, try narrowing discriminated union intersections.
                    // E.g., `(A | B) & { kind: "one" }` — filter union to matching members
                    // and retry property access on the narrowed type.
                    if let Some(narrowed) =
                        self.try_narrow_discriminated_intersection(members.as_ref())
                        && narrowed != obj_type
                    {
                        return self.resolve_property_access_inner(
                            narrowed,
                            prop_name,
                            Some(prop_atom),
                        );
                    }

                    return PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    };
                }

                let mut type_id = if results.len() == 1 {
                    results[0]
                } else {
                    self.interner().intersection(results)
                };

                // Compute write type as intersection of member write types.
                // This handles divergent accessor intersections like `(A & B)['prop']`
                // where A's setter accepts `string | number` and B's setter accepts `"hello" | number`.
                // The resulting write type should be the intersection, which normalizes
                // (e.g., `string & "hello"` → `"hello"`).
                let computed_write_type = if write_results.len() == 1 {
                    write_results[0]
                } else {
                    self.interner().intersection(write_results)
                };

                // Do NOT bind `this` here. When a method like `self(): this`
                // is on an intersection member, `this` must resolve to the
                // receiver's nominal type (e.g., Thing5, not just {a,b,c}).
                // The checker has the correct nominal receiver and will
                // substitute `this` via its own fallback path.

                if any_from_index && self.no_unchecked_indexed_access {
                    type_id = self.add_undefined_if_unchecked(type_id);
                }

                // Only store write_type if it differs from read type
                let write_type = (computed_write_type != type_id).then_some(computed_write_type);

                PropertyAccessResult::Success {
                    type_id,
                    write_type,
                    from_index_signature: any_from_index,
                }
            }

            // ReadonlyType and NoInfer are transparent wrappers for property access
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.resolve_property_access_inner(inner, prop_name, prop_atom)
            }

            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(constraint) = info.constraint {
                    // Skip `this` binding when resolving through a type parameter's
                    // constraint. The checker substitutes `this` with the actual
                    // receiver (the type parameter T, not the constraint A).
                    let prev = self.skip_this_binding.get();
                    self.skip_this_binding.set(true);
                    let mut result =
                        self.resolve_property_access_inner(constraint, prop_name, Some(prop_atom));
                    if matches!(
                        result,
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            from_index_signature: false,
                            ..
                        }
                    ) {
                        let evaluated = self.db.evaluate_type_with_options(
                            constraint,
                            self.no_unchecked_indexed_access,
                        );
                        if evaluated != constraint {
                            result = self.resolve_property_access_inner(
                                evaluated,
                                prop_name,
                                Some(prop_atom),
                            );
                        }
                    }
                    self.skip_this_binding.set(prev);
                    result
                } else {
                    // Unconstrained type parameters have no properties in tsc.
                    // In TypeScript 6.0+, an unconstrained T is treated as `{}`
                    // which does NOT include Object prototype methods (toString,
                    // valueOf, hasOwnProperty, etc.). Accessing any property on
                    // bare T emits TS2339 "Property X does not exist on type T".
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    }
                }
            }

            // TS apparent members: literals inherit primitive wrapper methods.
            TypeData::Literal(ref literal) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                match literal {
                    LiteralValue::String(_) => self.resolve_string_property(prop_name, prop_atom),
                    LiteralValue::Number(_) => self.resolve_number_property(prop_name, prop_atom),
                    LiteralValue::Boolean(_) => self.resolve_boolean_property(prop_name, prop_atom),
                    LiteralValue::BigInt(_) => self.resolve_bigint_property(prop_name, prop_atom),
                }
            }

            TypeData::TemplateLiteral(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            // Application: handle nominally (preserve class/interface identity)
            TypeData::Application(app_id) => {
                let _guard = match self.enter_property_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        let prop_atom =
                            prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                        return self
                            .resolve_object_member_or_not_found(obj_type, prop_name, prop_atom);
                    }
                };

                // Use nominal resolution for Application types
                // This preserves class/interface identity instead of structurally expanding
                self.resolve_application_property(app_id, prop_name, prop_atom)
            }

            // Mapped: try lazy property resolution first to avoid OOM on large mapped types
            TypeData::Mapped(mapped_id) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));

                // Try lazy resolution first - only computes the requested property
                if let Some(result) =
                    self.resolve_mapped_property_lazy(mapped_id, prop_name, prop_atom)
                {
                    return result;
                }

                // Lazy resolution failed (complex constraint) - fall back to eager expansion
                let _guard = match self.enter_property_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        return self
                            .resolve_object_member_or_not_found(obj_type, prop_name, prop_atom);
                    }
                };

                let evaluated = self
                    .db
                    .evaluate_type_with_options(obj_type, self.no_unchecked_indexed_access);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom))
                } else {
                    // Evaluation didn't change the type - try apparent members first
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // Type is deferred (contains type parameters that prevent evaluation).
                        // Return ANY to avoid false TS2339 errors - the checker will handle
                        // the actual error reporting for circular or unresolvable types.
                        PropertyAccessResult::simple(TypeId::ANY)
                    }
                }
            }

            // TypeQuery types: typeof queries that need resolution to their structural form
            TypeData::TypeQuery(_) => {
                let evaluated = self
                    .db
                    .evaluate_type_with_options(obj_type, self.no_unchecked_indexed_access);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // TypeQuery type is deferred - return ANY to avoid false TS2339
                        PropertyAccessResult::simple(TypeId::ANY)
                    }
                }
            }

            // Conditional types need evaluation to their resolved form
            TypeData::Conditional(_) => {
                // Add recursion guard for consistency with other recursive type resolutions
                let _guard = match self.enter_property_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        let prop_atom =
                            prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                        return self
                            .resolve_object_member_or_not_found(obj_type, prop_name, prop_atom);
                    }
                };

                let evaluated = self
                    .db
                    .evaluate_type_with_options(obj_type, self.no_unchecked_indexed_access);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // Conditional type is deferred - return ANY to avoid false TS2339
                        PropertyAccessResult::simple(TypeId::ANY)
                    }
                }
            }

            // Index access types need evaluation
            TypeData::IndexAccess(_, _) => {
                // Add recursion guard for consistency with other recursive type resolutions
                let _guard = match self.enter_property_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        let prop_atom =
                            prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                        return self
                            .resolve_object_member_or_not_found(obj_type, prop_name, prop_atom);
                    }
                };

                let evaluated = self
                    .db
                    .evaluate_type_with_options(obj_type, self.no_unchecked_indexed_access);
                if evaluated != obj_type {
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type (still deferred).
                    // Try resolving the base constraint of the indexed access:
                    // if the index is a type parameter with a constraint, evaluate
                    // object[constraint] to get the apparent result type.
                    // E.g., {[s:string]:V}[K] where K extends keyof T => V
                    if let Some(TypeData::IndexAccess(ia_obj, ia_idx)) =
                        self.interner().lookup(obj_type)
                        && let Some(TypeData::TypeParameter(info)) = self.interner().lookup(ia_idx)
                        && let Some(constraint) = info.constraint
                    {
                        let base_constraint = self.db.evaluate_index_access_with_options(
                            ia_obj,
                            constraint,
                            self.no_unchecked_indexed_access,
                        );
                        if base_constraint != obj_type
                            && !matches!(
                                self.interner().lookup(base_constraint),
                                Some(TypeData::IndexAccess(_, _))
                            )
                        {
                            return self.resolve_property_access_inner(
                                base_constraint,
                                prop_name,
                                prop_atom,
                            );
                        }
                    }

                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // IndexAccess type is deferred - return ANY to avoid false TS2339
                        PropertyAccessResult::simple(TypeId::ANY)
                    }
                }
            }

            // KeyOf types need evaluation
            TypeData::KeyOf(_) => {
                let evaluated = self
                    .db
                    .evaluate_type_with_options(obj_type, self.no_unchecked_indexed_access);
                if evaluated != obj_type {
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    // KeyOf typically returns string/number/symbol, try string member access
                    self.resolve_string_property(prop_name, prop_atom)
                }
            }

            // ThisType: represents 'this' type in a class/interface context
            // Should be resolved to the actual class type by the checker
            TypeData::ThisType => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                // 'this' type not resolved - return ANY to avoid false positives
                // (checker should resolve 'this' before reaching solver)
                PropertyAccessResult::simple(TypeId::ANY)
            }

            // Lazy types (interfaces, classes, type aliases) need resolution
            TypeData::Lazy(def_id) => {
                // CRITICAL: Add recursion guard for type aliases
                // Type aliases can form cycles: type A = B; type B = A;
                let _guard = match self.enter_property_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        let prop_atom =
                            prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                        return self
                            .resolve_object_member_or_not_found(obj_type, prop_name, prop_atom);
                    }
                };

                // Resolve the lazy type using the resolver
                if let Some(resolved) = self.resolver().resolve_lazy(def_id, self.interner()) {
                    let resolved = if crate::contains_this_type(self.interner(), resolved) {
                        crate::substitute_this_type(self.interner(), resolved, obj_type)
                    } else {
                        resolved
                    };
                    // Successfully resolved - resolve property on the concrete type
                    self.resolve_property_access_inner(resolved, prop_name, prop_atom)
                } else {
                    // Can't resolve lazy type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // Lazy type couldn't be resolved (likely circular) - return ANY
                        // to avoid false TS2339 errors
                        PropertyAccessResult::simple(TypeId::ANY)
                    }
                }
            }

            // Enum values inherit methods from their structural member type
            // (number for numeric enums, string for string enums)
            TypeData::Enum(_def_id, member_type) => {
                self.resolve_property_access_inner(member_type, prop_name, prop_atom)
            }

            // StringIntrinsic (Uppercase<T>, Lowercase<T>, etc.) — resolve as string
            TypeData::StringIntrinsic { .. } => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            _ => {
                // Unknown type key - try apparent members before giving up
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                // For truly unknown types, return ANY to avoid false positives
                PropertyAccessResult::simple(TypeId::ANY)
            }
        }
    }

    /// Try to resolve the apparent type for a given type.
    ///
    /// This is used by intersection property resolution to find properties that may
    /// only exist on the apparent type (e.g., type parameters with constraints).
    /// Returns the original type if no apparent type can be determined.
    fn try_resolve_apparent_type(&self, type_id: TypeId) -> TypeId {
        use crate::type_queries::get_type_parameter_constraint;

        if let Some(constraint) = get_type_parameter_constraint(self.interner(), type_id) {
            // For type parameters, return the constraint (or unknown if none)
            if constraint == TypeId::UNKNOWN {
                return TypeId::UNKNOWN;
            }
            return constraint;
        }

        // For primitive types, the solver already handles apparent types
        // in resolve_string_property, resolve_number_property, etc.
        // We return the original type here and let the normal resolution handle it.
        type_id
    }

    /// For intersections of a discriminated union with a literal discriminant object
    /// (e.g. `(A | B) & { kind: "one" }`), narrow the union by filtering members
    /// whose discriminant property conflicts with the literal, then return the
    /// simplified intersection. Returns `None` if the pattern doesn't apply.
    fn try_narrow_discriminated_intersection(&self, members: &[TypeId]) -> Option<TypeId> {
        // Find union members and object members with literal discriminant properties.
        let mut union_idx = None;
        let mut discriminant_props: smallvec::SmallVec<[(Atom, TypeId); 4]> =
            smallvec::SmallVec::new();
        let mut other_members: Vec<TypeId> = Vec::new();

        for (i, &member) in members.iter().enumerate() {
            match self.interner().lookup(member) {
                Some(TypeData::Union(_)) => {
                    if union_idx.is_some() {
                        // Multiple unions - too complex, bail
                        return None;
                    }
                    union_idx = Some(i);
                }
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    let shape = self.interner().object_shape(shape_id);
                    for prop in &shape.properties {
                        if crate::type_queries::is_unit_type(self.interner(), prop.type_id) {
                            discriminant_props.push((prop.name, prop.type_id));
                        }
                    }
                    other_members.push(member);
                }
                _ => {
                    other_members.push(member);
                }
            }
        }

        let union_idx = union_idx?;
        if discriminant_props.is_empty() {
            return None;
        }

        let union_member = members[union_idx];
        let TypeData::Union(union_list) = self.interner().lookup(union_member)? else {
            return None;
        };
        let union_members = self.interner().type_list(union_list);

        // Filter union members: keep only those whose discriminant properties don't
        // conflict with the literal values from the non-union objects.
        let mut filtered: Vec<TypeId> = Vec::new();
        for &um in union_members.iter() {
            let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                self.interner().lookup(um)
            else {
                // Non-object union member — can't narrow, keep it
                filtered.push(um);
                continue;
            };
            let shape = self.interner().object_shape(shape_id);
            let mut dominated = false;
            for &(disc_name, disc_value) in &discriminant_props {
                if let Some(prop) = shape.properties.iter().find(|p| p.name == disc_name)
                    && crate::type_queries::is_unit_type(self.interner(), prop.type_id)
                    && prop.type_id != disc_value
                {
                    // Conflicting discriminant — this member is eliminated
                    dominated = true;
                    break;
                }
            }
            if !dominated {
                filtered.push(um);
            }
        }

        // Only produce a result if we actually narrowed something
        if filtered.len() == union_members.len() {
            return None;
        }

        // Build the narrowed type
        let narrowed_union = if filtered.is_empty() {
            TypeId::NEVER
        } else if filtered.len() == 1 {
            filtered[0]
        } else {
            self.interner().union(filtered)
        };

        // Reconstruct the intersection with the narrowed union
        let mut new_members = other_members;
        new_members.push(narrowed_union);

        if new_members.len() == 1 {
            Some(new_members[0])
        } else {
            Some(self.interner().intersection(new_members))
        }
    }

    // Resolution helpers (mapped types, primitives, arrays, applications, etc.)
    // are in property_helpers.rs
}
