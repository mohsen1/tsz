//! Per-type-kind property resolution impls for `PropertyAccessEvaluator`.
//!
//! Contains the object/object-with-index/union resolution bodies dispatched
//! from `resolve_property_access_inner`, plus small shared predicates.
//!
//! Property names are threaded as interned `Atom`s; the string form is
//! resolved from the interner only at leaf sites that need character-level
//! checks (numeric index names, `#private` names, apparent-member tables).

use super::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::operations::expression_ops::normalize_fresh_object_literal_union_members;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{
    IndexSignature, ObjectFlags, ObjectShapeId, TypeData, TypeId, TypeListId, Visibility,
};
use tsz_common::interner::Atom;

impl<'a> PropertyAccessEvaluator<'a> {
    pub(crate) fn string_index_signature_accepts_property(
        &self,
        index: &IndexSignature,
        prop_atom: Atom,
    ) -> bool {
        // A `symbol`-keyed index signature never accepts a string/number-literal
        // property name. Historically a `[k: symbol]: V` signature was encoded in
        // the `string_index` slot with `key_type == SYMBOL`; treating that as a
        // catch-all string index silently typed `x["foo"]` / `x[1]` as `V`
        // instead of producing TS7053. Unique-symbol property names are routed
        // away from this helper earlier via the `__unique_` guard, so reaching
        // here with a symbol index always means a non-matching key.
        if index.key_type == TypeId::SYMBOL {
            return false;
        }

        if index.key_type == TypeId::STRING {
            return true;
        }

        let prop_type = self.interner().literal_string_atom(prop_atom);
        let mut checker = SubtypeChecker::new(self.interner());
        checker.is_subtype_of(prop_type, index.key_type)
    }

    /// Whether an index signature stored in the `string_index` slot should
    /// resolve `prop_atom`, used by the generic-instantiation (`Application`)
    /// property paths that do not perform key-type subtype gating.
    ///
    /// Unlike [`Self::string_index_signature_accepts_property`], this only
    /// excludes the `symbol`-keyed leak: a `[k: symbol]: V` signature (which
    /// may live in the `string_index` slot under the legacy encoding) must not
    /// satisfy a string/number-literal key, but still resolves an internal
    /// `__unique_`-named symbol property. Genuine `string`/template-literal
    /// indexes resolve every property name, preserving prior behavior.
    pub(crate) fn string_index_signature_resolves_property(
        &self,
        index: &IndexSignature,
        prop_atom: Atom,
    ) -> bool {
        if index.key_type == TypeId::SYMBOL {
            return self
                .interner()
                .resolve_atom_ref(prop_atom)
                .starts_with("__unique_");
        }
        true
    }

    /// True when `prop_atom` names an ES `#private` field that must not be
    /// visible through this lookup. The cheap visibility check runs first so
    /// public properties never resolve the atom's string form.
    pub(crate) fn is_private_identifier_property(
        &self,
        prop_atom: Atom,
        visibility: Visibility,
    ) -> bool {
        visibility == Visibility::Private
            && !self.allow_private_identifier_properties()
            && self.interner().resolve_atom_ref(prop_atom).starts_with('#')
    }

    fn is_typed_array_like_shape(&self, shape: &crate::types::ObjectShape) -> bool {
        if shape.number_index.is_none() {
            return false;
        }

        let has_prop = |name: &str| {
            let atom = self.interner().intern_string(name);
            shape.properties.iter().any(|prop| prop.name == atom)
        };

        has_prop("length") && has_prop("buffer") && has_prop("byteLength") && has_prop("byteOffset")
    }

    fn typed_array_to_locale_string_result(
        &self,
        shape: &crate::types::ObjectShape,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        // Typed-array-like shapes always carry a number index signature;
        // gate on it before resolving the atom's string form.
        shape.number_index.as_ref()?;
        if self.interner().resolve_atom_ref(prop_atom).as_ref() == "toLocaleString"
            && self.is_typed_array_like_shape(shape)
        {
            let function_type =
                crate::evaluation::evaluate_rules::apparent::make_apparent_method_type(
                    self.interner(),
                    TypeId::STRING,
                );
            Some(PropertyAccessResult::simple(function_type))
        } else {
            None
        }
    }

    pub(crate) fn visit_object_impl(
        &self,
        shape_id: u32,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        use crate::objects::index_signatures::IndexKind;

        let shape = self.interner().object_shape(ObjectShapeId(shape_id));
        let obj_type = self
            .interner()
            .object_type_from_shape(ObjectShapeId(shape_id));

        if let Some(result) = self.typed_array_to_locale_string_result(&shape, prop_atom) {
            return Some(result);
        }

        // Check explicit properties first (Atom-keyed; no string work on hits)
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
            && !self.is_private_identifier_property(prop_atom, prop.visibility)
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

        // Miss path: resolve the string form once for the leaf checks below.
        let prop_name_arc = self.interner().resolve_atom_ref(prop_atom);
        let prop_name = prop_name_arc.as_ref();

        // Check apparent members (toString, etc.)
        // Const enums have no runtime object, so they must not inherit
        // Object.prototype members (constructor, hasOwnProperty, etc.).
        if !shape.flags.contains(ObjectFlags::CONST_ENUM)
            && let Some(result) = self.resolve_object_member_named(prop_name)
        {
            return Some(result);
        }

        // Check for index signatures (some Object types may have index signatures that aren't in ObjectWithIndex)
        // Try string index signature first (most common).
        // Symbol-keyed properties (internal "__unique_N" names) must NOT
        // fall through to string index signatures.
        if !prop_name.starts_with("__unique_")
            && self.has_index_signature(obj_type, IndexKind::String)
            && let Some(value_type) = self.resolve_string_index_signature(obj_type)
            && self
                .get_index_info(obj_type)
                .string_index
                .as_ref()
                .is_none_or(|idx| self.string_index_signature_accepts_property(idx, prop_atom))
        {
            return Some(PropertyAccessResult::from_index(
                self.add_undefined_if_unchecked(value_type),
            ));
        }

        // Try numeric index signature if property name looks numeric
        if self.is_numeric_index_name(prop_name)
            && let Some(value_type) = self.resolve_number_index_signature(obj_type)
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
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        let shape = self.interner().object_shape(ObjectShapeId(shape_id));
        let obj_type = self
            .interner()
            .object_with_index_type_from_shape(ObjectShapeId(shape_id));

        if let Some(result) = self.typed_array_to_locale_string_result(&shape, prop_atom) {
            return Some(result);
        }

        // Check explicit properties first (Atom-keyed; no string work on hits)
        if let Some(prop) =
            self.lookup_object_property(ObjectShapeId(shape_id), &shape.properties, prop_atom)
            && !self.is_private_identifier_property(prop_atom, prop.visibility)
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

        // Miss path: resolve the string form once for the leaf checks below.
        let prop_name_arc = self.interner().resolve_atom_ref(prop_atom);
        let prop_name = prop_name_arc.as_ref();

        // Check apparent members (toString, etc.)
        // Const enums have no runtime object, so they must not inherit
        // Object.prototype members (constructor, hasOwnProperty, etc.).
        if !shape.flags.contains(ObjectFlags::CONST_ENUM)
            && let Some(result) = self.resolve_object_member_named(prop_name)
        {
            return Some(result);
        }

        // Check numeric index signature FIRST if property name looks numeric.
        // Number index signatures take precedence over string index signatures
        // for numeric keys (e.g., obj["0"] or obj[0] prefers [n: number] over [s: string]).
        if self.is_numeric_index_name(prop_name)
            && let Some(ref idx) = shape.number_index
        {
            let bound = self.bind_object_receiver_this(obj_type, idx.value_type);
            return Some(self.index_signature_result_with_nuia_write_type(bound));
        }

        // Check string index signature.
        // Symbol-keyed properties (internal "__unique_N" names) must NOT
        // fall through to string index signatures — tsc treats symbol keys
        // as distinct from string keys for index signature purposes. Use the
        // `string_index_signature()` accessor so a `[k: symbol]: V` signature
        // (which may live in the `string_index` slot under the legacy encoding)
        // is not consulted for a string/number-literal key; otherwise
        // `x["foo"]` on a symbol-only-index type would silently resolve to `V`
        // instead of producing TS7053.
        if !prop_name.starts_with("__unique_")
            && let Some(idx) = shape.string_index_signature()
            && self.string_index_signature_accepts_property(idx, prop_atom)
        {
            return Some(self.index_signature_result_with_nuia_write_type(idx.value_type));
        }

        Some(PropertyAccessResult::PropertyNotFound {
            type_id: obj_type,
            property_name: prop_atom,
        })
    }

    pub(crate) fn visit_union_impl(
        &self,
        list_id: u32,
        prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        use crate::objects::index_signatures::IndexKind;

        // Re-enable `this` binding for union member resolution. When a union is
        // nested inside an intersection (e.g., `(A & B) | (C & D)`), the
        // intersection handler sets `skip_this_binding = true` to prevent
        // per-member binding within the intersection. But each union member is a
        // distinct receiver type and needs its own `this` substitution — otherwise
        // polymorphic `this: this` methods on different interfaces collapse to the
        // same unsubstituted function type, breaking TS2684 detection.
        let prev_skip = self.is_skip_this_binding();
        self.set_skip_this_binding(false);

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
        let has_unknown = members.contains(&TypeId::UNKNOWN);
        let mut non_unknown_members: Vec<_> = if has_unknown {
            members
                .iter()
                .filter(|&&t| t != TypeId::UNKNOWN)
                .copied()
                .collect()
        } else {
            members.to_vec()
        };

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

        // Only prune if the union is small enough for pruning to be useful.
        // For large unions (e.g., 200-member discriminated unions), pruning is
        // expensive (O(N) type lookups + union reconstruction) and rarely eliminates
        // any members since type alias union members are typically valid.
        if non_unknown_members.len() <= 64 {
            let pruned_union = crate::type_queries::prune_impossible_object_union_members(
                self.interner(),
                self.interner()
                    .union_preserve_members(non_unknown_members.clone()),
            );
            // Intrinsics are never Union — skip the dyn lookup.
            if pruned_union.is_intrinsic() {
                non_unknown_members = vec![pruned_union];
            } else {
                match self.interner().lookup(pruned_union) {
                    Some(TypeData::Union(pruned_members)) => {
                        non_unknown_members = self.interner().type_list(pruned_members).to_vec();
                    }
                    _ => {
                        non_unknown_members = vec![pruned_union];
                    }
                }
            }
        }

        // Reconstructing the union can be expensive for large unions. Delay it
        // until we actually need it for an error/index-signature fallback path.
        let mut obj_type_cache: Option<TypeId> = None;
        let mut obj_type_for_error = || {
            *obj_type_cache.get_or_insert_with(|| {
                self.interner()
                    .union_preserve_members(self.interner().type_list(TypeListId(list_id)).to_vec())
            })
        };

        // Property access on union: partition into nullable and non-nullable members
        let mut valid_results = Vec::with_capacity(non_unknown_members.len());
        let mut valid_write_results = Vec::with_capacity(non_unknown_members.len());
        let mut any_has_divergent_write_type = false;
        let mut nullable_causes = Vec::with_capacity(non_unknown_members.len());
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

            match self.resolve_property_access_inner(member, prop_atom) {
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

        // Restore the `this` binding flag after processing all union members.
        self.set_skip_this_binding(prev_skip);

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
            let obj_type = obj_type_for_error();
            let prop_name_arc = self.interner().resolve_atom_ref(prop_atom);
            let prop_name = prop_name_arc.as_ref();

            if !prop_name.starts_with("__unique_")
                && self.has_index_signature(obj_type, IndexKind::String)
                && let Some(value_type) = self.resolve_string_index_signature(obj_type)
                && self
                    .get_index_info(obj_type)
                    .string_index
                    .as_ref()
                    .is_none_or(|idx| self.string_index_signature_accepts_property(idx, prop_atom))
            {
                return Some(PropertyAccessResult::from_index(
                    self.add_undefined_if_unchecked(value_type),
                ));
            }

            if self.is_numeric_index_name(prop_name)
                && let Some(value_type) = self.resolve_number_index_signature(obj_type)
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

        // Non-null union members must agree that the property exists.
        // If some constituents succeed but another non-null constituent reports
        // PropertyNotFound, the overall union should still be a missing-property
        // error rather than silently exposing the property from the successful
        // members only.
        if has_not_found_member && !valid_results.is_empty() && nullable_causes.is_empty() {
            return Some(PropertyAccessResult::PropertyNotFound {
                type_id: obj_type_for_error(),
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

        let read_pre_nuia = self.interner().union(valid_results);
        let nuia_active = any_from_index && self.no_unchecked_indexed_access;
        let mut type_id = read_pre_nuia;
        if nuia_active {
            type_id = self.add_undefined_if_unchecked(type_id);
        }

        // `noUncheckedIndexedAccess` only widens the READ type with `| undefined`;
        // the WRITE type stays the index signature's declared value type. Without
        // this distinction, writes like `strMap["k"] = undefined` against
        // `{[s: string]: boolean}` would silently succeed because the assignment
        // target picked up the read-side `boolean | undefined`.
        let write_type = if any_has_divergent_write_type {
            let wt = self.interner().union(valid_write_results);
            if wt != type_id { Some(wt) } else { None }
        } else if nuia_active && type_id != read_pre_nuia {
            Some(read_pre_nuia)
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
