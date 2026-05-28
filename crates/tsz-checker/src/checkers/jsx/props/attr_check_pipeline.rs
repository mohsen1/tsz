//! Named pipeline state for `check_jsx_attributes_against_props`.
//!
//! The JSX attribute check happens in five phases:
//!
//! 1. **Context preparation** — `prepare_jsx_attr_check_context` reads the
//!    `JsxPropsCheckOpts` and computes the structural flags that don't change
//!    while we walk the attributes (whether props are dynamic, whether the
//!    component carries a managed-props shape, whether `as="..."` resolves to
//!    intrinsic props, etc.). Output is `JsxAttrCheckContext`.
//!
//! 2. **Per-attribute comparison** — `compare_jsx_attributes_loop` (in
//!    `resolution.rs`) walks `JsxAttribute` and `JsxSpread` nodes and
//!    populates `JsxAttrComparisonOutcome` (provided attrs, error flags,
//!    deferred spread entries). The named-attribute arm stays inline because
//!    its body interleaves contextual-type derivation, speculation snapshots,
//!    and per-attribute TS2322 anchoring in ways that resist clean
//!    extraction. The spread arm is extracted to `compare_jsx_spread_attribute`
//!    here, since it is self-contained and benefits from a name.
//!
//! 3. **Deferred spread checking** — `emit_deferred_jsx_spread_diagnostics`
//!    consumes `spread_entries` collected during phase 2 and emits TS2322
//!    spread-property errors plus TS2741 missing-spread-prop bookkeeping.
//!
//! 4. **Children synthesis** — `emit_jsx_children_synthesis_diagnostics`
//!    folds body children into `provided_attrs`, emits TS2710/TS2745/TS2746/
//!    TS2747 and the body-children excess-property diagnostic.
//!
//! 5. **Final assignability cascade** —
//!    `emit_jsx_attr_final_assignability_diagnostics` decides which of the
//!    several whole-attrs TS2322 / TS2741 paths fires, in the precedence
//!    order that matches tsc.
//!
//! Phase mutability contract:
//! - Phase 1 returns `JsxAttrCheckContext` (immutable downstream).
//! - Phase 2 (in `resolution.rs`) mutates `JsxAttrComparisonOutcome`.
//! - Phase 3 (deferred spread) mutates the outcome (sets `spread_covers_all`,
//!   may flip `has_excess_property_error` via `check_spread_property_types`).
//! - Phase 4 (children synthesis) mutates the outcome (`provided_attrs.push`,
//!   `has_excess_property_error`).
//! - Phase 5 (final cascade) **only reads** the outcome and emits
//!   diagnostics — it deliberately takes `&JsxAttrComparisonOutcome`.
//!
//! No phase touches a private `TypeKey` or pattern-matches on solver
//! internals — all type queries go through `query_boundaries`.

use crate::checkers_domain::JsxChildrenContext;
use crate::context::TypingRequest;
use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

use super::resolution::JsxPropsCheckOpts;

/// Precomputed JSX prop-check state that does not change during the
/// per-attribute comparison loop. Built once by
/// `prepare_jsx_attr_check_context`.
pub(in crate::checkers_domain::jsx) struct JsxAttrCheckContext {
    /// Original (un-normalized) props type from `JsxPropsCheckOpts`.
    pub(in crate::checkers_domain::jsx) raw_props_type: TypeId,
    /// Normalized props target used for per-property assignability lookups.
    pub(in crate::checkers_domain::jsx) props_type: TypeId,
    pub(in crate::checkers_domain::jsx) has_string_index: bool,
    pub(in crate::checkers_domain::jsx) props_has_type_params: bool,
    pub(in crate::checkers_domain::jsx) suppress_excess_for_generic_props: bool,
    /// Skip TS2322/TS2741 per-attribute checks because props are dynamic
    /// (`any`/`error` or an application carrying an unresolved error arg).
    pub(in crate::checkers_domain::jsx) skip_prop_checks: bool,
    pub(in crate::checkers_domain::jsx) component_has_managed_props_metadata: bool,
    /// If the JSX attribute set contains `as="tag"`, the intrinsic props
    /// resolved for that tag (used as a fallback prop source).
    pub(in crate::checkers_domain::jsx) as_intrinsic_props: Option<TypeId>,
    pub(in crate::checkers_domain::jsx) class_props_overload_component_type: Option<TypeId>,
    pub(in crate::checkers_domain::jsx) route_class_props_mismatch_to_overload: bool,
    pub(in crate::checkers_domain::jsx) any_spread_present: bool,
}

/// Mutable comparison-outcome state accumulated by the per-attribute loop and
/// the deferred-spread/children passes. Consumed by the final assignability
/// cascade to decide which TS2322/TS2741 paths fire.
#[derive(Default)]
pub(in crate::checkers_domain::jsx) struct JsxAttrComparisonOutcome {
    pub(in crate::checkers_domain::jsx) provided_attrs: Vec<(String, TypeId)>,
    pub(in crate::checkers_domain::jsx) spread_covers_all: bool,
    pub(in crate::checkers_domain::jsx) has_excess_property_error: bool,
    pub(in crate::checkers_domain::jsx) needs_special_attr_object_assignability: bool,
    pub(in crate::checkers_domain::jsx) has_prop_type_error: bool,
    pub(in crate::checkers_domain::jsx) invalid_generic_spread_types: Vec<TypeId>,
    pub(in crate::checkers_domain::jsx) has_explicit_jsx_attrs: bool,
    pub(in crate::checkers_domain::jsx) named_attr_nodes: FxHashMap<String, NodeIndex>,
    /// `(spread_type, display_spread_type, expression_idx, attr_position)`.
    pub(in crate::checkers_domain::jsx) spread_entries: Vec<(TypeId, TypeId, NodeIndex, usize)>,
}

impl<'a> CheckerState<'a> {
    /// Phase 1: collect the structural flags that don't change while we walk
    /// JSX attributes. Caller has already normalized `props_type` via
    /// `normalize_jsx_required_props_target` and decided to NOT take the
    /// union-props delegation path.
    pub(in crate::checkers_domain::jsx) fn prepare_jsx_attr_check_context(
        &mut self,
        raw_props_type: TypeId,
        props_type: TypeId,
        opts: &JsxPropsCheckOpts<'_>,
    ) -> JsxAttrCheckContext {
        let props_object_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type);
        let props_has_error_type_in_args =
            crate::query_boundaries::common::contains_error_type_in_args(
                self.ctx.types,
                props_type,
            );
        let intrinsic_props_have_known_surface = opts.component_type.is_none()
            && opts.special_attr_component_type.is_none()
            && !opts.raw_props_has_type_params
            && (props_object_shape.is_some()
                || crate::query_boundaries::common::intersection_members(
                    self.ctx.types,
                    props_type,
                )
                .is_some());
        let skip_prop_checks = props_type == TypeId::ANY
            || props_type == TypeId::ERROR
            || (props_has_error_type_in_args && !intrinsic_props_have_known_surface);

        let has_string_index = props_object_shape
            .as_ref()
            .is_some_and(|shape| shape.string_index.is_some());

        let props_has_type_params = opts.raw_props_has_type_params
            || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                props_type,
            );
        let suppress_excess_for_generic_props = props_has_type_params
            && (opts.raw_props_has_type_params
                || opts.component_type.is_some()
                || opts.special_attr_component_type.is_some());

        let component_has_managed_props_metadata = opts.component_type.is_some_and(|comp| {
            use crate::query_boundaries::common::PropertyAccessResult;
            matches!(
                self.resolve_property_access_with_env(comp, "defaultProps"),
                PropertyAccessResult::Success { .. }
            ) || matches!(
                self.resolve_property_access_with_env(comp, "propTypes"),
                PropertyAccessResult::Success { .. }
            )
        });

        let as_intrinsic_props = self
            .collect_jsx_union_resolution_attrs(opts.attributes_idx)
            .and_then(|attrs| {
                attrs.into_iter().find_map(|(name, ty)| {
                    if name != "as" {
                        return None;
                    }
                    ty.and_then(|ty| self.get_jsx_single_string_literal_tag_name(ty))
                })
            })
            .and_then(|tag| self.get_jsx_intrinsic_props_for_tag(opts.tag_name_idx, &tag, false))
            .map(|ty| self.normalize_jsx_required_props_target(ty));

        let class_props_overload_component_type = if self
            .get_jsx_namespace_export_symbol_id("ElementType")
            .is_some()
            && !self.jsx_tag_is_logical_component_alias(opts.tag_name_idx)
        {
            opts.special_attr_component_type.or(opts.component_type)
        } else {
            None
        };
        let route_class_props_mismatch_to_overload = class_props_overload_component_type
            .is_some_and(|comp| self.should_report_jsx_class_missing_props_via_assignability(comp));

        // Snapshot attribute node ids before we re-enter `&mut self`; the
        // inner spread-type computation needs a mutable borrow, which conflicts
        // with iterating the arena-borrowed slice.
        let attr_node_indices = self
            .jsx_attribute_node_indices(opts.attributes_idx)
            .unwrap_or_default();
        let any_spread_present = attr_node_indices.iter().any(|&attr_idx| {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                return false;
            };
            if attr_node.kind != syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                return false;
            }
            let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                return false;
            };
            let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                spread_data.expression,
                &TypingRequest::NONE,
            );
            matches!(spread_type, TypeId::ANY | TypeId::ERROR)
        });

        JsxAttrCheckContext {
            raw_props_type,
            props_type,
            has_string_index,
            props_has_type_params,
            suppress_excess_for_generic_props,
            skip_prop_checks,
            component_has_managed_props_metadata,
            as_intrinsic_props,
            class_props_overload_component_type,
            route_class_props_mismatch_to_overload,
            any_spread_present,
        }
    }

    /// Snapshot the `NodeIndex` of every attribute inside a `JsxAttributes`
    /// node. Used to release the arena borrow before the per-attribute loop
    /// (and the spread-presence scan) calls back into `&mut self` for spread
    /// type computation or assignability checks.
    ///
    /// Returns `None` when either arena lookup misses so callers can preserve
    /// the original early-return semantics (rather than silently iterating an
    /// empty slice).
    pub(in crate::checkers_domain::jsx) fn jsx_attribute_node_indices(
        &self,
        attributes_idx: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
        let node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_jsx_attributes(node)?;
        Some(attrs.properties.nodes.to_vec())
    }

    /// Per-attribute phase 2 helper for the JSX spread-attribute arm of the
    /// walk in `compare_jsx_attributes_loop`. Computes the spread's effective
    /// type, emits TS2783 spread-overwrite diagnostics, mirrors `provided`
    /// entries from the spread shape, sets `spread_covers_all` when the
    /// spread satisfies the whole props type structurally, and pushes the
    /// `(spread, display_spread, expr_idx, attr_pos)` entry that phase 3
    /// will consume for TS2322 per-property checking.
    pub(in crate::checkers_domain::jsx) fn compare_jsx_spread_attribute(
        &mut self,
        attr_idx: NodeIndex,
        attr_i: usize,
        opts: &JsxPropsCheckOpts<'_>,
        ctx: &JsxAttrCheckContext,
        outcome: &mut JsxAttrComparisonOutcome,
    ) {
        let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
            return;
        };
        let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
            return;
        };
        let spread_expr_idx = spread_data.expression;
        let raw_spread_type = self.compute_type_of_node(spread_expr_idx);
        let spread_has_type_parameters = crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            raw_spread_type,
        );
        let unresolved_spread_into_generic_props = raw_spread_type == TypeId::UNKNOWN
            && crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                ctx.props_type,
            );
        if (spread_has_type_parameters || unresolved_spread_into_generic_props)
            && !outcome
                .invalid_generic_spread_types
                .contains(&raw_spread_type)
        {
            outcome.invalid_generic_spread_types.push(raw_spread_type);
        }

        // Set contextual type so spread literals preserve narrow types.
        let spread_request = if !ctx.skip_prop_checks {
            opts.request
                .read()
                .normal_origin()
                .contextual(ctx.props_type)
        } else {
            opts.request.read().normal_origin().contextual_opt(None)
        };
        let spread_type =
            self.compute_normalized_jsx_spread_type_with_request(spread_expr_idx, &spread_request);

        // any/error spread covers all properties (no TS2698 — tsc treats
        // these as dynamic). `unknown` spread is *not* covered: tsc emits
        // TS2698 for it (and for `T extends any` after constraint
        // normalization, whose apparent type resolves to `unknown`).
        if spread_type == TypeId::ANY || spread_type == TypeId::ERROR {
            // Mark all required props as provided (any spread covers everything)
            outcome.spread_covers_all = true;
            return;
        }

        // TS2698 spread validity is emitted by the JSX orchestration entry
        // (`check_jsx_spread_attrs_for_ts2698`). We still skip further
        // processing of an invalid spread here so we don't try to
        // enumerate properties from a non-object source.
        let resolved = self.resolve_lazy_type(spread_type);
        if resolved == TypeId::NEVER
            || !crate::query_boundaries::type_computation::access::is_valid_spread_type(
                self.ctx.types,
                resolved,
            )
        {
            return;
        }

        // TS2783: Check if any earlier explicit attributes will be
        // overwritten by required (non-optional) properties from this spread.
        if !outcome.named_attr_nodes.is_empty() {
            let spread_props = self.collect_object_spread_properties(spread_type);
            for sp in &spread_props {
                if !sp.optional {
                    let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                    if let Some(&attr_name_idx) = outcome.named_attr_nodes.get(&sp_name) {
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                            &[&sp_name],
                        );
                        self.error_at_node(
                            attr_name_idx,
                            &message,
                            crate::diagnostics::diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                        );
                    }
                }
            }
            // Clear required spread props from tracking.
            for sp in &spread_props {
                if !sp.optional {
                    let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                    outcome.named_attr_nodes.remove(&sp_name);
                }
            }
        }

        // Extract spread props for TS2741 tracking.
        if let Some(spread_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, spread_type)
        {
            for prop in &spread_shape.properties {
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                outcome
                    .provided_attrs
                    .push((prop_name.to_string(), prop.type_id));
            }
        }

        // When the spread type contains type parameters (e.g., `{...props}`
        // where `props: T`), we can't enumerate the properties it provides.
        // Mark spread_covers_all so missing-required-property checks (TS2741)
        // don't fire — the generic spread could provide any property.
        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, spread_type) {
            outcome.spread_covers_all = true;
        } else if !ctx.skip_prop_checks
            && self.diagnostic_relation_boolean_guard(spread_type, ctx.props_type)
        {
            // The solver reports the spread is structurally assignable to the
            // whole props type, so all required members are satisfied — including
            // ones inherited from Object.prototype (toString, valueOf, …) that
            // wouldn't appear in the spread's declared property shape. The
            // property-by-property missing check (TS2741) only walks declared
            // shapes, so it would otherwise emit a false positive when a spread
            // like `{...{}}` is fed into a target that requires only inherited
            // members. Defer to the solver here. Per-property type-mismatch
            // checking still runs via the deferred `check_spread_property_types`
            // below.
            outcome.spread_covers_all = true;
        }

        // Defer TS2322 spread checking until after attribute override tracking.
        if !ctx.skip_prop_checks {
            let display_spread_type = if crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                raw_spread_type,
            ) {
                raw_spread_type
            } else if self
                .ctx
                .arena
                .get(spread_expr_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            {
                spread_type
            } else {
                raw_spread_type
            };
            outcome.spread_entries.push((
                spread_type,
                display_spread_type,
                spread_expr_idx,
                attr_i,
            ));
        }
    }

    /// Phase 3: TS2322 spread-property checks deferred until after attribute
    /// override tracking has been recorded. Walks `outcome.spread_entries`
    /// in declaration order; for each spread we collect the names of later
    /// explicit attributes (which override it) and earlier explicit attrs
    /// (used for TS2783 anchoring of the per-property mismatch).
    pub(in crate::checkers_domain::jsx) fn emit_deferred_jsx_spread_diagnostics(
        &mut self,
        opts: &JsxPropsCheckOpts<'_>,
        ctx: &JsxAttrCheckContext,
        outcome: &mut JsxAttrComparisonOutcome,
    ) {
        if outcome.spread_entries.is_empty() {
            return;
        }

        let Some(attr_nodes) = self.jsx_attribute_node_indices(opts.attributes_idx) else {
            return;
        };

        let mut explicit_attr_entries: Vec<(usize, String, NodeIndex)> = Vec::new();
        for (i, &node_idx) in attr_nodes.iter().enumerate() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(node)
                && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                && let Some(attr_name) = self.get_jsx_attribute_name(name_node)
            {
                explicit_attr_entries.push((i, attr_name, attr_data.name));
            }
        }

        let spread_count = outcome.spread_entries.len();
        let merged_attrs_display = self.format_jsx_attrs_effective_source_for_spread_assignability(
            opts.attributes_idx,
            ctx.props_type,
            opts.request,
        );
        // Loop-invariant: `opts.children_ctx` is fixed for the whole element,
        // so the "are there JSX body children?" predicate is constant across
        // every spread iteration.
        let has_body_children = opts
            .children_ctx
            .as_ref()
            .is_some_and(|child_ctx| child_ctx.child_count > 0);

        let mut suppress_missing_props_from_spread = false;
        let mut earlier_spread_props: FxHashSet<String> = FxHashSet::default();
        let spread_entries_snapshot: Vec<(TypeId, TypeId, NodeIndex, usize)> =
            outcome.spread_entries.clone();
        for (i, &(spread_type, raw_spread_type, _spread_expr_idx, spread_pos)) in
            spread_entries_snapshot.iter().enumerate()
        {
            // Only later explicit attributes override the current spread.
            let mut overridden: FxHashSet<&str> = explicit_attr_entries
                .iter()
                .filter(|(attr_pos, _, _)| *attr_pos > spread_pos)
                .map(|(_, name, _)| name.as_str())
                .collect();
            for prop_name in &earlier_spread_props {
                overridden.insert(prop_name.as_str());
            }

            // For missing property checks (TS2741), also include explicit
            // attrs that come BEFORE this spread - they provide the property.
            let mut overridden_for_missing = overridden.clone();
            for (attr_pos, attr_name, _) in &explicit_attr_entries {
                if *attr_pos < spread_pos {
                    overridden_for_missing.insert(attr_name.as_str());
                }
            }

            // Earlier explicit attrs (BEFORE this spread): when the spread
            // overrides one of them (TS2783) AND the spread's prop type
            // mismatches the expected, the per-property TS2322 anchors here.
            let earlier_explicit_attrs: FxHashMap<String, NodeIndex> = explicit_attr_entries
                .iter()
                .filter(|(attr_pos, _, _)| *attr_pos < spread_pos)
                .map(|(_, name, name_idx)| (name.clone(), *name_idx))
                .collect();

            // When JSX body children exist, treat `children` as already
            // provided so spreads without `children` don't trigger TS2741.
            if has_body_children {
                overridden.insert("children");
                overridden_for_missing.insert("children");
            }

            let has_later_spreads = i < spread_count - 1;
            let has_later_explicit_excess_attr = outcome.has_excess_property_error
                && explicit_attr_entries
                    .iter()
                    .filter(|(attr_pos, _, _)| *attr_pos > spread_pos)
                    .any(|(_, attr_name, _)| {
                        if attr_name == "key"
                            || attr_name == "ref"
                            || attr_name.starts_with("data-")
                            || attr_name.starts_with("aria-")
                        {
                            return false;
                        }
                        !matches!(
                            self.resolve_property_access_with_env(ctx.props_type, attr_name),
                            crate::query_boundaries::common::PropertyAccessResult::Success { .. }
                                | crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                                    property_type: Some(_),
                                    ..
                                }
                        )
                    });

            // TS2710 will fire when the spread has a `children` property and
            // body children exist; suppress the per-property missing check.
            let spread_has_children = if let Some(spread_shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, spread_type)
            {
                spread_shape.properties.iter().any(|p| {
                    let name = self.ctx.types.resolve_atom(p.name);
                    name == "children"
                })
            } else {
                false
            };
            let suppress_missing_props = spread_has_children && has_body_children;

            let had_error =
                self.check_spread_property_types(super::super::spread::SpreadCheckOpts {
                    spread_type,
                    spread_source_type: raw_spread_type,
                    props_type: ctx.props_type,
                    tag_name_idx: opts.tag_name_idx,
                    overridden_names: &overridden,
                    overridden_for_missing: &overridden_for_missing,
                    earlier_explicit_attrs: &earlier_explicit_attrs,
                    has_later_spreads,
                    suppress_missing_props,
                    suppress_unanchored_type_mismatch: outcome.has_prop_type_error
                        || has_later_explicit_excess_attr,
                    display_target: &opts.display_target,
                    preferred_target_display: opts.preferred_target_display,
                    merged_attrs_display: merged_attrs_display.as_deref(),
                });
            suppress_missing_props_from_spread |= had_error || suppress_missing_props;

            // Record this spread's property names for later iterations.
            let resolved_spread = self.evaluate_type_with_env(spread_type);
            let resolved_spread = self.resolve_type_for_property_access(resolved_spread);
            if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                resolved_spread,
            ) {
                for prop in &shape.properties {
                    earlier_spread_props.insert(self.ctx.types.resolve_atom(prop.name).to_string());
                }
            }
        }

        if suppress_missing_props_from_spread {
            outcome.spread_covers_all = true;
        }
    }

    /// Phase 4: fold JSX body children into `provided_attrs`, emit TS2710 for
    /// double-children specification, the body-children excess-property
    /// diagnostic, TS2745/TS2746 via the shape classifier, and TS2747 for
    /// text children not accepted by the component.
    pub(in crate::checkers_domain::jsx) fn emit_jsx_children_synthesis_diagnostics(
        &mut self,
        opts: &JsxPropsCheckOpts<'_>,
        ctx: &JsxAttrCheckContext,
        outcome: &mut JsxAttrComparisonOutcome,
    ) {
        let Some(children_ctx) = opts.children_ctx.clone() else {
            return;
        };
        let JsxChildrenContext {
            child_count,
            has_text_child,
            contextual_type,
            synthesized_type,
            text_child_indices,
        } = children_ctx;

        // TS2710: explicit children attr + body children = double specification.
        let children_prop_name = self.get_jsx_children_prop_name();
        let has_explicit_children_attr =
            self.has_explicit_jsx_attribute(opts.attributes_idx, &children_prop_name);
        if has_explicit_children_attr && !ctx.skip_prop_checks {
            // tsc reports TS2710 on the JsxAttributes node, which spans from
            // the first attribute to the closing >. Our parser sets
            // JsxAttributes.pos to the first token after the tag name, matching
            // tsc's behavior.
            self.error_at_node_msg(
                opts.attributes_idx,
                diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN,
                &[&children_prop_name],
            );
        }

        outcome
            .provided_attrs
            .push((children_prop_name.clone(), synthesized_type));
        if child_count > 0
            && !has_explicit_children_attr
            && !ctx.skip_prop_checks
            && !outcome.has_excess_property_error
            && !ctx.has_string_index
            && !ctx.props_has_type_params
            && !opts.display_target.is_empty()
        {
            let has_intrinsic_key_or_ref = outcome
                .provided_attrs
                .iter()
                .any(|(name, _)| name == "key" || name == "ref");
            use crate::query_boundaries::common::PropertyAccessResult;
            let props_has_children = matches!(
                self.resolve_property_access_with_env(ctx.props_type, &children_prop_name),
                PropertyAccessResult::Success { .. }
            );
            let intrinsic_has_children =
                self.get_intrinsic_attributes_type().is_some_and(|ia_type| {
                    let resolved_ia = self.resolve_type_for_property_access(ia_type);
                    matches!(
                        self.resolve_property_access_with_env(resolved_ia, &children_prop_name),
                        PropertyAccessResult::Success { .. }
                    )
                });
            if has_intrinsic_key_or_ref && !props_has_children && !intrinsic_has_children {
                self.report_jsx_body_children_excess_property(
                    opts.tag_name_idx,
                    &opts.display_target,
                    &outcome.provided_attrs,
                );
                outcome.has_excess_property_error = true;
            }
        }
        // TS2745/TS2746: route JSX body children through one normalized
        // classifier so union/tuple shapes don't drift by component path.
        if child_count > 0 && !ctx.skip_prop_checks {
            self.check_jsx_children_shape(
                ctx.props_type,
                opts.attributes_idx,
                child_count,
                has_text_child,
                contextual_type,
                synthesized_type,
                opts.tag_name_idx,
            );
        }
        // TS2747: text children not accepted by component.
        if has_text_child
            && !ctx.skip_prop_checks
            && !self.jsx_children_shape_diagnostic_takes_precedence(ctx.props_type, child_count)
        {
            self.check_jsx_text_children_accepted(
                ctx.props_type,
                opts.tag_name_idx,
                &text_child_indices,
            );
        }
    }

    /// Phase 5: precedence-ordered cascade of whole-attrs TS2322 / TS2741
    /// diagnostics. Each gated branch checks the structural condition tsc uses
    /// and records whether it fired so later branches can suppress.
    pub(in crate::checkers_domain::jsx) fn emit_jsx_attr_final_assignability_diagnostics(
        &mut self,
        opts: &JsxPropsCheckOpts<'_>,
        ctx: &JsxAttrCheckContext,
        outcome: &JsxAttrComparisonOutcome,
    ) {
        // For nonstandard ElementChildrenAttribute names, tsc reports the
        // missing required children property through whole-object
        // assignability (TS2322) rather than the generic TS2741 JSX fallback.
        let reported_custom_children_assignability = if !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && !ctx.skip_prop_checks
            && !outcome.needs_special_attr_object_assignability
            && self.should_report_custom_jsx_children_via_assignability(
                ctx.props_type,
                &outcome.provided_attrs,
            ) {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &opts.display_target,
                opts.tag_name_idx,
            );
            true
        } else {
            false
        };

        // tsc suppresses whole-attrs TS2322 when props is primitive and an
        // IntrinsicAttributes required-prop is missing — TS2741 covers it.
        let suppress_for_primitive_props_with_missing_ia_required =
            crate::query_boundaries::common::is_primitive_type(self.ctx.types, ctx.props_type)
                && self.get_intrinsic_attributes_type().is_some_and(|ia| {
                    self.jsx_has_missing_required_props(ia, &outcome.provided_attrs)
                });
        let reported_special_attr_assignability = if !reported_custom_children_assignability
            && !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && !ctx.skip_prop_checks
            && outcome.needs_special_attr_object_assignability
            // When props have unresolved type parameters, the synthesized
            // attrs type is incomplete — generic spread contributions are not
            // captured by get_object_shape, so the object built from
            // provided_attrs is missing those properties. tsc skips this
            // path for generic components.
            && !ctx.props_has_type_params
            && !suppress_for_primitive_props_with_missing_ia_required
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
            if !self.diagnostic_relation_boolean_guard(attrs_type, ctx.props_type) {
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &opts.display_target,
                    opts.tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        let class_missing_props_component_type =
            opts.special_attr_component_type.or(opts.component_type);
        let empty_attrs_with_children_injected_props = outcome.provided_attrs.is_empty()
            && self.strip_jsx_children_injection_for_display(ctx.props_type) != ctx.props_type;

        let class_has_missing_required_props =
            self.jsx_has_missing_required_props(ctx.props_type, &outcome.provided_attrs);
        let reported_class_missing_props_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && !ctx.skip_prop_checks
            && !opts.display_target.is_empty()
            && !empty_attrs_with_children_injected_props
            && !outcome.has_prop_type_error
            && !self.jsx_tag_is_logical_component_alias(opts.tag_name_idx)
            && class_has_missing_required_props
        {
            if ctx.route_class_props_mismatch_to_overload
                && ctx.class_props_overload_component_type.is_some_and(|comp| {
                    self.report_jsx_class_props_overload_failure_if_needed(
                        comp,
                        ctx.props_type,
                        opts.attributes_idx,
                        opts.tag_name_idx,
                        opts.children_ctx.clone(),
                    )
                })
            {
                true
            } else if class_missing_props_component_type.is_some_and(|comp| {
                self.should_report_jsx_class_missing_props_via_assignability(comp)
            }) {
                let attrs_type = self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &opts.display_target,
                    opts.tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        // TS2322: whole-object assignability for bare type-parameter props.
        let props_is_type_param =
            crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ctx.props_type);
        let spread_satisfies_type_param = props_is_type_param
            && outcome
                .spread_entries
                .iter()
                .any(|&(spread_type, display_spread_type, _, _)| {
                    self.diagnostic_relation_boolean_guard(spread_type, ctx.props_type)
                        || crate::query_boundaries::checkers::jsx::spread_source_covers_readonly_wrapped_type_parameter(
                            self.ctx.types,
                            &self.ctx.definition_store,
                            spread_type,
                            ctx.props_type,
                        )
                        || crate::query_boundaries::checkers::jsx::spread_source_covers_readonly_wrapped_type_parameter(
                            self.ctx.types,
                            &self.ctx.definition_store,
                            display_spread_type,
                            ctx.props_type,
                        )
                });
        let react_alias_spread_only_contributes_children = props_is_type_param
            && !outcome.has_explicit_jsx_attrs
            && !outcome.spread_entries.is_empty()
            && !outcome.provided_attrs.is_empty()
            && outcome
                .provided_attrs
                .iter()
                .all(|(name, _)| name == "children")
            && opts
                .special_attr_component_type
                .or(opts.component_type)
                .is_some_and(|component| {
                    self.is_react_jsx_component_alias_application(component)
                        || self.is_react_jsx_component_alias_union(component)
                });
        let reported_type_param_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_class_missing_props_assignability
            && !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && !ctx.skip_prop_checks
            && !outcome.has_prop_type_error
            && props_is_type_param
            && !self.jsx_props_type_is_library_managed_attributes_application(ctx.raw_props_type)
            && !spread_satisfies_type_param
            && !react_alias_spread_only_contributes_children
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
            if !self.diagnostic_relation_boolean_guard(attrs_type, ctx.props_type) {
                // tsc uses just the type parameter name here (e.g. "P"), not
                // the full "IntrinsicAttributes & P" display target. The
                // IntrinsicAttributes intersection check for spread
                // attributes is handled separately by
                // check_generic_sfc_spread_intrinsic_attrs.
                let type_param_target = self.format_type(ctx.props_type);
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &type_param_target,
                    opts.tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        let reported_invalid_generic_spread_assignability = self
            .report_invalid_generic_jsx_spread_assignability(
                super::generic_spread::GenericSpreadAssignabilityReport {
                    generic_spread_types: outcome.invalid_generic_spread_types.clone(),
                    provided_attrs: &outcome.provided_attrs,
                    props_type: ctx.props_type,
                    display_target: &opts.display_target,
                    tag_name_idx: opts.tag_name_idx,
                    has_excess_property_error: outcome.has_excess_property_error,
                    skip_prop_checks: ctx.skip_prop_checks,
                    has_explicit_jsx_attrs: outcome.has_explicit_jsx_attrs,
                },
            );

        let reported_dynamic_intrinsic_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_class_missing_props_assignability
            && !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && !ctx.skip_prop_checks
            && opts.component_type.is_none()
            && outcome.provided_attrs.is_empty()
            && opts.raw_props_has_type_params
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &opts.display_target,
                opts.tag_name_idx,
            );
            true
        } else {
            false
        };

        let reported_generic_managed_attrs_assignability =
            if !reported_custom_children_assignability
                && !reported_special_attr_assignability
                && !reported_class_missing_props_assignability
                && !reported_type_param_assignability
                && !reported_invalid_generic_spread_assignability
                && !reported_dynamic_intrinsic_assignability
                && !outcome.has_excess_property_error
                && !outcome.spread_covers_all
                && !ctx.skip_prop_checks
                && !outcome.has_prop_type_error
                && opts.component_type.is_some()
                && outcome.provided_attrs.is_empty()
                && opts.raw_props_has_type_params
                && self.jsx_props_type_is_library_managed_attributes_application(ctx.raw_props_type)
            {
                self.emit_jsx_generic_managed_attrs_assignability(opts, ctx, outcome)
            } else {
                false
            };

        // TS2741: missing required properties.
        if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_type_param_assignability
            && !reported_invalid_generic_spread_assignability
            && !reported_dynamic_intrinsic_assignability
            && !reported_generic_managed_attrs_assignability
            && (!reported_class_missing_props_assignability
                || (outcome.provided_attrs.is_empty() && opts.raw_props_has_type_params))
            && !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && !ctx.skip_prop_checks
            && !outcome.has_prop_type_error
        {
            self.check_missing_required_jsx_props(
                ctx.props_type,
                &outcome.provided_attrs,
                opts.tag_name_idx,
                Some(opts.tag_name_idx),
                opts.preferred_target_display,
            );
        }

        // Also check required IntrinsicAttributes.
        if !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && let Some(intrinsic_attrs_type) = self.get_intrinsic_attributes_type()
        {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &outcome.provided_attrs,
                opts.tag_name_idx,
                None,
                None,
            );
        }

        if !outcome.has_excess_property_error
            && !outcome.spread_covers_all
            && let Some(comp) = opts.special_attr_component_type
            && let Some(intrinsic_class_attrs_type) =
                self.get_intrinsic_class_attributes_type_for_component(comp)
        {
            self.check_missing_required_jsx_props(
                intrinsic_class_attrs_type,
                &outcome.provided_attrs,
                opts.tag_name_idx,
                None,
                None,
            );
        }
    }

    /// Helper for the generic-managed-attrs branch of the final cascade;
    /// kept as a separate method to keep the cascade body readable.
    fn emit_jsx_generic_managed_attrs_assignability(
        &mut self,
        opts: &JsxPropsCheckOpts<'_>,
        ctx: &JsxAttrCheckContext,
        outcome: &JsxAttrComparisonOutcome,
    ) -> bool {
        let attrs_type = self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
        if !crate::query_boundaries::checkers::jsx::types_are_assignable(
            self,
            attrs_type,
            ctx.raw_props_type,
        ) {
            let display_props_type = opts
                .component_type
                .filter(|&component| {
                    crate::query_boundaries::checkers::jsx::is_type_parameter_like(
                        self.ctx.types,
                        component,
                    )
                })
                .and_then(|component| {
                    let mut props_type = self
                        .get_jsx_type_parameter_callable_constraint_props_type(component)
                        .unwrap_or(ctx.props_type);
                    if outcome.provided_attrs.is_empty()
                        && (!crate::query_boundaries::checkers::jsx::has_object_shape(
                            self.ctx.types,
                            props_type,
                        ) || self.jsx_type_contains_callable_surface(props_type))
                    {
                        props_type = attrs_type;
                    }
                    self.get_jsx_library_managed_attributes_application(component, props_type)
                })
                .or_else(|| {
                    if outcome.provided_attrs.is_empty() {
                        let component = self
                            .jsx_library_managed_attributes_application_args(ctx.raw_props_type)
                            .and_then(|args| args.first().copied())?;
                        return self
                            .get_jsx_library_managed_attributes_application(component, attrs_type);
                    }
                    None
                })
                .unwrap_or(ctx.raw_props_type);
            let mut target = self
                .jsx_library_managed_attributes_application_display(display_props_type)
                .or_else(|| self.jsx_library_managed_structural_props_display(display_props_type))
                .unwrap_or_else(|| self.format_type(display_props_type));
            if target.starts_with("LibraryManagedAttributes<") && target.ends_with(", Element>") {
                target.truncate(target.len() - ", Element>".len());
                target.push_str(", {}>");
            }
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &target,
                opts.tag_name_idx,
            );
            true
        } else {
            false
        }
    }
}
