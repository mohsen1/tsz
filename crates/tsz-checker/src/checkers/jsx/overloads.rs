//! JSX overload resolution for overloaded Stateless Function Components.
//!
//! When a component has multiple call signatures (generic or non-generic),
//! tries each overload against the provided JSX attributes. If no overload
//! matches, emits TS2769 ("No overload matches this call.").

use crate::context::speculation::DiagnosticSpeculationSnapshot;
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
    /// Source node for explicit attribute names; absent for spread-sourced/synthesized attrs.
    name_node_idx: Option<NodeIndex>,
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
    /// When a component has multiple call signatures (generic or non-generic),
    /// tries each overload against the provided JSX attributes. If no overload
    /// matches, emits TS2769 ("No overload matches this call.").
    ///
    /// JSX overloads differ from regular function overloads: instead of positional
    /// arguments, the "call" is a single attributes object checked with excess
    /// property checking (like a fresh object literal).
    ///
    /// Generic overloads are instantiated with constraint/default substitutions
    /// before checking, matching tsc's behavior of attempting inference for each
    /// candidate signature.
    pub(crate) fn check_jsx_overloaded_sfc(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
        children_ctx: Option<crate::checkers_domain::JsxChildrenContext>,
    ) {
        // Try call signatures first (SFC overloads), then construct signatures
        // (class component overloads like React.Component with 2 constructors).
        let sigs = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        )
        .filter(|s| s.len() >= 2)
        .or_else(|| {
            crate::query_boundaries::common::construct_signatures_for_type(
                self.ctx.types,
                component_type,
            )
            .filter(|s| s.len() >= 2)
        });
        let Some(sigs) = sigs else {
            return;
        };

        // Speculative attribute collection: save diagnostic checkpoint so side-effect
        // diagnostics (e.g. TS7006 from callback params without contextual typing) are
        // rolled back. Only the final TS2769 (if no overload matches) is kept.
        // (TS2698 spread validity is emitted earlier by the JSX orchestration entry,
        // so it survives this rollback even when no overload matches.)
        let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);

        // Collect JSX attributes: explicit + spread-merged, with override tracking
        let mut attrs_info = self.collect_jsx_provided_attrs(attributes_idx);

        // Include synthesized children from JSX element body
        let children_prop_name = self.get_jsx_children_prop_name();
        if let Some(children) = children_ctx {
            attrs_info.attrs.push(JsxAttrInfo {
                name: children_prop_name,
                type_id: children.synthesized_type,
                from_spread: false,
                name_node_idx: None,
            });
        }

        // For class components with `static defaultProps`, treat the keys of
        // defaultProps as already-provided so required-prop checks don't reject
        // overloads where the value would be supplied by the class default.
        // This mirrors tsc's `LibraryManagedAttributes` relaxation.
        let default_props_keys = self.collect_jsx_default_props_keys(component_type);

        // Try each overload
        let has_any_attrs = !attrs_info.attrs.is_empty() || attrs_info.has_spread;
        let mut shared_explicit_anchor_name: Option<String> = None;
        let mut all_overload_failures_share_explicit_anchor = true;
        let mut considered_overload_failures: usize = 0;

        // When an `any`-typed spread exists, any non-0-param overload matches.
        // The `any` spread dominates the merged type, making it `any`.
        // Skip detailed attribute checking (it would false-positive on explicit attrs).
        if attrs_info.has_any_spread {
            let has_non_zero_param = sigs.iter().any(|s| !s.params.is_empty());
            if has_non_zero_param {
                snap.rollback(&mut self.ctx);
                return;
            }
        }

        for sig in &sigs {
            let instantiated_return = if !sig.type_params.is_empty() {
                self.instantiate_type_with_constraints(sig.return_type, &sig.type_params)
            } else {
                sig.return_type
            };

            // For 0-param overloads: only match when NO attributes are provided.
            // tsc treats JSX as a 1-arg call (the attributes object), so 0-param
            // overloads fail on arg count when any attributes exist.
            if sig.params.is_empty() {
                if !has_any_attrs {
                    snap.rollback(&mut self.ctx);
                    self.check_jsx_sfc_return_type(instantiated_return, tag_name_idx);
                    return;
                }
                continue;
            }

            let props_type = sig.params[0].type_id;

            // For generic signatures, instantiate with constraint/default substitutions
            // so that type parameters are resolved to concrete types for matching.
            // Unconstrained type params are substituted with `any`, which makes type
            // compatibility checks pass while still catching structural issues
            // (missing required properties, excess properties).
            let props_type = if !sig.type_params.is_empty() {
                self.instantiate_type_with_constraints(props_type, &sig.type_params)
            } else {
                props_type
            };
            // Evaluate the props type, including generic applications like
            // `Readonly<Props>`. If evaluation degrades to `unknown`, fall back to
            // the richer pre-resolution type to avoid false overload matches.
            let application_evaluated = self.evaluate_application_type(props_type);
            let evaluated = self.evaluate_type_with_env(application_evaluated);
            let evaluated =
                if evaluated == TypeId::UNKNOWN && application_evaluated != TypeId::UNKNOWN {
                    application_evaluated
                } else {
                    evaluated
                };
            let resolved = self.resolve_type_for_property_access(evaluated);
            // If resolution still produces UNKNOWN (e.g. type not yet resolved),
            // keep the evaluated type which preserves more structural information.
            let props_resolved = if resolved == TypeId::UNKNOWN {
                evaluated
            } else {
                resolved
            };

            if self.jsx_attrs_match_overload(&attrs_info, props_resolved, &default_props_keys) {
                // Found a matching overload — done.
                // Roll back speculative diagnostics from attribute collection.
                snap.rollback(&mut self.ctx);
                self.check_jsx_sfc_return_type(instantiated_return, tag_name_idx);
                return;
            }

            considered_overload_failures += 1;
            if let Some(overload_anchor_name) =
                self.jsx_overload_explicit_failure_attr(&attrs_info, props_resolved)
            {
                if let Some(shared_name) = shared_explicit_anchor_name.as_deref() {
                    if shared_name != overload_anchor_name.as_str() {
                        all_overload_failures_share_explicit_anchor = false;
                    }
                } else {
                    shared_explicit_anchor_name = Some(overload_anchor_name);
                }
            } else {
                all_overload_failures_share_explicit_anchor = false;
            }
        }

        // No overload matched — roll back speculative diagnostics and emit TS2769.
        // tsc often anchors at the tag name, but when every non-0-param overload
        // fails on the same explicit attribute, anchor that attribute instead.
        snap.rollback(&mut self.ctx);
        let anchor_idx =
            if considered_overload_failures > 0 && all_overload_failures_share_explicit_anchor {
                shared_explicit_anchor_name
                    .as_deref()
                    .and_then(|shared_name| {
                        attrs_info
                            .attrs
                            .iter()
                            .find(|a| {
                                !a.from_spread
                                    && a.name_node_idx.is_some()
                                    && a.name.as_str() == shared_name
                            })
                            .and_then(|a| a.name_node_idx)
                    })
                    .unwrap_or(tag_name_idx)
            } else {
                tag_name_idx
            };

        // TS2786: When no overload matches, also check if the component's return
        // type is compatible with JSX.Element. tsc emits TS2786 alongside TS2769
        // when none of the overloads return a valid JSX element type.
        self.check_jsx_component_return_type(component_type, tag_name_idx);

        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            anchor_idx,
            diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL,
            diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
        );
    }

    /// Instantiate a props type by substituting type parameters with their
    /// constraints, defaults, or `any` (for unconstrained params).
    ///
    /// Using `any` for unconstrained type parameters is conservative: it means
    /// type compatibility checks will pass (any is assignable to anything), but
    /// structural checks (required properties, excess properties) still work
    /// correctly because property names don't depend on type arguments.
    fn instantiate_type_with_constraints(
        &mut self,
        type_id: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> TypeId {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        let type_args: Vec<TypeId> = type_params
            .iter()
            .map(|param| param.default.or(param.constraint).unwrap_or(TypeId::ANY))
            .collect();
        let substitution = TypeSubstitution::from_args(self.ctx.types, type_params, &type_args);
        instantiate_type(self.ctx.types, type_id, &substitution)
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
                    // Prefer the literal type for syntactic literal values
                    // (`attr="text"`, `attr={42}`, `attr={true}`). Without
                    // literal preservation, per-overload assignability
                    // walks `compute_type_of_node`'s widened result and
                    // produces false-positive failure-attr matches that
                    // skew the shared-anchor heuristic in
                    // `jsx_overload_explicit_failure_attr` — see
                    // `contextuallyTypedStringLiteralsInJsxAttributes02.tsx`
                    // (b4 case).
                    self.literal_type_from_initializer(value_idx)
                        .unwrap_or_else(|| self.compute_type_of_node(value_idx))
                } else {
                    TypeId::ANY
                };

                // Override any earlier attr with the same name
                if let Some(existing) = attr_map.iter_mut().find(|a| a.name == attr_name) {
                    existing.type_id = attr_type;
                    existing.from_spread = false;
                    existing.name_node_idx = Some(attr_data.name);
                } else {
                    attr_map.push(JsxAttrInfo {
                        name: attr_name,
                        type_id: attr_type,
                        from_spread: false,
                        name_node_idx: Some(attr_data.name),
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
                    crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved)
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
                            existing.name_node_idx = None;
                        } else {
                            attr_map.push(JsxAttrInfo {
                                name,
                                type_id: prop.type_id,
                                from_spread: true,
                                name_node_idx: None,
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
    fn jsx_attrs_match_overload(
        &mut self,
        info: &JsxAttrsInfo,
        props_type: TypeId,
        default_props_keys: &rustc_hash::FxHashSet<String>,
    ) -> bool {
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return true;
        }

        // When an `any`-typed spread exists, the merged attributes type is effectively
        // `any & {...explicitAttrs}` which simplifies to `any`. Since `any` is assignable
        // to any type, this overload automatically matches.
        if info.has_any_spread {
            return true;
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type)
        else {
            // Can't resolve shape — use assignability fallback.
            // Always check assignability even with empty attrs: an empty object `{}`
            // is not assignable to type parameters like `P`, so we can't just assume
            // empty attrs match when the shape can't be resolved.
            let attrs_type = self.build_attrs_object_type_from_info(&info.attrs);
            return self.is_assignable_to(attrs_type, props_type);
        };

        let has_string_index = shape.string_index.is_some();
        let provided_names: rustc_hash::FxHashSet<&str> =
            info.attrs.iter().map(|a| a.name.as_str()).collect();

        // Check 1: All required props must be provided.
        // Children are now included in provided_names via synthesis above.
        // Props supplied by `static defaultProps` are treated as provided too,
        // matching tsc's `LibraryManagedAttributes` relaxation.
        if !info.has_any_spread {
            for prop in &shape.properties {
                if prop.optional {
                    continue;
                }
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if default_props_keys.contains(prop_name.as_str()) {
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
        // Synthesized attrs (no source name token, e.g. `children` from JSX body) are
        // also exempt: they aren't user-written attributes, and class components'
        // constructor props type doesn't include the children injected by JSX.
        if !has_string_index {
            for attr in &info.attrs {
                if attr.from_spread {
                    continue; // Spreads are exempt from excess checking
                }
                if attr.name_node_idx.is_none() {
                    continue; // Synthesized attrs (e.g. JSX children) exempt
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
            use crate::query_boundaries::common::PropertyAccessResult;
            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(props_type, &attr.name)
            {
                let expected =
                    crate::query_boundaries::common::remove_undefined(self.ctx.types, type_id);
                if !self.is_assignable_to(attr.type_id, expected) {
                    return false;
                }
            }
        }

        true
    }

    /// Collect the set of property names declared in the component's
    /// `static defaultProps` (if any). Used to relax required-prop checks
    /// during overload resolution: a prop with a default value should not
    /// fail an overload just because the JSX call doesn't provide it.
    fn collect_jsx_default_props_keys(
        &mut self,
        component_type: TypeId,
    ) -> rustc_hash::FxHashSet<String> {
        use crate::query_boundaries::common::PropertyAccessResult;
        let mut keys: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        let dp_type = match self.resolve_property_access_with_env(component_type, "defaultProps") {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => type_id,
            _ => return keys,
        };
        let evaluated = self.evaluate_application_type(dp_type);
        let evaluated = self.evaluate_type_with_env(evaluated);
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
        {
            for prop in &shape.properties {
                let name = self.ctx.types.resolve_atom(prop.name);
                keys.insert(name.to_string());
            }
        }
        keys
    }

    /// Returns the explicit attribute name that best explains an overload mismatch.
    ///
    /// We prefer explicit attribute failures (type mismatch or excess property) and
    /// ignore spread-sourced/synthesized attrs. Missing-required-property failures
    /// do not produce an explicit anchor candidate.
    fn jsx_overload_explicit_failure_attr(
        &mut self,
        info: &JsxAttrsInfo,
        props_type: TypeId,
    ) -> Option<String> {
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return None;
        }

        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type)?;
        let has_string_index = shape.string_index.is_some();

        for attr in &info.attrs {
            if attr.from_spread || attr.name_node_idx.is_none() || attr.name.contains('-') {
                continue;
            }

            use crate::query_boundaries::common::PropertyAccessResult;
            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(props_type, &attr.name)
            {
                if attr.type_id == TypeId::ANY || attr.type_id == TypeId::ERROR {
                    continue;
                }
                let expected =
                    crate::query_boundaries::common::remove_undefined(self.ctx.types, type_id);
                if !self.is_assignable_to(attr.type_id, expected) {
                    return Some(attr.name.clone());
                }
                continue;
            }

            if !has_string_index {
                return Some(attr.name.clone());
            }
        }

        None
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
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    single_quoted_name: false,
                }
            })
            .collect();
        self.ctx.types.factory().object(properties)
    }
}
