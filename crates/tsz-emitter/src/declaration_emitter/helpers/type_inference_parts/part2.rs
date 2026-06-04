impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn value_reference_symbol_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let cache = self.type_cache.as_ref()?;
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        let symbol = binder.symbols.get(resolved_sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };

            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, prop_decl.name])
            {
                let effective_type = if self
                    .arena
                    .has_modifier(&prop_decl.modifiers, SyntaxKind::ReadonlyKeyword)
                {
                    type_id
                } else {
                    self.type_interner
                        .map(|interner| {
                            tsz_solver::operations::widening::widen_literal_type(interner, type_id)
                        })
                        .unwrap_or(type_id)
                };
                return Some(self.print_type_id(effective_type));
            }

            if let Some(accessor) = self.arena.get_accessor(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, accessor.name])
            {
                return Some(self.print_type_id(type_id));
            }
        }

        let type_id = cache.symbol_types.get(&resolved_sym_id).copied()?;
        Some(self.print_type_id(type_id))
    }

    pub(in crate::declaration_emitter) fn local_type_annotation_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let node = self.arena.get(type_idx)?;
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        let slice = text.get(start..end)?.trim();
        (!slice.is_empty()).then(|| slice.to_string())
    }

    pub(in crate::declaration_emitter) fn preferred_annotation_name_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let raw = self.local_type_annotation_text(type_idx)?;
        Self::simple_type_reference_name(&raw).map(|_| raw)
    }

    pub(in crate::declaration_emitter) fn call_expression_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let (sym_id, imported_module) =
            self.resolve_call_expression_callee_symbol(call.expression, raw_sym_id, binder);
        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            let source_file = self.arena_source_file(source_arena)?;
            let is_ambient_function =
                source_file.is_declaration_file || source_arena.is_declare_ref(callable.modifiers);
            let is_source_overload_signature = callable.body.is_none()
                && callable
                    .type_parameters
                    .is_some_and(|params| !params.nodes.is_empty());
            let is_source_with_return_annotation =
                callable.body.is_some() && callable.type_annotation.is_some();
            if imported_module.is_some()
                && !is_ambient_function
                && self
                    .current_file_path
                    .as_deref()
                    .is_some_and(|current_path| {
                        self.paths_refer_to_same_source_file(current_path, &source_file.file_name)
                    })
            {
                return None;
            }
            if (!is_ambient_function
                && !is_source_overload_signature
                && !is_source_with_return_annotation)
                || !callable.type_annotation.is_some()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    callable.parameters,
                    call,
                )
            {
                return None;
            }

            let mut type_text = self
                .source_slice_from_arena(source_arena, callable.type_annotation)
                .or_else(|| {
                    self.emit_type_node_text_from_arena(source_arena, callable.type_annotation)
                })?
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_string();

            let mut type_param_names = Vec::new();
            let mut type_param_substitutions = Vec::new();
            let mut type_param_constraints = Vec::new();
            let mut type_param_fallbacks = Vec::new();
            if let Some(type_params) = callable.type_parameters {
                for &param_idx in &type_params.nodes {
                    if let Some(param_node) = source_arena.get(param_idx)
                        && let Some(param) = source_arena.get_type_parameter(param_node)
                        && let Some(name_text) =
                            self.identifier_text_from_arena(source_arena, param.name)
                    {
                        let fallback = if param.default.is_some() {
                            self.emit_type_node_text_from_arena(source_arena, param.default)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.default)
                                })
                        } else if param.constraint.is_some() {
                            self.emit_type_node_text_from_arena(source_arena, param.constraint)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.constraint)
                                })
                        } else {
                            None
                        };
                        if param.constraint.is_some()
                            && let Some(constraint) = self
                                .emit_type_node_text_from_arena(source_arena, param.constraint)
                                .or_else(|| {
                                    self.source_slice_from_arena(source_arena, param.constraint)
                                })
                        {
                            type_param_constraints.push((name_text.clone(), constraint));
                        }
                        if let Some(fallback) = fallback {
                            type_param_fallbacks.push((name_text.clone(), fallback));
                        }
                        type_param_names.push(name_text);
                    }
                }

                if !explicit_type_args.is_empty() {
                    for (name_text, arg_text) in
                        type_param_names.iter().zip(explicit_type_args.iter())
                    {
                        type_param_substitutions.push((name_text.clone(), arg_text.clone()));
                    }
                } else {
                    type_param_substitutions.extend(
                        self.infer_call_type_param_substitutions_from_arguments(
                            source_arena,
                            callable.parameters,
                            call,
                            &type_param_names,
                            &type_param_constraints,
                        ),
                    );
                    self.clear_conflicting_literal_substitution(
                        source_arena,
                        decl_idx,
                        call,
                        &type_text,
                        &type_param_names,
                        &mut type_param_substitutions,
                    );
                }
                if Self::type_text_contains_mapped_type_literal(&type_text) {
                    self.preserve_literal_mapped_return_type_substitutions(
                        source_arena,
                        callable.parameters,
                        call,
                        &type_param_names,
                        &mut type_param_substitutions,
                    );
                }
            }
            let has_call_site_type_param_substitutions = !type_param_substitutions.is_empty();
            for (name_text, fallback_text) in &type_param_fallbacks {
                if type_param_substitutions
                    .iter()
                    .any(|(substituted, _)| substituted == name_text)
                    || !Self::contains_whole_word_in_text(&type_text, name_text)
                {
                    continue;
                }
                let fallback_text =
                    Self::replace_whole_words_in_text(fallback_text, &type_param_substitutions);
                type_param_substitutions.push((name_text.clone(), fallback_text));
            }
            if explicit_type_args.is_empty()
                && type_param_substitutions.is_empty()
                && type_param_names
                    .iter()
                    .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                return None;
            }
            let mut protected_type_param_names = Vec::new();
            let protected_substitutions = type_param_substitutions
                .iter()
                .enumerate()
                .map(|(substitution_idx, (name_text, arg_text))| {
                    let mut protected_arg_text = arg_text.clone();
                    for (param_idx, param_name) in type_param_names.iter().enumerate() {
                        if !Self::contains_whole_word_in_text(&protected_arg_text, param_name) {
                            continue;
                        }
                        let protected_name =
                            format!("__tszDeclEmitTypeParam{substitution_idx}_{param_idx}__");
                        protected_arg_text = Self::replace_whole_words_in_text(
                            &protected_arg_text,
                            &[(param_name.clone(), protected_name.clone())],
                        );
                        protected_type_param_names.push((protected_name, param_name.clone()));
                    }
                    (name_text.clone(), protected_arg_text)
                })
                .collect::<Vec<_>>();
            type_text = Self::replace_whole_words_in_text(&type_text, &protected_substitutions);
            if type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                return None;
            }
            if !protected_type_param_names.is_empty() {
                type_text =
                    Self::replace_whole_words_in_text(&type_text, &protected_type_param_names);
            }
            type_text = Self::flatten_tuple_spread_substitutions_text(&type_text);
            if let Some(surface_text) = self.call_expression_declared_return_surface_text(
                expr_idx,
                source_arena,
                callable.type_annotation,
                &type_text,
                &explicit_type_args,
                has_call_site_type_param_substitutions,
            ) {
                return Some(surface_text);
            }
            if let Some(expanded) =
                self.event_like_correlated_alias_return_text(source_arena, &type_text, call)
            {
                type_text = expanded;
            } else if let Some(expanded) =
                Self::expand_tuple_item_lookup_mapped_type_text(&type_text)
            {
                type_text = expanded;
            }

            let source_path = self.get_symbol_source_path(sym_id, binder).or_else(|| {
                self.arena_to_path
                    .get(&(source_arena as *const NodeArena as usize))
                    .cloned()
            });
            type_text = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            if let (Some(source_path), Some(module_specifier)) =
                (source_path.as_deref(), imported_module.as_deref())
                && let Some(rewritten) = self.rewrite_typeof_import_default_return_type(
                    source_path,
                    module_specifier,
                    &type_text,
                    binder,
                )
            {
                type_text = rewritten;
            }
            if let Some(module_specifier) = imported_module.as_deref() {
                type_text = self.qualify_ambient_module_exported_names_in_text(
                    source_arena,
                    module_specifier,
                    &type_text,
                    &type_param_names,
                );
                if !Self::type_text_contains_import_type(&type_text)
                    && let Some(root_name) = Self::leading_type_reference_name(&type_text)
                    && !type_param_names.iter().any(|name| name == root_name)
                    && self.imported_module_exports_name(binder, module_specifier, root_name)
                {
                    type_text = format!(
                        "import(\"{module_specifier}\").{}{}",
                        root_name,
                        &type_text[root_name.len()..]
                    );
                }
            }
            if let Some(source_path) = source_path.as_deref() {
                if !Self::type_text_contains_import_type(&type_text) {
                    type_text = self.qualify_foreign_exported_names_in_text(
                        source_arena,
                        source_path,
                        &type_text,
                        &type_param_names,
                    );
                }
                if self
                    .current_file_path
                    .as_deref()
                    .is_some_and(|current_path| {
                        !self.paths_refer_to_same_source_file(current_path, source_path)
                            && type_text.starts_with("typeof ")
                            && !Self::type_text_contains_import_type(&type_text)
                    })
                {
                    return None;
                }
                if self.type_text_contains_unqualified_foreign_value_export(
                    source_arena,
                    source_path,
                    &type_text,
                ) {
                    return None;
                }
            }
            if let (Some(source_path), Some(module_specifier)) =
                (source_path.as_deref(), imported_module.as_deref())
                && self.package_json_name_matches_import_specifier(source_path, module_specifier)
            {
                type_text =
                    Self::rewrite_relative_import_type_specifiers(&type_text, module_specifier);
            }
            type_text = Self::ensure_single_line_type_literal_member_semicolon(&type_text);
            let formatted = self.format_reused_call_structural_return_type_text(&type_text);
            Some(
                self.expand_rest_tuple_parameters_in_function_type_text(expr_idx, &formatted)
                    .unwrap_or(formatted),
            )
        })
    }

    fn format_reused_call_structural_return_type_text(&self, type_text: &str) -> String {
        if !type_text.contains(" & ") || !type_text.contains("=> {") {
            return type_text.to_string();
        }

        let mut out = String::with_capacity(type_text.len() + 16);
        let mut rest = type_text;
        let member_indent = "    ".repeat((self.indent_level + 1) as usize);
        let closing_indent = "    ".repeat(self.indent_level as usize);

        while let Some(start) = rest.find("=> {") {
            let (before, after_marker) = rest.split_at(start + 4);
            out.push_str(before);
            let Some(end) = after_marker.find('}') else {
                out.push_str(after_marker);
                return out;
            };
            let body = after_marker[..end].trim();
            if body.is_empty()
                || body.contains('\n')
                || body.contains(';')
                || body.contains(',')
                || !body.contains(':')
            {
                out.push_str(&after_marker[..=end]);
                rest = &after_marker[end + 1..];
                continue;
            }

            let member = body.trim_end_matches(';').trim();
            out.push('\n');
            out.push_str(&member_indent);
            out.push_str(member);
            out.push(';');
            out.push('\n');
            out.push_str(&closing_indent);
            out.push('}');
            rest = &after_marker[end + 1..];
        }

        out.push_str(rest);
        out
    }

    fn preserve_literal_mapped_return_type_substitutions(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let Some(args) = call.arguments.as_ref() else {
            return;
        };

        for (&param_idx, &arg_idx) in parameters.nodes.iter().zip(args.nodes.iter()) {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            let Some(param_type_text) = self
                .emit_type_node_text_from_arena(source_arena, param.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, param.type_annotation))
            else {
                continue;
            };
            let param_type_text = param_type_text.trim();
            if !type_param_names
                .iter()
                .any(|name| name.as_str() == param_type_text)
            {
                continue;
            }
            let Some(substitution_text) = self
                .enclosing_parameter_type_annotation_text_for_identifier(arg_idx)
                .or_else(|| self.reference_declared_type_annotation_text(arg_idx))
                .filter(|text| Self::simple_type_reference_name(text).is_some())
                .or_else(|| self.const_literal_initializer_text(arg_idx))
            else {
                continue;
            };
            if let Some((_, existing)) = substitutions
                .iter_mut()
                .find(|(name, _)| name.as_str() == param_type_text)
            {
                *existing = substitution_text;
            } else {
                substitutions.push((param_type_text.to_string(), substitution_text));
            }
        }
    }

    fn enclosing_parameter_type_annotation_text_for_identifier(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_name = self.get_identifier_text(arg_idx)?;
        let mut current = arg_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if let Some(func) = self.arena.get_function(parent_node) {
                for &param_idx in &func.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    if self.get_identifier_text(param.name).as_deref() == Some(arg_name.as_str()) {
                        return self
                            .type_annotation_text_from_arena_node(self.arena, param.type_annotation)
                            .or_else(|| {
                                self.source_slice_from_arena(self.arena, param.type_annotation)
                            })
                            .map(|text| text.trim().to_string());
                    }
                }
                return None;
            }
            current = parent_idx;
        }
        None
    }

    fn ensure_single_line_type_literal_member_semicolon(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if trimmed.contains('\n') {
            return type_text.to_string();
        }
        let Some(inner) = trimmed
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            .map(str::trim)
        else {
            return type_text.to_string();
        };
        if inner.is_empty() || inner.ends_with(';') || inner.contains(';') || !inner.contains(':') {
            type_text.to_string()
        } else {
            format!("{{ {inner}; }}")
        }
    }

    pub(in crate::declaration_emitter) fn imported_static_method_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let callee_node = self.arena.get(call.expression)?;
        let access = self.arena.get_access_expr(callee_node)?;
        let receiver_name = self.get_identifier_text(access.expression)?;
        let method_name = self.get_identifier_text(access.name_or_argument)?;
        let imported_module =
            self.imported_value_module_specifier_from_syntax(access.expression)?;
        if imported_module.starts_with('.') || imported_module.starts_with('/') {
            return None;
        }

        let binder = self.binder?;
        let imported_name = self
            .imported_value_export_name_from_syntax(access.expression, &imported_module)
            .unwrap_or(receiver_name);
        let class_sym = self
            .export_symbol_from_module_specifier(binder, &imported_module, &imported_name)
            .or_else(|| {
                self.imported_value_export_symbol_from_syntax(
                    access.expression,
                    &imported_module,
                    binder,
                )
            })
            .or_else(|| {
                let raw_sym_id = self.value_reference_symbol(access.expression)?;
                self.resolve_portability_import_alias(raw_sym_id, binder)
                    .or_else(|| Some(self.resolve_portability_symbol(raw_sym_id, binder)))
            })?;
        let class_sym = self.resolve_portability_symbol(class_sym, binder);
        let explicit_type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());

        let from_symbol = self.with_symbol_declarations(class_sym, |source_arena, decl_idx| {
            let class_decl = Self::class_decl_from_symbol_decl(source_arena, decl_idx)?;
            self.imported_static_method_return_type_from_class_decl(
                binder,
                source_arena,
                class_decl,
                &ImportedMethodRef {
                    imported_module: &imported_module,
                    imported_name: &imported_name,
                    method_name: &method_name,
                },
                call,
                &explicit_type_args,
            )
        });
        from_symbol.or_else(|| {
            self.imported_static_method_return_type_from_named_classes(
                binder,
                &imported_module,
                &imported_name,
                &method_name,
                call,
                &explicit_type_args,
            )
        })
    }

    fn imported_static_method_return_type_from_named_classes(
        &self,
        binder: &BinderState,
        imported_module: &str,
        imported_name: &str,
        method_name: &str,
        call: &tsz_parser::parser::node::CallExprData,
        explicit_type_args: &[String],
    ) -> Option<String> {
        for symbol in binder.symbols.iter() {
            if symbol.escaped_name != imported_name {
                continue;
            }
            let Some(source_arena) = binder
                .symbol_arenas
                .get(&symbol.id)
                .or_else(|| self.global_symbol_arenas.get(&symbol.id))
                .map(|arena| arena.as_ref())
            else {
                continue;
            };
            for decl_idx in symbol.declarations.iter().copied() {
                let Some(class_decl) = Self::class_decl_from_symbol_decl(source_arena, decl_idx)
                else {
                    continue;
                };
                if let Some(type_text) = self.imported_static_method_return_type_from_class_decl(
                    binder,
                    source_arena,
                    class_decl,
                    &ImportedMethodRef {
                        imported_module,
                        imported_name,
                        method_name,
                    },
                    call,
                    explicit_type_args,
                ) {
                    return Some(type_text);
                }
            }
        }

        None
    }

    fn imported_static_method_return_type_from_class_decl(
        &self,
        binder: &BinderState,
        source_arena: &NodeArena,
        class_decl: &tsz_parser::parser::node::ClassData,
        method_ref: &ImportedMethodRef<'_>,
        call: &tsz_parser::parser::node::CallExprData,
        explicit_type_args: &[String],
    ) -> Option<String> {
        let ImportedMethodRef {
            imported_module,
            imported_name,
            method_name,
        } = method_ref;
        for &member_idx in &class_decl.members.nodes {
            let Some(member_node) = source_arena.get(member_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_method_decl(member_node) else {
                continue;
            };
            if !source_arena.is_static(&func.modifiers) {
                continue;
            }
            if self
                .identifier_text_from_arena(source_arena, func.name)
                .as_deref()
                != Some(method_name)
            {
                continue;
            }
            if func.type_annotation.is_none()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    &func.parameters,
                    call,
                )
            {
                continue;
            }

            let mut type_text = self
                .emit_type_node_text_from_arena(source_arena, func.type_annotation)
                .or_else(|| self.source_slice_from_arena(source_arena, func.type_annotation))?
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_string();
            let mut type_param_names = Vec::new();
            let mut type_param_substitutions = Vec::new();
            let mut type_param_fallbacks = Vec::new();
            if let Some(type_params) = func.type_parameters.as_ref() {
                for &param_idx in &type_params.nodes {
                    let Some(param_node) = source_arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = source_arena.get_type_parameter(param_node) else {
                        continue;
                    };
                    let Some(name_text) = self.identifier_text_from_arena(source_arena, param.name)
                    else {
                        continue;
                    };
                    let fallback = if param.default.is_some() {
                        self.emit_type_node_text_from_arena(source_arena, param.default)
                            .or_else(|| self.source_slice_from_arena(source_arena, param.default))
                    } else if param.constraint.is_some() {
                        self.emit_type_node_text_from_arena(source_arena, param.constraint)
                            .or_else(|| {
                                self.source_slice_from_arena(source_arena, param.constraint)
                            })
                    } else {
                        None
                    };
                    if let Some(fallback) = fallback {
                        type_param_fallbacks.push((name_text.clone(), fallback));
                    }
                    type_param_names.push(name_text);
                }
            }
            for (name_text, arg_text) in type_param_names.iter().zip(explicit_type_args.iter()) {
                type_param_substitutions.push((name_text.clone(), arg_text.clone()));
            }
            for (name_text, fallback_text) in &type_param_fallbacks {
                if type_param_substitutions
                    .iter()
                    .any(|(substituted, _)| substituted == name_text)
                    || !Self::contains_whole_word_in_text(&type_text, name_text)
                {
                    continue;
                }
                let fallback_text =
                    Self::replace_whole_words_in_text(fallback_text, &type_param_substitutions);
                type_param_substitutions.push((name_text.clone(), fallback_text));
            }
            type_text = Self::replace_whole_words_in_text(&type_text, &type_param_substitutions);
            if type_param_names
                .iter()
                .any(|name| Self::contains_whole_word_in_text(&type_text, name))
            {
                continue;
            }

            let excluded_names = [imported_name.to_string()];
            return Some(self.qualify_public_package_names_in_text(
                binder,
                imported_module,
                &type_text,
                &excluded_names,
            ));
        }

        None
    }

    fn class_decl_from_symbol_decl(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::ClassData> {
        let class_idx = Self::class_decl_index_from_symbol_decl(arena, decl_idx)?;
        let node = arena.get(class_idx)?;
        arena.get_class(node)
    }

    pub(in crate::declaration_emitter) fn class_decl_index_from_symbol_decl(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = arena.get(current)?;
            if arena.get_class(node).is_some() {
                return Some(current);
            }
            current = arena.parent_of(current)?;
        }

        None
    }

    pub(in crate::declaration_emitter) fn imported_value_module_specifier(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        self.import_symbol_map
            .get(&sym_id)
            .cloned()
            .or_else(|| binder.symbols.get(sym_id)?.import_module.clone())
    }

    pub(in crate::declaration_emitter) fn imported_value_module_specifier_from_syntax(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let local_name = self.get_identifier_text(expr_idx)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && self.get_identifier_text(clause.name).as_deref() == Some(local_name.as_str())
            {
                return Some(module_lit.text.clone());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some()
                    && self.get_identifier_text(bindings.name).as_deref()
                        == Some(local_name.as_str())
                {
                    return Some(module_lit.text.clone());
                }
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref()
                        == Some(local_name.as_str())
                    {
                        return Some(module_lit.text.clone());
                    }
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn imported_value_export_symbol_from_syntax(
        &self,
        expr_idx: NodeIndex,
        module_specifier: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let export_name =
            self.imported_value_export_name_from_syntax(expr_idx, module_specifier)?;
        if let Some(sym_id) = binder
            .module_exports
            .get(module_specifier)
            .and_then(|exports| exports.get(&export_name))
        {
            return Some(sym_id);
        }

        let module_paths = if module_specifier.starts_with('.') || module_specifier.starts_with('/')
        {
            let current_path = self.current_file_path.as_deref()?;
            self.matching_module_export_paths(binder, current_path, module_specifier)
        } else {
            let mut paths: Vec<_> = binder
                .module_exports
                .keys()
                .filter_map(|module_path| {
                    (self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                        || self.node_modules_package_path_matches_import_specifier(
                            module_path,
                            module_specifier,
                        )
                        || self.node_modules_package_contains_import_specifier(
                            module_path,
                            module_specifier,
                        )
                        || self.package_json_name_matches_import_specifier(
                            module_path,
                            module_specifier,
                        ))
                    .then_some(module_path.as_str())
                })
                .collect();
            paths.sort();
            paths
        };
        for module_path in module_paths {
            if let Some(sym_id) = binder
                .module_exports
                .get(module_path)
                .and_then(|exports| exports.get(&export_name))
            {
                return Some(sym_id);
            }
        }

        None
    }

    fn imported_value_export_name_from_syntax(
        &self,
        expr_idx: NodeIndex,
        module_specifier: &str,
    ) -> Option<String> {
        let local_name = self.get_identifier_text(expr_idx)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            if module_lit.text != module_specifier {
                continue;
            }

            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && self.get_identifier_text(clause.name).as_deref() == Some(local_name.as_str())
            {
                return Some("default".to_string());
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some() {
                    continue;
                }
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref()
                        != Some(local_name.as_str())
                    {
                        continue;
                    }
                    return self
                        .get_identifier_text(specifier.property_name)
                        .or_else(|| self.get_identifier_text(specifier.name));
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn node_modules_package_path_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let Some(nm_idx) = components.iter().position(|component| {
            matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
        }) else {
            return false;
        };

        let pkg_start = nm_idx + 1;
        if components.len() == pkg_start + 1
            && let Component::Normal(part) = components[pkg_start]
            && let Some(file_name) = part.to_str()
            && let Some(runtime_path) = self.declaration_runtime_relative_path(file_name)
        {
            let runtime_path = runtime_path.trim_start_matches("./");
            let package_name = runtime_path
                .strip_suffix(".js")
                .unwrap_or(runtime_path)
                .trim_end_matches("/index");
            return module_specifier == package_name;
        }

        let pkg_len = if components.get(pkg_start).is_some_and(|component| {
            matches!(component, Component::Normal(part) if part.to_str().is_some_and(|text| text.starts_with('@')))
        }) {
            2
        } else {
            1
        };
        if components.len() < pkg_start + pkg_len {
            return false;
        }

        let package_name = components[pkg_start..pkg_start + pkg_len]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        let relative_path = components[pkg_start + pkg_len..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        let Some(runtime_subpath) = self.declaration_runtime_relative_path(&relative_path) else {
            return false;
        };
        let mut runtime_subpath = runtime_subpath.trim_start_matches("./").to_string();
        if runtime_subpath.ends_with("/index.js") {
            runtime_subpath.truncate(runtime_subpath.len() - "/index.js".len());
        } else if runtime_subpath == "index.js" {
            runtime_subpath.clear();
        }

        if runtime_subpath.is_empty() {
            module_specifier == package_name
        } else {
            module_specifier == format!("{package_name}/{runtime_subpath}")
        }
    }

    pub(in crate::declaration_emitter) fn imported_module_exports_name(
        &self,
        binder: &BinderState,
        module_specifier: &str,
        export_name: &str,
    ) -> bool {
        if binder
            .module_exports
            .get(module_specifier)
            .is_some_and(|exports| exports.get(export_name).is_some())
        {
            return true;
        }

        if let Some(current_path) = self.current_file_path.as_deref() {
            for module_path in
                self.matching_module_export_paths(binder, current_path, module_specifier)
            {
                if binder
                    .module_exports
                    .get(module_path)
                    .is_some_and(|exports| exports.get(export_name).is_some())
                {
                    return true;
                }
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            return binder.module_exports.iter().any(|(module_path, exports)| {
                (self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                    || self.node_modules_package_path_matches_import_specifier(
                        module_path,
                        module_specifier,
                    )
                    || self.node_modules_package_contains_import_specifier(
                        module_path,
                        module_specifier,
                    ))
                    && exports.get(export_name).is_some()
            });
        }

        false
    }

    pub(in crate::declaration_emitter) fn leading_type_reference_name(
        type_text: &str,
    ) -> Option<&str> {
        let trimmed = type_text.trim_start();
        if Self::type_text_starts_with_import_type(trimmed) || trimmed.starts_with("typeof ") {
            return None;
        }
        let end = trimmed
            .char_indices()
            .find_map(|(idx, ch)| {
                (!(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())).then_some(idx)
            })
            .unwrap_or(trimmed.len());
        if end == 0 {
            return None;
        }
        let name = &trimmed[..end];
        name.chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
            .then_some(name)
    }

    pub(in crate::declaration_emitter) fn type_text_starts_with_string_intrinsic(
        type_text: &str,
    ) -> bool {
        matches!(
            Self::leading_type_reference_name(type_text),
            Some("Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize")
        )
    }

    pub(in crate::declaration_emitter) fn function_signature_accepts_call_arguments(
        &self,
        source_arena: &NodeArena,
        parameters: &NodeList,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> bool {
        let arg_count = call.arguments.as_ref().map_or(0, |args| args.nodes.len());
        let mut required_count = 0usize;
        let mut has_rest = false;

        for &param_idx in &parameters.nodes {
            let Some(param_node) = source_arena.get(param_idx) else {
                continue;
            };
            let Some(param) = source_arena.get_parameter(param_node) else {
                continue;
            };
            has_rest |= param.dot_dot_dot_token;
            if !param.dot_dot_dot_token
                && !param.question_token
                && param.initializer == NodeIndex::NONE
            {
                required_count += 1;
            }
        }

        arg_count >= required_count && (has_rest || arg_count <= parameters.nodes.len())
    }

    pub(in crate::declaration_emitter) fn callable_decl_parts_from_node<'b>(
        source_arena: &'b NodeArena,
        decl_node: &'b Node,
    ) -> Option<CallableDeclParts<'b>> {
        if let Some(func) = source_arena.get_function(decl_node) {
            return Some(CallableDeclParts {
                modifiers: func.modifiers.as_ref(),
                type_parameters: func.type_parameters.as_ref(),
                parameters: &func.parameters,
                type_annotation: func.type_annotation,
                body: func.body,
            });
        }

        if let Some(method) = source_arena.get_method_decl(decl_node) {
            return Some(CallableDeclParts {
                modifiers: method.modifiers.as_ref(),
                type_parameters: method.type_parameters.as_ref(),
                parameters: &method.parameters,
                type_annotation: method.type_annotation,
                body: method.body,
            });
        }

        None
    }

    fn qualify_ambient_module_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        module_specifier: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_ambient_module_export_replacements(
                source_arena,
                stmt_idx,
                module_specifier,
                excluded_names,
                &mut replacements,
            );
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    fn collect_ambient_module_export_replacements(
        &self,
        source_arena: &NodeArena,
        module_idx: NodeIndex,
        module_specifier: &str,
        excluded_names: &[String],
        replacements: &mut Vec<(String, String)>,
    ) {
        let Some(module_node) = source_arena.get(module_idx) else {
            return;
        };
        let Some(module) = source_arena.get_module(module_node) else {
            return;
        };

        let Some(name_node) = source_arena.get(module.name) else {
            return;
        };
        if name_node.kind != SyntaxKind::StringLiteral as u16 {
            return;
        }
        let Some(literal) = source_arena.get_literal(name_node) else {
            return;
        };
        if literal.text != module_specifier {
            return;
        }

        let Some(body_node) = source_arena.get(module.body) else {
            return;
        };
        if source_arena.get_module(body_node).is_some() {
            self.collect_ambient_module_export_replacements(
                source_arena,
                module.body,
                module_specifier,
                excluded_names,
                replacements,
            );
            return;
        }

        let Some(block) = source_arena.get_module_block(body_node) else {
            return;
        };
        let Some(statements) = block.statements.as_ref() else {
            return;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let export_name = if let Some(decl) = source_arena.get_interface(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_class(stmt_node) {
                Some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(stmt_node) {
                Some(decl.name)
            } else {
                source_arena.get_function(stmt_node).map(|decl| decl.name)
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{module_specifier}\").{export_name}");
            replacements.push((export_name, qualified));
        }
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_non_null_and_comma(
        &self,
        mut idx: NodeIndex,
    ) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }
            return idx;
        }
        idx
    }
}
