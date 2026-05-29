//! JSX props union resolution, attr checking, spread validation, and children diagnostics.

use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

use super::attr_check_pipeline::{JsxAttrCheckContext, JsxAttrComparisonOutcome};

pub(crate) struct JsxPropsCheckOpts<'a> {
    pub(crate) attributes_idx: NodeIndex,
    pub(crate) props_type: TypeId,
    pub(crate) tag_name_idx: NodeIndex,
    pub(crate) component_type: Option<TypeId>,
    pub(crate) special_attr_component_type: Option<TypeId>,
    pub(crate) raw_props_has_type_params: bool,
    pub(crate) display_target: String,
    pub(crate) preferred_target_display: Option<&'a str>,
    pub(crate) request: &'a TypingRequest,
    pub(crate) children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
}

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn collect_jsx_union_resolution_attrs(
        &mut self,
        attributes_idx: NodeIndex,
    ) -> Option<Vec<(String, Option<TypeId>)>> {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return Some(Vec::new());
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return Some(Vec::new());
        };

        let mut provided = Vec::new();
        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let spread_data = self.ctx.arena.get_jsx_spread_attribute(attr_node)?;
                if !self.collect_jsx_union_resolution_spread_attrs(
                    spread_data.expression,
                    &mut provided,
                ) {
                    return None;
                }
                continue;
            }
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                continue;
            };
            let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                continue;
            };
            if matches!(attr_name.as_str(), "key" | "ref") {
                continue;
            }

            let attr_type = if attr_data.initializer.is_none() {
                Some(TypeId::BOOLEAN_TRUE)
            } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                    self.ctx
                        .arena
                        .get_jsx_expression(init_node)
                        .map(|expr| expr.expression)
                        .unwrap_or(attr_data.initializer)
                } else {
                    attr_data.initializer
                };
                if let Some(value_node) = self.ctx.arena.get(value_idx)
                    && matches!(
                        value_node.kind,
                        syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                    )
                {
                    // Only defer context-sensitive functions (those with unannotated params).
                    // Non-context-sensitive functions like `() => 10` can contribute to
                    // Round 1 inference by having their return type extracted.
                    if crate::computation::contextual::is_contextually_sensitive(self, value_idx) {
                        None
                    } else {
                        // Allow function types since we've already verified it's not context-sensitive
                        self.collect_jsx_union_resolution_attr_value_type(value_idx, true)
                    }
                } else {
                    self.collect_jsx_union_resolution_attr_value_type(value_idx, false)
                }
            } else {
                Some(TypeId::ANY)
            };

            provided.push((attr_name, attr_type));
        }

        let children_prop_name = self.get_jsx_children_prop_name();
        if self
            .get_jsx_body_child_nodes(attributes_idx)
            .is_some_and(|children| !children.is_empty())
            && !provided.iter().any(|(name, _)| name == &children_prop_name)
        {
            provided.push((children_prop_name, None));
        }

        Some(provided)
    }

    pub(in crate::checkers_domain::jsx) fn narrow_jsx_props_union_from_attributes(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
    ) -> TypeId {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, props_type)
        else {
            return props_type;
        };
        let member_count = members.len();

        let Some(provided_attrs) = self.collect_jsx_union_resolution_attrs(attributes_idx) else {
            return props_type;
        };
        let children_prop_name = self.get_jsx_children_prop_name();
        let provided_names: rustc_hash::FxHashSet<&str> = provided_attrs
            .iter()
            .map(|(name, _type_id)| name.as_str())
            .collect();
        let prefer_children_specificity = provided_names.contains(children_prop_name.as_str());

        let compatible: Vec<TypeId> = members
            .into_iter()
            .filter(|&member| {
                let member = self.resolve_type_for_property_access(member);

                let attrs_match = provided_attrs.iter().all(|(name, attr_type)| {
                    use crate::query_boundaries::common::PropertyAccessResult;
                    match self.resolve_property_access_with_env(member, name) {
                        PropertyAccessResult::Success { type_id, .. } => {
                            let expected = crate::query_boundaries::common::remove_undefined(
                                self.ctx.types,
                                type_id,
                            );
                            match attr_type {
                                Some(attr_type) => {
                                    *attr_type == TypeId::ANY
                                        || *attr_type == TypeId::ERROR
                                        || self
                                            .assign_relation_outcome(*attr_type, expected)
                                            .related
                                }
                                None => expected != TypeId::NEVER && expected != TypeId::ERROR,
                            }
                        }
                        _ => false,
                    }
                });
                if !attrs_match {
                    return false;
                }

                if let Some(shape) =
                    crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
                {
                    shape.properties.iter().all(|prop| {
                        if prop.optional {
                            return true;
                        }
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        prop_name.as_str() == children_prop_name.as_str()
                            || provided_names.contains(prop_name.as_str())
                    })
                } else {
                    true
                }
            })
            .collect();

        match compatible.len() {
            0 => props_type,
            1 => {
                if prefer_children_specificity {
                    self.normalize_jsx_props_member_for_children_resolution(compatible[0])
                } else {
                    compatible[0]
                }
            }
            _ if !prefer_children_specificity => {
                if compatible.len() >= member_count {
                    props_type
                } else {
                    let mut compatible_members = Vec::new();
                    let mut seen = rustc_hash::FxHashSet::default();
                    for member in compatible {
                        let key = self.format_type(member);
                        if seen.insert(key) {
                            compatible_members.push(member);
                        }
                    }

                    match compatible_members.len() {
                        0 => props_type,
                        1 => compatible_members[0],
                        _ => self.ctx.types.factory().union(compatible_members),
                    }
                }
            }
            _ => {
                let mut normalized_members = Vec::new();
                let mut seen = rustc_hash::FxHashSet::default();
                for member in compatible {
                    let member = self.normalize_jsx_props_member_for_children_resolution(member);
                    let key = self.format_type(member);
                    if seen.insert(key) {
                        normalized_members.push(member);
                    }
                }

                match normalized_members.len() {
                    0 => props_type,
                    1 => normalized_members[0],
                    _ => self.ctx.types.factory().union(normalized_members),
                }
            }
        }
    }
    /// Check JSX attributes against an already-evaluated props type.
    ///
    /// For each attribute, checks that the assigned value is assignable to the
    /// expected property type from the props interface. Emits:
    /// - TS2322 for type mismatches and excess properties
    /// - TS2741 for missing required properties
    ///
    /// `display_target` is the pre-formatted string shown in TS2322 error messages
    /// for excess properties. tsc uses `IntrinsicAttributes & PropsType` (or
    /// `IntrinsicAttributes & IntrinsicClassAttributes<T> & PropsType`) rather
    /// than just `PropsType`.
    ///
    /// Pipeline overview (see `attr_check_pipeline` for the named phase types):
    ///
    /// 1. Grammar + props-type normalization.
    /// 2. Optional early union-props delegation.
    /// 3. `prepare_jsx_attr_check_context` — build precomputed flags.
    /// 4. `compare_jsx_attributes_loop` — walk attributes, fill in
    ///    `JsxAttrComparisonOutcome` (provided attrs, spread entries,
    ///    error flags).
    /// 5. `emit_deferred_jsx_spread_diagnostics` — TS2322 spread mismatches
    ///    deferred until after override tracking is recorded.
    /// 6. `emit_jsx_children_synthesis_diagnostics` — fold body children in
    ///    and emit TS2710/TS2745/TS2746/TS2747.
    /// 7. `emit_jsx_attr_final_assignability_diagnostics` — precedence-ordered
    ///    cascade of whole-attrs TS2322 / TS2741.
    pub(crate) fn check_jsx_attributes_against_props(&mut self, opts: JsxPropsCheckOpts<'_>) {
        // Grammar check: TS17000 for empty expressions in JSX attributes.
        // Matches tsc: only the first empty expression per element is reported.
        self.check_grammar_jsx_element(opts.attributes_idx);

        // Normalize managed/evaluated JSX props before any checks so conditional,
        // mapped, and application-based surfaces (e.g.
        // `JSX.LibraryManagedAttributes<...>`) are read through the same
        // structural path we already use for missing-required-prop analysis.
        let raw_props_type = opts.props_type;
        let props_type = self.normalize_jsx_required_props_target(opts.props_type);

        // Union props: delegate to whole-object assignability checking.
        if crate::query_boundaries::common::is_union_type(self.ctx.types, props_type)
            && !(opts.raw_props_has_type_params && opts.component_type.is_none())
        {
            let union_display_target = self.build_jsx_union_props_display_target(
                raw_props_type,
                opts.component_type.or(opts.special_attr_component_type),
                opts.tag_name_idx,
                &opts.display_target,
            );
            self.check_jsx_union_props(
                opts.attributes_idx,
                props_type,
                &union_display_target,
                opts.tag_name_idx,
                opts.children_ctx,
            );
            return;
        }

        let ctx = self.prepare_jsx_attr_check_context(raw_props_type, props_type, &opts);

        let mut outcome = JsxAttrComparisonOutcome::default();
        self.compare_jsx_attributes_loop(&opts, &ctx, &mut outcome);
        self.emit_deferred_jsx_spread_diagnostics(&opts, &ctx, &mut outcome);
        self.emit_jsx_children_synthesis_diagnostics(&opts, &ctx, &mut outcome);
        self.emit_jsx_attr_final_assignability_diagnostics(&opts, &ctx, &outcome);
    }

    /// Phase 2 of `check_jsx_attributes_against_props`: walk every JSX
    /// attribute and spread, recording provided names/types, named-attribute
    /// override anchors, deferred spread entries, and the various early-stop
    /// error flags consumed by phases 3–5.
    ///
    /// Per-attribute diagnostics that anchor at a single attribute (TS2322
    /// excess property, TS2322 key/ref assignability, TS2322 value-type
    /// assignability, TS2783 overwrite-by-spread) are emitted inline here
    /// because their span depends on the attribute currently being walked.
    /// Whole-attrs diagnostics defer to the cascading helpers.
    fn compare_jsx_attributes_loop(
        &mut self,
        opts: &JsxPropsCheckOpts<'_>,
        ctx: &JsxAttrCheckContext,
        outcome: &mut JsxAttrComparisonOutcome,
    ) {
        let Some(attr_nodes) = self.jsx_attribute_node_indices(opts.attributes_idx) else {
            return;
        };

        for (attr_i, &attr_idx) in attr_nodes.iter().enumerate() {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                // Regular JSX attribute: name={value}
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };

                // Get attribute name (handles both simple and namespaced names like `ns:attr`)
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };
                outcome.has_explicit_jsx_attrs = true;

                // Track all attributes for missing-prop checking (including key/ref).
                // Even though key/ref are not checked against component props for TYPE
                // compatibility (they come from IntrinsicAttributes/IntrinsicClassAttributes),
                // they still need to be tracked as "provided" so the IntrinsicAttributes
                // missing-required-property check knows they were given.
                // Type will be filled in later after compute_type_of_node is called.
                outcome
                    .provided_attrs
                    .push((attr_name.clone(), TypeId::ANY));

                // Skip type-checking 'key' and 'ref' against component props.
                // These are special JSX attributes managed by IntrinsicAttributes /
                // IntrinsicClassAttributes, not by component props directly.
                // Checking them against the props type produces false positives when the
                // props type is an unevaluated application (e.g. DetailedHTMLProps<...>).
                if attr_name == "key" || attr_name == "ref" {
                    let expected_special_type = self
                        .get_jsx_special_attribute_expected_type(
                            &attr_name,
                            ctx.props_type,
                            opts.special_attr_component_type,
                        )
                        .map(|type_id| self.normalize_jsx_function_context_type(type_id));
                    let value_node_idx =
                        if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                            if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                                self.ctx
                                    .arena
                                    .get_jsx_expression(init_node)
                                    .map(|e| e.expression)
                                    .unwrap_or(attr_data.initializer)
                            } else {
                                attr_data.initializer
                            }
                        } else {
                            attr_data.initializer
                        };
                    let attr_value_type = if attr_data.initializer.is_none() {
                        TypeId::BOOLEAN_TRUE
                    } else if let Some(expected_type) = expected_special_type {
                        let expected_context_type =
                            self.normalize_jsx_required_props_target(expected_type);
                        let is_function_value =
                            self.ctx.arena.get(value_node_idx).is_some_and(|node| {
                                matches!(
                                    node.kind,
                                    syntax_kind_ext::ARROW_FUNCTION
                                        | syntax_kind_ext::FUNCTION_EXPRESSION
                                )
                            });
                        let contextual_expected_type = if is_function_value {
                            self.ctx
                                .implicit_any_contextual_closures
                                .insert(value_node_idx);
                            self.ctx
                                .implicit_any_checked_closures
                                .insert(value_node_idx);
                            self.invalidate_function_like_for_contextual_retry(value_node_idx);
                            self.refine_jsx_callable_contextual_type(expected_context_type)
                        } else {
                            expected_context_type
                        };
                        let attr_value_type = self.compute_type_of_node_with_request(
                            value_node_idx,
                            &opts
                                .request
                                .read()
                                .normal_origin()
                                .contextual(contextual_expected_type),
                        );
                        if is_function_value {
                            self.check_jsx_special_attribute_function_body(
                                value_node_idx,
                                contextual_expected_type,
                                opts.request,
                            );
                        }
                        attr_value_type
                    } else if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                        let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                            self.ctx
                                .arena
                                .get_jsx_expression(init_node)
                                .map(|e| e.expression)
                                .unwrap_or(attr_data.initializer)
                        } else {
                            attr_data.initializer
                        };
                        crate::query_boundaries::common::widen_type(
                            self.ctx.types,
                            self.compute_type_of_node(value_idx),
                        )
                    } else {
                        TypeId::ANY
                    };
                    if let Some(entry) = outcome.provided_attrs.last_mut() {
                        entry.1 = attr_value_type;
                    }
                    if let Some(expected_type) = expected_special_type {
                        if attr_data.initializer.is_none() {
                            if !self
                                .assign_relation_outcome(TypeId::BOOLEAN_TRUE, expected_type)
                                .related
                            {
                                let target_str = self.format_type(expected_type);
                                let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &["boolean", &target_str],
                                );
                                self.error_at_node(
                                    attr_data.name,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                );
                                outcome.has_prop_type_error = true;
                            }
                        } else if attr_value_type != TypeId::ANY
                            && attr_value_type != TypeId::ERROR
                            && !self.check_assignable_or_report_at(
                                attr_value_type,
                                expected_type,
                                value_node_idx,
                                attr_data.name,
                            )
                        {
                            outcome.has_prop_type_error = true;
                        }
                    } else if attr_name == "ref" && !ctx.props_has_type_params {
                        let attrs_type =
                            self.build_jsx_provided_attrs_object_type(&outcome.provided_attrs);
                        self.report_jsx_synthesized_props_assignability_error(
                            attrs_type,
                            &opts.display_target,
                            attr_data.name,
                        );
                        outcome.has_prop_type_error = true;
                    }
                    // Only skip normal prop checking if we found a special type for
                    // this attribute (from IntrinsicAttributes/IntrinsicClassAttributes
                    // or the props type itself). When the attribute isn't declared
                    // anywhere (e.g. minimal JSX types without IntrinsicAttributes
                    // defining 'key'), fall through so it gets checked as an excess
                    // property — matching tsc behavior.
                    if expected_special_type.is_some() || attr_name == "ref" {
                        continue;
                    }
                }

                // Track for TS2783 spread-overwrite detection
                outcome
                    .named_attr_nodes
                    .insert(attr_name.clone(), attr_data.name);

                // Skip prop-type checking when props type is any/error/contains-error,
                // or when an `any`/`error`/`unknown`-typed spread is present anywhere on
                // the element (the merged JSX-attributes object is `any`-compatible, so
                // tsc's `checkJsxExpression`/intersection logic suppresses TS2322 here).
                if ctx.skip_prop_checks || ctx.any_spread_present {
                    let attr_value_type =
                        self.compute_jsx_attr_value_type_without_context(attr_data.initializer);
                    if let Some(entry) = outcome.provided_attrs.last_mut() {
                        entry.1 = attr_value_type;
                    }
                    continue;
                }

                // Get expected type from props
                use crate::query_boundaries::common::PropertyAccessResult;
                let is_data_or_aria =
                    attr_name.starts_with("data-") || attr_name.starts_with("aria-");
                let is_special_named_attr = attr_name.contains('-') || attr_name.contains(':');
                let direct_prop_access =
                    self.resolve_property_access_with_env(ctx.props_type, &attr_name);
                let attr_prop_access = match direct_prop_access {
                    crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound {
                        ..
                    } if attr_name != "as" => {
                        if let Some(intrinsic_props) = ctx.as_intrinsic_props {
                            match self.resolve_property_access_with_env(intrinsic_props, &attr_name) {
                                crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. } => direct_prop_access,
                                intrinsic_access => intrinsic_access,
                            }
                        } else {
                            direct_prop_access
                        }
                    }
                    other => other,
                };
                let attr_prop_is_optional_in_anonymous_source = self
                    .jsx_attr_prop_is_optional_in_anonymous_source(
                        &direct_prop_access,
                        ctx.as_intrinsic_props,
                        ctx.props_type,
                        &attr_name,
                    );
                let (expected_type, expected_type_is_boolean_literal, original_property_type) =
                    match attr_prop_access {
                        PropertyAccessResult::Success {
                            type_id,
                            from_index_signature,
                            ..
                        } => {
                            // data-*/aria-* via index signature: skip (HTML convention).
                            if is_data_or_aria && from_index_signature {
                                if let Some(entry) = outcome.provided_attrs.last_mut() {
                                    entry.1 = self.compute_jsx_attr_value_type_without_context(
                                        attr_data.initializer,
                                    );
                                }
                                continue;
                            }
                            let write_check_type =
                                crate::query_boundaries::common::remove_undefined(
                                    self.ctx.types,
                                    type_id,
                                );
                            // Strip undefined from optional props (write-position checking).
                            let display_type = self.jsx_attr_display_target_type(
                                write_check_type,
                                type_id,
                                attr_prop_is_optional_in_anonymous_source,
                            );
                            (
                                write_check_type,
                                matches!(type_id, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE),
                                display_type,
                            )
                        }
                        PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                            let Some(type_id) = property_type else {
                                continue;
                            };
                            let write_check_type =
                                crate::query_boundaries::common::remove_undefined(
                                    self.ctx.types,
                                    type_id,
                                );
                            let display_type = self.jsx_attr_display_target_type(
                                write_check_type,
                                type_id,
                                attr_prop_is_optional_in_anonymous_source,
                            );
                            (
                                write_check_type,
                                matches!(type_id, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE),
                                display_type,
                            )
                        }
                        PropertyAccessResult::PropertyNotFound { .. } => {
                            // Compute actual value type (replacing ANY placeholder) for error messages.
                            let attr_value_type = self
                                .compute_jsx_attr_value_type_without_context(attr_data.initializer);
                            if let Some(entry) = outcome.provided_attrs.last_mut() {
                                entry.1 = attr_value_type;
                            }

                            let props_target_has_object_shape =
                                crate::query_boundaries::common::object_shape_for_type(
                                    self.ctx.types,
                                    ctx.props_type,
                                )
                                .is_some();
                            if ctx.component_has_managed_props_metadata
                                && !props_target_has_object_shape
                            {
                                outcome.needs_special_attr_object_assignability = true;
                                continue;
                            }

                            if !props_target_has_object_shape {
                                outcome.needs_special_attr_object_assignability = true;
                                continue;
                            }

                            // Check if the component has type parameters. This handles cases like
                            // class components with generic props where the display target is
                            // `IntrinsicAttributes & IntrinsicClassAttributes<ElemClass<T>> & { x: number; }`
                            // but the props_type has been instantiated to a concrete type.
                            let component_has_type_params = opts.component_type.is_some_and(|comp| {
                                self.is_generic_jsx_component(comp)
                                    || crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types,
                                        comp,
                                    )
                            }) || opts.special_attr_component_type.is_some_and(|comp| {
                                self.is_generic_jsx_component(comp)
                                    || crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types,
                                        comp,
                                    )
                            });

                            if !ctx.has_string_index // excess property check
                                && !outcome.has_excess_property_error
                                && !ctx.suppress_excess_for_generic_props
                                && !component_has_type_params
                                && !attr_name.starts_with("data-")
                                && !attr_name.starts_with("aria-")
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
                                    outcome.has_excess_property_error = true;
                                    continue;
                                }

                                // Build the synthesized JSX-attributes source-type display:
                                // when the element has spread attributes, tsc prints the merged
                                // object (`{ extra: true; onClick: ... }`) rather than just the
                                // single failing attribute. The helper falls back to `None` when
                                // it can't materialize the attrs, in which case we use the
                                // original single-attr fallback.
                                let synthesized = self
                                    .format_jsx_attrs_synthesized_source_for_excess(
                                        opts.attributes_idx,
                                        ctx.props_type,
                                        opts.request,
                                    );
                                let source_display = synthesized.unwrap_or_else(|| {
                                    let attr_type_name = if attr_data.initializer.is_none() {
                                        "true".to_string()
                                    } else {
                                        self.format_type(attr_value_type)
                                    };
                                    let display_name = {
                                        let mut chars = attr_name.chars();
                                        let is_ident = chars.next().is_some_and(|first| {
                                            (first == '_'
                                                || first == '$'
                                                || first.is_ascii_alphabetic())
                                                && chars.all(|ch| {
                                                    ch == '_'
                                                        || ch == '$'
                                                        || ch.is_ascii_alphanumeric()
                                                })
                                        });
                                        if is_ident {
                                            attr_name.clone()
                                        } else {
                                            format!("\"{attr_name}\"")
                                        }
                                    };
                                    format!("{{ {display_name}: {attr_type_name}; }}")
                                });
                                let base = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &[&source_display, &opts.display_target],
                                );
                                let message = format!(
                                    "{base}\n  Object literal may only specify known properties, \
                                     and '{attr_name}' does not exist in type '{}'.",
                                    opts.display_target
                                );
                                self.error_at_node(
                                    attr_idx,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                );
                                outcome.has_excess_property_error = true;
                            }
                            continue;
                        }
                        _ => continue,
                    };

                // Check attribute value assignability
                if attr_data.initializer.is_none() {
                    // Shorthand JSX attribute (e.g. `<X foo />`) is the
                    // literal type `true`. tsc emits the source as `true`
                    // when the target is a literal type (boolean literal or
                    // other literal like `2`), and widens to `boolean` for
                    // non-literal targets (like `number` or `string`).
                    if let Some(entry) = outcome.provided_attrs.last_mut() {
                        entry.1 = TypeId::BOOLEAN_TRUE;
                    }
                    if !self
                        .assign_relation_outcome(TypeId::BOOLEAN_TRUE, expected_type)
                        .related
                    {
                        let is_literal_target = crate::query_boundaries::common::is_literal_type(
                            self.ctx.types,
                            expected_type,
                        );
                        let source_str = if expected_type_is_boolean_literal || is_literal_target {
                            "true"
                        } else {
                            "boolean"
                        };
                        let target_str = self.format_type(expected_type);
                        let message = format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[source_str, &target_str],
                        );
                        self.error_at_node(
                            attr_data.name,
                            &message,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                        outcome.has_prop_type_error = true;
                    }
                    continue;
                }

                // The initializer might be a JSX expression wrapper or a string literal
                let mut initializer_is_bare_string_literal = false;
                let value_node_idx =
                    if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                        if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                            // Unwrap JSX expression to get the actual expression
                            if let Some(jsx_expr) = self.ctx.arena.get_jsx_expression(init_node) {
                                jsx_expr.expression
                            } else {
                                continue;
                            }
                        } else {
                            // String literal or other expression (no `{...}` wrapper).
                            // tsc preserves `| undefined` in TS2322 target display only
                            // for bare string-literal JSX attribute initializers.
                            initializer_is_bare_string_literal = true;
                            attr_data.initializer
                        }
                    } else {
                        continue;
                    };
                let expected_type = self.normalize_jsx_function_context_type(expected_type);

                // TS2783: Check if a later spread overwrites this attr (skip type check if so).
                let overwritten = self.check_jsx_attr_overwritten_by_spread(
                    &attr_name,
                    attr_data.name,
                    &attr_nodes,
                    attr_i,
                );

                if !overwritten {
                    let expected_context_type = self.evaluate_application_type(expected_type);
                    let expected_context_type =
                        self.resolve_type_for_property_access(expected_context_type);
                    let expected_context_type = self.resolve_lazy_type(expected_context_type);
                    let expected_context_type =
                        self.evaluate_application_type(expected_context_type);
                    let expected_context_type = self.evaluate_type_with_env(expected_context_type);
                    // Pre-extract before &mut self calls to release the arena borrow.
                    let value_node_fn_span = self
                        .ctx
                        .arena
                        .get(value_node_idx)
                        .filter(|n| n.is_function_expression_or_arrow())
                        .map(|n| (n.pos, n.end));

                    let mut function_param_diagnostic_span = None;
                    let contextual_expected_type = if let Some(fn_span) = value_node_fn_span {
                        // Determine whether contextual typing applies to this arrow function.
                        // For union props (e.g. `(e: MouseEvent) => void | undefined`),
                        // extract callable members first — the raw union fails
                        // `has_function_context`. For non-union non-callable types (e.g.
                        // `ReactNode`), exit early without calling
                        // `refine_jsx_callable_contextual_type` to avoid unnecessary
                        // `resolve_type_for_property_access` side-effects in its fallback path.
                        let is_directly_callable =
                            crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                expected_context_type,
                            )
                            .is_some()
                                || crate::query_boundaries::common::call_signatures_for_type(
                                    self.ctx.types,
                                    expected_context_type,
                                )
                                .is_some_and(|sigs| !sigs.is_empty());
                        let is_union = !is_directly_callable
                            && crate::query_boundaries::common::union_members(
                                self.ctx.types,
                                expected_context_type,
                            )
                            .is_some();
                        let refined = if is_directly_callable || is_union {
                            self.refine_jsx_callable_contextual_type(expected_context_type)
                        } else {
                            expected_context_type
                        };
                        let has_function_context = is_directly_callable
                            || crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                refined,
                            )
                            .is_some()
                            || crate::query_boundaries::common::call_signatures_for_type(
                                self.ctx.types,
                                refined,
                            )
                            .is_some_and(|sigs| !sigs.is_empty());
                        if !has_function_context {
                            let actual_type = self.compute_type_of_node(value_node_idx);
                            if let Some(entry) = outcome.provided_attrs.last_mut() {
                                entry.1 = actual_type;
                            }
                            continue;
                        }
                        self.ctx
                            .implicit_any_contextual_closures
                            .insert(value_node_idx);
                        self.ctx
                            .implicit_any_checked_closures
                            .insert(value_node_idx);
                        self.invalidate_function_like_for_contextual_retry(value_node_idx);
                        function_param_diagnostic_span = Some(fn_span);
                        refined
                    } else {
                        expected_context_type
                    };
                    // Set contextual type to preserve narrow literal types.
                    let is_function_attr = function_param_diagnostic_span.is_some();
                    let spec_snap = function_param_diagnostic_span
                        .map(|_| DiagnosticSpeculationSnapshot::new(&self.ctx));
                    let actual_type = self.compute_type_of_node_with_request(
                        value_node_idx,
                        &opts
                            .request
                            .read()
                            .normal_origin()
                            .contextual(contextual_expected_type),
                    );
                    if let (Some((start, end)), Some(snap)) =
                        (function_param_diagnostic_span, spec_snap)
                    {
                        snap.rollback_filtered(&mut self.ctx.diagnostic_state(), |diag| {
                            !(matches!(
                                diag.code,
                                7006 | 7019
                                    | 7031
                                    | 7051
                                    | diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                            ) && diag.start >= start
                                && diag.start < end)
                        });
                    }

                    if let Some(entry) = outcome.provided_attrs.last_mut() {
                        entry.1 = actual_type;
                    }
                    if is_special_named_attr {
                        if actual_type != TypeId::ANY
                            && actual_type != TypeId::ERROR
                            && !self
                                .assign_relation_outcome(actual_type, expected_type)
                                .related
                        {
                            outcome.needs_special_attr_object_assignability = true;
                        }
                        continue;
                    }
                    // Assignability check — tsc anchors at the attribute NAME.
                    //
                    // When the *expected* prop type contains unresolved type parameters
                    // (e.g., from a deferred conditional like
                    // `ExtractValueType<WrappedProps>`), skip per-attribute type
                    // checking. tsc's "applicability" mechanism is more lenient for
                    // generic components with complex signatures — it defers the real
                    // check to instantiation time. Without this, we emit false TS2322
                    // for valid JSX like:
                    //   <ReactSelectClass<ExtractValueType<WrappedProps>> value={props.value} />
                    // where the conditional type in the expected prop can't yet be
                    // resolved.
                    //
                    // We do NOT skip when only the *actual* attribute value contains
                    // type parameters and the expected type is concrete. tsc still
                    // checks `<Comp s={x} />` where `Comp` expects `s: string` and
                    // `x: T` is unconstrained — it emits TS2322 because `T`'s
                    // constraint (`unknown`) is not assignable to `string`. Letting the
                    // standard assignability path run handles both the constrained
                    // case (where the constraint satisfies the target) and the
                    // unconstrained case (where it does not).
                    let expected_has_unresolved_type_params =
                        crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            expected_type,
                        );
                    if actual_type != TypeId::ANY
                        && actual_type != TypeId::ERROR
                        && !expected_has_unresolved_type_params
                    {
                        let assignable = if is_function_attr {
                            if attr_name == self.get_jsx_children_prop_name() {
                                // JSX `children={p => "y"}` uses the same return
                                // elaboration as JSX body children: tsc points at the
                                // returned expression, not the `children` attribute name.
                                self.check_assignable_or_report_at_exact_anchor(
                                    actual_type,
                                    expected_type,
                                    value_node_idx,
                                    value_node_idx,
                                )
                            } else {
                                // For other function-valued JSX props, tsc anchors at
                                // the attribute name and displays an intersection of the
                                // inferred and expected function types in the error message.
                                self.check_assignable_or_report_jsx_callback_prop_at(
                                    actual_type,
                                    expected_type,
                                    value_node_idx,
                                    attr_data.name,
                                )
                            }
                        } else if let Some(result) = self
                            .try_emit_jsx_bare_string_attr_undefined_target(
                                actual_type,
                                expected_type,
                                original_property_type,
                                attr_data.name,
                                initializer_is_bare_string_literal,
                            )
                        {
                            result
                        } else {
                            self.check_assignable_or_report_at_with_display_types(
                                actual_type,
                                expected_type,
                                actual_type,
                                original_property_type,
                                value_node_idx,
                                attr_data.name,
                            )
                        };
                        if !assignable {
                            outcome.has_prop_type_error = true;
                        }
                    }
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                self.compare_jsx_spread_attribute(attr_idx, attr_i, opts, ctx, outcome);
            }
        }
    }
}
