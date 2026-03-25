//! `TypeVisitor` implementation for `PropertyAccessEvaluator`.
//!
//! Contains the visitor dispatch that resolves property access for each
//! type kind, plus impl helper methods for complex cases (objects, unions, etc.).

use super::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::operations::expression_ops::normalize_fresh_object_literal_union_members;
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectFlags, ObjectShapeId, TupleListId, TypeData, TypeId,
    TypeListId,
};
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
            IntrinsicKind::Any => Some(PropertyAccessResult::simple(TypeId::ANY)),
            IntrinsicKind::Never => {
                // Property access on never returns never (code is unreachable)
                Some(PropertyAccessResult::simple(TypeId::NEVER))
            }
            IntrinsicKind::Unknown => Some(PropertyAccessResult::IsUnknown),
            IntrinsicKind::Void => {
                // In tsc, accessing a property on `void` produces TS2339
                // ("Property 'X' does not exist on type 'void'"), NOT TS2532.
                let prop_atom = self.current_prop_atom.borrow();
                let atom = prop_atom.unwrap_or_else(|| {
                    let prop_name = self.current_prop_name.borrow();
                    self.interner()
                        .intern_string(prop_name.as_deref().unwrap_or(""))
                });
                Some(PropertyAccessResult::PropertyNotFound {
                    type_id: TypeId::VOID,
                    property_name: atom,
                })
            }
            IntrinsicKind::Null | IntrinsicKind::Undefined => {
                let cause = if kind == IntrinsicKind::Undefined {
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
        use crate::objects::index_signatures::{IndexKind, IndexSignatureResolver};

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));
        // PERF: Reuse existing interned type for this shape instead of cloning
        // the entire property list and re-interning. The shape is already interned,
        // so this is an O(1) cache hit.
        let obj_type = self
            .interner()
            .object_type_from_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let read_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_type(prop));
            let write_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_write_type(prop));
            let write = (write_type != read_type).then_some(write_type);
            return Some(PropertyAccessResult::Success {
                type_id: read_type,
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

        // Try string index signature first (most common).
        // Symbol-keyed properties (internal "__unique_N" names) must NOT
        // fall through to string index signatures.
        if !prop_name.starts_with("__unique_")
            && resolver.has_index_signature(obj_type, IndexKind::String)
            && let Some(value_type) = resolver.resolve_string_index(obj_type)
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, value_type),
                ),
            ));
        }

        // Try numeric index signature if property name looks numeric
        if resolver.is_numeric_index_name(prop_name)
            && let Some(value_type) = resolver.resolve_number_index(obj_type)
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, value_type),
                ),
            ));
        }

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    fn visit_object_with_index(&mut self, shape_id: u32) -> Self::Output {
        use crate::objects::index_signatures::IndexSignatureResolver;

        let prop_name = self.current_prop_name.borrow();
        let prop_atom_opt = self.current_prop_atom.borrow();

        let prop_name = prop_name.as_deref()?;
        let prop_atom = match prop_atom_opt.as_ref() {
            Some(&atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));
        let obj_type = self
            .interner()
            .object_with_index_type_from_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let read_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_type(prop));
            let write_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_write_type(prop));
            let write = (write_type != read_type).then_some(write_type);
            return Some(PropertyAccessResult::Success {
                type_id: read_type,
                write_type: write,
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return Some(result);
        }

        // Check numeric index signature FIRST if property name looks numeric.
        // Number index signatures take precedence over string index signatures
        // for numeric keys (e.g., obj["0"] or obj[0] prefers [n: number] over [s: string]).
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name)
            && let Some(ref idx) = shape.number_index
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, idx.value_type),
                ),
            ));
        }

        // Check string index signature (skip for symbol-keyed properties)
        if !prop_name.starts_with("__unique_")
            && let Some(ref idx) = shape.string_index
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(idx.value_type),
            ));
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
        use crate::objects::index_signatures::{IndexKind, IndexSignatureResolver};

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));
        // PERF: Reuse existing interned type for this shape instead of cloning
        // the entire property list and re-interning. The shape is already interned,
        // so this is an O(1) cache hit.
        let obj_type = self
            .interner()
            .object_type_from_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let read_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_type(prop));
            let write_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_write_type(prop));
            let write = (write_type != read_type).then_some(write_type);
            return Some(PropertyAccessResult::Success {
                type_id: read_type,
                write_type: write,
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        // Const enums have no runtime object, so they must not inherit
        // Object.prototype members (constructor, hasOwnProperty, etc.).
        if !shape.flags.contains(ObjectFlags::CONST_ENUM)
            && let Some(result) = self.resolve_object_member(prop_name, prop_atom)
        {
            return Some(result);
        }

        // Check for index signatures (some Object types may have index signatures that aren't in ObjectWithIndex)
        let resolver = IndexSignatureResolver::new(self.interner());

        // Try string index signature first (most common).
        // Symbol-keyed properties (internal "__unique_N" names) must NOT
        // fall through to string index signatures.
        if !prop_name.starts_with("__unique_")
            && resolver.has_index_signature(obj_type, IndexKind::String)
            && let Some(value_type) = resolver.resolve_string_index(obj_type)
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, value_type),
                ),
            ));
        }

        // Try numeric index signature if property name looks numeric
        if resolver.is_numeric_index_name(prop_name)
            && let Some(value_type) = resolver.resolve_number_index(obj_type)
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, value_type),
                ),
            ));
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
        use crate::objects::index_signatures::IndexSignatureResolver;

        let prop_atom = match prop_atom {
            Some(atom) => atom,
            None => self.interner().intern_string(prop_name),
        };

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));
        let obj_type = self
            .interner()
            .object_with_index_type_from_shape(ObjectShapeId(shape_id));

        // Check explicit properties first
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
        {
            let read_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_type(prop));
            let write_type =
                self.bind_object_receiver_this(obj_type, self.optional_property_write_type(prop));
            let write = (write_type != read_type).then_some(write_type);
            return Some(PropertyAccessResult::Success {
                type_id: read_type,
                write_type: write,
                from_index_signature: false,
            });
        }

        // Check apparent members (toString, etc.)
        // Const enums have no runtime object, so they must not inherit
        // Object.prototype members (constructor, hasOwnProperty, etc.).
        if !shape.flags.contains(ObjectFlags::CONST_ENUM)
            && let Some(result) = self.resolve_object_member(prop_name, prop_atom)
        {
            return Some(result);
        }

        // Check numeric index signature FIRST if property name looks numeric.
        // Number index signatures take precedence over string index signatures
        // for numeric keys (e.g., obj["0"] or obj[0] prefers [n: number] over [s: string]).
        let resolver = IndexSignatureResolver::new(self.interner());
        if resolver.is_numeric_index_name(prop_name)
            && let Some(ref idx) = shape.number_index
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, idx.value_type),
                ),
            ));
        }

        // Check string index signature.
        // Symbol-keyed properties (internal "__unique_N" names) must NOT
        // fall through to string index signatures — tsc treats symbol keys
        // as distinct from string keys for index signature purposes.
        if !prop_name.starts_with("__unique_")
            && let Some(ref idx) = shape.string_index
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(
                    self.bind_object_receiver_this(obj_type, idx.value_type),
                ),
            ));
        }

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
        use crate::objects::index_signatures::{IndexKind, IndexSignatureResolver};

        let members = self.interner().type_list(crate::types::TypeListId(list_id));

        // Fast-path: if ANY member is any, result is any
        if members.contains(&TypeId::ANY) {
            return Some(PropertyAccessResult::simple(TypeId::ANY));
        }

        // Fast-path: if ANY member is error, result is error
        if members.contains(&TypeId::ERROR) {
            return Some(PropertyAccessResult::simple(TypeId::ERROR));
        }

        // Filter out UNKNOWN members - they shouldn't cause the entire union to be unknown
        // Only return IsUnknown if ALL members are UNKNOWN
        let mut non_unknown_members: Vec<_> = members
            .iter()
            .filter(|&&t| t != TypeId::UNKNOWN)
            .copied()
            .collect();

        if non_unknown_members.is_empty() {
            // All members are UNKNOWN
            return Some(PropertyAccessResult::IsUnknown);
        }

        let fresh_object_union = non_unknown_members.iter().all(|&member| {
            crate::relations::freshness::is_fresh_object_type(self.interner(), member)
        });

        if let Some(normalized) =
            normalize_fresh_object_literal_union_members(self.interner(), &non_unknown_members)
        {
            non_unknown_members = normalized;
        }

        let pruned_union = crate::type_queries::prune_impossible_object_union_members(
            self.interner(),
            self.interner().union_from_slice(&non_unknown_members),
        );
        match self.interner().lookup(pruned_union) {
            Some(TypeData::Union(pruned_members)) => {
                non_unknown_members = self.interner().type_list(pruned_members).to_vec();
            }
            _ => {
                non_unknown_members = vec![pruned_union];
            }
        }

        // Reconstructing the union can be expensive for large unions. Delay it
        // until we actually need it for an error/index-signature fallback path.
        let mut obj_type_cache: Option<TypeId> = None;
        let mut obj_type_for_error = || {
            *obj_type_cache.get_or_insert_with(|| {
                self.interner()
                    .union_from_slice(&self.interner().type_list(TypeListId(list_id)))
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
        let mut any_from_index = false; // ANY member used index signature (for noUncheckedIndexedAccess)
        let mut all_from_index = true; // ALL members used index signature (for TS2540 vs TS2542)
        let mut has_unknown_members = false;
        let mut saw_deferred_any_fallback = false;
        let mut has_not_found_member = false;
        // Pre-check: does the union contain nullable members? If so, we must
        // not early-return PropertyNotFound when a non-nullable member is missing
        // the property — tsc prioritizes "possibly null/undefined" (TS18049)
        // over "property does not exist" (TS2339).
        let union_has_nullable = non_unknown_members.iter().any(|m| m.is_nullable());

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
                    if type_id == TypeId::ANY
                        && !from_index_signature
                        && self.is_deferred_any_fallback_member(member)
                    {
                        saw_deferred_any_fallback = true;
                        continue;
                    }
                    valid_results.push(type_id);
                    if let Some(wt) = write_type {
                        valid_write_results.push(wt);
                        any_has_divergent_write_type = true;
                    } else {
                        valid_write_results.push(type_id);
                    }
                    if from_index_signature {
                        any_from_index = true;
                    } else {
                        all_from_index = false; // If ANY member has named property, not index-only
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
                // PropertyNotFound: if a non-empty-object member is missing the property,
                // the property does not exist on the union.
                //
                // Fresh empty object types ({} from object literal expressions, e.g.
                // `options || {}`) are treated as partial: they contribute `undefined`
                // for any property that exists on other members. This matches tsc's
                // behavior where `(x || {}).prop` is allowed when `x` has `prop`.
                // Non-fresh empty objects (from type annotations like `T | {}`) are NOT
                // treated as partial — tsc reports TS2339 for those.
                //
                // If the union also contains nullable members (null/undefined), tsc
                // prioritizes reporting "possibly null/undefined" (TS18049) over
                // "property does not exist" (TS2339). So we defer the PropertyNotFound
                // decision until after all members have been processed.
                PropertyAccessResult::PropertyNotFound { .. } => {
                    let is_fresh_empty = crate::is_empty_object_type(self.interner(), member)
                        && (fresh_object_union
                            || crate::relations::freshness::is_fresh_object_type(
                                self.interner(),
                                member,
                            ));
                    if is_fresh_empty {
                        // Fresh empty object: treat as partial, property yields undefined
                        valid_results.push(TypeId::UNDEFINED);
                        valid_write_results.push(TypeId::UNDEFINED);
                        all_from_index = false;
                        continue;
                    }
                    // When the union has nullable members, defer the not-found
                    // decision. tsc prioritizes "possibly null/undefined" (TS18049)
                    // over "property doesn't exist" (TS2339) when at least one
                    // non-nullable member HAS the property. The post-loop logic
                    // handles the final decision.
                    if !union_has_nullable {
                        return Some(PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type_for_error(),
                            property_name: prop_atom,
                        });
                    }
                    has_not_found_member = true;
                }
                // IsUnknown: skip unknown members in unions — they shouldn't prevent
                // property access on other union members that DO have the property.
                // Only return IsUnknown if ALL non-nullable members are unknown.
                PropertyAccessResult::IsUnknown => {
                    has_unknown_members = true;
                    continue;
                }
            }
        }

        // If all non-nullable, non-unknown members had no results and some were unknown,
        // then the union is effectively unknown for property access purposes.
        if valid_results.is_empty() && nullable_causes.is_empty() && has_unknown_members {
            return Some(PropertyAccessResult::IsUnknown);
        }

        // If no non-nullable members had the property, it's a PropertyNotFound error.
        // This also applies when nullable members exist but ALL non-nullable members
        // failed — tsc reports TS2339 (property doesn't exist) not TS18049 (possibly null).
        if valid_results.is_empty() && (nullable_causes.is_empty() || has_not_found_member) {
            if saw_deferred_any_fallback {
                return Some(PropertyAccessResult::simple(TypeId::ANY));
            }

            // Before giving up, check union-level index signatures
            let resolver = IndexSignatureResolver::new(self.interner());
            let obj_type = obj_type_for_error();

            if !prop_name.starts_with("__unique_")
                && resolver.has_index_signature(obj_type, IndexKind::String)
                && let Some(value_type) = resolver.resolve_string_index(obj_type)
            {
                return Some(PropertyAccessResult::from_index(
                    self.add_undefined_if_unchecked(value_type),
                ));
            }

            if resolver.is_numeric_index_name(prop_name)
                && let Some(value_type) = resolver.resolve_number_index(obj_type)
            {
                return Some(PropertyAccessResult::from_index(
                    self.add_undefined_if_unchecked(value_type),
                ));
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

        // Union of all result types — only flag as "from index signature" if ALL
        // members resolved through index signatures. If any member has the property
        // as a named property, the checker should use TS2540 (not TS2542).
        Some(PropertyAccessResult::Success {
            type_id,
            write_type,
            from_index_signature: all_from_index,
        })
    }
}
