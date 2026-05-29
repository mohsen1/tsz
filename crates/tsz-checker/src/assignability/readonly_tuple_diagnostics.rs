//! Readonly array/tuple diagnostic preflights for assignment reporting.

use crate::query_boundaries::common::{
    array_element_type, is_array_type, is_tuple_type, readonly_inner_type, tuple_list_id,
    type_param_info,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// A *mutable* (not `readonly`-wrapped) array or tuple type. `[...T]` counts
    /// because a spread tuple is a mutable `Tuple`; `readonly number[]` does not.
    fn is_mutable_array_or_tuple_type(&mut self, ty: TypeId) -> bool {
        let evaluated = self.evaluate_type_for_assignability(ty);
        for candidate in [ty, evaluated] {
            if readonly_inner_type(self.ctx.types, candidate).is_none()
                && (is_array_type(self.ctx.types, candidate)
                    || is_tuple_type(self.ctx.types, candidate))
            {
                return true;
            }
        }
        false
    }

    /// Whether a contextual type asks for a mutable array/tuple value — either
    /// directly (`number[]`, `[number, number]`, `[...T]`) or via a type
    /// parameter whose constraint is one (`T extends unknown[]`). A `readonly`
    /// contextual type (`readonly number[]`, `T extends readonly unknown[]`) does
    /// not, so the source's const-ness is preserved there.
    fn contextual_demands_mutable_array_or_tuple(&mut self, contextual: TypeId) -> bool {
        if self.is_mutable_array_or_tuple_type(contextual) {
            return true;
        }
        let evaluated = self.evaluate_type_for_assignability(contextual);
        for candidate in [contextual, evaluated] {
            if let Some(constraint) =
                type_param_info(self.ctx.types, candidate).and_then(|info| info.constraint)
                && self.is_mutable_array_or_tuple_type(constraint)
            {
                return true;
            }
        }
        false
    }

    /// Whether `expr_idx` is a *fresh* array literal whose `readonly`-ness comes
    /// from `as const` — either the array literal itself (when called with the
    /// assertion's operand) or an `[...] as const` wrapper (when called with the
    /// whole assertion expression). An aliased value (a variable) is not fresh.
    fn is_fresh_const_assertion_array_literal(&self, expr_idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return true;
        }
        (node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::TYPE_ASSERTION)
            && self
                .ctx
                .arena
                .get_type_assertion(node)
                .filter(|assertion| self.is_const_assertion_type_node(assertion.type_node))
                .and_then(|assertion| {
                    let inner = self.ctx.arena.skip_parenthesized(assertion.expression);
                    self.ctx.arena.get(inner)
                })
                .is_some_and(|inner| inner.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
    }

    /// tsc drops the `readonly` modifier from a *fresh* array/tuple literal
    /// written with `as const` when its contextual type is a mutable array/tuple
    /// (or a type parameter constrained to one). A fresh literal is not aliased,
    /// so its const-ness is not binding at a mutable consumption site:
    ///
    /// ```text
    /// const a: number[] = [1, 2] as const;          // ok ([1, 2] is mutable here)
    /// declare function f<T extends readonly unknown[]>(t: [...T]): T;
    /// const r = f([1, 2] as const);                 // ok, infers T = [1, 2]
    /// ```
    ///
    /// while keeping it where the target is itself `readonly` or not an
    /// array/tuple (`const x = [1, 2] as const` stays `readonly [1, 2]`), and
    /// while an aliased `readonly` value (a variable) is still rejected by the
    /// normal relation. `expr_idx` may be the array literal itself or the
    /// enclosing `as const` expression. Returns the readonly-peeled (mutable)
    /// type when the modifier should be dropped.
    pub(crate) fn const_assertion_array_literal_drops_readonly(
        &mut self,
        expr_idx: NodeIndex,
        result_type: TypeId,
        contextual: Option<TypeId>,
    ) -> Option<TypeId> {
        let contextual = contextual?;
        let inner = readonly_inner_type(self.ctx.types, result_type)?;
        if !(is_array_type(self.ctx.types, inner) || is_tuple_type(self.ctx.types, inner)) {
            return None;
        }
        if !self.is_fresh_const_assertion_array_literal(expr_idx) {
            return None;
        }
        self.contextual_demands_mutable_array_or_tuple(contextual)
            .then_some(inner)
    }

    fn is_array_or_tuple_like_for_readonly_assignment(&mut self, type_id: TypeId) -> bool {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        for candidate in [type_id, evaluated] {
            let candidate = readonly_inner_type(self.ctx.types, candidate).unwrap_or(candidate);
            if tuple_list_id(self.ctx.types, candidate).is_some()
                || array_element_type(self.ctx.types, candidate).is_some()
            {
                return true;
            }
            if let Some(constraint) =
                type_param_info(self.ctx.types, candidate).and_then(|info| info.constraint)
            {
                let evaluated_constraint = self.evaluate_type_for_assignability(constraint);
                for constraint_candidate in [constraint, evaluated_constraint] {
                    let constraint_candidate =
                        readonly_inner_type(self.ctx.types, constraint_candidate)
                            .unwrap_or(constraint_candidate);
                    if tuple_list_id(self.ctx.types, constraint_candidate).is_some()
                        || array_element_type(self.ctx.types, constraint_candidate).is_some()
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn readonly_to_mutable_array_or_tuple_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        let evaluated_source = self.evaluate_type_for_assignability(source);
        let evaluated_target = self.evaluate_type_for_assignability(target);

        let readonly_source_inner = readonly_inner_type(self.ctx.types, source)
            .or_else(|| readonly_inner_type(self.ctx.types, evaluated_source))?;
        if readonly_inner_type(self.ctx.types, target).is_some()
            || readonly_inner_type(self.ctx.types, evaluated_target).is_some()
        {
            return None;
        }

        if !self.is_array_or_tuple_like_for_readonly_assignment(readonly_source_inner) {
            return None;
        }

        let target_is_mutable_array_or_tuple =
            is_tuple_type(self.ctx.types, target) || is_array_type(self.ctx.types, target);
        target_is_mutable_array_or_tuple.then_some(
            tsz_solver::SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type: source,
                target_type: target,
            },
        )
    }
}
