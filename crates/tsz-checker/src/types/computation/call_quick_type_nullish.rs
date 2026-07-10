//! Nullish-callee companion diagnostics for declaration-inferred call results.
//!
//! When a variable-like declaration infers its type from a call-expression
//! initializer, tsc computes that type through `getQuickTypeOfExpression`,
//! which re-checks the callee with `checkNonNullExpression`. For a
//! possibly-nullish callee that re-check reports the
//! `reportObjectPossiblyNullOrUndefinedError` family (TS18047/TS18048/TS18049
//! for entity names, TS2531/TS2532/TS2533 otherwise) in addition to the
//! TS2721/TS2722/TS2723 that call resolution reports. Optional-chain calls
//! take the chain arm of the quick path, which never re-checks the callee, so
//! they never get the companion (a direct `?.()` also suppresses the
//! invoke-family diagnostic upstream; only inherited-chain forms like
//! `a?.b.c()` reach this guard with one already emitted).

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Emit the possibly-nullish companion for a callee that already produced
    /// TS2721/TS2722/TS2723, when the call's type flows into a variable-like
    /// declaration's inferred type (tsc's quick-type path).
    pub(crate) fn report_nullish_callee_declaration_companion(
        &mut self,
        call_idx: NodeIndex,
        callee_expr: NodeIndex,
        cause: TypeId,
    ) {
        if super::access::is_optional_chain(self.ctx.arena, call_idx) {
            return;
        }
        if !self.call_result_infers_declaration_type(call_idx) {
            return;
        }
        self.report_possibly_nullish_expression(callee_expr, cause);
    }

    /// Whether tsc's `getQuickTypeOfExpression` runs for this call: the call,
    /// reachable through parentheses and `await`, is the initializer of a
    /// variable declaration, property declaration, or parameter without a
    /// type annotation, or the default of a binding element (which never
    /// carries its own annotation).
    ///
    /// tsc's gate also excludes `super(...)`, `import(...)`, `require(...)`,
    /// and `Symbol()` calls and skips JSDoc type assertions. Those arms are
    /// deliberately omitted: this predicate only runs after TS2721/TS2722/
    /// TS2723 fired for the callee, and none of those callee forms can be
    /// possibly-nullish (`super`/`import` are keywords, `require`/`Symbol`
    /// resolve to non-nullish globals), while JSDoc assertions only occur in
    /// JS files where `strictNullChecks` call diagnostics take other paths.
    fn call_result_infers_declaration_type(&self, call_idx: NodeIndex) -> bool {
        let arena = self.ctx.arena;
        let mut cur = call_idx;
        // Bounded walk to guard against malformed parent links.
        for _ in 0..100 {
            let Some(parent_idx) = arena.parent_of(cur) else {
                return false;
            };
            let Some(parent) = arena.get(parent_idx) else {
                return false;
            };
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::AWAIT_EXPRESSION =>
                {
                    cur = parent_idx;
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    return arena.get_variable_declaration(parent).is_some_and(|decl| {
                        decl.initializer == cur && decl.type_annotation.is_none()
                    });
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    return arena.get_property_decl(parent).is_some_and(|decl| {
                        decl.initializer == cur && decl.type_annotation.is_none()
                    });
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    return arena.get_parameter(parent).is_some_and(|param| {
                        param.initializer == cur && param.type_annotation.is_none()
                    });
                }
                k if k == syntax_kind_ext::BINDING_ELEMENT => {
                    return arena
                        .get_binding_element(parent)
                        .is_some_and(|element| element.initializer == cur);
                }
                _ => return false,
            }
        }
        false
    }
}
