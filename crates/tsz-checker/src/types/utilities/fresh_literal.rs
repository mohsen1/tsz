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
//!
//! Enum members participate in the same model (#15445): a direct member
//! access (`E.A`) mints a fresh enum literal that widens to the parent enum
//! at mutable observation points, while a non-fresh source — an annotated
//! const reference, a property read, a call result — keeps the member type,
//! exactly mirroring `tsc`'s fresh/regular enum literal types. The solver's
//! primitive wideners leave `TypeData::Enum` untouched, so the enum arm of
//! the widening lives here, next to the freshness gate.

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

    /// Does `expr` observe `tsc`'s literal-widening rule at a widening
    /// point? True for fresh literal expressions — including a direct
    /// enum-member access (`E.A`), which mints a fresh enum literal that
    /// widens to `E` — and false for non-fresh sources (variable references,
    /// narrowed values, computed expressions), which keep their type.
    /// [`Self::widen_mutable_binding_initializer_type`] applies the same rule
    /// in widening form.
    pub(crate) fn is_widening_literal_source(&self, expr: NodeIndex) -> bool {
        self.is_fresh_literal_expression(expr)
    }

    /// Widen a mutable binding's initializer type when the initializer is a
    /// fresh literal expression; non-fresh sources keep their type. This is
    /// the widening form of [`Self::is_widening_literal_source`].
    ///
    /// The enum arm is freshness-gated exactly like the primitive arm
    /// (#15445): `let x = E.A` widens to `E`, while an annotated const
    /// reference (`const c: E.A = E.A; let x = c`) keeps `E.A`.
    pub(crate) fn widen_mutable_binding_initializer_type(
        &mut self,
        initializer: NodeIndex,
        init_type: TypeId,
    ) -> TypeId {
        if !self.is_fresh_literal_expression(initializer) {
            return init_type;
        }
        // The solver's primitive wideners leave `TypeData::Enum` untouched,
        // so fold a fresh enum member (bare, or a union constituent minted
        // by a conditional over member accesses) to its parent enum here.
        // The def-id-only gate intentionally excludes
        // `widen_enum_member_type`'s legacy symbol-flags fallback, keeping
        // the fresh path free of unconditional symbol probes.
        if self.is_enum_member_type_for_widening(init_type) {
            return self.widen_enum_member_type(init_type);
        }
        let folded = self.widen_fresh_union_enum_member_constituents(init_type);
        crate::query_boundaries::widening::widen_type_for_mutable_binding(self.ctx.types, folded)
    }

    /// Widen `ty` to its base only when `expr` is a fresh literal
    /// expression; a non-fresh source (typed identifier reference, declared
    /// literal union) keeps its literal type. Mirrors `tsc`'s
    /// `getWidenedLiteralType` applied to fresh types only.
    ///
    /// Enum members are left unchanged: the underlying primitive widener
    /// skips `TypeData::Enum`, and the enum-to-parent fold is owned by the
    /// binding/return widening paths above, not by this display-adjacent
    /// helper.
    pub(crate) fn widen_expression_type_if_fresh(&mut self, expr: NodeIndex, ty: TypeId) -> TypeId {
        if self.is_fresh_literal_expression(expr) {
            self.widen_literal_type(ty)
        } else {
            ty
        }
    }

    /// Fold enum-member constituents of a fresh initializer's top-level
    /// union to their parent enum (`cond ? E.A : E.B` → `E`), mirroring
    /// `tsc`'s `getWidenedLiteralType`, which maps union constituents and
    /// widens each fresh enum literal via `getBaseTypeOfEnumLikeType`.
    /// Non-union, non-enum types return unchanged. Reached only from the
    /// fresh-initializer path, so folding every enum-member constituent is
    /// safe — they originate from direct member accesses.
    fn widen_fresh_union_enum_member_constituents(&mut self, ty: TypeId) -> TypeId {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty)
        else {
            return ty;
        };
        if !members
            .iter()
            .any(|&m| self.is_enum_member_type_for_widening(m))
        {
            return ty;
        }
        let folded: Vec<TypeId> = members
            .into_iter()
            .map(|m| {
                if self.is_enum_member_type_for_widening(m) {
                    self.widen_enum_member_type(m)
                } else {
                    m
                }
            })
            .collect();
        self.ctx.types.factory().union(folded)
    }
}
