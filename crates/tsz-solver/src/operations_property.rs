//! Property access resolution (`PropertyAccessEvaluator`) for resolving
//! property access on types (obj.prop, obj["key"], etc.).

use crate::db::QueryDatabase;
use crate::subtype::TypeResolver;
use crate::types::{IntrinsicKind, LiteralValue, ObjectShapeId, TypeData, TypeId};
use crate::{ApparentMemberKind, TypeDatabase, apparent_object_member_kind};
use std::cell::RefCell;
use tsz_common::interner::Atom;

// Re-export readonly helpers for backward compatibility
pub use crate::operations_property_readonly::{is_readonly_index_signature, property_is_readonly};

// Child module: resolution helpers (mapped types, primitives, arrays, applications, etc.)
#[path = "operations_property_helpers.rs"]
mod operations_property_helpers;

// =============================================================================
// Property Access Resolution
// =============================================================================

/// Result of attempting to access a property on a type.
#[derive(Clone, Debug)]
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

    /// Maps the `type_id` in a Success result, leaving other variants unchanged.
    pub fn map_success_type<F>(self, f: F) -> Self
    where
        F: FnOnce(TypeId) -> TypeId,
    {
        match self {
            Self::Success {
                type_id,
                write_type,
                from_index_signature,
            } => Self::Success {
                type_id: f(type_id),
                write_type,
                from_index_signature,
            },
            other => other,
        }
    }

    /// Returns the type if Success, otherwise returns the default value.
    pub fn success_type_or(&self, default: TypeId) -> TypeId {
        self.success_type().unwrap_or(default)
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
    pub(crate) no_unchecked_indexed_access: bool,
    /// Unified recursion guard for cycle detection and depth limiting.
    pub(crate) guard: RefCell<crate::recursion::RecursionGuard<TypeId>>,
    // Context for visitor pattern (set during property access resolution)
    // We store both the str (for immediate use) and Atom (for interned comparisons)
    pub(crate) current_prop_name: RefCell<Option<String>>,
    pub(crate) current_prop_atom: RefCell<Option<Atom>>,
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
            no_unchecked_indexed_access: false,
            guard: RefCell::new(crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::PropertyAccess,
            )),
            current_prop_name: RefCell::new(None),
            current_prop_atom: RefCell::new(None),
        }
    }

    pub fn with_resolver(db: &'a dyn QueryDatabase, _resolver: &dyn TypeResolver) -> Self {
        // Note: resolver parameter is currently unused but kept for API compatibility
        // TODO: Integrate resolver into PropertyAccessEvaluator if needed
        PropertyAccessEvaluator {
            db,
            no_unchecked_indexed_access: false,
            guard: RefCell::new(crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::PropertyAccess,
            )),
            current_prop_name: RefCell::new(None),
            current_prop_atom: RefCell::new(None),
        }
    }

    pub const fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    /// Helper to access the underlying `TypeDatabase`
    pub(crate) fn interner(&self) -> &dyn TypeDatabase {
        self.db.as_type_database()
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
        *self.current_prop_name.borrow_mut() = Some(prop_name.to_string());
        *self.current_prop_atom.borrow_mut() = prop_atom;

        // Use visitor for types we've migrated (Intrinsic, Object, ObjectWithIndex, Array, Tuple)
        // Due to TypeVisitor requiring &mut self, we inline the visitor logic here
        // This maintains the visitor pattern while avoiding unsafe casts
        let key_opt = self.interner().lookup(obj_type);
        if let Some(key) = key_opt {
            let result = match key {
                TypeData::Intrinsic(kind) => {
                    // Inline visitor logic for intrinsics
                    match kind {
                        IntrinsicKind::Any => Some(PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            write_type: None,
                            from_index_signature: false,
                        }),
                        IntrinsicKind::Unknown => Some(PropertyAccessResult::IsUnknown),
                        IntrinsicKind::Void | IntrinsicKind::Null | IntrinsicKind::Undefined => {
                            let cause = if kind == IntrinsicKind::Void
                                || kind == IntrinsicKind::Undefined
                            {
                                TypeId::UNDEFINED
                            } else {
                                TypeId::NULL
                            };
                            Some(PropertyAccessResult::PossiblyNullOrUndefined {
                                property_type: None,
                                cause,
                            })
                        }
                        IntrinsicKind::Symbol => {
                            let prop_atom_inner = prop_atom
                                .unwrap_or_else(|| self.interner().intern_string(prop_name));
                            Some(self.resolve_symbol_primitive_property(prop_name, prop_atom_inner))
                        }
                        _ => None,
                    }
                }
                TypeData::Object(shape_id) => {
                    // Inline visitor logic for Object - calls visit_object implementation
                    self.visit_object_impl(shape_id.0, prop_name, prop_atom)
                }
                TypeData::ObjectWithIndex(shape_id) => {
                    // Inline visitor logic for ObjectWithIndex - calls visit_object_with_index implementation
                    self.visit_object_with_index_impl(shape_id.0, prop_name, prop_atom)
                }
                TypeData::Array(_elem) => {
                    // Inline visitor logic for Array
                    self.visit_array_impl(obj_type, prop_name, prop_atom)
                }
                TypeData::Tuple(_list_id) => {
                    // Inline visitor logic for Tuple
                    self.visit_array_impl(obj_type, prop_name, prop_atom)
                }
                TypeData::Union(list_id) => {
                    // Inline visitor logic for Union - calls visit_union implementation
                    self.visit_union_impl(list_id.0, prop_name, prop_atom)
                }
                // Note: TypeData::Application is handled in the fallback section with proper type substitution
                _ => None, // Not yet migrated to visitor
            };

            if let Some(res) = result {
                return res;
            }
        }

        // Fallback to existing match statement for types not yet migrated
        // Look up the type key
        let key = match self.interner().lookup(obj_type) {
            Some(k) => k,
            None => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                return PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                };
            }
        };

        match key {
            TypeData::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(prop),
                        write_type: None,
                        from_index_signature: false,
                    };
                }
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }

                // Check for index signatures using IndexSignatureResolver
                // Some Object types may have index signatures that aren't in ObjectWithIndex
                use crate::index_signatures::{IndexKind, IndexSignatureResolver};
                let resolver = IndexSignatureResolver::new(self.interner());

                // Try string index signature first (most common)
                if resolver.has_index_signature(obj_type, IndexKind::String)
                    && let Some(value_type) = resolver.resolve_string_index(obj_type)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(value_type),
                        write_type: None,
                        from_index_signature: true,
                    };
                }

                // Try numeric index signature if property name looks numeric
                if resolver.is_numeric_index_name(prop_name)
                    && let Some(value_type) = resolver.resolve_number_index(obj_type)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(value_type),
                        write_type: None,
                        from_index_signature: true,
                    };
                }

                PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                }
            }

            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(prop),
                        write_type: None,
                        from_index_signature: false,
                    };
                }

                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }

                // Check string index signature
                if let Some(ref idx) = shape.string_index {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(idx.value_type),
                        write_type: None,
                        from_index_signature: true,
                    };
                }

                // Check numeric index signature if property name looks numeric
                use crate::index_signatures::IndexSignatureResolver;
                let resolver = IndexSignatureResolver::new(self.interner());
                if resolver.is_numeric_index_name(prop_name)
                    && let Some(ref idx) = shape.number_index
                {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(idx.value_type),
                        write_type: None,
                        from_index_signature: true,
                    };
                }

                PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
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
                        return PropertyAccessResult::Success {
                            type_id: self.optional_property_type(prop),
                            write_type: None,
                            from_index_signature: false,
                        };
                    }
                }
                // Check string index signature (for static index signatures on class constructors)
                if let Some(ref idx) = shape.string_index {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(idx.value_type),
                        write_type: None,
                        from_index_signature: true,
                    };
                }
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            // TypeData::Union - migrated to visitor pattern (see visit_union_impl)
            // This should never be reached since visitor handles it above
            TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                let mut results = Vec::new();
                let mut any_from_index = false;
                let mut nullable_causes = Vec::new();
                let mut saw_unknown = false;

                for &member in members.iter() {
                    match self.resolve_property_access_inner(member, prop_name, Some(prop_atom)) {
                        PropertyAccessResult::Success {
                            type_id,
                            from_index_signature,
                            ..
                        } => {
                            results.push(type_id);
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
                        PropertyAccessResult::PropertyNotFound { .. } => {}
                    }
                }

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

                    // Before giving up, check if any member has an index signature
                    // For intersections, if ANY member has an index signature, the property access should succeed
                    use crate::index_signatures::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.interner());

                    // Check string index signature on all members
                    for &member in members.iter() {
                        if resolver.has_index_signature(member, IndexKind::String)
                            && let Some(value_type) = resolver.resolve_string_index(member)
                        {
                            return PropertyAccessResult::Success {
                                type_id: self.add_undefined_if_unchecked(value_type),
                                write_type: None,
                                from_index_signature: true,
                            };
                        }
                    }

                    // Check numeric index signature if property name looks numeric
                    if resolver.is_numeric_index_name(prop_name) {
                        for &member in members.iter() {
                            if let Some(value_type) = resolver.resolve_number_index(member) {
                                return PropertyAccessResult::Success {
                                    type_id: self.add_undefined_if_unchecked(value_type),
                                    write_type: None,
                                    from_index_signature: true,
                                };
                            }
                        }
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
                if any_from_index && self.no_unchecked_indexed_access {
                    type_id = self.add_undefined_if_unchecked(type_id);
                }

                PropertyAccessResult::Success {
                    type_id,
                    write_type: None,
                    from_index_signature: any_from_index,
                }
            }

            TypeData::ReadonlyType(inner) => {
                self.resolve_property_access_inner(inner, prop_name, prop_atom)
            }

            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(constraint) = info.constraint {
                    // Recurse into the constraint to find the property
                    self.resolve_property_access_inner(constraint, prop_name, Some(prop_atom))
                } else {
                    // TypeParameter with no constraint: fallback to Object members
                    // In TypeScript, unconstrained type parameters allow access to Object members
                    // like toString, hasOwnProperty, etc.
                    self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                        PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type,
                            property_name: prop_atom,
                        },
                    )
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

            // Built-in properties
            TypeData::Intrinsic(IntrinsicKind::String) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            TypeData::Intrinsic(IntrinsicKind::Number) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_number_property(prop_name, prop_atom)
            }

            TypeData::Intrinsic(IntrinsicKind::Boolean) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_boolean_property(prop_name, prop_atom)
            }

            TypeData::Intrinsic(IntrinsicKind::Bigint) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_bigint_property(prop_name, prop_atom)
            }

            TypeData::Intrinsic(IntrinsicKind::Object) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    },
                )
            }

            TypeData::Array(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            TypeData::Tuple(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            // Application: handle nominally (preserve class/interface identity)
            TypeData::Application(app_id) => {
                let _guard = match self.enter_property_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        let prop_atom =
                            prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                        return self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                            PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            },
                        );
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
                        return self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                            PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            },
                        );
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
                        // Can't determine the actual type - return ANY to avoid false positives
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            write_type: None,
                            from_index_signature: false,
                        }
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
                        // Can't resolve type query - return ANY to avoid false positives
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            write_type: None,
                            from_index_signature: false,
                        }
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
                        return self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                            PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            },
                        );
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
                        // Can't evaluate - return ANY to avoid false positives
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            write_type: None,
                            from_index_signature: false,
                        }
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
                        return self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                            PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            },
                        );
                    }
                };

                let evaluated = self
                    .db
                    .evaluate_type_with_options(obj_type, self.no_unchecked_indexed_access);
                if evaluated != obj_type {
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // Can't evaluate - return ANY to avoid false positives
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            write_type: None,
                            from_index_signature: false,
                        }
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
                PropertyAccessResult::Success {
                    type_id: TypeId::ANY,
                    write_type: None,
                    from_index_signature: false,
                }
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
                        return self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                            PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            },
                        );
                    }
                };

                // Resolve the lazy type using the resolver
                if let Some(resolved) = self.db.resolve_lazy(def_id, self.interner()) {
                    // Successfully resolved - resolve property on the concrete type
                    self.resolve_property_access_inner(resolved, prop_name, prop_atom)
                } else {
                    // Can't resolve - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // Can't evaluate - return ANY to avoid false positives
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            write_type: None,
                            from_index_signature: false,
                        }
                    }
                }
            }

            // Enum values inherit methods from their structural member type
            // (number for numeric enums, string for string enums)
            TypeData::Enum(_def_id, member_type) => {
                self.resolve_property_access_inner(member_type, prop_name, prop_atom)
            }

            _ => {
                // Unknown type key - try apparent members before giving up
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                // For truly unknown types, return ANY to avoid false positives
                // This includes UniqueSymbol, Error, and any future type keys
                PropertyAccessResult::Success {
                    type_id: TypeId::ANY,
                    write_type: None,
                    from_index_signature: false,
                }
            }
        }
    }

    // Resolution helpers (mapped types, primitives, arrays, applications, etc.)
    // are in operations_property_helpers.rs
}
