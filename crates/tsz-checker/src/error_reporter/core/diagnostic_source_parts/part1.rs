impl<'a> CheckerState<'a> {
    pub(crate) fn object_literal_initializer_anchor_for_type(
        &mut self,
        object_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<(u32, u32)> {
        let mut current = self.ctx.arena.skip_parenthesized_and_assertions(object_idx);
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 32 {
                return None;
            }

            let node = self.ctx.arena.get(current)?;

            let direct_initializer =
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    Some(prop.initializer)
                } else {
                    self.ctx
                        .arena
                        .get_shorthand_property(node)
                        .map(|prop| prop.name)
                };

            if let Some(initializer_idx) = direct_initializer {
                if let Some(anchor) = self.resolve_diagnostic_anchor(
                    initializer_idx,
                    crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind::Exact,
                ) {
                    return Some((anchor.start, anchor.length));
                }

                let (pos, end) = self.get_node_span(initializer_idx)?;
                return Some(self.normalized_anchor_span(
                    initializer_idx,
                    pos,
                    end.saturating_sub(pos),
                ));
            }

            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                let literal = self.ctx.arena.get_literal_expr(node)?;
                let source_display = self.format_type_for_assignability_message(
                    self.widen_type_for_display(source_type),
                );

                for child_idx in literal.elements.nodes.iter().copied() {
                    let Some(child) = self.ctx.arena.get(child_idx) else {
                        continue;
                    };

                    let candidate_idx =
                        if let Some(prop) = self.ctx.arena.get_property_assignment(child) {
                            prop.initializer
                        } else if let Some(prop) = self.ctx.arena.get_shorthand_property(child) {
                            prop.name
                        } else {
                            continue;
                        };

                    let candidate_type = self.get_type_of_node(candidate_idx);
                    if matches!(candidate_type, TypeId::ERROR | TypeId::UNKNOWN) {
                        continue;
                    }

                    let candidate_display = self.format_type_for_assignability_message(
                        self.widen_type_for_display(candidate_type),
                    );
                    if candidate_type != source_type && candidate_display != source_display {
                        continue;
                    }

                    if let Some(anchor) = self.resolve_diagnostic_anchor(
                        candidate_idx,
                        crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind::Exact,
                    ) {
                        return Some((anchor.start, anchor.length));
                    }

                    let (pos, end) = self.get_node_span(candidate_idx)?;
                    return Some(self.normalized_anchor_span(
                        candidate_idx,
                        pos,
                        end.saturating_sub(pos),
                    ));
                }

                return None;
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = self.ctx.arena.skip_parenthesized_and_assertions(ext.parent);
        }
    }

    pub(in crate::error_reporter) fn direct_diagnostic_source_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Only skip parenthesized expressions, NOT type assertions.
        // For `<foo>({})`, we want the type assertion node (type `foo`),
        // not the inner `{}` expression.
        let expr_idx = self.ctx.arena.skip_parenthesized(anchor_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(return_stmt) = self.ctx.arena.get_return_statement(node)
            && return_stmt.expression.is_some()
        {
            return Some(return_stmt.expression);
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && self.is_assignment_operator(binary.operator_token)
        {
            return None;
        }
        let is_expression_like = matches!(
            node.kind,
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::BINARY_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
        );
        if !is_expression_like {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(expr_idx)?.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return Some(expr_idx);
        };

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(parent_node)
            && self.is_assignment_operator(bin.operator_token)
            && bin.left == expr_idx
        {
            return None;
        }

        if (parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
            || parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT)
            && let Some(for_in_of) = self.ctx.arena.get_for_in_of(parent_node)
            && for_in_of.initializer == expr_idx
        {
            return None;
        }

        if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.ctx.arena.get_property_assignment(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        if parent_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.ctx.arena.get_shorthand_property(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        // Class property names are assignment targets; the initializer is the
        // source expression, and resolving the name can emit false TS2304.
        if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            && let Some(prop) = self.ctx.arena.get_property_decl(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        // Variable declaration names are assignment targets, not source expressions.
        // When TS2322 is anchored at the declared name (e.g. `b` in
        // `const b: typeof A = B`), the source expression is the initializer `B`.
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.ctx.arena.get_variable_declaration(parent_node)
            && decl.name == expr_idx
        {
            return None;
        }

        // Binding-element names are assignment targets; default-value
        // initializers are the source expressions.
        if parent_node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(elem) = self.ctx.arena.get_binding_element(parent_node)
            && elem.name == expr_idx
        {
            return None;
        }

        // JSX attribute names are not source expressions.
        // When TS2322 is anchored at an attribute name (e.g., `x` in `<Comp x={10} />`),
        // the error reporter must not call get_type_of_node on the attribute name
        // identifier, which would trigger TS2304 "Cannot find name".
        if parent_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
            && let Some(attr) = self.ctx.arena.get_jsx_attribute(parent_node)
            && attr.name == expr_idx
        {
            return None;
        }

        Some(expr_idx)
    }

    fn declared_type_annotation_text_for_expression_with_options(
        &self,
        expr_idx: NodeIndex,
        allow_object_shapes: bool,
    ) -> Option<String> {
        let node_text_in_arena = |arena: &tsz_parser::NodeArena, node_idx: NodeIndex| {
            let node = arena.get(node_idx)?;
            let source = arena.source_files.first()?.text.as_ref();
            let start = node.pos as usize;
            let end = node.end as usize;
            if start >= end || end > source.len() {
                return None;
            }
            Some(source[start..end].to_string())
        };
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        // Scope-chain resolution covers values; `node_symbols` recovers
        // declaration-site identifiers such as class property names.
        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let owner_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let fallback_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            owner_binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or(self.ctx.arena)
        };

        let mut declarations: Vec<(NodeIndex, &tsz_parser::NodeArena)> = Vec::new();
        let mut push_declaration = |decl_idx: NodeIndex| {
            if decl_idx.is_none() {
                return;
            }

            let mut pushed = false;
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    let arena = arena.as_ref();
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    let key = (decl_idx, arena as *const tsz_parser::NodeArena);
                    if declarations.iter().all(|(existing_idx, existing_arena)| {
                        (
                            *existing_idx,
                            *existing_arena as *const tsz_parser::NodeArena,
                        ) != key
                    }) {
                        declarations.push((decl_idx, arena));
                    }
                    pushed = true;
                }
            }

            if !pushed && fallback_arena.get(decl_idx).is_some() {
                let key = (decl_idx, fallback_arena as *const tsz_parser::NodeArena);
                if declarations.iter().all(|(existing_idx, existing_arena)| {
                    (
                        *existing_idx,
                        *existing_arena as *const tsz_parser::NodeArena,
                    ) != key
                }) {
                    declarations.push((decl_idx, fallback_arena));
                }
            }
        };

        push_declaration(symbol.value_declaration);
        for &decl_idx in &symbol.declarations {
            push_declaration(decl_idx);
        }

        for (decl_idx, decl_arena) in declarations {
            let decl_idx = if decl_arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            {
                let parent = decl_arena
                    .get_extended(decl_idx)
                    .map(|ext| ext.parent)
                    .unwrap_or(NodeIndex::NONE);
                let parent_node = decl_arena.get(parent);
                if parent.is_some()
                    && parent_node.is_some_and(|node| {
                        decl_arena.get_variable_declaration(node).is_some()
                            || decl_arena.get_parameter(node).is_some()
                    })
                {
                    parent
                } else {
                    decl_idx
                }
            } else {
                decl_idx
            };
            let decl = decl_arena.get(decl_idx)?;
            if let Some(param) = decl_arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                if self.annotation_names_type_query_alias(decl_arena, param.type_annotation) {
                    return None;
                }
                let mut text =
                    node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                    })?;
                let annotation_contains_undefined =
                    type_node_includes_explicit_undefined(decl_arena, param.type_annotation);
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && !annotation_contains_undefined
                {
                    if text.contains("=>") {
                        text = format!("({text}) | undefined");
                    } else {
                        text.push_str(" | undefined");
                    }
                }
                return Some(text);
            }

            if let Some(var_decl) = decl_arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                if self.annotation_names_type_query_alias(decl_arena, var_decl.type_annotation) {
                    return None;
                }
                return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                });
            }

            // tsc shows class-property annotation text in TS2322, not the
            // evaluated type, which may be `() => error` for unresolved names.
            if let Some(prop_decl) = decl_arena.get_property_decl(decl)
                && prop_decl.type_annotation.is_some()
            {
                if self.annotation_names_type_query_alias(decl_arena, prop_decl.type_annotation) {
                    return None;
                }
                return node_text_in_arena(decl_arena, prop_decl.type_annotation).and_then(
                    |text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                    },
                );
            }
        }

        None
    }

    pub(crate) fn declared_type_annotation_text_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, false)
    }

    fn declared_diagnostic_source_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, true)
    }

    fn declared_type_annotation_text_for_symbol_type(
        &self,
        ty: TypeId,
        allow_object_shapes: bool,
    ) -> Option<String> {
        let sym_id = self.ctx.resolve_type_to_symbol_id(ty)?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return None;
        }

        let owner_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let fallback_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            owner_binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or(self.ctx.arena)
        };

        let decl_arena = owner_binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .and_then(|arenas| arenas.first().map(|arena| arena.as_ref()))
            .filter(|arena| arena.get(decl_idx).is_some())
            .unwrap_or(fallback_arena);
        let decl = decl_arena.get(decl_idx)?;

        let node_text_in_arena = |arena: &tsz_parser::NodeArena, node_idx: NodeIndex| {
            let node = arena.get(node_idx)?;
            let source = arena.source_files.first()?.text.as_ref();
            let start = node.pos as usize;
            let end = node.end as usize;
            if start >= end || end > source.len() {
                return None;
            }
            Some(source[start..end].to_string())
        };

        if let Some(param) = decl_arena.get_parameter(decl)
            && param.type_annotation.is_some()
        {
            if self.annotation_names_type_query_alias(decl_arena, param.type_annotation) {
                return None;
            }
            return node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
            });
        }

        if let Some(var_decl) = decl_arena.get_variable_declaration(decl)
            && var_decl.type_annotation.is_some()
        {
            if self.annotation_names_type_query_alias(decl_arena, var_decl.type_annotation) {
                return None;
            }
            return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
            });
        }

        None
    }

    pub(in crate::error_reporter) fn should_prefer_declared_source_annotation_display(
        &mut self,
        expr_idx: NodeIndex,
        expr_type: TypeId,
        annotation_text: &str,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        let annotation = annotation_text.trim();
        if self.declared_source_annotation_names_type_query_alias(expr_idx) {
            return false;
        }
        if annotation.contains("`${") {
            return true;
        }
        if annotation.contains('&') && !annotation.starts_with("keyof ") {
            if self.source_type_contains_number_literal_only_union(expr_type) {
                return false;
            }
            return !annotation.starts_with("null |") && !annotation.starts_with("undefined |");
        }

        let display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_type));
        let formatted = self.format_type_for_assignability_message(display_type);
        if formatted == "unknown"
            && annotation.contains('<')
            && crate::query_boundaries::common::contains_type_parameters(self.ctx.types, expr_type)
        {
            return true;
        }
        // Keep declaration-site function signatures whenever the fallback display
        // has diverged from the annotation. tsc prefers the declared callable
        // surface for source identifiers, especially when the computed display has
        // widened return literals or otherwise normalized the signature.
        if annotation.contains("=>") {
            if annotation.contains("?:") && formatted.contains("| undefined") {
                return false;
            }
            return formatted != annotation;
        }
        let resolved = self.resolve_type_for_property_access(display_type);
        let evaluated = self.judge_evaluate(resolved);
        let resolver =
            tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
        let has_index_signature = resolver.has_index_signature(
            evaluated,
            tsz_solver::objects::index_signatures::IndexKind::String,
        ) || resolver.has_index_signature(
            evaluated,
            tsz_solver::objects::index_signatures::IndexKind::Number,
        );
        if !formatted.starts_with('{') && !has_index_signature {
            return false;
        }

        // Don't use annotation text when it starts with `null` or `undefined` in
        // a union — the computed type formatter correctly reorders null/undefined
        // to the end (matching tsc's display), but annotation text preserves
        // source order which would put them first.
        if (annotation.starts_with("null |") || annotation.starts_with("undefined |"))
            && !annotation.contains('&')
        {
            return false;
        }
        if annotation.contains('&') || !annotation.starts_with('{') {
            return true;
        }

        if annotation.contains('[') && annotation.contains(']') && formatted.contains("__unique_") {
            return true;
        }

        false
    }

    pub(crate) fn format_type_diagnostic_structural(&self, ty: TypeId) -> String {
        let mut formatter =
            tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                .with_def_store(&self.ctx.definition_store)
                .with_diagnostic_mode()
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_display_properties();
        formatter.format(ty).into_owned()
    }

    fn synthesized_object_parent_display_name(&self, ty: TypeId) -> Option<String> {
        use crate::query_boundaries::common::object_shape_id;
        use tsz_binder::symbol_flags;

        let shape_id = object_shape_id(self.ctx.types, ty)?;
        let shape = self.ctx.types.object_shape(shape_id);
        let has_js_ctor_brand = shape.properties.iter().any(|prop| {
            self.ctx
                .types
                .resolve_atom_ref(prop.name)
                .starts_with("__js_ctor_brand_")
        });
        let mut parent_ids = shape.properties.iter().filter_map(|prop| prop.parent_id);
        let parent_sym = parent_ids.next()?;
        if parent_ids.any(|other| other != parent_sym) {
            return None;
        }

        let symbol = self.get_cross_file_symbol(parent_sym)?;
        if !has_js_ctor_brand && !symbol.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS)
        {
            return None;
        }

        Some(symbol.escaped_name.clone())
    }

    pub(crate) fn property_receiver_application_base_name(
        &self,
        type_id: TypeId,
    ) -> Option<String> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, type_id)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(app.base))?;
        let def = self.ctx.definition_store.get(def_id)?;
        Some(self.ctx.types.resolve_atom(def.name))
    }

    pub(crate) fn format_property_receiver_type_for_diagnostic(&mut self, ty: TypeId) -> String {
        if let Some(module_name) = self.ctx.namespace_module_names.get(&ty) {
            return format!(
                "typeof import(\"{}\")",
                strip_module_specifier_extension(module_name)
            );
        }
        let evaluated = self.evaluate_type_for_assignability(ty);
        if evaluated != ty
            && self.named_type_display_name(evaluated).is_some()
            && crate::query_boundaries::common::type_application(self.ctx.types, ty).is_some()
        {
            return self.format_type_for_assignability_message(evaluated);
        }
        let application_display =
            crate::query_boundaries::common::type_application(self.ctx.types, ty)
                .map(|_| ty)
                .or_else(|| {
                    self.ctx.types.get_display_alias(ty).filter(|&alias| {
                        crate::query_boundaries::common::type_application(self.ctx.types, alias)
                            .is_some()
                    })
                });
        if let Some(application_display) = application_display
            && !diagnostic_query::application_base_has_conditional_alias_body(
                self.ctx.types,
                &self.ctx.definition_store,
                application_display,
            )
        {
            let display_ty =
                self.normalize_property_receiver_application_display_type(application_display);
            let preserve_object_args = self
                .property_receiver_application_base_name(display_ty)
                .is_some_and(|name| name == "merge");
            let mut formatter = self
                .ctx
                .create_diagnostic_type_formatter()
                .with_long_property_receiver_display()
                .with_display_properties()
                .with_skip_application_alias_names();
            if !preserve_object_args {
                formatter = formatter.with_long_property_receiver_object_elision_end_depth(192);
            } else {
                formatter = formatter.with_long_property_receiver_object_elision_end_depth(0);
            }
            return Self::truncate_property_receiver_display(
                formatter.format(display_ty).into_owned(),
            );
        }
        let has_object_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty).is_some();
        let has_def = self.ctx.definition_store.find_def_for_type(ty).is_some();
        let has_alias = self
            .ctx
            .definition_store
            .find_type_alias_by_body(ty)
            .is_some();
        let has_namespace_name = self.ctx.namespace_module_names.contains_key(&ty);
        // If this type was produced by evaluating a generic application
        // (e.g., `Omit<this, K>` → `{}`), fall through to
        // `format_type_for_assignability_message` which respects the display_alias
        // mechanism and renders `Omit<this, K>` instead of the structural form.
        let has_display_alias = self.ctx.types.get_display_alias(ty).is_some();
        // Preserve namespace identity (`typeof import("...")`) for CommonJS
        // namespace objects that are represented as anonymous object shapes.
        // Structural widening here drops the namespace tag and expands the full
        // object literal in diagnostics.
        if has_namespace_name {
            return self.format_type_diagnostic(ty);
        }
        if has_object_shape && !has_def && !has_alias && !has_display_alias {
            // Only widen literal properties of *fresh* object literal types
            // (e.g., the type of `{ x: 1 }` expression). Declared object
            // annotations like `let a: { __foo: 10 }` preserve their literal
            // property types in property-access diagnostics, matching tsc.
            let display_ty =
                if crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, ty) {
                    self.widen_fresh_object_literal_properties_for_display(ty)
                } else {
                    ty
                };
            return Self::truncate_property_receiver_display(
                self.format_type_diagnostic_widened(display_ty),
            );
        }
        // Only widen object-like types (to convert literal properties to primitives).
        // For literal/primitive receiver types (e.g., `""`, `42`), tsc preserves the
        // literal in TS2339 messages (e.g., `'""'` not `'string'`).  Unions whose
        // every member is a literal are also preserved (e.g., `"foo" | "bar"`) —
        // widening them to `string` loses discriminative information tsc keeps in
        // property-existence diagnostics.
        let is_literal_or_primitive =
            crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some()
                || crate::query_boundaries::common::is_primitive_type(self.ctx.types, ty);
        let is_union_of_literals = !is_literal_or_primitive
            && crate::query_boundaries::common::union_members(self.ctx.types, ty).is_some_and(
                |members| {
                    !members.is_empty()
                        && members.iter().all(|&m| {
                            crate::query_boundaries::common::literal_value(self.ctx.types, m)
                                .is_some()
                        })
                },
            );
        let ty = if is_literal_or_primitive || is_union_of_literals {
            ty
        } else {
            self.widen_type_for_display(ty)
        };
        let mut assignability_display = self.format_type_for_property_receiver_message(ty);
        if assignability_display.len() > 320 && assignability_display.starts_with("Omit<") {
            assignability_display = self.format_long_property_receiver_type_for_diagnostic(ty);
        }
        let assignability_display = Self::truncate_property_receiver_display(assignability_display);
        if let Some(name) = self.synthesized_object_parent_display_name(ty) {
            let generic_prefix = format!("{name}<");
            if assignability_display.starts_with(&generic_prefix) {
                return assignability_display;
            }
            return name;
        }
        if self.ctx.definition_store.find_def_for_type(ty).is_none()
            && self
                .ctx
                .definition_store
                .find_type_alias_by_body(ty)
                .is_some()
            && !(assignability_display.starts_with("Omit<")
                || assignability_display.starts_with("merge<"))
        {
            return self.format_type_diagnostic_structural(ty);
        }
        assignability_display
    }

    pub(crate) fn preferred_constructor_display_name(&mut self, type_id: TypeId) -> Option<String> {
        let base_name = self.named_type_display_name(type_id)?;
        let is_callable_or_constructible =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
                || crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    type_id,
                )
                .is_some();
        if !is_callable_or_constructible {
            return None;
        }

        let constructor_name = format!("{base_name}Constructor");
        let constructor_type = self.resolve_lib_type_by_name(&constructor_name)?;
        if constructor_type.is_unknown_or_error() {
            return None;
        }

        let source_type = self.widen_type_for_display(type_id);
        let constructor_type = self.widen_type_for_display(constructor_type);
        crate::query_boundaries::assignability::are_types_structurally_identical(
            self.ctx.types,
            &self.ctx,
            source_type,
            constructor_type,
        )
        .then_some(constructor_name)
    }

    /// When a source expression is a property/element access whose value type
    /// is `unique symbol` (e.g. `Symbol.toPrimitive`), tsc renders the
    /// assignability source as `typeof <expr>` rather than widening to
    /// `symbol`. Mirrors that behavior so diagnostics like
    /// "Type 'typeof Symbol.toPrimitive' is not assignable to type 'object'"
    /// match tsc.
    fn typeof_unique_symbol_source_display(&mut self, anchor_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let expr_idx = self.direct_diagnostic_source_expression(anchor_idx)?;
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let expr_type = self.get_type_of_node(expr_idx);
        if !crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, expr_type) {
            return None;
        }
        let text = self.node_text(expr_idx)?;
        // node_text spans the AST node; for trailing-semicolon expressions
        // (e.g. `"" in Symbol.toPrimitive;`) the parsed PropertyAccess can
        // include the `;` byte. tsc strips it before display.
        let text = text.trim().trim_end_matches(';').trim_end().to_string();
        Some(format!("typeof {text}"))
    }

    fn jsdoc_annotated_expression_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = expr_idx;
        loop {
            // Skip JSDoc-derived source display when `current` is the name of a
            // class property declaration whose leading JSDoc `@type` describes
            // the declared (target) type, not an initializer/source expression.
            // Without this guard the property name picks up the property's own
            // `@type` annotation as the "source" string and produces tautological
            // diagnostics like "Type 'boolean' is not assignable to type 'boolean'."
            // for e.g. `/** @type {boolean} */ #foo = 3` where the source is `3`.
            if self
                .ctx
                .arena
                .node_info(current)
                .and_then(|info| self.ctx.arena.get(info.parent))
                .is_some_and(|parent| {
                    matches!(
                        parent.kind,
                        syntax_kind_ext::PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::GET_ACCESSOR
                            | syntax_kind_ext::SET_ACCESSOR
                            | syntax_kind_ext::PROPERTY_DECLARATION
                    )
                })
            {
                return None;
            }
            if let Some(type_id) = self.jsdoc_type_annotation_for_node_direct(current) {
                // When `current` is a CommonJS module-exports assignment (e.g.
                // `/** @type {string} */ module.exports = 0;`), the `@type`
                // describes the declared export type, not the source RHS type.
                // Returning the annotated type as the source display yields
                // "Type 'string' is not assignable to type 'string'" where the
                // RHS is actually a `number`. Skip the rewrite in that case so
                // the real source type (e.g., `number`) is displayed.
                if self.is_jsdoc_declared_target_assignment(current) {
                    return None;
                }
                let display_type = self.widen_function_like_display_type(type_id);
                return Some(self.format_assignability_type_for_message(display_type, target));
            }

            let node = self.ctx.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return None;
            }

            let paren = self.ctx.arena.get_parenthesized(node)?;
            current = paren.expression;
        }
    }

    /// Determine whether `node` is the LHS (or the whole binary expression) of
    /// a CommonJS `module.exports = X` / `exports = X` assignment in a JS file.
    /// For these forms a leading JSDoc `@type` annotation declares the target
    /// type, not the source type, and must not drive source-side display.
    fn is_jsdoc_declared_target_assignment(&self, node: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        if !self.is_js_file() {
            return false;
        }
        let Some(node_data) = self.ctx.arena.get(node) else {
            return false;
        };
        // Resolve the enclosing assignment binary expression.  The JSDoc
        // annotation may have been attached to the wrapping ExpressionStatement,
        // so accept that form too (`/** @type {string} */ module.exports = 0;`).
        let binary_idx = match node_data.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => node,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(stmt) = self.ctx.arena.get_expression_statement(node_data) else {
                    return false;
                };
                stmt.expression
            }
            _ => {
                // If `node` is the LHS of an assignment, walk to the parent.
                let Some(parent_idx) = self
                    .ctx
                    .arena
                    .node_info(node)
                    .map(|info| info.parent)
                    .filter(|idx| idx.is_some())
                else {
                    return false;
                };
                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                    return false;
                };
                if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                    return false;
                }
                parent_idx
            }
        };

        let Some(binary_node) = self.ctx.arena.get(binary_idx) else {
            return false;
        };
        if binary_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(binary_node) else {
            return false;
        };
        if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
            return false;
        }
        if self.is_commonjs_module_exports_assignment(binary.left) {
            return true;
        }
        // Same target-annotation carve-out for `Foo.prototype = X`.
        let n = match self.ctx.arena.get(binary.left) {
            Some(n) => n,
            None => return false,
        };
        if n.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_access_expr(n)
            .and_then(|a| self.ctx.arena.get(a.name_or_argument))
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|i| i.escaped_text == "prototype")
    }

    fn empty_array_literal_source_type_display(&self, expr_idx: NodeIndex) -> Option<String> {
        // Only skip parentheses, not type assertions.  When the source is
        // `[] as Foo`, the diagnostic should display the asserted type `Foo`,
        // not the inner empty array's intrinsic type.  Returning `None` here
        // lets the caller fall through to `get_type_of_node` (or further display
        // policy) which yields the asserted type.  Mirrors the behavior of
        // `object_literal_source_type_display` for `({} as Foo)`.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(node)?;
        if !literal.elements.nodes.is_empty() {
            return None;
        }
        Some(if self.ctx.strict_null_checks() {
            "never[]".to_string()
        } else {
            "undefined[]".to_string()
        })
    }

    pub(crate) fn tuple_structural_source_display(
        &mut self,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let target = self.evaluate_type_for_assignability(target);
        if !crate::query_boundaries::common::is_tuple_type(self.ctx.types, target) {
            return None;
        }
        // Track whether the source is a readonly-wrapped tuple. tsc renders
        // `readonly [...]` for the source side when the value type is
        // `readonly` (e.g. produced by `as const`); without this prefix the
        // assignment-failure message reads `Type '[1]'...` instead of
        // `Type 'readonly [1]'...` for sources whose readonliness is the
        // very property the assignment is failing on.
        // This can evaluate applications/lazy aliases; reuse it so recursive
        // readonly tuple sources do not take the same tuple path twice.
        let source_elements =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, source_type);
        let source_is_readonly_tuple =
            crate::query_boundaries::type_computation::complex::is_readonly_type(
                self.ctx.types,
                source_type,
            ) && source_elements.is_some();
        let elements = source_elements.or_else(|| {
            let evaluated = self.evaluate_type_for_assignability(source_type);
            crate::query_boundaries::common::tuple_elements(self.ctx.types, evaluated)
        })?;
        if elements.is_empty() {
            return None;
        }

        // Single-rest tuples (`[...T[]]`) collapse to the array type `T[]` in
        // tsc's diagnostic display, except when the rest element is a type
        // parameter (which keeps the bracketed `[...T]` form). The canonical
        // type formatter already implements this rule, so defer to it instead
        // of building a per-element display that would produce `[...T[]]`.
        if elements.len() == 1 && elements[0].rest {
            return None;
        }

        let mut parts = Vec::with_capacity(elements.len());
        for element in elements {
            // Rest element `type_id` is the array type itself (e.g.
            // `number[]`), not the element type. The canonical tuple printer
            // renders rest elements as `...{type_id}` — a bare `...` prefix —
            // so do the same here. Wrapping `part` with `[]` produced
            // `...number[][]` instead of `...number[]`.
            let mut part = self.format_type_for_assignability_message(element.type_id);
            if element.optional {
                part.push('?');
            }
            if element.rest {
                part = format!("...{part}");
            }
            parts.push(part);
        }
        let body = format!("[{}]", parts.join(", "));
        if source_is_readonly_tuple {
            Some(format!("readonly {body}"))
        } else {
            Some(body)
        }
    }

    pub(crate) fn object_literal_source_type_display(
        &mut self,
        expr_idx: NodeIndex,
        target: Option<TypeId>,
    ) -> Option<String> {
        // Only skip parentheses, not type assertions.  When the source is
        // `<foo>({})`, the diagnostic should display the asserted type name
        // `foo`, not the inner object literal `{}`.  Returning `None` here
        // lets the caller fall through to `get_type_of_node` which yields
        // the asserted type.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(return_stmt) = self.ctx.arena.get_return_statement(node)
            && return_stmt.expression.is_some()
        {
            return self.object_literal_source_type_display(return_stmt.expression, target);
        }
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let target = target.map(|target| self.evaluate_type_for_assignability(target));
        if let Some(display) =
            self.computed_index_signature_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        let preserve_literal_source_for_normalized_union =
            target.is_some_and(|target| self.target_is_normalized_object_literal_union(target));
        let target_shape = target.and_then(|target| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
        });
        let mut parts = Vec::new();
        let mut contextual_index_key_kind: Option<&'static str> = None;
        let mut contextual_index_value_types = Vec::new();
        let mut all_contextual_index_properties = !literal.elements.nodes.is_empty();
        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let (name_idx, value_idx) = if child.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_property_assignment(child)?;
                (prop.name, prop.initializer)
            } else if child.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_shorthand_property(child)?;
                (prop.name, prop.name)
            } else {
                return None;
            };
            let name_node = self.ctx.arena.get(name_idx)?;
            let display_name = match name_node.kind {
                k if k == tsz_scanner::SyntaxKind::Identifier as u16 => self
                    .ctx
                    .arena
                    .get_identifier(name_node)?
                    .escaped_text
                    .clone(),
                k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    let lit = self.ctx.arena.get_literal(name_node)?;
                    format!("\"{}\"", lit.text)
                }
                k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    self.ctx.arena.get_literal(name_node)?.text.clone()
                }
                k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                    if let Some(name) = self.get_member_name_display_text(name_idx) {
                        name
                    } else {
                        let computed = self.ctx.arena.get_computed_property(name_node)?;
                        let expr = self.node_text(computed.expression)?;
                        format!("[{expr}]", expr = expr.trim())
                    }
                }
                _ => return None,
            };
            let computed_index_kind =
                self.contextual_computed_index_key_kind(name_idx, target_shape.as_deref());
            match (contextual_index_key_kind, computed_index_kind) {
                (None, Some(kind)) => contextual_index_key_kind = Some(kind),
                (Some(existing), Some(kind)) if existing == kind => {}
                _ => all_contextual_index_properties = false,
            }
            let property_name = self
                .get_property_name(name_idx)
                .map(|name| self.ctx.types.intern_string(&name));
            if self
                .ctx
                .arena
                .get(value_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16)
            {
                parts.push(format!("{display_name}: this"));
                continue;
            }
            let value_type = self.get_type_of_node(value_idx);
            if value_type == TypeId::ERROR {
                return None;
            }

            // tsc preserves literal types in fresh object literal error messages
            // when the target property type accepts literals (e.g., discriminated
            // unions: `tag: "A" | "B" | "C"`). Otherwise it widens (e.g., `string`).
            // Check the target property type to decide.
            // When the target is a union (e.g., discriminated union ADT), check
            // each union member's properties for literal acceptance.
            let target_accepts_literal = property_name
                .and_then(|name| {
                    // First try the direct object shape
                    if let Some(shape) = target_shape.as_ref() {
                        return shape
                            .properties
                            .iter()
                            .find(|p| p.name == name)
                            .map(|p| p.type_id);
                    }
                    // For union targets, check each member's properties
                    let target = target?;
                    let members =
                        crate::query_boundaries::common::union_members(self.ctx.types, target)?;
                    for member in &members {
                        if let Some(member_shape) =
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                *member,
                            )
                            && let Some(prop) =
                                member_shape.properties.iter().find(|p| p.name == name)
                            && self.type_contains_string_literal(prop.type_id)
                        {
                            return Some(prop.type_id);
                        }
                    }
                    None
                })
                .is_some_and(|target_prop_type| {
                    self.type_contains_string_literal(target_prop_type)
                });
            if let Some(literal_display) = self.literal_expression_display(value_idx) {
                let preserve_normalized_union_boolean = preserve_literal_source_for_normalized_union
                    && matches!(literal_display.as_str(), "true" | "false");
                if target_accepts_literal || preserve_normalized_union_boolean {
                    parts.push(format!("{display_name}: {literal_display}"));
                    continue;
                }
            }

            // For nested object literals, recurse
            if let Some(nested_display) = self.object_literal_source_type_display(value_idx, None) {
                parts.push(format!("{display_name}: {nested_display}"));
                continue;
            }

            // Fall back to type system for non-literal expressions.
            // For function properties, merge parameter types from target shape.
            let value_display_type = property_name
                .and_then(|name| {
                    let shape = target_shape.as_ref()?;
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == name)
                        .map(|prop| prop.type_id)
                })
                .filter(|target_prop_type| {
                    crate::query_boundaries::diagnostics::function_shape(self.ctx.types, value_type)
                        .is_some()
                        && crate::query_boundaries::diagnostics::function_shape(
                            self.ctx.types,
                            *target_prop_type,
                        )
                        .is_some()
                })
                .and_then(|target_prop_type| {
                    let value_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        value_type,
                    )?;
                    let target_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        target_prop_type,
                    )?;
                    let merged_params: Vec<_> = value_shape
                        .params
                        .iter()
                        .zip(target_shape.params.iter())
                        .map(|(value_param, target_param)| tsz_solver::ParamInfo {
                            type_id: target_param.type_id,
                            ..*value_param
                        })
                        .collect();
                    let merged = self
                        .ctx
                        .types
                        .factory()
                        .function(tsz_solver::FunctionShape {
                            type_params: value_shape.type_params.clone(),
                            params: merged_params,
                            this_type: value_shape.this_type,
                            return_type: value_shape.return_type,
                            type_predicate: value_shape.type_predicate,
                            is_constructor: value_shape.is_constructor,
                            is_method: value_shape.is_method,
                        });
                    Some(merged)
                })
                .unwrap_or(value_type);
            let value_display_type = if target_accepts_literal {
                value_display_type
            } else {
                let widened = self.widen_type_for_display(value_display_type);
                if crate::query_boundaries::common::is_template_literal_type(
                    self.ctx.types,
                    widened,
                ) || crate::query_boundaries::common::is_string_intrinsic_type(
                    self.ctx.types,
                    widened,
                ) {
                    TypeId::STRING
                } else {
                    widened
                }
            };
            let widened_value_display_type =
                self.widen_function_like_display_type(value_display_type);
            let value_display =
                self.format_type_for_assignability_message(widened_value_display_type);
            if computed_index_kind.is_some() {
                contextual_index_value_types.push(widened_value_display_type);
            }
            parts.push(format!("{display_name}: {value_display}"));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        if let Some(index_display) = self.contextual_index_signature_source_display(
            all_contextual_index_properties,
            contextual_index_key_kind,
            contextual_index_value_types,
        ) {
            return Some(index_display);
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    pub(in crate::error_reporter) fn is_literal_sensitive_assignment_target(
        &mut self,
        target: TypeId,
    ) -> bool {
        if crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
            .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        if target == TypeId::UNDEFINED || target == TypeId::NULL {
            return true;
        }
        self.is_literal_sensitive_assignment_target_inner(target)
    }

    /// Check if the target type is a bare type parameter (e.g. `T`).
    /// Used to decide whether to widen literals in error messages:
    /// tsc widens `""` → `string` when the target is a simple type param,
    /// but preserves literals for complex generic targets like `Type[K]`.
    pub(in crate::error_reporter) fn target_is_bare_type_parameter(&self, target: TypeId) -> bool {
        crate::query_boundaries::state::checking::is_type_parameter(self.ctx.types, target)
    }

    fn is_literal_sensitive_assignment_target_inner(&self, target: TypeId) -> bool {
        // NoInfer<T> wraps T without changing its literal nature — unwrap and check inner
        if let Some(inner) =
            crate::query_boundaries::common::no_infer_inner_type(self.ctx.types, target)
        {
            return self.is_literal_sensitive_assignment_target_inner(inner);
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, target).is_some() {
            return true;
        }
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some() {
            return true;
        }
        if crate::query_boundaries::common::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            return true;
        }
        // Template literal types (e.g., `:${string}:`) expect specific string
        // patterns — preserving the source literal in the diagnostic is more
        // informative than showing widened `string`.
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target) {
            return true;
        }
        if let Some(list) = crate::query_boundaries::common::union_list_id(self.ctx.types, target)
            .or_else(|| {
                crate::query_boundaries::common::intersection_list_id(self.ctx.types, target)
            })
        {
            return self
                .ctx
                .types
                .type_list(list)
                .iter()
                .copied()
                .any(|member| self.is_literal_sensitive_assignment_target_inner(member));
        }
        target == TypeId::NEVER
    }

    fn should_widen_enum_member_assignment_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let widened_source = self.widen_enum_member_type(source);
        if widened_source == source {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_none()
            && crate::query_boundaries::common::union_members(self.ctx.types, target).is_none()
            && crate::query_boundaries::common::intersection_members(self.ctx.types, target)
                .is_none()
    }

    pub(in crate::error_reporter) fn unresolved_unused_renaming_property_in_type_query(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<String> {
        let mut saw_type_query = false;
        let mut current = idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                saw_type_query = true;
            }

            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::CALL_SIGNATURE
                    | syntax_kind_ext::CONSTRUCT_SIGNATURE
                    | syntax_kind_ext::METHOD_SIGNATURE
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
            ) {
                if !saw_type_query {
                    return None;
                }
                return self.find_renamed_binding_property_for_name(current, name);
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn find_renamed_binding_property_for_name(
        &self,
        root: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(node)
                && binding.property_name.is_some()
                && binding.name.is_some()
                && self.ctx.arena.get_identifier_text(binding.name) == Some(name)
            {
                let prop_name = self
                    .ctx
                    .arena
                    .get_identifier_text(binding.property_name)
                    .map(str::to_string)?;
                return Some(prop_name);
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }
        None
    }
}
