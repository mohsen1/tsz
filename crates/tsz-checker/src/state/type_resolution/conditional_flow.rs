//! Conditional-flow substitution for type-variable references.
//!
//! Implements `tsc`'s `getConditionalFlowTypeOfType`: a reference to a
//! conditional type's check variable that appears inside the conditional's true
//! branch carries the implied constraint from the `extends` type, modelled as a
//! solver substitution type. This keeps a check variable used in the true branch
//! well-formed against dependent constraints.

use crate::query_boundaries::common::{self as query_common, TypeSubstitution};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Instantiate a whole type argument with conditional-flow substitutions
    /// visible at `node_idx`.
    ///
    /// This is intentionally narrower than lowering every true-branch type
    /// reference. It is used by generic constraint validation, where tsc asks
    /// whether the current type argument satisfies the constraint under the
    /// true-branch assumption.
    pub(crate) fn apply_conditional_flow_to_type_arg(
        &mut self,
        node_idx: NodeIndex,
        type_arg: TypeId,
    ) -> TypeId {
        // Two narrowing channels, both modelling `tsc`'s `getImpliedConstraint`:
        //   * a naked type-parameter check operand (`T extends U ? …T… : …`)
        //     narrows *occurrences* of `T` inside the type argument — a
        //     name-keyed substitution;
        //   * a structured check operand (`F<V> extends U ? …F<V>… : …`, where
        //     the operand is not a bare parameter) narrows the type argument as a
        //     whole when it *is* that operand — `tsc` compares the actual type
        //     variable of the check type against the type itself, not only bare
        //     parameters, so a generic-alias check operand narrows here too.
        let mut subst = TypeSubstitution::new();
        // Accumulated `extends` narrowing for the whole-argument channel; stays
        // `None` until a structured check operand matches (0 or 1 is the common
        // case, so avoid a heap `Vec`).
        let mut whole_constraint: Option<TypeId> = None;
        // Naked-check-param channel: collect every enclosing true-branch
        // `extends` constraint per narrowed type parameter as a FLAT
        // intersection, then form a single substitution per parameter after the
        // walk. Mirrors tsc's `getConditionalFlowTypeOfType`, which appends each
        // implied constraint to one list and builds ONE
        // `getSubstitutionType(type, getIntersectionType([...constraints, type]))`.
        // Wrapping the prior substitution as the next intersection base inside
        // the loop instead nests substitution types (`Sub(T, Sub(T, T & A) & B)`)
        // that the subtype relation cannot see through, producing spurious
        // `TS2344` for nested conditionals such as
        // `T extends A ? Wrap<T extends B ? F<T> : never> : never`.
        // Entries are `(type-param name, type-param id, accumulated extends)`.
        let mut naked: Vec<(tsz_common::interner::Atom, TypeId, TypeId)> = Vec::new();

        let arg_actual = tsz_solver::type_queries::substitution_base_or_self(
            self.ctx.types.as_type_database(),
            type_arg,
        );

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
            {
                if let Some(type_param) = self.naked_check_type_param_id(cond.check_type)
                    && let Some(info) =
                        query_common::type_param_info(self.ctx.types.as_type_database(), type_param)
                {
                    let extends = self.get_type_from_type_node(cond.extends_type);
                    if let Some(pos) = naked.iter().position(|(n, _, _)| *n == info.name) {
                        let acc = self.ctx.types.intersection2(naked[pos].2, extends);
                        naked[pos].2 = acc;
                    } else {
                        naked.push((info.name, type_param, extends));
                    }
                } else {
                    // Structured check operand: narrow the whole argument when it
                    // is the conditional's check operand (compared by actual type
                    // variable so a substitution on either side still matches).
                    let check_t = self.get_type_from_type_node(cond.check_type);
                    let check_actual = tsz_solver::type_queries::substitution_base_or_self(
                        self.ctx.types.as_type_database(),
                        check_t,
                    );
                    if check_actual == arg_actual {
                        let extends = self.get_type_from_type_node(cond.extends_type);
                        whole_constraint = Some(
                            whole_constraint
                                .map_or(extends, |c| self.ctx.types.intersection2(c, extends)),
                        );
                    }
                }
            }
            child = parent;
            parent = self
                .ctx
                .arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        // Form one substitution per narrowed parameter from the flattened
        // constraints: `Sub(T, T & extends1 & extends2 & …)`.
        for (name, type_param, acc_extends) in naked {
            let constraint = self.ctx.types.intersection2(type_param, acc_extends);
            subst.insert(name, self.ctx.types.substitution(type_param, constraint));
        }

        if subst.is_empty() && whole_constraint.is_none() {
            return type_arg;
        }

        let mut result = if subst.is_empty() {
            type_arg
        } else {
            query_common::instantiate_type(self.ctx.types, type_arg, &subst)
        };
        if let Some(extends) = whole_constraint {
            let constraint = self.ctx.types.intersection2(result, extends);
            result = self.ctx.types.substitution(result, constraint);
        }
        result
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
