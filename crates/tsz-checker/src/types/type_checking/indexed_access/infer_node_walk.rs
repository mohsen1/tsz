//! AST traversal helpers for locating `infer` type nodes within an indexed
//! access constraint subtree. Split out of `indexed_access.rs` to keep that
//! file under the per-file LOC ceiling; behavior is unchanged.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Collect every `infer` type node within the subtree rooted at `root_idx`,
    /// walking the type-node children explicitly so nested `infer Y`
    /// constraints in the same scope are also captured.
    pub(super) fn collect_infer_nodes_in_subtree(&self, root_idx: NodeIndex) -> Vec<NodeIndex> {
        let mut result = Vec::new();
        let mut stack = vec![root_idx];
        while let Some(idx) = stack.pop() {
            if idx == NodeIndex::NONE {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::INFER_TYPE {
                result.push(idx);
                // Nested `infer Y` inside the constraint is in the same scope.
                if let Some(infer_data) = self.ctx.arena.get_infer_type(node) {
                    stack.push(infer_data.type_parameter);
                }
                continue;
            }
            // Push children based on node type
            self.push_type_node_children(idx, node, &mut stack);
        }
        result
    }

    /// Push child indices of a type node onto the stack for traversal.
    fn push_type_node_children(
        &self,
        _idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
        stack: &mut Vec<NodeIndex>,
    ) {
        // Tuple type: push elements
        if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
            stack.extend(tuple.elements.nodes.iter().copied());
            return;
        }
        // Array type
        if let Some(arr) = self.ctx.arena.get_array_type(node) {
            stack.push(arr.element_type);
            return;
        }
        // Union/intersection type (both use CompositeTypeData)
        if let Some(composite) = self.ctx.arena.get_composite_type(node) {
            stack.extend(composite.types.nodes.iter().copied());
            return;
        }
        // Type reference with type arguments
        if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
            if let Some(ref args) = type_ref.type_arguments {
                stack.extend(args.nodes.iter().copied());
            }
            return;
        }
        // Wrapped types: rest, optional, parenthesized (all share WrappedTypeData)
        if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
            stack.push(wrapped.type_node);
            return;
        }
        // Conditional type
        if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
            stack.push(cond.check_type);
            stack.push(cond.extends_type);
            stack.push(cond.true_type);
            stack.push(cond.false_type);
            return;
        }
        // Indexed access type
        if let Some(iat) = self.ctx.arena.get_indexed_access_type(node) {
            stack.push(iat.object_type);
            stack.push(iat.index_type);
            return;
        }
        // Type operator (keyof, readonly, unique)
        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            stack.push(type_op.type_node);
            return;
        }
        if let Some(tp) = self.ctx.arena.get_type_parameter(node) {
            stack.extend_from_slice(&[tp.constraint, tp.default]);
        }
    }
}
