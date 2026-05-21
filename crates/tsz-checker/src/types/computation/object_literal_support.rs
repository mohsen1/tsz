use crate::context::TypingRequest;
use crate::query_boundaries::common::TypeResolver;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{PropertyInfo, TypeId};

/// Parameters for finalizing an object literal type after all properties have been processed.
pub(crate) struct ObjectLiteralFinalizeCtx {
    /// Named properties collected from the literal.
    pub(crate) properties: FxHashMap<Atom, PropertyInfo>,
    /// Per-property display-type overrides (used for freshness diagnostics).
    pub(crate) display_type_overrides: FxHashMap<Atom, TypeId>,
    /// Value types for string index signatures collected from spreads.
    pub(crate) string_index_types: Vec<TypeId>,
    /// Value types for number index signatures collected from spreads.
    pub(crate) number_index_types: Vec<TypeId>,
    /// Value types contributed by computed property names whose key type is
    /// the wide `symbol` (non-`unique`). Per tsc, such keys synthesize a
    /// `[k: symbol]: V` index signature rather than a named member.
    pub(crate) symbol_index_types: Vec<TypeId>,
    /// Parameter name atom for the string index signature, if present.
    pub(crate) string_index_param_name: Option<Atom>,
    /// Parameter name atom for the number index signature, if present.
    pub(crate) number_index_param_name: Option<Atom>,
    /// Whether any spread element is present.
    pub(crate) has_spread: bool,
    /// Whether any spread element resolved to `any`.
    pub(crate) has_any_spread: bool,
    /// Whether any spread element resolved to a union type.
    pub(crate) has_union_spread: bool,
    /// Per-branch property maps for union-spread expansion.
    pub(crate) union_spread_branches: Vec<FxHashMap<Atom, PropertyInfo>>,
    /// Generic (unevaluated) spread types that could not be unioned.
    pub(crate) generic_spread_types: Vec<TypeId>,
    /// Whether all properties are context-sensitive (deferred freshness).
    pub(crate) all_properties_context_sensitive: bool,
}

fn order_preserving_union(
    factory: tsz_solver::construction::TypeFactory<'_>,
    mut members: Vec<TypeId>,
) -> TypeId {
    let mut seen = rustc_hash::FxHashSet::default();
    members.retain(|id| *id != TypeId::NEVER && seen.insert(*id));
    match members.as_slice() {
        [] => TypeId::NEVER,
        [only] => *only,
        _ => factory.union_preserve_order(members),
    }
}

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

    /// Build the parameter span list for `clear_stale_function_like_implicit_any_diagnostics`,
    /// restricted to parameters whose post-refresh symbol type is no longer
    /// `any`/`unknown`. A parameter that's still `any` after the refresh
    /// means contextual typing didn't actually provide it a type — clearing
    /// its TS7006 would be a false negative. This is the difference between
    /// genuine contextual typing (`let f: { a: (n: number) => … } = { a:
    /// function(n) {…} }`) and the IIFE-arg self-referential shape
    /// (`(o => o.a(11))({ a: function(n) {…} })`) where the property's
    /// "contextual type" loops back through the function's own value type
    /// without actually constraining the parameter.
    pub(super) fn contextually_typed_param_spans_for_node(
        &mut self,
        idx: NodeIndex,
    ) -> Vec<(u32, u32)> {
        let Some(node) = self.ctx.arena.get(idx) else {
            return Vec::new();
        };
        let params = if let Some(func) = self.ctx.arena.get_function(node) {
            func.parameters.nodes.clone()
        } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
            method.parameters.nodes.clone()
        } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
            accessor.parameters.nodes.clone()
        } else {
            return Vec::new();
        };

        let mut out = Vec::with_capacity(params.len());
        for param_idx in params {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let param = match self.ctx.arena.get_parameter(param_node) {
                Some(p) => p,
                None => continue,
            };
            let span = (param_node.pos, param_node.end);
            // Resolve the parameter's symbol and inspect its current type.
            let sym_id = self
                .ctx
                .binder
                .get_node_symbol(param.name)
                .or_else(|| self.ctx.binder.get_node_symbol(param_idx));
            let Some(sym_id) = sym_id else { continue };
            let ty = self.get_type_of_symbol(sym_id);
            if ty != TypeId::ANY && ty != TypeId::UNKNOWN {
                out.push(span);
            }
        }
        out
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
                && !crate::query_boundaries::common::contains_infer_types(self.ctx.types, type_id)
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
                && crate::query_boundaries::common::contains_this_type(self.ctx.types, type_id)
            {
                crate::query_boundaries::common::substitute_this_type(
                    self.ctx.types,
                    type_id,
                    receiver_this_type,
                )
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

    pub(crate) fn finalize_object_literal_type(&mut self, ctx: ObjectLiteralFinalizeCtx) -> TypeId {
        let ObjectLiteralFinalizeCtx {
            properties,
            display_type_overrides,
            mut string_index_types,
            number_index_types,
            symbol_index_types,
            string_index_param_name,
            number_index_param_name,
            has_spread,
            has_any_spread,
            has_union_spread,
            union_spread_branches,
            generic_spread_types,
            all_properties_context_sensitive,
        } = ctx;
        if has_any_spread {
            return TypeId::ANY;
        }

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
                let mut display_props = branch_props.clone();
                crate::query_boundaries::common::normalize_display_property_order(
                    &mut display_props,
                );
                let obj = self
                    .ctx
                    .types
                    .factory()
                    .object_preserve_declaration_order(branch_props);
                self.ctx.types.store_display_properties(obj, display_props);
                union_members.push(obj);
            }
            self.ctx.types.factory().union(union_members)
        } else {
            let mut properties: Vec<PropertyInfo> = properties.into_values().collect();
            properties.sort_by_key(|p| p.declaration_order);

            if string_index_types.is_empty()
                && number_index_types.is_empty()
                && symbol_index_types.is_empty()
            {
                if has_spread {
                    let mut display_props = properties.clone();
                    crate::query_boundaries::common::normalize_display_property_order(
                        &mut display_props,
                    );
                    let type_id = self
                        .ctx
                        .types
                        .factory()
                        .object_preserve_declaration_order(properties);
                    self.ctx
                        .types
                        .store_display_properties(type_id, display_props);
                    type_id
                } else {
                    let type_id = if all_properties_context_sensitive {
                        self.ctx
                            .types
                            .factory()
                            .object_fresh_all_properties_context_sensitive(properties.clone())
                    } else {
                        self.ctx.types.factory().object_fresh(properties.clone())
                    };
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
                        crate::query_boundaries::common::normalize_display_property_order(
                            &mut display_props,
                        );
                        self.ctx
                            .types
                            .store_display_properties(type_id, display_props);
                    }
                    type_id
                }
            } else {
                use tsz_solver::{IndexSignature, ObjectShape};

                if !string_index_types.is_empty() {
                    let prop_types = properties.iter().map(|prop| prop.type_id);
                    if self.ctx.in_const_assertion {
                        string_index_types = prop_types.chain(string_index_types).collect();
                    } else {
                        string_index_types.extend(prop_types);
                    }
                }

                let string_index = if !string_index_types.is_empty() {
                    let value_type = if self.ctx.in_const_assertion {
                        order_preserving_union(self.ctx.types.factory(), string_index_types)
                    } else {
                        self.ctx.types.factory().union(string_index_types)
                    };
                    Some(IndexSignature {
                        key_type: TypeId::STRING,
                        value_type,
                        readonly: false,
                        param_name: string_index_param_name,
                    })
                } else {
                    None
                };

                let symbol_index = if !symbol_index_types.is_empty() {
                    let value_type = if self.ctx.in_const_assertion {
                        order_preserving_union(self.ctx.types.factory(), symbol_index_types)
                    } else {
                        self.ctx.types.factory().union(symbol_index_types)
                    };
                    Some(IndexSignature {
                        key_type: TypeId::SYMBOL,
                        value_type,
                        readonly: false,
                        param_name: None,
                    })
                } else {
                    None
                };

                let number_index = if !number_index_types.is_empty() {
                    let value_type = if self.ctx.in_const_assertion {
                        order_preserving_union(self.ctx.types.factory(), number_index_types)
                    } else {
                        self.ctx.types.factory().union(number_index_types)
                    };
                    Some(IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type,
                        readonly: false,
                        param_name: number_index_param_name,
                    })
                } else {
                    None
                };

                let mut shape = ObjectShape {
                    properties,
                    string_index,
                    number_index,
                    symbol_index,
                    ..ObjectShape::default()
                };
                if !has_spread {
                    shape.mark_fresh_literal();
                    if all_properties_context_sensitive {
                        shape.mark_all_properties_context_sensitive();
                    }
                }

                let display_props = if has_spread {
                    shape.mark_preserve_declaration_order();
                    let mut display_props = shape.properties.clone();
                    crate::query_boundaries::common::normalize_display_property_order(
                        &mut display_props,
                    );
                    Some(display_props)
                } else {
                    None
                };
                let type_id = self.ctx.types.factory().object_with_index(shape);
                if let Some(display_props) = display_props {
                    self.ctx
                        .types
                        .store_display_properties(type_id, display_props);
                }
                type_id
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
        let resolved = self.evaluate_type_with_env(resolved);
        // Cross-file lowering can leave intersection members as
        // `Application(UnresolvedTypeName(name), args)` when an imported
        // qualified type (e.g. `util.OmitKeys`) was referenced inside an
        // alias body that the lowering pass couldn't fully resolve. By the
        // time the spread collector runs, the merged binder graph IS
        // available — re-run the cross-file qualified-name resolver and
        // rewrite the base to `Lazy(def_id)` when we can.
        let resolved = self.reresolve_unresolved_application_bases_in_intersection(resolved);
        // Even with a `Lazy(def_id)` base, the solver-level spread collector
        // returns `Vec::new()` for `Application` members because no arm of
        // its match handles them. Force-expand each such member here via
        // `instantiate_type` + `evaluate_type_with_env`. Falls back to the
        // original member when expansion collapses to `unknown`/`error`.
        let resolved = self.expand_intersection_application_members_for_spread(resolved);
        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let eval_constraint =
                self.evaluate_mapped_constraint_with_resolution(mapped.constraint);
            let mapped_id = if eval_constraint != mapped.constraint {
                let mapped_type = tsz_solver::MappedType {
                    type_param: mapped.type_param,
                    constraint: eval_constraint,
                    name_type: mapped.name_type,
                    template: mapped.template,
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };
                crate::query_boundaries::common::mapped_type_id(
                    self.ctx.types,
                    self.ctx.types.factory().mapped(mapped_type),
                )
                .unwrap_or(mapped_id)
            } else {
                mapped_id
            };
            if let Some(names) =
                crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                    self.ctx.types,
                    mapped_id,
                )
            {
                return names
                    .into_iter()
                    .filter_map(|name| {
                        let prop_name = self.ctx.types.resolve_atom_ref(name);
                        let type_id =
                            crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                                self.ctx.types,
                                mapped_id,
                                prop_name.as_ref(),
                            )?;
                        Some(PropertyInfo {
                            name,
                            type_id,
                            optional: false,
                            readonly: false,
                            write_type: type_id,
                            is_class_prototype: false,
                            is_method: false,
                            visibility: tsz_solver::Visibility::Public,
                            parent_id: None,
                            declaration_order: 0,
                            is_string_named: false,
                            is_symbol_named: false,
                            single_quoted_name: false,
                        })
                    })
                    .collect();
            }
        }
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

    /// When `type_id` is an intersection containing
    /// `Application(UnresolvedTypeName(name), args)` members, attempt to
    /// recover the alias body by re-running the cross-file qualified-name
    /// resolver from the current checker context. Cross-file lowering can
    /// fall back to `UnresolvedTypeName` when an imported namespace member
    /// (e.g. `util.OmitKeys`) wasn't yet visible at the time the alias body
    /// was lowered; by the time the spread collector runs, the full binder
    /// graph is available and the dotted name resolves cleanly.
    fn reresolve_unresolved_application_bases_in_intersection(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let mut rewritten = Vec::with_capacity(members.len());
        let mut changed = false;
        for member in members {
            let new_member = self.reresolve_unresolved_application_base(member);
            if new_member != member {
                changed = true;
            }
            rewritten.push(new_member);
        }
        if !changed {
            return type_id;
        }
        let new_intersection =
            crate::query_boundaries::spread::make_intersection(self.ctx.types, rewritten);
        self.evaluate_type_with_env(new_intersection)
    }

    /// If `type_id` is `Application(UnresolvedTypeName(name), args)` whose
    /// flat dotted `name` resolves to a `DefId` via the current binder, return
    /// `Application(Lazy(def_id), args)`. Otherwise return `type_id` unchanged.
    fn reresolve_unresolved_application_base(&mut self, type_id: TypeId) -> TypeId {
        let Some(app_id) = crate::query_boundaries::common::application_id(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let app = self.ctx.types.type_application(app_id);
        let Some(atom) =
            crate::query_boundaries::spread::unresolved_type_name_atom(self.ctx.types, app.base)
        else {
            return type_id;
        };
        let name = self.ctx.types.resolve_atom(atom);
        let Some(def_id) = self.resolve_entity_name_text_to_def_id_for_lowering(&name) else {
            return type_id;
        };
        let lazy_base = crate::query_boundaries::spread::make_lazy(self.ctx.types, def_id);
        crate::query_boundaries::spread::make_application(
            self.ctx.types,
            lazy_base,
            app.args.clone(),
        )
    }

    /// When `type_id` is an intersection whose members include an unreduced
    /// `Application(Lazy(def_id), [args])`, expand each such member by
    /// instantiating the alias body with the supplied args using the
    /// resolver-equipped checker context. The solver-level spread collector
    /// returns no properties for `Application` members (they fall through
    /// `_ => Vec::new()`), so without this expansion the structural
    /// contributions of e.g. `OmitKeys<U, "path">` would be lost.
    fn expand_intersection_application_members_for_spread(&mut self, type_id: TypeId) -> TypeId {
        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let mut expanded = Vec::with_capacity(members.len());
        let mut changed = false;
        for member in members {
            let exp = self.expand_application_member_for_spread(member);
            if exp != member {
                changed = true;
            }
            expanded.push(exp);
        }
        if !changed {
            return type_id;
        }
        let new_intersection =
            crate::query_boundaries::spread::make_intersection(self.ctx.types, expanded);
        self.evaluate_type_with_env(new_intersection)
    }

    /// Expand a single `Application(Lazy(def_id), args)` to its instantiated
    /// body via `instantiate_type` + `evaluate_type_with_env`. Falls back to
    /// the original `type_id` for non-applications, missing type params,
    /// missing body, arity mismatch, or when evaluation collapses to
    /// `unknown`/`error`/`any` (which would otherwise let downstream
    /// intersection reduction silently drop the application via
    /// `unknown & T = T`).
    fn expand_application_member_for_spread(&mut self, type_id: TypeId) -> TypeId {
        // When the member is a Mapped type (typical residue of a partially
        // expanded `Pick<T, K>`/`Omit<T, K>` body whose deeper helper bodies
        // weren't reachable cross-file), pre-evaluate it into a concrete
        // object so the solver-level spread collector — which falls through
        // `Mapped` in its intersection match arm — can see the property
        // contributions instead of returning `Vec::new()`.
        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, type_id)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let eval_constraint =
                self.evaluate_mapped_constraint_with_resolution(mapped.constraint);
            let resolved_mapped_id = if eval_constraint != mapped.constraint {
                let mapped_type = tsz_solver::MappedType {
                    type_param: mapped.type_param,
                    constraint: eval_constraint,
                    name_type: mapped.name_type,
                    template: mapped.template,
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };
                crate::query_boundaries::common::mapped_type_id(
                    self.ctx.types,
                    self.ctx.types.factory().mapped(mapped_type),
                )
                .unwrap_or(mapped_id)
            } else {
                mapped_id
            };
            if let Some(names) =
                crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                    self.ctx.types,
                    resolved_mapped_id,
                )
            {
                let mut properties = Vec::with_capacity(names.len());
                for name in names {
                    let prop_name = self.ctx.types.resolve_atom_ref(name);
                    if let Some(prop_type_id) =
                        crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                            self.ctx.types,
                            resolved_mapped_id,
                            prop_name.as_ref(),
                        )
                    {
                        properties.push(tsz_solver::PropertyInfo::new(name, prop_type_id));
                    }
                }
                if !properties.is_empty() {
                    return self.ctx.types.factory().object(properties);
                }
            }
        }
        let Some(app_id) = crate::query_boundaries::common::application_id(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let app = self.ctx.types.type_application(app_id);
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
        else {
            return type_id;
        };
        let Some(type_params) = self.ctx.get_lazy_type_params(def_id) else {
            return type_id;
        };
        let Some(body) = self.ctx.resolve_lazy(def_id, self.ctx.types) else {
            return type_id;
        };
        if type_params.len() != app.args.len() {
            return type_id;
        }
        // When the alias body itself is `unknown` (the declaring file's
        // checker hasn't populated the body yet, typical for parallel
        // cross-file checking), instantiation collapses to `unknown` and
        // the spread loses all properties. As a structural fallback, treat
        // an opaque-body homomorphic-ish alias as projecting from its
        // first type argument: spread the first arg in place of the
        // unresolved alias body. This recovers spread shape for patterns
        // like `OmitKeys<T, K> = Pick<T, Exclude<keyof T, K>>` where
        // expanding `T`'s properties is approximately correct (the spread
        // immediately re-assigns the omitted key anyway).
        if body == TypeId::UNKNOWN
            && let Some(first_arg) = app.args.first()
            && !app.args.is_empty()
        {
            let arg_evaluated = self.evaluate_type_with_env(*first_arg);
            if arg_evaluated != TypeId::UNKNOWN
                && arg_evaluated != TypeId::ERROR
                && arg_evaluated != TypeId::ANY
            {
                return arg_evaluated;
            }
        }
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &type_params,
            &app.args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &substitution);
        if instantiated == type_id {
            return type_id;
        }
        let evaluated = self.evaluate_type_with_env(instantiated);
        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR || evaluated == TypeId::ANY {
            return type_id;
        }
        // When the evaluated form is still an opaque `Application(Lazy(_), _)`
        // — typically a nested helper (`Pick`/`Exclude`) whose body is
        // `unknown` because the declaring file's checker hasn't populated
        // it yet — fall back to the FIRST type argument of the original
        // outer application. For homomorphic-mapped patterns like
        // `OmitKeys<T, K> = Pick<T, Exclude<keyof T, K>>` this is
        // approximately correct: the spread carries `T`'s properties and
        // any explicit subsequent assignments overwrite the omitted key.
        // Without this fallback, the entire alias's structural shape is
        // lost when the inner helper's body is missing.
        if crate::query_boundaries::common::application_id(self.ctx.types, evaluated).is_some()
            && let Some(first_arg) = app.args.first()
        {
            let arg_evaluated = self.evaluate_type_with_env(*first_arg);
            if arg_evaluated != TypeId::UNKNOWN
                && arg_evaluated != TypeId::ERROR
                && arg_evaluated != TypeId::ANY
                && crate::query_boundaries::common::application_id(self.ctx.types, arg_evaluated)
                    .is_none()
            {
                return arg_evaluated;
            }
        }
        evaluated
    }
}
