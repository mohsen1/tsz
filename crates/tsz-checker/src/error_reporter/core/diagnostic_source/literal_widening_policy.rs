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
        let ast_literal = self.literal_type_from_initializer(arg_idx);
        let basis = ast_literal.unwrap_or(source);
        let source_primitive =
            crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, basis);
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
        if let Some(inner) =
            crate::query_boundaries::common::no_infer_inner_type(self.ctx.types, target)
        {
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
        if crate::query_boundaries::common::literal_value(self.ctx.types, target).is_some() {
            let prim =
                crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, target);
            scan.literal_primitive_bases.insert(prim);
            return;
        }
        // Template literal types (e.g. `:${string}:`) are string-shaped and
        // act as string literals for the matching purposes.
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target) {
            scan.literal_primitive_bases.insert(TypeId::STRING);
            return;
        }
        // unique symbol literals are symbol-shaped.
        if crate::query_boundaries::common::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            scan.literal_primitive_bases.insert(TypeId::SYMBOL);
            return;
        }
        // Enums carry a primitive (string or number), but the public query
        // surface doesn't expose it cheaply. Treat them as unclassifiable so
        // the caller falls back to the conservative literal-preserving
        // default — that matches existing enum diagnostic behaviour.
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some() {
            scan.has_unclassifiable_member = true;
            return;
        }
        // Recurse into unions / intersections.
        if let Some(list) = crate::query_boundaries::common::union_list_id(self.ctx.types, target)
            .or_else(|| {
                crate::query_boundaries::common::intersection_list_id(self.ctx.types, target)
            })
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
