use super::{NodeIndex, NodeList, OptionalChainSegment, Printer, syntax_kind_ext};

impl<'a> Printer<'a> {
    pub(super) fn emit_invalid_new_optional_chain(
        &mut self,
        callee: NodeIndex,
        args: Option<&NodeList>,
    ) -> bool {
        let mut tail = Vec::new();
        let Some((access_kind, base, name_or_argument)) =
            self.collect_invalid_new_optional_access(callee, &mut tail)
        else {
            return false;
        };

        let needs_parens = self.ctx.flags.optional_chain_needs_parens;
        if needs_parens {
            self.open_paren();
            self.ctx.flags.optional_chain_needs_parens = false;
        }
        let temp = self.make_unique_name_hoisted();
        self.open_paren();
        self.write(&temp);
        self.write(" = new ");
        let prev_new = self.paren_in_new_callee;
        self.paren_in_new_callee = true;
        self.emit(base);
        self.paren_in_new_callee = prev_new;
        self.close_paren();
        self.write(" === null || ");
        self.write(&temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&temp);
        self.emit_optional_access_segment(access_kind, name_or_argument);
        self.emit_optional_chain_tail(&tail);
        if let Some(args) = args {
            self.open_paren();
            let valid_args: Vec<_> = args.nodes.iter().copied().filter(|n| n.is_some()).collect();
            self.emit_comma_separated(&valid_args);
            self.close_paren();
        }
        if needs_parens {
            self.close_paren();
        }
        true
    }

    fn collect_invalid_new_optional_access(
        &self,
        idx: NodeIndex,
        tail: &mut Vec<OptionalChainSegment>,
    ) -> Option<(u16, NodeIndex, NodeIndex)> {
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.arena.get_parenthesized(node)
        {
            return self.collect_invalid_new_optional_access(paren.expression, tail);
        }

        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if access.question_dot_token {
                return Some((node.kind, access.expression, access.name_or_argument));
            }

            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                tail.push(OptionalChainSegment::Property(access.name_or_argument));
            } else {
                tail.push(OptionalChainSegment::Element(access.name_or_argument));
            }
            return self.collect_invalid_new_optional_access(access.expression, tail);
        }

        None
    }

    pub(super) fn emit_invalid_new_type_assertion_callee(&mut self, expression: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::TYPE_ASSERTION {
            return false;
        }
        let Some(assertion) = self.arena.get_type_assertion(expr_node) else {
            return false;
        };

        self.write(" < ");
        self.emit(assertion.type_node);
        self.write(" > ");
        self.emit(assertion.expression);
        true
    }
}
