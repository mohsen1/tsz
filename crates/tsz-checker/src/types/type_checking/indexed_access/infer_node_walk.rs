//! AST-walk helpers for locating `infer` type nodes and testing AST descendancy
//! while validating indexed-access types.
//!
//! Extracted from `indexed_access.rs` to keep that file under the 2000-line
//! checker-boundary limit enforced by `scripts/arch/arch_guard.py`. Pure code
//! motion: these are arena-only traversals with no type semantics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Collect all `INFER_TYPE` node indices in a subtree, using parent-tracking.
    /// Walks all nodes whose parent chain leads back to `root_idx`.
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
    pub(super) fn push_type_node_children(
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

    /// Check if `node_a` is a descendant of `node_b` in the AST.
    pub(super) fn is_descendant_of(&self, node_a: NodeIndex, node_b: NodeIndex) -> bool {
        let mut current = Some(node_a);
        while let Some(idx) = current {
            if idx == node_b {
                return true;
            }
            current = self.ctx.arena.parent_of(idx);
        }
        false
    }
}
