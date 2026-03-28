use crate::context::TypingRequest;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{PropertyInfo, TypeId};

impl<'a> CheckerState<'a> {
    const fn implicit_any_like_diagnostic_code(code: u32) -> bool {
        matches!(
            code,
            crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                | crate::diagnostics::diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                | crate::diagnostics::diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                | crate::diagnostics::diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
        )
    }

    pub(crate) fn function_like_param_spans_for_node(&self, idx: NodeIndex) -> Vec<(u32, u32)> {
        let Some(node) = self.ctx.arena.get(idx) else {
            return Vec::new();
        };

        let params = if let Some(func) = self.ctx.arena.get_function(node) {
            Some(func.parameters.nodes.as_slice())
        } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
            Some(method.parameters.nodes.as_slice())
        } else {
            self.ctx
                .arena
                .get_accessor(node)
                .map(|accessor| accessor.parameters.nodes.as_slice())
        };

        params
            .into_iter()
            .flatten()
            .filter_map(|&param_idx| {
                self.ctx
                    .arena
                    .get(param_idx)
                    .map(|param| (param.pos, param.end))
            })
            .collect()
    }

    pub(super) fn request_has_concrete_contextual_type(&self, request: &TypingRequest) -> bool {
        request.contextual_type.is_some_and(|type_id| {
            type_id != TypeId::UNKNOWN
                && type_id != TypeId::ERROR
                && !tsz_solver::type_queries::contains_infer_types_db(self.ctx.types, type_id)
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    type_id,
                )
        })
    }

    /// Remove stale implicit-any diagnostics (TS7006/TS7019/TS7031/TS7051) from
    /// parameter spans when a contextual typing refresh resolved them.
    ///
    /// Uses a `DiagnosticSnapshot` (from `snapshot_diagnostics()`) to identify
    /// which diagnostics were emitted during the refresh pass, avoiding raw
    /// index arithmetic.
    pub(super) fn clear_stale_function_like_implicit_any_diagnostics(
        &mut self,
        spans: &[(u32, u32)],
        pre_refresh: &crate::context::speculation::DiagnosticSnapshot,
    ) {
        if spans.is_empty() {
            return;
        }

        // Check if the refresh pass re-emitted any implicit-any diagnostics.
        // If so, contextual typing didn't help — keep everything.
        let refreshed_still_has_implicit_any = self.ctx.diagnostics[pre_refresh.diagnostics_len..]
            .iter()
            .any(|diag| {
                Self::implicit_any_like_diagnostic_code(diag.code)
                    && spans
                        .iter()
                        .any(|(start, end)| diag.start >= *start && diag.start < *end)
            });

        if refreshed_still_has_implicit_any {
            return;
        }

        // Contextual typing resolved all implicit-any in these spans.
        // Remove the old (pre-refresh) implicit-any diagnostics.
        self.ctx.diagnostics.retain(|diag| {
            !Self::implicit_any_like_diagnostic_code(diag.code)
                || !spans
                    .iter()
                    .any(|(start, end)| diag.start >= *start && diag.start < *end)
        });
    }

    pub(super) fn contextual_object_receiver_this_type(
        &mut self,
        contextual_type: Option<TypeId>,
        marker_this_type: Option<TypeId>,
    ) -> Option<TypeId> {
        marker_this_type.or_else(|| contextual_type.map(|ty| self.evaluate_contextual_type(ty)))
    }

    pub(super) fn substitute_contextual_this_type(
        &self,
        type_id: Option<TypeId>,
        receiver_this_type: Option<TypeId>,
    ) -> Option<TypeId> {
        type_id.map(|type_id| {
            if let Some(receiver_this_type) = receiver_this_type
                && tsz_solver::contains_this_type(self.ctx.types, type_id)
            {
                tsz_solver::substitute_this_type(self.ctx.types, type_id, receiver_this_type)
            } else {
                type_id
            }
        })
    }

    pub(crate) fn contextual_lookup_type(&mut self, contextual_type: TypeId) -> TypeId {
        let contextual_type = self.strip_contextual_this_type_markers(contextual_type);
        self.resolve_type_for_property_access(self.evaluate_contextual_type(contextual_type))
    }

    pub(super) fn contextual_object_property_type_for_lookup(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let direct = self.contextual_object_literal_property_type(contextual_type, property_name);
        if direct.is_some() {
            return direct;
        }

        let lookup_type = self.contextual_lookup_type(contextual_type);
        if lookup_type != contextual_type {
            self.contextual_object_literal_property_type(lookup_type, property_name)
        } else {
            None
        }
    }

    pub(super) fn contextual_callable_property_fallback_for_lookup(
        &mut self,
        contextual_type: TypeId,
        property_context_type: Option<TypeId>,
    ) -> Option<TypeId> {
        let fallback =
            self.contextual_callable_property_fallback_type(contextual_type, property_context_type);
        if fallback.is_some() {
            return fallback;
        }

        let lookup_type = self.contextual_lookup_type(contextual_type);
        if lookup_type != contextual_type {
            self.contextual_callable_property_fallback_type(lookup_type, property_context_type)
        } else {
            None
        }
    }

    pub(super) fn contextual_method_context_type_for_lookup(
        &mut self,
        contextual_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let allows_callable_fallback =
            self.named_contextual_property_allows_callable_fallback(contextual_type, property_name);
        let direct = match self.resolve_property_access_with_env(contextual_type, property_name) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } => {
                self.precise_callable_context_type(type_id)
            }
            _ => None,
        }
        .or_else(|| self.contextual_object_property_type_for_lookup(contextual_type, property_name))
        .or_else(|| {
            allows_callable_fallback
                .then(|| {
                    self.contextual_callable_property_fallback_for_lookup(contextual_type, None)
                })
                .flatten()
        });

        // Filter out `unknown` — it's not callable, so it provides no parameter
        // type information for method declarations. Without this, `unknown` from
        // e.g. `Record<string, unknown>` string index signatures would incorrectly
        // suppress TS7006 on method parameters. tsc emits TS7006 in this case.
        let direct = direct.filter(|&t| t != TypeId::UNKNOWN);

        if direct.is_some() {
            return direct;
        }

        let lookup_type = self.contextual_lookup_type(contextual_type);
        if lookup_type != contextual_type {
            let allows_lookup_callable_fallback =
                self.named_contextual_property_allows_callable_fallback(lookup_type, property_name);
            let result = match self.resolve_property_access_with_env(lookup_type, property_name) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                } => self.precise_callable_context_type(type_id),
                _ => None,
            }
            .or_else(|| self.contextual_object_literal_property_type(lookup_type, property_name))
            .or_else(|| {
                allows_lookup_callable_fallback
                    .then(|| self.contextual_callable_property_fallback_type(lookup_type, None))
                    .flatten()
            });
            // Same `unknown` filter for the lookup-type path.
            result.filter(|&t| t != TypeId::UNKNOWN)
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finalize_object_literal_type(
        &mut self,
        properties: FxHashMap<Atom, PropertyInfo>,
        display_type_overrides: FxHashMap<Atom, TypeId>,
        mut string_index_types: Vec<TypeId>,
        number_index_types: Vec<TypeId>,
        has_spread: bool,
        has_union_spread: bool,
        union_spread_branches: Vec<FxHashMap<Atom, PropertyInfo>>,
        generic_spread_types: Vec<TypeId>,
    ) -> TypeId {
        let object_type = if has_union_spread && !union_spread_branches.is_empty() {
            let mut post_spread_props: Vec<PropertyInfo> = properties.into_values().collect();
            post_spread_props.sort_by_key(|p| p.declaration_order);

            let mut union_members: Vec<TypeId> = Vec::new();
            for mut branch in union_spread_branches {
                for prop in &post_spread_props {
                    branch.insert(prop.name, prop.clone());
                }
                let mut branch_props: Vec<PropertyInfo> = branch.into_values().collect();
                branch_props.sort_by_key(|p| p.declaration_order);
                let obj = self.ctx.types.factory().object(branch_props);
                union_members.push(obj);
            }
            self.ctx.types.factory().union(union_members)
        } else {
            let mut properties: Vec<PropertyInfo> = properties.into_values().collect();
            properties.sort_by_key(|p| p.declaration_order);

            if string_index_types.is_empty() && number_index_types.is_empty() {
                if has_spread {
                    self.ctx.types.factory().object(properties)
                } else {
                    let type_id = self.ctx.types.factory().object_fresh(properties.clone());
                    if !display_type_overrides.is_empty() {
                        let mut display_props: Vec<PropertyInfo> = properties
                            .iter()
                            .map(|prop| {
                                if let Some(&display_type) = display_type_overrides.get(&prop.name)
                                {
                                    PropertyInfo {
                                        type_id: display_type,
                                        ..prop.clone()
                                    }
                                } else {
                                    prop.clone()
                                }
                            })
                            .collect();
                        display_props.sort_by_key(|a| a.name);
                        self.ctx
                            .types
                            .store_display_properties(type_id, display_props);
                    }
                    type_id
                }
            } else {
                use tsz_solver::{IndexSignature, ObjectShape};

                if !string_index_types.is_empty() {
                    string_index_types.extend(properties.iter().map(|prop| prop.type_id));
                }

                let string_index = if !string_index_types.is_empty() {
                    Some(IndexSignature {
                        key_type: TypeId::STRING,
                        value_type: self.ctx.types.factory().union(string_index_types),
                        readonly: false,
                        param_name: None,
                    })
                } else {
                    None
                };

                let number_index = if !number_index_types.is_empty() {
                    Some(IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: self.ctx.types.factory().union(number_index_types),
                        readonly: false,
                        param_name: None,
                    })
                } else {
                    None
                };

                let mut shape = ObjectShape {
                    properties,
                    string_index,
                    number_index,
                    ..ObjectShape::default()
                };
                if !has_spread {
                    shape.mark_fresh_literal();
                }

                self.ctx.types.factory().object_with_index(shape)
            }
        };

        if !generic_spread_types.is_empty() {
            let mut members = generic_spread_types;
            members.push(object_type);
            self.ctx.types.factory().intersection(members)
        } else {
            object_type
        }
    }

    pub(crate) fn collect_object_spread_properties(
        &mut self,
        type_id: TypeId,
    ) -> Vec<PropertyInfo> {
        let resolved = self.resolve_type_for_property_access(type_id);
        let resolved = self.resolve_lazy_type(resolved);
        self.ctx
            .types
            .collect_object_spread_properties(resolved)
            .into_iter()
            .map(|mut prop| {
                prop.parent_id = None;
                prop
            })
            .collect()
    }
}
