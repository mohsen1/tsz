//! Coverage checks for duplicate type-alias missing-name validation.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(crate) fn type_alias_body_missing_names_covered_by_type_node_checking(
        &self,
        root: NodeIndex,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            if node_idx == NodeIndex::NONE {
                continue;
            }
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                        return false;
                    };
                    if let Some(args) = &type_ref.type_arguments {
                        stack.extend(args.nodes.iter().copied());
                    }
                }
                k if k == syntax_kind_ext::UNION_TYPE
                    || k == syntax_kind_ext::INTERSECTION_TYPE =>
                {
                    let Some(composite) = self.ctx.arena.get_composite_type(node) else {
                        return false;
                    };
                    stack.extend(composite.types.nodes.iter().copied());
                }
                k if k == syntax_kind_ext::ARRAY_TYPE => {
                    let Some(array) = self.ctx.arena.get_array_type(node) else {
                        return false;
                    };
                    stack.push(array.element_type);
                }
                k if k == syntax_kind_ext::OPTIONAL_TYPE
                    || k == syntax_kind_ext::REST_TYPE
                    || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
                {
                    let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) else {
                        return false;
                    };
                    stack.push(wrapped.type_node);
                }
                k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                    let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) else {
                        return false;
                    };
                    stack.push(indexed.object_type);
                    stack.push(indexed.index_type);
                }
                _ => return false,
            }
        }
        true
    }
}
