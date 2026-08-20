//! Render `SubtypeFailureReason` values into diagnostics.
//! Split from `assignability.rs` for maintainability.
use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::query_boundaries::type_checking_utilities as query_utils;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

use super::assignability::{
    is_builtin_wrapper_name, is_object_prototype_method,
    is_object_prototype_method_for_array_target, is_primitive_type_name,
};
mod constraint_walk_display;
mod nested_application_property_mismatch;
#[path = "render_failure_index_access.rs"]
mod render_failure_index_access;
#[path = "render_failure_missing_property.rs"]
mod render_failure_missing_property;
#[path = "render_failure_missing_property_base_class.rs"]
mod render_failure_missing_property_base_class;
#[path = "render_failure_property_helpers.rs"]
mod render_failure_property_helpers;
mod type_mismatch;
mod union_source_mismatch;
mod union_target_member_frame;

/// Depth at which the shared property-type-mismatch renderer
/// ([`CheckerState::render_property_type_mismatch`]) stops recursing into a
/// nested reason. A contravariant callback chain whose object property leaf
/// would render at or beyond this depth is left to the prior signature-only
/// rendering rather than emitted with its final relation line truncated.
const PROPERTY_MISMATCH_RENDER_DEPTH_CAP: u32 = 5;

/// Elaboration depth of the *first child* of a header rendered at chain depth
/// `depth`. A top-level mismatch (`depth == 0`) is the diagnostic message
/// header itself, so its first child stays at elaboration depth `0`
/// (indent `2`); a nested header at `depth > 0` is already an elaboration line,
/// so its first child sits one level deeper at `depth + 1`. Every nested
/// renderer that hangs a note, leaf, frame, or constraint-walk step beneath a
/// header shares this rule — funnel it through here rather than re-deriving the
/// `depth == 0` special case (getting it wrong over-indents the whole subtree by
/// one level; see #17797).
pub(in crate::error_reporter) const fn first_child_depth(depth: u32) -> u32 {
    if depth == 0 { 0 } else { depth + 1 }
}

/// Parameters shared across all `render_*` dispatch helpers.
pub(in crate::error_reporter) struct RenderContext {
    pub source: TypeId,
    pub target: TypeId,
    pub idx: NodeIndex,
    pub depth: u32,
    pub start: u32,
    pub length: u32,
    pub file_name: String,
    /// Pre-rendered source display supplied by the caller when its context
    /// applies display policy the renderer cannot recompute (e.g. the
    /// argument path's fresh-object-literal widening, whose resolved-env
    /// widening pass is unavailable once rendering has borrowed the env).
    pub source_display_override: Option<String>,
}

impl<'a> CheckerState<'a> {
    /// Apply tsc's `reportRelationError` literal-source generalization to a
    /// nested relation line's source type: a literal source is displayed as
    /// its base type (`"no"` -> `string`, `true` -> `boolean`, `E.X` -> `E`)
    /// when the target could not hold a top-level singleton type, and
    /// preserved when the literal-vs-literal comparison stays meaningful
    /// (`"no"` vs `"yes"`, literal-union targets, singleton-constrained type
    /// parameters).
    ///
    /// tsc applies this in every relation-error report. This entry serves the
    /// nested chain leaves — property / array-element / tuple-position /
    /// return frames — the TS2769 overload elaboration
    /// (`overload_failure_generalized_pending`), and the top-level
    /// union-of-literals surface in `render_type_mismatch` (#15626, #15628).
    ///
    /// An all-unit union source maps member-wise through the same bases
    /// (tsc `getBaseTypeOfLiteralTypeUnion`): `"x" | "y"` -> `string`,
    /// `true | 1` -> `number | boolean` (#15628).
    pub(in crate::error_reporter) fn generalize_nested_relation_source_for_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> TypeId {
        // Source side first: it is a cheap shape check for the overwhelmingly
        // common non-literal sources, while the target gate may need
        // resolver-backed constraint evaluation for deferred targets.
        let generalized = self.relation_literal_source_base_for_display(source);
        if generalized == source {
            return source;
        }
        // tsc preserves a literal / literal-union source verbatim against a
        // `never` target: the exhaustiveness and narrowed-to-`never` surface
        // reports the residual literal union (`Type '"d" | "c"' is not
        // assignable to type 'never'`), not its widened base (`string`). `never`
        // is not singleton-capable, so the general gate below would otherwise
        // widen it. This is the one non-singleton-capable target where tsc keeps
        // the literal source, so it is handled explicitly rather than through
        // `relation_target_could_hold_singleton`.
        if target == TypeId::NEVER {
            return source;
        }
        if crate::query_boundaries::diagnostics::relation_target_could_hold_singleton(
            self.ctx.types,
            &self.ctx,
            target,
        ) {
            return source;
        }
        generalized
    }

    /// tsc's `getBaseTypeOfLiteralType` over the checker environment: a scalar
    /// literal widens to its primitive base, an enum member (including a
    /// still-deferred `Lazy` member ref) to its parent enum, and an all-unit
    /// union maps member-wise (`getBaseTypeOfLiteralTypeUnion`). Every other
    /// type is returned unchanged. The target gate is the caller's job.
    fn relation_literal_source_base_for_display(&mut self, source: TypeId) -> TypeId {
        let generalized = crate::query_boundaries::diagnostics::literal_base_type_for_display(
            self.ctx.types,
            source,
        );
        if generalized != source {
            return generalized;
        }
        // Enum members widen to their parent enum (tsc's
        // `getBaseTypeOfLiteralType` EnumLike branch). The parent lookup needs
        // the checker's enum environment, so it sits outside the pure query;
        // the widen is a no-op for non-members.
        let enum_widened = self.widen_enum_member_type(source);
        if enum_widened != source {
            return enum_widened;
        }
        if crate::query_boundaries::common::is_unit_type(self.ctx.types, source)
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, source)
        {
            let members: Vec<TypeId> = members.iter().copied().collect();
            let mapped: Vec<TypeId> = members
                .iter()
                .map(|&member| self.relation_literal_source_base_for_display(member))
                .collect();
            if mapped != members {
                return crate::query_boundaries::flow_analysis::union_types(self.ctx.types, mapped);
            }
        }
        source
    }

    /// Resolve the parameter name at `param_index` in the first call
    /// signature of `callable_ty` (if any). Used to render TS2328
    /// "Types of parameters '_' and '_' are incompatible." messages.
    fn callable_param_name_at(&self, callable_ty: TypeId, param_index: usize) -> Option<String> {
        let shape = crate::query_boundaries::common::get_callable_shape_for_type(
            self.ctx.types,
            callable_ty,
        )?;
        let atom = shape
            .call_signatures
            .first()
            .and_then(|sig| sig.params.get(param_index).and_then(|p| p.name))?;
        Some(self.ctx.types.resolve_atom(atom))
    }

    fn callable_type_after_display_evaluation(&mut self, ty: TypeId) -> Option<TypeId> {
        if crate::query_boundaries::common::is_callable_type(self.ctx.types, ty) {
            return Some(ty);
        }
        let evaluated = self.evaluate_type_with_resolution(ty);
        if evaluated != TypeId::ERROR
            && crate::query_boundaries::common::is_callable_type(self.ctx.types, evaluated)
        {
            return Some(evaluated);
        }
        let evaluated = self.evaluate_type_for_assignability(ty);
        if evaluated != TypeId::ERROR
            && crate::query_boundaries::common::is_callable_type(self.ctx.types, evaluated)
        {
            return Some(evaluated);
        }
        let evaluated = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
        (evaluated != TypeId::ERROR
            && crate::query_boundaries::common::is_callable_type(self.ctx.types, evaluated))
        .then_some(evaluated)
    }

    fn strict_callback_param_display_type(&mut self, ty: TypeId) -> TypeId {
        self.callable_type_after_display_evaluation(ty)
            .unwrap_or(ty)
    }

    fn strict_callback_outer_display_type(
        &mut self,
        ty: TypeId,
        param_index: usize,
    ) -> Option<TypeId> {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
            && param_index < shape.params.len()
        {
            let mut shape = (*shape).clone();
            shape.params[param_index].type_id =
                self.strict_callback_param_display_type(shape.params[param_index].type_id);
            return Some(diagnostic_query::function_type_from_shape(
                self.ctx.types,
                shape,
            ));
        }

        let shape = crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)?;
        if !shape.construct_signatures.is_empty()
            || shape.call_signatures.len() != 1
            || param_index >= shape.call_signatures[0].params.len()
        {
            return None;
        }
        let mut sig = shape.call_signatures[0].clone();
        sig.params[param_index].type_id =
            self.strict_callback_param_display_type(sig.params[param_index].type_id);
        Some(diagnostic_query::function_type_from_call_signature(
            self.ctx.types,
            &sig,
            false,
        ))
    }

    fn strict_callback_assignment_display_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
        param_index: usize,
    ) -> Option<(String, String)> {
        let source_display = self.strict_callback_outer_display_type(source, param_index)?;
        let target_display = self.strict_callback_outer_display_type(target, param_index)?;
        Some((
            self.format_assignability_type_for_message(source_display, target_display),
            self.format_assignability_type_for_message(target_display, source_display),
        ))
    }

    /// Emit a single `Types of parameters 'a' and 'b' are incompatible.` frame
    /// for the parameter at `param_index` of the `source_fn`/`target_fn` pair at
    /// elaboration `depth`. The argument order mirrors tsc's
    /// `compareSignaturesRelated`, which reports the *source* signature's
    /// parameter name first and the *target* signature's second.
    fn push_types_of_parameters_frame(
        &mut self,
        diag: &mut Diagnostic,
        source_fn: TypeId,
        target_fn: TypeId,
        param_index: usize,
        depth: u32,
    ) {
        let source_name = self
            .callable_param_name_at(source_fn, param_index)
            .unwrap_or_else(|| format!("arg{param_index}"));
        let target_name = self
            .callable_param_name_at(target_fn, param_index)
            .unwrap_or_else(|| format!("arg{param_index}"));
        let frame = format_message(
            diagnostic_messages::TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE,
            &[&source_name, &target_name],
        );
        diag.push_elaboration(
            frame,
            diagnostic_codes::TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE,
            depth,
        );
    }

    /// Emit a `Type 'S' is not assignable to type 'T'.` callback signature
    /// relation line for the `source_fn`/`target_fn` pair at elaboration
    /// `depth`. tsc re-prints this line for the current callback signature at
    /// every second contravariance flip while drilling into nested callback
    /// parameters.
    fn push_callback_signature_relation_line(
        &mut self,
        diag: &mut Diagnostic,
        source_fn: TypeId,
        target_fn: TypeId,
        depth: u32,
    ) {
        let message = self.element_mismatch_message(source_fn, target_fn);
        diag.push_elaboration(
            message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            depth,
        );
    }

    /// Append tsc's signature-mismatch elaboration beneath a function-to-function
    /// relation line: a `Types of parameters 'a' and 'b' are incompatible.` frame
    /// followed by the contravariant leaf relation for the offending parameter.
    ///
    /// Only runs when both enclosing types are function/callable, so the parent
    /// line is genuinely the signature-to-signature line rather than a parameter
    /// leaf rendered directly. Parameters are contravariant, so the leaf is
    /// `Type '<target param>' is not assignable to type '<source param>'.`; the
    /// solver's `inner_reason` already carries that orientation.
    ///
    /// When the offending parameter is itself a callback, the contravariant
    /// relation between the two callbacks is again a signature comparison whose
    /// failure is another `ParameterTypeMismatch`. tsc keeps descending — one
    /// `Types of parameters` frame per callback nesting level, flipping the
    /// source/target orientation at each flip — and re-prints the current
    /// callback signature relation line before every second frame. This walks
    /// the solver's nested `inner_reason` chain and reproduces that layout
    /// exactly. The chain must bottom out in a non-callable parameter relation
    /// whose leaf layout is reproduced exactly (scalar, missing-property, or
    /// object property-type mismatch); for anything else (e.g. the callback
    /// differs on its return type) nothing is appended so the previous
    /// signature-line-only rendering is preserved.
    fn push_parameter_mismatch_elaboration(
        &mut self,
        diag: &mut Diagnostic,
        rctx: &RenderContext,
        param_index: usize,
        source_param: TypeId,
        target_param: TypeId,
        inner_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) {
        use crate::query_boundaries::common::SubtypeFailureReason;

        let (source, target, idx, depth) = (rctx.source, rctx.target, rctx.idx, rctx.depth);
        if self
            .callable_type_after_display_evaluation(source)
            .is_none()
            || self
                .callable_type_after_display_evaluation(target)
                .is_none()
        {
            return;
        }

        // Phase 1: walk the contravariant chain, collecting one
        // `(source_fn, target_fn, param_index)` frame per callback nesting level
        // and the terminating scalar leaf. Validate the whole chain before
        // emitting anything so a non-callback / non-parameter failure falls back
        // to the previous (signature-line-only) rendering instead of leaving a
        // dangling frame.
        let mut frames: Vec<(TypeId, TypeId, usize)> = vec![(source, target, param_index)];
        let mut src_param = source_param;
        let mut tgt_param = target_param;
        let mut inner = inner_reason;
        // Leaf relation `tgt_param <: src_param` (parameters are contravariant).
        let leaf_src;
        let leaf_tgt;
        let leaf_reason;
        loop {
            let src_callable = self
                .callable_type_after_display_evaluation(src_param)
                .is_some();
            let tgt_callable = self
                .callable_type_after_display_evaluation(tgt_param)
                .is_some();
            if !src_callable && !tgt_callable {
                // The chain bottoms out in a non-callable parameter relation.
                // Only emit when the leaf reason is one whose tsc layout is
                // reproduced exactly here (a scalar/missing-property self-heading
                // leaf, or an object property-type mismatch that takes an
                // explicit header); otherwise preserve the prior no-op so an
                // unverified leaf shape is never rendered almost-right.
                if !Self::contravariant_param_leaf_supported(inner) {
                    return;
                }
                leaf_src = tgt_param;
                leaf_tgt = src_param;
                leaf_reason = inner;
                break;
            }
            if !(src_callable && tgt_callable) {
                // Mixed callable/non-callable parameter: not a chain tsc renders
                // with the nested-callback layout. Preserve prior behavior.
                return;
            }
            // Descend into the next callback signature pair. The relation
            // between the two callbacks is `tgt_param <: src_param`, so the next
            // pair's source is `tgt_param` and its target is `src_param`; its
            // failure must be the carried `ParameterTypeMismatch`.
            let Some(SubtypeFailureReason::ParameterTypeMismatch {
                param_index: next_index,
                source_param: next_source_param,
                target_param: next_target_param,
                inner_reason: next_inner,
            }) = inner
            else {
                return;
            };
            frames.push((tgt_param, src_param, *next_index));
            src_param = *next_source_param;
            tgt_param = *next_target_param;
            inner = next_inner.as_deref();
            // Function nesting is finite, but guard against pathological inputs.
            if frames.len() > 16 {
                return;
            }
        }

        // Phase 2: emit. At depth 0 the parent line is the primary diagnostic
        // (indent level 0), so its first elaboration sits at field 0. At depth
        // > 0 the parent line is itself a related entry at field `depth`, so its
        // elaboration sits one level deeper.
        let frame_depth = first_child_depth(depth);

        // An object property-type leaf drills through the shared property
        // renderer, which caps its own recursion at depth 5. Compute the leaf's
        // emission depth and bail (preserving the prior no-op) when that drill
        // would be truncated, so a deep object leaf is never rendered with its
        // final relation line dropped. Each frame advances one level, plus one
        // extra level for every signature reprint (before frames k = 2, 4, …),
        // i.e. `(frames - 1) / 2` reprints.
        if Self::contravariant_param_leaf_needs_header(leaf_reason) {
            let reprints = (frames.len().saturating_sub(1) / 2) as u32;
            let leaf_depth = frame_depth + frames.len() as u32 + reprints;
            // The leaf reason renders one level beneath the object header.
            if leaf_depth + 1 >= PROPERTY_MISMATCH_RENDER_DEPTH_CAP {
                return;
            }
        }

        let mut next_depth = frame_depth;
        for (k, &(source_fn, target_fn, frame_param_index)) in frames.iter().enumerate() {
            // tsc re-prints the current callback signature relation line before
            // every second frame (k = 2, 4, …); the outermost frame (k = 0) sits
            // directly beneath the already-emitted parent signature line.
            if k > 0 && k.is_multiple_of(2) {
                self.push_callback_signature_relation_line(diag, source_fn, target_fn, next_depth);
                next_depth += 1;
            }
            self.push_types_of_parameters_frame(
                diag,
                source_fn,
                target_fn,
                frame_param_index,
                next_depth,
            );
            next_depth += 1;
        }

        // Parameters are contravariant, so the leaf compares the (innermost)
        // target parameter against the source parameter.
        if Self::contravariant_param_leaf_needs_header(leaf_reason) {
            // A header-led structural mismatch (object property-type, tuple
            // element/arity, or index-signature) leads with its specialized
            // line at depth >= 1, so emit the explicit `Type 'S' is not
            // assignable to type 'T'.` header over the parameter pair first and
            // render the structured reason one level deeper — exactly like the
            // union-source renderer drills its member. The drill takes the
            // parameter (parent) types so a tuple/index reason renders against
            // the whole parameter, not its element/value type.
            self.push_callback_signature_relation_line(diag, leaf_src, leaf_tgt, next_depth);
            if let Some(leaf) = leaf_reason {
                let leaf_diag =
                    self.render_failure_reason(leaf, leaf_src, leaf_tgt, idx, next_depth + 1);
                Self::push_nested_chain(diag, leaf_diag, next_depth + 1);
            }
        } else {
            // `push_property_chain_leaf` renders the structured leaf reason when
            // present (keeping intrinsic/literal display accurate) and otherwise
            // emits the plain `Type 'S' is not assignable to type 'T'.` line.
            // Scalar and missing-property leaves self-head through this path.
            self.push_property_chain_leaf(diag, leaf_reason, leaf_src, leaf_tgt, idx, next_depth);
        }
    }

    /// Whether the contravariant callback-parameter chain's terminating leaf
    /// reason has a tsc layout reproduced exactly by
    /// [`Self::push_parameter_mismatch_elaboration`]. Plain scalar/literal leaves
    /// and the `MissingProperty`/`MissingProperties` summaries self-head through
    /// [`Self::push_property_chain_leaf`]; an object `PropertyTypeMismatch` is
    /// rendered with an explicit object header. Any other leaf shape is left to
    /// the prior signature-line-only rendering.
    const fn contravariant_param_leaf_supported(
        leaf_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> bool {
        use crate::query_boundaries::common::SubtypeFailureReason;
        match leaf_reason {
            None => true,
            // The leaf relation `target_param <: source_param` is an ordinary
            // assignability failure, so its reason can take any of the shapes a
            // union member's failure can — the contravariant frame plays the
            // same role as the `Type 'M' is not assignable to type 'T'.` union
            // line. Accept the same member-failure set the union-source renderer
            // composes exactly (plus the union reasons themselves, which arise
            // when a parameter is a union): self-heading leaves
            // (scalar/literal/error, missing-property summaries, array-element,
            // readonly-to-mutable, and nested union failures) render their own
            // member line, and the header-led structural reasons
            // (tuple/property/index) get an explicit `Type 'S' is not assignable
            // to type 'T'.` header from `contravariant_param_leaf_needs_header`.
            // `ReturnTypeMismatch`/`ParameterTypeMismatch` never bottom out here
            // (the descent loop above keeps walking while both params are
            // callable), so they are intentionally absent.
            Some(reason) => matches!(
                reason,
                SubtypeFailureReason::TypeMismatch { .. }
                    | SubtypeFailureReason::IntrinsicTypeMismatch { .. }
                    | SubtypeFailureReason::LiteralTypeMismatch { .. }
                    | SubtypeFailureReason::ErrorType { .. }
                    | SubtypeFailureReason::MissingProperty { .. }
                    | SubtypeFailureReason::MissingProperties { .. }
                    | SubtypeFailureReason::PropertyTypeMismatch { .. }
                    | SubtypeFailureReason::ArrayElementMismatch { .. }
                    | SubtypeFailureReason::ReadonlyToMutableAssignment { .. }
                    | SubtypeFailureReason::UnionSourceMismatch { .. }
                    | SubtypeFailureReason::NoUnionMemberMatches { .. }
                    | SubtypeFailureReason::TupleElementTypeMismatch { .. }
                    | SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
                    | SubtypeFailureReason::TupleElementMismatch { .. }
                    | SubtypeFailureReason::TupleArityMismatch(_)
                    | SubtypeFailureReason::SourceProvidesNoMatch { .. }
                    | SubtypeFailureReason::IndexSignatureMismatch { .. }
            ),
        }
    }

    /// Whether the terminating leaf reason needs an explicit
    /// `Type 'S' is not assignable to type 'T'.` header emitted before its drill.
    /// The header-led structural reasons (object property-type, tuple
    /// element/arity, and index-signature mismatches) lead with a specialized
    /// line (`Types of property 'p' …`, `Type at position N …`, `'string' index
    /// signatures are incompatible.`) at depth >= 1, so the header is supplied
    /// here; scalar, missing-property, array-element, and union leaves self-head.
    /// This is the same split the union-source member renderer applies, so the
    /// contravariant parameter chain reproduces the identical nested layout.
    const fn contravariant_param_leaf_needs_header(
        leaf_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> bool {
        match leaf_reason {
            Some(reason) => Self::union_member_nested_needs_header(reason),
            None => false,
        }
    }

    fn no_union_member_matches_switch_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        let expected_len = crate::query_boundaries::common::union_members(self.ctx.types, source)
            .map(|members| members.len())?;
        if expected_len < 2 {
            return None;
        }

        let mut current = anchor_idx;
        let clause_idx = loop {
            let parent = self.ctx.arena.parent_of(current)?;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::CASE_CLAUSE {
                break parent;
            }
            current = parent;
        };

        let case_block_idx = self.ctx.arena.parent_of(clause_idx)?;
        let case_block_node = self.ctx.arena.get(case_block_idx)?;
        let case_block = self.ctx.arena.get_block(case_block_node)?;
        let clause_pos = case_block
            .statements
            .nodes
            .iter()
            .position(|&idx| idx == clause_idx)?;

        let mut start = clause_pos;
        while start > 0 {
            let prev_idx = case_block.statements.nodes[start - 1];
            let Some(prev_node) = self.ctx.arena.get(prev_idx) else {
                break;
            };
            let Some(prev_clause) = self.ctx.arena.get_case_clause(prev_node) else {
                break;
            };
            if !prev_clause.statements.nodes.is_empty() {
                break;
            }
            start -= 1;
        }

        let mut entries: Vec<(TypeId, String)> = Vec::new();
        for &idx in &case_block.statements.nodes[start..=clause_pos] {
            let clause_node = self.ctx.arena.get(idx)?;
            let clause = self.ctx.arena.get_case_clause(clause_node)?;
            if clause.expression.is_none() {
                return None;
            }
            let case_type = self.literal_type_from_initializer(clause.expression)?;
            let display = self
                .literal_expression_display(clause.expression)
                .unwrap_or_else(|| self.format_assignability_type_for_message(case_type, target));
            entries.push((case_type, display));
        }

        if entries.len() != expected_len {
            return None;
        }

        // Order the reconstructed case members through the shared union
        // comparator so the source-union display matches tsc's sorted literal
        // union (oracle unknownType2: `"maybe" | "no" | "yes"`) rather than the
        // clause order.
        let ordered = {
            let mut formatter = self.ctx.create_diagnostic_type_formatter();
            formatter.order_union_members_for_display(entries.iter().map(|(ty, _)| *ty).collect())
        };
        let mut displays = Vec::with_capacity(ordered.len());
        for ty in ordered {
            if let Some(pos) = entries.iter().position(|(entry_ty, _)| *entry_ty == ty) {
                displays.push(entries.remove(pos).1);
            }
        }
        Some(displays.join(" | "))
    }

    fn format_tuple_shape_for_readonly_to_mutable(&mut self, type_id: TypeId) -> Option<String> {
        let elements = crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)?;
        let mut formatted = Vec::with_capacity(elements.len());
        for element in elements {
            let rest = if element.rest { "..." } else { "" };
            let optional = if element.optional && !element.rest {
                "?"
            } else {
                ""
            };
            let type_str = self.format_type_diagnostic(element.type_id);
            if let Some(name_atom) = element.name {
                let name = self.ctx.types.resolve_atom_ref(name_atom);
                formatted.push(format!("{rest}{name}{optional}: {type_str}"));
            } else {
                formatted.push(format!("{rest}{type_str}{optional}"));
            }
        }
        Some(format!("[{}]", formatted.join(", ")))
    }

    fn class_own_missing_properties_for_display(
        &self,
        source_candidates: &[TypeId],
        target_candidates: &[TypeId],
        missing_property_name: tsz_common::interner::Atom,
        fallback_target_type: TypeId,
    ) -> Option<(
        tsz_binder::SymbolId,
        TypeId,
        Vec<tsz_common::interner::Atom>,
    )> {
        let target_symbol = target_candidates
            .iter()
            .find_map(|&candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
                    .and_then(|shape| {
                        shape.properties.iter().find_map(|prop| {
                            (prop.name == missing_property_name)
                                .then_some(prop.parent_id)
                                .flatten()
                                .filter(|sym| {
                                    self.ctx.binder.get_symbol(*sym).is_some_and(|symbol| {
                                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                                    })
                                })
                        })
                    })
            })
            .or_else(|| {
                target_candidates.iter().find_map(|&candidate| {
                    crate::query_boundaries::diagnostics::get_object_symbol(
                        self.ctx.types,
                        candidate,
                    )
                    .or_else(|| {
                        crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            candidate,
                        )
                        .and_then(|shape| {
                            shape.properties.iter().find_map(|prop| {
                                prop.parent_id.filter(|sym| {
                                    self.ctx.binder.get_symbol(*sym).is_some_and(|symbol| {
                                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                                    })
                                })
                            })
                        })
                    })
                })
            })?;

        let mut source_props = Vec::new();
        for &candidate in source_candidates {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            {
                for prop in &shape.properties {
                    if !source_props.contains(&prop.name) {
                        source_props.push(prop.name);
                    }
                }
            }
        }

        let mut class_own_missing = Vec::new();
        let mut target_display_type = None;
        for &candidate in target_candidates {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            {
                let mut saw_own = false;
                for prop in &shape.properties {
                    if prop.parent_id == Some(target_symbol) {
                        saw_own = true;
                        let name = self.ctx.types.resolve_atom_ref(prop.name);
                        if !tsz_solver::utils::is_synthetic_private_brand_name(&name)
                            && !is_object_prototype_method(&name)
                            && !source_props.contains(&prop.name)
                            && !class_own_missing.contains(&prop.name)
                        {
                            class_own_missing.push(prop.name);
                        }
                    }
                }
                if saw_own && target_display_type.is_none() {
                    target_display_type = Some(candidate);
                }
            }
        }

        (class_own_missing.len() > 1).then(|| {
            (
                target_symbol,
                target_display_type.unwrap_or(fallback_target_type),
                class_own_missing,
            )
        })
    }

    /// When `target` is a union whose only non-nullish member is a single type
    /// (`T | null`, `T | undefined`, `T | null | undefined`), return that member.
    /// Used to display the missing-property target as `T` rather than the whole
    /// nullable union, matching tsc (which elaborates the non-nullish source
    /// against `T` alone). The nullish predicate (`null`/`undefined`) matches the
    /// solver's promotion in `explain.rs`.
    fn single_non_nullish_union_member(&self, target: TypeId) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, target)?;
        let mut non_nullish = members
            .iter()
            .copied()
            .filter(|&m| m != TypeId::NULL && m != TypeId::UNDEFINED);
        let first = non_nullish.next()?;
        non_nullish.next().is_none().then_some(first)
    }

    /// Recursively render a `SubtypeFailureReason` into a Diagnostic.
    pub(crate) fn render_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> Diagnostic {
        self.render_failure_reason_with_source_display(reason, source, target, idx, depth, None)
    }

    pub(crate) fn render_failure_reason_with_source_display(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        source_display_override: Option<String>,
    ) -> Diagnostic {
        use crate::query_boundaries::common::SubtypeFailureReason;

        // Discarded-diagnostics children (transient cross-arena delegation
        // subtrees) never surface their diagnostics: keep the code and span
        // (so child-internal counting/dedup predicates behave the same) but
        // skip the expensive presentation work — diagnostic type formatting,
        // nested-reason elaboration, and related-info chains. The placeholder
        // message embeds the type ids so distinct failures keep distinct
        // message-hash dedup keys, mirroring distinct rendered messages.
        if self.ctx.diagnostics_discarded {
            let (start, length) = self
                .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact)
                .map(|anchor| (anchor.start, anchor.length))
                .unwrap_or_else(|| {
                    let (pos, end) = self.get_node_span(idx).unwrap_or((0, 0));
                    self.normalized_anchor_span(idx, pos, end.saturating_sub(pos))
                });
            return Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                length,
                format!(
                    "[discarded diagnostic: type '#{}' is not assignable to type '#{}']",
                    source.0, target.0
                ),
                reason.diagnostic_code(),
            );
        }

        // Fail-safe work-budget scope: the recursive failure-reason tree of
        // one diagnostic shares a single budget (issue #13040).
        let _budget_scope = crate::error_reporter::display_budget::DisplayBudgetScope::enter();
        let source = self.recover_unknown_array_source_type_for_display(source, idx, depth);
        let (start, length) = self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact)
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                // get_node_span returns (pos, end); convert to (start, length)
                // and apply the same span normalization as the primary path.
                let (pos, end) = self.get_node_span(idx).unwrap_or((0, 0));
                self.normalized_anchor_span(idx, pos, end.saturating_sub(pos))
            });
        let file_name = self.ctx.file_name.clone();
        // A reduced alias/application body can fail through a property reason
        // even when the two value-level types have the same declared surface.
        // Distinct free declaration origins prove the semantic unrelatedness;
        // the final display equality only selects tsc's TS2719 presentation.
        if depth == 0
            && source_display_override.is_none()
            && crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                source,
            )
            && crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
            && crate::query_boundaries::assignability::
                have_same_surface_distinct_decl_scoped_free_type_parameters(
                    self.ctx.types,
                    &self.ctx,
                    source,
                    target,
                )
        {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            if source_str == target_str {
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                    &[&source_str, &target_str],
                );
                let mut diagnostic = Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                );
                if let Some(related) = self.unrelated_type_parameter_target_related_info(
                    source,
                    target,
                    &source_str,
                    &target_str,
                    start,
                    length,
                    0,
                ) {
                    diagnostic.related_information.push(related);
                }
                return diagnostic;
            }
        }
        // TS2696: property-only failures from the `Object` wrapper use the
        // specialized message unless the target is callable/constructable.
        if depth == 0 {
            let is_property_failure = matches!(
                reason,
                SubtypeFailureReason::MissingProperty { .. }
                    | SubtypeFailureReason::MissingProperties { .. }
                    | SubtypeFailureReason::PropertyTypeMismatch { .. }
                    | SubtypeFailureReason::OptionalPropertyRequired { .. }
                    | SubtypeFailureReason::NoCommonProperties { .. }
            );
            if is_property_failure
                && crate::query_boundaries::diagnostics::is_global_object_interface_for_diagnostic(
                    self.ctx.types,
                    source,
                )
            {
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    diagnostic_messages::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN
                        .to_string(),
                    diagnostic_codes::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN,
                );
            }
        }
        // For a nullable-object target (`T | null`, `T | undefined`,
        // `T | null | undefined`) the solver promotes a missing-property
        // failure to a top-level `MissingProperty`/`MissingProperties` reason
        // (matching tsc — the non-nullish source is elaborated against `T`
        // alone). tsc displays that single non-nullish member, not the union,
        // so rebind the render target to it for these property-miss arms.
        let property_miss_target = if matches!(
            reason,
            SubtypeFailureReason::MissingProperty { .. }
                | SubtypeFailureReason::MissingProperties { .. }
        ) && let Some(member) =
            self.single_non_nullish_union_member(target)
        {
            // The rebind splits on the SOURCE kind, mirroring tsc's two report
            // paths for a missing property against a nullable union:
            // * An object-like source fails through `elaborateError`, which
            //   reports the inner failure against the sole non-nullish member
            //   directly — the alias surface never survives (`x: MaybeRec`
            //   where `type MaybeRec = Rec0 | null` shows `Rec0`), so the
            //   member always replaces the union.
            // * A primitive source fails the whole relation and
            //   `reportErrorResults` restores the original target whenever it
            //   carried an `aliasSymbol`, so an alias-named union renders
            //   whole (`x: MaybeBox = 5` shows `MaybeBox`). The annotation AST
            //   is authoritative when present — a structurally identical
            //   anonymous annotation interns to the same `TypeId` as the alias
            //   body, so only the syntax can tell the two references apart.
            let restores_alias =
                crate::query_boundaries::common::is_primitive_type(self.ctx.types, source)
                    && self
                        .assignment_target_annotation_alias_reference_verdict(idx)
                        .unwrap_or_else(|| {
                            crate::query_boundaries::diagnostics::type_keeps_alias_symbol_surface(
                                self.ctx.types.as_type_database(),
                                &self.ctx.definition_store,
                                target,
                            )
                        });
            if restores_alias { target } else { member }
        } else {
            target
        };
        let rctx = RenderContext {
            source,
            target: property_miss_target,
            idx,
            depth,
            start,
            length,
            file_name: file_name.clone(),
            source_display_override,
        };
        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => self.render_missing_property(&rctx, *property_name, *source_type, *target_type),
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => self.render_missing_properties(&rctx, property_names, *source_type, *target_type),
            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => self.render_property_type_mismatch(
                reason,
                &rctx,
                *property_name,
                *source_property_type,
                *target_property_type,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::OptionalPropertyRequired { property_name } => {
                self.render_optional_property_required(&rctx, *property_name)
            }
            SubtypeFailureReason::ReadonlyPropertyMismatch { property_name } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let message = format_message(
                    diagnostic_messages::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY,
                    &[&prop_name],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }
            SubtypeFailureReason::PropertyVisibilityMismatch {
                property_name,
                source_visibility,
                target_visibility,
            } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types_at(source, target, idx);
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let base = self.property_visibility_assignability_message(
                    &source_str,
                    &target_str,
                    &prop_name,
                    *source_visibility,
                    *target_visibility,
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }
            SubtypeFailureReason::PropertyNominalMismatch { property_name } => {
                self.render_property_nominal_mismatch(reason, &rctx, *property_name)
            }
            SubtypeFailureReason::PrivateIdentifierMemberMismatch { property_name } => {
                self.render_private_identifier_member_mismatch(reason, &rctx, *property_name)
            }
            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type: _,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                // When the source is a union (e.g. a fresh literal flowed
                // through `?:`, `??`, or `||` and yielded a union of fresh
                // members), tsc reports the assignment as TS2322 with the
                // excess-property message attached as related-information
                // elaboration, not as a standalone TS2353 anchored at the
                // property. The structural rule: a fresh literal in a
                // composite source produces an excess-property elaboration
                // on the outer assignment, not a property-anchored emit.
                if crate::query_boundaries::common::union_members(self.ctx.types, source).is_some()
                {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let main_message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let (elab_code, elab_message) =
                        self.excess_property_diagnostic_message(&prop_name, target, idx);
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        main_message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    diag.push_elaboration(elab_message, elab_code, 0);
                    return diag;
                }
                let (code, message) =
                    self.excess_property_diagnostic_message(&prop_name, target, idx);
                let (excess_start, excess_length) = self
                    .find_excess_property_anchor(idx, *property_name)
                    .unwrap_or((start, length));
                Diagnostic::error(file_name, excess_start, excess_length, message, code)
            }
            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => self.render_return_type_mismatch(
                reason,
                &rctx,
                *source_return,
                *target_return,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::TypePredicateMismatch {
                source_predicate,
                target_predicate,
                source_signature,
                nested_reason,
            } => self.render_type_predicate_mismatch(
                reason,
                &rctx,
                source_predicate.as_ref(),
                target_predicate,
                *source_signature,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::TypeArgumentMismatch {
                source_arg,
                target_arg,
                nested_reason,
            } => self.render_type_argument_mismatch(
                &rctx,
                *source_arg,
                *target_arg,
                nested_reason.as_ref(),
            ),
            SubtypeFailureReason::UnionSourceMismatch {
                source_type,
                target_type,
                member_type,
                nested_reason,
            } => self.render_union_source_mismatch(
                &rctx,
                *source_type,
                *target_type,
                *member_type,
                nested_reason.as_ref(),
            ),
            SubtypeFailureReason::UnionTargetMismatch {
                source_type,
                target_type,
                member_type,
                nested_reason,
            } => match nested_reason.as_ref() {
                // A missing required property folds directly beneath the union
                // head (`Property 'x' is missing …` already names the member),
                // exactly like tsc's flattened form.
                SubtypeFailureReason::MissingProperty { .. }
                | SubtypeFailureReason::MissingProperties { .. } => self
                    .render_parent_with_child_relation(
                        &rctx,
                        *source_type,
                        *target_type,
                        *source_type,
                        *member_type,
                        nested_reason.as_ref(),
                    ),
                // Any other member failure elaborates beneath the member frame
                // `Type 'S' is not assignable to type '<member>'.` — tsc's
                // `getBestMatchingType` re-runs the relation against the best
                // member with errors enabled.
                _ => self.render_union_target_member_frame_mismatch(
                    &rctx,
                    *source_type,
                    *target_type,
                    *member_type,
                    nested_reason.as_ref(),
                ),
            },
            SubtypeFailureReason::ConditionalBranchMismatch {
                source_type,
                target_type,
                branch_source,
                branch_target,
                nested_reason,
            } => self.render_parent_with_child_relation(
                &rctx,
                *source_type,
                *target_type,
                *branch_source,
                *branch_target,
                nested_reason.as_ref(),
            ),
            SubtypeFailureReason::IntersectionTargetMismatch {
                source_type,
                target_type,
                constituent_type,
                nested_reason,
                original_reason,
            } => self.render_intersection_target_mismatch(
                &rctx,
                *source_type,
                *target_type,
                *constituent_type,
                nested_reason,
                original_reason,
            ),
            SubtypeFailureReason::TypeParameterConstraintMismatch {
                source_type,
                target_type,
                constraint_type,
                nested_reason,
            } => {
                // The reason carries the *evaluated* target so the top line
                // matches tsc (e.g. the concrete result of an instantiated
                // conditional alias, not the unevaluated `Alias<Arg>` spelling).
                // The depth-0 outer line is built from `RenderContext::target`,
                // so rebind it to the evaluated target before delegating to the
                // shared parent-with-child renderer; the child relation is the
                // constraint vs the same evaluated target.
                let eval_rctx = RenderContext {
                    target: *target_type,
                    ..rctx
                };
                self.render_parent_with_child_relation(
                    &eval_rctx,
                    *source_type,
                    *target_type,
                    *constraint_type,
                    *target_type,
                    nested_reason.as_ref(),
                )
            }
            SubtypeFailureReason::TooManyParameters {
                source_count,
                target_count,
            } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types_at(source, target, idx);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                let mut diag = Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                let elaboration = format_message(
                    diagnostic_messages::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT,
                    &[&source_count.to_string(), &target_count.to_string()],
                );
                // The arity leaf sits one level under this reason's own
                // `Type 'S' is not assignable to type 'T'.` line. At the top
                // level that line is the diagnostic's primary message, so the
                // leaf is the first elaboration (depth 0). When this reason is
                // drilled beneath a property/member header (e.g. a method
                // property whose signature takes too few target arguments), the
                // `Type 'S' …` line is re-seated at `depth` by the parent chain,
                // so the leaf must follow at `depth + 1` — mirroring the
                // absolute-depth convention every other nested `render_*` arm
                // honors. Authoring it at a fixed `0` collapsed it up to the
                // property-header level (issue #16859).
                let leaf_depth = first_child_depth(depth);
                diag.push_elaboration(
                    elaboration,
                    diagnostic_codes::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT,
                    leaf_depth,
                );
                diag
            }
            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => {
                // The arity leaf is identical at every depth: direction picks the
                // catalog message — a source longer than a closed target ->
                // "target allows only M" (`TS2619`); shorter than required ->
                // "target requires M" (`TS2618`). Only its framing differs by
                // depth (top-level headline vs. drilled leaf), so compute it once.
                let (arity_message, arity_code) = if source_count > target_count {
                    (
                        diagnostic_messages::SOURCE_HAS_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY,
                        diagnostic_codes::SOURCE_HAS_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY,
                    )
                } else {
                    (
                        diagnostic_messages::SOURCE_HAS_ELEMENT_S_BUT_TARGET_REQUIRES,
                        diagnostic_codes::SOURCE_HAS_ELEMENT_S_BUT_TARGET_REQUIRES,
                    )
                };
                let arity_text = format_message(
                    arity_message,
                    &[&source_count.to_string(), &target_count.to_string()],
                );
                if depth == 0 {
                    // Top level: tsc keeps the `TS2322` headline and attaches the
                    // arity reason as a nested elaboration line, matching the
                    // sibling function-arity elaboration above.
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    diag.push_elaboration(arity_text, arity_code, 0);
                    diag
                } else {
                    // Nested (depth >= 1) render: a closed-tuple arity mismatch
                    // drilled beneath a member/property header (e.g. a failing
                    // union member or `Types of property 'p' …`). tsc emits the
                    // same `TS2618`/`TS2619` leaf here as at the top level.
                    Diagnostic::error(file_name, start, length, arity_text, arity_code)
                }
            }
            SubtypeFailureReason::TupleArityMismatch(arity) => {
                // tsc's arity gate (`TS2618`–`TS2621`) resolves the message
                // family, diagnostic code, and argument count up front in the
                // solver; the renderer just formats the recorded arguments into
                // the catalog template the solver selected.
                let arity_code = arity.diagnostic_code();
                let arity_args: Vec<String> =
                    arity.message_args().iter().map(|n| n.to_string()).collect();
                let arity_arg_refs: Vec<&str> = arity_args.iter().map(String::as_str).collect();
                let arity_text = format_message(arity.diagnostic_message(), &arity_arg_refs);
                if depth == 0 {
                    // Top level: keep the `TS2322` headline and attach the arity
                    // reason as a nested elaboration line, matching the sibling
                    // `TupleElementMismatch` rendering above.
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    diag.push_elaboration(arity_text, arity_code, 0);
                    diag
                } else {
                    Diagnostic::error(file_name, start, length, arity_text, arity_code)
                }
            }
            SubtypeFailureReason::SourceProvidesNoMatch { position, variadic } => {
                // An unbounded array source provides no value for a required
                // (`TS2623`) or variadic (`TS2624`) tuple slot whose target
                // carries a rest element. tsc keeps the `TS2322`/`TS2345`
                // headline and attaches the position line as the elaboration,
                // matching the sibling arity rendering above.
                let (no_match_message, no_match_code) = if *variadic {
                    (
                        diagnostic_messages::SOURCE_PROVIDES_NO_MATCH_FOR_VARIADIC_ELEMENT_AT_POSITION_IN_TARGET,
                        diagnostic_codes::SOURCE_PROVIDES_NO_MATCH_FOR_VARIADIC_ELEMENT_AT_POSITION_IN_TARGET,
                    )
                } else {
                    (
                        diagnostic_messages::SOURCE_PROVIDES_NO_MATCH_FOR_REQUIRED_ELEMENT_AT_POSITION_IN_TARGET,
                        diagnostic_codes::SOURCE_PROVIDES_NO_MATCH_FOR_REQUIRED_ELEMENT_AT_POSITION_IN_TARGET,
                    )
                };
                let no_match_text = format_message(no_match_message, &[&position.to_string()]);
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    diag.push_elaboration(no_match_text, no_match_code, 0);
                    diag
                } else {
                    Diagnostic::error(file_name, start, length, no_match_text, no_match_code)
                }
            }
            SubtypeFailureReason::TupleElementTypeMismatch {
                index,
                target_index,
                source_element,
                target_element,
                nested_reason,
                multi_element,
            } => self.render_tuple_element_type_mismatch(
                &rctx,
                *index,
                *target_index,
                *source_element,
                *target_element,
                nested_reason.as_deref(),
                *multi_element,
            ),

            SubtypeFailureReason::TupleVariadicPositionMismatch {
                source_start,
                source_end,
                target_position,
                source_element,
                target_element,
                nested_reason,
            } => {
                let (detail, detail_code) =
                    Self::variadic_positional_detail(*source_start, *source_end, *target_position);
                self.render_tuple_positional_chain(
                    &rctx,
                    detail,
                    detail_code,
                    *source_element,
                    *target_element,
                    nested_reason.as_deref(),
                )
            }

            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
                nested_reason,
            } => self.render_array_element_mismatch(
                &rctx,
                *source_element,
                *target_element,
                nested_reason.as_deref(),
            ),

            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind,
                source_value_type,
                target_value_type,
                nested_reason,
                property_name,
            } => self.render_index_signature_mismatch(
                &rctx,
                index_kind,
                *source_value_type,
                *target_value_type,
                nested_reason.as_deref(),
                *property_name,
            ),

            SubtypeFailureReason::MissingIndexSignature { index_kind } => {
                if depth == 0 {
                    let source_str = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
                    let target_str = self.format_assignability_type_for_message(target, source);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
                let source_str = self.format_type_diagnostic(source);
                let message = format_message(
                    diagnostic_messages::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                    &[index_kind, &source_str],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members: _,
            } => {
                let display_source = if depth == 0 { source } else { *source_type };
                let (mut source_str, mut target_str) = if depth == 0 {
                    let use_structural_source_display =
                        crate::query_boundaries::common::enum_def_id(
                            self.ctx.types,
                            display_source,
                        )
                        .is_none();
                    (
                        if use_structural_source_display {
                            self.format_type_for_diagnostic_role(
                                display_source,
                                DiagnosticTypeDisplayRole::AssignmentSource {
                                    target,
                                    anchor_idx: idx,
                                },
                            )
                        } else {
                            self.format_assignability_type_for_message(display_source, target)
                        },
                        if use_structural_source_display {
                            self.format_type_for_diagnostic_role(
                                target,
                                DiagnosticTypeDisplayRole::AssignmentTarget {
                                    source: display_source,
                                    anchor_idx: idx,
                                },
                            )
                        } else {
                            self.format_assignability_type_for_message(target, display_source)
                        },
                    )
                } else {
                    (
                        self.format_type_diagnostic(display_source),
                        self.format_type_diagnostic(target),
                    )
                };
                if let Some(widened) =
                    self.rewrite_standalone_literal_source_for_keyof_display(display_source, target)
                {
                    source_str = widened;
                }
                if source_str == "unknown" && source != TypeId::UNKNOWN {
                    let fallback =
                        self.format_assignability_type_for_message(display_source, target);
                    if fallback != "unknown" {
                        source_str = fallback;
                    }
                }
                if depth == 0
                    && let Some(switch_display) =
                        self.no_union_member_matches_switch_source_display(source, target, idx)
                {
                    source_str = switch_display;
                }
                if let Some(display) = self
                    .object_literal_property_literal_union_alias_target_display(
                        target,
                        &target_str,
                        idx,
                    )
                {
                    target_str = display;
                }
                let evaluated_target_for_suggestion = self.evaluate_type_with_env(target);
                if let Some(suggestion) = self.find_string_literal_spelling_suggestion(
                    source,
                    evaluated_target_for_suggestion,
                ) {
                    let display_target_str = self.format_ts2820_target_display(
                        target,
                        evaluated_target_for_suggestion,
                        &target_str,
                    );
                    let msg = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                        &[&source_str, &display_target_str, &suggestion],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        msg,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                    );
                }
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::NoCommonProperties {
                source_type: _,
                target_type: _,
            } => {
                // Use the unwidened source: tsc preserves literal spellings in
                // "has no properties in common" messages.
                let callable_widened_source =
                    ((crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        source,
                    ) || crate::query_boundaries::common::has_construct_signatures(
                        self.ctx.types,
                        source,
                    )) && depth == 0)
                        .then(|| {
                            let widened_source = self.widen_type_for_display(source);
                            self.widen_function_like_display_type(widened_source)
                        });
                let mut source_str = match callable_widened_source {
                    Some(widened_source) => {
                        self.format_type_for_assignability_message(widened_source)
                    }
                    None => self.format_type_diagnostic(source),
                };
                let target_str = self.format_type_for_assignability_message(target);

                // Fresh literal spellings can live only in display provenance
                // while the canonical type is already widened. Normalize that
                // residue for both `TS2559` and `TS2560`; canonical declared and
                // non-widening literal annotations stay untouched.
                let display_source = callable_widened_source.unwrap_or(source);
                let widened = self.widen_annotation_literals_for_display(
                    display_source,
                    crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
                );
                if widened.display_residue {
                    source_str = self.format_type_diagnostic_widened(widened.type_id);
                }

                // If calling the source would fix the mismatch, emit TS2560 instead.
                let (msg_template, code) = if self
                    .should_suggest_calling_for_weak_type(source, target)
                {
                    (
                            diagnostic_messages::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                            diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                        )
                } else {
                    (
                        diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                        diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    )
                };
                let (source_str, target_str) = self
                    .finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
                let message = format_message(msg_template, &[&source_str, &target_str]);
                Diagnostic::error(file_name, start, length, message, code)
            }

            SubtypeFailureReason::TypeMismatch {
                source_type: _,
                target_type: _,
            } => self.render_type_mismatch(&rctx),

            SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type,
                target_type,
            } => {
                // `tsc` preserves the source's type-alias name in the TS4104
                // message (`The type 'RA' is 'readonly' …`). `tsz` interns the
                // readonly array/tuple structurally, so the alias is recovered
                // from the source expression's declared annotation; falls back
                // to the structural display for inline (non-aliased) readonly
                // types and generic alias applications.
                let alias_source_display = if depth == 0 {
                    self.direct_diagnostic_source_expression(idx)
                        .or_else(|| self.assignment_source_expression(idx))
                        .and_then(|expr_idx| {
                            self.declared_source_type_reference_alias_name(expr_idx)
                        })
                } else {
                    None
                };
                let source_str = alias_source_display.unwrap_or_else(|| {
                    if let Some(inner) = crate::query_boundaries::common::readonly_inner_type(
                        self.ctx.types,
                        *source_type,
                    ) && let Some(tuple_display) =
                        self.format_tuple_shape_for_readonly_to_mutable(inner)
                    {
                        format!("readonly {tuple_display}")
                    } else {
                        self.format_type_diagnostic(*source_type)
                    }
                });
                let target_str = self
                    .format_tuple_shape_for_readonly_to_mutable(*target_type)
                    .unwrap_or_else(|| self.format_type_diagnostic(*target_type));
                let message = format_message(
                    diagnostic_messages::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE,
                )
            }

            SubtypeFailureReason::ParameterTypeMismatch {
                param_index,
                source_param,
                target_param,
                inner_reason,
            } => {
                // For top-level direct-callable mismatches whose param types
                // are themselves callable and non-generic, tsc treats the
                // inner contravariant comparison as a callback. When that
                // inner check fails on the callback's RETURN type, tsc
                // suppresses the outer "Type X is not assignable to Y"
                // (TS2322) wrapper and reports the diagnostic directly with
                // code TS2328 ("Types of parameters '_' and '_' are
                // incompatible.") — see checker.ts `reportErrorResults`,
                // which honours `overrideNextErrorInfo` bumped by the
                // elided `Call_signature_return_types_0_and_1_are_incompatible`
                // (TS2202) report. When the inner failure is on a
                // PARAMETER, no elision happens and tsc keeps the TS2322
                // wrapper.
                let contains_type_params = |ty| {
                    crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
                };
                let strict_callback_case = if depth == 0 {
                    let source_callable = self.callable_type_after_display_evaluation(source);
                    let target_callable = self.callable_type_after_display_evaluation(target);
                    let source_param_callable =
                        self.callable_type_after_display_evaluation(*source_param);
                    let target_param_callable =
                        self.callable_type_after_display_evaluation(*target_param);
                    source_callable.is_some()
                        && target_callable.is_some()
                        && source_param_callable.is_some()
                        && target_param_callable.is_some()
                        && !contains_type_params(source_param_callable.unwrap_or(*source_param))
                        && !contains_type_params(target_param_callable.unwrap_or(*target_param))
                } else {
                    false
                };
                // tsc 7.0.2 always reports a top-level function-to-function
                // assignment failure with a TS2322 head and chains the
                // `Types of parameters ... are incompatible.` (TS2328) frame
                // beneath it — it never promotes TS2328 to a standalone head
                // (no test in the conformance corpus has a TS2328 head, and
                // strictFunctionTypesErrors expects TS2322 for `fc1 = fc2`).
                // The former strict-callback branch that emitted a bare TS2328
                // head matched stale 6.0 `overrideNextErrorInfo` behavior and
                // has been removed; always render the TS2322 head + chain.
                {
                    // At depth > 0 we are rendering a nested property/element
                    // failure. The outer anchor index no longer points at the
                    // sub-expression whose type is `source`; using the
                    // `AssignmentSource` role would look up the outer RHS
                    // expression and render its type (e.g. the enclosing class
                    // instance) instead of the mismatched parameter's actual
                    // type. Use the structural formatter at depth > 0 so the
                    // rendered source matches the solver's `source` TypeId.
                    let (source_str, target_str) = if strict_callback_case {
                        self.strict_callback_assignment_display_pair(source, target, *param_index)
                            .unwrap_or_else(|| {
                                let source_str = if depth > 0 {
                                    self.format_type_for_assignability_message(source)
                                } else {
                                    self.format_type_for_diagnostic_role(
                                        source,
                                        DiagnosticTypeDisplayRole::AssignmentSource {
                                            target,
                                            anchor_idx: idx,
                                        },
                                    )
                                };
                                (
                                    source_str,
                                    self.format_assignability_type_for_message(target, source),
                                )
                            })
                    } else {
                        let source_str = if depth > 0 {
                            self.format_type_for_assignability_message(source)
                        } else {
                            self.format_type_for_diagnostic_role(
                                source,
                                DiagnosticTypeDisplayRole::AssignmentSource {
                                    target,
                                    anchor_idx: idx,
                                },
                            )
                        };
                        (
                            source_str,
                            self.format_assignability_type_for_message(target, source),
                        )
                    };
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    // When the enclosing source/target are themselves
                    // function/callable types, the line just rendered is the
                    // signature-to-signature relation line (e.g.
                    // `Type '(x: number) => void' is not assignable to type
                    // '(x: string) => void'.`). tsc then explains *why* the
                    // signatures differ with a `Types of parameters 'a' and 'b'
                    // are incompatible.` frame followed by the contravariant
                    // leaf relation. Descend into the structured parameter
                    // reason so the chain matches tsc instead of stopping at the
                    // bare function line.
                    self.push_parameter_mismatch_elaboration(
                        &mut diag,
                        &rctx,
                        *param_index,
                        *source_param,
                        *target_param,
                        inner_reason.as_deref(),
                    );
                    diag
                }
            }

            SubtypeFailureReason::IndexAccessTypeParameterMismatch {
                source_param,
                target_param,
                target_constraint,
            } => self.render_index_access_type_parameter_mismatch(
                &rctx,
                *source_param,
                *target_param,
                *target_constraint,
            ),

            SubtypeFailureReason::AbstractConstructorAssignment => {
                self.render_abstract_constructor_assignment(&rctx)
            }

            _ => {
                // At depth > 0 we are rendering a nested property/element
                // failure. The outer anchor index no longer points at the
                // sub-expression whose type is `source`; using the
                // AssignmentSource role would look up the outer RHS expression
                // and return the wrong type (e.g. the enclosing class instance
                // instead of the mismatched property type).  Use the plain
                // structural formatter instead so the rendered type matches the
                // solver's `source` TypeId.
                let mut source_str = if depth > 0 {
                    // Nested relation leaf: generalize a literal source to its
                    // base type when the target has no singleton capacity
                    // (tsc `reportRelationError`).
                    let display_source =
                        self.generalize_nested_relation_source_for_display(source, target);
                    self.format_type_for_assignability_message(display_source)
                } else {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                };
                // tsc `reportRelationError`: a generalized enum-ish source
                // against a non-singleton-capable target renders with
                // `UseFullyQualifiedType` (`P.Q`); every other enum display
                // stays bare (`Q`).
                if let Some(display) =
                    self.generalized_enum_source_qualified_display(source, target)
                {
                    source_str = display;
                }
                let mut target_str = self.format_assignability_type_for_message(target, source);
                if let Some(display) = self
                    .object_literal_property_literal_union_alias_target_display(
                        target,
                        &target_str,
                        idx,
                    )
                {
                    target_str = display;
                }
                if depth > 0 {
                    // Same-named nominal pairs at nested leaves disambiguate
                    // like the top level (tsc `getTypeNamesForErrorDisplay`).
                    (source_str, target_str) = self.finalize_pair_display_for_diagnostic(
                        source, target, source_str, target_str,
                    );
                }
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                let mut diagnostic = Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                // Same constraint-walk elaboration as the `TypeMismatch` arm's
                // top-level fallthrough (`render_type_mismatch`) — this
                // catch-all handles other bare-mismatch reasons (e.g.
                // `IntrinsicTypeMismatch` for a concrete-receiver `Bag[KSel]`
                // indexed access whose resolved value type is itself an
                // intrinsic) that keep the same as-written deferred operand on
                // the head line and need the same per-step walk beneath it.
                if depth == 0 && self.is_deferred_constraint_relative_source(source) {
                    self.push_deferred_constraint_walk_steps(
                        &mut diagnostic,
                        source,
                        target,
                        depth,
                    );
                }
                diagnostic
            }
        }
    }

    /// Render the TS2322 + TS2517 elaboration chain emitted when an abstract
    /// constructor type is assigned to a non-abstract constructor type.
    ///
    /// tsc emits, for `const c: new () => A = A` where `A` is abstract:
    ///
    /// ```text
    /// error TS2322: Type 'typeof A' is not assignable to type 'new () => A'.
    ///   Cannot assign an abstract constructor type to a non-abstract constructor type.
    /// ```
    ///
    /// The relation decision is correct on its own; only this explanation
    /// line was missing. The shape is independent of class/alias spelling:
    /// any abstract construct-signature source against a concrete
    /// construct-signature target produces it.
    fn render_abstract_constructor_assignment(&mut self, ctx: &RenderContext) -> Diagnostic {
        let (source_str, target_str) =
            self.format_top_level_assignability_message_types_at(ctx.source, ctx.target, ctx.idx);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            ctx.file_name.clone(),
            ctx.start,
            ctx.length,
            message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        diag.push_elaboration(
            diagnostic_messages::CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE,
            diagnostic_codes::CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE,
            0,
        );
        diag
    }

    fn object_literal_property_literal_union_alias_target_display(
        &mut self,
        target: TypeId,
        current_display: &str,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        if current_display.contains(" | ")
            || !self.anchor_is_within_object_literal_property(anchor_idx)
        {
            return None;
        }

        let evaluated = self.evaluate_type_for_assignability(target);
        let display_target =
            if crate::query_boundaries::common::union_members(self.ctx.types, target).is_some() {
                target
            } else {
                evaluated
            };
        let members =
            crate::query_boundaries::common::union_members(self.ctx.types, display_target)?;
        if members.len() < 2
            || !members.iter().all(|&member| {
                crate::query_boundaries::common::literal_value(self.ctx.types, member).is_some()
                    || member == TypeId::BOOLEAN_TRUE
                    || member == TypeId::BOOLEAN_FALSE
            })
        {
            return None;
        }

        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        // This site expands a literal-union alias target and joins its members
        // directly; order them through the shared union comparator so the
        // display matches tsc's sorted literal union (oracle
        // assignmentCompatWithDiscriminatedUnion: `"categorical" | "linear"`).
        let ordered = formatter.order_union_members_for_display(members.to_vec());
        Some(
            ordered
                .iter()
                .map(|&member| formatter.format(member).into_owned())
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    fn anchor_is_within_object_literal_property(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        for _ in 0..12 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            ) {
                return self
                    .ctx
                    .arena
                    .get_extended(current)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
            }
            if matches!(
                node.kind,
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
            ) {
                return false;
            }
            let Some(parent) = self.ctx.arena.get_extended(current).map(|ext| ext.parent) else {
                return false;
            };
            if parent.is_none() {
                return false;
            }
            current = parent;
        }
        false
    }
}
