//! Conditional-flow substitution for type-variable references.
//!
//! Implements `tsc`'s `getConditionalFlowTypeOfType`: a reference to a
//! conditional type's check variable that appears inside the conditional's true
//! branch carries the implied constraint from the `extends` type, modelled as a
//! solver substitution type. This keeps a check variable used in the true branch
//! well-formed against dependent constraints.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Apply `tsc`'s `getConditionalFlowTypeOfType`: when a type-variable
    /// reference at `node_idx` appears inside the true branch of one or more
    /// enclosing conditional types whose check operand is that same variable,
    /// narrow it to a substitution type `type_param & extends_1 & … & extends_n`.
    ///
    /// This lets a check variable used inside the true branch satisfy a dependent
    /// constraint: inside `T extends string ? F<T> : never`, references to `T`
    /// carry `T & string`, so a use like `Capitalize<T>` (or a nested
    /// `Capitalize<CamelCase<T>>`) is well-formed.
    pub(crate) fn apply_conditional_flow_substitution(
        &mut self,
        node_idx: NodeIndex,
        type_param: TypeId,
    ) -> TypeId {
        // Walk up the AST, collecting the extends nodes of every enclosing
        // conditional whose true branch contains `node_idx` and whose check
        // operand is the same type variable. Collect node indices first so the
        // immutable arena walk does not conflict with the `&mut self` lowering
        // of each extends type afterwards.
        let mut extends_nodes: Vec<NodeIndex> = Vec::new();
        let mut child = node_idx;
        let mut parent = self
            .ctx
            .arena
            .get_extended(child)
            .map_or(NodeIndex::NONE, |info| info.parent);
        let mut iterations = 0u32;
        while parent.is_some() {
            iterations += 1;
            if iterations > tsz_common::limits::MAX_TREE_WALK_ITERATIONS {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                && let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                && cond.true_type == child
                && self.naked_check_type_param_id(cond.check_type) == Some(type_param)
            {
                extends_nodes.push(cond.extends_type);
            }
            child = parent;
            parent = self
                .ctx
                .arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        if extends_nodes.is_empty() {
            return type_param;
        }

        // The substitution's constraint is `type_param & extends_1 & …`, mirroring
        // tsc's `getIntersectionType([...constraints, type])`.
        let mut constraint = type_param;
        for extends_idx in extends_nodes {
            let extends = self.get_type_from_type_node(extends_idx);
            constraint = self.ctx.types.intersection2(constraint, extends);
        }
        self.ctx.types.substitution(type_param, constraint)
    }

    /// Resolve a type node to the `TypeId` of the naked type parameter it names
    /// (parenthesised / type-reference / identifier with no type arguments), or
    /// `None`. Used to compare a conditional's check operand by identity.
    fn naked_check_type_param_id(&self, node_idx: NodeIndex) -> Option<TypeId> {
        let mut current = node_idx;
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > tsz_common::limits::MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                    current = self.ctx.arena.get_wrapped_type(node)?.type_node;
                }
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    let data = self.ctx.arena.get_type_ref(node)?;
                    if let Some(args) = &data.type_arguments
                        && !args.nodes.is_empty()
                    {
                        return None;
                    }
                    let name_node = self.ctx.arena.get(data.type_name)?;
                    let ident = self.ctx.arena.get_identifier(name_node)?;
                    return self.lookup_type_parameter(ident.escaped_text.as_str());
                }
                k if k == SyntaxKind::Identifier as u16 => {
                    let ident = self.ctx.arena.get_identifier(node)?;
                    return self.lookup_type_parameter(ident.escaped_text.as_str());
                }
                _ => return None,
            }
        }
    }
}
