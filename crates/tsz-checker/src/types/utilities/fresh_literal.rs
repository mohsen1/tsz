//! Literal-freshness query boundary (#15390).
//!
//! `tsc` mints a fresh literal type when it checks a literal expression and
//! consults the freshness bit *on the type* when deciding whether an
//! observation point widens it. tsz stores freshness as AST-shape predicates
//! instead, so consumers used to re-derive the literal from the AST (via
//! `literal_type_from_initializer`) or re-model the widening timing (via
//! `is_fresh_literal_expression` plus a widening call) independently at each
//! call site — and every consumer that forgot the recovery or re-modeled the
//! rule wrongly diverged from `tsc` (#15366, #15373).
//!
//! This module is the single owner of those rules:
//!
//! * **Recovery** — [`CheckerState::fresh_literal_type_of`] and
//!   [`CheckerState::expression_display_type_preferring_literal`] recover the
//!   unwidened literal type from the expression node for diagnostic display.
//! * **Widening timing** —
//!   [`CheckerState::widen_mutable_binding_initializer_type`] (mutable
//!   `let`/`var`/parameter/property-flow bindings, including `tsc`'s
//!   enum-member widening) and [`CheckerState::widen_expression_type_if_fresh`]
//!   (plain literal widening gated on freshness) own *when* a fresh literal
//!   widens.
//!
//! New display or binding consumers must route through these methods instead
//! of pairing raw `literal_type_from_initializer` /
//! `is_fresh_literal_expression` calls with ad-hoc widening at the call site.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Recover the fresh (unwidened) literal type of an expression from its
    /// AST node, or `None` when the expression is not a literal.
    ///
    /// This is the recovery entry point of the freshness boundary: it owns
    /// the AST walk (`literal_type_from_initializer`) plus the fallback for
    /// `NumericLiteral` nodes minted on secondary parser paths with
    /// `value: None`, whose value is recoverable from the literal text
    /// (hex/binary/octal/separator forms).
    pub(crate) fn fresh_literal_type_of(&self, expr: NodeIndex) -> Option<TypeId> {
        self.literal_type_from_initializer(expr).or_else(|| {
            let node = self.ctx.arena.get(expr)?;
            if node.kind != SyntaxKind::NumericLiteral as u16 {
                return None;
            }
            let lit = self.ctx.arena.get_literal(node)?;
            tsz_common::numeric::parse_numeric_literal_value(&lit.text)
                .map(|value| self.ctx.types.literal_number(value))
        })
    }

    /// The type a diagnostic should display for `expr`: its own fresh
    /// literal type when the expression is a literal, otherwise the checked
    /// (possibly widened) type the caller already computed.
    ///
    /// `tsc` renders an operand's unwidened checked type in diagnostics —
    /// widening to the base primitive happens only at binding observation
    /// points, never for the message — so `f(42)` shows `Type '42'`, not
    /// `Type 'number'`.
    pub(crate) fn expression_display_type_preferring_literal(
        &self,
        expr: NodeIndex,
        checked: TypeId,
    ) -> TypeId {
        self.fresh_literal_type_of(expr).unwrap_or(checked)
    }

    /// Widen a mutable binding's initializer type exactly when `tsc` would:
    /// the initializer is a fresh literal expression, or the type is an enum
    /// member literal (`let x = E.A` widens to `E` even though `E.A` is not
    /// an AST literal). Non-fresh sources — variable references, narrowed
    /// values, computed expressions — keep their type.
    pub(crate) fn widen_mutable_binding_initializer_type(
        &mut self,
        initializer: NodeIndex,
        init_type: TypeId,
    ) -> TypeId {
        if self.is_enum_member_type_for_widening(init_type)
            || self.is_fresh_literal_expression(initializer)
        {
            self.widen_initializer_type_for_mutable_binding(init_type)
        } else {
            init_type
        }
    }

    /// Widen `ty` to its base only when `expr` is a fresh literal
    /// expression; a non-fresh source (typed identifier reference, declared
    /// literal union) keeps its literal type. Mirrors `tsc`'s
    /// `getWidenedLiteralType` applied to fresh types only.
    pub(crate) fn widen_expression_type_if_fresh(&mut self, expr: NodeIndex, ty: TypeId) -> TypeId {
        if self.is_fresh_literal_expression(expr) {
            self.widen_literal_type(ty)
        } else {
            ty
        }
    }
}
