//! Type computation for conditional (ternary) expressions
//! (`condition ? whenTrue : whenFalse`).
//!
//! Split out of `helpers.rs` to keep that module under the checker's
//! per-file line limit; the conditional-expression concern (branch typing,
//! dead-branch diagnostic suppression, and tsc's `UnionReduction.Subtype`
//! collapse) lives here.

use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::query_boundaries::assignability;
use crate::query_boundaries::common;
use crate::query_boundaries::type_computation::core as expr_ops;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a conditional expression (ternary operator).
    ///
    /// Computes the type of `condition ? whenTrue : whenFalse`.
    /// Returns the union of the two branch types if they differ.
    ///
    /// When a contextual type is available, each branch is checked against it
    /// to catch type errors (TS2322).
    ///
    /// Uses `solver::compute_conditional_expression_type` for type computation
    /// as part of the Solver-First architecture migration.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_conditional_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_conditional_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR;
        };

        // Get condition type for type computation
        let condition_type = self.get_type_of_node(cond.condition);
        self.check_truthy_or_falsy_with_type(cond.condition, condition_type);
        // TS2774: check for non-nullable callable tested for truthiness
        self.check_callable_truthiness(cond.condition, Some(cond.when_true));

        // Apply contextual typing to each branch for better inference,
        // but don't check assignability here - that happens at the call site.
        // This allows `cond ? "a" : "b"` to infer as `"a" | "b"` and then
        // the union is checked against the contextual type.
        let contextual_type = request.contextual_type;

        // Preserve literal types in conditional branches so that
        // `const x = cond ? "a" : "b"` infers `"a" | "b"` (tsc behavior).
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;

        // tsc always evaluates BOTH branches and unions them for the result
        // type, even when the condition is a literal boolean.  This ensures
        // `var r = true ? t : u; var r = true ? u : t;` computes the same
        // union type regardless of branch order (fixing false TS2403).
        //
        // When the condition IS a literal boolean, the dead branch may contain
        // code that would emit false diagnostics (e.g. TS2454 for variables
        // that are genuinely uninitialized on that path).  We suppress
        // diagnostics from the dead branch by snapshot/restore.
        use tsz_scanner::SyntaxKind;
        let condition_is_true = self
            .ctx
            .arena
            .get(cond.condition)
            .is_some_and(|n| n.kind == SyntaxKind::TrueKeyword as u16);
        let condition_is_false = self
            .ctx
            .arena
            .get(cond.condition)
            .is_some_and(|n| n.kind == SyntaxKind::FalseKeyword as u16);

        let should_suppress_contextual_branch_assignability =
            contextual_type.is_some() && !self.assignment_source_is_return_expression(idx);
        let suppress_contextual_branch_ts2322 =
            |state: &mut Self, branch_idx: NodeIndex, snap: DiagnosticSpeculationSnapshot| {
                if !should_suppress_contextual_branch_assignability {
                    snap.commit(&mut state.ctx.diagnostic_state());
                    return;
                }
                let Some(branch_node) = state.ctx.arena.get(branch_idx) else {
                    snap.commit(&mut state.ctx.diagnostic_state());
                    return;
                };
                let branch_start = branch_node.pos;
                let branch_end = branch_node.end;
                snap.rollback_filtered(&mut state.ctx.diagnostic_state(), |diag| {
                    let in_branch = diag.start >= branch_start && diag.start < branch_end;
                    !(in_branch && diag.code == 2322)
                });
            };

        // Compute branch types with the outer contextual type for inference.
        // Use per-branch requests so each branch gets its own narrowed contextual type.
        let true_ctx = contextual_type
            .map(|ctx| self.contextual_type_for_conditional_branch(ctx, cond.when_true));
        let true_request = request.contextual_opt(true_ctx);
        let when_true = if condition_is_false {
            // Dead branch — suppress diagnostics but still compute type.
            // Must save/restore BOTH the diagnostics vec AND the dedup set,
            // otherwise entries added to the dedup set would prevent the same
            // diagnostic from being emitted later by the regular checker pass
            // (e.g. TS8010 grammar errors in JS files).
            self.speculative_type_of_node(cond.when_true, &true_request)
        } else {
            let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
            let ty = self.get_type_of_node_with_request(cond.when_true, &true_request);
            suppress_contextual_branch_ts2322(self, cond.when_true, snap);
            ty
        };

        let false_ctx = contextual_type
            .map(|ctx| self.contextual_type_for_conditional_branch(ctx, cond.when_false));
        let false_request = request.contextual_opt(false_ctx);
        let when_false = if condition_is_true {
            // Dead branch — suppress diagnostics but still compute type.
            self.speculative_type_of_node(cond.when_false, &false_request)
        } else {
            let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
            let ty = self.get_type_of_node_with_request(cond.when_false, &false_request);
            suppress_contextual_branch_ts2322(self, cond.when_false, snap);
            ty
        };

        self.ctx.preserve_literal_types = prev_preserve;

        // Do NOT widen branch literal types here. In tsc, conditional expressions
        // preserve literal types (possibly "fresh") and widening is deferred to the
        // point of use: `let`/`var` declarations widen via
        // `widen_initializer_type_for_mutable_binding`, and return type inference
        // widens via `widen_literal_type` in `infer_return_type_from_body`.
        // Eagerly widening here caused false TS2322 errors when the result was
        // assigned to a `const` with a literal union annotation, e.g.:
        //   const c1 = cond ? "foo" : "bar";        // should be "foo" | "bar"
        //   const c2: "foo" | "bar" = c1;            // should pass
        if common::literal_value(self.ctx.types, when_true).is_some()
            || common::literal_value(self.ctx.types, when_false).is_some()
        {
            return self
                .ctx
                .types
                .factory()
                .union_preserve_members(vec![when_true, when_false]);
        }

        // tsc computes a conditional expression's type via
        // `getUnionType([whenTrue, whenFalse], UnionReduction.Subtype)`, which
        // drops a branch whose type is a subtype of the other branch. The
        // solver's interner-level union cannot subtype-reduce members that need
        // type-environment resolution (`Lazy`/`Application`/class refs), so a
        // `cond ? sub : super` expression survives as a 2-member union (e.g.
        // `{} | Record<string, unknown>` or `Sub | Super`). That stray subtype
        // member then breaks downstream index/property checks (false TS7053 on
        // `{}`). Apply the binary subtype reduction here, where the resolver is
        // available, keeping the surviving branch's original id so its alias
        // display is preserved. Skip it for `any`/`never` conditions so the
        // solver keeps its dedicated handling (plain union for `any`,
        // unreachable `never`).
        if condition_type != TypeId::ANY
            && condition_type != TypeId::NEVER
            && let Some(reduced) = self.reduce_conditional_subtype_branches(when_true, when_false)
        {
            return reduced;
        }

        // Use Solver API for type computation (Solver-First architecture)
        expr_ops::compute_conditional_expression_type(
            self.ctx.types,
            condition_type,
            when_true,
            when_false,
        )
    }

    /// Mirror tsc's `UnionReduction.Subtype` for the two branches of a
    /// conditional expression: when one branch type is a subtype of the other,
    /// the union collapses to the supertype. Returns the surviving branch's
    /// original `TypeId` (preserving alias display), or `None` when neither
    /// branch subsumes the other and the normal union should be built.
    ///
    /// Both-fresh-object-literal pairs are intentionally excluded: tsc keeps
    /// those as a complement union (e.g. `{ a: number; b?: undefined } | { a:
    /// number; b: number }`), which the solver's
    /// `compute_conditional_expression_type` already reproduces.
    fn reduce_conditional_subtype_branches(
        &mut self,
        when_true: TypeId,
        when_false: TypeId,
    ) -> Option<TypeId> {
        if when_true == when_false {
            return None;
        }
        // `any`/`unknown`/`error`/`never` have dedicated union semantics
        // (absorption, propagation, removal) handled by the solver union.
        for ty in [when_true, when_false] {
            if ty == TypeId::ANY
                || ty == TypeId::UNKNOWN
                || ty == TypeId::ERROR
                || ty == TypeId::NEVER
            {
                return None;
            }
        }
        if common::is_fresh_object_type(self.ctx.types, when_true)
            && common::is_fresh_object_type(self.ctx.types, when_false)
        {
            return None;
        }
        let resolved_true = self.evaluate_type_for_assignability(when_true);
        let resolved_false = self.evaluate_type_for_assignability(when_false);
        let true_sub_false =
            assignability::is_fresh_subtype_of(self.ctx.types, resolved_true, resolved_false);
        let false_sub_true =
            assignability::is_fresh_subtype_of(self.ctx.types, resolved_false, resolved_true);
        match (true_sub_false, false_sub_true) {
            // Proper subtype: drop it, keep the supertype branch.
            (true, false) => Some(when_false),
            (false, true) => Some(when_true),
            // Mutual subtypes (e.g. `{}` vs `{ [k: string]: V }`, which are
            // assignable both ways): tsc keeps the structured member and drops
            // the contentless empty object type. When both or neither side is
            // an empty object, the branches are interchangeable, so leave the
            // union for the solver to build rather than guessing a winner.
            (true, true) => {
                let true_empty = common::is_empty_object_type(self.ctx.types, resolved_true);
                let false_empty = common::is_empty_object_type(self.ctx.types, resolved_false);
                match (true_empty, false_empty) {
                    (true, false) => Some(when_false),
                    (false, true) => Some(when_true),
                    _ => None,
                }
            }
            (false, false) => None,
        }
    }
}
