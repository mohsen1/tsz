//! Source-literal preservation policy for TS2345 argument-mismatch displays.
//!
//! tsc has a subtle rule for whether a literal source like `1` should be
//! shown as `'1'` or widened to `'number'` in
//! `Argument of type X is not assignable to parameter of type Y.` messages.
//! `is_literal_sensitive_assignment_target` answers "does the target contain
//! any literal-shaped member?" but that alone is not sufficient: a target
//! union like `string | "hello"` is literal-sensitive because of `"hello"`,
//! yet tsc still widens a number-literal source to `'number'` because the
//! target collapses to the single primitive `string` for display.
//!
//! Rule (verified against tsc 6.0.3 across permutations of `string | <lit>`,
//! `number | <lit>`, single-literal, all-literal-union, mixed-primitive
//! targets, and `boolean | null | undefined` style targets):
//!
//! Source widens to its primitive base iff the target contains a *plain*
//! primitive `P` AND a literal whose primitive base is also `P` (i.e. tsc
//! has a primitive-shaped collapse target available), AND the source's
//! primitive base differs from `P`.
//!
//! In every other case the source literal is preserved:
//!  * single literal targets (`bar(x: T = 1, "")` keeps `'""'`),
//!  * all-literal unions (`fA(x: 1 | 2)("foo")` keeps `'"foo"'`),
//!  * mixed-primitive unions whose literals don't share a primitive with any
//!    plain primitive in the target (`fA(x: string | 1)(2n)` keeps `'2n'`),
//!  * targets with only plain primitives and unit-like members
//!    (`takes(x: boolean | null | undefined)(0)` keeps `'0'`).
//!
//! NOTE: this helper alone is not sufficient for every TS2345 fingerprint
//! gap — when the failure-analysis layer narrows a union to a single literal
//! constituent before the target reaches the display path (e.g. inside
//! `unionTypeInference.ts`'s `f1<T>(x: T, y: string | T)` repro), the helper
//! sees only the literal and conservatively preserves the source. The full
//! fix requires the upstream constituent selector to surface the union (or
//! its primitive base) instead of a single literal member; that is tracked
//! separately.
//!
//! This module is split out of `diagnostic_source.rs` to keep that file
//! under the architecture LOC ceiling.

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

#[derive(Default)]
struct TargetPrimitiveScan {
    /// Primitive bases that appear as a *plain* primitive (not a literal of
    /// that primitive) somewhere in the target.
    plain_primitives: FxHashSet<TypeId>,
    /// Primitive bases of literal members (e.g. `"hello"` → `string`,
    /// `1` → `number`, `true` → `boolean`).
    literal_primitive_bases: FxHashSet<TypeId>,
    /// Set when a leaf member doesn't have a meaningful primitive base
    /// (e.g. type parameters, generic instantiations, errors). Forces the
    /// caller to fall back to the conservative preserve-source default.
    has_unclassifiable_member: bool,
}

impl<'a> CheckerState<'a> {
    /// Whether a fresh string/number/bigint literal `source` must widen to its
    /// primitive base for an assignment-failure display against `target`,
    /// because `target` does not admit a literal of the *source's* primitive
    /// domain.
    ///
    /// tsc widens a fresh literal property to its primitive whenever the target
    /// property type is not literal-preferring for that literal's domain — so
    /// `{ configurable: "yes" }` against `{ configurable?: boolean }` renders
    /// `string`, and `{ f: "yes" }` against `{ f?: 1 | 2 }` renders `string`,
    /// because neither target has a *string*-literal surface. It preserves the
    /// source only when the target admits the source's own domain
    /// (`{ f: "yes" }` against `{ f?: "a" | "b" }` keeps `"yes"`). The existing
    /// literal-sensitivity gate that drives this display is domain-agnostic —
    /// it keeps the literal for any target that could hold a top-level singleton
    /// (`boolean` is stored as `true | false`, numeric-literal unions are
    /// singleton-shaped) — so this refines it with the source domain.
    ///
    /// Boolean literal sources are excluded: tsc keeps `true` / `false`
    /// verbatim in these messages (`{ f: true }` against `{ f?: 1 | 2 }` renders
    /// `true`, never `boolean`). The domain decision reuses the shared
    /// `contextual_type_allows_literal` gateway (tsc's `isLiteralOfContextualType`)
    /// so it is a structural query over the two types, never a predicate over
    /// rendered text.
    pub(in crate::error_reporter) fn scalar_source_widens_across_literal_domain(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let is_widenable_scalar = matches!(
            crate::query_boundaries::common::literal_value(self.ctx.types, source),
            Some(
                tsz_solver::LiteralValue::String(_)
                    | tsz_solver::LiteralValue::Number(_)
                    | tsz_solver::LiteralValue::BigInt(_)
            )
        );
        is_widenable_scalar && !self.contextual_type_allows_literal(target, source)
    }

    /// Returns `true` when the source literal should be preserved verbatim
    /// in the call-argument display, `false` when it should be widened to
    /// its primitive base.
    ///
    /// `source` is the assignability source type (which inference may have
    /// already widened); `arg_idx` is the AST literal expression that the
    /// caller would otherwise spell out verbatim. The AST takes precedence
    /// so the check stays aligned with the literal-text path that actually
    /// drives the display.
    pub(in crate::error_reporter) fn source_literal_primitive_matches_target_literal(
        &mut self,
        source: TypeId,
        arg_idx: NodeIndex,
        target: TypeId,
    ) -> bool {
        let basis = self.expression_display_type_preferring_literal(arg_idx, source);
        let source_primitive = diagnostic_query::widen_literal_to_primitive(self.ctx.types, basis);
        // Basis isn't a literal type — this filter doesn't apply, so leave
        // the existing literal-preserving behaviour untouched.
        if source_primitive == basis {
            return true;
        }
        // Resolve type-parameter substitutions via the type environment so
        // that targets like `string | T` (with `T` inferred to `"hello"`) are
        // analysed in their fully-substituted form. Without this, an
        // unresolved `T` would mark the scan as unclassifiable and we'd
        // over-preserve the source literal.
        let evaluated = self.evaluate_type_with_env(target);
        let target = self.evaluate_type_for_assignability(evaluated);

        let mut scan = TargetPrimitiveScan::default();
        self.scan_target_primitives(target, &mut scan);

        // Anything we couldn't classify (type parameters, deferred generics)
        // → conservative default, preserve the source literal.
        if scan.has_unclassifiable_member {
            return true;
        }
        // Find a primitive base that appears in BOTH plain form and literal
        // form within the target — that's the case where tsc collapses the
        // union to the primitive for display. If no such base exists (only
        // plain primitives, only literals, or plain/literals on different
        // bases), tsc preserves the source.
        let widening_base = scan
            .plain_primitives
            .iter()
            .copied()
            .find(|p| scan.literal_primitive_bases.contains(p));
        let Some(widening_base) = widening_base else {
            return true;
        };
        // The widening base must also differ from the source primitive —
        // otherwise the source literal lands inside the target's literal set
        // and stays informative.
        source_primitive == widening_base
    }

    fn scan_target_primitives(&self, target: TypeId, scan: &mut TargetPrimitiveScan) {
        if scan.has_unclassifiable_member {
            return;
        }
        if let Some(inner) = diagnostic_query::no_infer_inner_type(self.ctx.types, target) {
            self.scan_target_primitives(inner, scan);
            return;
        }
        // Unit-like targets contribute no primitive base. tsc preserves the
        // source literal verbatim in these messages.
        if target == TypeId::NEVER || target == TypeId::UNDEFINED || target == TypeId::NULL {
            return;
        }
        // Plain primitives — register as a primitive that can collapse the
        // target's display.
        if matches!(
            target,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        ) {
            scan.plain_primitives.insert(target);
            return;
        }
        // Literal types — register their primitive base.
        if diagnostic_query::literal_value(self.ctx.types, target).is_some() {
            let prim = diagnostic_query::widen_literal_to_primitive(self.ctx.types, target);
            scan.literal_primitive_bases.insert(prim);
            return;
        }
        // Template literal types (e.g. `:${string}:`) are string-shaped and
        // act as string literals for the matching purposes.
        if diagnostic_query::is_template_literal_type(self.ctx.types, target) {
            scan.literal_primitive_bases.insert(TypeId::STRING);
            return;
        }
        // unique symbol literals are symbol-shaped.
        if diagnostic_query::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            scan.literal_primitive_bases.insert(TypeId::SYMBOL);
            return;
        }
        // Enums carry a primitive (string or number), but the public query
        // surface doesn't expose it cheaply. Treat them as unclassifiable so
        // the caller falls back to the conservative literal-preserving
        // default — that matches existing enum diagnostic behaviour.
        if diagnostic_query::enum_def_id(self.ctx.types, target).is_some() {
            scan.has_unclassifiable_member = true;
            return;
        }
        // Recurse into unions / intersections.
        if let Some(list) = diagnostic_query::union_list_id(self.ctx.types, target)
            .or_else(|| diagnostic_query::intersection_list_id(self.ctx.types, target))
        {
            for member in self.ctx.types.type_list(list).iter().copied() {
                self.scan_target_primitives(member, scan);
                if scan.has_unclassifiable_member {
                    return;
                }
            }
            return;
        }
        // Anything else (object types, type parameters, etc.) — bail out and
        // keep the source literal.
        scan.has_unclassifiable_member = true;
    }
}
