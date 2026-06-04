impl<'a> CheckerState<'a> {
    fn collect_expando_property_assignment_type(
        &mut self,
        idx: NodeIndex,
        expected_key: &str,
        read_pos: u32,
        best_match: &mut Option<(u32, TypeId)>,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && node.pos < read_pos
            && self
                .expando_assignment_access_key(binary.left)
                .is_some_and(|key| key == expected_key)
            && !Self::is_void_zero_or_undefined_rhs_in_arena(self.ctx.arena, binary.right)
        {
            // In JS/Salsa files, `x.y = void 0` is a property declaration placeholder,
            // not a meaningful type assignment. Skip it so the property type doesn't
            // become `undefined`, which would cause spurious TS18048 diagnostics.
            if !self.js_assignment_rhs_is_void_zero(binary.right) {
                let rhs_idx = Self::checked_js_constructor_initializer_expression(
                    self.ctx.arena,
                    binary.left,
                )
                .unwrap_or_else(|| self.terminal_expando_assignment_rhs(binary.right));
                let rhs_type = self.get_type_of_node(rhs_idx);
                if rhs_type != TypeId::ANY
                    && rhs_type != TypeId::ERROR
                    && rhs_type != TypeId::UNDEFINED
                    && best_match.is_none_or(|(best_pos, _)| node.pos >= best_pos)
                {
                    *best_match = Some((node.pos, rhs_type));
                }
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_expando_property_assignment_type(
                child_idx,
                expected_key,
                read_pos,
                best_match,
            );
        }
    }

    fn terminal_expando_assignment_rhs(&self, idx: NodeIndex) -> NodeIndex {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.terminal_expando_assignment_rhs(binary.right);
        }
        idx
    }

    fn expando_assignment_access_key(&mut self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone()),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let left = self.expando_assignment_access_key(access.expression)?;
                let right = self.ctx.arena.get_identifier_at(access.name_or_argument)?;
                Some(format!("{left}.{}", right.escaped_text))
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let left = self.expando_assignment_access_key(access.expression)?;
                let right = self.expando_element_key_name(access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    pub(in crate::types_domain) fn expando_property_read_before_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        if self.property_access_is_write_target_or_base(property_access_idx) {
            return false;
        }
        if self.expando_read_is_self_default_initializer(property_access_idx) {
            return false;
        }
        if self.is_current_file_commonjs_export_base_for_expando(object_expr_idx) {
            if !self.is_js_file() || !self.ctx.compiler_options.check_js {
                return false;
            }
            return self.commonjs_export_read_before_assignment(property_access_idx, property_name);
        }
        if !self.expando_read_is_within_initializing_scope(property_access_idx, object_expr_idx) {
            return false;
        }
        if !self.is_expando_capable_read_root(object_expr_idx, property_name) {
            return false;
        }

        if let Some(file_idx) = self.expando_root_js_file_idx(object_expr_idx)
            && file_idx != self.ctx.current_file_idx
        {
            return false;
        }

        let Some(flow_node) = self.flow_node_for_reference_usage(property_access_idx) else {
            return false;
        };

        !self
            .flow_analyzer_for_property_reads()
            .is_definitely_assigned(property_access_idx, flow_node)
    }

    fn is_expando_capable_read_root(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        self.is_expando_property_read(object_expr_idx, property_name)
            || ((self.is_js_file() && self.ctx.compiler_options.check_js)
                && self.is_js_prototype_read_root(object_expr_idx, property_name))
    }

    pub(in crate::types_domain) fn current_file_commonjs_export_member_name(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.clone())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.commonjs_static_member_name_for_expando(access.name_or_argument)
            }
            _ => None,
        }
    }

    fn is_current_file_commonjs_export_base_for_expando(&self, idx: NodeIndex) -> bool {
        if self
            .ctx
            .js_export_surface_cache
            .get(&self.ctx.current_file_idx)
            .and_then(|surface| surface.direct_export_type)
            .is_some_and(|direct_export_type| {
                !crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                    self.ctx.types,
                    direct_export_type,
                )
            })
        {
            return false;
        }

        self.is_current_file_commonjs_export_base_syntax(idx)
    }

    fn is_current_file_commonjs_export_base_syntax(&self, idx: NodeIndex) -> bool {
        if self.current_source_file_has_esm_syntax() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.is_unshadowed_commonjs_exports_identifier(idx);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.is_unshadowed_commonjs_module_identifier(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn commonjs_static_member_name_for_expando(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    fn commonjs_export_read_before_assignment(
        &self,
        property_access_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(read_node) = self.ctx.arena.get(property_access_idx) else {
            return false;
        };
        let read_pos = read_node.pos;
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        let mut assigned_before = false;
        let mut assigned_after = false;
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_commonjs_export_assignment_order(
                stmt_idx,
                property_name,
                read_pos,
                &mut assigned_before,
                &mut assigned_after,
            );
            if assigned_before && assigned_after {
                break;
            }
        }

        assigned_after && !assigned_before
    }

    fn collect_commonjs_export_assignment_order(
        &self,
        idx: NodeIndex,
        property_name: &str,
        read_pos: u32,
        assigned_before: &mut bool,
        assigned_after: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if self.is_scope_owner_kind(node.kind) || node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return;
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && let Some(name) = self.commonjs_export_assignment_name(binary.left)
            && name == property_name
        {
            if node.pos < read_pos {
                *assigned_before = true;
            } else if node.pos > read_pos {
                *assigned_after = true;
            }
        }

        for child_idx in self.ctx.arena.get_children(idx) {
            self.collect_commonjs_export_assignment_order(
                child_idx,
                property_name,
                read_pos,
                assigned_before,
                assigned_after,
            );
        }
    }

    fn commonjs_export_assignment_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.clone())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base_for_expando(access.expression) {
                    return None;
                }
                self.commonjs_static_member_name_for_expando(access.name_or_argument)
            }
            _ => None,
        }
    }
}
