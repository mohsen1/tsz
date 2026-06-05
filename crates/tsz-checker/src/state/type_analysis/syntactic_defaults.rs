use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn type_parameter_default_syntactically_satisfies_constraint(
        &self,
        param_idx: NodeIndex,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return false;
        };
        if param.default == NodeIndex::NONE || param.constraint == NodeIndex::NONE {
            return false;
        }
        self.type_node_syntactically_in_constraint(param.default, param.constraint, 0)
    }

    fn type_node_syntactically_in_constraint(
        &self,
        default_node: NodeIndex,
        constraint_node: NodeIndex,
        depth: u8,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(node) = self.ctx.arena.get(constraint_node) else {
            return false;
        };
        if node.kind == syntax_kind_ext::UNION_TYPE
            && let Some(union) = self.ctx.arena.get_composite_type(node)
        {
            return union.types.nodes.iter().any(|&member| {
                self.type_nodes_syntactically_equal(default_node, member, depth + 1)
            });
        }
        self.type_nodes_syntactically_equal(default_node, constraint_node, depth + 1)
    }

    fn type_nodes_syntactically_equal(&self, left: NodeIndex, right: NodeIndex, depth: u8) -> bool {
        if depth > 32 || left == NodeIndex::NONE || right == NodeIndex::NONE {
            return false;
        }

        let left = self.unwrap_syntactic_type_node(left, depth);
        let right = self.unwrap_syntactic_type_node(right, depth);
        if left == right {
            return true;
        }

        let Some(left_node) = self.ctx.arena.get(left) else {
            return false;
        };
        let Some(right_node) = self.ctx.arena.get(right) else {
            return false;
        };
        if left_node.kind != right_node.kind {
            return false;
        }

        if let (Some(left_ident), Some(right_ident)) = (
            self.ctx.arena.get_identifier(left_node),
            self.ctx.arena.get_identifier(right_node),
        ) {
            return left_ident.escaped_text == right_ident.escaped_text;
        }

        if let (Some(left_name), Some(right_name)) = (
            self.ctx.arena.get_qualified_name(left_node),
            self.ctx.arena.get_qualified_name(right_node),
        ) {
            return self.type_nodes_syntactically_equal(left_name.left, right_name.left, depth + 1)
                && self.type_nodes_syntactically_equal(
                    left_name.right,
                    right_name.right,
                    depth + 1,
                );
        }

        if let (Some(left_ref), Some(right_ref)) = (
            self.ctx.arena.get_type_ref(left_node),
            self.ctx.arena.get_type_ref(right_node),
        ) {
            if !self.type_nodes_syntactically_equal(
                left_ref.type_name,
                right_ref.type_name,
                depth + 1,
            ) {
                return false;
            }
            return match (&left_ref.type_arguments, &right_ref.type_arguments) {
                (None, None) => true,
                (Some(left_args), Some(right_args)) => {
                    left_args.nodes.len() == right_args.nodes.len()
                        && left_args.nodes.iter().zip(right_args.nodes.iter()).all(
                            |(&left_arg, &right_arg)| {
                                self.type_nodes_syntactically_equal(left_arg, right_arg, depth + 1)
                            },
                        )
                }
                _ => false,
            };
        }

        if let (Some(left_wrapped), Some(right_wrapped)) = (
            self.ctx.arena.get_wrapped_type(left_node),
            self.ctx.arena.get_wrapped_type(right_node),
        ) {
            return self.type_nodes_syntactically_equal(
                left_wrapped.type_node,
                right_wrapped.type_node,
                depth + 1,
            );
        }

        if let (Some(left_literal_type), Some(right_literal_type)) = (
            self.ctx.arena.get_literal_type(left_node),
            self.ctx.arena.get_literal_type(right_node),
        ) {
            return self.type_nodes_syntactically_equal(
                left_literal_type.literal,
                right_literal_type.literal,
                depth + 1,
            );
        }

        if let (Some(left_literal), Some(right_literal)) = (
            self.ctx.arena.get_literal(left_node),
            self.ctx.arena.get_literal(right_node),
        ) {
            return left_literal.text == right_literal.text;
        }

        if let (Some(left_array), Some(right_array)) = (
            self.ctx.arena.get_array_type(left_node),
            self.ctx.arena.get_array_type(right_node),
        ) {
            return self.type_nodes_syntactically_equal(
                left_array.element_type,
                right_array.element_type,
                depth + 1,
            );
        }

        if let (Some(left_tuple), Some(right_tuple)) = (
            self.ctx.arena.get_tuple_type(left_node),
            self.ctx.arena.get_tuple_type(right_node),
        ) {
            return left_tuple.elements.nodes.len() == right_tuple.elements.nodes.len()
                && left_tuple
                    .elements
                    .nodes
                    .iter()
                    .zip(right_tuple.elements.nodes.iter())
                    .all(|(&left_elem, &right_elem)| {
                        self.type_nodes_syntactically_equal(left_elem, right_elem, depth + 1)
                    });
        }

        if let (Some(left_composite), Some(right_composite)) = (
            self.ctx.arena.get_composite_type(left_node),
            self.ctx.arena.get_composite_type(right_node),
        ) {
            return left_composite.types.nodes.len() == right_composite.types.nodes.len()
                && left_composite
                    .types
                    .nodes
                    .iter()
                    .zip(right_composite.types.nodes.iter())
                    .all(|(&left_type, &right_type)| {
                        self.type_nodes_syntactically_equal(left_type, right_type, depth + 1)
                    });
        }

        matches!(
            left_node.kind,
            k if k == SyntaxKind::AnyKeyword as u16
                || k == SyntaxKind::UnknownKeyword as u16
                || k == SyntaxKind::NeverKeyword as u16
                || k == SyntaxKind::StringKeyword as u16
                || k == SyntaxKind::NumberKeyword as u16
                || k == SyntaxKind::BooleanKeyword as u16
                || k == SyntaxKind::BigIntKeyword as u16
                || k == SyntaxKind::SymbolKeyword as u16
                || k == SyntaxKind::VoidKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::ObjectKeyword as u16
                || k == SyntaxKind::ThisKeyword as u16
        )
    }

    fn unwrap_syntactic_type_node(&self, mut node_idx: NodeIndex, mut depth: u8) -> NodeIndex {
        while depth <= 32 {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return node_idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            {
                node_idx = wrapped.type_node;
                depth += 1;
                continue;
            }
            return node_idx;
        }
        node_idx
    }
}
