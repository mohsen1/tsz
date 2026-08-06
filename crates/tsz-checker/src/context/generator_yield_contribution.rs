//! One `yield` operand's contribution to the enclosing unannotated generator's
//! inferred signature, and the collection API the yield dispatcher pushes
//! through.

use super::CheckerContext;
use tsz_solver::TypeId;

/// One `yield` operand's contribution to an unannotated generator's inferred
/// yield type.
///
/// `tsc` widens the aggregated yield type only when it is a single literal that
/// is *fresh* (`getWidenedLiteralType` inside
/// `getWidenedLiteralLikeTypeForContextualIterationTypeIfNeeded`). tsz does not
/// carry freshness on types, so each collection site records whether its
/// operand expression was a widenable (fresh) literal contribution; the
/// aggregation site widens the collapsed literal union only when every
/// contribution agreed.
#[derive(Clone, Copy, Debug)]
pub struct GeneratorYieldContribution {
    /// The yielded value type (element type for `yield*`).
    pub type_id: TypeId,
    /// Whether the operand was a fresh literal expression (or fresh enum-member
    /// access) with no `const`-assertion pinning — i.e. `tsc` would widen it.
    pub widenable: bool,
    /// For a `yield*` operand, the delegate's own declared `TNext` — the type
    /// its `next()` accepts. `tsc` aggregates these across every delegation in
    /// the body (`checkAndAggregateYieldOperandTypes` collects
    /// `getIterationTypeOfIterable(IterationTypeKind.Next, ...)`, then
    /// `getIntersectionType`) and uses the result as the enclosing unannotated
    /// generator's `TNext`. `None` for a plain `yield`, and for a delegate that
    /// declares no `TNext` of its own — those contribute nothing, leaving the
    /// slot at its `unknown` default.
    pub delegated_next_type: Option<TypeId>,
}

impl CheckerContext<'_> {
    /// Record one `yield` operand's contribution to the enclosing unannotated
    /// generator's inferred yield type. Pass `widenable: false` for delegated
    /// (`yield*`) element types — they come from another iterator's declared
    /// type, never a fresh literal.
    pub fn push_generator_yield_contribution(&mut self, type_id: TypeId, widenable: bool) {
        self.generator_yield_operand_types
            .push(GeneratorYieldContribution {
                type_id,
                widenable,
                delegated_next_type: None,
            });
    }

    /// Record one `yield*` operand's contribution: the delegated element type
    /// plus the delegate's own `TNext`, which feeds the enclosing unannotated
    /// generator's `TNext` slot. Delegated element types are never fresh
    /// literals, so `widenable` is always `false` here.
    pub fn push_generator_yield_star_contribution(
        &mut self,
        type_id: TypeId,
        delegated_next_type: Option<TypeId>,
    ) {
        self.generator_yield_operand_types
            .push(GeneratorYieldContribution {
                type_id,
                widenable: false,
                delegated_next_type,
            });
    }
}
