impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn should_use_evaluated_assignability_display(
        &self,
        ty: TypeId,
        evaluated: TypeId,
    ) -> bool {
        if ty == evaluated || evaluated == TypeId::ERROR {
            return false;
        }

        if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
            return false;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some() {
            return false;
        }

        // For TypeQuery (typeof X), don't use evaluated display - preserve the
        // typeof syntax instead of expanding to the full function type.
        // This prevents double function arrows like `() => () => typeof fn`.
        if crate::query_boundaries::common::is_type_query_type(self.ctx.types, ty) {
            return false;
        }

        // For function/callable types whose signatures carry a `TypeQuery`
        // in any param or return position, don't use the evaluated display.
        // Evaluation would resolve the `TypeQuery` to the full function
        // type, causing double arrows like `() => () => typeof fn`
        // (return-side) or extra wrapping like `(t: (t: typeof g) => void)
        // => void` (param-side, for recursive `typeof X` referring to the
        // enclosing function).
        if crate::query_boundaries::common::function_signature_has_typeof(self.ctx.types, ty) {
            return false;
        }

        // Generic Application of a TypeAlias whose body is IndexedAccess or
        // Conditional: expand only when evaluation reduces to a concrete
        // (non-conditional, non-indexed-access) shape. tsc keeps the alias
        // when reduction stalls (e.g. free type params in a conditional).
        // Must run before the contains-type-parameters guard below.
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
            && let Some(def_id) =
                crate::query_boundaries::common::get_application_lazy_def_id(self.ctx.types, ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && let Some(body) = def.body
            && (crate::query_boundaries::common::is_index_access_type(self.ctx.types, body)
                || crate::query_boundaries::common::is_conditional_type(self.ctx.types, body))
        {
            return !crate::query_boundaries::common::is_conditional_type(
                self.ctx.types,
                evaluated,
            ) && !crate::query_boundaries::common::is_index_access_type(
                self.ctx.types,
                evaluated,
            );
        }

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, evaluated)
        {
            return false;
        }

        if evaluated == TypeId::NEVER
            || crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
        {
            return true;
        }

        if (crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty).is_some()
            || crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, ty)
                .is_some())
            && (crate::query_boundaries::common::is_template_literal_type(
                self.ctx.types,
                evaluated,
            ) || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some())
        {
            return true;
        }

        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_keyof_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
        {
            return false;
        }

        // For IndexAccess types, display the evaluated form when it resolves to a
        // concrete type (union, object, primitive). This makes error messages show
        // the resolved type instead of the raw indexed access syntax.
        // e.g., `Pairs<FooBar>[keyof FooBar]` → `{ key: "foo"; value: string; } | { key: "bar"; value: number; }`
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty) {
            return true;
        }

        matches!(
            evaluated,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::VOID
        )
    }

    pub(in crate::error_reporter) fn format_structural_indexed_object_type(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
        if shape.string_index.is_none() && shape.number_index.is_none() {
            return None;
        }

        let mut parts = Vec::new();
        for idx in shape.string_index.iter().chain(shape.number_index.iter()) {
            let key_name = idx
                .param_name
                .map(|a| self.ctx.types.resolve_atom_ref(a).to_string())
                .unwrap_or_else(|| "x".to_string());
            let key_kind = self.format_type(idx.key_type);
            parts.push(format!(
                "[{key_name}: {key_kind}]: {}",
                self.format_type(idx.value_type)
            ));
        }
        for prop in &shape.properties {
            let name = self.ctx.types.resolve_atom_ref(prop.name);
            let optional = if prop.optional { "?" } else { "" };
            let readonly = if prop.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{readonly}{name}{optional}: {}",
                self.format_type(prop.type_id)
            ));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    /// Check if a type contains string literal types (directly or as union members).
    /// Used to determine whether an object literal property should display its
    /// literal value (for discriminated union contexts) or the widened type.
    pub(in crate::error_reporter) fn type_contains_string_literal(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::type_contains_string_literal(self.ctx.types, type_id)
    }

    pub(in crate::error_reporter) fn literal_expression_display(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        // Skip only parentheses, NOT type assertions. A type assertion like
        // `'bar' as any` changes the type to `any`, so the literal display
        // should not be used — the asserted type should be displayed instead.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        // If this is a type assertion expression (as/angle-bracket), don't
        // display the inner literal — let the caller use the asserted type.
        if node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                let escaped = lit
                    .text
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                Some(format!("\"{escaped}\""))
            }
            k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.literal_expression_display(unary.operand)?;
                match unary.operator {
                    k if k == tsz_scanner::SyntaxKind::MinusToken as u16 => {
                        if operand.parse::<f64>().is_ok_and(|value| value == 0.0) {
                            return Some("0".to_string());
                        }
                        Some(format!("-{operand}"))
                    }
                    k if k == tsz_scanner::SyntaxKind::PlusToken as u16 => Some(operand),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let cond = self.ctx.arena.get_conditional_expr(node)?;
                let left = self.literal_expression_display(cond.when_true)?;
                let right = self.literal_expression_display(cond.when_false)?;
                if left == right {
                    Some(left)
                } else {
                    Some(format!("{left} | {right}"))
                }
            }
            _ => None,
        }
    }

    pub(in crate::error_reporter) fn assignment_source_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(self.terminal_assignment_source_expression(bin.right));
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(self.terminal_assignment_source_expression(bin.right));
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(decl.initializer));
                }
                k if k == syntax_kind_ext::BINDING_ELEMENT => {
                    let elem = self.ctx.arena.get_binding_element(node)?;
                    if elem.initializer.is_some() {
                        return Some(self.terminal_assignment_source_expression(elem.initializer));
                    }
                    // Fall through to walk further up if the binding element has
                    // no own default — the parameter / variable initializer is
                    // the relevant source.
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    let param = self.ctx.arena.get_parameter(node)?;
                    return param
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(param.initializer));
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(node)?;
                    return prop.initializer.is_some().then_some(prop.initializer);
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(node)?;
                    return prop.name.is_some().then_some(prop.name);
                }
                k if k == syntax_kind_ext::RETURN_STATEMENT => {
                    let ret = self.ctx.arena.get_return_statement(node)?;
                    return ret.expression.is_some().then_some(ret.expression);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(in crate::error_reporter) fn assignment_target_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(bin.left);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(bin.left);
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl.name.is_some().then_some(decl.name);
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    let param = self.ctx.arena.get_parameter(node)?;
                    return param.name.is_some().then_some(param.name);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(crate) fn assignment_source_is_return_expression(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return true;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                && let Some(func) = self.ctx.arena.get_function(parent_node)
                && func.body == current
                && node.kind != syntax_kind_ext::BLOCK
            {
                return true;
            }
            current = ext.parent;
        }

        false
    }
}
