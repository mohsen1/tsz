//! `TypeVisitor` implementation for `PropertyAccessEvaluator`.
//!
//! Contains the visitor dispatch that resolves property access for each
//! type kind, plus impl helper methods for complex cases (objects, unions, etc.).

use crate::operations_property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::types::{IntrinsicKind, LiteralValue, ObjectShapeId, TupleListId, TypeId, TypeListId};
use crate::visitor::TypeVisitor;
use tsz_common::interner::Atom;

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
                write_type: None,
                from_index_signature: false,
            }),
            IntrinsicKind::Never => {
                // Property access on never returns never (code is unreachable)
                Some(PropertyAccessResult::Success {
                    type_id: TypeId::NEVER,
                    write_type: None,
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

        let prop_name = prop_name.as_deref()?;
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

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let write = (prop.write_type != prop.type_id).then_some(prop.write_type);
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                write_type: write,
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
        let obj_type = self.interner().object_with_flags_and_symbol(
            self.interner()
                .object_shape(ObjectShapeId(shape_id))
                .properties
                .clone(),
            self.interner().object_shape(ObjectShapeId(shape_id)).flags,
            self.interner().object_shape(ObjectShapeId(shape_id)).symbol,
        );

        // Try string index signature first (most common)
        if resolver.has_index_signature(obj_type, IndexKind::String)
            && let Some(value_type) = resolver.resolve_string_index(obj_type)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                write_type: None,
                from_index_signature: true,
            });
        }

        // Try numeric index signature if property name looks numeric
        if resolver.is_numeric_index_name(prop_name)
            && let Some(value_type) = resolver.resolve_number_index(obj_type)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                write_type: None,
                from_index_signature: true,
            });
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

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let write = (prop.write_type != prop.type_id).then_some(prop.write_type);
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                write_type: write,
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
                write_type: None,
                from_index_signature: true,
            });
        }

        // Check numeric index signature if property name looks numeric
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name)
            && let Some(ref idx) = shape.number_index
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(idx.value_type),
                write_type: None,
                from_index_signature: true,
            });
        }

        // Reconstruct obj_type for PropertyNotFound result
        let obj_type = self.interner().object_with_index(
            self.interner()
                .object_shape(ObjectShapeId(shape_id))
                .as_ref()
                .clone(),
        );

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Reconstruct obj_type for resolve_array_property
        let obj_type = self.interner().array(element_type);
        Some(self.resolve_array_property(obj_type, prop_name, prop_atom))
    }

    fn visit_tuple(&mut self, list_id: u32) -> Self::Output {
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Reconstruct obj_type for resolve_array_property
        let obj_type = self
            .interner()
            .tuple(self.interner().tuple_list(TupleListId(list_id)).to_vec());
        Some(self.resolve_array_property(obj_type, prop_name, prop_atom))
    }

    fn visit_template_literal(&mut self, _template_id: u32) -> Self::Output {
        // Template literals are string-like for property access
        // They support the same properties as the String interface
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;
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

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        Some(self.resolve_string_property(prop_name, prop_atom))
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;

        self.visit_union_impl(list_id, prop_name, prop_atom_opt.as_ref().copied())
    }

    fn default_output() -> Self::Output {
        None
    }
}

impl<'a> PropertyAccessEvaluator<'a> {
    // Helper methods to call visitor logic from &self context
    // These contain the actual implementation that the TypeVisitor trait methods delegate to

    pub(crate) fn visit_object_impl(
        &self,
        shape_id: u32,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> Option<PropertyAccessResult> {
        use crate::index_signatures::{IndexKind, IndexSignatureResolver};

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let write = (prop.write_type != prop.type_id).then_some(prop.write_type);
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                write_type: write,
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
        let obj_type = self.interner().object_with_flags_and_symbol(
            self.interner()
                .object_shape(ObjectShapeId(shape_id))
                .properties
                .clone(),
            self.interner().object_shape(ObjectShapeId(shape_id)).flags,
            self.interner().object_shape(ObjectShapeId(shape_id)).symbol,
        );

        // Try string index signature first (most common)
        if resolver.has_index_signature(obj_type, IndexKind::String)
            && let Some(value_type) = resolver.resolve_string_index(obj_type)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                write_type: None,
                from_index_signature: true,
            });
        }

        // Try numeric index signature if property name looks numeric
        if resolver.is_numeric_index_name(prop_name)
            && let Some(value_type) = resolver.resolve_number_index(obj_type)
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                write_type: None,
                from_index_signature: true,
            });
        }

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    pub(crate) fn visit_object_with_index_impl(
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
            let write = (prop.write_type != prop.type_id).then_some(prop.write_type);
            return Some(PropertyAccessResult::Success {
                type_id: self.optional_property_type(prop),
                write_type: write,
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
                write_type: None,
                from_index_signature: true,
            });
        }

        // Check numeric index signature if property name looks numeric
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name)
            && let Some(ref idx) = shape.number_index
        {
            return Some(PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(idx.value_type),
                write_type: None,
                from_index_signature: true,
            });
        }

        // Reconstruct obj_type for PropertyNotFound result
        let obj_type = self
            .interner()
            .intern(crate::types::TypeData::ObjectWithIndex(ObjectShapeId(
                shape_id,
            )));

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    pub(crate) fn visit_array_impl(
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

    pub(crate) fn visit_union_impl(
        &self,
        list_id: u32,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> Option<PropertyAccessResult> {
        use crate::index_signatures::{IndexKind, IndexSignatureResolver};

        let members = self.interner().type_list(crate::types::TypeListId(list_id));

        // Fast-path: if ANY member is any, result is any
        if members.contains(&TypeId::ANY) {
            return Some(PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                write_type: None,
                from_index_signature: false,
            });
        }

        // Fast-path: if ANY member is error, result is error
        if members.contains(&TypeId::ERROR) {
            return Some(PropertyAccessResult::Success {
                type_id: TypeId::ERROR,
                write_type: None,
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

        // Reconstructing the union can be expensive for large unions. Delay it
        // until we actually need it for an error/index-signature fallback path.
        let mut obj_type_cache: Option<TypeId> = None;
        let mut obj_type_for_error = || {
            *obj_type_cache.get_or_insert_with(|| {
                self.interner()
                    .union(self.interner().type_list(TypeListId(list_id)).to_vec())
            })
        };

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        // Property access on union: partition into nullable and non-nullable members
        let mut valid_results = Vec::new();
        let mut valid_write_results = Vec::new();
        let mut any_has_divergent_write_type = false;
        let mut nullable_causes = Vec::new();
        let mut any_from_index = false; // Track if any member used index signature

        for &member in &non_unknown_members {
            // Check for null/undefined directly
            if member.is_nullable() {
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
                    write_type,
                    from_index_signature,
                } => {
                    valid_results.push(type_id);
                    if let Some(wt) = write_type {
                        valid_write_results.push(wt);
                        any_has_divergent_write_type = true;
                    } else {
                        valid_write_results.push(type_id);
                    }
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
                        valid_write_results.push(t);
                    }
                    nullable_causes.push(cause);
                }
                // PropertyNotFound: if ANY member is missing the property, the property does not exist on the Union
                PropertyAccessResult::PropertyNotFound { .. } => {
                    return Some(PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type_for_error(),
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
            let obj_type = obj_type_for_error();

            if resolver.has_index_signature(obj_type, IndexKind::String)
                && let Some(value_type) = resolver.resolve_string_index(obj_type)
            {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    write_type: None,
                    from_index_signature: true,
                });
            }

            if resolver.is_numeric_index_name(prop_name)
                && let Some(value_type) = resolver.resolve_number_index(obj_type)
            {
                return Some(PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    write_type: None,
                    from_index_signature: true,
                });
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

        let write_type = if any_has_divergent_write_type {
            let mut wt = self.interner().union(valid_write_results);
            if any_from_index && self.no_unchecked_indexed_access {
                wt = self.add_undefined_if_unchecked(wt);
            }
            if wt != type_id { Some(wt) } else { None }
        } else {
            None
        };

        // Union of all result types
        Some(PropertyAccessResult::Success {
            type_id,
            write_type,
            from_index_signature: any_from_index, // Contagious across union members
        })
    }
}
