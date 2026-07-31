use super::super::function_type_helpers::GeneratorBodyReturnCheckCtx;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

pub(super) struct GeneratorDeclarationYieldCtx {
    pub(super) body: NodeIndex,
    pub(super) contextual_type: Option<TypeId>,
    pub(super) has_type_annotation: bool,
    pub(super) annotated_return_type: Option<TypeId>,
    pub(super) return_type: TypeId,
    pub(super) type_annotation: NodeIndex,
    pub(super) idx: NodeIndex,
    pub(super) function_is_async: bool,
    pub(super) early_yield_type: Option<TypeId>,
}

impl CheckerState<'_> {
    /// Recover inferred yield type for unannotated generator declarations.
    ///
    /// Function declarations defer their body walk to `check_function_declaration`
    /// so the full type-parameter scope chain is maintained. This suppressed
    /// pass computes only the signature's yield type; real body diagnostics still
    /// come from the later declaration check.
    pub(super) fn infer_generator_declaration_yield_type(
        &mut self,
        ctx: GeneratorDeclarationYieldCtx,
    ) -> Option<TypeId> {
        let GeneratorDeclarationYieldCtx {
            body,
            contextual_type,
            has_type_annotation,
            annotated_return_type,
            return_type,
            type_annotation,
            idx,
            function_is_async,
            early_yield_type,
        } = ctx;
        if self.generator_body_has_self_referential_yield_star(body) {
            return None;
        }

        let yield_snapshot = self.ctx.snapshot_return_type();
        let saved_cf_context = (
            self.ctx.iteration_depth,
            self.ctx.switch_depth,
            self.ctx.label_stack.len(),
            self.ctx.had_outer_loop,
        );
        self.ctx.iteration_depth = 0;
        self.ctx.switch_depth = 0;
        let saved_yield_collection = std::mem::take(&mut self.ctx.generator_yield_operand_types);
        let saved_had_ts7057 = std::mem::replace(&mut self.ctx.generator_had_ts7057, false);

        let body_request = self.function_body_statement_request(false, contextual_type);
        self.check_function_body_statement_with_own_literal_context(body, &body_request);
        let inferred_yield = self.check_generator_body_return(GeneratorBodyReturnCheckCtx {
            is_generator: true,
            has_type_annotation,
            annotated_return_type,
            return_type,
            type_annotation,
            idx,
            function_is_async,
            early_yield_type,
            name_node: None,
            name_for_error: None,
        });
        let final_yield = self.concrete_or_empty_generator_yield(body, inferred_yield);

        self.ctx.generator_yield_operand_types = saved_yield_collection;
        self.ctx.generator_had_ts7057 = saved_had_ts7057;
        self.ctx.iteration_depth = saved_cf_context.0;
        self.ctx.switch_depth = saved_cf_context.1;
        self.ctx.label_stack.truncate(saved_cf_context.2);
        self.ctx.had_outer_loop = saved_cf_context.3;
        yield_snapshot.rollback(&mut self.ctx.speculation_state());
        final_yield
    }

    fn concrete_or_empty_generator_yield(
        &self,
        body: NodeIndex,
        inferred_yield: Option<TypeId>,
    ) -> Option<TypeId> {
        match inferred_yield {
            Some(yield_t)
                if yield_t != TypeId::NEVER || !self.generator_body_contains_yield(body, true) =>
            {
                Some(yield_t)
            }
            _ => None,
        }
    }

    /// Whether `node_idx` contains a `yield` / `yield*` for this generator, not
    /// one nested inside another function or class.
    fn generator_body_contains_yield(&self, node_idx: NodeIndex, is_root: bool) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if !is_root && Self::is_nested_function_or_class_boundary(node.kind) {
            return false;
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| self.generator_body_contains_yield(child, false))
    }

    /// Whether the generator body contains a `yield*` whose operand feeds back
    /// into the very aggregate the delegation produces.
    ///
    /// The recovery pass below walks the body a second time to recover a yield
    /// type. That is safe for a delegation whose operand type is already
    /// knowable (`yield* [1, 2]`, `yield* inner()`, `yield* someIterable`), but
    /// not for the *evolving-binding* shape:
    ///
    /// ```text
    /// function* stream() {
    ///     var bucket = []          // evolving, no annotation
    ///     while (true) {
    ///         bucket = yield* bucket   // type depends on the aggregate it delegates to
    ///     }
    /// }
    /// ```
    ///
    /// There the operand's type is only settled by the real declaration body
    /// pass, so pre-checking it here consumes the implicit-any diagnostics
    /// (`TS7005`/`TS7034`) that pass owns. Detect that structurally — a `yield*`
    /// operand mentioning an un-annotated `var`/`let` binder introduced by this
    /// same body — rather than bailing on every `yield*`, which erases the
    /// inferred yield type for every ordinary delegation as well.
    fn generator_body_has_self_referential_yield_star(&self, body: NodeIndex) -> bool {
        let mut evolving: Vec<String> = Vec::new();
        self.collect_evolving_body_binders(body, true, &mut evolving);
        if evolving.is_empty() {
            return false;
        }
        self.yield_star_operand_mentions(body, true, &evolving)
    }

    /// Collect un-annotated `var`/`let` binding names introduced directly by
    /// this generator body (not by a nested function or class).
    fn collect_evolving_body_binders(
        &self,
        node_idx: NodeIndex,
        is_root: bool,
        out: &mut Vec<String>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        if !is_root && Self::is_nested_function_or_class_boundary(node.kind) {
            return;
        }
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.ctx.arena.get_variable_declaration(node)
            && decl.type_annotation.is_none()
            && let Some(name_node) = self.ctx.arena.get(decl.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            out.push(ident.escaped_text.to_string());
        }
        for child in self.ctx.arena.get_children(node_idx) {
            self.collect_evolving_body_binders(child, false, out);
        }
    }

    /// Whether some `yield*` operand in this body mentions one of `binders`.
    fn yield_star_operand_mentions(
        &self,
        node_idx: NodeIndex,
        is_root: bool,
        binders: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if !is_root && Self::is_nested_function_or_class_boundary(node.kind) {
            return false;
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION
            && let Some(yield_expr) = self.ctx.arena.get_unary_expr_ex(node)
            && yield_expr.asterisk_token
            && self.subtree_mentions_binder(yield_expr.expression, binders)
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| self.yield_star_operand_mentions(child, false, binders))
    }

    /// Whether `node_idx`'s subtree references any identifier in `binders`.
    fn subtree_mentions_binder(&self, node_idx: NodeIndex, binders: &[String]) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if let Some(ident) = self.ctx.arena.get_identifier(node)
            && binders.iter().any(|b| b == ident.escaped_text.as_str())
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| self.subtree_mentions_binder(child, binders))
    }

    const fn is_nested_function_or_class_boundary(kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::METHOD_DECLARATION
            || kind == syntax_kind_ext::GET_ACCESSOR
            || kind == syntax_kind_ext::SET_ACCESSOR
            || kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION
    }
}
