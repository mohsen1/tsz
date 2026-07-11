//! When a property access reads through a bare `this` receiver, the flow type
//! of `this` may lack a direct object shape (e.g. it resolved to a bare type
//! parameter or an index-access remnant). tsc falls back to the enclosing
//! class's instance/constructor type in that case so member lookup still
//! succeeds. This module owns that fallback, extracted from
//! `get_type_of_property_access_inner` to keep `resolve.rs` cohesive.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Override `object_type`/`display_object_type` with the enclosing class's
    /// `this` type when the receiver is a bare `this` whose current type has no
    /// object shape. Returns the (possibly overridden) pair
    /// `(object_type, display_object_type)`.
    pub(crate) fn apply_this_receiver_class_type_override(
        &mut self,
        idx: NodeIndex,
        access_expression: NodeIndex,
        object_type: TypeId,
        original_object_type: TypeId,
        display_object_type: TypeId,
    ) -> (TypeId, TypeId) {
        // When `this` has been deliberately typed as `any` (e.g. TS2683 was
        // emitted because the `this` expression is in a nested regular
        // function without its own `this` binding), don't override back to
        // the enclosing class type — property access on `any` must succeed
        // without a TS2339 cascade.
        let this_has_own_fresh_binding = self
            .ctx
            .arena
            .get(access_expression)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
            && self.is_this_in_nested_function_without_own_this_binding(access_expression);
        if self
            .ctx
            .arena
            .get(access_expression)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
            && !this_has_own_fresh_binding
            && object_type != TypeId::ANY
            && let Some(class_info) = self.ctx.enclosing_class.as_ref()
            && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
                .is_none()
        {
            // In static context, `this` refers to the constructor type (typeof ClassName).
            // In instance context, `this` refers to the instance type (ClassName).
            let is_static_context = self.is_in_static_class_member_context(idx);
            let class_this_type = if is_static_context {
                // Get the constructor type for static context
                let class_idx = class_info.class_idx;
                self.ctx
                    .arena
                    .get(class_idx)
                    .and_then(|node| self.ctx.arena.get_class(node))
                    .map(|class| self.get_class_constructor_type(class_idx, class))
            } else {
                // Use cached instance type for instance context
                class_info.cached_instance_this_type
            };
            if let Some(class_this_type) = class_this_type
                && crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    class_this_type,
                )
                .is_some()
            {
                // When `this` has been narrowed by flow analysis (e.g., via a
                // `this is DatafulFoo<T>` type predicate), the narrowed type is
                // an intersection that lacks a direct object shape. Do NOT
                // override it with the class instance type — that would discard
                // the narrowing and cause false TS2532/TS2339 diagnostics on
                // properties that differ between the original class and the
                // predicate target interface.
                let was_narrowed_by_flow =
                    object_type != class_this_type && original_object_type != class_this_type;
                if !was_narrowed_by_flow {
                    return (class_this_type, class_this_type);
                }
            }
        }
        (object_type, display_object_type)
    }
}
