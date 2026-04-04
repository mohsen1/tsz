//! Diagnostic source/target expression analysis and formatting.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn is_property_assignment_initializer(&self, anchor_idx: NodeIndex) -> bool {
        let current = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let Some(ext) = self.ctx.arena.get_extended(current) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && self
                .ctx
                .arena
                .get_property_assignment(parent)
                .is_some_and(|prop| prop.initializer == current)
    }

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
        if annotation.contains('&') {
            return !annotation.starts_with("null |") && !annotation.starts_with("undefined |");
        }

        let display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_type));
        let formatted = self.format_type_for_assignability_message(display_type);
        // Keep declaration-site function signatures when the fallback display has
        // collapsed them to an alias name. tsc uses the declared callable surface
        // for lanes like templateLiteralTypes7 rather than a later alias-equivalent
        // name discovered from the shared type body.
        if annotation.contains("=>") && !formatted.contains("=>") {
            return true;
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
        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        formatter.format(ty).into_owned()
    }

    fn synthesized_object_parent_display_name(&self, ty: TypeId) -> Option<String> {
        use tsz_binder::symbol_flags;
        use tsz_solver::type_queries::get_object_shape_id;

        let shape_id = get_object_shape_id(self.ctx.types, ty)?;
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
        if !has_js_ctor_brand
            && (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) == 0
        {
            return None;
        }

        Some(symbol.escaped_name.clone())
    }

    pub(crate) fn format_property_receiver_type_for_diagnostic(&mut self, ty: TypeId) -> String {
        let assignability_display = self.format_type_for_assignability_message(ty);
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

    pub(crate) fn named_type_display_name(&self, type_id: TypeId) -> Option<String> {
        if self.ctx.types.get_display_alias(type_id).is_some() {
            return None;
        }

        if let Some(def_id) = tsz_solver::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }

        if let Some(shape_id) =
            tsz_solver::type_queries::get_object_shape_id(self.ctx.types, type_id)
        {
            let shape = self.ctx.types.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(symbol) = self.get_cross_file_symbol(sym_id)
                && !symbol.escaped_name.is_empty()
            {
                return Some(symbol.escaped_name.clone());
            }
        }

        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
            && !symbol.escaped_name.is_empty()
        {
            return Some(symbol.escaped_name.clone());
        }

        None
    }

    pub(crate) fn preferred_constructor_display_name(&mut self, type_id: TypeId) -> Option<String> {
        let base_name = self.named_type_display_name(type_id)?;
        let is_callable_or_constructible =
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id).is_some()
                || tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id).is_some();
        if !is_callable_or_constructible {
            return None;
        }

        let constructor_name = format!("{base_name}Constructor");
        let constructor_type = self.resolve_lib_type_by_name(&constructor_name)?;
        if matches!(constructor_type, TypeId::UNKNOWN | TypeId::ERROR) {
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
            let target_accepts_literal = property_name
                .and_then(|name| {
                    let shape = target_shape.as_ref()?;
                    shape
                        .properties
                        .iter()
                        .find(|p| p.name == name)
                        .map(|p| p.type_id)
                })
                .is_some_and(|target_prop_type| {
                    self.type_contains_string_literal(target_prop_type)
                });
            if target_accepts_literal {
                if let Some(literal_display) = self.literal_expression_display(prop.initializer) {
                    parts.push(format!("{display_name}: {literal_display}"));
                    continue;
                }
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

        if let Some(display) = self.jsdoc_annotated_expression_display(anchor_idx, target) {
            return display;
        }

        if tsz_solver::literal_value(self.ctx.types, source).is_some()
            && tsz_solver::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source) {
            return display;
        }

        if self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
        {
            return display;
        }
        if self.is_literal_sensitive_assignment_target(target)
            && tsz_solver::literal_value(self.ctx.types, source).is_some()
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if self.is_literal_sensitive_assignment_target(target)
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
            // Only use the node-derived type when it plausibly represents the
            // source of the assignment, not the target.  For-of loops pass the
            // element type as `source` but anchor the diagnostic at the loop
            // variable whose node type equals the *target* (declared variable
            // type), not the source.  When the node type matches the target but
            // not the source, the anchor is the assignment target — skip
            // node-based resolution to avoid confusing "Type 'X' is not
            // assignable to type 'X'" messages.
            let node_is_target_not_source = expr_type == target && expr_type != source;
            let node_type_matches_source = expr_type != TypeId::ERROR && !node_is_target_not_source;
            if node_type_matches_source {
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_type,
                        &annotation_text,
                    )
                {
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
                let display_type =
                    if self.should_widen_enum_member_assignment_source(expr_type, target) {
                        self.widen_enum_member_type(expr_type)
                    } else {
                        expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                let display_type = if self.is_literal_sensitive_assignment_target(target) {
                    display_type
                } else if tsz_solver::keyof_inner_type(self.ctx.types, display_type).is_some() {
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
                return self.format_assignability_type_for_message(display_type, target);
            }

            if node_type_matches_source
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.literal_expression_display(expr_idx)
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
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_type,
                    &annotation_text,
                )
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = self.widen_type_for_display(expr_type);
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

            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
                && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            {
                return self.format_assignability_type_for_message(display_type, target);
            }

            if expr_type == TypeId::ERROR
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }

            let display_type =
                if tsz_solver::keyof_inner_type(self.ctx.types, display_type).is_some() {
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
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && !display.starts_with("keyof ")
                && !display.starts_with("typeof ")
                && !display.contains("[P in ")
                && !display.contains("[K in ")
                // Don't use annotation text for union types — the TypeFormatter
                // reorders null/undefined to the end to match tsc's display.
                // Annotation text preserves the user's original order which
                // differs from tsc's canonical display.
                && !display.contains(" | ")
                // Don't use annotation text when the formatted type includes
                // `| undefined` (added by strictNullChecks for optional params)
                // that the raw annotation text doesn't have. The annotation text
                // reflects the source code literally and misses the semantic
                // `| undefined` injection.
                && (!formatted.contains("| undefined") || display.contains("| undefined"))
            {
                if tsz_solver::type_queries::get_enum_def_id(self.ctx.types, display_type).is_some()
                {
                    return self.format_assignability_type_for_message(display_type, target);
                }
                return self.format_annotation_like_type(&display);
            }
            return formatted;
        }

        self.format_assignability_type_for_message(source, target)
    }

    pub(in crate::error_reporter) fn format_assignment_target_type_for_diagnostic(
        &mut self,
        target: TypeId,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);

        if let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && (display.starts_with("keyof ")
                || display.starts_with("typeof ")
                || display.contains("[P in ")
                || display.contains("[K in "))
        {
            return self.format_annotation_like_type(&display);
        }

        self.format_assignability_type_for_message(target, source)
    }

    pub(in crate::error_reporter) fn format_nested_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if tsz_solver::literal_value(self.ctx.types, source).is_some()
            && tsz_solver::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source) {
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
                let widened_expr_type = self.widen_type_for_display(expr_type);
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

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.is_literal_sensitive_assignment_target(target) {
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
            }
            return Some(ctor_display);
        }

        Some(self.format_property_receiver_type_for_diagnostic(display_type))
    }

    fn preferred_evaluated_source_display(&mut self, source: TypeId) -> Option<String> {
        if tsz_solver::is_template_literal_type(self.ctx.types, source) {
            return Some(self.format_type_diagnostic_structural(source));
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        if evaluated == source || evaluated == TypeId::ERROR {
            return None;
        }

        if tsz_solver::literal_value(self.ctx.types, evaluated).is_some()
            || tsz_solver::is_template_literal_type(self.ctx.types, evaluated)
            || tsz_solver::string_intrinsic_components(self.ctx.types, evaluated).is_some()
        {
            return Some(self.format_type_diagnostic_structural(evaluated));
        }

        None
    }

    pub(in crate::error_reporter) fn is_literal_sensitive_assignment_target(
        &mut self,
        target: TypeId,
    ) -> bool {
        if tsz_solver::string_intrinsic_components(self.ctx.types, target)
            .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
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
        if tsz_solver::literal_value(self.ctx.types, target).is_some() {
            return true;
        }
        if tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target).is_some() {
            return true;
        }
        if tsz_solver::type_queries::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            return true;
        }
        // Template literal types (e.g., `:${string}:`) expect specific string
        // patterns — preserving the source literal in the diagnostic is more
        // informative than showing widened `string`.
        if tsz_solver::is_template_literal_type(self.ctx.types, target) {
            return true;
        }
        if let Some(list) = tsz_solver::union_list_id(self.ctx.types, target)
            .or_else(|| tsz_solver::intersection_list_id(self.ctx.types, target))
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
        tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target).is_none()
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
}
