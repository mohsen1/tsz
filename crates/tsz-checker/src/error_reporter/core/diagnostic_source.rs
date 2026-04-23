//! Diagnostic source/target expression analysis and formatting.

mod compound_assignment_context;
mod object_literal_targets;

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

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

    fn direct_diagnostic_source_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Only skip parenthesized expressions, NOT type assertions.
        // For `<foo>({})`, we want the type assertion node (type `foo`),
        // not the inner `{}` expression.
        let expr_idx = self.ctx.arena.skip_parenthesized(anchor_idx);
        let node = self.ctx.arena.get(expr_idx)?;
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

        // Class property declaration names are not source expressions.
        // When TS2322 is anchored at the property name (e.g., `y` in `y: string = 42`),
        // the source expression is the initializer, not the name identifier.
        // Without this guard, get_type_of_node on the name triggers identifier
        // resolution → TS2304 "Cannot find name" false positive.
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

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
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
                let mut text =
                    node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                    })?;
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && !text.contains("undefined")
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
                return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                });
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
            return node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
            });
        }

        if let Some(var_decl) = decl_arena.get_variable_declaration(decl)
            && var_decl.type_annotation.is_some()
        {
            return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
            });
        }

        None
    }

    fn should_prefer_declared_source_annotation_display(
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
        if annotation.contains("`${") {
            return true;
        }
        if annotation.contains('&') && !annotation.starts_with("keyof ") {
            return !annotation.starts_with("null |") && !annotation.starts_with("undefined |");
        }

        let display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_type));
        let formatted = self.format_type_for_assignability_message(display_type);
        // Keep declaration-site function signatures whenever the fallback display
        // has diverged from the annotation. tsc prefers the declared callable
        // surface for source identifiers, especially when the computed display has
        // widened return literals or otherwise normalized the signature.
        if annotation.contains("=>") {
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
        annotation.contains('&') || !annotation.starts_with('{')
    }

    fn format_declared_annotation_for_diagnostic(&self, annotation_text: &str) -> String {
        let mut formatted = annotation_text.trim().to_string();
        if formatted.contains(':') {
            formatted = formatted.replace(" }", "; }");
        }
        formatted
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

    pub(crate) fn format_property_receiver_type_for_diagnostic(&mut self, ty: TypeId) -> String {
        if let Some(module_name) = self.ctx.namespace_module_names.get(&ty) {
            return format!("typeof import(\"{module_name}\")");
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
        if let Some(application_display) = application_display {
            let display_ty =
                self.normalize_property_receiver_application_display_type(application_display);
            let mut formatter = self
                .ctx
                .create_diagnostic_type_formatter()
                .with_long_property_receiver_display()
                .with_display_properties()
                .with_skip_application_alias_names();
            return formatter.format(display_ty).into_owned();
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
        let mut assignability_display = self.format_type_for_assignability_message(ty);
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

        let source_display =
            self.format_type_for_assignability_message(self.widen_type_for_display(type_id));
        let constructor_display = self
            .format_type_for_assignability_message(self.widen_type_for_display(constructor_type));
        (source_display == constructor_display).then_some(constructor_name)
    }

    fn jsdoc_annotated_expression_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = expr_idx;
        loop {
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
        self.is_commonjs_module_exports_assignment(binary.left)
    }

    fn empty_array_literal_source_type_display(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
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

    fn object_literal_source_type_display(
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
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let target = target.map(|target| self.evaluate_type_for_assignability(target));
        let target_shape = target.and_then(|target| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
        });
        let mut parts = Vec::new();
        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let prop = self.ctx.arena.get_property_assignment(child)?;
            let name_node = self.ctx.arena.get(prop.name)?;
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
                _ => return None,
            };
            let property_name = self
                .get_property_name(prop.name)
                .map(|name| self.ctx.types.intern_string(&name));
            let value_type = self.get_type_of_node(prop.initializer);
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
            if target_accepts_literal
                && let Some(literal_display) = self.literal_expression_display(prop.initializer)
            {
                parts.push(format!("{display_name}: {literal_display}"));
                continue;
            }

            // For nested object literals, recurse
            if let Some(nested_display) =
                self.object_literal_source_type_display(prop.initializer, None)
            {
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
            let widened_value_display_type =
                self.widen_function_like_display_type(value_display_type);
            let value_display =
                self.format_type_for_assignability_message(widened_value_display_type);
            parts.push(format!("{display_name}: {value_display}"));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    pub(in crate::error_reporter) fn format_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if source == TypeId::UNDEFINED
            && self.ctx.arena.get(anchor_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            })
        {
            return self.format_assignability_type_for_message(source, target);
        }

        // Generic intersection source reduction: when the source is an intersection
        // containing type parameters (e.g., `T & U`), tsc displays the reduced base
        // constraint instead of the raw generic intersection.  For example,
        // `T extends string | number | undefined` and `U extends string | null | undefined`
        // display as `string | undefined` rather than `T & U`.
        //
        // This matches tsc's `getBaseConstraintOfType` behavior for intersection types
        // in error messages.
        if let Some(reduced) = self.generic_intersection_source_display_substitution(source) {
            return self.format_type_for_assignability_message(reduced);
        }

        // For Lazy(DefId) source types representing named interfaces (non-generic),
        // return the interface name directly. This prevents get_type_of_node from
        // resolving the Lazy to its structural form, losing the name (e.g., showing
        // "{ constraint: Constraint<this>; ... }" instead of "Num").
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, source)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::Interface
            && def.type_params.is_empty()
        {
            let name = self.ctx.types.resolve_atom_ref(def.name);
            return name.to_string();
        }

        if let Some(display) = self.jsdoc_annotated_expression_display(anchor_idx, target) {
            return display;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        let in_arith_compound = self.in_arithmetic_compound_assignment_context(anchor_idx);

        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
        {
            return display;
        }
        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if !in_arith_compound
                && self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            let node_is_array_of_source = crate::query_boundaries::common::array_element_type(
                self.ctx.types,
                expr_display_type,
            )
            .is_some_and(|elem| elem == source);
            if node_is_array_of_source {
                return self.format_assignability_type_for_message(source, target);
            }
            let node_is_target_not_source =
                expr_display_type == target && expr_display_type != source;
            let node_type_matches_source =
                expr_display_type != TypeId::ERROR && !node_is_target_not_source;
            if node_type_matches_source {
                let preserve_literal_surface = self.target_preserves_literal_surface(target);
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_display_type,
                        &annotation_text,
                    )
                {
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
                let display_type =
                    if self.should_widen_enum_member_assignment_source(expr_display_type, target) {
                        self.widen_enum_member_type(expr_display_type)
                    } else {
                        expr_display_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                let display_type = if self.is_literal_sensitive_assignment_target(target)
                    || preserve_literal_surface
                {
                    display_type
                } else if crate::query_boundaries::common::keyof_inner_type(
                    self.ctx.types,
                    display_type,
                )
                .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    crate::query_boundaries::common::widen_type(self.ctx.types, display_type)
                };
                if let Some(display) =
                    self.new_expression_nominal_source_display(expr_idx, display_type)
                {
                    return display;
                }
                if crate::query_boundaries::common::array_element_type(self.ctx.types, display_type)
                    == Some(TypeId::UNKNOWN)
                    && let Some(display) = self.call_unknown_array_source_display(expr_idx, target)
                {
                    return display;
                }
                if let Some(display) =
                    self.declared_identifier_source_display(expr_idx, target, expr_display_type)
                {
                    return display;
                }
                if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                    return display;
                }
                return self.format_assignability_type_for_message(display_type, target);
            }

            if node_type_matches_source
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }
        }
        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && display.contains("=>")
            {
                return self.format_annotation_like_type(&display);
            }
            if let Some(display) = self.literal_expression_display(expr_idx)
                && !self.in_arithmetic_compound_assignment_context(anchor_idx)
                && (self.is_literal_sensitive_assignment_target(target)
                    || (self.assignment_source_is_return_expression(anchor_idx)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            target,
                        )
                        && !self.is_property_assignment_initializer(expr_idx)
                        // When the target is a bare type parameter (e.g. T),
                        // tsc widens literals in error messages: "Type 'string'
                        // is not assignable to type 'T'" rather than "Type '\"\"'
                        // is not assignable to type 'T'". Preserve literals only
                        // for complex generic targets like indexed access types.
                        && !self.target_is_bare_type_parameter(target)))
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            let preserve_literal_surface = self.target_preserves_literal_surface(target);
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_display_type,
                    &annotation_text,
                )
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let display_type = if expr_display_type != TypeId::ERROR {
                let widened_expr_type = if preserve_literal_surface {
                    expr_display_type
                } else {
                    self.widen_type_for_display(expr_display_type)
                };
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);
            if let Some(display) =
                self.new_expression_nominal_source_display(expr_idx, display_type)
            {
                return display;
            }
            if crate::query_boundaries::common::array_element_type(self.ctx.types, display_type)
                == Some(TypeId::UNKNOWN)
                && let Some(display) = self.call_unknown_array_source_display(expr_idx, target)
            {
                return display;
            }
            if let Some(display) =
                self.declared_identifier_source_display(expr_idx, target, expr_display_type)
            {
                return display;
            }
            if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                return display;
            }

            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
            {
                return self.format_assignability_type_for_message(display_type, target);
            }

            if expr_type == TypeId::ERROR
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }

            let display_type =
                if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, display_type)
                    .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    display_type
                };
            let formatted = self.format_type_for_assignability_message(display_type);
            let resolved_for_access = self.resolve_type_for_property_access(display_type);
            let resolved = self.judge_evaluate(resolved_for_access);
            let resolver =
                tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
            if !formatted.contains('{')
                && !formatted.contains('[')
                && !formatted.contains('|')
                && !formatted.contains('&')
                && !formatted.contains('<')
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    display_type,
                )
                && (resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::String,
                ) || resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::Number,
                ))
            {
                if let Some(structural) = self.format_structural_indexed_object_type(resolved) {
                    return structural;
                }
                return self.format_type(resolved);
            }
            // For generic type aliases whose conditional body is ambiguous
            // (e.g. `IsArray<T>` where T extends `object`), skip annotation text.
            let eval_for_ambiguous = self.evaluate_type_for_assignability(display_type);
            let is_ambiguous_conditional_alias = self
                .compute_ambiguous_conditional_display(eval_for_ambiguous)
                .is_some();
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && !is_ambiguous_conditional_alias
                && !display.starts_with("keyof ")
                && !display.starts_with("typeof ")
                && !display.contains("[P in ")
                && !display.contains("[K in ")
                // Don't use annotation text for union types — the TypeFormatter
                // reorders null/undefined to the end to match tsc's display.
                // Annotation text preserves the user's original order which
                // differs from tsc's canonical display.
                && (!display.contains(" | ")
                    || Self::display_has_member_literals_assignability(&display))
                // Don't use annotation text when the formatted type includes
                // `| undefined` (added by strictNullChecks for optional params)
                // that the raw annotation text doesn't have. The annotation text
                // reflects the source code literally and misses the semantic
                // `| undefined` injection.
                && (!formatted.contains("| undefined") || display.contains("| undefined"))
            {
                if crate::query_boundaries::common::enum_def_id(self.ctx.types, display_type)
                    .is_some()
                {
                    return self.format_assignability_type_for_message(display_type, target);
                }
                return self.format_annotation_like_type(&display);
            }
            return formatted;
        }

        // Check if source is a single-call-signature callable that tsc displays in
        // arrow syntax. For these, use the TypeFormatter instead of annotation text.
        let source_uses_arrow_syntax =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| {
                    shape.call_signatures.len() == 1
                        && shape.construct_signatures.is_empty()
                        && shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                });
        if !source_uses_arrow_syntax {
            if let Some(annotation_text) =
                self.declared_type_annotation_text_for_symbol_type(source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let evaluated_source = self.evaluate_type_with_env(source);
            if evaluated_source != source
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_symbol_type(evaluated_source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
        }

        self.format_assignability_type_for_message(source, target)
    }

    pub(in crate::error_reporter) fn format_assignment_target_type_for_diagnostic(
        &mut self,
        target: TypeId,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if let Some(contextual_target) =
            self.object_literal_property_contextual_target_for_diagnostic(anchor_idx, target)
        {
            return self.format_object_literal_property_diag_target(contextual_target);
        }

        // When the target is a nullable union (e.g., `T | null | undefined`)
        // and the source is non-nullable, strip null/undefined from the
        // top-level display to match tsc's behavior.
        let display_target = self
            .strip_nullish_for_assignability_display(target, source)
            .unwrap_or(target);

        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);
        if let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && (display.starts_with("keyof ")
                || display.contains("[P in ")
                || display.contains("[K in "))
        {
            // For `typeof EnumName.Member`, tsc evaluates to the enum member type
            // and displays as `EnumName.Member` (without `typeof` prefix). Skip the
            // raw annotation text when the target resolves to an enum member type.
            if display.starts_with("typeof ")
                && crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some()
            {
                // Fall through to use the TypeFormatter, which correctly displays
                // `TypeData::Enum` as qualified `W.a` style names.
            } else if display.starts_with("keyof ") && display_target == target {
                // For `keyof (A | B)` / `keyof (A & B)`, use the TypeFormatter so
                // that distribution rules apply (→ `keyof A & keyof B`).
                // For plain `keyof SomeName`, the annotation text is already correct
                // (tsc shows `keyof A`, not the expanded literal union). Only route
                // through TypeFormatter when the operand contains a union/intersection.
                let operand_text = display.trim_start_matches("keyof ").trim();
                let needs_distribution = operand_text.contains('|')
                    || (operand_text.contains('&') && operand_text.starts_with('('));
                if needs_distribution {
                    return self.format_type_for_assignability_message(display_target);
                }
                return self.format_annotation_like_type(&display);
            } else if display_target == target {
                // Only use annotation text when we didn't strip nullable members;
                // otherwise the annotation includes null/undefined that tsc omits.
                return self.format_annotation_like_type(&display);
            }
        }

        if display_target == target
            && let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
        {
            let preserve_literal_surface = self.target_preserves_literal_surface(source);
            let fallback = if preserve_literal_surface {
                self.format_type_diagnostic(target)
            } else {
                // Use diagnostic mode to avoid synthetic `?: undefined` in unions
                self.format_type_diagnostic_widened(
                    self.widen_fresh_object_literal_properties_for_display(target),
                )
            };
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                return assignability_display;
            }
            // Generic callable targets preserve type alias names from annotations
            let target_is_generic_callable =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .chain(shape.construct_signatures.iter())
                            .any(|sig| !sig.type_params.is_empty())
                    })
                    || crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        target,
                    )
                    .is_some_and(|shape| !shape.type_params.is_empty());
            if target_is_generic_callable {
                return self.format_annotation_like_type(&display);
            }
            if Self::display_has_member_literals_assignability(&display) {
                return self.format_annotation_like_type(&display);
            }
            if Self::display_has_member_literals_assignability(&fallback)
                && !Self::display_has_member_literals_assignability(&display)
            {
                return self.format_annotation_like_type(&display);
            }
            // When the fallback produces duplicate names in a union or tuple
            // (e.g., `Yep | Yep` or `[Yep, Yep]`) but the annotation text preserves
            // namespace-qualified names (e.g., `Foo.Yep | Bar.Yep` or
            // `[Foo.Yep, Bar.Yep]`), prefer the annotation text. This matches tsc's
            // behavior of qualifying types when they'd otherwise be ambiguous.
            if Self::has_duplicate_union_member_names(&fallback)
                && !Self::has_duplicate_union_member_names(&display)
            {
                return self.format_annotation_like_type(&display);
            }
            // When the target is an enum type, format_type() may resolve to
            // an unrelated type name (e.g., a DOM interface that shares the
            // same structural shape). Use the assignability formatter which
            // correctly produces namespace-qualified enum names.
            if crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some() {
                return self.format_assignability_type_for_message(target, source);
            }
            return fallback;
        }

        // When the target is an enum type without annotation text, use the
        // assignability formatter for correct qualified enum name display.
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, display_target).is_some() {
            return self.format_assignability_type_for_message(display_target, source);
        }

        if self.target_preserves_literal_surface(source) {
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            let fallback = self.format_type_diagnostic(display_target);
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                assignability_display
            } else {
                fallback
            }
        } else {
            // Use diagnostic mode to avoid synthetic `?: undefined` in unions
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            let fallback = self.format_type_diagnostic_widened(
                self.widen_fresh_object_literal_properties_for_display(display_target),
            );
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                assignability_display
            } else {
                fallback
            }
        }
    }

    pub(in crate::error_reporter) fn format_nested_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.target_preserves_literal_surface(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                let display_type =
                    if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                        self.widen_enum_member_type(widened_expr_type)
                    } else {
                        widened_expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                if let Some(display) =
                    self.new_expression_nominal_source_display(expr_idx, display_type)
                {
                    return display;
                }
                return self.format_assignability_type_for_message(display_type, target);
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            if self.is_literal_sensitive_assignment_target(target)
                && let Some(display) =
                    self.call_object_literal_intersection_source_display(expr_idx, source, target)
            {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.target_preserves_literal_surface(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);
            if let Some(display) =
                self.new_expression_nominal_source_display(expr_idx, display_type)
            {
                return display;
            }
            return self.format_assignability_type_for_message(display_type, target);
        }

        // Check if source is a single-call-signature callable that tsc displays in
        // arrow syntax. For these, use the TypeFormatter instead of annotation text.
        let source_uses_arrow_syntax =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| {
                    shape.call_signatures.len() == 1
                        && shape.construct_signatures.is_empty()
                        && shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                });
        if !source_uses_arrow_syntax {
            if let Some(annotation_text) =
                self.declared_type_annotation_text_for_symbol_type(source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let evaluated_source = self.evaluate_type_with_env(source);
            if evaluated_source != source
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_symbol_type(evaluated_source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
        }

        self.format_assignability_type_for_message(source, target)
    }

    fn new_expression_nominal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        display_type: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        // When the result type is a union (e.g., `number | Date` from
        // `new unionOfDifferentReturnType(10)` where unionOfDifferentReturnType
        // is `{ new (a: number): number } | { new (a: number): Date }`),
        // TSC shows the actual result type, not the constructor variable name.
        // Return None to let the fallback formatting handle it.
        if crate::query_boundaries::common::union_members(self.ctx.types, display_type).is_some() {
            return None;
        }

        if let Some(new_expr) = self.ctx.arena.get_call_expr(node)
            && let Some(mut ctor_display) = self.expression_text(new_expr.expression)
        {
            if let Some(type_args) = &new_expr.type_arguments
                && !type_args.nodes.is_empty()
            {
                let rendered_args: Vec<String> = type_args
                    .nodes
                    .iter()
                    .map(|&arg| self.get_source_text_for_node(arg))
                    .collect();
                ctor_display.push('<');
                ctor_display.push_str(&rendered_args.join(", "));
                ctor_display.push('>');
                return Some(ctor_display);
            }
            // For generic constructor calls without explicit type args (e.g.
            // `new D()` where `class D<T>`), use the type formatter which
            // respects display_alias to show inferred type params like
            // `D<unknown>`. Without this, the expression text "D" would be
            // returned, losing the inferred type arguments.
            if self.ctx.types.get_display_alias(display_type).is_some() {
                return Some(self.format_type_diagnostic_structural(display_type));
            }
            return Some(ctor_display);
        }

        Some(self.format_property_receiver_type_for_diagnostic(display_type))
    }

    fn call_unknown_array_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;

        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let first_arg_type = self.get_type_of_node(first_arg);
        if matches!(first_arg_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, first_arg_type)
                .or_else(|| {
                    tsz_solver::operations::get_iterator_info(self.ctx.types, first_arg_type, false)
                        .map(|info| info.yield_type)
                })?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let recovered = self
            .ctx
            .types
            .array(self.widen_type_for_display(element_type));
        Some(self.format_assignability_type_for_message(recovered, target))
    }

    fn preferred_evaluated_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let preserve_literal_surface = self.target_preserves_literal_surface(target);
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, source) {
            return Some(self.format_type_diagnostic_structural(source));
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        if evaluated == source || evaluated == TypeId::ERROR {
            return None;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
            || crate::query_boundaries::common::is_template_literal_type(self.ctx.types, evaluated)
            || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some()
        {
            return Some(if preserve_literal_surface {
                self.format_type_diagnostic(evaluated)
            } else {
                self.format_type_diagnostic_structural(evaluated)
            });
        }

        None
    }

    pub(crate) fn target_preserves_literal_surface(&mut self, target: TypeId) -> bool {
        let target = self.evaluate_type_for_assignability(target);

        let has_literal_member = |shape: &tsz_solver::ObjectShape| {
            shape
                .properties
                .iter()
                .any(|prop| self.type_contains_string_literal(prop.type_id))
        };

        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            && has_literal_member(&shape)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            return members.into_iter().any(|member| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
                    .is_some_and(|shape| has_literal_member(&shape))
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target)
        {
            return members.into_iter().any(|member| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, member)
                    .is_some_and(|shape| has_literal_member(&shape))
            });
        }

        false
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

    pub(in crate::error_reporter) fn declared_identifier_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
        expr_display_type: TypeId,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return None;
        }
        // Merged INTERFACE+VALUE (e.g. `Date`): `get_type_of_symbol` returns the interface side, not the value-position constructor.
        if symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
        {
            return None;
        }

        let declared_type = self.get_type_of_symbol(sym_id);
        if matches!(declared_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        let prefer_declared_display = if declared_type == TypeId::ANY
            && expr_display_type != TypeId::ANY
        {
            let mut decl_idx = symbol.value_declaration;
            let mut decl_node = self.ctx.arena.get(decl_idx)?;
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
            {
                decl_idx = ext.parent;
                decl_node = parent_node;
            }
            let is_control_flow_typed_any = self
                .ctx
                .arena
                .get_variable_declaration(decl_node)
                .is_some_and(|decl| {
                    decl.type_annotation.is_none()
                        && !self.ctx.arena.is_const_variable_declaration(decl_idx)
                        && match decl.initializer {
                            idx if idx.is_none() => true,
                            idx => {
                                let inner = self.ctx.arena.skip_parenthesized(idx);
                                inner.is_some()
                                    && self.ctx.arena.get(inner).is_some_and(|node| {
                                        node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
                                            || node.kind
                                                == tsz_scanner::SyntaxKind::UndefinedKeyword as u16
                                            || self.ctx.arena.get_identifier(node).is_some_and(
                                                |ident| ident.escaped_text == "undefined",
                                            )
                                    })
                            }
                        }
                });
            !is_control_flow_typed_any
        } else {
            let expr_is_strictly_narrower = expr_display_type != declared_type
                && self.is_assignable_to(expr_display_type, declared_type)
                && !self.is_assignable_to(declared_type, expr_display_type);
            !expr_is_strictly_narrower
        };

        // If flow narrowing narrowed a nullable union to specifically null or
        // undefined, don't override with the broader declared type. For example,
        // `x: number | null` narrowed to `null` should show
        // "Type 'null' is not assignable to type 'string'", not
        // "Type 'number' is not assignable to type 'string'" (which happens
        // because strip_nullish_for_assignability_display strips the null member
        // when the target is non-nullable, leaving only "number").
        if (expr_display_type == TypeId::NULL || expr_display_type == TypeId::UNDEFINED)
            && expr_display_type != declared_type
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, declared_type)
            && members.contains(&expr_display_type)
        {
            return None;
        }

        if let Some(display) = self.identifier_array_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if let Some(display) = self.identifier_literal_initializer_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if let Some(display) = self.rebuilt_array_source_display(declared_type, target) {
            return Some(display);
        }

        // When the declared type annotation contains literal property types
        // (e.g. `var z: { length: 2; }`), the standard widening path produces
        // `length: number` instead of `length: 2`. tsc preserves the declared
        // literal in the error message.
        // Only applies to declared annotation types (canonical props contain Literal types,
        // no display_properties on `declared_type` itself). Fresh object literals (inferred
        // types from expressions like `var o1 = { one: 1 }`) have display_properties on
        // `declared_type` and must NOT be handled here — the rewrite function widens them.
        // NOTE: check `declared_type` directly (not its evaluated form) because
        // `evaluate_type_with_env` strips display_properties from fresh types, making their
        // evaluated form look like a declared annotation type.
        if prefer_declared_display
            && self
                .ctx
                .types
                .get_display_properties(declared_type)
                .is_none()
        {
            let widened =
                crate::query_boundaries::common::widen_type(self.ctx.types, declared_type);
            if widened != declared_type {
                let literal_display =
                    self.format_assignability_type_for_message(declared_type, target);
                let widened_display = self.format_assignability_type_for_message(widened, target);
                if literal_display != widened_display {
                    return Some(literal_display);
                }
            }
        }

        if prefer_declared_display
            && crate::query_boundaries::common::is_mapped_type(self.ctx.types, declared_type)
        {
            let declared_structural_display = self.format_type_diagnostic(declared_type);
            if declared_structural_display.starts_with('{')
                && !declared_structural_display.contains(" in ")
            {
                let expr_display =
                    self.format_assignability_type_for_message(expr_display_type, target);
                if declared_structural_display != expr_display {
                    return Some(declared_structural_display);
                }
            }
        }

        let declared_display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(declared_type));
        let expr_display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_display_type));
        let declared_is_generic_callable = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            declared_display_type,
        )
        .is_some_and(|shape| {
            shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
                .any(|sig| !sig.type_params.is_empty())
        })
            || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                declared_display_type,
            )
            .is_some_and(|shape| !shape.type_params.is_empty());
        if declared_is_generic_callable
            && let Some(annotation_text) = self.declared_diagnostic_source_annotation_text(expr_idx)
        {
            // Check if this is a single-call-signature callable that tsc displays in
            // arrow syntax (e.g., `<S>() => S[]`). For these, skip annotation text
            // and use the TypeFormatter which correctly produces arrow syntax.
            let should_use_arrow_syntax = crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                declared_display_type,
            )
            .is_some_and(|shape| {
                shape.call_signatures.len() == 1
                    && shape.construct_signatures.is_empty()
                    && shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            });
            if !should_use_arrow_syntax {
                let annotation_display =
                    self.format_declared_annotation_for_diagnostic(&annotation_text);
                let expr_display =
                    self.format_assignability_type_for_message(expr_display_type, target);
                if prefer_declared_display && annotation_display != expr_display {
                    return Some(annotation_display);
                }
            }
        }
        let declared_display = if declared_is_generic_callable {
            let mut formatter =
                tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                    .with_def_store(&self.ctx.definition_store)
                    .with_diagnostic_mode()
                    .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks);
            formatter.format(declared_display_type).into_owned()
        } else {
            self.format_assignability_type_for_message(declared_display_type, target)
        };
        let expr_display = self.format_assignability_type_for_message(expr_display_type, target);

        (prefer_declared_display && declared_display != expr_display).then_some(declared_display)
    }

    pub(in crate::error_reporter) fn rebuilt_array_source_display(
        &mut self,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, source_type)?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        let widened_element =
            self.normalize_assignability_display_type(self.widen_type_for_display(element_type));
        let rebuilt = self.ctx.types.array(widened_element);
        // Preserve the readonly modifier: tsc displays `readonly number[]` not `number[]`
        // when the source type was a readonly array (ReadonlyType(Array(...))).
        let rebuilt = if crate::query_boundaries::type_computation::complex::is_readonly_type(
            self.ctx.types,
            source_type,
        ) {
            self.ctx.types.readonly_type(rebuilt)
        } else {
            rebuilt
        };
        Some(self.format_assignability_type_for_message(rebuilt, target))
    }

    fn call_object_literal_intersection_source_display(
        &mut self,
        expr_idx: NodeIndex,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(node)?;
        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let object_display = self.object_literal_source_type_display(first_arg, Some(target))?;

        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, source_type)?;
        let mut displays = Vec::with_capacity(members.len());
        let mut replaced_object_member = false;

        for &member in members.iter() {
            let evaluated = self.evaluate_type_for_assignability(member);
            let is_object_like_member =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
                    .is_some()
                    || crate::query_boundaries::common::get_merged_object_shape_for_type(
                        self.ctx.types,
                        evaluated,
                    )
                    .is_some();
            if !replaced_object_member && is_object_like_member {
                displays.push(object_display.clone());
                replaced_object_member = true;
            } else {
                displays.push(self.format_assignability_type_for_message(member, target));
            }
        }

        replaced_object_member.then(|| displays.join(" & "))
    }

    pub(in crate::error_reporter) fn has_more_specific_diagnostic_at_span(
        &self,
        start: u32,
        length: u32,
    ) -> bool {
        self.ctx.diagnostics.iter().any(|diag| {
            diag.start == start
                && diag.length == length
                && diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
    }

    pub(crate) fn has_diagnostic_code_within_span(&self, start: u32, end: u32, code: u32) -> bool {
        self.ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == code && diag.start >= start && diag.start < end)
    }

    /// When the source of an assignment is a generic intersection (e.g., `T & U`
    /// where at least one member is a type parameter with a constraint), return
    /// the reduced base-constraint form for display.  Returns `None` when no
    /// reduction applies (source is not an intersection, or no members have
    /// usable constraints, or the reduction yields the same type).
    ///
    /// This matches tsc's `getBaseConstraintOfType` behavior for intersection
    /// types: the base constraint of `T & U` is `constraint(T) & constraint(U)`,
    /// which the interner further simplifies via distribution.
    pub(in crate::error_reporter) fn generic_intersection_source_display_substitution(
        &self,
        source: TypeId,
    ) -> Option<TypeId> {
        let members = crate::query_boundaries::common::intersection_members(
            self.ctx.types.as_type_database(),
            source,
        )?;
        // Only rewrite when at least one member is a bare type parameter with a
        // constraint — otherwise there's no reduction and this would just hide
        // the intersection unnecessarily.
        let has_constrained_type_param = members.iter().any(|&m| {
            crate::query_boundaries::common::type_param_info(self.ctx.types.as_type_database(), m)
                .and_then(|info| info.constraint)
                .is_some()
        });
        if !has_constrained_type_param {
            return None;
        }
        let reduced = crate::query_boundaries::common::get_base_constraint_for_display(
            self.ctx.types.as_type_database(),
            source,
        );
        if reduced == source {
            return None;
        }
        Some(reduced)
    }
}
