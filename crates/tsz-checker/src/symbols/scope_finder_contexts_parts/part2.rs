impl<'a> CheckerState<'a> {
    // =========================================================================
    // Namespace Context Detection
    // =========================================================================

    /// Returns `true` when the identifier is being evaluated inside a computed
    /// property name (`[expr]`) that belongs to a type-only or ambient context
    /// (interface member, type literal member, abstract member, `declare`
    /// member, or ambient class).  In these positions the expression is never
    /// emitted as runtime code, so TS1361/TS1362 should be suppressed.
    pub(crate) fn is_in_ambient_computed_property_context(&self) -> bool {
        let Some(cpn_idx) = self.ctx.checking_computed_property_name else {
            return false;
        };

        // Walk from the computed property name node upward to the member
        // declaration, then to its parent (class/interface/type literal).
        let mut current = cpn_idx;
        for _ in 0..8 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            match parent_node.kind {
                // Interface and type literal members are always type-only
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => return true,
                k if k == syntax_kind_ext::TYPE_LITERAL => return true,

                // Ambient class: `declare class C { [x]: any; }`
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(class) = self.ctx.arena.get_class(parent_node)
                        && self.has_declare_modifier(&class.modifiers)
                    {
                        return true;
                    }
                    return false;
                }

                // Property/method declarations may have abstract or declare modifiers
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(parent_node)
                        && (self.has_abstract_modifier(&prop.modifiers)
                            || self.has_declare_modifier(&prop.modifiers))
                    {
                        return true;
                    }
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(parent_node)
                        && self.has_abstract_modifier(&method.modifiers)
                    {
                        return true;
                    }
                    current = parent_idx;
                }

                _ => {
                    current = parent_idx;
                }
            }
        }
        false
    }

    /// Check if a function's body contains `this.property = value` assignments,
    /// which in JS files indicates a constructor function pattern. When a function
    /// has such assignments, tsc types `this` as the constructed instance and
    /// does not emit TS2683.
    pub(crate) fn function_body_has_this_property_assignments(&self, func_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            BINARY_EXPRESSION, EXPRESSION_STATEMENT, PROPERTY_ACCESS_EXPRESSION,
        };

        let Some(fn_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(fn_node) else {
            return false;
        };
        let body_idx = func.body;
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };

        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind != BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let Some(lhs_node) = self.ctx.arena.get(binary.left) else {
                continue;
            };
            if lhs_node.kind != PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = self.ctx.arena.get_access_expr(lhs_node) else {
                continue;
            };
            let Some(base_node) = self.ctx.arena.get(access.expression) else {
                continue;
            };
            if base_node.kind == SyntaxKind::ThisKeyword as u16 {
                return true;
            }
        }

        false
    }
}
