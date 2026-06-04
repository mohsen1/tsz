impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_exported_class(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
        let extends_alias = self.emit_synthetic_class_extends_alias_if_needed(
            class.name,
            class.heritage_clauses.as_ref(),
            false,
        );

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");
        self.emit_node(class.name);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        } else {
            let jsdoc_template_params =
                self.jsdoc_template_params_for_class_declaration(class_idx, class);
            if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        }

        if let Some(ref heritage) = class.heritage_clauses {
            let jsdoc_extends_type =
                self.jsdoc_extends_type_for_class_declaration(class_idx, class);
            self.emit_class_heritage_clauses(
                heritage,
                extends_alias.as_deref(),
                jsdoc_extends_type.as_deref(),
            );
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| {
                        h.token == SyntaxKind::ExtendsKeyword as u16
                            && h.types.nodes.iter().any(|&type_idx| {
                                !(self.source_is_js_file && self.heritage_type_is_null(type_idx))
                            })
                    })
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        // Suppress method implementations that share a computed name with
        // an accessor (tsc emits only the accessor in .d.ts).
        let shadowed = self.computed_names_shadowed_by_accessors(&class.members);
        self.method_names_with_overloads.extend(shadowed);

        // Emit parameter properties from constructor first (before other members)
        self.emit_parameter_properties(&class.members);

        let delay_private_identifier_marker = self
            .should_delay_private_identifier_marker_for_js_constructor_overloads(&class.members);

        // Emit `#private;` if any member has a private identifier name (e.g., #foo)
        if self.class_has_private_identifier_member(&class.members)
            && !delay_private_identifier_marker
        {
            self.emit_private_identifier_marker();
        }

        self.emit_js_any_base_index_signature_if_needed(class.heritage_clauses.as_ref());
        self.emit_ordered_class_members_with_js_constructor_assignment_properties(&class.members);
        if self.class_has_private_identifier_member(&class.members)
            && delay_private_identifier_marker
        {
            self.emit_private_identifier_marker();
        }
        if self.source_is_js_file {
            self.emit_js_class_define_property_accessors_for_name(class.name);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_exported_function(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Get function name as string for overload tracking
        let function_name = self.get_function_name(func_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = func.body.is_none();
        let is_implementation = !is_overload;
        let should_emit_late_bound_namespace =
            self.should_emit_ts_late_bound_function_namespace(func_idx, func.name, is_overload);

        // Overload handling:
        // - If this is an overload, emit it and mark that this function has overloads
        // - If this is an implementation and the function already has overloads, skip it
        // - If this is an implementation with no overloads, emit it
        if is_overload {
            // Mark that this function name has overload signatures
            if let Some(ref name) = function_name {
                self.function_names_with_overloads.insert(name.clone());
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = function_name
                && self.function_names_with_overloads.contains(name)
            {
                // Skip implementation signature when overloads exist
                return;
            }
        }
        let late_bound_members = self.collect_ts_late_bound_assignment_members(func.name);

        if self.source_is_js_file {
            let jsdoc_overload_signatures = self.jsdoc_overload_signatures_for_node(func_idx);
            if self.emit_jsdoc_overload_function_signatures(
                func_idx,
                true,
                self.should_emit_export_keyword(),
                &jsdoc_overload_signatures,
            ) {
                if should_emit_late_bound_namespace {
                    self.emit_ts_late_bound_function_namespace_from_members(
                        func.name,
                        true,
                        &late_bound_members,
                    );
                }
                self.emit_js_function_like_class_if_needed(
                    func.name,
                    &func.parameters,
                    func.body,
                    true,
                    func_idx,
                );
                self.emit_js_namespace_export_aliases_for_name(func.name, true);
                return;
            }
        }

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(func.name);

        if self.source_is_js_file
            && let Some((type_params, params, return_type)) =
                self.jsdoc_function_type_signature_for_node(func_idx)
        {
            self.emit_jsdoc_function_type_signature(&type_params, &params, &return_type);
            self.write(";");
            self.write_line();
            if should_emit_late_bound_namespace {
                self.emit_ts_late_bound_function_namespace_from_members(
                    func.name,
                    true,
                    &late_bound_members,
                );
            }
            self.emit_js_function_like_class_if_needed(
                func.name,
                &func.parameters,
                func.body,
                true,
                func_idx,
            );
            self.emit_js_namespace_export_aliases_for_name(func.name, true);
            return;
        }

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(func_idx)
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        let func_body = func.body;
        let func_name = func.name;
        let (preferred_return, direct_function_return) =
            self.function_body_return_hint(func, func_body);
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(func_idx) {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(type_text) = func_body
            .is_some()
            .then(|| self.returned_late_bound_function_typeof_text(func_body))
            .flatten()
        {
            self.write(": ");
            self.write(&type_text);
        } else if let Some(type_text) = preferred_return.as_ref()
            && direct_function_return
        {
            let (type_text, _) =
                self.function_return_type_text_for_declaration_scope(func, type_text);
            self.emit_non_portable_function_return_diagnostics(&type_text, func_body, func_name);
            self.write(": ");
            self.write(&type_text);
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                func.name,
                &func.parameters,
            )
        {
            self.emit_non_portable_function_return_diagnostics(
                &return_type_text,
                func_body,
                func_name,
            );
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self.boolean_default_param_return_type_text(func) {
            self.write(": ");
            self.write(&return_type_text);
        } else if func_body.is_some()
            && self.emit_js_returned_define_property_function_type(func_body)
        {
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it from the type cache
            let func_type_id = cache
                .node_types
                .get(&func_idx.0)
                .copied()
                .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name));
            if let Some(func_type_id) = func_type_id {
                if let Some(predicate_text) =
                    self.function_type_predicate_text(func_type_id, func.type_parameters.as_ref())
                {
                    self.write(": ");
                    self.write(&predicate_text);
                } else if let Some(return_type_id) =
                    type_queries::get_return_type(*interner, func_type_id)
                {
                    // If solver returned `any` but the function body clearly returns void,
                    // prefer void (the solver's `any` is a fallback, not an actual inference)
                    if return_type_id == tsz_solver::types::TypeId::ANY
                        && func_body.is_some()
                        && self.body_returns_void(func_body)
                    {
                        self.write(": void");
                    } else if let Some(type_text) = func_body
                        .is_some()
                        .then(|| self.returned_late_bound_function_typeof_text(func_body))
                        .flatten()
                    {
                        self.write(": ");
                        self.write(&type_text);
                    } else if let Some(type_text) = func_body
                        .is_some()
                        .then(|| {
                            self.async_returned_function_initializer_promise_type_text(
                                func, func_body,
                            )
                        })
                        .flatten()
                    {
                        self.write(": ");
                        self.write(&type_text);
                    } else if let Some(type_text) = preferred_return.as_ref()
                        && (direct_function_return
                            || self
                                .should_prefer_source_return_type_text(type_text, return_type_id)
                            || self.source_return_type_is_function_type_param(func, type_text)
                            || self.source_return_type_preserves_function_type_param(
                                func,
                                type_text,
                                return_type_id,
                            ))
                    {
                        let (type_text, _) =
                            self.function_return_type_text_for_declaration_scope(func, type_text);
                        self.emit_non_portable_function_return_diagnostics(
                            &type_text, func_body, func_name,
                        );
                        self.write(": ");
                        self.write(&type_text);
                    } else if self.emit_single_nameable_new_return_type_if_solver_any(
                        func,
                        func_body,
                        func_name,
                        return_type_id,
                    ) {
                    } else {
                        self.write(": ");
                        let printed_type_text =
                            self.inferred_function_return_type_text(func, return_type_id);
                        self.write(&printed_type_text);
                        let emitted_return_expr_diagnostic = self
                            .emit_non_portable_function_return_diagnostics(
                                &printed_type_text,
                                func_body,
                                func_name,
                            );
                        if !emitted_return_expr_diagnostic
                            && let Some(name_text) = self.get_identifier_text(func_name)
                            && let Some(name_node) = self.arena.get(func_name)
                            && let Some(file_path) = self.current_file_path.clone()
                        {
                            self.check_non_portable_type_references(
                                return_type_id,
                                &name_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    }
                } else if func_body.is_some() {
                    let _ = self.emit_body_inferred_function_return_type(
                        func_idx, func, func_body, func_name,
                    );
                }
            } else if func_body.is_some() {
                let _ = self
                    .emit_body_inferred_function_return_type(func_idx, func, func_body, func_name);
            }
        } else if func_body.is_some() {
            let _ =
                self.emit_body_inferred_function_return_type(func_idx, func, func_body, func_name);
        }

        self.write(";");
        self.write_line();
        if should_emit_late_bound_namespace {
            self.emit_ts_late_bound_function_namespace_from_members(
                func.name,
                true,
                &late_bound_members,
            );
        }
        if self.source_is_js_file {
            self.emit_js_function_like_class_if_needed(
                func.name,
                &func.parameters,
                func.body,
                true,
                func_idx,
            );
            self.emit_js_namespace_export_aliases_for_name(func.name, true);
        }
    }

    pub(crate) fn emit_exported_type_alias(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.arena.is_declare(&alias.modifiers) && !self.inside_declare_namespace {
            self.write("declare ");
        }
        self.write("type ");
        self.emit_node(alias.name);

        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type_alias_rhs(alias_idx, alias.type_node);
        self.write(";");
        self.write_line();
    }
}
