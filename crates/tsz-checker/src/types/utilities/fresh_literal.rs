//! Literal-freshness policy owner (#15390).
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
//! This checker-side module owns the freshness *policy* rules (literal
//! recovery itself stays with `literal_type_from_initializer`; the
//! underlying type widening lives in `crate::query_boundaries::widening`).
//! New display or binding consumers must route through these methods instead
//! of pairing raw `literal_type_from_initializer` /
//! `is_fresh_literal_expression` calls with ad-hoc widening at the call
//! site. The long-term fix is carrying a freshness bit on primitive literal
//! types (as the solver already does for fresh object literals) so widening
//! can consult the type instead of the AST.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
        self.literal_type_from_initializer(expr).unwrap_or(checked)
    }

    /// Widen a mutable binding's initializer type when the initializer is a
    /// fresh literal expression; non-fresh sources keep their type.
    ///
    /// Enum members follow the same freshness rule (#15444, #15445): a direct
    /// member access (`E.A`) is fresh — [`Self::is_fresh_literal_expression`]
    /// resolves it to an `ENUM_MEMBER` symbol — and widens to the parent enum
    /// type `E`, while a non-fresh reference (an annotated const, a property
    /// read) keeps the member type. The enum probe lives inside
    /// [`Self::widen_initializer_type_for_mutable_binding`], so the fresh arm
    /// handles both primitive literals and enum-member accesses.
    pub(crate) fn widen_mutable_binding_initializer_type(
        &mut self,
        initializer: NodeIndex,
        init_type: TypeId,
    ) -> TypeId {
        if self.is_fresh_literal_expression(initializer) {
            return self.widen_initializer_type_for_mutable_binding(init_type);
        }
        init_type
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
