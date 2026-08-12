//! Faithful port of tsc's `getUnionSignatures` (checker.ts).
//!
//! When a property is accessed on a union type, the call/construct signatures of
//! the union are *not* the simple concatenation of each member's signatures.
//! TypeScript combines them: a signature is usable on the union only when every
//! member contributes a compatible signature, and the combined signature
//! intersects parameter types (contravariant) and unions return types.
//!
//! The subtle case this module exists for: a union where exactly one member
//! declares the property with an *overload set* (multiple call signatures). The
//! per-position "combined signature" path in `call_resolution` cannot represent
//! that member, so without this algorithm the union is wrongly reported as not
//! callable (`TS2349`) / not constructable (`TS2351`). tsc handles it in two
//! passes:
//!
//! 1. For each candidate signature, try to find a matching signature in every
//!    member (exact identity first, then a subtype "partial" match). When found
//!    in all members, combine them into one union signature.
//! 2. If no single signature is common to all members but *only one* member has
//!    multiple signatures, use that member's overload set as a master list and
//!    combine each master signature with the (single) signature of every other
//!    member.
//!
//! The result is the union's signature list; an empty result means the union is
//! not callable/constructable with this signature kind.

use super::call_evaluator::{AssignabilityChecker, CallEvaluator};
use crate::types::{CallSignature, CallableShape, ParamInfo, TypeId};
use tsz_common::interner::Atom;

/// Which union member, if any, carries an overload set. Mirrors tsc's
/// `indexWithLengthOverOne` (`undefined` / a member index / `-1` for
/// "more than one member is overloaded").
#[derive(Clone, Copy)]
enum OverloadedMember {
    None,
    One(usize),
    Many,
}

impl<C: AssignabilityChecker> CallEvaluator<'_, C> {
    /// Shared entry for the tsc `getUnionSignatures` path used by both the call
    /// (`resolve_union_call`) and construct (`resolve_union_new`) resolvers.
    ///
    /// When a union member declares the property with an overload set (multiple
    /// signatures) and no member constrains `this`, build the union's combined
    /// signature list and return a synthesized callable carrying it, to be
    /// resolved as an overload set. Returns `None` — falling through to the
    /// existing per-member logic — when there is no overloaded member, when some
    /// member contributes no signatures of this kind, when any signature carries
    /// a `this` type (those route through the precise TS2349-vs-TS2684 branches),
    /// or when the union has no combined signature (a genuine negative).
    pub(crate) fn union_overloaded_member_callable(
        &mut self,
        members: &[TypeId],
        signature_lists: &[(usize, Vec<CallSignature>)],
        is_construct: bool,
    ) -> Option<TypeId> {
        let has_overloaded_member = signature_lists.iter().any(|(_, sigs)| sigs.len() > 1);
        let every_member_contributes = signature_lists.len() == members.len();
        let any_sig_has_this = signature_lists
            .iter()
            .any(|(_, sigs)| sigs.iter().any(|sig| sig.this_type.is_some()));
        if !has_overloaded_member || !every_member_contributes || any_sig_has_this {
            return None;
        }

        let lists: Vec<&[CallSignature]> = signature_lists
            .iter()
            .map(|(_, sigs)| sigs.as_slice())
            .collect();
        let union_sigs = self.get_union_signatures(&lists);
        if union_sigs.is_empty() {
            return None;
        }

        let shape = if is_construct {
            CallableShape {
                construct_signatures: union_sigs,
                ..CallableShape::default()
            }
        } else {
            CallableShape {
                call_signatures: union_sigs,
                ..CallableShape::default()
            }
        };
        Some(self.interner.callable(shape))
    }

    /// Port of tsc `getUnionSignatures`. `signature_lists` holds one signature
    /// list per union member, in member order. Returns the union's combined
    /// signature list (empty when the union has no signatures of this kind).
    pub(crate) fn get_union_signatures(
        &mut self,
        signature_lists: &[&[CallSignature]],
    ) -> Vec<CallSignature> {
        let mut result: Option<Vec<CallSignature>> = None;
        let mut overloaded_member = OverloadedMember::None;

        for (i, list) in signature_lists.iter().enumerate() {
            if list.is_empty() {
                // A member with no signatures of this kind makes the whole union
                // have none. (tsc returns `emptyArray`.)
                return Vec::new();
            }
            if list.len() > 1 {
                overloaded_member = match overloaded_member {
                    OverloadedMember::None => OverloadedMember::One(i),
                    _ => OverloadedMember::Many,
                };
            }
            for signature in *list {
                // Only process signatures whose parameter list isn't already
                // represented in the result (tsc `findMatchingSignature` guard).
                let already_present = match &result {
                    Some(existing) => self
                        .find_matching_signature_for_union(existing, signature, false, true)
                        .is_some(),
                    None => false,
                };
                if already_present {
                    continue;
                }
                if let Some(union_signatures) =
                    self.find_matching_signatures_for_union(signature_lists, signature, i)
                {
                    let combined = if union_signatures.len() > 1 {
                        // Union the matched signatures (intersect params, union returns).
                        let mut acc = union_signatures[0].clone();
                        for next in &union_signatures[1..] {
                            acc = self.combine_union_member_signatures(&acc, next);
                        }
                        acc
                    } else {
                        signature.clone()
                    };
                    result.get_or_insert_with(Vec::new).push(combined);
                }
            }
        }

        let no_common_signature = result.as_ref().is_none_or(Vec::is_empty);
        // Second pass runs only when no single signature subsumes every member and
        // at most one member is overloaded.
        if let (true, OverloadedMember::None | OverloadedMember::One(_)) =
            (no_common_signature, overloaded_member)
        {
            // Use the overloaded member's signatures as the master list and combine
            // each with the first signature of every other member.
            let master_idx = match overloaded_member {
                OverloadedMember::One(idx) => idx,
                _ => 0,
            };
            let mut results: Option<Vec<CallSignature>> =
                Some(signature_lists[master_idx].to_vec());
            for (li, list) in signature_lists.iter().enumerate() {
                if li == master_idx {
                    continue;
                }
                // Members are guaranteed non-empty here (checked in pass one).
                let signature = list[0].clone();
                let current = results.take().unwrap_or_default();
                // Generic signatures combine only when their type-parameter arity
                // matches; otherwise bail (tsc `compareTypeParametersIdentical`).
                if !signature.type_params.is_empty()
                    && current.iter().any(|s| {
                        !s.type_params.is_empty()
                            && s.type_params.len() != signature.type_params.len()
                    })
                {
                    results = None;
                    break;
                }
                let combined = current
                    .iter()
                    .map(|s| self.combine_union_member_signatures(s, &signature))
                    .collect();
                results = Some(combined);
            }
            result = results;
        }

        result.unwrap_or_default()
    }

    /// tsc `findMatchingSignatures`: for an anchor signature, find one matching
    /// signature in every member list. Returns the per-member matches (to be
    /// combined) or `None` when some member has no match.
    fn find_matching_signatures_for_union(
        &mut self,
        signature_lists: &[&[CallSignature]],
        signature: &CallSignature,
        list_index: usize,
    ) -> Option<Vec<CallSignature>> {
        if !signature.type_params.is_empty() {
            // Generic signatures require an exact match, and only the first
            // member's generic signatures may anchor a union signature.
            if list_index > 0 {
                return None;
            }
            for list in &signature_lists[1..] {
                // Propagate `None` (no exact match in this member) up to the caller.
                self.find_matching_signature_for_union(list, signature, false, false)?;
            }
            return Some(vec![signature.clone()]);
        }

        let mut result: Vec<CallSignature> = Vec::with_capacity(signature_lists.len());
        for (i, list) in signature_lists.iter().enumerate() {
            let matched = if i == list_index {
                Some(signature.clone())
            } else {
                // Prefer an exact match (excess optional params, differing return
                // ok), then fall back to a subtype partial match.
                self.find_matching_signature_for_union(list, signature, false, true)
                    .or_else(|| self.find_matching_signature_for_union(list, signature, true, true))
            };
            let sig = matched?;
            append_if_unique(&mut result, sig);
        }
        Some(result)
    }

    /// tsc `findMatchingSignature`: first signature in `list` identical (or, when
    /// `partial_match`, subtype-compatible) to `signature`.
    fn find_matching_signature_for_union(
        &mut self,
        list: &[CallSignature],
        signature: &CallSignature,
        partial_match: bool,
        ignore_return_types: bool,
    ) -> Option<CallSignature> {
        list.iter()
            .find(|candidate| {
                self.compare_signatures_identical_for_union(
                    candidate,
                    signature,
                    partial_match,
                    ignore_return_types,
                )
            })
            .cloned()
    }

    /// tsc `compareSignaturesIdentical` restricted to the union-signature use:
    /// `ignoreThisTypes` is always false here. `source` is the candidate from a
    /// member list; `target` is the anchor signature.
    fn compare_signatures_identical_for_union(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        partial_match: bool,
        ignore_return_types: bool,
    ) -> bool {
        if !is_matching_signature(source, target, partial_match) {
            return false;
        }
        if source.type_params.len() != target.type_params.len() {
            return false;
        }
        // `this` types (compareTypes(sourceThis, targetThis)).
        if let (Some(source_this), Some(target_this)) = (source.this_type, target.this_type)
            && !self.compare_types_for_union(source_this, target_this, partial_match)
        {
            return false;
        }
        // Parameters (compareTypes(targetParam, sourceParam) — contravariant).
        let target_count = param_count(target);
        for i in 0..target_count {
            let s = sig_type_at(self.interner, source, i);
            let t = sig_type_at(self.interner, target, i);
            if !self.compare_types_for_union(t, s, partial_match) {
                return false;
            }
        }
        if !ignore_return_types
            && !self.compare_types_for_union(source.return_type, target.return_type, partial_match)
        {
            return false;
        }
        true
    }

    /// `compareTypesIdentical` (exact) or `compareTypesSubtypeOf` (partial).
    fn compare_types_for_union(&mut self, a: TypeId, b: TypeId, partial_match: bool) -> bool {
        if a == b {
            return true;
        }
        if partial_match {
            self.checker.is_assignable_to(a, b)
        } else {
            self.checker.are_types_identical(a, b)
        }
    }

    /// tsc `combineSignaturesOfUnionMembers` / `combineUnionParameters`:
    /// intersect parameter types, union return types, take the wider arity.
    fn combine_union_member_signatures(
        &mut self,
        left: &CallSignature,
        right: &CallSignature,
    ) -> CallSignature {
        let left_count = param_count(left);
        let right_count = param_count(right);
        let left_is_longest = left_count >= right_count;
        let (longest, shorter) = if left_is_longest {
            (left, right)
        } else {
            (right, left)
        };
        let longest_count = param_count(longest);
        let shorter_count = param_count(shorter);

        let either_has_rest = sig_has_rest(left) || sig_has_rest(right);
        let needs_extra_rest = either_has_rest && !sig_has_rest(longest);

        let left_min = sig_min_args(left);
        let right_min = sig_min_args(right);

        let mut params: Vec<ParamInfo> = Vec::with_capacity(longest_count + 1);
        for i in 0..longest_count {
            let longest_param_type = sig_type_at(self.interner, longest, i);
            let shorter_param_type = if i < shorter_count {
                sig_type_at(self.interner, shorter, i)
            } else {
                TypeId::UNKNOWN
            };
            let union_param_type = self
                .interner
                .intersection2(longest_param_type, shorter_param_type);
            let is_rest_param = either_has_rest && !needs_extra_rest && i == longest_count - 1;
            let is_optional = i >= left_min && i >= right_min;
            let name = combined_param_name(left, right, i);
            params.push(ParamInfo {
                name,
                type_id: if is_rest_param {
                    self.interner.array(union_param_type)
                } else {
                    union_param_type
                },
                optional: is_optional && !is_rest_param,
                rest: is_rest_param,
                arity_only_optional: false,
            });
        }
        if needs_extra_rest {
            // The shorter signature carries the rest tail; widen it to an array.
            let rest_element = sig_type_at(self.interner, shorter, longest_count);
            params.push(ParamInfo {
                name: None,
                type_id: self.interner.array(rest_element),
                optional: false,
                rest: true,
                arity_only_optional: false,
            });
        }

        let this_type = match (left.this_type, right.this_type) {
            (Some(l), Some(r)) => Some(self.interner.intersection2(l, r)),
            (l, r) => l.or(r),
        };

        CallSignature {
            type_params: if left.type_params.is_empty() {
                right.type_params.clone()
            } else {
                left.type_params.clone()
            },
            params,
            this_type,
            return_type: self.interner.union2(left.return_type, right.return_type),
            type_predicate: None,
            is_method: left.is_method || right.is_method,
        }
    }
}

/// Append `sig` unless a structurally-equal signature is already present
/// (tsc `appendIfUnique`, which dedupes by reference; we dedupe structurally).
fn append_if_unique(result: &mut Vec<CallSignature>, sig: CallSignature) {
    if !result
        .iter()
        .any(|existing| signatures_structurally_eq(existing, &sig))
    {
        result.push(sig);
    }
}

fn signatures_structurally_eq(a: &CallSignature, b: &CallSignature) -> bool {
    a.return_type == b.return_type
        && a.this_type == b.this_type
        && a.type_params.len() == b.type_params.len()
        && a.params.len() == b.params.len()
        && a.params
            .iter()
            .zip(&b.params)
            .all(|(x, y)| x.type_id == y.type_id && x.optional == y.optional && x.rest == y.rest)
}

/// tsc `isMatchingSignature`: same required/optional/rest arity, or (partial) the
/// source requires no more arguments than the target.
fn is_matching_signature(
    source: &CallSignature,
    target: &CallSignature,
    partial_match: bool,
) -> bool {
    let source_count = param_count(source);
    let target_count = param_count(target);
    let source_min = sig_min_args(source);
    let target_min = sig_min_args(target);
    let source_has_rest = sig_has_rest(source);
    let target_has_rest = sig_has_rest(target);
    if source_count == target_count
        && source_min == target_min
        && source_has_rest == target_has_rest
    {
        return true;
    }
    partial_match && source_min <= target_min
}

/// Number of parameters; a rest parameter counts as one (tsc `getParameterCount`
/// for the non-tuple-rest case).
const fn param_count(sig: &CallSignature) -> usize {
    sig.params.len()
}

/// Minimum required argument count (tsc `getMinArgumentCount`): the index after
/// the last required, non-rest parameter.
fn sig_min_args(sig: &CallSignature) -> usize {
    sig.params
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_required())
        .map(|(i, _)| i + 1)
        .max()
        .unwrap_or(0)
}

fn sig_has_rest(sig: &CallSignature) -> bool {
    sig.params.last().is_some_and(|p| p.rest)
}

/// tsc `getTypeAtPosition`: the declared type at `pos`, the rest element type for
/// trailing positions, or `any` when out of range.
fn sig_type_at(
    db: &dyn crate::construction::QueryDatabase,
    sig: &CallSignature,
    pos: usize,
) -> TypeId {
    let has_rest = sig_has_rest(sig);
    let non_rest = sig.params.len() - usize::from(has_rest);
    if pos < non_rest {
        return sig.params[pos].type_id;
    }
    if has_rest
        && let Some(rest) = sig.params.last()
        && let Some(elem) = crate::type_queries::get_array_element_type(db, rest.type_id)
    {
        return elem;
    }
    TypeId::ANY
}

fn combined_param_name(left: &CallSignature, right: &CallSignature, i: usize) -> Option<Atom> {
    let left_name = left.params.get(i).and_then(|p| p.name);
    let right_name = right.params.get(i).and_then(|p| p.name);
    match (left_name, right_name) {
        // tsc keeps the name only when it is unambiguous across both members:
        // two conflicting names (or none at all) drop to an unnamed parameter.
        (Some(l), Some(r)) if l == r => Some(l),
        (Some(l), None) => Some(l),
        (None, Some(r)) => Some(r),
        (Some(_), Some(_)) | (None, None) => None,
    }
}
