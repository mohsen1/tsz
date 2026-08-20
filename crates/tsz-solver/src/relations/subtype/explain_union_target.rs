//! Union-**target** failure explanation: best-member selection and the
//! elaboration reason carried past the bare union head line.
//!
//! Extracted from `explain.rs`'s structural walk. When a relation fails
//! against a union target, `tsc` continues the diagnostic chain past
//! `Type 'S' is not assignable to type 'A | B'.`: an application-shaped
//! comparison folds to a direct missing-property line, a sole-real-member
//! nullable union promotes the member's own reason, a union source recurses
//! per member, and a structural union selects the best-matching member
//! (`getBestMatchingType`, see `explain_union_discriminant.rs`) whose failure
//! is carried in [`SubtypeFailureReason::UnionTargetMismatch`].

use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::relations::subtype::SubtypeChecker;
use crate::types::TypeId;
use crate::visitor::{
    application_id, is_type_parameter, object_shape_id, object_with_index_shape_id, union_list_id,
};

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Wrap a promoted sole-real-member failure in
    /// [`SubtypeFailureReason::UnionSourceMismatch`] when the source is itself
    /// a union, so the relation-pair frame survives past the promotion.
    ///
    /// tsc's chain for a union-vs-union failure keeps the pair line before the
    /// member's own elaboration (`Type 'boolean | undefined' is not assignable
    /// to type 'string | undefined'.` -> `Type 'boolean' is not assignable to
    /// type 'string'.`). A single-member source has no such frame — its head
    /// line already names the pair — so the promoted reason returns bare.
    fn wrap_union_source_member_reason(
        &self,
        source: TypeId,
        target: TypeId,
        member_type: TypeId,
        source_members: &[TypeId],
        reason: SubtypeFailureReason,
    ) -> SubtypeFailureReason {
        if source_members.len() > 1 {
            return SubtypeFailureReason::UnionSourceMismatch {
                source_type: source,
                target_type: target,
                member_type,
                nested_reason: Box::new(reason),
            };
        }
        reason
    }

    /// Refine a failing `boolean` union-source member to its failing literal
    /// half.
    ///
    /// tsc's `booleanType` is a primitive union of the two boolean literals,
    /// so its union-source walk (`eachTypeRelatedToType`) relates `false` and
    /// `true` separately: the reported witness is the first failing literal
    /// constituent (`Type 'false' is not assignable to type 'true'.`), and
    /// display-side literal generalization (`reportRelationError`) widens it
    /// back to `boolean` only when the target holds no top-level singleton
    /// types. tsz interns `boolean` as a single intrinsic, so the walk refines
    /// the witness here; the checker's display generalization owns the
    /// widening. Callers gate this to genuine multi-member sources — a bare
    /// `boolean` source is a primitive union in tsc, whose walk never reports
    /// per constituent, so its witness stays `boolean`.
    fn boolean_member_failing_half(&mut self, member: TypeId, resolved_target: TypeId) -> TypeId {
        if self.resolve_lazy_type(member) != TypeId::BOOLEAN {
            return member;
        }
        for half in [TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN_TRUE] {
            if !self.check_subtype(half, resolved_target).is_true() {
                return half;
            }
        }
        member
    }

    /// tsc's `getBestMatchingType` same-reference step
    /// (`findMatchingTypeReferenceOrTypeAliasReference`): the first union
    /// member — in the target's own member order — whose generic application
    /// names the source's own canonical base, across forwarding-alias
    /// spellings on either side. Returns the member as written in the union
    /// (the alias spelling the elaboration frame displays), or `None` when
    /// no member shares the source's base or the source has no recoverable
    /// application identity.
    fn union_same_base_reference_member(
        &mut self,
        source: TypeId,
        resolved_source: TypeId,
        members: &[TypeId],
    ) -> Option<TypeId> {
        let source_base = self
            .explain_application_base_def(source)
            .or_else(|| self.explain_application_base_def(resolved_source))?;
        members.iter().copied().find(|&member| {
            self.explain_application_base_def(member)
                .is_some_and(|member_base| {
                    self.application_base_defs_match(source_base, member_base)
                })
        })
    }

    /// Explain a failed relation whose `resolved_target` is a union. Always
    /// returns a reason; the caller has already established the union shape.
    pub(super) fn explain_union_target_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        resolved_source: TypeId,
        resolved_target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Prefer the original target's union members so member display keeps
        // user-facing aliases (e.g. an identity mapped type `Mapped<B>` that
        // structurally simplifies to `B` in `resolved_target` must still
        // render as `Mapped<B>` in the elaboration, matching tsc). Fall back
        // to the resolved union when the target is itself a lazy alias.
        let members_id = union_list_id(self.interner, target)
            .or_else(|| union_list_id(self.interner, resolved_target))
            .expect("resolved_target is a union");
        let members = self.interner.type_list(members_id);
        let application_shaped_comparison = application_id(self.interner, source).is_some()
            || application_id(self.interner, target).is_some();
        let source_members = union_list_id(self.interner, resolved_source)
            .map(|list_id| self.interner.type_list(list_id).as_ref().to_vec())
            .unwrap_or_else(|| vec![resolved_source]);

        // tsc's `getBestMatchingType` prefers a same-reference member
        // (`findMatchingTypeReferenceOrTypeAliasReference`) over both the
        // application-shaped missing-property fold and the key-overlap pick
        // below: the failure elaborates beneath a member frame against the
        // first union member naming the source's own generic base, and that
        // member relation drills its differing type argument (`Type 'string |
        // number' is not assignable to type 'number'.` -> `Type 'string' is
        // not assignable to type 'number'.`) instead of walking properties.
        // Union-shaped sources are excluded: tsc's union-source walk
        // (`eachTypeRelatedToType`) reports before any best-member selection,
        // and the blocks below own that shape. Sole-real-member nullable
        // targets (`T | undefined`) are excluded the same way: tsc folds
        // their head to the source-vs-member pair with no member frame, and
        // the promotion block below owns that fold.
        if source_members.len() == 1
            && members.iter().filter(|m| !m.is_nullish()).count() > 1
            && let Some(member) =
                self.union_same_base_reference_member(source, resolved_source, &members)
            && let Some(reason) = self.explain_failure_guarded(resolved_source, member)
        {
            return Some(SubtypeFailureReason::UnionTargetMismatch {
                source_type: source,
                target_type: target,
                member_type: member,
                nested_reason: Box::new(reason),
            });
        }

        // Application-shaped comparison (e.g. assigning to `Foo<X>` that
        // resolves to a union): tsc collapses the elaboration to a direct
        // missing-property line against the application target rather than
        // the structural union members, so keep that first-failing-member
        // behavior here.
        if application_shaped_comparison {
            let mut missing_property_reason = None;
            'find_missing: for &member in members.iter() {
                if self.check_subtype(resolved_source, member).is_true() {
                    continue;
                }
                for &source_member in &source_members {
                    if self.check_subtype(source_member, member).is_true() {
                        continue;
                    }
                    let member_reason = self.explain_failure_guarded(source_member, member);
                    let missing_property = match member_reason {
                        Some(SubtypeFailureReason::MissingProperty { property_name, .. }) => {
                            Some(property_name)
                        }
                        Some(SubtypeFailureReason::MissingProperties {
                            property_names, ..
                        }) => property_names.first().copied(),
                        _ => None,
                    };
                    if let Some(property_name) = missing_property {
                        missing_property_reason = Some(property_name);
                        break 'find_missing;
                    }
                }
            }
            if let Some(property_name) = missing_property_reason {
                return Some(SubtypeFailureReason::MissingProperty {
                    property_name,
                    source_type: source,
                    target_type: target,
                });
            }
            // No member failed on a missing property: fall through to the
            // general best-matching-member elaboration below instead of
            // collapsing to a bare union-head line. tsc's `getBestMatchingType`
            // always re-relates against the selected member with errors
            // enabled regardless of source shape, so an application-shaped
            // source failing on a property *type* mismatch (not a missing
            // property) — e.g. a generic tag's inferred `RawBuilder<string |
            // number>` against `StrRow | RawBuilder<number>` — still drills
            // into that member's own reason instead of losing the chain.
        }

        // Nullable-object target (`T | null`, `T | undefined`,
        // `T | null | undefined`): every member other than a single
        // object-like member is nullish. A non-nullish source (an object
        // literal here) can never satisfy the nullish members, so tsc
        // elaborates the failure against `T` exactly as if the target were
        // `T` alone — a missing required property surfaces as the top-level
        // `MissingProperty`/`MissingProperties` reason (rendered TS2741 /
        // TS2739 in an assignment/return position, TS2345 in an argument
        // position), not as a `UnionTargetMismatch` whose missing-property
        // line is demoted to a child of a generic TS2322 union mismatch.
        // Promote that reason here so the single-real-member shape matches
        // tsc; a genuine multi-member union (`A | B`, `T | number`) keeps
        // the union-mismatch elaboration below.
        {
            let mut non_nullish = members.iter().copied().filter(|m| !m.is_nullish());
            if let (Some(sole_member), None) = (non_nullish.next(), non_nullish.next()) {
                for &source_member in &source_members {
                    // tsc's union-source walk (`eachTypeRelatedToType`) only
                    // reports a member that fails against the WHOLE union
                    // target, so a nullish source member absorbed by the
                    // target's own nullish members is never the witness
                    // (`undefined` <: `T | undefined` — the failing member of
                    // `{ a: boolean } | undefined` vs `{ a: string } |
                    // undefined` is the object arm, not `undefined`). Gated to
                    // genuine multi-member sources: for a single-member walk
                    // `source_member` IS `resolved_source`, so this would
                    // re-query the exact pair being explained and pick up the
                    // relation stack's provisional in-progress verdict.
                    if source_members.len() > 1
                        && self.check_subtype(source_member, resolved_target).is_true()
                    {
                        continue;
                    }
                    // A type-parameter member has no best-matching target
                    // member (tsc's `getBestMatchingType` finds none), so its
                    // failure is explained against the full union target:
                    // `Type 'Q' is not assignable to type 'string |
                    // undefined'.`, drilling through the declared constraint
                    // when one exists, never against the reduced sole member.
                    if is_type_parameter(self.interner, self.resolve_lazy_type(source_member)) {
                        if let Some(reason) =
                            self.explain_failure_guarded(source_member, resolved_target)
                        {
                            return Some(self.wrap_union_source_member_reason(
                                source,
                                target,
                                source_member,
                                &source_members,
                                reason,
                            ));
                        }
                        break;
                    }
                    // A failing `boolean` member in a genuine union walk
                    // explains through its failing literal half, as tsc's
                    // constituent walk does (`Type 'false' is not assignable
                    // to type 'true'.`); the display layer widens the literal
                    // back to `boolean` against singleton-free targets.
                    let witness = if source_members.len() > 1 {
                        self.boolean_member_failing_half(source_member, resolved_target)
                    } else {
                        source_member
                    };
                    if let Some(reason) = self.explain_failure_guarded(witness, sole_member) {
                        let promote = match &reason {
                            // Object/array source missing a required property:
                            // surface the missing-property reason directly
                            // (TS2741 / TS2739 / TS2345).
                            //
                            // Tuple sources drill the member relation the same
                            // way (`Type '[boolean]' is not assignable to type
                            // '[string]'.` beneath the union pair line), so
                            // they promote with the missing-property family.
                            SubtypeFailureReason::MissingProperty { .. }
                            | SubtypeFailureReason::MissingProperties { .. }
                            | SubtypeFailureReason::TupleElementTypeMismatch { .. }
                            | SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
                            | SubtypeFailureReason::TupleElementMismatch { .. }
                            | SubtypeFailureReason::TupleArityMismatch(_) => true,
                            // Scalar source (a primitive / string-literal property
                            // value): tsc elaborates `S` against the sole real member
                            // `T` directly instead of a `NoUnionMemberMatches` over
                            // `[T, undefined]`. The bare reason both (a) renders the
                            // evaluated leaf (`number`) where `T` is a still-deferred
                            // application (e.g. the `DP<number>` value of a recursive
                            // `DeepPartial`-style mapped type), and (b) drops the
                            // spurious `| undefined` and "Did you mean" suggestion tsc
                            // never shows for a sole-real-member nullable target. Object
                            // sources are excluded so their per-property elaboration is
                            // unaffected.
                            SubtypeFailureReason::TypeMismatch { .. }
                            | SubtypeFailureReason::IntrinsicTypeMismatch { .. }
                            | SubtypeFailureReason::LiteralTypeMismatch { .. } => {
                                !self.is_object_like(source_member)
                            }
                            // Object source with a structural drill reason (a
                            // property-type mismatch, an index-signature
                            // failure, …): tsc elaborates against the sole
                            // real member exactly as if the target were `T`
                            // alone — the head display already folds the
                            // nullish members away, so a member frame here
                            // would duplicate the head line. Promote the
                            // member's own reason; the best-member wrap below
                            // stays for genuine multi-member unions.
                            _ => self.is_object_like(source_member),
                        };
                        if promote {
                            return Some(self.wrap_union_source_member_reason(
                                source,
                                target,
                                witness,
                                &source_members,
                                reason,
                            ));
                        }
                    }
                }
            }
        }

        // Union source against a union target: tsc's union-source loop
        // (`eachTypeRelatedToType`) runs first, so the first failing
        // source member elaborates against the *whole* union target —
        // member header (`Type 'M' is not assignable to type 'U'.`)
        // followed by that member's own union-target elaboration. The
        // best-member selection below only understands object-shaped
        // sources, so recurse per member and wrap the composed
        // union-target reason; any other nested shape keeps the current
        // bare-union-line behavior.
        if source_members.len() > 1 {
            for &source_member in &source_members {
                if source_member == source {
                    // Defensive: avoid self-recursion on a degenerate union.
                    continue;
                }
                if self.check_subtype(source_member, resolved_target).is_true() {
                    continue;
                }
                if let Some(nested @ SubtypeFailureReason::UnionTargetMismatch { .. }) =
                    self.explain_failure_guarded(source_member, target)
                {
                    return Some(SubtypeFailureReason::UnionSourceMismatch {
                        source_type: source,
                        target_type: target,
                        member_type: source_member,
                        nested_reason: Box::new(nested),
                    });
                }
                break;
            }
        }

        // A FRESH object-literal source never reaches tsc's
        // `typeRelatedToSomeType` best-member frame: `isRelatedTo`'s
        // excess-property phase (`hasExcessProperties`, checker.ts) runs
        // first, reduces the union target to the discriminant-matched
        // constituent (else its object-like members), and reports the first
        // source property whose type fails against those members' property
        // types folded directly beneath the head line — no member frame.
        // Non-fresh sources skip that phase and keep the member frame below.
        // Witness family: #17721.
        if source_members.len() == 1
            && let Some(reason) =
                self.fresh_object_literal_union_property_fold(resolved_source, &members)
        {
            return Some(reason);
        }

        // Structural union target: select the best-matching member the way
        // tsc's `getBestMatchingType` does — discriminant first, then
        // key-overlap, and no member at all when nothing overlaps. See
        // [`SubtypeChecker::select_union_target_best_member`].
        let best_member: Option<TypeId> =
            self.select_union_target_best_member(resolved_source, &members);

        // Elaborate against the best member, carrying whatever failure the
        // member relation reports. tsc's `getBestMatchingType` re-runs the
        // failed relation against the selected member with errors enabled,
        // so the chain continues past the union head for every failure
        // kind: a missing required property folds directly beneath the
        // head, and a structural failure (a property-type mismatch, an
        // index-signature mismatch, …) elaborates beneath a
        // `Type 'S' is not assignable to type '<member>'.` member frame —
        // the checker's `UnionTargetMismatch` renderer owns that split.
        // Fresh object-literal sources whose failure is a present property's
        // type mismatch folded above and never reach this reason; the
        // checker's expression elaboration
        // (`try_elaborate_assignment_source_error`) reports at the
        // offending property node first when a property-level anchor exists.
        if let Some(member) = best_member {
            for &source_member in &source_members {
                if self.check_subtype(source_member, member).is_true() {
                    continue;
                }
                if let Some(reason) = self.explain_failure_guarded(source_member, member) {
                    return Some(SubtypeFailureReason::UnionTargetMismatch {
                        source_type: source,
                        target_type: target,
                        member_type: member,
                        nested_reason: Box::new(reason),
                    });
                }
            }
        }

        // Genuine multi-member union source with no best-matching target
        // member: tsc's union-source walk (`eachTypeRelatedToType`) still
        // reports the first failing constituent against the whole union
        // target (`Type 'string | boolean' is not assignable to type
        // 'number | true'.` -> `Type 'false' is not assignable to type
        // 'number | true'.`) instead of stopping at the bare union head.
        // tsc gates that walk off for primitive unions (`boolean`, an enum
        // type), which tsz never resolves to a multi-member list here, so
        // the `len() > 1` gate matches tsc's `!(source.flags & Primitive)`.
        if source_members.len() > 1 {
            for &source_member in &source_members {
                if source_member == source {
                    // Defensive: avoid re-explaining a degenerate union.
                    continue;
                }
                if self.check_subtype(source_member, resolved_target).is_true() {
                    continue;
                }
                let witness = self.boolean_member_failing_half(source_member, resolved_target);
                return Some(SubtypeFailureReason::UnionSourceMismatch {
                    source_type: source,
                    target_type: target,
                    member_type: witness,
                    nested_reason: Box::new(SubtypeFailureReason::TypeMismatch {
                        source_type: witness,
                        target_type: target,
                    }),
                });
            }
        }

        Some(SubtypeFailureReason::NoUnionMemberMatches {
            source_type: source,
            target_union_members: members.to_vec(),
        })
    }

    /// `tsc`'s `hasExcessProperties` union fold for a fresh object-literal
    /// source: reduce the union target to the discriminant-matched member
    /// (`findMatchingDiscriminantType`), falling back to every object-like
    /// member, then report the FIRST source property (declaration order) whose
    /// type fails against those members' property types
    /// (`getTypeOfPropertyInTypes`: per-member property type, `undefined` for
    /// a member lacking the property) as a bare
    /// [`SubtypeFailureReason::PropertyTypeMismatch`] — rendered
    /// `Types of property 'X' are incompatible.` directly beneath the head,
    /// with the property relation's own reason nested below and NO member
    /// frame. Returns `None` for a non-fresh source, a union with no
    /// object-like member, or when no present property fails (a missing
    /// required property keeps the best-member elaboration).
    fn fresh_object_literal_union_property_fold(
        &mut self,
        resolved_source: TypeId,
        members: &[TypeId],
    ) -> Option<SubtypeFailureReason> {
        let shape_id = object_shape_id(self.interner, resolved_source)
            .or_else(|| object_with_index_shape_id(self.interner, resolved_source))?;
        if !self.interner.object_shape(shape_id).is_fresh_literal() {
            return None;
        }
        let check_members: Vec<TypeId> =
            match self.union_discriminant_matched_member(resolved_source, members) {
                Some(member) => vec![member],
                None => members
                    .iter()
                    .copied()
                    .filter(|&member| {
                        let resolved = self.apparent_type_for_keys(member);
                        object_shape_id(self.interner, resolved).is_some()
                            || object_with_index_shape_id(self.interner, resolved).is_some()
                    })
                    .collect(),
            };
        if check_members.is_empty() {
            return None;
        }
        // tsc iterates the fresh literal's properties in DECLARATION order
        // (`getPropertiesOfType`), so when several properties fail, the
        // first-declared one is the reported witness. The stored shape is
        // name-sorted; restore declaration order (0 = unrecorded, kept after
        // the recorded ones in stored order — `sort_by_key` is stable).
        let mut properties: Vec<(u32, tsz_common::interner::Atom, TypeId)> = self
            .interner
            .object_shape(shape_id)
            .properties
            .iter()
            .map(|prop| {
                let order = if prop.declaration_order == 0 {
                    u32::MAX
                } else {
                    prop.declaration_order
                };
                (order, prop.name, prop.type_id)
            })
            .collect();
        properties.sort_by_key(|&(order, _, _)| order);
        for (_, name, source_prop) in properties {
            let mut member_prop_types = Vec::with_capacity(check_members.len());
            let mut any_present = false;
            for &member in &check_members {
                match self.discriminant_property_type(member, name) {
                    Some(prop_type) => {
                        any_present = true;
                        // tsc's read type of an OPTIONAL property carries
                        // `| undefined` under strictNullChecks, so a written
                        // `undefined` value satisfies an optional slot and the
                        // fold moves on to the genuinely failing property.
                        let prop_type = if self.strict_null_checks
                            && prop_type != TypeId::UNDEFINED
                            && self.discriminant_property_is_optional(member, name)
                        {
                            self.interner.union(vec![prop_type, TypeId::UNDEFINED])
                        } else {
                            prop_type
                        };
                        member_prop_types.push(prop_type);
                    }
                    // `getTypeOfPropertyInTypes`: a member without the
                    // property contributes `undefined` to the checked union.
                    None => member_prop_types.push(TypeId::UNDEFINED),
                }
            }
            // A property unknown to every checked member belongs to the
            // excess-property report, not a property-type fold.
            if !any_present {
                continue;
            }
            let target_prop = if let [single] = member_prop_types.as_slice() {
                *single
            } else {
                self.interner.union(member_prop_types)
            };
            if self.check_subtype(source_prop, target_prop).is_true() {
                continue;
            }
            let nested_reason = self
                .explain_failure_guarded(source_prop, target_prop)
                .map(Box::new);
            return Some(SubtypeFailureReason::PropertyTypeMismatch {
                property_name: name,
                source_property_type: source_prop,
                target_property_type: target_prop,
                nested_reason,
            });
        }
        None
    }
}
