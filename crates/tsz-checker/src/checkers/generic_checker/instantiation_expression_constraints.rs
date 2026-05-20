use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Return `true` when `type_arg` is the type of an instantiation expression
    /// `typeof fn<TArgs>` whose `TArgs` do not match the type-parameter arity
    /// of any call/construct signature on the underlying function. Such
    /// expressions also raise TS2635 at the instantiation site; tsc treats the
    /// resulting type as `errorType`, which then fails any non-trivial
    /// type-parameter constraint check (TS2344).
    pub(crate) fn is_failed_typeof_instantiation_arg(&mut self, type_arg: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((base, args)) = query::application_base_and_args(db, type_arg) else {
            return false;
        };

        // Generic-type-reference Applications (`Foo<X>` for a type alias /
        // class / interface) use a `Lazy(DefId)` / `Recursive` / `BoundParameter`
        // base. Their arity mismatches are reported elsewhere (TS2305 / TS2558)
        // — not the typeof-instantiation flow.
        if query::is_named_type_reference(db, base) {
            return false;
        }

        let Some(shape) = self.typeof_instantiation_callable_shape(base) else {
            return false;
        };
        let num_args = args.len();
        let call_match = shape
            .call_signatures
            .iter()
            .any(|s| s.type_params.len() == num_args);
        let construct_match = shape
            .construct_signatures
            .iter()
            .any(|s| s.type_params.len() == num_args);
        !(call_match || construct_match)
    }

    pub(crate) fn is_successful_typeof_instantiation_arg(&mut self, type_arg: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((base, args)) = query::application_base_and_args(db, type_arg) else {
            return false;
        };
        if query::is_named_type_reference(db, base) {
            return false;
        }

        let Some(shape) = self.typeof_instantiation_callable_shape(base) else {
            return false;
        };
        let num_args = args.len();
        shape
            .call_signatures
            .iter()
            .any(|s| s.type_params.len() == num_args)
            || shape
                .construct_signatures
                .iter()
                .any(|s| s.type_params.len() == num_args)
    }

    /// Skip TS2344 when a `typeof` / instantiation-expression type argument is
    /// applied to a constraint shaped like a constructor (`new ...`) or callable.
    pub(crate) fn skip_constraint_for_typeof_instantiation(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        arg_node: Option<tsz_parser::parser::NodeIndex>,
    ) -> bool {
        if self.constraint_is_callable_or_constructable(constraint)
            && arg_node.is_some_and(|arg_idx| {
                self.type_query_has_qualified_instantiation_node(arg_idx)
                    && !self.is_failed_typeof_instantiation_node(arg_idx)
            })
        {
            return true;
        }

        let constraint_is_constructable = self.constraint_is_constructable(constraint);
        if constraint_is_constructable
            && arg_node
                .is_some_and(|arg_idx| self.type_query_constructor_access_level(arg_idx).is_some())
        {
            return false;
        }

        if self.is_successful_typeof_instantiation_arg(type_arg)
            && self.constraint_is_callable_or_constructable(constraint)
        {
            if constraint_is_constructable
                && self.constructor_accessibility_blocks_type_arg_constraint(type_arg, constraint)
            {
                return false;
            }
            return true;
        }

        if !constraint_is_constructable {
            return false;
        }

        if self.constructor_accessibility_blocks_type_arg_constraint(type_arg, constraint) {
            return false;
        }

        {
            use crate::query_boundaries::common::{TypeQueryKind, classify_type_query};

            if matches!(
                classify_type_query(self.ctx.types.as_type_database(), type_arg),
                TypeQueryKind::TypeQuery(_) | TypeQueryKind::ApplicationWithTypeQuery { .. }
            ) {
                return true;
            }
        }
        arg_node.is_some_and(|arg_idx| self.is_type_query_node_through_parens(arg_idx))
    }

    pub(crate) fn is_failed_typeof_instantiation_node(
        &mut self,
        mut arg_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        for _ in 0..10 {
            let Some(node) = self.ctx.arena.get(arg_idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            {
                arg_idx = wrapped.type_node;
                continue;
            }
            if node.kind != syntax_kind_ext::TYPE_QUERY {
                return false;
            }
            let Some(type_query) = self.ctx.arena.get_type_query(node) else {
                return false;
            };
            let has_type_args = type_query
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty());
            if !has_type_args {
                return false;
            }
            let Some(args) = type_query.type_arguments.as_ref() else {
                return false;
            };
            let expr_type = if self
                .ctx
                .arena
                .get(type_query.expr_name)
                .is_some_and(|expr| {
                    expr.kind == syntax_kind_ext::QUALIFIED_NAME
                        || expr.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                }) {
                self.resolve_typeof_qualified_value_chain(type_query.expr_name, true)
            } else {
                self.get_type_of_node(type_query.expr_name)
            };
            return self
                .instantiation_expression_applicability_error_type(expr_type, args.nodes.len())
                .is_some();
        }

        false
    }

    fn type_query_has_qualified_instantiation_node(
        &self,
        mut arg_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        for _ in 0..10 {
            let Some(node) = self.ctx.arena.get(arg_idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            {
                arg_idx = wrapped.type_node;
                continue;
            }
            if node.kind != syntax_kind_ext::TYPE_QUERY {
                return false;
            }
            let Some(type_query) = self.ctx.arena.get_type_query(node) else {
                return false;
            };
            if type_query
                .type_arguments
                .as_ref()
                .is_none_or(|args| args.nodes.is_empty())
            {
                return false;
            }
            return self
                .ctx
                .arena
                .get(type_query.expr_name)
                .is_some_and(|expr| {
                    expr.kind == syntax_kind_ext::QUALIFIED_NAME
                        || expr.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                });
        }

        false
    }

    pub(crate) fn constraint_is_constructable(&mut self, constraint: TypeId) -> bool {
        let constraint = self.resolve_lazy_type(constraint);
        crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, constraint)
            .is_some_and(|sigs| !sigs.is_empty())
            || {
                let evaluated = self.evaluate_type_for_assignability(constraint);
                evaluated != constraint
                    && crate::query_boundaries::common::construct_signatures_for_type(
                        self.ctx.types,
                        evaluated,
                    )
                    .is_some_and(|sigs| !sigs.is_empty())
            }
    }

    pub(crate) fn constraint_is_callable_or_constructable(&mut self, constraint: TypeId) -> bool {
        let constraint = self.resolve_lazy_type(constraint);
        crate::query_boundaries::common::function_shape_for_type(self.ctx.types, constraint)
            .is_some()
            || crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, constraint)
                .is_some_and(|sigs| !sigs.is_empty())
            || crate::query_boundaries::common::construct_signatures_for_type(
                self.ctx.types,
                constraint,
            )
            .is_some_and(|sigs| !sigs.is_empty())
    }

    fn typeof_instantiation_callable_shape(
        &mut self,
        base: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::CallableShape>> {
        let resolved = self.resolve_lazy_type(base);
        let resolved = self.evaluate_type_for_assignability(resolved);
        crate::query_boundaries::common::get_callable_shape_for_type(
            self.ctx.types.as_type_database(),
            resolved,
        )
    }
}
