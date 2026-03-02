//! JSX attribute compatibility checks (union props, spread property types, overwrite detection,
//! overload resolution).
//!
//! Extracted from `jsx_checker.rs` to keep the main file under 2000 LOC.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// A JSX attribute with its source information for overload matching.
struct JsxAttrInfo {
    name: String,
    type_id: TypeId,
    /// Whether this attribute came from a spread (`{...obj}`) vs explicit (`name={val}`)
    from_spread: bool,
}

/// Collected JSX attribute information for overload matching.
struct JsxAttrsInfo {
    /// All attributes (explicit + spread-merged), last-wins for duplicates
    attrs: Vec<JsxAttrInfo>,
    /// Whether any spread attribute exists
    has_spread: bool,
    /// Whether any spread is `any`-typed (covers all props)
    has_any_spread: bool,
}

impl<'a> CheckerState<'a> {
    /// JSX overload resolution for overloaded Stateless Function Components.
    ///
    /// When a component has multiple non-generic call signatures, tries each
    /// overload against the provided JSX attributes. If no overload matches,
    /// emits TS2769 ("No overload matches this call.").
    ///
    /// JSX overloads differ from regular function overloads: instead of positional
    /// arguments, the "call" is a single attributes object checked with excess
    /// property checking (like a fresh object literal).
    pub(crate) fn check_jsx_overloaded_sfc(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
        else {
            return;
        };

        // Collect non-generic call signatures
        let non_generic: Vec<_> = sigs.iter().filter(|s| s.type_params.is_empty()).collect();
        if non_generic.len() < 2 {
            return;
        }

        // Speculative attribute collection: save diagnostic checkpoint so side-effect
        // diagnostics (e.g. TS7006 from callback params without contextual typing) are
        // rolled back. Only the final TS2769 (if no overload matches) is kept.
        let diag_checkpoint = self.ctx.diagnostics.len();

        // Collect JSX attributes: explicit + spread-merged, with override tracking
        let attrs_info = self.collect_jsx_provided_attrs(attributes_idx);

        // Try each overload
        let has_any_attrs = !attrs_info.attrs.is_empty() || attrs_info.has_spread;

        // When an `any`-typed spread exists, any non-0-param overload matches.
        // The `any` spread dominates the merged type, making it `any`.
        // Skip detailed attribute checking (it would false-positive on explicit attrs).
        if attrs_info.has_any_spread {
            let has_non_zero_param = non_generic.iter().any(|s| !s.params.is_empty());
            if has_non_zero_param {
                self.ctx.diagnostics.truncate(diag_checkpoint);
                return;
            }
        }

        for sig in &non_generic {
            // For 0-param overloads: only match when NO attributes are provided.
            // tsc treats JSX as a 1-arg call (the attributes object), so 0-param
            // overloads fail on arg count when any attributes exist.
            if sig.params.is_empty() {
                if !has_any_attrs {
                    self.ctx.diagnostics.truncate(diag_checkpoint);
                    return;
                }
                continue;
            }

            let props_type = sig.params[0].type_id;
            let evaluated = self.evaluate_type_with_env(props_type);
            let props_resolved = self.resolve_type_for_property_access(evaluated);

            if self.jsx_attrs_match_overload(&attrs_info, props_resolved) {
                // Found a matching overload — done.
                // Roll back speculative diagnostics from attribute collection.
                self.ctx.diagnostics.truncate(diag_checkpoint);
                return;
            }
        }

        // No overload matched — roll back speculative diagnostics and emit TS2769.
        // tsc anchors JSX TS2769 at the tag name.
        self.ctx.diagnostics.truncate(diag_checkpoint);
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            tag_name_idx,
            diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL,
            diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
        );
    }

    /// Collect provided JSX attributes as `JsxAttrsInfo`.
    ///
    /// Merges explicit and spread attributes. Later attributes override earlier
    /// ones with the same name (matching tsc's JSX override semantics).
    fn collect_jsx_provided_attrs(&mut self, attributes_idx: NodeIndex) -> JsxAttrsInfo {
        let empty = JsxAttrsInfo {
            attrs: Vec::new(),
            has_spread: false,
            has_any_spread: false,
        };
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return empty;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return empty;
        };

        // Use a map to handle overrides: later attrs with the same name replace earlier ones
        let mut attr_map: Vec<JsxAttrInfo> = Vec::new();
        let mut has_spread = false;
        let mut has_any_spread = false;

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
                    self.compute_type_of_node(value_idx)
                } else {
                    TypeId::ANY
                };

                // Override any earlier attr with the same name
                if let Some(existing) = attr_map.iter_mut().find(|a| a.name == attr_name) {
                    existing.type_id = attr_type;
                    existing.from_spread = false;
                } else {
                    attr_map.push(JsxAttrInfo {
                        name: attr_name,
                        type_id: attr_type,
                        from_spread: false,
                    });
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                has_spread = true;
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_type = self.compute_type_of_node(spread_data.expression);
                if spread_type == TypeId::ANY {
                    has_any_spread = true;
                }
                let spread_evaluated = self.evaluate_type_with_env(spread_type);
                let resolved = self.resolve_type_for_property_access(spread_evaluated);
                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved)
                {
                    for prop in &shape.properties {
                        let name = self.ctx.types.resolve_atom(prop.name).to_string();
                        if name == "key" || name == "ref" {
                            continue;
                        }
                        // Override earlier attrs with the same name from this spread
                        if let Some(existing) = attr_map.iter_mut().find(|a| a.name == name) {
                            existing.type_id = prop.type_id;
                            existing.from_spread = true;
                        } else {
                            attr_map.push(JsxAttrInfo {
                                name,
                                type_id: prop.type_id,
                                from_spread: true,
                            });
                        }
                    }
                }
            }
        }

        JsxAttrsInfo {
            attrs: attr_map,
            has_spread,
            has_any_spread,
        }
    }

    /// Check whether JSX attributes match a specific overload's props type.
    ///
    /// Performs three checks (like tsc's `checkApplicableSignatureForJsxOpeningLikeElement`):
    /// 1. All required props in the overload must be provided
    /// 2. No excess properties from EXPLICIT attributes (spread props are exempt)
    /// 3. Provided attribute types must be assignable to expected prop types
    fn jsx_attrs_match_overload(&mut self, info: &JsxAttrsInfo, props_type: TypeId) -> bool {
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return true;
        }

        // When an `any`-typed spread exists, the merged attributes type is effectively
        // `any & {...explicitAttrs}` which simplifies to `any`. Since `any` is assignable
        // to any type, this overload automatically matches.
        if info.has_any_spread {
            return true;
        }

        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        else {
            // Can't resolve shape — use assignability fallback
            if info.attrs.is_empty() && !info.has_spread {
                return true;
            }
            let attrs_type = self.build_attrs_object_type_from_info(&info.attrs);
            return self.is_assignable_to(attrs_type, props_type);
        };

        let has_string_index = shape.string_index.is_some();
        let provided_names: rustc_hash::FxHashSet<&str> =
            info.attrs.iter().map(|a| a.name.as_str()).collect();

        // Check 1: All required props must be provided
        if !info.has_any_spread {
            for prop in &shape.properties {
                if prop.optional {
                    continue;
                }
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if prop_name == "children" {
                    continue;
                }
                if !provided_names.contains(prop_name.as_str()) {
                    return false;
                }
            }
        }

        // Check 2: Excess property check — only for EXPLICIT (non-spread) attributes.
        // tsc does not do excess property checking for spread-sourced attributes;
        // when all attrs come from spreads, no excess check occurs.
        // Hyphenated attribute names (e.g., `extra-prop`) are also exempt — in JSX,
        // they are only checked against string index signatures, not named properties.
        if !has_string_index {
            for attr in &info.attrs {
                if attr.from_spread {
                    continue; // Spreads are exempt from excess checking
                }
                if attr.name.contains('-') {
                    continue; // Hyphenated attrs exempt from excess checking
                }
                let attr_atom = self.ctx.types.intern_string(&attr.name);
                let exists = shape.properties.iter().any(|p| p.name == attr_atom);
                if !exists {
                    return false;
                }
            }
        }

        // Check 3: Type compatibility for all provided attributes
        for attr in &info.attrs {
            if attr.type_id == TypeId::ANY || attr.type_id == TypeId::ERROR {
                continue;
            }
            use tsz_solver::operations::property::PropertyAccessResult;
            if let PropertyAccessResult::Success { type_id, .. } = self.resolve_property_access_with_env(props_type, &attr.name) {
                let expected = tsz_solver::remove_undefined(self.ctx.types, type_id);
                if !self.is_assignable_to(attr.type_id, expected) {
                    return false;
                }
            }
        }

        true
    }

    /// Build an object type from collected JSX attribute info.
    fn build_attrs_object_type_from_info(&mut self, attrs: &[JsxAttrInfo]) -> TypeId {
        let properties: Vec<tsz_solver::PropertyInfo> = attrs
            .iter()
            .map(|attr| {
                let name_atom = self.ctx.types.intern_string(&attr.name);
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: attr.type_id,
                    write_type: attr.type_id,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                }
            })
            .collect();
        self.ctx.types.factory().object(properties)
    }

    /// Check JSX attributes against union-typed props via whole-object assignability.
    ///
    /// When the component's props type is a union (e.g., discriminated unions like
    /// `{ editable: false } | { editable: true, onEdit: ... }`), we can't do per-property
    /// checking because `get_object_shape` doesn't work on unions. Instead, we build
    /// an object type from the provided JSX attributes and check the whole object
    /// against the union props type, letting the solver handle discriminated union logic.
    ///
    /// This matches tsc's behavior of constructing an "attributes type" object literal
    /// and checking assignability against the full props type.
    pub(crate) fn check_jsx_union_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
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
                // in the union members.
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

        // Skip union check when there are no concrete attributes to check,
        // or when spread attributes are involved (handled separately).
        if provided_attrs.is_empty() || has_spread {
            return;
        }

        // Get union members — bail if not a union
        let Some(members) = tsz_solver::type_queries::get_union_members(self.ctx.types, props_type)
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
                use tsz_solver::operations::property::PropertyAccessResult;
                match self.resolve_property_access_with_env(member_resolved, name) {
                    PropertyAccessResult::Success { type_id, .. } => {
                        // Strip undefined from optional properties (write-position)
                        let expected = tsz_solver::remove_undefined(self.ctx.types, type_id);
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
            // Skip `children` (synthesized from JSX element body, not checked here).
            let all_required_present = if let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, member_resolved)
            {
                shape.properties.iter().all(|prop| {
                    if prop.optional {
                        return true;
                    }
                    let prop_name = self.ctx.types.resolve_atom(prop.name);
                    if prop_name == "children" {
                        return true;
                    }
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
            // Build the attributes object type for the error message
            let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
                .iter()
                .map(|(name, type_id)| {
                    let name_atom = self.ctx.types.intern_string(name);
                    tsz_solver::PropertyInfo {
                        name: name_atom,
                        type_id: *type_id,
                        write_type: *type_id,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        visibility: tsz_solver::Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                    }
                })
                .collect();
            let attrs_type = self.ctx.types.factory().object(properties);
            // tsc anchors JSX union props errors at the tag name (e.g., <TextComponent>),
            // not the attributes container.
            self.check_assignable_or_report_at(attrs_type, props_type, tag_name_idx, tag_name_idx);
        }
    }

    /// TS2322: Check that spread attribute property types are compatible with props.
    ///
    /// tsc checks if the spread type is assignable to the expected props type and
    /// emits TS2322 with "Type '{`spread_type`}' is not assignable to type '{`props_type`}'"
    /// when a property type mismatch is found. Missing properties are handled
    /// separately by TS2741, not TS2322.
    ///
    /// Properties overridden by explicit attributes (either before or after the spread)
    /// are excluded from the check.
    ///
    /// tsc anchors these errors at the JSX opening tag (not the spread expression).
    pub(crate) fn check_spread_property_types(
        &mut self,
        spread_type: TypeId,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        overridden_names: &rustc_hash::FxHashSet<&str>,
    ) {
        use tsz_solver::operations::property::PropertyAccessResult;

        // Safety guard: skip when types involve unresolved generics or errors
        if tsz_solver::contains_type_parameters(self.ctx.types, spread_type)
            || tsz_solver::contains_error_type(self.ctx.types, spread_type)
        {
            return;
        }

        // If the whole spread type is assignable to props, no error needed.
        // This is the fast path and also prevents false positives from imprecise
        // per-property extraction (e.g., mapped/conditional/utility types).
        if self.is_assignable_to(spread_type, props_type) {
            return;
        }

        // Resolve the spread type to extract its properties
        let resolved_spread = self.evaluate_type_with_env(spread_type);
        let resolved_spread = self.resolve_type_for_property_access(resolved_spread);

        let Some(spread_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_spread)
        else {
            // If spread type has no object shape (e.g., type parameter), emit
            // whole-type TS2322: "Type 'U' is not assignable to type 'IntrinsicAttributes & U'".
            let spread_name = self.format_type(spread_type);
            let props_name = self.format_type(props_type);
            let message = format!("Type '{spread_name}' is not assignable to type '{props_name}'.");
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                tag_name_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            return;
        };

        // tsc suppresses TS2322 for per-property type mismatches in spreads when
        // the spread also has missing required properties from the target. In that case,
        // TS2741 (missing required property) is emitted instead, and tsc doesn't pile on
        // with TS2322 for the type mismatches. Check if any required props are missing
        // from the spread + explicit attributes.
        if let Some(props_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        {
            let spread_prop_names: rustc_hash::FxHashSet<String> = spread_shape
                .properties
                .iter()
                .map(|p| self.ctx.types.resolve_atom(p.name))
                .collect();
            for req_prop in &props_shape.properties {
                if req_prop.optional {
                    continue;
                }
                let req_name = self.ctx.types.resolve_atom(req_prop.name).to_string();
                if req_name == "children" || req_name == "key" || req_name == "ref" {
                    continue;
                }
                if !spread_prop_names.contains(&req_name)
                    && !overridden_names.contains(req_name.as_str())
                {
                    // Missing required property → TS2741 will fire, suppress TS2322
                    return;
                }
            }
        }

        // Check if the mismatch is a TYPE mismatch (not just missing properties).
        // tsc only emits TS2322 for spread type mismatches, not for missing properties
        // (those are handled by TS2741).
        let mut has_type_mismatch = false;
        for prop in &spread_shape.properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name).to_string();

            // Skip properties overridden by explicit attributes
            if overridden_names.contains(prop_name.as_str()) {
                continue;
            }

            // Skip key/ref — same as other JSX attribute handling
            if prop_name == "key" || prop_name == "ref" {
                continue;
            }

            // Look up the expected type for this property in the props type
            let expected_type = match self.resolve_property_access_with_env(props_type, &prop_name)
            {
                PropertyAccessResult::Success { type_id, .. } => {
                    tsz_solver::remove_undefined(self.ctx.types, type_id)
                }
                _ => continue,
            };

            // Check if the spread property type is assignable to the expected type
            if !self.is_assignable_to(prop.type_id, expected_type) {
                has_type_mismatch = true;
                break;
            }
        }

        // Emit a single TS2322 with whole-type message matching tsc's format:
        // "Type '{ x: number; }' is not assignable to type 'Attribs1'."
        if has_type_mismatch {
            let spread_name = self.format_type(spread_type);
            let props_name = self.format_type(props_type);
            let message = format!("Type '{spread_name}' is not assignable to type '{props_name}'.");
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                tag_name_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
    }

    /// TS2783: Check if a later spread attribute will overwrite the current attribute.
    ///
    /// In JSX, `<Foo a={1} {...props}>` — if `props` has a required property `a`,
    /// the spread overwrites the explicit `a={1}`. TSC warns with TS2783:
    /// "'a' is specified more than once, so this usage will be overwritten."
    ///
    /// Only emitted under `strictNullChecks` (matching tsc behavior) and only for
    /// non-optional spread properties (optional properties may not overwrite).
    /// Returns `true` if the attribute is overwritten by a later spread (and
    /// optionally emits TS2783 when `strictNullChecks` is enabled).
    pub(crate) fn check_jsx_attr_overwritten_by_spread(
        &mut self,
        attr_name: &str,
        attr_name_idx: NodeIndex,
        attr_nodes: &[NodeIndex],
        current_idx: usize,
    ) -> bool {
        // Look at later siblings for spreads that contain this property
        for &later_idx in &attr_nodes[current_idx + 1..] {
            let Some(later_node) = self.ctx.arena.get(later_idx) else {
                continue;
            };
            if later_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(later_node) else {
                    continue;
                };
                let spread_type = self.compute_type_of_node(spread_data.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);

                // Skip any/error/unknown — they might cover everything but we
                // can't tell which specific properties they contain.
                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    continue;
                }

                // Check if the spread type has a non-optional property with this name
                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    let attr_atom = self.ctx.types.intern_string(attr_name);
                    let has_required_prop = shape
                        .properties
                        .iter()
                        .any(|p| p.name == attr_atom && !p.optional);
                    if has_required_prop {
                        // TS2783: only emitted under strictNullChecks (matching tsc)
                        if self.ctx.strict_null_checks() {
                            use tsz_common::diagnostics::{
                                diagnostic_codes, diagnostic_messages, format_message,
                            };
                            let message = format_message(
                                diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                &[attr_name],
                            );
                            self.error_at_node(
                                attr_name_idx,
                                &message,
                                diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                            );
                        }
                        // Attribute is overwritten regardless of SNC
                        return true;
                    }
                }
            }
        }
        false
    }
}
