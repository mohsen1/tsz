//! JSX props union resolution, attribute-vs-props checking (TS2322), spread property
//! validation, and children excess property diagnostics.

use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::diagnostics::{diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Format a single property fragment "name: type" used inside the synthesized
    /// JSX-attributes source-type display. Mirrors tsc's per-property display:
    /// shorthand attrs render as `name: true`, others use the formatted value type.
    fn format_jsx_synthesized_prop_fragment(&mut self, name: &str, type_id: TypeId) -> String {
        let display_name = {
            let mut chars = name.chars();
            let is_ident = chars.next().is_some_and(|first| {
                (first == '_' || first == '$' || first.is_ascii_alphabetic())
                    && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            });
            if is_ident {
                name.to_string()
            } else {
                format!("\"{name}\"")
            }
        };
        let type_str = if type_id == TypeId::BOOLEAN_TRUE {
            "true".to_string()
        } else {
            self.format_type(type_id)
        };
        format!("{display_name}: {type_str}")
    }

    /// Build the synthesized JSX-attributes source-type display string for the
    /// per-attribute excess-property TS2322 diagnostic.
    ///
    /// Walks the attributes once and produces a formatted object-type string with
    /// explicit (non-spread) attrs first (in source order), then spread-derived
    /// props that aren't shadowed by an explicit attr (in spread source order).
    /// This matches tsc's display for elements like `<X {...{p: v}} q />` where
    /// the printed source type is `{ q: true; p: v; }`.
    ///
    /// All `compute_*` calls below are cache hits during a normal check pass:
    /// the main attribute loop has already computed each attribute and spread
    /// type, so re-walking does not double-report diagnostics.
    fn format_jsx_attrs_synthesized_source_for_excess(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        request: &TypingRequest,
    ) -> Option<String> {
        let attrs_node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_jsx_attributes(attrs_node)?;

        let mut explicit: Vec<(String, TypeId)> = Vec::new();
        let mut spread_props: Vec<(String, TypeId)> = Vec::new();

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };
                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                let attr_value_type = if attr_data.initializer.is_none() {
                    TypeId::BOOLEAN_TRUE
                } else {
                    self.compute_jsx_attr_value_type_without_context(attr_data.initializer)
                };

                if let Some(existing) = explicit.iter_mut().find(|(n, _)| n == &attr_name) {
                    existing.1 = attr_value_type;
                } else {
                    explicit.push((attr_name, attr_value_type));
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_request = request.read().normal_origin().contextual(props_type);
                let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                    spread_data.expression,
                    &spread_request,
                );
                if matches!(spread_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
                    continue;
                }
                if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    spread_type,
                ) {
                    // shape.properties is sorted by atom for canonical interning;
                    // walk in declaration order so the synthesized source-type
                    // mirrors tsc's display, which preserves source order.
                    let mut props_by_decl: Vec<&tsz_solver::PropertyInfo> =
                        shape.properties.iter().collect();
                    props_by_decl.sort_by_key(|p| p.declaration_order);
                    for prop in props_by_decl {
                        let name = self.ctx.types.resolve_atom(prop.name).to_string();
                        if name == "key" || name == "ref" {
                            continue;
                        }
                        if let Some(existing) = spread_props.iter_mut().find(|(n, _)| *n == name) {
                            existing.1 = prop.type_id;
                        } else {
                            spread_props.push((name, prop.type_id));
                        }
                    }
                }
            }
        }

        if explicit.is_empty() && spread_props.is_empty() {
            return None;
        }

        let explicit_names: rustc_hash::FxHashSet<String> =
            explicit.iter().map(|(n, _)| n.clone()).collect();
        let mut fragments: Vec<String> = Vec::with_capacity(explicit.len() + spread_props.len());
        for (name, type_id) in &explicit {
            fragments.push(self.format_jsx_synthesized_prop_fragment(name, *type_id));
        }
        for (name, type_id) in &spread_props {
            if explicit_names.contains(name) {
                continue;
            }
            fragments.push(self.format_jsx_synthesized_prop_fragment(name, *type_id));
        }

        if fragments.is_empty() {
            return None;
        }
        Some(format!("{{ {}; }}", fragments.join("; ")))
    }

    fn compute_jsx_attr_value_type_without_context(&mut self, initializer: NodeIndex) -> TypeId {
        if initializer.is_none() {
            return TypeId::BOOLEAN_TRUE;
        }
        let init_node_idx = initializer;
        if let Some(init_node) = self.ctx.arena.get(init_node_idx) {
            let value_idx = if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                self.ctx
                    .arena
                    .get_jsx_expression(init_node)
                    .map(|expr| expr.expression)
                    .unwrap_or(init_node_idx)
            } else {
                init_node_idx
            };
            return self.compute_type_of_node(value_idx);
        }
        TypeId::ANY
    }

    fn collect_jsx_union_resolution_attr_value_type(
        &mut self,
        value_idx: NodeIndex,
        allow_function_types: bool,
    ) -> Option<TypeId> {
        let Some(value_node) = self.ctx.arena.get(value_idx) else {
            return Some(TypeId::ANY);
        };
        if !allow_function_types
            && matches!(
                value_node.kind,
                syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
            )
        {
            return None;
        }

        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let ty = self.compute_type_of_node(value_idx);
        self.ctx.preserve_literal_types = prev;
        Some(ty)
    }

    fn collect_jsx_union_resolution_spread_attrs(
        &mut self,
        expr_idx: NodeIndex,
        provided: &mut Vec<(String, Option<TypeId>)>,
    ) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(expr_node) else {
            return false;
        };

        for &elem_idx in &obj_lit.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    let Some(name) = self.get_property_name(prop.name) else {
                        return false;
                    };
                    let ty =
                        self.collect_jsx_union_resolution_attr_value_type(prop.initializer, true);
                    provided.push((name, ty));
                }
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) else {
                        continue;
                    };
                    let Some(name) = self.get_property_name(prop.name) else {
                        return false;
                    };
                    let ty = self.collect_jsx_union_resolution_attr_value_type(prop.name, true);
                    provided.push((name, ty));
                }
                syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR => {
                    let name = match elem_node.kind {
                        syntax_kind_ext::METHOD_DECLARATION => self
                            .ctx
                            .arena
                            .get_method_decl(elem_node)
                            .and_then(|method| self.get_property_name(method.name)),
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                            .ctx
                            .arena
                            .get_accessor(elem_node)
                            .and_then(|accessor| self.get_property_name(accessor.name)),
                        _ => None,
                    };
                    let Some(name) = name else {
                        return false;
                    };
                    let ty = self.collect_jsx_union_resolution_attr_value_type(elem_idx, true);
                    provided.push((name, ty));
                }
                syntax_kind_ext::SPREAD_ASSIGNMENT | syntax_kind_ext::SPREAD_ELEMENT => {
                    let Some(spread) = self.ctx.arena.get_spread(elem_node) else {
                        return false;
                    };
                    if !self.collect_jsx_union_resolution_spread_attrs(spread.expression, provided)
                    {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        true
    }

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
                                        || self.is_assignable_to(*attr_type, expected)
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_jsx_attributes_against_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        component_type: Option<TypeId>,
        special_attr_component_type: Option<TypeId>,
        raw_props_has_type_params: bool,
        display_target: String,
        preferred_target_display: Option<&str>,
        request: &TypingRequest,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        // Grammar check: TS17000 for empty expressions in JSX attributes.
        // Matches tsc: only the first empty expression per element is reported.
        self.check_grammar_jsx_element(attributes_idx);

        let original_props_type = props_type;
        // Normalize managed/evaluated JSX props before any checks so conditional,
        // mapped, and application-based surfaces (for example
        // JSX.LibraryManagedAttributes<...>) are read through the same structural
        // path we already use for missing-required-prop analysis.
        let props_type = self.normalize_jsx_required_props_target(props_type);

        // Union props: delegate to whole-object assignability checking.
        if crate::query_boundaries::common::is_union_type(self.ctx.types, props_type) {
            self.check_jsx_union_props(attributes_idx, props_type, tag_name_idx, children_ctx);
            return;
        }
        // Skip attribute-vs-props checking for any/error props.
        let skip_prop_checks = props_type == TypeId::ANY
            || props_type == TypeId::ERROR
            || crate::query_boundaries::common::contains_error_type_in_args(
                self.ctx.types,
                props_type,
            );

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // String index signature → any attribute name is valid.
        let has_string_index =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type)
                .is_some_and(|shape| shape.string_index.is_some());

        // Suppress excess-property errors when props has unresolved type params.
        // Check both raw and evaluated props (evaluation may collapse type params).
        let props_has_type_params = raw_props_has_type_params
            || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                props_type,
            );
        let component_has_managed_props_metadata = component_type.is_some_and(|comp| {
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
            .collect_jsx_union_resolution_attrs(attributes_idx)
            .and_then(|attrs| {
                attrs.into_iter().find_map(|(name, ty)| {
                    if name != "as" {
                        return None;
                    }
                    ty.and_then(|ty| self.get_jsx_single_string_literal_tag_name(ty))
                })
            })
            .and_then(|tag| self.get_jsx_intrinsic_props_for_tag(tag_name_idx, &tag, false))
            .map(|ty| self.normalize_jsx_required_props_target(ty));

        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let mut spread_covers_all = false;
        let mut has_excess_property_error = false;
        let mut needs_special_attr_object_assignability = false;
        let mut has_prop_type_error = false;

        // TS2783: track explicit attr names for spread overwrite detection.
        let mut named_attr_nodes: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();

        // Deferred spread entries: (spread_type, expr_idx, attr_index) for TS2322.
        let mut spread_entries: Vec<(TypeId, NodeIndex, usize)> = Vec::new();

        // Pre-scan: if any attribute is an `any`/`error`/`unknown`-typed spread,
        // tsc widens the merged JSX-attributes object to be `any`-compatible
        // and skips per-attribute assignability checks against the props type
        // for *every* explicit attribute on the element (regardless of order).
        // Mirrors `tsxSpreadAttributesResolution12.tsx` where
        // `<OverWriteAttr {...anyobj} x={3} />` produces no TS2322.
        let attr_nodes = &attrs.properties.nodes;
        let any_spread_present = attr_nodes.iter().any(|&attr_idx| {
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
            matches!(spread_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
        });

        // Check each attribute
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

                // Track all attributes for missing-prop checking (including key/ref).
                // Even though key/ref are not checked against component props for TYPE
                // compatibility (they come from IntrinsicAttributes/IntrinsicClassAttributes),
                // they still need to be tracked as "provided" so the IntrinsicAttributes
                // missing-required-property check knows they were given.
                // Type will be filled in later after compute_type_of_node is called.
                provided_attrs.push((attr_name.clone(), TypeId::ANY));

                // Skip type-checking 'key' and 'ref' against component props.
                // These are special JSX attributes managed by IntrinsicAttributes /
                // IntrinsicClassAttributes, not by component props directly.
                // Checking them against the props type produces false positives when the
                // props type is an unevaluated application (e.g. DetailedHTMLProps<...>).
                if attr_name == "key" || attr_name == "ref" {
                    let expected_special_type = self
                        .get_jsx_special_attribute_expected_type(
                            &attr_name,
                            props_type,
                            special_attr_component_type,
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
                        let contextual_expected_type =
                            if self.ctx.arena.get(value_node_idx).is_some_and(|node| {
                                node.kind == syntax_kind_ext::ARROW_FUNCTION
                                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                            }) {
                                self.refine_jsx_callable_contextual_type(expected_context_type)
                            } else {
                                expected_context_type
                            };
                        self.compute_type_of_node_with_request(
                            value_node_idx,
                            &request
                                .read()
                                .normal_origin()
                                .contextual(contextual_expected_type),
                        )
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
                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = attr_value_type;
                    }
                    if let Some(expected_type) = expected_special_type {
                        if attr_data.initializer.is_none() {
                            if !self.is_assignable_to(TypeId::BOOLEAN_TRUE, expected_type) {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
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
                                has_prop_type_error = true;
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
                            has_prop_type_error = true;
                        }
                    } else if attr_name == "ref" && !props_has_type_params {
                        let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
                        self.report_jsx_synthesized_props_assignability_error(
                            attrs_type,
                            &display_target,
                            attr_data.name,
                        );
                        has_prop_type_error = true;
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
                named_attr_nodes.insert(attr_name.clone(), attr_data.name);

                // Skip prop-type checking when props type is any/error/contains-error,
                // or when an `any`/`error`/`unknown`-typed spread is present anywhere on
                // the element (the merged JSX-attributes object is `any`-compatible, so
                // tsc's `checkJsxExpression`/intersection logic suppresses TS2322 here).
                if skip_prop_checks || any_spread_present {
                    let attr_value_type =
                        self.compute_jsx_attr_value_type_without_context(attr_data.initializer);
                    if let Some(entry) = provided_attrs.last_mut() {
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
                    self.resolve_property_access_with_env(props_type, &attr_name);
                let attr_prop_access = match direct_prop_access {
                    crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound {
                        ..
                    } if attr_name != "as" => {
                        if let Some(intrinsic_props) = as_intrinsic_props {
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
                let (expected_type, expected_type_is_boolean_literal) = match attr_prop_access {
                    PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => {
                        // data-*/aria-* via index signature: skip (HTML convention).
                        if is_data_or_aria && from_index_signature {
                            if let Some(entry) = provided_attrs.last_mut() {
                                entry.1 = self.compute_jsx_attr_value_type_without_context(
                                    attr_data.initializer,
                                );
                            }
                            continue;
                        }
                        let write_check_type = crate::query_boundaries::common::remove_undefined(
                            self.ctx.types,
                            type_id,
                        );
                        // Strip undefined from optional props (write-position checking).
                        (
                            write_check_type,
                            matches!(type_id, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE),
                        )
                    }
                    PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                        let Some(type_id) = property_type else {
                            continue;
                        };
                        let write_check_type = crate::query_boundaries::common::remove_undefined(
                            self.ctx.types,
                            type_id,
                        );
                        (
                            write_check_type,
                            matches!(type_id, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE),
                        )
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        // Compute actual value type (replacing ANY placeholder) for error messages.
                        let attr_value_type =
                            self.compute_jsx_attr_value_type_without_context(attr_data.initializer);
                        if let Some(entry) = provided_attrs.last_mut() {
                            entry.1 = attr_value_type;
                        }

                        if component_has_managed_props_metadata {
                            needs_special_attr_object_assignability = true;
                            continue;
                        }

                        let props_target_has_object_shape =
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                props_type,
                            )
                            .is_some();
                        if !props_target_has_object_shape {
                            needs_special_attr_object_assignability = true;
                            continue;
                        }

                        // Check if the component has type parameters. This handles cases like
                        // class components with generic props where the display target is
                        // `IntrinsicAttributes & IntrinsicClassAttributes<ElemClass<T>> & { x: number; }`
                        // but the props_type has been instantiated to a concrete type.
                        let component_has_type_params = component_type.is_some_and(|comp| {
                            self.is_generic_jsx_component(comp)
                                || crate::query_boundaries::common::contains_type_parameters(
                                    self.ctx.types,
                                    comp,
                                )
                        }) || special_attr_component_type
                            .is_some_and(|comp| {
                                self.is_generic_jsx_component(comp)
                                    || crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types,
                                        comp,
                                    )
                            });

                        if !has_string_index // excess property check
                            && !props_has_type_params
                            && !component_has_type_params
                            && !attr_name.starts_with("data-")
                            && !attr_name.starts_with("aria-")
                        {
                            // Build the synthesized JSX-attributes source-type display:
                            // when the element has spread attributes, tsc prints the merged
                            // object (`{ extra: true; onClick: ... }`) rather than just the
                            // single failing attribute. The helper falls back to `None` when
                            // it can't materialize the attrs, in which case we use the
                            // original single-attr fallback.
                            let synthesized = self.format_jsx_attrs_synthesized_source_for_excess(
                                attributes_idx,
                                props_type,
                                request,
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
                                                ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
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
                                &[&source_display, &display_target],
                            );
                            let message = format!(
                                "{base}\n  Object literal may only specify known properties, \
                                     and '{attr_name}' does not exist in type '{display_target}'."
                            );
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                attr_idx,
                                &message,
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            );
                            has_excess_property_error = true;
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
                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = TypeId::BOOLEAN_TRUE;
                    }
                    if !self.is_assignable_to(TypeId::BOOLEAN_TRUE, expected_type) {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
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
                        has_prop_type_error = true;
                    }
                    continue;
                }

                // The initializer might be a JSX expression wrapper or a string literal
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
                            // String literal or other expression
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
                    attr_nodes,
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
                    let mut function_param_diagnostic_span = None;
                    if let Some(value_node) = self.ctx.arena.get(value_node_idx)
                        && matches!(
                            value_node.kind,
                            syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                        )
                    {
                        let has_function_context =
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
                        if !has_function_context {
                            if let Some(entry) = provided_attrs.last_mut() {
                                entry.1 = TypeId::ANY;
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
                        let param_span_end = self
                            .ctx
                            .arena
                            .get_function(value_node)
                            .and_then(|func| self.ctx.arena.get(func.body))
                            .map_or(value_node.end, |body_node| body_node.pos);
                        function_param_diagnostic_span = Some((value_node.pos, param_span_end));
                    }
                    let contextual_expected_type =
                        if self.ctx.arena.get(value_node_idx).is_some_and(|node| {
                            node.kind == syntax_kind_ext::ARROW_FUNCTION
                                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                        }) {
                            self.refine_jsx_callable_contextual_type(expected_context_type)
                        } else {
                            expected_context_type
                        };
                    // Set contextual type to preserve narrow literal types.
                    let spec_snap = function_param_diagnostic_span
                        .map(|_| DiagnosticSpeculationSnapshot::new(&self.ctx));
                    let actual_type = self.compute_type_of_node_with_request(
                        value_node_idx,
                        &request
                            .read()
                            .normal_origin()
                            .contextual(contextual_expected_type),
                    );
                    if let (Some((start, end)), Some(snap)) =
                        (function_param_diagnostic_span, spec_snap)
                    {
                        snap.rollback_filtered(&mut self.ctx, |diag| {
                            !(matches!(diag.code, 7006 | 7019 | 7031 | 7051)
                                && diag.start >= start
                                && diag.start < end)
                        });
                    }

                    if let Some(entry) = provided_attrs.last_mut() {
                        entry.1 = actual_type;
                    }
                    if is_special_named_attr {
                        if actual_type != TypeId::ANY
                            && actual_type != TypeId::ERROR
                            && !self.is_assignable_to(actual_type, expected_type)
                        {
                            needs_special_attr_object_assignability = true;
                        }
                        continue;
                    }
                    // Assignability check — tsc anchors at the attribute NAME.
                    //
                    // When either the actual or expected type contains unresolved type
                    // parameters (e.g., from a deferred conditional like
                    // `ExtractValueType<WrappedProps>`), skip per-attribute type checking.
                    // tsc's "applicability" mechanism is more lenient for generic
                    // components with complex signatures — it defers the real check to
                    // instantiation time. Without this, we emit false TS2322 for valid
                    // JSX like:
                    //   <ReactSelectClass<ExtractValueType<WrappedProps>> value={props.value} />
                    // where the conditional type in `props.value` can't yet be resolved.
                    let attr_has_unresolved_type_params =
                        crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            expected_type,
                        ) || crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            actual_type,
                        );
                    if actual_type != TypeId::ANY
                        && actual_type != TypeId::ERROR
                        && !attr_has_unresolved_type_params
                        && !self.check_assignable_or_report_at(
                            actual_type,
                            expected_type,
                            value_node_idx,
                            attr_data.name,
                        )
                    {
                        has_prop_type_error = true;
                    }
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_expr_idx = spread_data.expression;
                // Set contextual type so spread literals preserve narrow types.
                let spread_request = if !skip_prop_checks {
                    request.read().normal_origin().contextual(props_type)
                } else {
                    request.read().normal_origin().contextual_opt(None)
                };
                let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                    spread_expr_idx,
                    &spread_request,
                );

                // any/error/unknown spread covers all properties.
                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    // Mark all required props as provided (any spread covers everything)
                    spread_covers_all = true;
                    continue;
                }

                // TS2698: Validate spread type is object-like.
                // tsc rejects spreading `null`, `undefined`, `never`, primitives in JSX.
                // This runs regardless of skip_prop_checks — it's independent of props type.
                let resolved = self.resolve_lazy_type(spread_type);
                if resolved == TypeId::NEVER
                    || !crate::query_boundaries::type_computation::access::is_valid_spread_type(
                        self.ctx.types,
                        resolved,
                    )
                {
                    self.report_spread_not_object_type(spread_expr_idx);
                    continue;
                }

                // TS2783: Check if any earlier explicit attributes will be
                // overwritten by required (non-optional) properties from this spread.
                if !named_attr_nodes.is_empty() {
                    let spread_props = self.collect_object_spread_properties(spread_type);
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            if let Some(&attr_name_idx) = named_attr_nodes.get(&sp_name) {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                    &[&sp_name],
                                );
                                self.error_at_node(
                                    attr_name_idx,
                                    &message,
                                    diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                );
                            }
                        }
                    }
                    // Clear required spread props from tracking.
                    for sp in &spread_props {
                        if !sp.optional {
                            let sp_name = self.ctx.types.resolve_atom(sp.name).to_string();
                            named_attr_nodes.remove(&sp_name);
                        }
                    }
                }

                // Extract spread props for TS2741 tracking.
                if let Some(spread_shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    spread_type,
                ) {
                    for prop in &spread_shape.properties {
                        let prop_name = self.ctx.types.resolve_atom(prop.name);
                        provided_attrs.push((prop_name.to_string(), prop.type_id));
                    }
                }

                // When the spread type contains type parameters (e.g., `{...props}`
                // where `props: T`), we can't enumerate the properties it provides.
                // Mark spread_covers_all so missing-required-property checks (TS2741)
                // don't fire — the generic spread could provide any property.
                if crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    spread_type,
                ) {
                    spread_covers_all = true;
                }

                // Defer TS2322 spread checking until after attribute override tracking.
                if !skip_prop_checks {
                    spread_entries.push((spread_type, spread_expr_idx, attr_i));
                }
            }
        }

        // TS2322: Check spread props against expected types (deferred to account for overrides).
        if !spread_entries.is_empty() {
            // Track explicit attrs WITH their attr index AND name node index, so
            // the spread checker can anchor per-property TS2322 at the earlier
            // explicit attribute when a spread overrides it (TS2783 case).
            let mut explicit_attr_entries: Vec<(usize, String, NodeIndex)> = Vec::new();
            let mut suppress_missing_props_from_spread = false;
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

            let spread_count = spread_entries.len();
            // Collect property names from each spread so later iterations know
            // what properties earlier spreads already provide.
            let mut earlier_spread_props: rustc_hash::FxHashSet<String> =
                rustc_hash::FxHashSet::default();
            for (i, &(spread_type, _spread_expr_idx, spread_pos)) in
                spread_entries.iter().enumerate()
            {
                // Only later explicit attributes override the current spread.
                let mut overridden: rustc_hash::FxHashSet<&str> = explicit_attr_entries
                    .iter()
                    .filter(|(attr_pos, _, _)| *attr_pos > spread_pos)
                    .map(|(_, name, _)| name.as_str())
                    .collect();
                // Also include properties already provided by earlier spreads.
                // This prevents false TS2739 on the last spread when earlier spreads
                // cover some of the required properties.
                for prop_name in &earlier_spread_props {
                    overridden.insert(prop_name.as_str());
                }

                // For missing property checks (TS2741), also include explicit attrs
                // that come BEFORE this spread - they provide the property.
                let mut overridden_for_missing = overridden.clone();
                for (attr_pos, attr_name, _) in &explicit_attr_entries {
                    if *attr_pos < spread_pos {
                        overridden_for_missing.insert(attr_name.as_str());
                    }
                }

                // Earlier explicit attrs (BEFORE this spread): when the spread
                // overrides one of them (TS2783) AND the spread's prop type
                // mismatches the expected, the per-property TS2322 anchors here.
                let earlier_explicit_attrs: rustc_hash::FxHashMap<String, NodeIndex> =
                    explicit_attr_entries
                        .iter()
                        .filter(|(attr_pos, _, _)| *attr_pos < spread_pos)
                        .map(|(_, name, name_idx)| (name.clone(), *name_idx))
                        .collect();

                // When JSX body children exist, treat `children` as already provided
                // so spreads that don't include `children` don't trigger TS2741.
                if children_ctx.as_ref().is_some_and(|ctx| ctx.child_count > 0) {
                    overridden.insert("children");
                    overridden_for_missing.insert("children");
                }

                // Check if there are later spreads that could provide missing properties.
                let has_later_spreads = i < spread_count - 1;

                // Check if TS2710 will be emitted: spread has children property AND there are body children
                let spread_has_children = if let Some(spread_shape) =
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        spread_type,
                    ) {
                    spread_shape.properties.iter().any(|p| {
                        let name = self.ctx.types.resolve_atom(p.name);
                        name == "children"
                    })
                } else {
                    false
                };
                let has_body_children =
                    children_ctx.as_ref().is_some_and(|ctx| ctx.child_count > 0);
                let suppress_missing_props = spread_has_children && has_body_children;

                let had_error = self.check_spread_property_types(
                    spread_type,
                    props_type,
                    tag_name_idx,
                    &overridden,
                    &overridden_for_missing,
                    &earlier_explicit_attrs,
                    has_later_spreads,
                    suppress_missing_props,
                    &display_target,
                );
                suppress_missing_props_from_spread |= had_error || suppress_missing_props;

                // Record this spread's property names for later iterations.
                let resolved_spread = self.evaluate_type_with_env(spread_type);
                let resolved_spread = self.resolve_type_for_property_access(resolved_spread);
                if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    resolved_spread,
                ) {
                    for prop in &shape.properties {
                        earlier_spread_props
                            .insert(self.ctx.types.resolve_atom(prop.name).to_string());
                    }
                }
            }

            if suppress_missing_props_from_spread {
                spread_covers_all = true;
            }
        }

        // JSX children synthesis: incorporate body children into provided props.
        if let Some(crate::checkers_domain::JsxChildrenContext {
            child_count,
            has_text_child,
            contextual_type,
            synthesized_type,
            text_child_indices,
        }) = children_ctx
        {
            // TS2710: explicit children attr + body children = double specification.
            // Error location: the first JSX attribute (matching tsc's span).
            let children_prop_name = self.get_jsx_children_prop_name();
            let has_explicit_children_attr =
                self.has_explicit_jsx_attribute(attributes_idx, &children_prop_name);
            if has_explicit_children_attr && !skip_prop_checks {
                // tsc reports TS2710 on the JsxAttributes node, which spans from
                // the first attribute to the closing >. Our parser sets JsxAttributes.pos
                // to the first token after the tag name, matching tsc's behavior.
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    attributes_idx,
                    diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN,
                    &[&children_prop_name],
                );
            }

            provided_attrs.push((children_prop_name.clone(), synthesized_type));
            if child_count > 0
                && !has_explicit_children_attr
                && !skip_prop_checks
                && !has_excess_property_error
                && !has_string_index
                && !props_has_type_params
                && !display_target.is_empty()
            {
                let has_intrinsic_key_or_ref = provided_attrs
                    .iter()
                    .any(|(name, _)| name == "key" || name == "ref");
                use crate::query_boundaries::common::PropertyAccessResult;
                let props_has_children = matches!(
                    self.resolve_property_access_with_env(props_type, &children_prop_name),
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
                        tag_name_idx,
                        &display_target,
                        &provided_attrs,
                    );
                    has_excess_property_error = true;
                }
            }
            // TS2745/TS2746: route JSX body children through one normalized
            // classifier so union/tuple shapes don't drift by component path.
            if child_count > 0 && !skip_prop_checks {
                self.check_jsx_children_shape(
                    props_type,
                    attributes_idx,
                    child_count,
                    has_text_child,
                    contextual_type,
                    synthesized_type,
                    tag_name_idx,
                );
            }
            // TS2747: text children not accepted by component.
            if has_text_child
                && !skip_prop_checks
                && !self.jsx_children_shape_diagnostic_takes_precedence(props_type, child_count)
            {
                self.check_jsx_text_children_accepted(
                    props_type,
                    tag_name_idx,
                    &text_child_indices,
                );
            }
        }

        // For nonstandard ElementChildrenAttribute names, tsc reports the
        // missing required children property through whole-object
        // assignability (TS2322) rather than the generic TS2741 JSX fallback.
        let reported_custom_children_assignability = if !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !needs_special_attr_object_assignability
            && self.should_report_custom_jsx_children_via_assignability(props_type, &provided_attrs)
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &display_target,
                tag_name_idx,
            );
            true
        } else {
            false
        };

        let reported_special_attr_assignability = if !reported_custom_children_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && needs_special_attr_object_assignability
            // When props have unresolved type parameters, the synthesized attrs type
            // is incomplete — generic spread contributions (e.g., `{...props}` where
            // `props: T`) are not captured by get_object_shape, so the object built
            // from provided_attrs is missing those properties. Checking it against
            // the full props type produces false TS2322. TSC skips this path for
            // generic components.
            && !props_has_type_params
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            if !self.is_assignable_to(attrs_type, props_type) {
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &display_target,
                    tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        let class_missing_props_component_type = special_attr_component_type.or(component_type);

        let reported_class_missing_props_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !display_target.is_empty()
            && !has_prop_type_error
            && !self.jsx_tag_is_logical_component_alias(tag_name_idx)
            && class_missing_props_component_type.is_some_and(|comp| {
                self.should_report_jsx_class_missing_props_via_assignability(comp)
            })
            && self.jsx_has_missing_required_props(props_type, &provided_attrs)
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            self.report_jsx_synthesized_props_assignability_error(
                attrs_type,
                &display_target,
                tag_name_idx,
            );
            true
        } else {
            false
        };

        // TS2322: whole-object assignability for bare type parameter props.
        // When the props type IS a type parameter (e.g., `P` from an outer generic),
        // property-by-property checking can't enumerate the expected shape. Instead, build
        // the attributes object type and check assignability against the props type.
        // tsc emits TS2322 here: "Type '{}' is not assignable to type 'P'."
        // Only applies to bare type parameters, NOT object types that happen to
        // contain type parameters in their properties.
        let props_is_type_param =
            crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, props_type);
        // When a spread attribute's type is the same as or assignable to the props
        // type parameter, the spread already satisfies the type parameter constraint.
        // Checking a synthesized object (which loses the type parameter identity)
        // against the type parameter would produce a false TS2322.
        let spread_satisfies_type_param = props_is_type_param
            && spread_entries
                .iter()
                .any(|&(spread_type, _, _)| self.is_assignable_to(spread_type, props_type));
        let reported_type_param_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_class_missing_props_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !has_prop_type_error
            && props_is_type_param
            && !spread_satisfies_type_param
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            if !self.is_assignable_to(attrs_type, props_type) {
                // tsc uses just the type parameter name here (e.g. "P"), not the
                // full "IntrinsicAttributes & P" display target. The IntrinsicAttributes
                // intersection check for spread attributes is handled separately by
                // check_generic_sfc_spread_intrinsic_attrs.
                let type_param_target = self.format_type(props_type);
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &type_param_target,
                    tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        let reported_dynamic_intrinsic_assignability = if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_class_missing_props_assignability
            && !reported_type_param_assignability
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !has_prop_type_error
            && component_type.is_none()
            && provided_attrs.is_empty()
            && raw_props_has_type_params
        {
            let attrs_type = self.build_jsx_provided_attrs_object_type(&provided_attrs);
            if !self.is_assignable_to(attrs_type, original_props_type) {
                self.report_jsx_synthesized_props_assignability_error(
                    attrs_type,
                    &display_target,
                    tag_name_idx,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        // TS2741: missing required properties.
        if !reported_custom_children_assignability
            && !reported_special_attr_assignability
            && !reported_type_param_assignability
            && !reported_dynamic_intrinsic_assignability
            && (!reported_class_missing_props_assignability
                || (provided_attrs.is_empty() && raw_props_has_type_params))
            && !has_excess_property_error
            && !spread_covers_all
            && !skip_prop_checks
            && !has_prop_type_error
        {
            self.check_missing_required_jsx_props(
                props_type,
                &provided_attrs,
                tag_name_idx,
                Some(tag_name_idx),
                preferred_target_display,
            );
        }

        // Also check required IntrinsicAttributes.
        if !has_excess_property_error
            && !spread_covers_all
            && let Some(intrinsic_attrs_type) = self.get_intrinsic_attributes_type()
        {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &provided_attrs,
                tag_name_idx,
                None,
                None,
            );
        }

        if !has_excess_property_error
            && !spread_covers_all
            && let Some(comp) = special_attr_component_type
            && let Some(intrinsic_class_attrs_type) =
                self.get_intrinsic_class_attributes_type_for_component(comp)
        {
            self.check_missing_required_jsx_props(
                intrinsic_class_attrs_type,
                &provided_attrs,
                tag_name_idx,
                None,
                None,
            );
        }
    }

    fn report_jsx_body_children_excess_property(
        &mut self,
        tag_name_idx: NodeIndex,
        display_target: &str,
        provided_attrs: &[(String, TypeId)],
    ) {
        let mut ordered_attrs: Vec<(String, TypeId)> = Vec::with_capacity(provided_attrs.len());
        if let Some((_, children_type)) = provided_attrs.iter().find(|(name, _)| name == "children")
        {
            ordered_attrs.push(("children".to_string(), *children_type));
        }
        ordered_attrs.extend(
            provided_attrs
                .iter()
                .filter(|(name, _)| name != "children")
                .cloned(),
        );

        let properties: Vec<tsz_solver::PropertyInfo> = ordered_attrs
            .iter()
            .map(|(name, type_id)| {
                let name_atom = self.ctx.types.intern_string(name);
                let display_type = if name == "children" {
                    self.jsx_children_display_type(*type_id)
                } else {
                    *type_id
                };
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: display_type,
                    write_type: display_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                }
            })
            .collect();
        let source_type = self.format_type(self.ctx.types.factory().object(properties));
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_type, display_target],
        );
        let message =
            format!("{base}\n  Property 'children' does not exist on type '{display_target}'.");
        use crate::diagnostics::diagnostic_codes;
        self.error_at_node(
            tag_name_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    pub(crate) fn check_jsx_union_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // Collect provided attribute name→type pairs (excluding key/ref).
        // Skip when any attribute value is a function/arrow expression — these need
        // contextual typing from discriminated union narrowing which we don't implement.
        let attr_nodes = &attrs.properties.nodes;
        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let mut has_spread = false;

        for &attr_idx in attr_nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };

                // Skip key/ref — they come from IntrinsicAttributes, not component props
                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                // Check for function/arrow expressions — bail out for contextual typing
                if attr_data.initializer.is_some() {
                    let value_idx =
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
                    if let Some(value_node) = self.ctx.arena.get(value_idx)
                        && (value_node.kind == syntax_kind_ext::ARROW_FUNCTION
                            || value_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    {
                        return;
                    }
                }

                // Compute the attribute value type with literal preservation.
                // For union props, literals like "a" and true must stay as literal types
                // (not widen to string/boolean) so they can match discriminant properties
                // in the union members. Shorthand booleans stay as BOOLEAN_TRUE for
                // assignability but get widened to BOOLEAN in error message display.
                let attr_type = if attr_data.initializer.is_none() {
                    TypeId::BOOLEAN_TRUE
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
                    let prev = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = true;
                    let t = self.compute_type_of_node(value_idx);
                    self.ctx.preserve_literal_types = prev;
                    t
                } else {
                    TypeId::ANY
                };

                provided_attrs.push((attr_name, attr_type));
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                has_spread = true;
            }
        }

        // Include synthesized children prop if body children exist
        if let Some(children) = children_ctx {
            provided_attrs.push((self.get_jsx_children_prop_name(), children.synthesized_type));
        }

        // Skip union check when spread attributes are involved (handled separately).
        // When no attributes are provided, still proceed so we can detect missing required props.
        if has_spread {
            return;
        }

        // Get union members — bail if not a union
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, props_type)
        else {
            return;
        };

        // For each union member, check:
        // 1. All provided attributes are type-compatible with the member's properties
        // 2. All required properties in the member are provided
        // If at least one member passes both checks, the attributes are valid.
        // Only emit TS2322 when NO member is compatible.
        let provided_names: rustc_hash::FxHashSet<&str> = provided_attrs
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        let mut any_member_compatible = false;
        for &member in &members {
            let member_resolved = self.resolve_type_for_property_access(member);

            // Check 1: All provided attribute values are assignable to member properties
            let all_attrs_compatible = provided_attrs.iter().all(|(name, attr_type)| {
                use crate::query_boundaries::common::PropertyAccessResult;
                match self.resolve_property_access_with_env(member_resolved, name) {
                    PropertyAccessResult::Success { type_id, .. } => {
                        // Strip undefined from optional properties (write-position)
                        let expected = crate::query_boundaries::common::remove_undefined(
                            self.ctx.types,
                            type_id,
                        );
                        // any/error types are always compatible
                        if *attr_type == TypeId::ANY || *attr_type == TypeId::ERROR {
                            return true;
                        }
                        self.is_assignable_to(*attr_type, expected)
                    }
                    // PropertyNotFound or other results: still compatible
                    // (excess property checking is separate)
                    _ => true,
                }
            });

            if !all_attrs_compatible {
                continue;
            }

            // Check 2: All required properties in the member are provided.
            // Children are now included in provided_names via synthesis above.
            let all_required_present = if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    member_resolved,
                ) {
                shape.properties.iter().all(|prop| {
                    if prop.optional {
                        return true;
                    }
                    let prop_name = self.ctx.types.resolve_atom(prop.name);
                    provided_names.contains(prop_name.as_str())
                })
            } else {
                // Can't determine shape — assume compatible
                true
            };

            if all_required_present {
                any_member_compatible = true;
                break;
            }
        }

        if !any_member_compatible {
            // When any provided attribute type contains unresolved type parameters,
            // skip the TS2322 error. Type parameters can't be properly checked
            // against individual union members at this point — they'll be validated
            // when the generic function is instantiated. This prevents false TS2322
            // for cases like `<ListItem variant={v} />` inside a generic function
            // where `v: MenuItemVariant` and `MenuItemVariant extends ListItemVariant`.
            // The per-member check fails because the type parameter doesn't match any
            // single member, but the constraint ensures correctness at instantiation.
            let any_attr_has_type_params = provided_attrs.iter().any(|(_, attr_type)| {
                crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    *attr_type,
                )
            });

            if !any_attr_has_type_params {
                // Build the attributes object type for the error message.
                // tsc widens shorthand boolean `true` to `boolean` in the JSX attribute
                // object type displayed in error messages (fresh object literal widening).
                let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
                    .iter()
                    .map(|(name, type_id)| {
                        let name_atom = self.ctx.types.intern_string(name);
                        // Widen BOOLEAN_TRUE → BOOLEAN for error display
                        let display_type = if *type_id == TypeId::BOOLEAN_TRUE {
                            TypeId::BOOLEAN
                        } else if name == "children" {
                            self.jsx_children_display_type(*type_id)
                        } else {
                            *type_id
                        };
                        tsz_solver::PropertyInfo {
                            name: name_atom,
                            type_id: display_type,
                            write_type: display_type,
                            optional: false,
                            readonly: false,
                            is_method: false,
                            is_class_prototype: false,
                            visibility: tsz_solver::Visibility::Public,
                            parent_id: None,
                            declaration_order: 0,
                            is_string_named: false,
                        }
                    })
                    .collect();
                let attrs_type = self.ctx.types.factory().object(properties);
                // tsc anchors JSX union props errors at the tag name (e.g., <TextComponent>),
                // not the attributes container.
                self.check_assignable_or_report_at(
                    attrs_type,
                    props_type,
                    tag_name_idx,
                    tag_name_idx,
                );
            }
        }
    }
}
