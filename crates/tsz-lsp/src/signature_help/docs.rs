use super::*;

impl<'a> SignatureHelpProvider<'a> {
    pub(super) fn apply_signature_docs(
        &self,
        signatures: &mut [SignatureCandidate],
        docs: &SignatureDocs,
    ) {
        if signatures.is_empty() || docs.is_empty() {
            return;
        }

        if docs.candidates.len() == 1 {
            let doc = &docs.candidates[0].doc;
            for sig in signatures {
                self.apply_jsdoc_to_signature(sig, doc, true);
            }
            return;
        }

        if docs.candidates.is_empty() {
            if let Some(fallback) = docs.fallback.as_ref() {
                for sig in signatures {
                    self.apply_jsdoc_to_signature(sig, fallback, true);
                }
            }
            return;
        }

        let mut used = vec![false; docs.candidates.len()];
        for sig in signatures {
            if let Some(idx) = Self::match_doc_candidate(sig, &docs.candidates, &mut used) {
                let doc = &docs.candidates[idx].doc;
                self.apply_jsdoc_to_signature(sig, doc, true);
            } else if let Some(fallback) = docs.fallback.as_ref() {
                self.apply_jsdoc_to_signature(sig, fallback, false);
            }
        }
    }

    pub(super) fn apply_jsdoc_to_signature(
        &self,
        sig: &mut SignatureCandidate,
        parsed: &ParsedJsdoc,
        overwrite: bool,
    ) {
        if overwrite || sig.info.documentation.is_none() {
            sig.info.documentation = parsed.summary.clone();
        }

        for (idx, name) in sig.param_names.iter().enumerate() {
            let Some(name) = name else {
                continue;
            };
            let Some(param_doc) = parsed.params.get(name) else {
                continue;
            };
            if let Some(param_info) = sig.info.parameters.get_mut(idx)
                && (overwrite || param_info.documentation.is_none())
            {
                param_info.documentation = Some(param_doc.clone());
            }
        }

        // Copy non-param tags
        if (overwrite || sig.info.tags.is_empty()) && !parsed.tags.is_empty() {
            sig.info.tags = parsed.tags.clone();
        }
    }

    pub(super) fn match_doc_candidate(
        sig: &SignatureCandidate,
        candidates: &[SignatureDocCandidate],
        used: &mut [bool],
    ) -> Option<usize> {
        for (idx, candidate) in candidates.iter().enumerate() {
            if used[idx] {
                continue;
            }
            if candidate.required_params == sig.required_params
                && candidate.total_params == sig.total_params
                && candidate.has_rest == sig.has_rest
            {
                used[idx] = true;
                return Some(idx);
            }
        }
        None
    }

    pub(super) fn signature_documentation_for_symbol(
        &self,
        root: NodeIndex,
        symbol_id: tsz_binder::SymbolId,
        call_kind: CallKind,
    ) -> Option<SignatureDocs> {
        let symbol = self.binder.get_symbol(symbol_id)?;
        let decls = symbol.all_declarations();

        let mut candidates = Vec::new();
        let mut fallback = None;

        for decl in decls {
            if decl.is_none() {
                continue;
            }
            if call_kind == CallKind::New {
                self.collect_constructor_docs_from_class(
                    root,
                    decl,
                    &mut candidates,
                    &mut fallback,
                );
                // For new expressions, only use docs from explicit constructors
                // (collected above), not from the class declaration itself.
                // TypeScript does not propagate class-level JSDoc to implicit constructors.
                continue;
            }
            let doc = jsdoc_for_node(self.arena, root, decl, self.source_text);
            let mut parsed = if doc.is_empty() {
                ParsedJsdoc {
                    summary: None,
                    params: FxHashMap::default(),
                    tags: Vec::new(),
                }
            } else {
                parse_jsdoc(&doc)
            };

            // Merge inline parameter JSDoc comments (e.g. /** comment */ before param)
            let inline_docs = inline_param_jsdocs(self.arena, root, decl, self.source_text);
            for (name, doc) in inline_docs {
                // Inline docs take precedence over @param tags only when @param is absent
                parsed.params.entry(name).or_insert(doc);
            }

            if parsed.is_empty() {
                continue;
            }

            if let Some((required_params, total_params, has_rest)) =
                self.signature_meta_from_decl(decl)
            {
                candidates.push(SignatureDocCandidate {
                    doc: parsed,
                    required_params,
                    total_params,
                    has_rest,
                });
            } else if fallback.is_none() {
                fallback = Some(parsed);
            }
        }

        let docs = SignatureDocs {
            candidates,
            fallback,
        };
        if docs.is_empty() { None } else { Some(docs) }
    }

    pub(super) fn collect_constructor_docs_from_class(
        &self,
        root: NodeIndex,
        decl: NodeIndex,
        candidates: &mut Vec<SignatureDocCandidate>,
        fallback: &mut Option<ParsedJsdoc>,
    ) {
        let Some(node) = self.arena.get(decl) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return;
        };

        for &member in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member) else {
                continue;
            };
            if self.arena.get_constructor(member_node).is_none() {
                continue;
            }

            let doc = jsdoc_for_node(self.arena, root, member, self.source_text);
            let mut parsed = if doc.is_empty() {
                ParsedJsdoc {
                    summary: None,
                    params: FxHashMap::default(),
                    tags: Vec::new(),
                }
            } else {
                parse_jsdoc(&doc)
            };

            // Merge inline parameter JSDoc comments
            let inline_docs = inline_param_jsdocs(self.arena, root, member, self.source_text);
            for (name, doc) in inline_docs {
                parsed.params.entry(name).or_insert(doc);
            }

            if parsed.is_empty() {
                continue;
            }

            if let Some((required_params, total_params, has_rest)) =
                self.signature_meta_from_decl(member)
            {
                candidates.push(SignatureDocCandidate {
                    doc: parsed,
                    required_params,
                    total_params,
                    has_rest,
                });
            } else if fallback.is_none() {
                *fallback = Some(parsed);
            }
        }
    }

    pub(super) fn signature_documentation_for_property_access(
        &self,
        root: NodeIndex,
        access_idx: NodeIndex,
    ) -> Option<SignatureDocs> {
        let access_node = self.arena.get(access_idx)?;
        let access = self.arena.get_access_expr(access_node)?;
        let property_name = self
            .arena
            .get_identifier_text(access.name_or_argument)
            .or_else(|| self.arena.get_literal_text(access.name_or_argument))?;

        let (class_decls, static_only) =
            if let Some(result) = self.class_decls_for_expression(access.expression) {
                result
            } else {
                let decls = self.class_decls_for_property_name_in_file(root, property_name)?;
                (decls, false)
            };
        let mut candidates = Vec::new();
        let mut fallback = None;

        for class_decl in class_decls {
            let Some(class_node) = self.arena.get(class_decl) else {
                continue;
            };
            let Some(class_data) = self.arena.get_class(class_node) else {
                continue;
            };

            for &member in &class_data.members.nodes {
                let Some(member_node) = self.arena.get(member) else {
                    continue;
                };
                let Some(method) = self.arena.get_method_decl(member_node) else {
                    continue;
                };
                let Some(member_name) = self
                    .arena
                    .get_identifier_text(method.name)
                    .or_else(|| self.arena.get_literal_text(method.name))
                else {
                    continue;
                };
                if member_name != property_name {
                    continue;
                }

                let is_static = self.is_static_method(method);
                if static_only && !is_static {
                    continue;
                }
                if !static_only && is_static {
                    continue;
                }

                let doc = jsdoc_for_node(self.arena, root, member, self.source_text);
                let mut parsed = if doc.is_empty() {
                    ParsedJsdoc {
                        summary: None,
                        params: FxHashMap::default(),
                        tags: Vec::new(),
                    }
                } else {
                    parse_jsdoc(&doc)
                };

                // Merge inline parameter JSDoc comments
                let inline_docs = inline_param_jsdocs(self.arena, root, member, self.source_text);
                for (name, doc) in inline_docs {
                    parsed.params.entry(name).or_insert(doc);
                }

                if parsed.is_empty() {
                    continue;
                }

                if let Some((required_params, total_params, has_rest)) =
                    self.signature_meta_from_decl(member)
                {
                    candidates.push(SignatureDocCandidate {
                        doc: parsed,
                        required_params,
                        total_params,
                        has_rest,
                    });
                } else if fallback.is_none() {
                    fallback = Some(parsed);
                }
            }
        }

        let docs = SignatureDocs {
            candidates,
            fallback,
        };
        if docs.is_empty() { None } else { Some(docs) }
    }

    pub(super) fn class_decls_for_expression(
        &self,
        expr: NodeIndex,
    ) -> Option<(Vec<NodeIndex>, bool)> {
        let expr_node = self.arena.get(expr)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_symbol_for_identifier(expr)?;
            return self.class_decls_for_symbol(sym_id);
        }
        if expr_node.kind == syntax_kind_ext::NEW_EXPRESSION {
            let decls = self.class_decls_from_new_expression(expr);
            if !decls.is_empty() {
                return Some((decls, false));
            }
        }
        None
    }

    pub(super) fn class_decls_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<(Vec<NodeIndex>, bool)> {
        let symbol = self.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS != 0 {
            let decls = self.class_decls_from_symbol(sym_id);
            if decls.is_empty() {
                None
            } else {
                Some((decls, true))
            }
        } else if symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::FUNCTION_SCOPED_VARIABLE)
            != 0
        {
            let decls = self.class_decls_from_variable_symbol(symbol);
            if decls.is_empty() {
                None
            } else {
                Some((decls, false))
            }
        } else {
            None
        }
    }

    pub(super) fn class_decls_from_symbol(&self, sym_id: tsz_binder::SymbolId) -> Vec<NodeIndex> {
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return Vec::new();
        };
        let mut class_decls = Vec::new();
        for decl in symbol.all_declarations() {
            if decl.is_none() {
                continue;
            }
            let Some(node) = self.arena.get(decl) else {
                continue;
            };
            if self.arena.get_class(node).is_some() {
                class_decls.push(decl);
            }
        }
        class_decls
    }

    pub(super) fn class_decls_from_variable_symbol(
        &self,
        symbol: &tsz_binder::Symbol,
    ) -> Vec<NodeIndex> {
        let mut decls = Vec::new();
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return decls;
        }
        let Some(node) = self.arena.get(decl_idx) else {
            return decls;
        };
        let Some(var_decl) = self.arena.get_variable_declaration(node) else {
            return decls;
        };
        if var_decl.initializer.is_some() {
            decls.extend(self.class_decls_from_new_expression(var_decl.initializer));
        }
        decls
    }

    pub(super) fn class_decls_from_new_expression(&self, expr: NodeIndex) -> Vec<NodeIndex> {
        let Some(node) = self.arena.get(expr) else {
            return Vec::new();
        };
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return Vec::new();
        }
        let Some(call) = self.arena.get_call_expr(node) else {
            return Vec::new();
        };
        let callee_idx = call.expression;
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return Vec::new();
        };
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return Vec::new();
        }
        let Some(sym_id) = self.resolve_symbol_for_identifier(callee_idx) else {
            return Vec::new();
        };
        self.class_decls_from_symbol(sym_id)
    }

    pub(super) fn class_decls_for_property_name_in_file(
        &self,
        root: NodeIndex,
        property_name: &str,
    ) -> Option<Vec<NodeIndex>> {
        let root_node = self.arena.get(root)?;
        let sf = self.arena.get_source_file(root_node)?;
        let mut matches = Vec::new();

        for &stmt in &sf.statements.nodes {
            let Some(node) = self.arena.get(stmt) else {
                continue;
            };
            let Some(class_data) = self.arena.get_class(node) else {
                continue;
            };
            if self.class_has_method_named(class_data, property_name) {
                matches.push(stmt);
                if matches.len() > 1 {
                    return None;
                }
            }
        }

        if matches.is_empty() {
            None
        } else {
            Some(matches)
        }
    }

    pub(super) fn class_has_method_named(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
        property_name: &str,
    ) -> bool {
        for &member in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member) else {
                continue;
            };
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            let Some(member_name) = self
                .arena
                .get_identifier_text(method.name)
                .or_else(|| self.arena.get_literal_text(method.name))
            else {
                continue;
            };
            if member_name == property_name {
                return true;
            }
        }
        false
    }

    pub(super) fn is_static_method(
        &self,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> bool {
        let Some(modifiers) = method.modifiers.as_ref() else {
            return false;
        };
        for &mod_idx in &modifiers.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                return true;
            }
        }
        false
    }

    pub(super) fn resolve_symbol_for_identifier(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        self.binder
            .resolve_identifier(self.arena, ident_idx)
            .or_else(|| {
                let name = self.arena.get_identifier_text(ident_idx)?;
                self.binder.file_locals.get(name)
            })
            .or_else(|| {
                let name = self.arena.get_identifier_text(ident_idx)?;
                self.binder.get_symbols().find_by_name(name)
            })
    }

    pub(super) fn signature_meta_from_decl(&self, decl: NodeIndex) -> Option<(usize, usize, bool)> {
        let node = self.arena.get(decl)?;
        if let Some(func) = self.arena.get_function(node) {
            return self.signature_meta_from_params(&func.parameters);
        }
        if let Some(method) = self.arena.get_method_decl(node) {
            return self.signature_meta_from_params(&method.parameters);
        }
        if let Some(ctor) = self.arena.get_constructor(node) {
            return self.signature_meta_from_params(&ctor.parameters);
        }
        None
    }

    pub(super) fn signature_meta_from_params(
        &self,
        params: &NodeList,
    ) -> Option<(usize, usize, bool)> {
        let mut required_params = 0;
        let mut total_params = 0;
        let mut has_rest = false;

        for &param_idx in &params.nodes {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if let Some(name_node) = self.arena.get(param_data.name) {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    continue;
                }
                if let Some(ident) = self.arena.get_identifier(name_node)
                    && ident.escaped_text == "this"
                {
                    continue;
                }
            }

            total_params += 1;
            if param_data.dot_dot_dot_token {
                has_rest = true;
                continue;
            }
            if !param_data.question_token && param_data.initializer.is_none() {
                required_params += 1;
            }
        }

        Some((required_params, total_params, has_rest))
    }
}
