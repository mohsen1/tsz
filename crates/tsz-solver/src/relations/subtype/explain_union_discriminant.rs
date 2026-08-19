//! Best-matching-member selection for object-to-union failure elaboration.
//!
//! When an object value fails to assign to a union target, `tsc`'s
//! `getBestMatchingType` picks the single union member to elaborate the failure
//! against. It tries, in order:
//!
//! 1. `findMatchingDiscriminantType` — a written unit discriminant on the
//!    source (`kind: "a"`) selects the member whose discriminant it matches,
//!    regardless of raw key overlap;
//! 2. `findMostOverlappyType` — otherwise the member sharing the most
//!    property-name keys with the source, ties broken by the *last* such member
//!    (`tsc` compares overlap with `>=`);
//! 3. no member at all when nothing overlaps (`findMostOverlappyType` needs a
//!    unit-typed key intersection), leaving the bare union line.
//!
//! `SubtypeChecker::explain_failure`'s structural-union arm (`explain.rs`)
//! previously implemented only step 2, so a discriminated union
//! (`{ kind: "a"; x } | { kind: "b"; y }` given `{ kind: "a" }`) tied on the
//! shared `kind` key and elaborated the *last* member — naming the wrong
//! missing property — and a source with no overlap picked an arbitrary member
//! `tsc` never elaborates. These helpers restore the full ordering.

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::SubtypeChecker;
use crate::type_queries::flow::is_unit_type;
use crate::types::{ObjectShapeId, TypeId};
use crate::visitor::{application_id, object_shape_id, object_with_index_shape_id};
use tsz_common::interner::Atom;

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Select the union member `tsc`'s `getBestMatchingType` would elaborate a
    /// failed object-to-union assignment against: a discriminant match first,
    /// then a same-generic-base type-reference match, then the
    /// highest-key-overlap member (ties to the last), and `None` when no
    /// member shares a key with the source.
    pub(super) fn select_union_target_best_member(
        &mut self,
        resolved_source: TypeId,
        members: &[TypeId],
    ) -> Option<TypeId> {
        if let Some(member) = self.union_discriminant_matched_member(resolved_source, members) {
            return Some(member);
        }
        if let Some(member) = self.matching_generic_reference_member(resolved_source, members) {
            return Some(member);
        }

        let source_names = self.object_like_property_names(resolved_source);
        if source_names.is_empty() {
            return None;
        }
        let mut best_member: Option<TypeId> = None;
        let mut best_overlap = 0usize;
        for &member in members.iter() {
            if self.check_subtype(resolved_source, member).is_true() {
                continue;
            }
            let member_names = self.object_like_property_names(member);
            let overlap = source_names
                .iter()
                .filter(|name| member_names.contains(name))
                .count();
            if overlap > 0 && (best_member.is_none() || overlap >= best_overlap) {
                best_overlap = overlap;
                best_member = Some(member);
            }
        }
        best_member
    }

    /// `tsc`'s `getBestMatchingType` -> `findMatchingTypeReferenceOrTypeAliasReference`:
    /// when the source is a generic type reference (an `Application`), the
    /// first union member that instantiates the *same* generic declaration is
    /// the match (`source.target === target.target`), ahead of any
    /// property-overlap scoring. An alias arm (`type StrRow =
    /// RawBuilder<string>`) and a directly-spelled arm (`RawBuilder<number>`)
    /// of the same base still share this identity, so the alias is hopped
    /// before comparing bases. Returns `None` when the source is not an
    /// `Application` of a nominal (`Lazy`) base, or no member shares it.
    fn matching_generic_reference_member(
        &mut self,
        source: TypeId,
        members: &[TypeId],
    ) -> Option<TypeId> {
        let source_def = self.generic_reference_base_def_id(source)?;
        members
            .iter()
            .copied()
            .find(|&member| self.generic_reference_base_def_id(member) == Some(source_def))
    }

    /// The `DefId` of the generic declaration `type_id` instantiates, hopping
    /// one alias indirection first (`StrRow` -> `RawBuilder<string>`). `None`
    /// when `type_id` is not (after that hop) an `Application` — e.g. a
    /// structural/anonymous generic instantiation, which tsc's own
    /// `ObjectFlags.Reference` gate also excludes from this match.
    fn generic_reference_base_def_id(&mut self, type_id: TypeId) -> Option<crate::def::DefId> {
        let resolved = self.resolve_lazy_type(type_id);
        let application = application_id(self.interner, resolved)
            .map(|_| resolved)
            .or_else(|| self.interner.get_display_alias(resolved))
            .or_else(|| self.interner.get_application_eval_origin(resolved))?;
        let app_id = application_id(self.interner, application)?;
        let base = self.interner.type_application(app_id).base;
        self.application_base_def_id(base)
    }

    /// `tsc`'s `getBestMatchingType` -> `findMatchingDiscriminantType`: when the
    /// source object carries a discriminant property whose written unit value
    /// (`kind: "a"`) selects exactly one union member, elaborate against that
    /// member — regardless of raw key-overlap ties. Returns the matched member,
    /// or `None` when no source property narrows the union to a single member.
    ///
    /// A property is treated as a union discriminant only when every member that
    /// declares it types it as a unit (a literal / enum member / `true` /
    /// `false` / `null` / `undefined`) — matching `tsc`'s
    /// `isDiscriminantProperty` — and the source's own value for it is likewise
    /// a unit. Members are narrowed by relating the source value to each
    /// member's value; a single survivor is the match.
    pub(super) fn union_discriminant_matched_member(
        &mut self,
        source: TypeId,
        members: &[TypeId],
    ) -> Option<TypeId> {
        // tsc iterates the source's properties in DECLARATION order
        // (`getPropertiesOfType` via `findDiscriminantProperties`), so when two
        // properties both qualify as discriminants and narrow to different
        // members, the first-declared one decides. The interned shape stores
        // name-sorted properties; restore declaration order from
        // `PropertyInfo::declaration_order` (0 = unrecorded, kept after the
        // recorded ones in stored order).
        let source_names = self.object_like_property_names_in_declaration_order(source);
        if source_names.is_empty() {
            return None;
        }
        let mut candidates: Vec<TypeId> = members.to_vec();
        let mut narrowed = false;
        for name in source_names {
            let Some(source_value) = self.discriminant_property_type(source, name) else {
                continue;
            };
            if !is_unit_type(self.interner, source_value) {
                continue;
            }
            // Qualify `name` as a union discriminant: at least one member
            // declares it and every member that declares it types it as a unit.
            let mut any_present = false;
            let mut all_unit = true;
            for &member in members.iter() {
                if let Some(member_value) = self.discriminant_property_type(member, name) {
                    any_present = true;
                    if !is_unit_type(self.interner, member_value) {
                        all_unit = false;
                        break;
                    }
                }
            }
            if !any_present || !all_unit {
                continue;
            }
            let filtered: Vec<TypeId> = candidates
                .iter()
                .copied()
                .filter(|&member| {
                    self.discriminant_property_type(member, name)
                        .is_some_and(|member_value| {
                            self.check_subtype(source_value, member_value).is_true()
                        })
                })
                .collect();
            if !filtered.is_empty() {
                candidates = filtered;
                narrowed = true;
                if candidates.len() == 1 {
                    break;
                }
            }
        }
        if narrowed && candidates.len() == 1 {
            Some(candidates[0])
        } else {
            None
        }
    }

    /// The read type of property `name` on the apparent (lazy-resolved,
    /// application-expanded) form of `type_id`, folding intersection members.
    /// Returns `None` when the property is absent. Used to compare a source
    /// object's written discriminant value against each union member's own
    /// value for that key.
    pub(super) fn discriminant_property_type(
        &mut self,
        type_id: TypeId,
        name: Atom,
    ) -> Option<TypeId> {
        use crate::type_queries::data::get_intersection_members;

        let resolved = self.apparent_type_for_keys(type_id);
        if let Some(shape_id) = object_shape_id(self.interner, resolved)
            .or_else(|| object_with_index_shape_id(self.interner, resolved))
            && let Some(property_type) = self.object_shape_property_type(shape_id, name)
        {
            return Some(property_type);
        }
        if let Some(members) = get_intersection_members(self.interner, resolved) {
            for member in members {
                let resolved_member = self.apparent_type_for_keys(member);
                if let Some(shape_id) = object_shape_id(self.interner, resolved_member)
                    .or_else(|| object_with_index_shape_id(self.interner, resolved_member))
                    && let Some(property_type) = self.object_shape_property_type(shape_id, name)
                {
                    return Some(property_type);
                }
            }
        }
        None
    }

    /// The read type of property `name` declared directly on `shape_id`.
    fn object_shape_property_type(&self, shape_id: ObjectShapeId, name: Atom) -> Option<TypeId> {
        self.interner
            .object_shape(shape_id)
            .properties
            .iter()
            .find(|prop| prop.name == name)
            .map(|prop| prop.type_id)
    }
}
