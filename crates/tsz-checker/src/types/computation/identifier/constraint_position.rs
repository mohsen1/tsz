//! Constraint-position substitution for references (tsc's
//! `getNarrowableTypeForReference` / `isConstraintPosition`).
//!
//! A reference to a generic type parameter whose base constraint is a union
//! including `null`/`undefined` is seen as that base constraint when it is the
//! object of a property/element access or the target of a call/new, so the
//! access is possibly-undefined before a guard and narrowed afterwards.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

/// Syntactic constraint position of a reference, used by
/// [`CheckerState::narrowable_type_for_reference`].
enum ConstraintPosition {
    /// Object of a property access, target of a call/new — always a constraint
    /// position.
    NonElement,
    /// Object of an element access `obj[arg]`. Whether this is a constraint
    /// position depends on the object/index types (the deferred `T[K]` case).
    ElementObject(NodeIndex),
}

impl<'a> CheckerState<'a> {
    /// Classify the syntactic position of a reference for tsc's
    /// `isConstraintPosition`. Returns `None` when the reference is not the
    /// object of a property/element access nor the target of a call/new.
    fn constraint_position_for_reference(&self, idx: NodeIndex) -> Option<ConstraintPosition> {
        let ext = self.ctx.arena.get_extended(idx)?;
        let parent_node = self.ctx.arena.get(ext.parent)?;
        let kind = parent_node.kind;
        if kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(parent_node)?;
            return (access.expression == idx).then_some(ConstraintPosition::NonElement);
        }
        if kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(parent_node)?;
            return (access.expression == idx)
                .then_some(ConstraintPosition::ElementObject(access.name_or_argument));
        }
        if kind == syntax_kind_ext::CALL_EXPRESSION || kind == syntax_kind_ext::NEW_EXPRESSION {
            let call = self.ctx.arena.get_call_expr(parent_node)?;
            return (call.expression == idx).then_some(ConstraintPosition::NonElement);
        }
        None
    }

    /// tsc's `getNarrowableTypeForReference` (constraint-position subset).
    ///
    /// When a reference to a generic type parameter with a union/nullable
    /// constraint appears in a *constraint position* — the object of a
    /// property/element access or the target of a call/new — substitute it with
    /// its base constraint before flow narrowing. This lets a
    /// `T extends X | undefined` reference be seen as possibly-undefined at the
    /// access site (TS18048), while flow narrowing still removes `undefined`
    /// after a guard such as `if (ref === undefined) return;`.
    ///
    /// The element-access `obj[key]` case keeps its deferred `T[K]` form (no
    /// substitution) when the object is a generic type *without* a nullable
    /// constraint and the index is a generic index type, matching tsc's
    /// `isConstraintPosition` exception.
    pub(crate) fn narrowable_type_for_reference(
        &mut self,
        idx: NodeIndex,
        type_id: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::type_parameter_identity as tpi;

        // Assignment targets keep their declared type: tsc returns before the
        // narrowable substitution for write positions.
        if self.ctx.skip_flow_narrowing {
            return type_id;
        }
        let Some(position) = self.constraint_position_for_reference(idx) else {
            return type_id;
        };
        if !tpi::is_generic_type_with_union_constraint(self.ctx.types.as_type_database(), type_id) {
            return type_id;
        }
        if let ConstraintPosition::ElementObject(argument) = position
            && tpi::is_generic_type_without_nullable_constraint(
                self.ctx.types.as_type_database(),
                type_id,
            )
        {
            let index_type = self.get_type_of_node(argument);
            if self.is_generic_index_type(index_type) {
                return type_id;
            }
        }
        tpi::substitute_reference_base_constraints(self.ctx.types.as_type_database(), type_id)
    }
}
