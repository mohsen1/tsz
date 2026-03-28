//! JSX overload resolution for overloaded Stateless Function Components.
//!
//! When a component has multiple call signatures (generic or non-generic),
//! tries each overload against the provided JSX attributes. If no overload
//! matches, emits TS2769 ("No overload matches this call.").

use crate::context::speculation::DiagnosticSpeculationGuard;
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
        let Some(sigs) =
            tsz_solver::type_queries::get_call_signatures(self.ctx.types, component_type)
        else {
            return;
        };

        if sigs.len() < 2 {
            return;
        }

        // Speculative attribute collection: save diagnostic checkpoint so side-effect
        // diagnostics (e.g. TS7006 from callback params without contextual typing) are
        // rolled back. Only the final TS2769 (if no overload matches) is kept.
        let guard = DiagnosticSpeculationGuard::new(&self.ctx);

        // Collect JSX attributes: explicit + spread-merged, with override tracking
        let mut attrs_info = self.collect_jsx_provided_attrs(attributes_idx);

        // Include synthesized children from JSX element body
        let children_prop_name = self.get_jsx_children_prop_name();
        if let Some(children) = children_ctx {
            attrs_info.attrs.push(JsxAttrInfo {
                name: children_prop_name,
                type_id: children.synthesized_type,
                from_spread: false,
            });
        }

        // Try each overload
        let has_any_attrs = !attrs_info.attrs.is_empty() || attrs_info.has_spread;

        // When an `any`-typed spread exists, any non-0-param overload matches.
        // The `any` spread dominates the merged type, making it `any`.
        // Skip detailed attribute checking (it would false-positive on explicit attrs).
        if attrs_info.has_any_spread {
            let has_non_zero_param = sigs.iter().any(|s| !s.params.is_empty());
            if has_non_zero_param {
                guard.rollback(&mut self.ctx);
                return;
            }
        }

        for sig in &sigs {
            // For 0-param overloads: only match when NO attributes are provided.
            // tsc treats JSX as a 1-arg call (the attributes object), so 0-param
            // overloads fail on arg count when any attributes exist.
            if sig.params.is_empty() {
                if !has_any_attrs {
                    guard.rollback(&mut self.ctx);
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
                self.instantiate_props_with_constraints(props_type, &sig.type_params)
            } else {
                props_type
            };

            let evaluated = self.evaluate_type_with_env(props_type);
            let props_resolved = self.resolve_type_for_property_access(evaluated);

            if self.jsx_attrs_match_overload(&attrs_info, props_resolved) {
                // Found a matching overload — done.
                // Roll back speculative diagnostics from attribute collection.
                guard.rollback(&mut self.ctx);
                return;
            }
        }

        // No overload matched — roll back speculative diagnostics and emit TS2769.
        // tsc anchors JSX TS2769 at the tag name.
        guard.rollback(&mut self.ctx);
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            tag_name_idx,
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
    fn instantiate_props_with_constraints(
        &mut self,
        props_type: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> TypeId {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        let type_args: Vec<TypeId> = type_params
            .iter()
            .map(|param| param.default.or(param.constraint).unwrap_or(TypeId::ANY))
            .collect();
        let substitution = TypeSubstitution::from_args(self.ctx.types, type_params, &type_args);
        instantiate_type(self.ctx.types, props_type, &substitution)
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

        // Check 1: All required props must be provided.
        // Children are now included in provided_names via synthesis above.
        if !info.has_any_spread {
            for prop in &shape.properties {
                if prop.optional {
                    continue;
                }
                let prop_name = self.ctx.types.resolve_atom(prop.name);
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
            use crate::query_boundaries::common::PropertyAccessResult;
            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(props_type, &attr.name)
            {
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
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                }
            })
            .collect();
        self.ctx.types.factory().object(properties)
    }
}
