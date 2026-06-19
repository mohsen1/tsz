use super::{IRNode, IRPrinter};

impl IRPrinter<'_> {
    /// Check if a generator switch case should stay on the `case N:` line.
    pub(super) fn is_generator_inline_case_statement(node: &IRNode) -> bool {
        match node {
            IRNode::ThrowStatement(expr) => Self::is_generator_inline_throw_expression(expr),
            IRNode::ReturnStatement(Some(expr)) => {
                matches!(expr.as_ref(), IRNode::GeneratorOp { .. })
            }
            _ => false,
        }
    }

    pub(super) fn is_generator_break_return(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ReturnStatement(Some(expr))
                if matches!(expr.as_ref(), IRNode::GeneratorOp { opcode: 3, .. })
        )
    }

    pub(super) fn is_generator_sent_assignment(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ExpressionStatement(expr)
                if matches!(
                    expr.as_ref(),
                    IRNode::BinaryExpr { right, .. } if matches!(right.as_ref(), IRNode::GeneratorSent)
                )
        )
    }

    const fn is_generator_inline_throw_expression(expr: &IRNode) -> bool {
        match expr {
            // Stay transparent to the source-position wrapper: inline-ness is a
            // property of the wrapped expression.
            IRNode::Positioned { inner, .. } => Self::is_generator_inline_throw_expression(inner),
            IRNode::Identifier(_) | IRNode::CallExpr { .. } | IRNode::GeneratorSent => true,
            _ => false,
        }
    }

    pub(super) fn should_indent_sequence_child(node: &IRNode) -> bool {
        match node {
            IRNode::NamespaceIIFE {
                skip_sequence_indent,
                ..
            } => !skip_sequence_indent,
            _ => true,
        }
    }

    pub(super) fn is_noop_statement(node: &IRNode) -> bool {
        match node {
            IRNode::Sequence(nodes) if nodes.is_empty() => true,
            IRNode::EmptyStatement => true,
            IRNode::Raw(text) => text.trim().is_empty(),
            _ => false,
        }
    }
}
