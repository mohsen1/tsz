//! JSX props validation: attribute type-checking helpers, missing required props (TS2741),
//! intrinsic attribute resolution, and grammar checks.
//!
//! Props extraction lives in `extraction.rs`, overload resolution in `overloads.rs`.

use crate::context::TypingRequest;
use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn compute_normalized_jsx_spread_type_with_request(
        &mut self,
        spread_expr_idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        if request.contextual_type.is_some() {
            self.invalidate_expression_for_contextual_retry(spread_expr_idx);
        }

        let spread_type = self.get_type_of_node_with_request(spread_expr_idx, request);
        let spread_type = self.evaluate_type_with_env(spread_type);
        self.resolve_type_for_property_access(spread_type)
    }

    pub(in crate::checkers_domain::jsx) fn build_jsx_provided_attrs_object_type(
        &mut self,
        provided_attrs: &[(String, TypeId)],
    ) -> TypeId {
        let properties: Vec<tsz_solver::PropertyInfo> = provided_attrs
            .iter()
            .map(|(name, type_id)| {
                let name_atom = self.ctx.types.intern_string(name);
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
        self.ctx.types.factory().object(properties)
    }

    pub(in crate::checkers_domain::jsx) fn jsx_children_display_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        else {
            return type_id;
        };
        if elements.len() <= 1
            || elements
                .iter()
                .any(|element| element.optional || element.rest || element.name.is_some())
        {
            return type_id;
        }
        let element_types = elements
            .iter()
            .map(|element| self.widen_type_for_display(element.type_id))
            .collect::<Vec<_>>();
        let array_element = if element_types.len() == 1 {
            element_types[0]
        } else {
            self.ctx.types.factory().union(element_types)
        };
        self.ctx.types.factory().array(array_element)
    }

    fn format_jsx_provided_attrs_source_type(
        &mut self,
        provided_attrs: &[(String, TypeId)],
    ) -> String {
        if provided_attrs.is_empty() {
            return "{}".to_string();
        }

        let format_name = |name: &str| {
            let mut chars = name.chars();
            let Some(first) = chars.next() else {
                return "\"\"".to_string();
            };
            let is_ident = (first == '_' || first == '$' || first.is_ascii_alphabetic())
                && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric());
            if is_ident {
                name.to_string()
            } else {
                format!("\"{name}\"")
            }
        };

        let fields = provided_attrs
            .iter()
            .map(|(name, type_id)| {
                let display_type = if name == "children" {
                    self.jsx_children_display_type(*type_id)
                } else {
                    *type_id
                };
                format!("{}: {}", format_name(name), self.format_type(display_type))
            })
            .collect::<Vec<_>>()
            .join("; ");
        if fields.is_empty() {
            "{}".to_string()
        } else {
            format!("{{ {fields}; }}")
        }
    }

    fn format_jsx_missing_props_target_type(
        &mut self,
        target_type: TypeId,
        preferred_target_display: Option<&str>,
    ) -> String {
        let target_display = self.format_type(target_type);

        if let Some(display) = preferred_target_display
            && !display.contains("children?:")
        {
            if display.starts_with("IntrinsicClassAttributes<")
                && let Some(alias_display) =
                    self.jsx_intrinsic_class_attributes_alias_target_display()
            {
                return alias_display;
            }
            return display.to_string();
        }

        if preferred_target_display.is_none()
            && target_display.starts_with("IntrinsicClassAttributes<")
            && let Some(display) = self.jsx_intrinsic_class_attributes_alias_target_display()
        {
            return display;
        }

        let target_type = self.normalize_jsx_required_props_target(target_type);
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_type)
        {
            let filtered_props: Vec<_> = shape
                .properties
                .iter()
                .filter(|prop| {
                    let name = self.ctx.types.resolve_atom(prop.name);
                    !(name == "children" && prop.optional)
                })
                .cloned()
                .collect();
            if filtered_props.len() != shape.properties.len() {
                return self.format_type(self.ctx.types.factory().object(filtered_props));
            }
        }

        preferred_target_display
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.format_type(target_type))
    }

    fn jsx_intrinsic_class_attributes_alias_target_display(&mut self) -> Option<String> {
        let lib_binders = self.get_lib_binders();
        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let jsx_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let ica_sym_id = jsx_symbol
            .exports
            .as_ref()?
            .get("IntrinsicClassAttributes")?;

        let alias_target_text_for_symbol = |binder: &tsz_binder::BinderState,
                                            arena: &tsz_parser::parser::node::NodeArena|
         -> Option<String> {
            let symbol = binder.get_symbol(ica_sym_id)?;
            for &decl_idx in &symbol.declarations {
                let node = arena.get(decl_idx)?;
                let alias = arena.get_type_alias(node)?;
                let target_node = arena.get(alias.type_node)?;
                let source = arena.source_files.first()?.text.as_ref();
                let start = target_node.pos as usize;
                let end = target_node.end as usize;
                if start >= end || end > source.len() {
                    continue;
                }
                let text = source[start..end].trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            None
        };

        if let Some(text) = alias_target_text_for_symbol(self.ctx.binder, self.ctx.arena) {
            return Some(text);
        }
        if let (Some(all_binders), Some(all_arenas)) =
            (self.ctx.all_binders.as_ref(), self.ctx.all_arenas.as_ref())
        {
            for (binder, arena) in all_binders.iter().zip(all_arenas.iter()) {
                if let Some(text) = alias_target_text_for_symbol(binder, arena.as_ref()) {
                    return Some(text);
                }
            }
        }
        None
    }

    pub(in crate::checkers_domain::jsx) fn should_report_custom_jsx_children_via_assignability(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
    ) -> bool {
        let children_prop_name = self.get_jsx_children_prop_name();
        if children_prop_name == "children" {
            return false;
        }
        if provided_attrs
            .iter()
            .any(|(name, _)| name == &children_prop_name)
        {
            return false;
        }

        let resolved = self.resolve_type_for_property_access(props_type);
        let resolved = self.evaluate_type_with_env(resolved);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved)
        else {
            return false;
        };

        shape.properties.iter().any(|prop| {
            !prop.optional && self.ctx.types.resolve_atom(prop.name) == children_prop_name
        })
    }

    pub(in crate::checkers_domain::jsx) fn report_jsx_synthesized_props_assignability_error(
        &mut self,
        attrs_type: TypeId,
        display_target: &str,
        anchor_idx: NodeIndex,
    ) {
        let source_str = if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, attrs_type)
        {
            // Sort by declaration_order to preserve JSX attribute source order.
            // Properties are stored sorted by atom name for O(1) hashing dedup; declaration_order
            // captures the original insertion order (e.g. `x={1} render={2}` → x first).
            let mut props: Vec<_> = shape.properties.iter().collect();
            props.sort_by(
                |a, b| match (a.declaration_order > 0, b.declaration_order > 0) {
                    (true, true) => a.declaration_order.cmp(&b.declaration_order),
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (false, false) => a.name.cmp(&b.name),
                },
            );
            let fields = props
                .iter()
                .map(|prop| {
                    let name = self.ctx.types.resolve_atom_ref(prop.name);
                    let display_type = if name.as_ref() == "children" {
                        self.jsx_children_display_type(prop.type_id)
                    } else {
                        prop.type_id
                    };
                    let formatted_name = {
                        let n = name.as_ref();
                        let mut chars = n.chars();
                        let is_ident = chars
                            .next()
                            .is_some_and(|c| c == '_' || c == '$' || c.is_ascii_alphabetic())
                            && chars.all(|c| c == '_' || c == '$' || c.is_ascii_alphanumeric());
                        if is_ident {
                            n.to_string()
                        } else {
                            format!("\"{n}\"")
                        }
                    };
                    format!("{formatted_name}: {}", self.format_type(display_type))
                })
                .collect::<Vec<_>>()
                .join("; ");
            if fields.is_empty() {
                "{}".to_string()
            } else {
                format!("{{ {fields}; }}")
            }
        } else {
            self.format_type(attrs_type)
        };
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, display_target],
        );
        self.error_at_node(
            anchor_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    pub(in crate::checkers_domain::jsx) fn normalize_jsx_required_props_target(
        &mut self,
        props_type: TypeId,
    ) -> TypeId {
        let normalized = self.evaluate_application_type(props_type);
        let normalized = self.evaluate_type_with_env(normalized);
        let normalized = self.resolve_type_for_property_access(normalized);
        let normalized = self.resolve_lazy_type(normalized);
        let normalized = self.evaluate_application_type(normalized);
        self.evaluate_type_with_env(normalized)
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_special_attribute_expected_type(
        &mut self,
        attr_name: &str,
        props_type: TypeId,
        special_attr_component_type: Option<TypeId>,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        let get_property_type = |access: PropertyAccessResult| match access {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        };

        get_property_type(self.resolve_property_access_with_env(props_type, attr_name))
            .or_else(|| {
                if attr_name == "key" {
                    self.get_intrinsic_attributes_type().and_then(|ia_type| {
                        let ia_type = self.normalize_jsx_required_props_target(ia_type);
                        get_property_type(self.resolve_property_access_with_env(ia_type, attr_name))
                    })
                } else {
                    None
                }
            })
            .or_else(|| {
                if attr_name == "ref" {
                    special_attr_component_type.and_then(|component_type| {
                        self.get_intrinsic_class_attributes_type_for_component(component_type)
                            .and_then(|ica_type| {
                                let ica_type = self.normalize_jsx_required_props_target(ica_type);
                                get_property_type(
                                    self.resolve_property_access_with_env(ica_type, attr_name),
                                )
                            })
                            .or_else(|| {
                                self.get_jsx_intrinsic_class_attribute_from_heritage(
                                    attr_name,
                                    component_type,
                                )
                            })
                            .or_else(|| {
                                self.get_jsx_class_ref_fallback_type(attr_name, component_type)
                            })
                    })
                } else {
                    None
                }
            })
    }

    fn get_jsx_class_ref_fallback_type(
        &mut self,
        attr_name: &str,
        component_type: TypeId,
    ) -> Option<TypeId> {
        if attr_name != "ref" {
            return None;
        }

        let instance_type = self.get_class_instance_type_for_component(component_type)?;
        let param_name = self.ctx.types.intern_string("instance");
        let callback = self
            .ctx
            .types
            .factory()
            .function(tsz_solver::FunctionShape::new(
                vec![tsz_solver::ParamInfo::required(param_name, instance_type)],
                TypeId::ANY,
            ));
        Some(self.ctx.types.factory().union2(TypeId::STRING, callback))
    }

    fn get_jsx_intrinsic_class_attribute_from_heritage(
        &mut self,
        attr_name: &str,
        component_type: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;
        use tsz_scanner::SyntaxKind;

        let jsx_sym_id = self.get_jsx_namespace_type()?;
        let lib_binders = self.get_lib_binders();
        let jsx_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;
        let ica_sym_id = jsx_symbol
            .exports
            .as_ref()?
            .get("IntrinsicClassAttributes")?;
        let ica_symbol = self.ctx.binder.get_symbol(ica_sym_id)?;
        let instance_type = self.get_class_instance_type_for_component(component_type)?;

        for mut decl_idx in ica_symbol.all_declarations() {
            let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(parent) = self.ctx.arena.parent_of(decl_idx)
                && parent.is_some()
            {
                decl_idx = parent;
                let Some(parent_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                decl_node = parent_node;
            }
            let Some(iface) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };
            let Some(heritage_clauses) = &iface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else {
                            (type_idx, None)
                        }
                    } else {
                        (type_idx, None)
                    };

                    let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };

                    let mut base_type = self.get_type_of_symbol(base_sym_id);
                    if matches!(base_type, TypeId::ERROR | TypeId::UNKNOWN) {
                        continue;
                    }

                    let base_type_params = self.get_type_params_for_symbol(base_sym_id);
                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        if args.nodes.len() == 1 && base_type_params.len() == 1 {
                            type_args.push(instance_type);
                        } else {
                            for &arg_idx in &args.nodes {
                                type_args.push(self.get_type_from_type_node(arg_idx));
                            }
                        }
                    }

                    if !base_type_params.is_empty() {
                        let substitution = tsz_solver::TypeSubstitution::from_args(
                            self.ctx.types,
                            &base_type_params,
                            &type_args,
                        );
                        base_type = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            base_type,
                            &substitution,
                        );
                    }

                    let base_type = self.normalize_jsx_required_props_target(base_type);
                    match self.resolve_property_access_with_env(base_type, attr_name) {
                        PropertyAccessResult::Success { type_id, .. }
                        | PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: Some(type_id),
                            ..
                        } => return Some(type_id),
                        _ => {}
                    }
                }
            }
        }

        None
    }

    fn preferred_jsx_missing_props_target(&mut self, props_type: TypeId) -> TypeId {
        let normalized = self.normalize_jsx_required_props_target(props_type);
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, props_type)
                .or_else(|| {
                    crate::query_boundaries::common::intersection_members(
                        self.ctx.types,
                        normalized,
                    )
                });
        let Some(members) = members else {
            return normalized;
        };

        let mut best = None;
        let mut best_score = (true, usize::MAX, usize::MAX);
        for member in members {
            let resolved_member = self.normalize_jsx_required_props_target(member);
            let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                resolved_member,
            ) else {
                continue;
            };

            let required_count = shape
                .properties
                .iter()
                .filter(|prop| !prop.optional)
                .count();
            if required_count == 0 {
                continue;
            }

            let display = self.format_type(member);
            let is_anonymous = display.starts_with('{');
            let property_count = shape.properties.len();
            let score = (is_anonymous, property_count, required_count);
            if score < best_score {
                best = Some(resolved_member);
                best_score = score;
            }
        }

        best.unwrap_or(normalized)
    }

    /// Check that all required properties in the props type are provided. Emits TS2741.
    pub(in crate::checkers_domain::jsx) fn check_missing_required_jsx_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
        attributes_idx: NodeIndex,
        tag_name_idx: Option<NodeIndex>,
        preferred_target_display: Option<&str>,
    ) {
        let preferred_target = self.preferred_jsx_missing_props_target(props_type);
        let Some(shape) = self.get_normalized_jsx_required_props_shape(preferred_target) else {
            return;
        };

        let missing_names: Vec<_> = shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .filter_map(|prop| {
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                (!provided_attrs.iter().any(|(a, _)| a == &prop_name)).then_some(prop.name)
            })
            .collect();

        if missing_names.is_empty() {
            return;
        }
        let mut missing_names = missing_names;
        missing_names.sort_by_key(|name| self.ctx.types.resolve_atom_ref(*name).to_string());

        let source_type = self.format_jsx_provided_attrs_source_type(provided_attrs);
        let target_type = self.format_jsx_missing_props_target_type(
            preferred_target,
            preferred_target_display.filter(|display| !display.is_empty()),
        );

        if missing_names.len() == 1 {
            let missing_name = missing_names[0];
            let prop_name = self.ctx.types.resolve_atom(missing_name);
            let message = format!(
                "Property '{prop_name}' is missing in type '{source_type}' but required in type '{target_type}'."
            );
            let Some((start, end)) = self.get_node_span(attributes_idx) else {
                return;
            };
            let (start, length) =
                self.normalized_anchor_span(attributes_idx, start, end.saturating_sub(start));
            let mut related = Vec::new();
            if let Some(tag_name_idx) = tag_name_idx
                && let Some(prop_decl_idx) = self
                    .get_jsx_component_prop_declaration(tag_name_idx, &prop_name)
                    .or_else(|| self.nearest_property_declaration_before(tag_name_idx, &prop_name))
                && let Some((related_start, related_end)) = self.get_node_span(prop_decl_idx)
            {
                let (related_start, related_length) = self.normalized_anchor_span(
                    prop_decl_idx,
                    related_start,
                    related_end.saturating_sub(related_start),
                );
                let related_file = self
                    .source_file_data_for_node(prop_decl_idx)
                    .map(|sf| sf.file_name.clone())
                    .unwrap_or_else(|| self.ctx.file_name.clone());
                related.push(DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::IS_DECLARED_HERE,
                    file: related_file,
                    start: related_start,
                    length: related_length,
                    message_text: format_message(
                        diagnostic_messages::IS_DECLARED_HERE,
                        &[&prop_name],
                    ),
                });
            }
            self.emit_render_request_at_anchor(
                ResolvedDiagnosticAnchor {
                    node_idx: attributes_idx,
                    start,
                    length,
                },
                DiagnosticRenderRequest::with_related(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    message,
                    related,
                    RelatedInformationPolicy::ELABORATION,
                ),
            );
            return;
        }

        let props_joined = missing_names
            .iter()
            .take(4)
            .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
            .collect::<Vec<_>>()
            .join(", ");

        if missing_names.len() > 4 {
            let more_count = (missing_names.len() - 4).to_string();
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                &[&source_type, &target_type, &props_joined, &more_count],
            );
            self.error_at_node(
                attributes_idx,
                &message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
            );
        } else {
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                &[&source_type, &target_type, &props_joined],
            );
            self.error_at_node(
                attributes_idx,
                &message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
            );
        }
    }

    pub(in crate::checkers_domain::jsx) fn jsx_has_missing_required_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[(String, TypeId)],
    ) -> bool {
        let Some(shape) = self.get_normalized_jsx_required_props_shape(props_type) else {
            return false;
        };

        shape.properties.iter().any(|prop| {
            !prop.optional
                && !provided_attrs
                    .iter()
                    .any(|(name, _)| name == &self.ctx.types.resolve_atom(prop.name))
        })
    }

    fn jsx_component_member_signature_kinds(&mut self, component_type: TypeId) -> (bool, bool) {
        let component_type = self.normalize_jsx_component_type_for_resolution(component_type);
        let component_type = self.evaluate_type_with_env(component_type);
        let members =
            crate::query_boundaries::common::union_members(self.ctx.types, component_type)
                .unwrap_or_else(|| vec![component_type]);

        let mut has_construct = false;
        let mut has_call = false;

        for member in members {
            let member = self.resolve_type_for_property_access(member);
            let member = self.evaluate_type_with_env(member);

            has_construct |= crate::query_boundaries::common::construct_signatures_for_type(
                self.ctx.types,
                member,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            has_call |=
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, member)
                    .is_some_and(|shape| !shape.is_constructor)
                    || crate::query_boundaries::common::call_signatures_for_type(
                        self.ctx.types,
                        member,
                    )
                    .is_some_and(|sigs| !sigs.is_empty());
        }

        (has_construct, has_call)
    }

    pub(in crate::checkers_domain::jsx) fn should_report_jsx_class_missing_props_via_assignability(
        &mut self,
        component_type: TypeId,
    ) -> bool {
        // A union of class component types behaves differently from a single
        // class with union props. For `RC1 || RC4`, tsc reports TS2741 against
        // the selected required-props surface rather than a generic TS2322
        // whole-object assignability error. Keep the TS2322 route for a single
        // class component whose props type is itself a union.
        if crate::query_boundaries::common::union_members(self.ctx.types, component_type)
            .is_some_and(|members| members.len() > 1)
        {
            return false;
        }

        let (has_construct, has_call) = self.jsx_component_member_signature_kinds(component_type);
        if !has_construct || has_call {
            return false;
        }

        let component_type = self.normalize_jsx_component_type_for_resolution(component_type);
        let component_type = self.evaluate_type_with_env(component_type);
        let members =
            crate::query_boundaries::common::union_members(self.ctx.types, component_type)
                .unwrap_or_else(|| vec![component_type]);

        !members.into_iter().any(|member| {
            let member = self.resolve_type_for_property_access(member);
            let member = self.evaluate_type_with_env(member);
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, member)
                .is_some_and(|sigs| sigs.iter().any(|sig| !sig.type_params.is_empty()))
        })
    }

    pub(in crate::checkers_domain::jsx) fn jsx_tag_is_logical_component_alias(
        &self,
        tag_name_idx: NodeIndex,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(sym_id) = self.resolve_identifier_symbol(tag_name_idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(&decl_idx) = symbol.declarations.first() else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if var_decl.initializer.is_none() {
            return false;
        }
        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(init_node) else {
            return false;
        };

        matches!(
            binary.operator_token,
            x if x == SyntaxKind::BarBarToken as u16 || x == SyntaxKind::QuestionQuestionToken as u16
        )
    }

    fn get_normalized_jsx_required_props_shape(
        &mut self,
        props_type: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
        let resolved_props_type = self.normalize_jsx_required_props_target(props_type);
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved_props_type)
    }

    fn nearest_property_declaration_before(
        &self,
        anchor_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let (anchor_start, _) = self.get_node_span(anchor_idx)?;
        let mut best: Option<(NodeIndex, u32)> = None;

        for symbol in self.ctx.binder.symbols.iter() {
            if symbol.escaped_name != prop_name
                || (symbol.decl_file_idx != u32::MAX
                    && symbol.decl_file_idx != self.ctx.current_file_idx as u32)
            {
                continue;
            }

            let Some(decl_idx) = symbol.primary_declaration() else {
                continue;
            };

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let is_prop_like = if decl_node.kind == SyntaxKind::Identifier as u16 {
                self.ctx
                    .arena
                    .get_extended(decl_idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        parent.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                            || parent.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    })
            } else {
                decl_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
                    || decl_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            };
            if !is_prop_like {
                continue;
            }

            let Some((start, _)) = self.get_node_span(decl_idx) else {
                continue;
            };
            if start >= anchor_start {
                continue;
            }

            if best.is_none_or(|(_, best_start)| start > best_start) {
                best = Some((decl_idx, start));
            }
        }

        best.map(|(decl_idx, _)| decl_idx)
    }

    pub(in crate::checkers_domain::jsx) fn normalize_jsx_function_context_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let type_id = self.resolve_type_for_property_access(type_id);
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && shape.is_method
        {
            return self
                .ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: None,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: false,
                });
        }

        type_id
    }
    /// Fallback: check `IntrinsicAttributes` when component props couldn't be extracted.
    pub(in crate::checkers_domain::jsx) fn check_jsx_intrinsic_attributes_only(
        &mut self,
        component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let intrinsic_attrs_type = self.get_intrinsic_attributes_type();
        let intrinsic_class_attrs_type =
            self.get_intrinsic_class_attributes_type_for_component(component_type);
        if intrinsic_attrs_type.is_none() && intrinsic_class_attrs_type.is_none() {
            return;
        }

        // Collect provided attribute names with types
        let mut provided_attrs: Vec<(String, TypeId)> = Vec::new();
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                if let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                    && let Some(name_node) = self.ctx.arena.get(attr_data.name)
                    && let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                {
                    provided_attrs.push((attr_name, TypeId::ANY));
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                // Spread of `any` covers all properties
                if let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) {
                    let spread_type = self.compute_type_of_node(spread_data.expression);
                    if spread_type == TypeId::ANY {
                        return; // any covers everything
                    }
                }
            }
        }

        if let Some(intrinsic_attrs_type) = intrinsic_attrs_type {
            self.check_missing_required_jsx_props(
                intrinsic_attrs_type,
                &provided_attrs,
                tag_name_idx,
                None,
                None,
            );
        }
        if let Some(intrinsic_class_attrs_type) = intrinsic_class_attrs_type {
            self.check_missing_required_jsx_props(
                intrinsic_class_attrs_type,
                &provided_attrs,
                tag_name_idx,
                None,
                None,
            );
        }
    }
    /// TS2322: Check spread attributes against `IntrinsicAttributes`.
    ///
    /// Covers both SFCs with declared type parameters (e.g. `<T>(props: T) => ...`) and
    /// SFCs that use free type variables from an outer generic (e.g. `function(props: P)`
    /// inside `function test<P>`). tsc emits TS2322 whenever an unconstrained type
    /// parameter spread doesn't satisfy `IntrinsicAttributes`, regardless of whether the
    /// type parameter is declared on the SFC itself or comes from an enclosing scope.
    pub(in crate::checkers_domain::jsx) fn check_generic_sfc_spread_intrinsic_attrs(
        &mut self,
        _component_type: TypeId,
        attributes_idx: NodeIndex,
        tag_name_idx: NodeIndex,
    ) {
        let Some(ia_type) = self.get_intrinsic_attributes_type() else {
            return;
        };

        // Get spread attributes
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                continue;
            }
            let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                continue;
            };
            let spread_type = self.compute_type_of_node(spread_data.expression);

            if spread_type == TypeId::ANY || spread_type == TypeId::ERROR {
                continue;
            }

            // Build target: IntrinsicAttributes & spread_type
            let target = self.ctx.types.factory().intersection2(ia_type, spread_type);

            if !self.is_assignable_to(spread_type, target) {
                let spread_name = self.format_type(spread_type);
                let target_name = format!("IntrinsicAttributes & {spread_name}");
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&spread_name, &target_name],
                );
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    tag_name_idx,
                    &message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }
    }
    /// Grammar check: TS17000 for empty expressions in JSX attributes.
    /// Matches tsc's `checkGrammarJsxElement`: reports only the first empty
    /// expression per JSX opening element, then returns.
    pub(in crate::checkers_domain::jsx) fn check_grammar_jsx_element(
        &mut self,
        attributes_idx: NodeIndex,
    ) {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind != syntax_kind_ext::JSX_ATTRIBUTE {
                continue;
            }
            let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                continue;
            };
            if attr_data.initializer.is_none() {
                continue;
            }
            let Some(init_node) = self.ctx.arena.get(attr_data.initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::JSX_EXPRESSION {
                continue;
            }
            let Some(expr_data) = self.ctx.arena.get_jsx_expression(init_node) else {
                continue;
            };
            // Empty expression {} without spread
            if expr_data.expression.is_none() && !expr_data.dot_dot_dot_token {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    attr_data.initializer,
                    "JSX attributes must only be assigned a non-empty 'expression'.",
                    diagnostic_codes::JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION,
                );
                // tsc returns after the first grammar error per element
                return;
            }
        }
    }
}
