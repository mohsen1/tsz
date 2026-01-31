//! Property Access Resolution
//!
//! This module contains the PropertyAccessEvaluator and related types for
//! resolving property access on types (obj.prop, obj["key"], etc.).
//!
//! Extracted from operations.rs to keep file sizes manageable.

use crate::interner::Atom;
use crate::solver::evaluate::evaluate_type;
use crate::solver::instantiate::{TypeSubstitution, instantiate_type};
use crate::solver::subtype::{NoopResolver, TypeResolver};
use crate::solver::types::*;
use crate::solver::{
    ApparentMemberKind, TypeDatabase, apparent_object_member_kind, apparent_primitive_member_kind,
};
use rustc_hash::FxHashSet;
use std::cell::RefCell;

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
pub struct PropertyAccessEvaluator<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    mapped_access_visiting: RefCell<FxHashSet<TypeId>>,
    mapped_access_depth: RefCell<u32>,
}

struct MappedAccessGuard<'a, R: TypeResolver> {
    evaluator: &'a PropertyAccessEvaluator<'a, R>,
    obj_type: TypeId,
}

impl<'a, R: TypeResolver> Drop for MappedAccessGuard<'a, R> {
    fn drop(&mut self) {
        self.evaluator
            .mapped_access_visiting
            .borrow_mut()
            .remove(&self.obj_type);
        *self.evaluator.mapped_access_depth.borrow_mut() -= 1;
    }
}

impl<'a> PropertyAccessEvaluator<'a, NoopResolver> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        PropertyAccessEvaluator {
            interner,
            resolver: &NoopResolver,
            no_unchecked_indexed_access: false,
            mapped_access_visiting: RefCell::new(FxHashSet::default()),
            mapped_access_depth: RefCell::new(0),
        }
    }
}

impl<'a, R: TypeResolver> PropertyAccessEvaluator<'a, R> {
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        PropertyAccessEvaluator {
            interner,
            resolver,
            no_unchecked_indexed_access: false,
            mapped_access_visiting: RefCell::new(FxHashSet::default()),
            mapped_access_depth: RefCell::new(0),
        }
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    /// Resolve property access: obj.prop -> type
    pub fn resolve_property_access(
        &self,
        obj_type: TypeId,
        prop_name: &str,
    ) -> PropertyAccessResult {
        self.resolve_property_access_inner(obj_type, prop_name, None)
    }

    fn enter_mapped_access_guard(&self, obj_type: TypeId) -> Option<MappedAccessGuard<'_, R>> {
        const MAX_MAPPED_ACCESS_DEPTH: u32 = 50;

        let mut depth = self.mapped_access_depth.borrow_mut();
        if *depth >= MAX_MAPPED_ACCESS_DEPTH {
            return None;
        }
        *depth += 1;
        drop(depth);

        let mut visiting = self.mapped_access_visiting.borrow_mut();
        if !visiting.insert(obj_type) {
            drop(visiting);
            *self.mapped_access_depth.borrow_mut() -= 1;
            return None;
        }

        Some(MappedAccessGuard {
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
        use crate::solver::types::{LiteralValue, MappedModifier, TypeKey};

        let mapped = self.interner.mapped_type(mapped_id);

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
                    type_id: self.interner.mapped(mapped.as_ref().clone()),
                    property_name: prop_atom,
                });
            }
        }

        // Step 2: Create a substitution for just this property
        let key_literal = self
            .interner
            .intern(TypeKey::Literal(LiteralValue::String(prop_atom)));

        // Handle name remapping if present (e.g., `as` clause in mapped types)
        if let Some(name_type) = mapped.name_type {
            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);
            let remapped = instantiate_type(self.interner, name_type, &subst);
            let remapped = evaluate_type(self.interner, remapped);
            if remapped == TypeId::NEVER {
                // Key is filtered out by `as never`
                return Some(PropertyAccessResult::PropertyNotFound {
                    type_id: self.interner.mapped(mapped.as_ref().clone()),
                    property_name: prop_atom,
                });
            }
        }

        // Step 3: Instantiate the template with this single key
        let mut subst = TypeSubstitution::new();
        subst.insert(mapped.type_param.name, key_literal);
        let property_type = instantiate_type(self.interner, mapped.template, &subst);
        let property_type = evaluate_type(self.interner, property_type);

        // Step 4: Apply optional modifier
        let final_type = match mapped.optional_modifier {
            Some(MappedModifier::Add) => self.interner.union2(property_type, TypeId::UNDEFINED),
            Some(MappedModifier::Remove) => property_type,
            None => property_type,
        };

        Some(PropertyAccessResult::Success {
            type_id: final_type,
            from_index_signature: false,
        })
    }

    /// Check if a property name is valid in a mapped type's constraint.
    fn is_key_in_mapped_constraint(&self, constraint: TypeId, prop_name: &str) -> bool {
        use crate::solver::types::{LiteralValue, TypeKey};

        // Evaluate keyof if needed
        let evaluated = if let Some(TypeKey::KeyOf(operand)) = self.interner.lookup(constraint) {
            // Create a keyof type and evaluate it
            let keyof_type = self.interner.intern(TypeKey::KeyOf(operand));
            evaluate_type(self.interner, keyof_type)
        } else {
            constraint
        };

        let Some(key) = self.interner.lookup(evaluated) else {
            return false;
        };

        match key {
            // Single string literal - exact match
            TypeKey::Literal(LiteralValue::String(s)) => self.interner.resolve_atom(s) == prop_name,
            // Union of literals - check if prop_name is in the union
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        // string index covers all string properties
                        return true;
                    }
                    if let Some(TypeKey::Literal(LiteralValue::String(s))) =
                        self.interner.lookup(member)
                    {
                        if self.interner.resolve_atom(s) == prop_name {
                            return true;
                        }
                    }
                }
                false
            }
            // string type covers all string properties
            TypeKey::Intrinsic(crate::solver::types::IntrinsicKind::String) => true,
            // Other types - can't determine statically
            _ => false,
        }
    }

    /// Check if a mapped type has a string index signature (constraint includes `string`).
    fn mapped_has_string_index(&self, mapped: &MappedType) -> bool {
        use crate::solver::types::{IntrinsicKind, TypeKey};

        let constraint = mapped.constraint;

        // Evaluate keyof if needed
        let evaluated = if let Some(TypeKey::KeyOf(operand)) = self.interner.lookup(constraint) {
            let keyof_type = self.interner.intern(TypeKey::KeyOf(operand));
            evaluate_type(self.interner, keyof_type)
        } else {
            constraint
        };

        if evaluated == TypeId::STRING {
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(evaluated) {
            let members = self.interner.type_list(members);
            for &member in members.iter() {
                if member == TypeId::STRING {
                    return true;
                }
                if let Some(TypeKey::Intrinsic(IntrinsicKind::String)) =
                    self.interner.lookup(member)
                {
                    return true;
                }
            }
        }

        if let Some(TypeKey::Intrinsic(IntrinsicKind::String)) = self.interner.lookup(evaluated) {
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
        // Handle intrinsic types first
        if obj_type == TypeId::ANY {
            // Any type allows any property access, returning any
            return PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            };
        }

        if obj_type == TypeId::ERROR {
            // Error type suppresses further errors, returns error
            return PropertyAccessResult::Success {
                type_id: TypeId::ERROR,
                from_index_signature: false,
            };
        }

        if obj_type == TypeId::UNKNOWN {
            return PropertyAccessResult::IsUnknown;
        }

        if obj_type == TypeId::NULL || obj_type == TypeId::UNDEFINED || obj_type == TypeId::VOID {
            let cause = if obj_type == TypeId::VOID {
                TypeId::UNDEFINED
            } else {
                obj_type
            };
            return PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: None,
                cause,
            };
        }

        // Handle Symbol primitive properties
        if obj_type == TypeId::SYMBOL {
            let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
            return self.resolve_symbol_primitive_property(prop_name, prop_atom);
        }

        // Look up the type key
        let key = match self.interner.lookup(obj_type) {
            Some(k) => k,
            None => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                return PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                };
            }
        };

        match key {
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
                use crate::solver::index_signatures::{IndexKind, IndexSignatureResolver};
                let resolver = IndexSignatureResolver::new(self.interner);

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
                let shape = self.interner.object_shape(shape_id);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
                use crate::solver::index_signatures::IndexSignatureResolver;
                let resolver = IndexSignatureResolver::new(self.interner);
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
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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

            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                if members.contains(&TypeId::ANY) {
                    return PropertyAccessResult::Success {
                        type_id: TypeId::ANY,
                        from_index_signature: false,
                    };
                }
                if members.contains(&TypeId::ERROR) {
                    return PropertyAccessResult::Success {
                        type_id: TypeId::ERROR,
                        from_index_signature: false,
                    };
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
                    return PropertyAccessResult::IsUnknown;
                }
                // Continue with non-UNKNOWN members
                // Property access on union: partition into nullable and non-nullable members
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                let mut valid_results = Vec::new();
                let mut nullable_causes = Vec::new();
                let mut any_from_index = false; // Track if any member used index signature

                for &member in non_unknown_members.iter() {
                    // Check for null/undefined directly
                    if member == TypeId::NULL
                        || member == TypeId::UNDEFINED
                        || member == TypeId::VOID
                    {
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
                        // PropertyNotFound or IsUnknown: skip this member, continue checking others
                        PropertyAccessResult::PropertyNotFound { .. }
                        | PropertyAccessResult::IsUnknown => {
                            // Member doesn't have this property - skip it
                        }
                    }
                }

                // If no non-nullable members had the property, it's a PropertyNotFound error
                if valid_results.is_empty() && nullable_causes.is_empty() {
                    // Before giving up, check union-level index signatures
                    use crate::solver::index_signatures::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.interner);

                    if resolver.has_index_signature(obj_type, IndexKind::String) {
                        if let Some(value_type) = resolver.resolve_string_index(obj_type) {
                            return PropertyAccessResult::Success {
                                type_id: self.add_undefined_if_unchecked(value_type),
                                from_index_signature: true,
                            };
                        }
                    }

                    if resolver.is_numeric_index_name(prop_name) {
                        if let Some(value_type) = resolver.resolve_number_index(obj_type) {
                            return PropertyAccessResult::Success {
                                type_id: self.add_undefined_if_unchecked(value_type),
                                from_index_signature: true,
                            };
                        }
                    }

                    return PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    };
                }

                // If there are nullable causes, return PossiblyNullOrUndefined
                if !nullable_causes.is_empty() {
                    let cause = if nullable_causes.len() == 1 {
                        nullable_causes[0]
                    } else {
                        self.interner.union(nullable_causes)
                    };

                    let mut property_type = if valid_results.is_empty() {
                        None
                    } else if valid_results.len() == 1 {
                        Some(valid_results[0])
                    } else {
                        Some(self.interner.union(valid_results))
                    };

                    if any_from_index
                        && self.no_unchecked_indexed_access
                        && let Some(t) = property_type
                    {
                        property_type = Some(self.add_undefined_if_unchecked(t));
                    }

                    return PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type,
                        cause,
                    };
                }

                let mut type_id = self.interner.union(valid_results);
                if any_from_index && self.no_unchecked_indexed_access {
                    type_id = self.add_undefined_if_unchecked(type_id);
                }

                // Union of all result types
                PropertyAccessResult::Success {
                    type_id,
                    from_index_signature: any_from_index, // Contagious across union members
                }
            }

            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
                            self.interner.union(nullable_causes)
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
                    use crate::solver::index_signatures::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.interner);

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
                    self.interner.intersection(results)
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
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                if let Some(constraint) = info.constraint {
                    self.resolve_property_access_inner(constraint, prop_name, Some(prop_atom))
                } else {
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    }
                }
            }

            // TS apparent members: literals inherit primitive wrapper methods.
            TypeKey::Literal(ref literal) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                match literal {
                    LiteralValue::String(_) => self.resolve_string_property(prop_name, prop_atom),
                    LiteralValue::Number(_) => self.resolve_number_property(prop_name, prop_atom),
                    LiteralValue::Boolean(_) => self.resolve_boolean_property(prop_name, prop_atom),
                    LiteralValue::BigInt(_) => self.resolve_bigint_property(prop_name, prop_atom),
                }
            }

            TypeKey::TemplateLiteral(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            // Built-in properties
            TypeKey::Intrinsic(IntrinsicKind::String) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Number) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_number_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Boolean) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_boolean_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Bigint) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_bigint_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Object) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    },
                )
            }

            TypeKey::Array(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Tuple(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            // Application: evaluate and resolve
            TypeKey::Application(_app_id) => {
                let _guard = match self.enter_mapped_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        let prop_atom =
                            prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                        return self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                            PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            },
                        );
                    }
                };

                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                    self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                        PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type,
                            property_name: prop_atom,
                        },
                    )
                }
            }

            // Mapped: try lazy property resolution first to avoid OOM on large mapped types
            TypeKey::Mapped(mapped_id) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));

                // Try lazy resolution first - only computes the requested property
                if let Some(result) =
                    self.resolve_mapped_property_lazy(mapped_id, prop_name, prop_atom)
                {
                    return result;
                }

                // Lazy resolution failed (complex constraint) - fall back to eager expansion
                let _guard = match self.enter_mapped_access_guard(obj_type) {
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

                let evaluated = evaluate_type(self.interner, obj_type);
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

            // Ref types: symbol references that need resolution to their structural form
            TypeKey::Ref(_) => {
                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                        result
                    } else {
                        // Can't resolve symbol reference - return ANY to avoid false positives
                        PropertyAccessResult::Success {
                            type_id: TypeId::ANY,
                            from_index_signature: false,
                        }
                    }
                }
            }

            // TypeQuery types: typeof queries that need resolution to their structural form
            TypeKey::TypeQuery(_) => {
                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - try apparent members
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    let prop_atom =
                        prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                    // KeyOf typically returns string/number/symbol, try string member access
                    self.resolve_string_property(prop_name, prop_atom)
                }
            }

            // ThisType: represents 'this' type in a class/interface context
            // Should be resolved to the actual class type by the checker
            TypeKey::ThisType => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                // 'this' type not resolved - return ANY to avoid false positives
                PropertyAccessResult::Success {
                    type_id: TypeId::ANY,
                    from_index_signature: false,
                }
            }

            _ => {
                // Unknown type key - try apparent members before giving up
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
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
        match self.interner.object_property_index(shape_id, prop_atom) {
            PropertyLookup::Found(idx) => props.get(idx),
            PropertyLookup::NotFound => None,
            PropertyLookup::Uncached => props.iter().find(|p| p.name == prop_atom),
        }
    }

    fn any_args_function(&self, return_type: TypeId) -> TypeId {
        let rest_array = self.interner.array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.interner.function(FunctionShape {
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

    fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner.union2(type_id, TypeId::UNDEFINED)
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
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
        match apparent_primitive_member_kind(self.interner, kind, prop_name) {
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
        if let Some(boxed_type) = self.resolver.get_boxed_type(kind) {
            // Resolve the property on the boxed interface type
            // This handles inheritance (e.g., String extends Object) automatically
            // and allows user-defined augmentations to lib.d.ts to work
            return self.resolve_property_access_inner(boxed_type, prop_name, Some(prop_atom));
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
        if let Some(array_base) = self.resolver.get_array_base_type() {
            let type_params = self.resolver.get_array_base_type_params();
            if !type_params.is_empty() {
                // Instantiate Array<T> with T = element_type
                use crate::solver::instantiate::instantiate_generic;
                let instantiated =
                    instantiate_generic(self.interner, array_base, type_params, &[element_type]);

                // Resolve the property on the instantiated interface
                let result =
                    self.resolve_property_access_inner(instantiated, prop_name, Some(prop_atom));

                // If we found the property, return it
                if !matches!(result, PropertyAccessResult::PropertyNotFound { .. }) {
                    return result;
                }
            }
        }

        // Handle numeric index access (e.g., arr[0], arr["0"])
        use crate::solver::index_signatures::IndexSignatureResolver;
        let resolver = IndexSignatureResolver::new(self.interner);
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
        match self.interner.lookup(array_type) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
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
        self.interner.union(members)
    }

    fn element_type_with_undefined(&self, element_type: TypeId) -> TypeId {
        self.interner.union2(element_type, TypeId::UNDEFINED)
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
        Some(TypeKey::ReadonlyType(inner)) => {
            if let Some(TypeKey::Array(_) | TypeKey::Tuple(_)) = interner.lookup(inner)
                && is_numeric_index_name(prop_name)
            {
                return true;
            }
            property_is_readonly(interner, inner, prop_name)
        }
        Some(TypeKey::Object(shape_id)) => {
            object_property_is_readonly(interner, shape_id, prop_name)
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

    // Check index signatures for numeric properties
    if is_numeric_index_name(prop_name) {
        if shape.string_index.as_ref().is_some_and(|idx| idx.readonly) {
            return true;
        }
        if shape.number_index.as_ref().is_some_and(|idx| idx.readonly) {
            return true;
        }
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
    use crate::solver::index_signatures::{IndexKind, IndexSignatureResolver};

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
