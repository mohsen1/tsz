//! Property Access Resolution
//!
//! This module contains the PropertyAccessEvaluator and related types for
//! resolving property access on types (obj.prop, obj["key"], etc.).
//!
//! Extracted from operations.rs to keep file sizes manageable.

use crate::db::QueryDatabase;
use crate::evaluate::evaluate_type;
use crate::instantiate::{TypeSubstitution, instantiate_type};
use crate::subtype::TypeResolver;
use crate::types::*;
use crate::visitor::TypeVisitor;
use crate::{
    ApparentMemberKind, TypeDatabase, apparent_object_member_kind, apparent_primitive_member_kind,
};
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use tsz_common::interner::Atom;

// =============================================================================
// Property Access Resolution
// =============================================================================

/// Result of attempting to access a property on a type.
#[derive(Clone, Debug)]
pub enum PropertyAccessResult {
    /// Property exists, returns its type
    Success {
        type_id: TypeId,
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

/// Evaluates property access.
///
/// Uses QueryDatabase which provides both TypeDatabase and TypeResolver functionality,
/// enabling proper resolution of Lazy types and type aliases.
pub struct PropertyAccessEvaluator<'a> {
    db: &'a dyn QueryDatabase,
    no_unchecked_indexed_access: bool,
    visiting: RefCell<FxHashSet<TypeId>>,
    depth: RefCell<u32>,
    // Context for visitor pattern (set during property access resolution)
    // We store both the str (for immediate use) and Atom (for interned comparisons)
    current_prop_name: RefCell<Option<String>>,
    current_prop_atom: RefCell<Option<Atom>>,
}

struct PropertyAccessGuard<'a> {
    evaluator: &'a PropertyAccessEvaluator<'a>,
    obj_type: TypeId,
}

impl<'a> Drop for PropertyAccessGuard<'a> {
    fn drop(&mut self) {
        self.evaluator.visiting.borrow_mut().remove(&self.obj_type);
        *self.evaluator.depth.borrow_mut() -= 1;
    }
}

impl<'a> PropertyAccessEvaluator<'a> {
    pub fn new(db: &'a dyn QueryDatabase) -> Self {
        PropertyAccessEvaluator {
            db,
            no_unchecked_indexed_access: false,
            visiting: RefCell::new(FxHashSet::default()),
            depth: RefCell::new(0),
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
            visiting: RefCell::new(FxHashSet::default()),
            depth: RefCell::new(0),
            current_prop_name: RefCell::new(None),
            current_prop_atom: RefCell::new(None),
        }
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    /// Helper to access the underlying TypeDatabase
    fn interner(&self) -> &dyn TypeDatabase {
        self.db.as_type_database()
    }
}

// =============================================================================
// TypeVisitor Implementation for PropertyAccessEvaluator
// =============================================================================

// Implement TypeVisitor for &PropertyAccessEvaluator to solve &mut self issue
// This allows visitor methods to be called from &self methods while still
// being able to mutate internal state via RefCells.
impl<'a> TypeVisitor for &PropertyAccessEvaluator<'a> {
    type Output = Option<PropertyAccessResult>;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        match kind {
            IntrinsicKind::Any => Some(PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            }),
            IntrinsicKind::Never => {
                // Property access on never returns never (code is unreachable)
                Some(PropertyAccessResult::Success {
                    type_id: TypeId::NEVER,
                    from_index_signature: false,
                })
            }
            IntrinsicKind::Unknown => Some(PropertyAccessResult::IsUnknown),
            IntrinsicKind::Void | IntrinsicKind::Null | IntrinsicKind::Undefined => {
                let cause = if kind == IntrinsicKind::Void || kind == IntrinsicKind::Undefined {
                    TypeId::UNDEFINED
                } else {
                    TypeId::NULL
                };
                Some(PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: None,
                    cause,
                })
            }
            // Handle primitive intrinsic types by delegating to their boxed interfaces
            IntrinsicKind::String => {
                let prop_name = self.current_prop_name.borrow();
                let prop_atom = self.current_prop_atom.borrow();
                match (prop_name.as_deref(), prop_atom.as_ref()) {
                    (Some(name), Some(&atom)) => Some(self.resolve_string_property(name, atom)),
                    _ => None,
                }
            }
            IntrinsicKind::Number => {
                let prop_name = self.current_prop_name.borrow();
                let prop_atom = self.current_prop_atom.borrow();
                match (prop_name.as_deref(), prop_atom.as_ref()) {
                    (Some(name), Some(&atom)) => Some(self.resolve_number_property(name, atom)),
                    _ => None,
                }
            }
            IntrinsicKind::Boolean => {
                let prop_name = self.current_prop_name.borrow();
                let prop_atom = self.current_prop_atom.borrow();
                match (prop_name.as_deref(), prop_atom.as_ref()) {
                    (Some(name), Some(&atom)) => Some(self.resolve_boolean_property(name, atom)),
                    _ => None,
                }
            }
            IntrinsicKind::Bigint => {
                let prop_name = self.current_prop_name.borrow();
                let prop_atom = self.current_prop_atom.borrow();
                match (prop_name.as_deref(), prop_atom.as_ref()) {
                    (Some(name), Some(&atom)) => Some(self.resolve_bigint_property(name, atom)),
                    _ => None,
                }
            }
            // Symbol intrinsic is handled separately (has special properties)
            IntrinsicKind::Symbol => {
                // Get the property name from context
                let prop_name = self.current_prop_name.borrow();
                let prop_atom = self.current_prop_atom.borrow();
                match (prop_name.as_deref(), prop_atom.as_ref()) {
                    (Some(name), Some(&atom)) => {
                        Some(self.resolve_symbol_primitive_property(name, atom))
                    }
                    _ => None,
                }
            }
            // Other intrinsics (Object, etc.) fall back to None
            _ => None,
        }
    }

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        use crate::types::LiteralValue;

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Handle primitive literals by delegating to their boxed interface types
        match value {
            LiteralValue::String(_) => Some(self.resolve_string_property(prop_name, prop_atom)),
            LiteralValue::Number(_) => Some(self.resolve_number_property(prop_name, prop_atom)),
            LiteralValue::Boolean(_) => Some(self.resolve_boolean_property(prop_name, prop_atom)),
            LiteralValue::BigInt(_) => Some(self.resolve_bigint_property(prop_name, prop_atom)),
        }
    }

    fn visit_object(&mut self, shape_id: u32) -> Self::Output {
        use crate::index_signatures::{IndexKind, IndexSignatureResolver};
        use crate::types::TypeKey;

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return Some(result);
        }

        // Check for index signatures (some Object types may have index signatures that aren't in ObjectWithIndex)
        let resolver = IndexSignatureResolver::new(self.interner());

        // Reconstruct obj_type from shape_id for index signature checking
        let obj_type = self
            .interner()
            .intern(TypeKey::Object(ObjectShapeId(shape_id)));

        // Try string index signature first (most common)
        if resolver.has_index_signature(obj_type, IndexKind::String) {
            if let Some(value_type) = resolver.resolve_string_index(obj_type) {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    from_index_signature: true,
                });
            }
        }

        // Try numeric index signature if property name looks numeric
        if resolver.is_numeric_index_name(prop_name) {
            if let Some(value_type) = resolver.resolve_number_index(obj_type) {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    from_index_signature: true,
                });
            }
        }

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        use crate::index_signatures::IndexSignatureResolver;

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return Some(result);
        }

        // Check string index signature
        if let Some(ref idx) = shape.string_index {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(idx.value_type),
                from_index_signature: true,
            });
        }

        // Check numeric index signature if property name looks numeric
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name) {
            if let Some(ref idx) = shape.number_index {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(idx.value_type),
                    from_index_signature: true,
                });
            }
        }

        // Reconstruct obj_type for PropertyNotFound result
        let obj_type = self
            .interner()
            .intern(crate::types::TypeKey::ObjectWithIndex(ObjectShapeId(
                shape_id,
            )));

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        use crate::types::TypeKey;

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Reconstruct obj_type for resolve_array_property
        let obj_type = self.interner().intern(TypeKey::Array(element_type));
        Some(self.resolve_array_property(obj_type, prop_name, prop_atom))
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        use crate::types::TypeKey;

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Reconstruct obj_type for resolve_array_property
        let obj_type = self.interner().intern(TypeKey::Tuple(TupleListId(list_id)));
        Some(self.resolve_array_property(obj_type, prop_name, prop_atom))
    }

    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        // Template literals are string-like for property access
        // They support the same properties as the String interface
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        Some(self.resolve_string_property(prop_name, prop_atom))
    }

    fn visit_string_intrinsic(
        &mut self,
        _kind: crate::types::StringIntrinsicKind,
        _type_arg: TypeId,
    ) -> Self::Output {
        // String intrinsics (Uppercase<T>, Lowercase<T>, Capitalize<T>, etc.)
        // are string-like for property access
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        Some(self.resolve_string_property(prop_name, prop_atom))
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = match prop_name.as_deref() {
            Some(name) => name,
            None => return None,
        };

        self.visit_union_impl(list_id, prop_name, prop_atom_opt.as_ref().copied())
    }

    fn default_output() -> Self::Output {
        None
    }
}

impl<'a> PropertyAccessEvaluator<'a> {
    // Helper methods to call visitor logic from &self context
    // These contain the actual implementation that the TypeVisitor trait methods delegate to

    fn visit_object_impl(
        &self,
        shape_id: u32,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> Option<PropertyAccessResult> {
        use crate::index_signatures::{IndexKind, IndexSignatureResolver};
        use crate::types::TypeKey;

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return Some(result);
        }

        // Check for index signatures (some Object types may have index signatures that aren't in ObjectWithIndex)
        let resolver = IndexSignatureResolver::new(self.interner());

        // Reconstruct obj_type from shape_id for index signature checking
        let obj_type = self
            .interner()
            .intern(TypeKey::Object(ObjectShapeId(shape_id)));

        // Try string index signature first (most common)
        if resolver.has_index_signature(obj_type, IndexKind::String) {
            if let Some(value_type) = resolver.resolve_string_index(obj_type) {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    from_index_signature: true,
                });
            }
        }

        // Try numeric index signature if property name looks numeric
        if resolver.is_numeric_index_name(prop_name) {
            if let Some(value_type) = resolver.resolve_number_index(obj_type) {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    from_index_signature: true,
                });
            }
        }

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    fn visit_object_with_index_impl(
        &self,
        shape_id: u32,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> Option<PropertyAccessResult> {
        use crate::index_signatures::IndexSignatureResolver;

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return Some(result);
        }

        // Check string index signature
        if let Some(ref idx) = shape.string_index {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(idx.value_type),
                from_index_signature: true,
            });
        }

        // Check numeric index signature if property name looks numeric
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name) {
            if let Some(ref idx) = shape.number_index {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(idx.value_type),
                    from_index_signature: true,
                });
            }
        }

        // Reconstruct obj_type for PropertyNotFound result
        let obj_type = self
            .interner()
            .intern(crate::types::TypeKey::ObjectWithIndex(ObjectShapeId(
                shape_id,
            )));

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    fn visit_array_impl(
        &self,
        obj_type: TypeId,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> Option<PropertyAccessResult> {
        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };
        Some(self.resolve_array_property(obj_type, prop_name, prop_atom))
    }

    fn visit_union_impl(
        &self,
        list_id: u32,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> Option<PropertyAccessResult> {
        use crate::index_signatures::{IndexKind, IndexSignatureResolver};
        use crate::types::TypeKey;

        let members = self.interner().type_list(crate::types::TypeListId(list_id));

        // Fast-path: if ANY member is any, result is any
        if members.contains(&TypeId::ANY) {
            return Some(PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            });
        }

        // Fast-path: if ANY member is error, result is error
        if members.contains(&TypeId::ERROR) {
            return Some(PropertyAccessResult::Success {
                type_id: TypeId::ERROR,
                from_index_signature: false,
            });
        }

        // Filter out UNKNOWN members - they shouldn't cause the entire union to be unknown
        // Only return IsUnknown if ALL members are UNKNOWN
        let non_unknown_members: Vec<_> = members
            .iter()
            .filter(|&&t| t != TypeId::UNKNOWN)
            .copied()
            .collect();

        if non_unknown_members.is_empty() {
            // All members are UNKNOWN
            return Some(PropertyAccessResult::IsUnknown);
        }

        // Reconstruct obj_type for error messages
        let obj_type = self
            .interner()
            .intern(TypeKey::Union(crate::types::TypeListId(list_id)));

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Property access on union: partition into nullable and non-nullable members
        let mut valid_results = Vec::new();
        let mut nullable_causes = Vec::new();
        let mut any_from_index = false; // Track if any member used index signature

        for &member in &non_unknown_members {
            // Check for null/undefined directly
            if member == TypeId::NULL || member == TypeId::UNDEFINED || member == TypeId::VOID {
                let cause = if member == TypeId::VOID {
                    TypeId::UNDEFINED
                } else {
                    member
                };
                nullable_causes.push(cause);
                continue;
            }

            match self.resolve_property_access_inner(member, prop_name, Some(prop_atom)) {
                PropertyAccessResult::Success {
                    type_id,
                    from_index_signature,
                } => {
                    valid_results.push(type_id);
                    if from_index_signature {
                        any_from_index = true; // Propagate: if ANY member uses index, flag it
                    }
                }
                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => {
                    if let Some(t) = property_type {
                        valid_results.push(t);
                    }
                    nullable_causes.push(cause);
                }
                // PropertyNotFound: if ANY member is missing the property, the property does not exist on the Union
                PropertyAccessResult::PropertyNotFound { .. } => {
                    return Some(PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    });
                }
                // IsUnknown: if any member is unknown, we cannot safely access the property
                PropertyAccessResult::IsUnknown => {
                    return Some(PropertyAccessResult::IsUnknown);
                }
            }
        }

        // If no non-nullable members had the property, it's a PropertyNotFound error
        if valid_results.is_empty() && nullable_causes.is_empty() {
            // Before giving up, check union-level index signatures
            let resolver = IndexSignatureResolver::new(self.interner());

            if resolver.has_index_signature(obj_type, IndexKind::String) {
                if let Some(value_type) = resolver.resolve_string_index(obj_type) {
                    return Some(PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(value_type),
                        from_index_signature: true,
                    });
                }
            }

            if resolver.is_numeric_index_name(prop_name) {
                if let Some(value_type) = resolver.resolve_number_index(obj_type) {
                    return Some(PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(value_type),
                        from_index_signature: true,
                    });
                }
            }

            return Some(PropertyAccessResult::PropertyNotFound {
                type_id: obj_type,
                property_name: prop_atom,
            });
        }

        // If there are nullable causes, return PossiblyNullOrUndefined
        if !nullable_causes.is_empty() {
            let cause = if nullable_causes.len() == 1 {
                nullable_causes[0]
            } else {
                self.interner().union(nullable_causes)
            };

            let mut property_type = if valid_results.is_empty() {
                None
            } else if valid_results.len() == 1 {
                Some(valid_results[0])
            } else {
                Some(self.interner().union(valid_results))
            };

            if any_from_index
                && self.no_unchecked_indexed_access
                && let Some(t) = property_type
            {
                property_type = Some(self.add_undefined_if_unchecked(t));
            }

            return Some(PropertyAccessResult::PossiblyNullOrUndefined {
                property_type,
                cause,
            });
        }

        let mut type_id = self.interner().union(valid_results);
        if any_from_index && self.no_unchecked_indexed_access {
            type_id = self.add_undefined_if_unchecked(type_id);
        }

        // Union of all result types
        Some(PropertyAccessResult::Success {
            type_id,
            from_index_signature: any_from_index, // Contagious across union members
        })
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
        const MAX_PROPERTY_ACCESS_DEPTH: u32 = 50;

        let mut depth = self.depth.borrow_mut();
        if *depth >= MAX_PROPERTY_ACCESS_DEPTH {
            return None;
        }
        *depth += 1;
        drop(depth);

        let mut visiting = self.visiting.borrow_mut();
        if !visiting.insert(obj_type) {
            drop(visiting);
            *self.depth.borrow_mut() -= 1;
            return None;
        }

        Some(PropertyAccessGuard {
            evaluator: self,
            obj_type,
        })
    }

    /// Lazily resolve a single property from a mapped type without fully expanding it.
    /// This avoids OOM by only computing the property type that was requested.
    ///
    /// Returns `Some(result)` if we could resolve the property lazily,
    /// `None` if we need to fall back to eager expansion.
    fn resolve_mapped_property_lazy(
        &self,
        mapped_id: MappedTypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        use crate::types::{LiteralValue, MappedModifier, TypeKey};

        let mapped = self.interner().mapped_type(mapped_id);

        // SPECIAL CASE: Mapped types over array-like sources
        // When a mapped type like Boxified<T> = { [P in keyof T]: Box<T[P]> } is applied
        // to an array type, array methods (pop, push, concat, etc.) should NOT be mapped
        // through the template. They should be resolved from the resulting array type.
        //
        // For example: Boxified<T> where T extends any[]
        // - Numeric properties (0, 1, 2) → Box<T[number]>
        // - Array methods (pop, push) → resolved from Array<Box<T[number]>>
        if let Some(result) =
            self.resolve_array_mapped_type_method(&mapped, mapped_id, prop_name, prop_atom)
        {
            return Some(result);
        }

        // Step 1: Check if this property name is valid in the constraint
        // We need to check if the literal string prop_name is in the constraint
        let constraint = mapped.constraint;

        // Try to determine if prop_name is a valid key
        let is_valid_key = self.is_key_in_mapped_constraint(constraint, prop_name);

        if !is_valid_key {
            // Property not in constraint - check if there's a string index signature
            if self.mapped_has_string_index(&mapped) {
                // Has string index - property access is valid
            } else {
                return Some(PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().mapped(mapped.as_ref().clone()),
                    property_name: prop_atom,
                });
            }
        }

        // Step 2: Create a substitution for just this property
        let key_literal = self
            .interner()
            .intern(TypeKey::Literal(LiteralValue::String(prop_atom)));

        // Handle name remapping if present (e.g., `as` clause in mapped types)
        if let Some(name_type) = mapped.name_type {
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);
            let remapped = instantiate_type(self.interner(), name_type, &subst);
            let remapped = self
                .db
                .evaluate_type_with_options(remapped, self.no_unchecked_indexed_access);
            if remapped == TypeId::NEVER {
                // Key is filtered out by `as never`
                return Some(PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().mapped(mapped.as_ref().clone()),
                    property_name: prop_atom,
                });
            }
        }

        // Step 3: Instantiate the template with this single key
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, key_literal);
        let property_type = instantiate_type(self.interner(), mapped.template, &subst);
        let property_type = self
            .db
            .evaluate_type_with_options(property_type, self.no_unchecked_indexed_access);

        // Step 4: Apply optional modifier
        let final_type = match mapped.optional_modifier {
            Some(MappedModifier::Add) => self.interner().union2(property_type, TypeId::UNDEFINED),
            Some(MappedModifier::Remove) => property_type,
            None => property_type,
        };

        Some(PropertyAccessResult::Success {
            type_id: final_type,
            from_index_signature: false,
        })
    }

    /// Handle array method access on mapped types applied to array-like sources.
    ///
    /// When a mapped type like `{ [P in keyof T]: F<T[P]> }` is applied to an array type,
    /// TypeScript preserves array methods (pop, push, concat, etc.) from the resulting
    /// array type rather than mapping them through the template.
    ///
    /// Returns `Some(result)` if this is an array method on a mapped array type,
    /// `None` otherwise.
    fn resolve_array_mapped_type_method(
        &self,
        mapped: &MappedType,
        _mapped_id: MappedTypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        use crate::types::TypeKey;

        // Only handle non-numeric property names (array methods)
        // Numeric properties should go through normal template mapping
        if prop_name.parse::<usize>().is_ok() {
            return None;
        }

        // Check if constraint is `keyof T` where T might be array-like
        let source_type = self.get_homomorphic_source(mapped)?;

        // Check if source type is array-like (array, tuple, or type param with array constraint)
        if !self.is_array_like_type(source_type) {
            return None;
        }

        // For array methods, we need to:
        // 1. Compute the mapped element type: F<T[number]>
        // 2. Create Array<mapped_element>
        // 3. Resolve the property on that array type

        // Get the element type mapping: instantiate template with `number` as the key
        let number_type = TypeId::NUMBER;
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, number_type);
        let mapped_element = instantiate_type(self.interner(), mapped.template, &subst);
        let mapped_element = self
            .db
            .evaluate_type_with_options(mapped_element, self.no_unchecked_indexed_access);

        // Create the resulting array type
        let array_type = self.interner().intern(TypeKey::Array(mapped_element));

        // Resolve the property on the array type
        let result = self.resolve_array_property(array_type, prop_name, prop_atom);

        // If property not found on array, return None to fall through to normal handling
        if matches!(result, PropertyAccessResult::PropertyNotFound { .. }) {
            return None;
        }

        Some(result)
    }

    /// Get the homomorphic source type for a mapped type.
    ///
    /// For a mapped type like `{ [P in keyof T]: ... }`, returns `T`.
    /// Returns `None` if the mapped type is not homomorphic.
    fn get_homomorphic_source(&self, mapped: &MappedType) -> Option<TypeId> {
        use crate::types::TypeKey;

        // Check if constraint is `keyof T`
        if let Some(TypeKey::KeyOf(source)) = self.interner().lookup(mapped.constraint) {
            return Some(source);
        }

        None
    }

    /// Check if a type is array-like (array, tuple, or type parameter constrained to array).
    fn is_array_like_type(&self, type_id: TypeId) -> bool {
        use crate::types::TypeKey;

        match self.interner().lookup(type_id) {
            Some(TypeKey::Array(_)) => true,
            Some(TypeKey::Tuple(_)) => true,
            Some(TypeKey::TypeParameter(info)) => {
                // Check if the type parameter has an array-like constraint
                if let Some(constraint) = info.constraint {
                    self.is_array_like_type(constraint)
                } else {
                    false
                }
            }
            Some(TypeKey::ReadonlyType(inner)) => self.is_array_like_type(inner),
            // Also check for union types where all members are array-like
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                !members.is_empty() && members.iter().all(|&m| self.is_array_like_type(m))
            }
            Some(TypeKey::Intersection(members)) => {
                // For intersection, at least one member should be array-like
                let members = self.interner().type_list(members);
                members.iter().any(|&m| self.is_array_like_type(m))
            }
            _ => false,
        }
    }

    /// Check if a property name is valid in a mapped type's constraint.
    fn is_key_in_mapped_constraint(&self, constraint: TypeId, prop_name: &str) -> bool {
        use crate::types::{LiteralValue, TypeKey};

        // Evaluate the constraint to try to reduce it
        let evaluated = self
            .db
            .evaluate_type_with_options(constraint, self.no_unchecked_indexed_access);

        let Some(key) = self.interner().lookup(evaluated) else {
            return false;
        };

        match key {
            // Single string literal - exact match
            TypeKey::Literal(LiteralValue::String(s)) => {
                self.interner().resolve_atom(s) == prop_name
            }

            // Union of literals - check if prop_name is in the union
            TypeKey::Union(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        // string index covers all string properties
                        return true;
                    }
                    // Recursively check each union member
                    if self.is_key_in_mapped_constraint(member, prop_name) {
                        return true;
                    }
                }
                false
            }

            // Intersection - key must be valid in ALL members
            TypeKey::Intersection(members) => {
                let members = self.interner().type_list(members);
                // For intersection of key types, a key is valid if it's in the intersection
                // This is conservative - we check if it might be valid
                members
                    .iter()
                    .any(|&m| self.is_key_in_mapped_constraint(m, prop_name))
            }

            // string type covers all string properties
            TypeKey::Intrinsic(crate::types::IntrinsicKind::String) => true,

            // KeyOf that couldn't be fully evaluated - be permissive
            // This handles cases like `keyof T` where T is a type parameter
            TypeKey::KeyOf(_) => true,

            // Type parameters - we can't know the keys statically, be permissive
            TypeKey::TypeParameter(_) => true,

            // Conditional types - try to evaluate them
            // If evaluation didn't reduce it, be permissive as we can't know statically
            TypeKey::Conditional(_) => true,

            // Application types that didn't fully evaluate - be permissive
            TypeKey::Application(_) => true,

            // Infer types in conditional context - be permissive
            TypeKey::Infer(_) => true,

            // Other types - be conservative and reject
            _ => false,
        }
    }

    /// Check if a mapped type has a string index signature (constraint includes `string`).
    fn mapped_has_string_index(&self, mapped: &MappedType) -> bool {
        use crate::types::{IntrinsicKind, TypeKey};

        let constraint = mapped.constraint;

        // Evaluate keyof if needed
        let evaluated = if let Some(TypeKey::KeyOf(operand)) = self.interner().lookup(constraint) {
            let keyof_type = self.interner().intern(TypeKey::KeyOf(operand));
            self.db
                .evaluate_type_with_options(keyof_type, self.no_unchecked_indexed_access)
        } else {
            constraint
        };

        if evaluated == TypeId::STRING {
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.interner().lookup(evaluated) {
            let members = self.interner().type_list(members);
            for &member in members.iter() {
                if member == TypeId::STRING {
                    return true;
                }
                if let Some(TypeKey::Intrinsic(IntrinsicKind::String)) =
                    self.interner().lookup(member)
                {
                    return true;
                }
            }
        }

        if let Some(TypeKey::Intrinsic(IntrinsicKind::String)) = self.interner().lookup(evaluated) {
            return true;
        }

        false
    }

    /// Check if a property name is a private field (starts with #)
    #[allow(dead_code)] // Infrastructure for private field checking
    fn is_private_field(&self, prop_name: &str) -> bool {
        prop_name.starts_with('#')
    }

    fn resolve_property_access_inner(
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
                TypeKey::Intrinsic(kind) => {
                    // Inline visitor logic for intrinsics
                    match kind {
                        IntrinsicKind::Any => Some(PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
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
                TypeKey::Object(shape_id) => {
                    // Inline visitor logic for Object - calls visit_object implementation
                    self.visit_object_impl(shape_id.0, prop_name, prop_atom)
                }
                TypeKey::ObjectWithIndex(shape_id) => {
                    // Inline visitor logic for ObjectWithIndex - calls visit_object_with_index implementation
                    self.visit_object_with_index_impl(shape_id.0, prop_name, prop_atom)
                }
                TypeKey::Array(_elem) => {
                    // Inline visitor logic for Array
                    self.visit_array_impl(obj_type, prop_name, prop_atom)
                }
                TypeKey::Tuple(_list_id) => {
                    // Inline visitor logic for Tuple
                    self.visit_array_impl(obj_type, prop_name, prop_atom)
                }
                TypeKey::Union(list_id) => {
                    // Inline visitor logic for Union - calls visit_union implementation
                    self.visit_union_impl(list_id.0, prop_name, prop_atom)
                }
                // Note: TypeKey::Application is handled in the fallback section with proper type substitution
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
            TypeKey::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(prop),
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
                if resolver.has_index_signature(obj_type, IndexKind::String) {
                    if let Some(value_type) = resolver.resolve_string_index(obj_type) {
                        return PropertyAccessResult::Success {
                            type_id: self.add_undefined_if_unchecked(value_type),
                            from_index_signature: true,
                        };
                    }
                }

                // Try numeric index signature if property name looks numeric
                if resolver.is_numeric_index_name(prop_name) {
                    if let Some(value_type) = resolver.resolve_number_index(obj_type) {
                        return PropertyAccessResult::Success {
                            type_id: self.add_undefined_if_unchecked(value_type),
                            from_index_signature: true,
                        };
                    }
                }

                PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                }
            }

            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(prop),
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
                        from_index_signature: true,
                    };
                }

                // Check numeric index signature if property name looks numeric
                use crate::index_signatures::IndexSignatureResolver;
                let resolver = IndexSignatureResolver::new(self.interner());
                if resolver.is_numeric_index_name(prop_name) {
                    if let Some(ref idx) = shape.number_index {
                        return PropertyAccessResult::Success {
                            type_id: self.add_undefined_if_unchecked(idx.value_type),
                            from_index_signature: true,
                        };
                    }
                }

                PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                }
            }

            TypeKey::Function(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                for prop in &shape.properties {
                    if prop.name == prop_atom {
                        return PropertyAccessResult::Success {
                            type_id: self.optional_property_type(prop),
                            from_index_signature: false,
                        };
                    }
                }
                // Check string index signature (for static index signatures on class constructors)
                if let Some(ref idx) = shape.string_index {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(idx.value_type),
                        from_index_signature: true,
                    };
                }
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            // TypeKey::Union - migrated to visitor pattern (see visit_union_impl)
            // This should never be reached since visitor handles it above
            TypeKey::Intersection(members) => {
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
                        if resolver.has_index_signature(member, IndexKind::String) {
                            if let Some(value_type) = resolver.resolve_string_index(member) {
                                return PropertyAccessResult::Success {
                                    type_id: self.add_undefined_if_unchecked(value_type),
                                    from_index_signature: true,
                                };
                            }
                        }
                    }

                    // Check numeric index signature if property name looks numeric
                    if resolver.is_numeric_index_name(prop_name) {
                        for &member in members.iter() {
                            if let Some(value_type) = resolver.resolve_number_index(member) {
                                return PropertyAccessResult::Success {
                                    type_id: self.add_undefined_if_unchecked(value_type),
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
                    from_index_signature: any_from_index,
                }
            }

            TypeKey::ReadonlyType(inner) => {
                self.resolve_property_access_inner(inner, prop_name, prop_atom)
            }

            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
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
            TypeKey::Literal(ref literal) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                match literal {
                    LiteralValue::String(_) => self.resolve_string_property(prop_name, prop_atom),
                    LiteralValue::Number(_) => self.resolve_number_property(prop_name, prop_atom),
                    LiteralValue::Boolean(_) => self.resolve_boolean_property(prop_name, prop_atom),
                    LiteralValue::BigInt(_) => self.resolve_bigint_property(prop_name, prop_atom),
                }
            }

            TypeKey::TemplateLiteral(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            // Built-in properties
            TypeKey::Intrinsic(IntrinsicKind::String) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Number) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_number_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Boolean) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_boolean_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Bigint) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_bigint_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Object) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    },
                )
            }

            TypeKey::Array(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Tuple(_) => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            // Application: handle nominally (preserve class/interface identity)
            TypeKey::Application(app_id) => {
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
            TypeKey::Mapped(mapped_id) => {
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
                            from_index_signature: false,
                        }
                    }
                }
            }

            // TypeQuery types: typeof queries that need resolution to their structural form
            TypeKey::TypeQuery(_) => {
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
                            from_index_signature: false,
                        }
                    }
                }
            }

            // Conditional types need evaluation to their resolved form
            TypeKey::Conditional(_) => {
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
                            from_index_signature: false,
                        }
                    }
                }
            }

            // Index access types need evaluation
            TypeKey::IndexAccess(_, _) => {
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
                            from_index_signature: false,
                        }
                    }
                }
            }

            // KeyOf types need evaluation
            TypeKey::KeyOf(_) => {
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
            TypeKey::ThisType => {
                let prop_atom =
                    prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                // 'this' type not resolved - return ANY to avoid false positives
                PropertyAccessResult::Success {
                    type_id: TypeId::ANY,
                    from_index_signature: false,
                }
            }

            // Lazy types (interfaces, classes, type aliases) need resolution
            TypeKey::Lazy(def_id) => {
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
                            from_index_signature: false,
                        }
                    }
                }
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
                    from_index_signature: false,
                }
            }
        }
    }

    fn lookup_object_property<'props>(
        &self,
        shape_id: ObjectShapeId,
        props: &'props [PropertyInfo],
        prop_atom: Atom,
    ) -> Option<&'props PropertyInfo> {
        match self.interner().object_property_index(shape_id, prop_atom) {
            PropertyLookup::Found(idx) => props.get(idx),
            PropertyLookup::NotFound => None,
            PropertyLookup::Uncached => props.iter().find(|p| p.name == prop_atom),
        }
    }

    fn any_args_function(&self, return_type: TypeId) -> TypeId {
        let rest_array = self.interner().array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.interner().function(FunctionShape {
            params: vec![rest_param],
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn method_result(&self, return_type: TypeId) -> PropertyAccessResult {
        PropertyAccessResult::Success {
            type_id: self.any_args_function(return_type),
            from_index_signature: false,
        }
    }

    /// Resolve property access on a generic Application type (e.g., `D<string>`) nominally.
    ///
    /// This preserves nominal identity for classes/interfaces instead of structurally
    /// expanding them. The key difference:
    /// - Type aliases: expand structurally (transparent)
    /// - Classes/Interfaces: preserve nominal identity (opaque)
    ///
    /// For `D<string>.a`:
    /// 1. Get Application's base (D) and args ([string])
    /// 2. Resolve base to get its body (Object with properties)
    /// 3. Find property 'a' in the body
    /// 4. Instantiate property type T with arg string -> string
    /// 5. Return instantiated property type (NOT the full structurally expanded type)
    fn resolve_application_property(
        &self,
        app_id: TypeApplicationId,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> PropertyAccessResult {
        let app = self.interner().type_application(app_id);
        let prop_atom = prop_atom.unwrap_or_else(|| self.interner().intern_string(prop_name));

        // Get the base type (should be a Ref to class/interface/alias)
        let base_key = match self.interner().lookup(app.base) {
            Some(k) => k,
            None => {
                return PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().application(app.base, app.args.clone()),
                    property_name: prop_atom,
                };
            }
        };

        // Handle Object types (e.g., test array interface setup)
        if let TypeKey::Object(shape_id) = base_key {
            let shape = self.interner().object_shape(shape_id);

            // Try to find the property in the Object's properties
            if let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom) {
                // Get type params from the array base type (stored during test setup)
                let type_params = self.db.get_array_base_type_params();

                if type_params.is_empty() {
                    // No type params available, return the property type as-is
                    return PropertyAccessResult::Success {
                        type_id: prop.type_id,
                        from_index_signature: false,
                    };
                }

                // Create substitution: map type params to application args
                let substitution =
                    TypeSubstitution::from_args(self.interner(), type_params, &app.args);

                // Instantiate the property type with substitution
                use crate::instantiate::instantiate_type_with_infer;
                let instantiated_prop_type =
                    instantiate_type_with_infer(self.interner(), prop.type_id, &substitution);

                // Handle `this` types
                let app_type = self.interner().application(app.base, app.args.clone());
                use crate::instantiate::substitute_this_type;
                let final_type =
                    substitute_this_type(self.interner(), instantiated_prop_type, app_type);

                return PropertyAccessResult::Success {
                    type_id: final_type,
                    from_index_signature: false,
                };
            }

            return PropertyAccessResult::PropertyNotFound {
                type_id: self.interner().application(app.base, app.args.clone()),
                property_name: prop_atom,
            };
        }

        // Handle ObjectWithIndex types
        if let TypeKey::ObjectWithIndex(shape_id) = base_key {
            let shape = self.interner().object_shape(ObjectShapeId(shape_id.0));

            // Try to find the property in the ObjectWithIndex's properties
            if let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom) {
                // Get type params
                let type_params = self.db.get_array_base_type_params();

                if type_params.is_empty() {
                    return PropertyAccessResult::Success {
                        type_id: prop.type_id,
                        from_index_signature: false,
                    };
                }

                let substitution =
                    TypeSubstitution::from_args(self.interner(), type_params, &app.args);

                use crate::instantiate::instantiate_type_with_infer;
                let instantiated_prop_type =
                    instantiate_type_with_infer(self.interner(), prop.type_id, &substitution);

                let app_type = self.interner().application(app.base, app.args.clone());
                use crate::instantiate::substitute_this_type;
                let final_type =
                    substitute_this_type(self.interner(), instantiated_prop_type, app_type);

                return PropertyAccessResult::Success {
                    type_id: final_type,
                    from_index_signature: false,
                };
            }

            // Check index signatures if property not found
            if let Some(ref idx) = shape.string_index {
                return PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(idx.value_type),
                    from_index_signature: true,
                };
            }

            return PropertyAccessResult::PropertyNotFound {
                type_id: self.interner().application(app.base, app.args.clone()),
                property_name: prop_atom,
            };
        }

        // Handle Callable types (e.g., Array constructor with instance methods as properties)
        if let TypeKey::Callable(shape_id) = base_key {
            let shape = self.interner().callable_shape(shape_id);

            // Try to find the property in the Callable's properties
            if let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom) {
                // For Callable properties, we need to substitute type parameters
                // The Array Callable has properties that reference the type parameter T
                // We need to substitute T with the element_type from app.args[0]

                // Create substitution: map the Callable's type parameters to the application's arguments
                // For Array, this means T -> element_type
                let type_params = self.db.get_array_base_type_params();

                if type_params.is_empty() {
                    // No type params available, return the property type as-is
                    return PropertyAccessResult::Success {
                        type_id: prop.type_id,
                        from_index_signature: false,
                    };
                }

                // Task 2.2: Lazy Member Instantiation
                // Instantiate ONLY the property type, not the entire Callable
                // This avoids recursion into other 37+ Array methods
                let substitution =
                    TypeSubstitution::from_args(self.interner(), type_params, &app.args);

                // Use instantiate_type_infer to handle infer vars and avoid depth issues
                use crate::instantiate::instantiate_type_with_infer;
                let instantiated_prop_type =
                    instantiate_type_with_infer(self.interner(), prop.type_id, &substitution);

                // Task 2.3: Handle `this` Types
                // Array methods may return `this` or `this[]` which need to be
                // substituted with the actual Application type (e.g., `T[]`)
                let app_type = self.interner().application(app.base, app.args.clone());

                use crate::instantiate::substitute_this_type;
                let final_type =
                    substitute_this_type(self.interner(), instantiated_prop_type, app_type);

                return PropertyAccessResult::Success {
                    type_id: final_type,
                    from_index_signature: false,
                };
            }

            return PropertyAccessResult::PropertyNotFound {
                type_id: self.interner().application(app.base, app.args.clone()),
                property_name: prop_atom,
            };
        }

        // We only handle Lazy types (def_id references)
        let TypeKey::Lazy(def_id) = base_key else {
            // For non-Lazy bases (e.g., TypeParameter), fall back to structural evaluation
            let evaluated = self.db.evaluate_type_with_options(
                self.interner().application(app.base, app.args.clone()),
                self.no_unchecked_indexed_access,
            );
            return self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom));
        };

        // Resolve the def_id to get the SymbolId, then get the body type
        let sym_id = match self.db.def_to_symbol_id(def_id) {
            Some(id) => id,
            None => {
                // Can't convert def_id to symbol_id - fall back to structural evaluation
                let evaluated = self.db.evaluate_type_with_options(
                    self.interner().application(app.base, app.args.clone()),
                    self.no_unchecked_indexed_access,
                );
                return self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom));
            }
        };

        let symbol_ref = crate::SymbolRef(sym_id.0);

        // Resolve the symbol to get its body type
        let body_type = if let Some(inner_def_id) = self.db.symbol_to_def_id(symbol_ref) {
            self.db.resolve_lazy(inner_def_id, self.interner())
        } else {
            #[allow(deprecated)]
            let r = self.db.resolve_ref(symbol_ref, self.interner());
            r
        };

        let Some(body_type) = body_type else {
            // Resolution failed - fall back to structural evaluation
            let evaluated = self.db.evaluate_type_with_options(
                self.interner().application(app.base, app.args.clone()),
                self.no_unchecked_indexed_access,
            );
            return self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom));
        };

        // Get type parameters for this symbol
        let type_params = match self.db.get_type_params(symbol_ref) {
            Some(params) if !params.is_empty() => params,
            _ => {
                // No type params - resolve on the body directly
                return self.resolve_property_access_inner(body_type, prop_name, Some(prop_atom));
            }
        };

        // The body should be an Object type with properties
        let body_key = match self.interner().lookup(body_type) {
            Some(k) => k,
            None => {
                return PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().application(app.base, app.args.clone()),
                    property_name: prop_atom,
                };
            }
        };

        // Handle Object types (classes/interfaces)
        match body_key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);

                // Try to find the property in the shape
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    // Found! Now instantiate the property type with the type arguments
                    let substitution =
                        TypeSubstitution::from_args(self.interner(), &type_params, &app.args);

                    // Instantiate both read and write types
                    let instantiated_read_type =
                        instantiate_type(self.interner(), prop.type_id, &substitution);
                    let instantiated_write_type =
                        instantiate_type(self.interner(), prop.write_type, &substitution);

                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(&PropertyInfo {
                            name: prop.name,
                            type_id: instantiated_read_type,
                            write_type: instantiated_write_type,
                            readonly: prop.readonly,
                            optional: prop.optional,
                            is_method: prop.is_method,
                            visibility: prop.visibility,
                            parent_id: prop.parent_id,
                        }),
                        from_index_signature: false,
                    };
                }

                // Property not found in explicit properties - check index signatures
                if let Some(ref idx) = shape.string_index {
                    // Found string index signature - instantiate the value type
                    let substitution =
                        TypeSubstitution::from_args(self.interner(), &type_params, &app.args);
                    let instantiated_value =
                        instantiate_type(self.interner(), idx.value_type, &substitution);

                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(instantiated_value),
                        from_index_signature: true,
                    };
                }

                // Check numeric index signature for numeric property names
                use crate::index_signatures::IndexSignatureResolver;
                let resolver = IndexSignatureResolver::new(self.interner());
                if resolver.is_numeric_index_name(prop_name) {
                    if let Some(ref idx) = shape.number_index {
                        let substitution =
                            TypeSubstitution::from_args(self.interner(), &type_params, &app.args);
                        let instantiated_value =
                            instantiate_type(self.interner(), idx.value_type, &substitution);

                        return PropertyAccessResult::Success {
                            type_id: self.add_undefined_if_unchecked(instantiated_value),
                            from_index_signature: true,
                        };
                    }
                }

                // Property not found
                PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner().application(app.base, app.args.clone()),
                    property_name: prop_atom,
                }
            }
            // For non-Object body types (e.g., type aliases to unions), fall back to evaluation
            _ => {
                let evaluated = self.db.evaluate_type_with_options(
                    self.interner().application(app.base, app.args.clone()),
                    self.no_unchecked_indexed_access,
                );
                self.resolve_property_access_inner(evaluated, prop_name, Some(prop_atom))
            }
        }
    }

    fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner().union2(type_id, TypeId::UNDEFINED)
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner().union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn resolve_apparent_property(
        &self,
        kind: IntrinsicKind,
        owner_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        match apparent_primitive_member_kind(self.interner(), kind, prop_name) {
            Some(ApparentMemberKind::Value(type_id)) => PropertyAccessResult::Success {
                type_id,
                from_index_signature: false,
            },
            Some(ApparentMemberKind::Method(return_type)) => self.method_result(return_type),
            None => PropertyAccessResult::PropertyNotFound {
                type_id: owner_type,
                property_name: prop_atom,
            },
        }
    }

    fn resolve_object_member(
        &self,
        prop_name: &str,
        _prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        match apparent_object_member_kind(prop_name) {
            Some(ApparentMemberKind::Value(type_id)) => Some(PropertyAccessResult::Success {
                type_id,
                from_index_signature: false,
            }),
            Some(ApparentMemberKind::Method(return_type)) => Some(self.method_result(return_type)),
            None => None,
        }
    }

    /// Resolve properties on string type.
    fn resolve_string_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::String, TypeId::STRING, prop_name, prop_atom)
    }

    /// Resolve properties on number type.
    fn resolve_number_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::Number, TypeId::NUMBER, prop_name, prop_atom)
    }

    /// Resolve properties on boolean type.
    fn resolve_boolean_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(
            IntrinsicKind::Boolean,
            TypeId::BOOLEAN,
            prop_name,
            prop_atom,
        )
    }

    /// Resolve properties on bigint type.
    fn resolve_bigint_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::Bigint, TypeId::BIGINT, prop_name, prop_atom)
    }

    /// Helper to resolve properties on primitive types.
    /// Extracted to reduce duplication across string/number/boolean/bigint property resolvers.
    fn resolve_primitive_property(
        &self,
        kind: IntrinsicKind,
        type_id: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        // STEP 1: Try to get the boxed interface type from the resolver (e.g. Number for number)
        // This allows us to use lib.d.ts definitions instead of just hardcoded lists
        if let Some(boxed_type) = self.db.get_boxed_type(kind) {
            // Resolve the property on the boxed interface type
            // This handles inheritance (e.g., String extends Object) automatically
            // and allows user-defined augmentations to lib.d.ts to work
            let result = self.resolve_property_access_inner(boxed_type, prop_name, Some(prop_atom));

            // If the property was found (or we got a definitive answer like IsUnknown), return it.
            // Only fall back if the property was NOT found on the boxed type.
            // This ensures that if the environment defines the interface but is incomplete
            // (e.g., during bootstrapping or partial lib loading), we still find the intrinsic methods.
            if !matches!(result, PropertyAccessResult::PropertyNotFound { .. }) {
                return result;
            }
        }

        // STEP 2: Fallback to hardcoded apparent members (bootstrapping/no-lib behavior)
        self.resolve_apparent_property(kind, type_id, prop_name, prop_atom)
    }

    /// Resolve properties on symbol primitive type.
    fn resolve_symbol_primitive_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        if prop_name == "toString" || prop_name == "valueOf" {
            return PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            };
        }

        self.resolve_apparent_property(IntrinsicKind::Symbol, TypeId::SYMBOL, prop_name, prop_atom)
    }

    /// Resolve properties on array type.
    ///
    /// Uses the Array<T> interface from lib.d.ts to resolve array methods.
    /// Falls back to numeric index signature for numeric property names.
    fn resolve_array_property(
        &self,
        array_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        let element_type = self.array_element_type(array_type);

        // Try to use the Array<T> interface from lib.d.ts
        let array_base = self.db.get_array_base_type();

        if let Some(array_base) = array_base {
            // Create TypeApplication: Array<element_type>
            // This triggers resolve_application_property which handles substitution correctly
            let app_type = self.interner().application(array_base, vec![element_type]);

            // Resolve property on the application type
            let result = self.resolve_property_access_inner(app_type, prop_name, Some(prop_atom));

            // If we found the property, return it
            if !matches!(result, PropertyAccessResult::PropertyNotFound { .. }) {
                return result;
            }
        }

        // Handle numeric index access (e.g., arr[0], arr["0"])
        use crate::index_signatures::IndexSignatureResolver;
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name) {
            let element_or_undefined = self.element_type_with_undefined(element_type);
            return PropertyAccessResult::Success {
                type_id: element_or_undefined,
                from_index_signature: true,
            };
        }

        // Property not found
        PropertyAccessResult::PropertyNotFound {
            type_id: array_type,
            property_name: prop_atom,
        }
    }

    pub(crate) fn array_element_type(&self, array_type: TypeId) -> TypeId {
        match self.interner().lookup(array_type) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                self.tuple_element_union(&elements)
            }
            _ => TypeId::ERROR, // Return ERROR instead of ANY for non-array/tuple types
        }
    }

    fn tuple_element_union(&self, elements: &[TupleElement]) -> TypeId {
        let mut members = Vec::new();
        for elem in elements {
            let mut ty = if elem.rest {
                self.array_element_type(elem.type_id)
            } else {
                elem.type_id
            };
            if elem.optional {
                ty = self.element_type_with_undefined(ty);
            }
            members.push(ty);
        }
        self.interner().union(members)
    }

    fn element_type_with_undefined(&self, element_type: TypeId) -> TypeId {
        self.interner().union2(element_type, TypeId::UNDEFINED)
    }

    fn resolve_function_property(
        &self,
        func_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        match prop_name {
            "apply" | "call" | "bind" => self.method_result(TypeId::ANY),
            "toString" => self.method_result(TypeId::STRING),
            "length" => PropertyAccessResult::Success {
                type_id: TypeId::NUMBER,
                from_index_signature: false,
            },
            "prototype" | "arguments" => PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            },
            "caller" => PropertyAccessResult::Success {
                type_id: self.any_args_function(TypeId::ANY),
                from_index_signature: false,
            },
            _ => {
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                PropertyAccessResult::PropertyNotFound {
                    type_id: func_type,
                    property_name: prop_atom,
                }
            }
        }
    }
}

pub fn property_is_readonly(interner: &dyn TypeDatabase, type_id: TypeId, prop_name: &str) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::Lazy(_)) => {
            // Resolve lazy types (interfaces, classes, type aliases) before checking readonly
            // Note: This function uses NoopResolver, which is correct for the readonly check
            // The readonly metadata is stored in the type itself, not dependent on resolution
            let resolved = evaluate_type(interner, type_id);
            property_is_readonly(interner, resolved, prop_name)
        }
        Some(TypeKey::ReadonlyType(inner)) => {
            if let Some(TypeKey::Array(_) | TypeKey::Tuple(_)) = interner.lookup(inner)
                && is_numeric_index_name(prop_name)
            {
                return true;
            }
            property_is_readonly(interner, inner, prop_name)
        }
        Some(TypeKey::Object(shape_id)) => {
            tracing::trace!(
                "property_is_readonly: Object shape {:?} for prop {}",
                shape_id,
                prop_name
            );
            let result = object_property_is_readonly(interner, shape_id, prop_name);
            tracing::trace!("property_is_readonly: Object result = {}", result);
            result
        }
        Some(TypeKey::ObjectWithIndex(shape_id)) => {
            indexed_object_property_is_readonly(interner, shape_id, prop_name)
        }
        Some(TypeKey::Union(types)) => {
            // For unions: property is readonly if it's readonly in ANY constituent type
            let types = interner.type_list(types);
            types
                .iter()
                .any(|t| property_is_readonly(interner, *t, prop_name))
        }
        Some(TypeKey::Intersection(types)) => {
            // For intersections: property is readonly ONLY if it's readonly in ALL constituent types
            // This allows assignment to `{ readonly a: number } & { a: number }` (mixed readonly/mutable)
            let types = interner.type_list(types);
            types
                .iter()
                .all(|t| property_is_readonly(interner, *t, prop_name))
        }
        _ => false,
    }
}

/// Check if a property on a plain object type is readonly.
fn object_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.object_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);
    shape
        .properties
        .iter()
        .find(|prop| prop.name == prop_atom)
        .is_some_and(|prop| prop.readonly)
}

/// Check if a property on an indexed object type is readonly.
/// Checks both named properties and index signatures.
fn indexed_object_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.object_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);

    // Check named property first
    if let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_atom) {
        return prop.readonly;
    }

    // Check string index signature for ALL property names
    if shape.string_index.as_ref().is_some_and(|idx| idx.readonly) {
        return true;
    }

    // Check numeric index signature for numeric properties
    if is_numeric_index_name(prop_name)
        && shape.number_index.as_ref().is_some_and(|idx| idx.readonly)
    {
        return true;
    }

    false
}

/// Check if an index signature is readonly for the given type.
///
/// # Parameters
/// - `wants_string`: Check if string index signature should be readonly
/// - `wants_number`: Check if numeric index signature should be readonly
///
/// # Returns
/// `true` if the requested index signature is readonly, `false` otherwise.
///
/// # Examples
/// - `{ readonly [x: string]: string }` → `is_readonly_index_signature(t, true, false)` = `true`
/// - `{ [x: string]: string }` → `is_readonly_index_signature(t, true, false)` = `false`
pub fn is_readonly_index_signature(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    wants_string: bool,
    wants_number: bool,
) -> bool {
    use crate::index_signatures::{IndexKind, IndexSignatureResolver};

    let resolver = IndexSignatureResolver::new(interner);

    if wants_string && resolver.is_readonly(type_id, IndexKind::String) {
        return true;
    }

    if wants_number && resolver.is_readonly(type_id, IndexKind::Number) {
        return true;
    }

    false
}

/// Check if a string represents a valid numeric property name.
///
/// Returns `true` only for non-negative finite integers that round-trip correctly
/// through JavaScript's `Number.toString()` conversion.
///
/// This is used for determining if a property access can use numeric index signatures:
/// - `"0"` through `"4294967295"` are valid numeric property names (fits in usize)
/// - `"1.5"`, `"-1"`, `NaN`, `Infinity` are NOT valid numeric property names
///
/// # Examples
/// - `is_numeric_index_name("0")` → `true`
/// - `is_numeric_index_name("42")` → `true`
/// - `is_numeric_index_name("1.5")` → `false` (fractional part)
/// - `is_numeric_index_name("-1")` → `false` (negative)
/// - `is_numeric_index_name("NaN")` → `false` (special value)
fn is_numeric_index_name(name: &str) -> bool {
    let parsed: f64 = match name.parse() {
        Ok(value) => value,
        Err(_) => return false,
    };
    if !parsed.is_finite() || parsed.fract() != 0.0 || parsed < 0.0 {
        return false;
    }
    parsed <= (usize::MAX as f64)
}

// =============================================================================
// Binary Operations - Extracted to binary_ops.rs
// =============================================================================
//
// Binary operation evaluation has been extracted to `solver/binary_ops.rs`.
// The following are re-exported from that module:
// - BinaryOpEvaluator
// - BinaryOpResult
// - PrimitiveClass
//
// This extraction reduces operations.rs by ~330 lines and makes the code
// more maintainable by separating concerns.
