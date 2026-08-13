//! Error Explanation API for subtype checking.
//!
//! This module implements the "slow path" for generating structured failure reasons
//! when a subtype check fails. It re-runs subtype logic with tracing to produce
//! detailed error diagnostics (TS2322, TS2739, TS2740, TS2741, etc.).

use crate::def::resolver::TypeResolver;
use crate::diagnostics::SubtypeFailureReason;
use crate::diagnostics::format::TypeFormatter;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::SubtypeChecker;
use crate::relations::subtype::explain_guard::{ExplainFuelState, ExplainRecursionEntryState};
use crate::relations::subtype::explain_union_order::reorder_union_members_nullish_first;
use crate::type_queries::data::get_object_symbol;
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectShape, ObjectShapeId, PropertyInfo, TupleElement,
    TupleListId, TypeId, Visibility,
};
use crate::utils;
use crate::visitor::is_type_parameter;
use crate::visitor::{
    application_id, array_element_type, callable_shape_id, function_shape_id, intrinsic_kind,
    literal_value, object_shape_id, object_with_index_shape_id, readonly_inner_type, tuple_list_id,
    union_list_id,
};

/// Work budget for one failure-explanation traversal; see
/// [`SubtypeChecker::explain_eval_fuel`] (issue #13243).
///
/// Sized well above any legitimate single-diagnostic elaboration — the deepest
/// `tsc`-parity chains drill at most a few hundred distinct sub-types — and far
/// below the unbounded breadth of a pathological deeply-generic relation, so it
/// never alters rendered diagnostics on terminating workloads while still
/// bounding the explain pass on inputs that would otherwise not terminate. The
/// magnitude mirrors the proven eval-fuel bound added to the diagnostic display
/// pass in PR #13176.
pub(crate) const EXPLAIN_EVAL_BUDGET: u32 = 16_000;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Array element type, transparently peeling a single `readonly` array
    /// wrapper (`readonly T[]` / `ReadonlyArray<T>`).
    ///
    /// tsc walks a readonly array element exactly like a mutable one when
    /// elaborating an assignment failure, so the explanation must reach the
    /// element regardless of the `readonly` modifier. Without this peel,
    /// `readonly number[]` vs `readonly string[]` loses its nested
    /// `Type 'number' is not assignable to type 'string'.` line (the modifier
    /// itself is not the failure — the element relation is).
    fn array_element_peeling_readonly(&self, ty: TypeId) -> Option<TypeId> {
        array_element_type(self.interner, ty).or_else(|| {
            readonly_inner_type(self.interner, ty)
                .and_then(|inner| array_element_type(self.interner, inner))
        })
    }

    /// Tuple element list, transparently peeling a single `readonly` tuple
    /// wrapper (`readonly [..]`). Mirrors [`Self::array_element_peeling_readonly`]
    /// so readonly tuple mismatches still elaborate the offending position
    /// (`Type at position N in source is not compatible ...`).
    fn tuple_list_peeling_readonly(&self, ty: TypeId) -> Option<TupleListId> {
        tuple_list_id(self.interner, ty).or_else(|| {
            readonly_inner_type(self.interner, ty)
                .and_then(|inner| tuple_list_id(self.interner, inner))
        })
    }

    /// First target tuple position an unbounded array source cannot satisfy,
    /// modeling the source as an all-rest slot list (`[...E[]]`).
    ///
    /// The array guarantees no value at any fixed position, so the first
    /// *required* element yields the `TS2623` reason — including a required
    /// element that trails a rest (`[...number[], string]` reports position
    /// `1`) — and a *variadic* spread (`...T`, a non-array rest) yields
    /// `TS2624`. A plain array rest (`...E[]`) is filled by the source spread,
    /// so the scan skips past it (rather than stopping) to reach any trailing
    /// required slot; optional elements may be legitimately omitted, so they are
    /// skipped too. Returns `None` when no such position exists (e.g. an
    /// all-optional or pure-array-rest target), so the caller falls through to
    /// element-type elaboration without altering the established reason. (#14816)
    fn array_source_tuple_position_no_match(
        &self,
        target: &[TupleElement],
    ) -> Option<SubtypeFailureReason> {
        for (position, elem) in target.iter().enumerate() {
            if elem.rest {
                // A concrete array rest (`...E[]`) is covered by the source
                // spread, so keep scanning for a trailing required slot; a
                // variadic spread of a generic/tuple (`...T`) is not coverable
                // and fails at its own position.
                if array_element_type(self.interner, elem.type_id).is_none() {
                    return Some(SubtypeFailureReason::SourceProvidesNoMatch {
                        position,
                        variadic: true,
                    });
                }
            } else if elem.is_required() {
                return Some(SubtypeFailureReason::SourceProvidesNoMatch {
                    position,
                    variadic: false,
                });
            }
            // An optional element may be legitimately omitted by the source, so
            // it does not force a match; keep scanning.
        }
        None
    }

    fn shape_or_type_requires_declared_index_signature(
        &self,
        shape: &ObjectShape,
        type_id: TypeId,
    ) -> bool {
        let is_named_non_enum = |shape: &ObjectShape| {
            shape.symbol.is_some()
                && !shape
                    .flags
                    .contains(crate::types::ObjectFlags::ENUM_NAMESPACE)
        };
        if is_named_non_enum(shape) {
            return true;
        }

        let receiver_shape = object_with_index_shape_id(self.interner, type_id).or_else(|| {
            let app_id = application_id(self.interner, type_id)?;
            let app = self.interner.type_application(app_id);
            object_with_index_shape_id(self.interner, app.base)
        });

        receiver_shape
            .map(|shape_id| self.interner.object_shape(shape_id))
            .is_some_and(|shape| is_named_non_enum(&shape))
    }

    /// Collect source properties including those from intersection members.
    /// This ensures merged types (e.g., `{ a: string } & { b: number }`) have
    /// all properties available for missing property checks.
    pub(in crate::relations::subtype) fn collect_source_properties(
        &self,
        source: TypeId,
    ) -> Vec<PropertyInfo> {
        use crate::type_queries::data::get_intersection_members;

        let mut props = Vec::new();

        // Get base shape properties
        if let Some(shape_id) = object_shape_id(self.interner, source) {
            let shape = self.interner.object_shape(shape_id);
            props.extend(shape.properties.iter().cloned());
        }

        // Add properties from intersection members
        if let Some(members) = get_intersection_members(self.interner, source) {
            for member in members {
                if let Some(shape_id) = object_shape_id(self.interner, member) {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in shape.properties.iter() {
                        if !props.iter().any(|p| p.name == prop.name) {
                            props.push(prop.clone());
                        }
                    }
                }
            }
        }

        props
    }

    /// Append the deduplicated property-name keys of `shape_id` to `names`.
    fn push_object_shape_property_names(
        &self,
        shape_id: crate::types::ObjectShapeId,
        names: &mut Vec<tsz_common::interner::Atom>,
    ) {
        for prop in self.interner.object_shape(shape_id).properties.iter() {
            if !names.contains(&prop.name) {
                names.push(prop.name);
            }
        }
    }

    /// Resolve `type_id` to its apparent structural form, resolving lazy
    /// aliases and expanding a generic application one level.
    pub(super) fn apparent_type_for_keys(&mut self, type_id: TypeId) -> TypeId {
        let mut resolved = self.resolve_lazy_type(type_id);
        if let Some(app_id) = application_id(self.interner, resolved)
            && let Some(expanded) = self.try_expand_application_type(resolved, app_id)
        {
            resolved = self.resolve_lazy_type(expanded);
        }
        resolved
    }

    /// Collect the property-name keys of an object-like type, resolving lazy
    /// aliases / expanding generic applications and folding intersection
    /// members. Used to score union-member overlap the way tsc's
    /// `findMostOverlappyType` intersects `keyof source` with `keyof member`.
    pub(super) fn object_like_property_names(
        &mut self,
        type_id: TypeId,
    ) -> Vec<tsz_common::interner::Atom> {
        use crate::type_queries::data::get_intersection_members;

        let resolved = self.apparent_type_for_keys(type_id);
        let mut names: Vec<tsz_common::interner::Atom> = Vec::new();
        if let Some(sid) = object_shape_id(self.interner, resolved)
            .or_else(|| object_with_index_shape_id(self.interner, resolved))
        {
            self.push_object_shape_property_names(sid, &mut names);
        }
        if let Some(members) = get_intersection_members(self.interner, resolved) {
            for member in members {
                let resolved_member = self.apparent_type_for_keys(member);
                if let Some(sid) = object_shape_id(self.interner, resolved_member)
                    .or_else(|| object_with_index_shape_id(self.interner, resolved_member))
                {
                    self.push_object_shape_property_names(sid, &mut names);
                }
            }
        }
        names
    }

    /// Returns `true` if `type_id` is function-like — i.e. has at least one
    /// call or construct signature. Used by TS2739/TS2741 explain code to skip
    /// `prototype` from the missing-property list (tsc treats `prototype` as
    /// implicit on any callable value).
    fn type_has_callable_signature(&self, type_id: TypeId) -> bool {
        use crate::type_queries::has_call_signatures;
        if has_call_signatures(self.interner, type_id) {
            return true;
        }
        if let Some(cid) = callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(cid);
            return !shape.call_signatures.is_empty() || !shape.construct_signatures.is_empty();
        }
        if function_shape_id(self.interner, type_id).is_some() {
            return true;
        }
        false
    }

    /// Explain why `source` is not assignable to `target`.
    ///
    /// This is the "slow path" - called only when `is_assignable_to` returns false
    /// and we need to generate an error message. Re-runs the subtype logic with
    /// tracing enabled to produce a structured failure reason.
    ///
    /// Returns `None` if the types are actually compatible (shouldn't happen
    /// if called correctly after a failed check).
    pub fn explain_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // `explain_failure` is both the public entry *and* the recursion entry
        // for nested member / element / branch elaborations (explain_function,
        // explain_tuple, generics_application_helpers all call it). Only the
        // outermost call initializes the per-failure work budget (#13243) — an
        // already-active `Some(_)` fuel means a nested call, which shares the
        // outer budget so the whole elaboration is bounded as one unit rather
        // than refilling fuel at every level.
        if self.explain_eval_fuel.is_some() {
            return self.explain_failure_guarded(source, target);
        }
        self.explain_eval_fuel = Some(self.explain_budget);
        // Route the entry through the guarded path so the recursion guard
        // brackets every elaboration, including the nested branch / member /
        // constraint explanations that call `explain_failure_guarded` directly.
        let result = self.explain_failure_guarded(source, target);
        self.explain_eval_fuel = None;
        result
    }

    /// Guarded elaboration funnel. Every recursive elaboration path (object
    /// members, mapped constraints, conditional branches, tuple/function
    /// elements) routes back through this method, so applying the recursion
    /// guard at this single point bounds the depth of mutually-recursive
    /// conditional/mapped types. Without it, types like zod's
    /// `ZodFormattedError<any>` -- whose conditional branches reintern to
    /// fresh `(source, target)` pairs at every level -- recurse the explain
    /// path until the stack overflows, even though the main relation check is
    /// already depth-bounded.
    fn explain_failure_guarded(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Work budget exhausted (#13243): stop drilling and report the coarse
        // verdict. The boolean relation already decided these types do not
        // relate, so only elaboration detail is dropped — and only on
        // pathological traversals that would otherwise not terminate. On
        // terminating workloads the budget is never reached, so this is inert
        // and rendered output is byte-identical.
        if let Some(reason) =
            ExplainFuelState::from_fuel(self.explain_eval_fuel).fallback_reason(source, target)
        {
            return Some(reason);
        }
        let pair = (source, target);
        let entry_state = ExplainRecursionEntryState::from_recursion_result(self.guard.enter(pair));
        if let Some(reason) = entry_state.fallback_reason(source, target) {
            return Some(reason);
        }
        let result = self.explain_failure_body(source, target);
        self.guard.leave(pair);
        result
    }

    fn explain_failure_body(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // Fast path: if types are equal, no failure
        if source == target {
            return None;
        }

        if !self.strict_null_checks && source.is_nullish() {
            return None;
        }

        // Check for any/unknown/never special cases
        if source.is_any() || target.is_any_or_unknown() {
            return None;
        }
        if source.is_never() {
            return None;
        }
        // ERROR types should produce ErrorType failure reason
        if source.is_error() || target.is_error() {
            return Some(SubtypeFailureReason::ErrorType {
                source_type: source,
                target_type: target,
            });
        }

        // Note: Weak type checking is handled by CompatChecker (compat.rs:167-170).
        // Removed redundant check here to avoid double-checking which caused false positives.

        self.explain_failure_inner(source, target)
    }

    /// Resolve a `TypeQuery(SymbolRef)` type to its structural form for explain.
    ///
    /// Delegates to `resolve_type_query_symbol` (defined in generics.rs) which
    /// resolves via `resolve_ref` (value-space / constructor type) first, then
    /// falls back to `resolve_lazy` for non-class symbols (e.g., namespaces).
    fn resolve_type_query_for_explain(&self, type_id: TypeId) -> TypeId {
        if let Some(sym_ref) =
            crate::type_queries::get_type_query_symbol_ref(self.interner, type_id)
        {
            self.resolve_type_query_symbol(sym_ref)
                .map(|resolved| self.resolve_lazy_type(resolved))
                .unwrap_or(type_id)
        } else {
            type_id
        }
    }

    fn explain_failure_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        // `S[T1]` vs `S[T2]` where T1/T2 are distinct type parameters:
        // surface the tsc-parity TS2322 + TS5075 elaboration chain. Done
        // before any resolution/evaluation so the user-written types
        // appear verbatim and the IndexAccess shape isn't collapsed by
        // evaluate_type into an opaque defer. This is the defense-in-depth
        // path; in the common checker pipeline the inputs are evaluated
        // before reaching here and the same elaboration is surfaced from
        // the checker boundary.
        if let Some(reason) = self.explain_index_access_distinct_type_param_keys(source, target) {
            return Some(reason);
        }

        // Resolve lazy types (interfaces, type aliases) to their structural forms.
        // Without this, interface types (TypeData::Lazy) won't match the object_shape_id
        // check below, causing TS2322 instead of TS2741/TS2739/TS2740.
        let mut resolved_source = self.resolve_lazy_type(source);
        let mut resolved_target = self.resolve_lazy_type(target);

        // Resolve TypeQuery types (typeof X) to their value-space structural forms.
        // Without this, `typeof Namespace` types remain as TypeQuery(SymbolRef) and
        // skip property comparison, preventing TS2741 from being emitted.
        resolved_source = self.resolve_type_query_for_explain(resolved_source);
        resolved_target = self.resolve_type_query_for_explain(resolved_target);

        // Same-generic application (`C<A..>` vs `C<B..>`): tsc elaborates the
        // differing type arguments directly rather than recursing into a
        // structural property comparison. Detect this before expanding the
        // applications below — expansion would replace them with object shapes
        // and route into the `Types of property 'x' are incompatible.` path
        // that tsc does not emit for same-generic argument mismatches.
        if let Some(reason) =
            self.explain_same_generic_type_arguments(resolved_source, resolved_target)
        {
            return Some(reason);
        }

        // Expand applications (like Array<number>, MyGeneric<string>) to structural forms
        if let Some(app_id) = crate::visitor::application_id(self.interner, resolved_source)
            && let Some(expanded) = self.try_expand_application_type(resolved_source, app_id)
        {
            resolved_source = self.resolve_lazy_type(expanded);
        }
        if let Some(app_id) = crate::visitor::application_id(self.interner, resolved_target)
            && let Some(expanded) = self.try_expand_application_type(resolved_target, app_id)
        {
            resolved_target = self.resolve_lazy_type(expanded);
        }

        // TSC emits TS4104 when a readonly array/tuple is assigned to a mutable
        // array/tuple target. This check must happen before structural analysis —
        // readonly-to-mutable is the primary failure reason and short-circuits further
        // elaboration. When the target is a type parameter (not a concrete
        // array/tuple), the short-circuit depends on the source: readonly plain
        // arrays may still produce TS4104 via the existing constraint heuristic,
        // but a readonly source whose inner is a *tuple* (e.g. `readonly [...T]`)
        // must fall through to structural analysis so the tsc-parity TS2322 path
        // can report it — see variadicTuples1.ts:160 where `t: T` (target is a
        // type parameter) gets TS2322, while `m: [...T]` on line 162 gets TS4104.
        if let Some(readonly_source_inner) = readonly_inner_type(self.interner, resolved_source)
            && readonly_inner_type(self.interner, resolved_target).is_none()
        {
            let is_mutable_array_or_tuple = array_element_type(self.interner, resolved_target)
                .is_some()
                || tuple_list_id(self.interner, resolved_target).is_some();
            let source_inner_is_tuple =
                tuple_list_id(self.interner, readonly_source_inner).is_some();
            let is_type_param_with_array_constraint = !is_mutable_array_or_tuple
                && !source_inner_is_tuple
                && is_type_parameter(self.interner, resolved_target)
                && crate::visitor::type_param_info(self.interner, resolved_target)
                    .and_then(|info| info.constraint)
                    .is_some_and(|constraint| {
                        let resolved_constraint = self.resolve_lazy_type(constraint);
                        array_element_type(self.interner, resolved_constraint).is_some()
                            || tuple_list_id(self.interner, resolved_constraint).is_some()
                    });
            if is_mutable_array_or_tuple || is_type_param_with_array_constraint {
                return Some(SubtypeFailureReason::ReadonlyToMutableAssignment {
                    source_type: source,
                    target_type: target,
                });
            }
        }

        // TSC emits TS2322 (generic "not assignable") instead of TS2741/TS2739
        // when the target type is an intersection. Intersection types combine
        // constraints from multiple sources, so drilling into individual member
        // properties is misleading. Return TypeMismatch so the checker emits TS2322.
        // Check BEFORE evaluate_type, which may merge intersection members into
        // a single object, losing the intersection information.
        if let Some(reason) =
            self.explain_intersection_target(source, target, resolved_source, resolved_target)
        {
            return Some(reason);
        }

        // Evaluate meta-types (Mapped, Conditional, KeyOf, etc.) to structural forms.
        // Application expansion may produce a Mapped type (e.g., Required<Foo> →
        // { [K in keyof Foo]-?: Foo[K] }) which needs further evaluation to a concrete
        // object type so property enumeration can generate TS2739/TS2741 diagnostics.
        //
        // Preserve the pre-evaluation source union for member elaboration. tsc
        // applies `UnionReduction.Literal` to written/annotation unions: it absorbs
        // literals into their primitive but never drops a member merely because it
        // is a structural subtype of a sibling. `evaluate_type` re-normalizes via
        // the default (subtype-reducing) union path, so a written union like
        // `string[] | [string, string]` collapses to `string[]` here even though
        // tsc keeps both members. Capture the member-preserving union so the
        // union-source elaboration below can still drill into the failing member.
        let pre_eval_source = resolved_source;
        let eval_source = self.evaluate_type(resolved_source);
        if eval_source != resolved_source {
            resolved_source = eval_source;
        }
        let eval_target = self.evaluate_type(resolved_target);
        if eval_target != resolved_target {
            resolved_target = eval_target;
        }

        // A type-parameter source matches none of the structural arms below —
        // it has no object/tuple/union/primitive shape of its own — and tsc
        // always elaborates `T <: X` through `T`'s base constraint regardless
        // of the target shape (primitive, object, union, evaluated conditional,
        // ...). Surface that constraint relation before the shape-specific arms
        // so the chain reaches the real root instead of collapsing to a bare
        // `TypeMismatch`/`NoUnionMemberMatches` line.
        if let Some(reason) =
            self.explain_type_parameter_constraint_failure(source, resolved_source, resolved_target)
        {
            return Some(reason);
        }

        if let Some(shape) = self.apparent_primitive_shape_for_type(resolved_source) {
            if let Some(t_shape_id) = object_shape_id(self.interner, resolved_target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_object_failure(
                    source,
                    target,
                    &shape.properties,
                    None,
                    &t_shape.properties,
                );
            }
            if let Some(t_shape_id) = object_with_index_shape_id(self.interner, resolved_target) {
                let t_shape = self.interner.object_shape(t_shape_id);
                let source_kind = self.apparent_primitive_kind(resolved_source);
                let has_string_index = t_shape.string_index.is_some();
                let has_number_index = t_shape.number_index.is_some();
                let allow_indexed_structural = !has_string_index
                    && (!has_number_index || source_kind == Some(IntrinsicKind::String));
                if !allow_indexed_structural {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                return self.explain_indexed_object_failure(source, target, &shape, None, &t_shape);
            }
        }

        // The `object` intrinsic is `NonPrimitive`, not `StructuredType`, so
        // tsc's structural property comparison (which owns TS2741/TS2739/TS2740)
        // is never reached for an `object` source; a failed relation surfaces
        // the generic `TypeMismatch` (TS2322) naming `object` verbatim, not a
        // `{}`-rendered missing-property line (see nonPrimitiveAssignError.ts,
        // and the test header for the full rationale). The empty object type
        // `{}` and members-less interfaces are structured sources and keep the
        // missing-property elaboration through the object-shape arms below.
        // Placed before the union/array/callable-target arms so the generic
        // reason wins regardless of target shape. `intrinsic_kind == Object`
        // already subsumes the `TypeId::OBJECT` identity.
        if intrinsic_kind(self.interner, resolved_source) == Some(IntrinsicKind::Object) {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, resolved_source),
            object_shape_id(self.interner, resolved_target),
        ) {
            let s_props = self.collect_source_properties(resolved_source);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_object_failure(
                source,
                target,
                &s_props,
                Some(s_shape_id),
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, resolved_source),
            object_with_index_shape_id(self.interner, resolved_target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_indexed_object_failure(
                source,
                target,
                &s_shape,
                Some(s_shape_id),
                &t_shape,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_with_index_shape_id(self.interner, resolved_source),
            object_shape_id(self.interner, resolved_target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_object_with_index_to_object_failure(
                source,
                target,
                &s_shape,
                s_shape_id,
                &t_shape.properties,
            );
        }

        if let (Some(s_shape_id), Some(t_shape_id)) = (
            object_shape_id(self.interner, resolved_source),
            object_with_index_shape_id(self.interner, resolved_target),
        ) {
            let s_shape = self.interner.object_shape(s_shape_id);
            let t_shape = self.interner.object_shape(t_shape_id);
            return self.explain_indexed_object_failure(
                source,
                target,
                &s_shape,
                Some(s_shape_id),
                &t_shape,
            );
        }

        // Intersection source vs object target: collect merged properties from all
        // object-like members of the intersection, then check for missing properties.
        // This produces TS2739/TS2741 for branded/intersection types like
        // `number & { __brand: T }` assigned to an object type.
        if crate::visitor::intersection_list_id(self.interner, resolved_source).is_some() {
            let t_shape_id = object_shape_id(self.interner, resolved_target)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_target));
            if let Some(t_sid) = t_shape_id {
                // Reuse the context-free `collect_properties` memo (#13865) so
                // the explain pass does not re-walk the source intersection's
                // full recursive-schema closure that the boolean relation already
                // collected. Mirrors the relation-side sites (`overlap`/`helpers`/
                // `core`) which already thread `query_db`; the explain pass was
                // the remaining bare caller re-collecting from scratch.
                let collected = crate::objects::collect_properties_cached(
                    resolved_source,
                    self.interner,
                    self.resolver,
                    self.query_db,
                );
                if let crate::objects::PropertyCollectionResult::Properties { properties, .. } =
                    collected
                {
                    let t_shape = self.interner.object_shape(t_sid);
                    return self.explain_object_failure(
                        source,
                        target,
                        &properties,
                        None,
                        &t_shape.properties,
                    );
                }
            }
        }

        // Object source vs array target: resolve the array interface to its
        // members and find those missing on the source. tsc emits TS2740 here.
        // Both the mutable `Array<T>` surface and the `readonly T[]` /
        // `ReadonlyArray<T>` surface (the mutable set minus its mutating methods)
        // are handled; without the readonly peel the readonly target previously
        // fell through to a bare TS2322/TS2345.
        if let Some((t_elem, target_readonly)) =
            self.array_or_readonly_array_element(target, resolved_target)
        {
            let s_shape_id = object_shape_id(self.interner, resolved_source)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_source));
            if let Some(s_sid) = s_shape_id
                && let Some(array_base) = self.resolver.get_array_base_type()
            {
                let params = self.resolver.get_array_base_type_params();
                let instantiated = if params.is_empty() {
                    array_base
                } else {
                    let subst = TypeSubstitution::from_args(self.interner, params, &[t_elem]);
                    instantiate_type(self.interner, array_base, &subst)
                };
                let resolved_inst = self.resolve_lazy_type(instantiated);
                // The Array interface may resolve to an object shape or a callable
                // shape (with properties like length, push, concat, etc.).
                let s_shape = self.interner.object_shape(s_sid);
                let target_props = object_shape_id(self.interner, resolved_inst)
                    .or_else(|| object_with_index_shape_id(self.interner, resolved_inst))
                    .map(|sid| self.interner.object_shape(sid).properties.clone())
                    .or_else(|| {
                        callable_shape_id(self.interner, resolved_inst).and_then(|sid| {
                            let callable = self.interner.callable_shape(sid);
                            (!callable.properties.is_empty()).then(|| callable.properties.clone())
                        })
                    });
                if let Some(mut target_props) = target_props {
                    // A readonly-array target omits the mutating methods, so they
                    // are never "missing" — strip them to match tsc's readonly
                    // member list (the mutable arm keeps the full surface).
                    if target_readonly {
                        target_props.retain(|p| {
                            !crate::operations::property::property_helpers::is_array_mutating_method(
                                self.interner.resolve_atom_ref(p.name).as_ref(),
                            )
                        });
                    }
                    return self.explain_object_failure(
                        source,
                        target,
                        &s_shape.properties,
                        Some(s_sid),
                        &target_props,
                    );
                }
            }
        }

        // Array source vs Object target: resolve Array<T> to its interface properties
        // and find missing members. TSC emits TS2739/TS2741 here.
        if let Some(s_elem) = array_element_type(self.interner, resolved_source) {
            let t_shape_id = object_shape_id(self.interner, resolved_target)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_target));
            if let Some(t_sid) = t_shape_id
                && let Some(array_base) = self.resolver.get_array_base_type()
            {
                let params = self.resolver.get_array_base_type_params();
                let instantiated = if params.is_empty() {
                    array_base
                } else {
                    let subst = TypeSubstitution::from_args(self.interner, params, &[s_elem]);
                    instantiate_type(self.interner, array_base, &subst)
                };
                let resolved_inst = self.resolve_lazy_type(instantiated);
                // The Array interface may resolve to an object shape or a callable shape
                let t_shape = self.interner.object_shape(t_sid);
                if let Some(s_obj_sid) = object_shape_id(self.interner, resolved_inst)
                    .or_else(|| object_with_index_shape_id(self.interner, resolved_inst))
                {
                    let s_shape = self.interner.object_shape(s_obj_sid);
                    return self.explain_object_failure(
                        source,
                        target,
                        &s_shape.properties,
                        Some(s_obj_sid),
                        &t_shape.properties,
                    );
                }
                if let Some(callable_sid) = callable_shape_id(self.interner, resolved_inst) {
                    let callable = self.interner.callable_shape(callable_sid);
                    if !callable.properties.is_empty() {
                        return self.explain_object_failure(
                            source,
                            target,
                            &callable.properties,
                            None,
                            &t_shape.properties,
                        );
                    }
                }
            }
        }

        if let (Some(s_fn_id), Some(t_fn_id)) = (
            function_shape_id(self.interner, resolved_source),
            function_shape_id(self.interner, resolved_target),
        ) {
            let s_fn = self.interner.function_shape(s_fn_id);
            let t_fn = self.interner.function_shape(t_fn_id);
            return self.explain_function_failure(&s_fn, &t_fn);
        }

        // A `declare function` symbol's own type — or any other single-signature,
        // property-free Callable — behaves exactly like a bare Function for
        // every structural purpose, but tsz interns it as a `Callable` shape
        // rather than `Function`. When it fails against a genuine `Function`
        // target (e.g. an arrow-type-annotated variable), the Function-vs-Function
        // check above never matches on the source side, so return-type and
        // type-predicate elaboration (TS1224/TS1226) silently produced no
        // structured reason at all. Peel the lone call signature to a
        // `FunctionShape` and reuse the same explainer.
        if let Some(t_fn_id) = function_shape_id(self.interner, resolved_target)
            && let Some(s_callable_id) = callable_shape_id(self.interner, resolved_source)
        {
            let s_callable = self.interner.callable_shape(s_callable_id);
            if let [sig] = s_callable.call_signatures.as_slice()
                && s_callable.construct_signatures.is_empty()
                && s_callable.properties.is_empty()
            {
                let s_fn = Self::function_shape_from_call_signature(sig, false);
                let t_fn = self.interner.function_shape(t_fn_id);
                return self.explain_function_failure(&s_fn, &t_fn);
            }
        }

        if let Some(t_callable_id) = callable_shape_id(self.interner, resolved_target) {
            let t_callable = self.interner.callable_shape(t_callable_id);
            let source_intersection_members =
                crate::type_queries::data::get_intersection_members(self.interner, resolved_source);
            let prefer_property_failure = !t_callable.properties.is_empty()
                && !self.callable_properties_are_only_function_members(&t_callable.properties);
            if !prefer_property_failure && !t_callable.call_signatures.is_empty() {
                if let Some(s_fn_id) = function_shape_id(self.interner, resolved_source) {
                    let s_fn = self.interner.function_shape(s_fn_id);
                    if let Some(reason) =
                        self.explain_function_to_callable_failure(&s_fn, &t_callable)
                    {
                        return Some(reason);
                    }
                }

                if let Some(s_callable_id) = callable_shape_id(self.interner, resolved_source) {
                    let s_callable = self.interner.callable_shape(s_callable_id);
                    if let Some(reason) = self
                        .explain_callable_to_callable_signature_failure(&s_callable, &t_callable)
                    {
                        return Some(reason);
                    }
                }

                if let Some(members) = &source_intersection_members {
                    for member in members.iter() {
                        if let Some(s_fn_id) = function_shape_id(self.interner, *member) {
                            let s_fn = self.interner.function_shape(s_fn_id);
                            if let Some(reason) =
                                self.explain_function_to_callable_failure(&s_fn, &t_callable)
                            {
                                return Some(reason);
                            }
                        }
                        if let Some(s_callable_id) = callable_shape_id(self.interner, *member) {
                            let s_callable = self.interner.callable_shape(s_callable_id);
                            if let Some(reason) = self
                                .explain_callable_to_callable_signature_failure(
                                    &s_callable,
                                    &t_callable,
                                )
                            {
                                return Some(reason);
                            }
                        }
                    }
                }
            }

            // Emit TS2741/TS2739 for missing properties instead of generic TS2322.
            if !t_callable.properties.is_empty() {
                let source_props: Vec<PropertyInfo> = if let Some(s_callable_id) =
                    callable_shape_id(self.interner, resolved_source)
                {
                    self.interner
                        .callable_shape(s_callable_id)
                        .properties
                        .clone()
                } else if let Some(s_shape_id) = object_shape_id(self.interner, resolved_source) {
                    self.interner.object_shape(s_shape_id).properties.clone()
                } else if source_intersection_members.is_some() {
                    // Same memo activation as the intersection-vs-object arm
                    // above (the callable-target explain path is the other bare
                    // caller re-collecting the already-walked source closure).
                    match crate::objects::collect_properties_cached(
                        resolved_source,
                        self.interner,
                        self.resolver,
                        self.query_db,
                    ) {
                        crate::objects::PropertyCollectionResult::Properties {
                            properties, ..
                        } => properties,
                        _ => vec![],
                    }
                } else {
                    vec![]
                };
                return self.explain_object_failure(
                    source,
                    target,
                    &source_props,
                    None,
                    &t_callable.properties,
                );
            }
        }

        // Callable source vs Object target: when a callable type is assigned to an
        // object type, check for missing properties to produce TS2741/TS2739 instead
        // of generic TS2322.
        //
        // This applies to all callable types (functions, methods, constructors).
        // When a function is assigned to an object type, we should report which
        // properties are missing (TS2741/TS2739) rather than just saying it's not
        // assignable (TS2322).
        if let Some(s_callable_id) = callable_shape_id(self.interner, resolved_source) {
            let s_callable = self.interner.callable_shape(s_callable_id);
            if let Some(t_shape_id) = object_shape_id(self.interner, resolved_target)
                .or_else(|| object_with_index_shape_id(self.interner, resolved_target))
            {
                let t_shape = self.interner.object_shape(t_shape_id);
                return self.explain_object_failure(
                    source,
                    target,
                    &s_callable.properties,
                    None,
                    &t_shape.properties,
                );
            }
        }

        if let (Some(s_elem), Some(t_elem)) = (
            self.array_element_peeling_readonly(source),
            self.array_element_peeling_readonly(target),
        ) {
            if !self.check_subtype(s_elem, t_elem).is_true() {
                // Recurse into the element failure so the rendered chain carries
                // the inner reason (matching tsc, which walks an array element
                // exactly like a single-element tuple / numerically keyed
                // property: `number[][]` -> `string[][]` peels one array level
                // at a time, `{ b: T }[]` drills into the offending property).
                let nested_reason = self.explain_failure(s_elem, t_elem).map(Box::new);
                return Some(SubtypeFailureReason::ArrayElementMismatch {
                    source_element: s_elem,
                    target_element: t_elem,
                    nested_reason,
                });
            }
            return None;
        }

        // Object-with-index source vs Tuple target: check for missing numeric properties.
        // When an array-like object type (e.g., interface StrNum extends Array { 0: string; ... })
        // is assigned to a tuple type (e.g., [number, number, number]), detect missing
        // required numeric index properties and produce TS2741 instead of generic TS2322.
        // Only applies to types with index signatures (array-like); plain object types without
        // index signatures fall through to the generic TypeMismatch path, matching tsc behavior.
        if let Some(t_tuple_id) = tuple_list_id(self.interner, resolved_target)
            && let Some(s_sid) = object_with_index_shape_id(self.interner, resolved_source)
        {
            let t_elems = self.interner.tuple_list(t_tuple_id);
            let s_shape = self.interner.object_shape(s_sid);
            let mut missing_props: Vec<tsz_common::interner::Atom> = Vec::new();
            for (i, t_elem) in t_elems.iter().enumerate() {
                if t_elem.is_required() {
                    let prop_name = self.interner.intern_string(&i.to_string());
                    let has_prop = s_shape.properties.iter().any(|p| p.name == prop_name);
                    if !has_prop {
                        missing_props.push(prop_name);
                    }
                }
            }
            if missing_props.len() > 1 {
                return Some(SubtypeFailureReason::MissingProperties {
                    property_names: missing_props,
                    source_type: source,
                    target_type: target,
                });
            }
            if missing_props.len() == 1 {
                return Some(SubtypeFailureReason::MissingProperty {
                    property_name: missing_props[0],
                    source_type: source,
                    target_type: target,
                });
            }
        }

        // Array source vs Tuple target: an unbounded array can never
        // structurally satisfy a tuple's fixed slots. tsc reports the
        // closed-target arity gap (`TS2620`/`TS2621`) when the target has no
        // rest element, and otherwise the first required/variadic position the
        // open source cannot pin (`TS2623`/`TS2624`). The boolean relation has
        // already rejected; this only synthesizes the matching reason chain
        // (the tuple-vs-tuple path below never fires for a non-tuple source). (#14816)
        if let Some(s_elem) = self.array_element_peeling_readonly(resolved_source)
            && let Some(t_tuple_id) = self
                .tuple_list_peeling_readonly(target)
                .or_else(|| self.tuple_list_peeling_readonly(resolved_target))
        {
            let t_elems = self.interner.tuple_list(t_tuple_id);
            if t_elems.iter().any(|elem| elem.rest) {
                // The target carries a rest element, so tsc's closed-target
                // arity gate does not fire; report the first required/variadic
                // slot the open source cannot pin (`TS2623`/`TS2624`).
                if let Some(reason) = self.array_source_tuple_position_no_match(&t_elems) {
                    return Some(reason);
                }
            } else {
                // Closed target: an open array can never satisfy a fixed tuple.
                // Drive the shared arity classifier with the source modeled as a
                // single rest slot `[...E[]]` (`source_min == 0`). The classifier
                // inspects only the rest flag and required counts — never the
                // element type — so it yields the same `TS2620`/`TS2621` reason a
                // variadic-tuple source would, keeping parity with that path.
                let synthetic_source = [TupleElement::rest(s_elem)];
                if let Some(arity) = utils::classify_tuple_arity(&synthetic_source, &t_elems) {
                    return Some(SubtypeFailureReason::TupleArityMismatch(arity));
                }
            }
        }

        if let (Some(s_elems), Some(t_elems)) = (
            self.tuple_list_peeling_readonly(source),
            self.tuple_list_peeling_readonly(target),
        ) {
            let s_elems = self.interner.tuple_list(s_elems);
            let t_elems = self.interner.tuple_list(t_elems);
            return self.explain_tuple_failure(&s_elems, &t_elems);
        }

        if union_list_id(self.interner, resolved_target).is_some() {
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

            // Application-shaped comparison (e.g. assigning to `Foo<X>` that
            // resolves to a union): tsc collapses the elaboration to a direct
            // missing-property line against the application target rather than
            // the structural union members, so keep that first-failing-member
            // behavior here.
            if application_shaped_comparison {
                for &member in members.iter() {
                    if self.check_subtype(resolved_source, member).is_true() {
                        continue;
                    }
                    for &source_member in &source_members {
                        if self.check_subtype(source_member, member).is_true() {
                            continue;
                        }
                        let member_reason = self.explain_failure_guarded(source_member, member);
                        let missing_property = match member_reason {
                            Some(SubtypeFailureReason::MissingProperty {
                                property_name, ..
                            }) => Some(property_name),
                            Some(SubtypeFailureReason::MissingProperties {
                                property_names,
                                ..
                            }) => property_names.first().copied(),
                            _ => None,
                        };
                        if let Some(property_name) = missing_property {
                            return Some(SubtypeFailureReason::MissingProperty {
                                property_name,
                                source_type: source,
                                target_type: target,
                            });
                        }
                    }
                }
                return Some(SubtypeFailureReason::NoUnionMemberMatches {
                    source_type: source,
                    target_union_members: members.to_vec(),
                });
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
                        if self.check_subtype(source_member, sole_member).is_true() {
                            continue;
                        }
                        if let Some(reason) =
                            self.explain_failure_guarded(source_member, sole_member)
                        {
                            let promote = match &reason {
                                // Object/array source missing a required property:
                                // surface the missing-property reason directly
                                // (TS2741 / TS2739 / TS2345).
                                SubtypeFailureReason::MissingProperty { .. }
                                | SubtypeFailureReason::MissingProperties { .. } => true,
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
                                _ => false,
                            };
                            if promote {
                                return Some(reason);
                            }
                        }
                    }
                }
            }

            // Structural union target: select the best-matching member the way
            // tsc's `getBestMatchingType` does — discriminant first, then
            // key-overlap, and no member at all when nothing overlaps. See
            // [`SubtypeChecker::select_union_target_best_member`].
            let best_member: Option<TypeId> =
                self.select_union_target_best_member(resolved_source, &members);

            // Elaborate against the best member, but only when its failure is a
            // missing required property. Property-type mismatches and excess
            // properties on object literals are reported by the checker's
            // object-literal elaboration at the offending property's location;
            // surfacing the bare union line keeps parity for those.
            if let Some(member) = best_member {
                for &source_member in &source_members {
                    if self.check_subtype(source_member, member).is_true() {
                        continue;
                    }
                    if let Some(
                        reason @ (SubtypeFailureReason::MissingProperty { .. }
                        | SubtypeFailureReason::MissingProperties { .. }),
                    ) = self.explain_failure_guarded(source_member, member)
                    {
                        return Some(SubtypeFailureReason::UnionTargetMismatch {
                            source_type: source,
                            target_type: target,
                            member_type: member,
                            nested_reason: Box::new(reason),
                        });
                    }
                }
            }

            return Some(SubtypeFailureReason::NoUnionMemberMatches {
                source_type: source,
                target_union_members: members.to_vec(),
            });
        }

        if let (Some(s_kind), Some(t_kind)) = (
            intrinsic_kind(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            if s_kind != t_kind {
                return Some(SubtypeFailureReason::IntrinsicTypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            return None;
        }

        if literal_value(self.interner, source).is_some()
            && literal_value(self.interner, target).is_some()
        {
            return Some(SubtypeFailureReason::LiteralTypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        if let (Some(lit), Some(t_kind)) = (
            literal_value(self.interner, source),
            intrinsic_kind(self.interner, target),
        ) {
            let compatible = match lit {
                LiteralValue::String(_) => t_kind == IntrinsicKind::String,
                LiteralValue::Number(_) => t_kind == IntrinsicKind::Number,
                LiteralValue::BigInt(_) => t_kind == IntrinsicKind::Bigint,
                LiteralValue::Boolean(_) => t_kind == IntrinsicKind::Boolean,
            };
            if !compatible {
                return Some(SubtypeFailureReason::LiteralTypeMismatch {
                    source_type: source,
                    target_type: target,
                });
            }
            return None;
        }

        if intrinsic_kind(self.interner, source).is_some()
            && literal_value(self.interner, target).is_some()
        {
            return Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        // Union source: the relation failed, so at least one member is not
        // assignable to the target. tsc keeps the root mismatch visible by
        // elaborating the first failing member beneath the union-to-target line
        // (`Type 'A | B' is not assignable to type 'T'.` -> `Type 'B' is not
        // assignable to type 'T'.`). Without this, the chain stops at the bare
        // union line and hides why the assignment fails (e.g. the `undefined`
        // member contributed by an optional property).
        //
        // Prefer the evaluated `resolved_source` when it is still a union (e.g. a
        // conditional/mapped type that evaluates *into* a union). Otherwise fall
        // back to `pre_eval_source`: when subtype reduction during `evaluate_type`
        // collapsed a written union of structurally-related members (e.g.
        // `string[] | [string, string]` -> `string[]`), the member elaboration
        // must still walk the members tsc preserves under `UnionReduction.Literal`.
        let (union_member_source, union_member_list) =
            match union_list_id(self.interner, resolved_source) {
                Some(list) => (resolved_source, Some(list)),
                None => (
                    pre_eval_source,
                    union_list_id(self.interner, pre_eval_source),
                ),
            };
        if let Some(member_list) = union_member_list {
            let members = self.interner.type_list(member_list);
            // Name the same first failing member tsc drills beneath the union
            // line by walking the members in the union *header*'s order. tsc's
            // relation walk and its `typeToString` iterate one type-id-sorted
            // array, so header and nested always agree; tsz keeps two orders —
            // the interner's canonicalization order (`sort_union_members`,
            // allocation identity) and the header's display order
            // (`order_union_members_by_source`) — so the walk must be ranked
            // through the display comparator to match. Feed the comparator the
            // union's as-written source order when the interner recorded one
            // (`union_source_elaboration_origin_override`, a pure reordering of
            // the interned members that fixes anonymous-object source order,
            // #16965); the comparator then floats named/higher-rank members
            // ahead of inline anonymous objects exactly as the header does
            // (`{ z: string } | K` elaborates `K`, not the object — #16980).
            let origin_override =
                self.union_source_elaboration_origin_override(union_member_source, &members);
            let base = origin_override.unwrap_or_else(|| members.to_vec());
            let display_ordered = match self
                .query_db
                .and_then(|db| db.definition_store_for_inference())
            {
                Some(def_store) => TypeFormatter::new(self.interner)
                    .with_def_store(def_store)
                    .order_union_members_for_display(base),
                None => base,
            };
            // tsc's *relation* walk visits the nullish intrinsics first (smallest
            // type ids) even though the header shows them last, so hoist them
            // ahead of the display order — see `reorder_union_members_nullish_first`.
            // Same-rank enum members are already in declaration order: the
            // display comparator breaks their tie on the declaration span
            // (#16513) and the nullish hoist preserves their relative order.
            let ordered = reorder_union_members_nullish_first(&display_ordered);
            for &member in ordered.iter() {
                if member == source || member == union_member_source {
                    // Defensive: avoid self-recursion on a degenerate union.
                    continue;
                }
                if self.check_subtype(member, target).is_true() {
                    continue;
                }
                // Elaborate the first failing member beneath the union-to-target
                // line, mirroring tsc which always drills into that member. Each
                // arm below is a member-failure reason whose render composes
                // under the union line:
                //   * leaf relations, property summaries
                //     (`MissingProperty`/`MissingProperties`), and the
                //     array-element and readonly-to-mutable reasons self-head —
                //     their rendered line already names the member
                //     (`Property 'a' is missing in type '{ b: 2; }' …`,
                //     `Type 'number[]' is not assignable to type 'string[]' …`,
                //     `The type 'readonly [number]' is 'readonly' …`).
                //   * the tuple/property element-type, index-signature, and
                //     function-return mismatches are header-led; the union-source
                //     renderer supplies the `Type 'M' is not assignable to type
                //     'T'.` member header before drilling (`Type at position 0 …`
                //     / `Types of property 'p' …` / `'string' index signatures are
                //     incompatible.` / the bare return-relation leaf).
                //   * `ParameterTypeMismatch` self-heads with the signature
                //     relation line at depth >= 1, so the renderer routes it
                //     through the self-heading path (its own first line doubles as
                //     the member header), then drills `Types of parameters 'a' and
                //     'b' are incompatible.` + the contravariant leaf.
                // Without this the chain collapses to the bare union-to-target
                // line and hides which member fails.
                //
                // The set is intentionally limited to member-failure shapes whose
                // render composes exactly with `tsc` here.
                //
                // Tuple fixed-arity (`TupleElementMismatch`) and variadic-arity
                // (`TupleArityMismatch`) count mismatches are header-led leaves:
                // their `TS2618`/`TS2619` (`Source has N element(s) but target
                // requires/allows only M`) text carries no member name, so the
                // union renderer supplies the `Type 'M' is not assignable to type
                // 'T'.` member header (via `union_member_nested_needs_header`)
                // before drilling the arity leaf — matching tsc:
                //   Type '[2, 3] | [4]' is not assignable to type '[number]'.
                //     Type '[2, 3]' is not assignable to type '[number]'.
                //       Source has 2 element(s) but target allows only 1.
                //
                // Notably excluded:
                //   * `OptionalPropertyRequired`/`ReadonlyPropertyMismatch` — `tsc`
                //     leads these with the member header (and they only arise in
                //     narrow `exactOptionalPropertyTypes` / readonly-index shapes),
                //     so they need the header-led path, not this self-heading one.
                let nested = self.explain_failure_guarded(member, target);
                if let Some(nested) = nested
                    && matches!(
                        nested,
                        SubtypeFailureReason::TypeMismatch { .. }
                            | SubtypeFailureReason::IntrinsicTypeMismatch { .. }
                            | SubtypeFailureReason::LiteralTypeMismatch { .. }
                            | SubtypeFailureReason::MissingProperty { .. }
                            | SubtypeFailureReason::MissingProperties { .. }
                            | SubtypeFailureReason::TupleElementTypeMismatch { .. }
                            | SubtypeFailureReason::TupleElementMismatch { .. }
                            | SubtypeFailureReason::TupleArityMismatch(_)
                            | SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
                            | SubtypeFailureReason::PropertyTypeMismatch { .. }
                            | SubtypeFailureReason::ArrayElementMismatch { .. }
                            | SubtypeFailureReason::IndexSignatureMismatch { .. }
                            | SubtypeFailureReason::ReturnTypeMismatch { .. }
                            | SubtypeFailureReason::ParameterTypeMismatch { .. }
                            | SubtypeFailureReason::ReadonlyToMutableAssignment { .. }
                    )
                {
                    return Some(SubtypeFailureReason::UnionSourceMismatch {
                        source_type: source,
                        target_type: target,
                        member_type: member,
                        nested_reason: Box::new(nested),
                    });
                }
                break;
            }
        }

        // Conditional types that survived `evaluate_type` (i.e. deferred
        // conditionals like `T extends U ? X : Y` where `T` is a type
        // parameter) are not handled by any structural shape arm above and
        // would otherwise collapse to the bare `TypeMismatch` fallback,
        // hiding the actual branch-level relation failure.
        //
        // The structural rule, applicable on either side: a relation
        // involving a deferred conditional fails exactly when at least one
        // of its branches fails the corresponding branch relation. Surface
        // that failing branch as a `ConditionalBranchMismatch` carrying the
        // nested branch reason so the diagnostic chain stays intact.
        if let Some(reason) = self.explain_conditional_branch_failure(
            source,
            target,
            resolved_source,
            resolved_target,
        ) {
            return Some(reason);
        }

        Some(SubtypeFailureReason::TypeMismatch {
            source_type: source,
            target_type: target,
        })
    }

    /// Elaborate a failed `T <: X` relation (where `T` is a type parameter)
    /// through `T`'s declared base constraint, mirroring tsc's
    /// `getBaseConstraintOfType` elaboration.
    ///
    /// The structural rule: a type parameter is assignable to a target only
    /// through its constraint, so when the relation fails the constraint must
    /// also fail against the (evaluated) target. Surface that constraint-level
    /// relation as the nested reason so the diagnostic chain reaches the real
    /// root (`Type '<constraint>' is not assignable to type 'X'.` and deeper)
    /// instead of stopping at `Type 'T' is not assignable to type 'X'.`.
    ///
    /// Independent of the target shape (primitive, object, union, evaluated
    /// conditional, ...) and of any identifier spelling — it operates purely
    /// over the constraint TypeId. Unconstrained type parameters carry an
    /// implicit `unknown` constraint for which tsc adds no elaboration line, so
    /// they fall through to the bare `TypeMismatch` fallback.
    fn explain_type_parameter_constraint_failure(
        &mut self,
        source: TypeId,
        resolved_source: TypeId,
        resolved_target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        let info = crate::visitor::type_param_info(self.interner, resolved_source)?;
        let constraint = info.constraint?;

        // When the *target* is itself a (bare) type parameter, tsc does not
        // elaborate through the source constraint — it reports the
        // `'U' could be instantiated with an arbitrary type ...` caveat instead,
        // which a separate path owns. Adding a constraint chain on top of that
        // caveat would diverge from tsc, so leave those failures alone.
        if is_type_parameter(self.interner, resolved_target) {
            return None;
        }

        let resolved_constraint = self.resolve_lazy_type(constraint);

        // A constraint that reinterns to the parameter itself carries no extra
        // information; avoid emitting a degenerate self-referential chain.
        if resolved_constraint == resolved_source || resolved_constraint == source {
            return None;
        }

        // The relation has already failed. If the constraint nonetheless
        // relates to the target, the parameter failed for a reason other than
        // its constraint (e.g. variance or positional identity); there is no
        // constraint-level root to surface, so defer to the bare fallback.
        if self
            .check_subtype(resolved_constraint, resolved_target)
            .is_true()
        {
            return None;
        }

        let nested = self.explain_failure_guarded(resolved_constraint, resolved_target)?;
        Some(SubtypeFailureReason::TypeParameterConstraintMismatch {
            source_type: source,
            target_type: resolved_target,
            constraint_type: resolved_constraint,
            nested_reason: Box::new(nested),
        })
    }

    /// Detect a deferred-conditional-shaped relation failure and surface the
    /// failing branch as a `ConditionalBranchMismatch`.
    ///
    /// Applies to the three structural shapes:
    ///
    /// 1. **Concrete source vs deferred-conditional target** —
    ///    `S <: (T extends U ? X : Y)`. Strategy 2 of
    ///    `subtype_of_conditional_target` requires `S <: X` *and* `S <: Y`;
    ///    when the relation has already failed, the failing branch is the
    ///    one for which `check_subtype` returns false. Surface that branch.
    /// 2. **Deferred-conditional source vs concrete target** —
    ///    `(T extends U ? X : Y) <: T'`. Strategy 2 of
    ///    `conditional_branches_subtype` requires `X <: T'` *and* `Y <: T'`;
    ///    pick the first failing branch.
    /// 3. **Conditional source vs conditional target** with matching extends
    ///    shape — both `X <: X'` and `Y <: Y'` must hold; pick the first
    ///    failing branch pair.
    ///
    /// True-branch failures are reported before false-branch failures so the
    /// elaboration order is stable across runs and matches the textual
    /// reading order of `T extends U ? X : Y`.
    fn explain_conditional_branch_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        resolved_source: TypeId,
        resolved_target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        use crate::type_queries::data::get_conditional_type;

        // Pick the branch pairs once based on which side is conditional. The
        // three structural shapes all reduce to "try (true-pair, false-pair)
        // in order", so iteration is identical regardless of side. When a
        // side is not a conditional, the same resolved type sits in both
        // branch slots, so the corresponding `(resolved_X, branch_Y)` pair
        // falls out of the same construction.
        //
        // When neither side is a conditional there is nothing to surface and
        // we fall back to the caller's `TypeMismatch`.
        let source_cond = get_conditional_type(self.interner, resolved_source);
        let target_cond = get_conditional_type(self.interner, resolved_target);
        if source_cond.is_none() && target_cond.is_none() {
            return None;
        }

        let (s_true, s_false) = source_cond
            .as_deref()
            .map_or((resolved_source, resolved_source), |s| {
                (s.true_type, s.false_type)
            });
        let (t_true, t_false) = target_cond
            .as_deref()
            .map_or((resolved_target, resolved_target), |t| {
                (t.true_type, t.false_type)
            });
        let pairs = [(s_true, t_true), (s_false, t_false)];

        for (branch_source, branch_target) in pairs {
            if let Some(reason) =
                self.conditional_branch_reason(source, target, branch_source, branch_target)
            {
                return Some(reason);
            }
        }
        None
    }

    /// Build a `ConditionalBranchMismatch` if the branch relation
    /// `branch_source <: branch_target` actually fails. Returns `None` when
    /// the branch relation succeeds (so the caller can try the other
    /// branch) or when the branch pair would re-enter the outer relation —
    /// a self-referential conditional whose branch reinterns to the outer
    /// `(source, target)` pair would otherwise recurse indefinitely back
    /// into the same explain query.
    fn conditional_branch_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
        branch_source: TypeId,
        branch_target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        if branch_source == branch_target {
            return None;
        }
        if branch_source == source && branch_target == target {
            return None;
        }
        if self.check_subtype(branch_source, branch_target).is_true() {
            return None;
        }
        let nested = self.explain_failure_guarded(branch_source, branch_target)?;
        Some(SubtypeFailureReason::ConditionalBranchMismatch {
            source_type: source,
            target_type: target,
            branch_source,
            branch_target,
            nested_reason: Box::new(nested),
        })
    }

    /// Detect `S[T1]` vs `S[T2]` where T1/T2 are distinct type parameters
    /// and the object types resolve to the same underlying shape. Returns
    /// the failure reason that elaborates the TS2322 + TS5075 chain.
    ///
    /// Independent of identifier names by construction: operates over
    /// TypeId shapes, not surface text. Two object halves are accepted
    /// as "the same object" when they share a TypeId, share their
    /// resolved Lazy unwrap, or when `source <: target` holds — the
    /// elaboration is the right shape whenever the source object is
    /// assignable to the target object, even if `target <: source`
    /// does not also hold.
    fn explain_index_access_distinct_type_param_keys(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        let (s_obj, s_idx) = crate::visitor::index_access_parts(self.interner, source)?;
        let (t_obj, t_idx) = crate::visitor::index_access_parts(self.interner, target)?;
        let same_object = s_obj == t_obj
            || self.resolve_lazy_type(s_obj) == self.resolve_lazy_type(t_obj)
            || self.check_subtype(s_obj, t_obj).is_true();
        if !same_object {
            return None;
        }
        self.index_access_distinct_type_param_keys_failure_reason(s_idx, t_idx)
    }

    /// Explain why an object type assignment failed.
    pub(in crate::relations::subtype) fn explain_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_props: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        // First pass: collect all missing required property names.
        // tsc emits TS2739 (multiple missing) or TS2741 (single missing) before
        // checking property type compatibility.
        // Collect with declaration_order so we can sort by source order (tsc lists
        // missing properties in declaration order, not Atom/hash order).
        // For class inheritance, we need to show own properties first, then inherited.
        let target_symbol = get_object_symbol(self.interner, target);
        let mut missing_with_order: Vec<(
            tsz_common::interner::Atom,
            u32,
            Option<tsz_binder::SymbolId>,
        )> = Vec::new();
        let mut seen_names: rustc_hash::FxHashSet<tsz_common::interner::Atom> =
            rustc_hash::FxHashSet::default();
        for t_prop in target_props {
            if !t_prop.optional {
                // Skip the synthetic `__private_brand_*` marker (`tsc` never reports it
                // missing) so a distinct-brand pair reaches the nominal second pass.
                if crate::utils::is_synthetic_private_brand_name(
                    self.interner.resolve_atom_ref(t_prop.name).as_ref(),
                ) {
                    continue;
                }
                let s_prop = self.lookup_property(source_props, source_shape_id, t_prop.name);
                // A target member absent from the source's own properties is
                // still not "missing" when the implicit `Object.prototype`
                // members every object value carries supply it: tsc's
                // `getUnmatchedProperties` resolves each target name through
                // `getPropertyOfType(source, …)`, whose lookup falls back to
                // the global `Object` interface, so `toString`/`toLocaleString`
                // etc. never appear in a TS2739/TS2740/TS2741 missing list —
                // even when the source's own member of that name would be
                // incompatible (presence, not compatibility, is what removes
                // it from the *missing* list; a bad own member surfaces as a
                // property mismatch instead).
                if s_prop.is_none()
                    && self.get_object_base_property(t_prop.name).is_none()
                    && seen_names.insert(t_prop.name)
                {
                    missing_with_order.push((
                        t_prop.name,
                        t_prop.declaration_order,
                        t_prop.parent_id,
                    ));
                }
            }
        }
        missing_with_order.sort_by(
            |(left_name, left_order, left_parent), (right_name, right_order, right_parent)| {
                let name_order = || {
                    self.interner
                        .resolve_atom_ref(*left_name)
                        .cmp(&self.interner.resolve_atom_ref(*right_name))
                };
                // For class types, own properties (where parent_id matches the target symbol)
                // should come before inherited properties
                let left_is_own = target_symbol.is_some() && *left_parent == target_symbol;
                let right_is_own = target_symbol.is_some() && *right_parent == target_symbol;

                match (left_is_own, right_is_own) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    (true, true) => {
                        // When both are own properties of the target, tsc lists
                        // them in source-declaration order. Genuine source
                        // members carry non-zero `declaration_order`, so sort
                        // by it. Synthesized members (e.g. class `prototype`)
                        // carry `declaration_order == 0` and stay first via
                        // the (false, true) tie-break below; stable `sort_by`
                        // preserves their relative order.
                        match (*left_order > 0, *right_order > 0) {
                            (true, true) => {
                                return left_order.cmp(right_order).then_with(name_order);
                            }
                            (false, true) => return std::cmp::Ordering::Less,
                            (true, false) => return std::cmp::Ordering::Greater,
                            (false, false) => return std::cmp::Ordering::Equal,
                        }
                    }
                    (false, false) => {}
                }

                // Inherited-on-both-sides path: fall through to declaration_order
                // (1-based) comparison, with alphabetic tie-break for synthesized
                // properties. This preserves the prior interface-merge ordering.
                match (*left_order > 0, *right_order > 0) {
                    (true, true) => left_order.cmp(right_order).then_with(name_order),
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (false, false) => name_order(),
                }
            },
        );
        // Well-known-symbol members (`[Symbol.iterator]`, `[Symbol.unscopables]`,
        // `[Symbol.species]`, …) are ordinary members of the missing list: tsc
        // counts and lists them like any other property, for array-like and
        // non-array targets alike (e.g. `Type 'C' is missing the following
        // properties from type 'Bar[]': length, pop, push, concat, and 24 more.`
        // — the 24 includes `[Symbol.unscopables]` when the source supplies its
        // own `[Symbol.iterator]`, and a lone missing symbol member renders as
        // `Property '[Symbol.iterator]' is missing in type 'X' …`). The
        // `Object.prototype` presence fallback above is the only implicit
        // matching tsc applies here.

        // tsc treats `prototype` as implicit on callable sources (any function
        // or class value has a `.prototype` in JS), so it never lists it as a
        // missing property — even when comparing a plain function type against
        // an interface like `ArrayConstructor` that declares `prototype`.
        // Strip it here if the source has call or construct signatures.
        if !missing_with_order.is_empty() && self.type_has_callable_signature(source) {
            let prototype_atom = self.interner.intern_string("prototype");
            missing_with_order.retain(|(name, _, _)| *name != prototype_atom);
        }
        let missing_props: Vec<tsz_common::interner::Atom> = missing_with_order
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();

        if missing_props.len() > 1 {
            return Some(SubtypeFailureReason::MissingProperties {
                property_names: missing_props,
                source_type: source,
                target_type: target,
            });
        }
        if missing_props.len() == 1 {
            return Some(SubtypeFailureReason::MissingProperty {
                property_name: missing_props[0],
                source_type: source,
                target_type: target,
            });
        }

        // Second pass: check property type compatibility
        for t_prop in target_props {
            let s_prop = self.lookup_property(source_props, source_shape_id, t_prop.name);

            if let Some(sp) = s_prop {
                // Check nominal identity for private/protected properties.
                // `protected` is hierarchical (shared `nominal_member_origin_ok`).
                if t_prop.visibility != Visibility::Public {
                    if !self.nominal_member_origin_ok(
                        t_prop.name,
                        sp.parent_id,
                        t_prop.parent_id,
                        t_prop.visibility,
                    ) {
                        // An ES private identifier (`#name`) is a per-class
                        // slot: tsc reports "refers to a different member"
                        // (TS18015), while a modifier-`private` member gets
                        // "separate declarations" (TS2446).
                        return Some(
                            if crate::utils::is_es_private_identifier_name(
                                self.interner.resolve_atom_ref(t_prop.name).as_ref(),
                            ) {
                                SubtypeFailureReason::PrivateIdentifierMemberMismatch {
                                    property_name: t_prop.name,
                                }
                            } else {
                                SubtypeFailureReason::PropertyNominalMismatch {
                                    property_name: t_prop.name,
                                }
                            },
                        );
                    }
                }
                // Cannot assign private/protected source to public target
                else if sp.visibility != Visibility::Public {
                    return Some(SubtypeFailureReason::PropertyVisibilityMismatch {
                        property_name: t_prop.name,
                        source_visibility: sp.visibility,
                        target_visibility: t_prop.visibility,
                    });
                }

                // Check property type compatibility first.
                //
                // The optional-vs-required message (TS2327) only applies when the
                // property *types* are otherwise compatible, so optionality is the
                // sole reason the relation fails. When the read types are themselves
                // incompatible (e.g. `{a?: number}` vs `{a: number}`, where the
                // optional source contributes `number | undefined` that is not
                // assignable to `number`), tsc reports the type-incompatibility chain
                // ("Types of property 'a' are incompatible." -> root mismatch) and
                // does *not* collapse it to the optional/required line. Emitting
                // TS2327 before this check would hide that root mismatch.
                let source_type = self.optional_property_type(sp);
                let target_type = self.optional_property_type(t_prop);
                let allow_bivariant = t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    let nested = self.explain_failure_with_method_variance(
                        source_type,
                        target_type,
                        allow_bivariant,
                    );
                    return Some(SubtypeFailureReason::PropertyTypeMismatch {
                        property_name: t_prop.name,
                        source_property_type: source_type,
                        target_property_type: target_type,
                        nested_reason: nested.map(Box::new),
                    });
                }

                // Read types are compatible: now optionality presence is the only
                // remaining incompatibility (TS2327). This also covers
                // `{a?: T}` vs `{a: T | undefined}` and exactOptionalPropertyTypes,
                // where the read types match but the source may still be absent.
                if sp.optional && !t_prop.optional {
                    return Some(SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: t_prop.name,
                    });
                }
                // Sound Mode only: tsc never relates split-accessor write
                // types (mirrors the gate in check_property_types).
                if self.check_split_accessor_writes
                    && !t_prop.readonly
                    && !sp.readonly
                    && (sp.has_split_accessor() || t_prop.has_split_accessor())
                {
                    let source_write = self.optional_property_write_type(sp);
                    let target_write = self.optional_property_write_type(t_prop);
                    if !self
                        .check_subtype_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        let nested = self.explain_failure_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        );
                        return Some(SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: t_prop.name,
                            source_property_type: source_write,
                            target_property_type: target_write,
                            nested_reason: nested.map(Box::new),
                        });
                    }
                }
            }
        }

        None
    }

    /// Build the `IndexSignatureMismatch` reason for a failing index-to-index or
    /// property-to-index check, applying the `MissingProperty` priority rule:
    /// when the nested failure is `MissingProperty` or `MissingProperties`,
    /// bubble it up directly so the diagnostic reports the missing property
    /// rather than wrapping it in an index-signature incompatibility.
    /// `property_name` distinguishes the two incompatibility shapes `tsc`
    /// renders differently: `Some(name)` is a named source **property** measured
    /// against the target index (TS2530 "Property '{name}' is incompatible with
    /// index signature."), `None` is a source **index signature** vs the target
    /// index (TS2634 "'{kind}' index signatures are incompatible.").
    pub(in crate::relations::subtype) fn make_index_sig_reason(
        &mut self,
        index_kind: &'static str,
        source_value_type: TypeId,
        target_value_type: TypeId,
        property_name: Option<tsz_common::interner::Atom>,
    ) -> Option<SubtypeFailureReason> {
        let nested = self.explain_failure(source_value_type, target_value_type);
        if matches!(
            nested,
            Some(
                SubtypeFailureReason::MissingProperty { .. }
                    | SubtypeFailureReason::MissingProperties { .. }
            )
        ) {
            return nested;
        }
        Some(SubtypeFailureReason::IndexSignatureMismatch {
            index_kind,
            source_value_type,
            target_value_type,
            nested_reason: nested.map(Box::new),
            property_name,
        })
    }

    /// Explain why an indexed object type assignment failed.
    fn explain_indexed_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target_shape: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        // First check properties
        if let Some(reason) = self.explain_object_failure(
            source,
            target,
            &source_shape.properties,
            source_shape_id,
            &target_shape.properties,
        ) {
            return Some(reason);
        }

        // Check string index signature
        if let Some(t_string_idx) = target_shape.string_index_signature() {
            match source_shape.string_index_signature() {
                Some(s_string_idx) => {
                    if s_string_idx.readonly && !t_string_idx.readonly {
                        return Some(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        });
                    }
                    if !self
                        .check_subtype(s_string_idx.value_type, t_string_idx.value_type)
                        .is_true()
                    {
                        return self.make_index_sig_reason(
                            "string",
                            s_string_idx.value_type,
                            t_string_idx.value_type,
                            None,
                        );
                    }
                }
                None => {
                    // Class/interface types must have an explicit string index
                    // signature — a number index alone is not enough (see
                    // check_string_index_compatibility for the full rationale).
                    if self
                        .requires_explicit_declared_index_signature_for(source_shape, Some(source))
                    {
                        return Some(SubtypeFailureReason::MissingIndexSignature {
                            index_kind: "string",
                        });
                    }
                    // Source properties measured against the target string index
                    // are explained by the shared
                    // `explain_properties_against_index_signatures` fallback at
                    // the end of this function (it carries the property name so
                    // the renderer selects TS2530). Duplicating that loop here
                    // only risked the two paths drifting.
                }
            }
        }

        // Check number index signature
        if let Some(ref t_number_idx) = target_shape.number_index {
            if let Some(ref s_number_idx) = source_shape.number_index {
                if s_number_idx.readonly && !t_number_idx.readonly {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                if !self
                    .check_subtype(s_number_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    return self.make_index_sig_reason(
                        "number",
                        s_number_idx.value_type,
                        t_number_idx.value_type,
                        None,
                    );
                }
            } else if let Some(s_string_idx) = source_shape.string_index_signature() {
                if s_string_idx.readonly && !t_number_idx.readonly {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                if !self
                    .check_subtype(s_string_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    return self.make_index_sig_reason(
                        "number",
                        s_string_idx.value_type,
                        t_number_idx.value_type,
                        None,
                    );
                }
            } else if self.shape_or_type_requires_declared_index_signature(source_shape, source) {
                return Some(SubtypeFailureReason::MissingIndexSignature {
                    index_kind: "number",
                });
            }
        }

        // Check symbol index signature
        if let Some(t_symbol_idx) = target_shape.symbol_index_signature() {
            match source_shape.symbol_index_signature() {
                Some(s_symbol_idx)
                    if self.index_signature_key_covers(
                        s_symbol_idx.key_type,
                        t_symbol_idx.key_type,
                    ) =>
                {
                    if !self
                        .check_subtype(s_symbol_idx.value_type, t_symbol_idx.value_type)
                        .is_true()
                    {
                        return self.make_index_sig_reason(
                            "symbol",
                            s_symbol_idx.value_type,
                            t_symbol_idx.value_type,
                            None,
                        );
                    }
                }
                _ => {
                    let optional_index_satisfied_by_empty_source = target_shape
                        .symbol_index_is_optional()
                        && source_shape.properties.is_empty();
                    if !optional_index_satisfied_by_empty_source
                        && self.requires_explicit_declared_index_signature_for(
                            source_shape,
                            Some(source),
                        )
                    {
                        return Some(SubtypeFailureReason::MissingIndexSignature {
                            index_kind: "symbol",
                        });
                    }
                }
            }
        }

        if let Some(reason) =
            self.explain_properties_against_index_signatures(&source_shape.properties, target_shape)
        {
            return Some(reason);
        }

        None
    }

    fn explain_object_with_index_to_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: ObjectShapeId,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        // The source's index signatures never participate in satisfying the
        // target's *named* members (`tsc`'s `getPropertyOfType` does not
        // synthesize a member from an index signature), and a plain object
        // target declares no index signature for the source index to relate
        // against. Every failure on this path is therefore a named-member
        // failure — explained exactly as for a plain object source, which also
        // yields the correct `TS2739`/`TS2741`/property-mismatch selection.
        self.explain_object_failure(
            source,
            target,
            &source_shape.properties,
            Some(source_shape_id),
            target_props,
        )
    }
}
