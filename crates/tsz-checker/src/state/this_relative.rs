use super::state::CheckerState;
use crate::query_boundaries::common;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Choose the binding for `this`-at-return-position substitution given a
    /// call's receiver type.
    ///
    /// When the receiver is itself a *compound* `this`-relative type (e.g.
    /// `this.stack: this[]` accessed inside the class body), the return type's
    /// `this` is the *same* polymorphic `this` and must stay polymorphic;
    /// binding it to the this-bearing receiver would spuriously nest it
    /// (`pop(): this` would become `this[]`, drawing a false TS2322). Falling
    /// back to the canonical `this` makes the substitution an identity for
    /// `this` positions, preserving the polymorphic `this`. A receiver that is
    /// not this-relative (or is exactly `this`) is used verbatim.
    pub(crate) fn this_substitution_target_for_receiver(&self, receiver_type: TypeId) -> TypeId {
        if self.type_is_compound_this_relative(receiver_type) {
            self.ctx.types.this_type()
        } else {
            receiver_type
        }
    }

    /// True when `type_id` mentions the polymorphic `this` but is not the
    /// canonical `this` itself — i.e. a *compound* `this`-relative type such as
    /// `this[]`, `this | undefined`, or `Foo & this`.
    ///
    /// Substituting a member's `this` with such a type would nest the
    /// polymorphic `this` one level too deep, so the receiver-`this` binding
    /// sites treat it as "leave `this` polymorphic" rather than rebinding
    /// (issue #14512). A bare `this` is exempt: substituting `this -> this` is
    /// an identity no-op, so the existing direct-`this`-receiver paths are
    /// preserved.
    pub(crate) fn type_is_compound_this_relative(&self, type_id: TypeId) -> bool {
        type_id != self.ctx.types.this_type() && common::contains_this_type(self.ctx.types, type_id)
    }
}
