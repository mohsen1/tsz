//! Direct diagnostic source-expression selection.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl CheckerState<'_> {
    pub(in crate::error_reporter) fn direct_diagnostic_source_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        // Only skip parenthesized expressions, NOT type assertions.
        // For `<foo>({})`, we want the type assertion node (type `foo`),
        // not the inner `{}` expression.
        let expr_idx = self.ctx.arena.skip_parenthesized(anchor_idx);
        // A destructuring-pattern write target is not a source expression:
        // the value flowing into it is a computed slice with no node of its
        // own, and treating the target as the source repaints the message's
        // source side with the target's own declared annotation.
        if self.anchor_is_destructuring_assignment_write_target(expr_idx) {
            return None;
        }
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

        // Shorthand method and accessor member names (`{ m(x) {} }`,
        // `{ get x() {} }`, `{ set x(v) {} }`) are declaration names, not source
        // expressions. The member's value is the declaration itself; resolving
        // the name as a value reference would emit a false TS2304 "Cannot find name".
        // This mirrors the property-assignment guard above.
        if matches!(
            parent_node.kind,
            syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
        ) && self.get_declaration_name_node(parent_idx) == Some(expr_idx)
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
}
