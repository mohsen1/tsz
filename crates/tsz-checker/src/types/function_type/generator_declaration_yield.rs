use super::super::function_type_helpers::{GeneratorBodyReturnCheckCtx, InferredGeneratorYield};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
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
    ) -> InferredGeneratorYield {
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
        // This speculative pass is not diagnostic-safe for a `yield*` whose
        // delegate reads an evolving (`var o;` / `let x = []`) binding: that
        // binding's type depends circularly on the very yield* aggregate being
        // inferred here, so running the body check resolves the circularity as
        // a side effect and the later "real" declaration-check pass no longer
        // sees it as unresolved — silently dropping the implicit-any
        // diagnostics it owns (TS2322/TS7005/TS7034). This is not JS-specific:
        // TypeScript's own `yieldExpressionInControlFlow.ts` conformance
        // fixture hits the identical shape in a plain `.ts` file
        // (`var o = []; while (true) { o = yield* o }`), which is why an
        // earlier narrowing to `is_js_file()` alone regressed the corpus.
        // The hazard is the *evolving delegate*, not `yield*` itself, so the
        // gate is that shape structurally — an ordinary delegate (array,
        // annotated generator, `const` binding, type parameter) infers through.
        if self.generator_body_delegates_to_evolving_binding(body, true) {
            return InferredGeneratorYield::NONE;
        }

        let yield_snapshot = self.ctx.snapshot_return_type();
        let saved_cf_context = self.ctx.enter_function_like_control_flow();
        let saved_yield_collection = std::mem::take(&mut self.ctx.generator_yield_operand_types);
        let saved_had_ts7057 = std::mem::replace(&mut self.ctx.generator_had_ts7057, false);
        // `checked_classes` is not part of `snapshot_return_type`. This
        // suppressed pass walks the whole body, so any nested class it reaches
        // gets marked there as fully checked — but with its diagnostics rolled
        // back below. Left in place, that mark makes the later real declaration
        // check treat the class as "already checked" and skip it, silently
        // dropping every diagnostic its members owe (TS2322 on a member body,
        // TS1308/TS1166 on a computed name, TS2507 on the heritage, ...).
        // Snapshot it and restore past the rollback so classes checked
        // speculatively are re-checked for real. A generator never draws its
        // yield type from a nested class body (classes own their own yields),
        // so un-memoizing them costs the signature pass nothing. `checking_classes`
        // needs no snapshot: it is a recursion guard that `check_class_declaration`
        // balances (every insert has a matching remove on completion), so a pass
        // that runs to completion leaves it exactly as it found it.
        let saved_checked_classes = self.ctx.checked_classes.clone();

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
        let final_yield = self.concrete_or_empty_generator_yield(body, inferred_yield.yield_type);
        // A dropped yield type (an empty/never aggregate) means this speculative
        // pass had nothing to say about the signature at all, so its delegated
        // `TNext` is dropped with it rather than being applied to a `Generator`
        // whose `TYield` came from somewhere else.
        let delegated_next_type = final_yield.and(inferred_yield.delegated_next_type);

        self.ctx.generator_yield_operand_types = saved_yield_collection;
        self.ctx.generator_had_ts7057 = saved_had_ts7057;
        self.ctx.exit_function_like_control_flow(saved_cf_context);
        yield_snapshot.rollback(&mut self.ctx.speculation_state());
        self.ctx.checked_classes = saved_checked_classes;
        InferredGeneratorYield {
            yield_type: final_yield,
            delegated_next_type,
        }
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
        if !is_root && Self::node_kind_owns_its_own_yields(node.kind) {
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

    /// Whether `node_idx` contains a `yield*` whose delegate operand reads an
    /// evolving (implicit-any) binding, for this generator rather than one
    /// nested inside another function or class.
    ///
    /// This is the shape the speculative pass must not run on: the delegate's
    /// own type is still being derived from control flow, and one of the
    /// assignments feeding it is the `yield*` aggregate this pass is computing.
    fn generator_body_delegates_to_evolving_binding(
        &self,
        node_idx: NodeIndex,
        is_root: bool,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if !is_root && Self::node_kind_owns_its_own_yields(node.kind) {
            return false;
        }
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION
            && let Some(yield_expr) = self.ctx.arena.get_unary_expr_ex(node)
            && yield_expr.asterisk_token
            && self.expression_reads_evolving_binding(yield_expr.expression)
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| self.generator_body_delegates_to_evolving_binding(child, false))
    }

    /// Whether any reference anywhere in the delegate operand's subtree
    /// resolves to an evolving binding.
    ///
    /// The whole subtree counts, nested closures included: a delegate built
    /// from a closure that reads the evolving binding still routes that
    /// binding's unresolved type into this aggregate, and bailing is the safe
    /// direction.
    fn expression_reads_evolving_binding(&self, node_idx: NodeIndex) -> bool {
        if node_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16
            && self
                .flow_analyzer()
                .reference_is_evolving_array_symbol(node_idx)
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| self.expression_reads_evolving_binding(child))
    }

    /// Node kinds that introduce a generator/function boundary, so their
    /// `yield`s belong to that inner function rather than the one being
    /// inferred.
    const fn node_kind_owns_its_own_yields(kind: u16) -> bool {
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
