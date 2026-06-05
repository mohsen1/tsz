use super::super::DeclarationEmitter;
use super::type_inference::CallableDeclParts;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{CallExprData, Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn node_symbol_from_arena(
        &self,
        binder: &BinderState,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let arena_addr = arena as *const NodeArena as usize;
        binder
            .cross_file_node_symbols
            .get(&arena_addr)
            .and_then(|symbols| symbols.get(&node_idx.0).copied())
            .or_else(|| {
                std::ptr::eq(arena, self.arena).then(|| binder.get_node_symbol(node_idx))?
            })
            .or_else(|| binder.resolve_identifier(arena, node_idx))
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

        if let Some(signature) = source_arena.get_signature(decl_node)
            && let Some(parameters) = signature.parameters.as_ref()
        {
            return Some(CallableDeclParts {
                modifiers: signature.modifiers.as_ref(),
                type_parameters: signature.type_parameters.as_ref(),
                parameters,
                type_annotation: signature.type_annotation,
                body: NodeIndex::NONE,
            });
        }

        None
    }

    /// Resolve a call expression to the canonical callee symbol used for
    /// emitter-side declaration lookups, following the same portability and
    /// import-aliasing chain as the rest of this module. Also returns the
    /// resolved import module specifier when the callee crosses a module
    /// boundary, since several callers need both.
    pub(in crate::declaration_emitter) fn resolve_call_expression_callee_symbol(
        &self,
        callee_expr: NodeIndex,
        raw_sym_id: SymbolId,
        binder: &BinderState,
    ) -> (SymbolId, Option<String>) {
        let imported_module = self
            .imported_value_module_specifier(raw_sym_id, binder)
            .or_else(|| self.imported_value_module_specifier_from_syntax(callee_expr));
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .or_else(|| {
                imported_module.as_deref().and_then(|module_specifier| {
                    self.imported_value_export_symbol_from_syntax(
                        callee_expr,
                        module_specifier,
                        binder,
                    )
                })
            })
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        (sym_id, imported_module)
    }

    pub(in crate::declaration_emitter) fn resolve_declared_call_callee_symbol(
        &self,
        callee_expr: NodeIndex,
        binder: &BinderState,
    ) -> Option<(SymbolId, Option<String>)> {
        if let Some(raw_sym_id) = self.value_reference_symbol(callee_expr) {
            let resolved =
                self.resolve_call_expression_callee_symbol(callee_expr, raw_sym_id, binder);
            if self.symbol_has_callable_declaration(resolved.0) {
                return Some(resolved);
            }
            if let Some(fallback) =
                self.property_access_declared_type_member_symbol(callee_expr, binder)
            {
                return Some(fallback);
            }
            return Some(resolved);
        }

        self.property_access_declared_type_member_symbol(callee_expr, binder)
    }

    fn symbol_has_callable_declaration(&self, sym_id: SymbolId) -> bool {
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            source_arena
                .get(decl_idx)
                .and_then(|decl_node| Self::callable_decl_parts_from_node(source_arena, decl_node))
                .map(|_| true)
        })
        .unwrap_or(false)
    }

    fn property_access_declared_type_member_symbol(
        &self,
        callee_expr: NodeIndex,
        binder: &BinderState,
    ) -> Option<(SymbolId, Option<String>)> {
        let callee_node = self.arena.get(callee_expr)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        let member_name = self.get_identifier_text(access.name_or_argument)?;

        if let Some((_, module_specifier)) = self.call_receiver_default_import_alias(callee_expr)
            && let Some(default_sym) =
                self.export_symbol_from_module_specifier(binder, &module_specifier, "default")
        {
            tracing::trace!(
                target: "tsz_emit_declared_call",
                module_specifier,
                default_sym = default_sym.0,
                member_name,
                "resolved default import alias receiver"
            );
            let default_sym = self
                .resolve_alias_in_source_context(default_sym, binder)
                .unwrap_or(default_sym);
            let default_sym = self.resolve_portability_declaration_symbol(default_sym, binder);
            if let Some(member_sym) =
                self.declared_type_member_symbol(default_sym, &member_name, binder)
            {
                tracing::trace!(
                    target: "tsz_emit_declared_call",
                    member_sym = member_sym.0,
                    "resolved member from default declared type"
                );
                return Some((member_sym, Some(module_specifier)));
            }
            if let Some(member_sym) = self.default_export_target_declared_type_member_symbol(
                default_sym,
                &member_name,
                binder,
            ) {
                tracing::trace!(
                    target: "tsz_emit_declared_call",
                    member_sym = member_sym.0,
                    "resolved member from default export target"
                );
                return Some((member_sym, Some(module_specifier)));
            }
            tracing::trace!(
                target: "tsz_emit_declared_call",
                default_sym = default_sym.0,
                member_name,
                "default import alias receiver had no declared member"
            );
        }

        let base_sym = self.value_reference_symbol(access.expression)?;
        let imported_module = self
            .imported_value_module_specifier(base_sym, binder)
            .or_else(|| self.imported_value_module_specifier_from_syntax(access.expression));
        let base_sym = self.resolve_portability_declaration_symbol(base_sym, binder);
        self.declared_type_member_symbol(base_sym, &member_name, binder)
            .map(|member_sym| (member_sym, imported_module))
    }

    fn default_export_target_declared_type_member_symbol(
        &self,
        default_sym: SymbolId,
        member_name: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        self.with_symbol_declarations(default_sym, |source_arena, decl_idx| {
            let decl_idx = self
                .default_export_identifier_from_arena(source_arena)
                .unwrap_or(decl_idx);
            let target_sym = self
                .node_symbol_from_arena(binder, source_arena, decl_idx)
                .filter(|sym_id| *sym_id != default_sym)
                .or_else(|| {
                    self.identifier_text_from_arena(source_arena, decl_idx)
                        .and_then(|name| binder.symbols.find_by_name(&name))
                        .filter(|sym_id| *sym_id != default_sym)
                })?;
            let target_sym = self
                .resolve_alias_in_source_context(target_sym, binder)
                .unwrap_or(target_sym);
            let target_sym = self.resolve_portability_declaration_symbol(target_sym, binder);
            self.declared_type_member_symbol(target_sym, member_name, binder)
        })
    }

    fn default_export_identifier_from_arena(&self, source_arena: &NodeArena) -> Option<NodeIndex> {
        let source_file = self.arena_source_file(source_arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = source_arena.get(stmt_idx)?;
            let export = source_arena.get_export_decl(stmt_node)?;
            if !export.is_default_export || export.export_clause.is_none() {
                continue;
            }
            let export_clause_node = source_arena.get(export.export_clause)?;
            if export_clause_node.kind == SyntaxKind::Identifier as u16 {
                return Some(export.export_clause);
            }
        }
        None
    }

    fn declared_type_member_symbol(
        &self,
        value_sym: SymbolId,
        member_name: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let type_sym = self.declared_type_symbol_for_symbol(value_sym)?;
        let type_sym = self
            .resolve_portability_import_alias(type_sym, binder)
            .unwrap_or(type_sym);
        let type_sym = self.resolve_portability_declaration_symbol(type_sym, binder);
        if let Some(member_sym) = binder
            .symbols
            .get(type_sym)
            .and_then(|symbol| symbol.members.as_ref())
            .and_then(|members| members.get(member_name))
        {
            return Some(member_sym);
        }
        self.type_member_symbol(type_sym, member_name, binder)
    }

    pub(in crate::declaration_emitter) fn property_access_declared_type_member_return_type_text(
        &self,
        expr_idx: NodeIndex,
        callee_expr: NodeIndex,
        call: &CallExprData,
        explicit_type_args: &[String],
        binder: &BinderState,
    ) -> Option<String> {
        let callee_node = self.arena.get(callee_expr)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(callee_node)?;
        let member_name = self.get_identifier_text(access.name_or_argument)?;
        let base_sym = self.value_reference_symbol(access.expression)?;
        let type_sym = self.declared_type_symbol_for_symbol(base_sym)?;
        let type_sym = self
            .resolve_portability_import_alias(type_sym, binder)
            .unwrap_or(type_sym);
        let type_sym = self.resolve_portability_declaration_symbol(type_sym, binder);

        self.with_symbol_declarations(type_sym, |source_arena, decl_idx| {
            let member_idx =
                self.type_member_decl_from_decl(source_arena, decl_idx, &member_name)?;
            let decl_node = source_arena.get(member_idx)?;
            let callable =
                Self::callable_type_member_decl_parts_from_node(source_arena, decl_node)?;
            if !callable.type_annotation.is_some()
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
                        member_idx,
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
                explicit_type_args,
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
            type_text = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            type_text = Self::ensure_single_line_type_literal_member_semicolon(&type_text);
            let formatted = self.format_reused_call_structural_return_type_text(&type_text);
            Some(
                self.expand_rest_tuple_parameters_in_function_type_text(expr_idx, &formatted)
                    .unwrap_or(formatted),
            )
        })
    }

    fn type_member_symbol(
        &self,
        type_sym: SymbolId,
        member_name: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        self.with_symbol_declarations(type_sym, |source_arena, decl_idx| {
            let member_idx =
                self.type_member_decl_from_decl(source_arena, decl_idx, member_name)?;
            let member_node = source_arena.get(member_idx)?;
            if let Some(signature) = source_arena.get_signature(member_node) {
                return self.symbol_for_declaration_node(
                    binder,
                    source_arena,
                    signature.name,
                    member_idx,
                );
            }
            if let Some(method) = source_arena.get_method_decl(member_node) {
                return self.symbol_for_declaration_node(
                    binder,
                    source_arena,
                    method.name,
                    member_idx,
                );
            }
            None
        })
    }

    fn callable_type_member_decl_parts_from_node<'b>(
        source_arena: &'b NodeArena,
        decl_node: &'b tsz_parser::parser::node::Node,
    ) -> Option<CallableDeclParts<'b>> {
        if let Some(signature) = source_arena.get_signature(decl_node)
            && let Some(parameters) = signature.parameters.as_ref()
        {
            return Some(CallableDeclParts {
                modifiers: signature.modifiers.as_ref(),
                type_parameters: signature.type_parameters.as_ref(),
                parameters,
                type_annotation: signature.type_annotation,
                body: NodeIndex::NONE,
            });
        }
        Self::callable_decl_parts_from_node(source_arena, decl_node)
    }

    fn type_member_decl_from_decl(
        &self,
        source_arena: &NodeArena,
        decl_idx: NodeIndex,
        member_name: &str,
    ) -> Option<NodeIndex> {
        let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
            .unwrap_or(decl_idx);
        let decl_node = source_arena.get(decl_idx)?;
        let mut members = Vec::new();
        if let Some(interface) = source_arena.get_interface(decl_node) {
            members.extend(interface.members.nodes.iter().copied());
        }
        if let Some(class_decl) = source_arena.get_class(decl_node) {
            members.extend(class_decl.members.nodes.iter().copied());
        }
        if let Some(type_alias) = source_arena.get_type_alias(decl_node)
            && let Some(type_node) = source_arena.get(type_alias.type_node)
            && type_node.kind == syntax_kind_ext::TYPE_LITERAL
            && let Some(type_literal) = source_arena.get_type_literal(type_node)
        {
            members.extend(type_literal.members.nodes.iter().copied());
        }

        members.into_iter().find_map(|member_idx| {
            let member_node = source_arena.get(member_idx)?;
            if let Some(signature) = source_arena.get_signature(member_node)
                && self
                    .property_name_text_from_arena(source_arena, signature.name)
                    .as_deref()
                    == Some(member_name)
            {
                return Some(member_idx);
            }
            if let Some(method) = source_arena.get_method_decl(member_node)
                && self
                    .property_name_text_from_arena(source_arena, method.name)
                    .as_deref()
                    == Some(member_name)
            {
                return Some(member_idx);
            }
            None
        })
    }

    fn symbol_for_declaration_node(
        &self,
        binder: &BinderState,
        source_arena: &NodeArena,
        name_idx: NodeIndex,
        decl_idx: NodeIndex,
    ) -> Option<SymbolId> {
        self.node_symbol_from_arena(binder, source_arena, name_idx)
            .or_else(|| self.node_symbol_from_arena(binder, source_arena, decl_idx))
            .or_else(|| {
                binder.symbols.iter().find_map(|symbol| {
                    symbol
                        .declarations
                        .iter()
                        .any(|candidate| *candidate == name_idx || *candidate == decl_idx)
                        .then_some(symbol.id)
                })
            })
    }

    /// Returns true iff the callee's declared return type is a bare reference
    /// to one of its own type parameters, for example `<T>(x: T): T`. Composed
    /// returns like `` `${T}` ``, `T | undefined`, `T[]`, `{ v: T }`, or
    /// `Promise<T>` return false; the initializer form is only safe when the
    /// consumer can recover the result by re-inferring the type parameter from
    /// the literal argument.
    ///
    /// The `type_arguments.is_some_and(...)` guard rejects `T<X>` shapes: a
    /// bare type parameter cannot syntactically carry type arguments, so a
    /// `TypeReference` that does is necessarily an alias or generic, not the
    /// identity reference we accept.
    pub(in crate::declaration_emitter) fn call_expression_returns_bare_type_parameter_reference(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(raw_sym_id) = self.value_reference_symbol(call.expression) else {
            return false;
        };
        let (sym_id, _imported_module) =
            self.resolve_call_expression_callee_symbol(call.expression, raw_sym_id, binder);

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            let return_idx = callable.type_annotation.into_option()?;
            let type_params = callable.type_parameters?;
            let return_node = source_arena.get(source_arena.skip_parenthesized(return_idx))?;
            if return_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return Some(false);
            }
            let type_ref = source_arena.get_type_ref(return_node)?;
            if type_ref
                .type_arguments
                .as_ref()
                .is_some_and(|ta| !ta.nodes.is_empty())
            {
                return Some(false);
            }
            let return_name = self.identifier_text_from_arena(source_arena, type_ref.type_name)?;
            let matched = type_params.nodes.iter().any(|&param_idx| {
                source_arena
                    .get(param_idx)
                    .and_then(|n| source_arena.get_type_parameter(n))
                    .and_then(|tp| self.identifier_text_from_arena(source_arena, tp.name))
                    .is_some_and(|name| name == return_name)
            });
            Some(matched)
        })
        .unwrap_or(false)
    }
}
