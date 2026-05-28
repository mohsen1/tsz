//! Dispatcher for `this` keyword type computation.
//!
//! Extracted from `dispatch.rs` to keep that file under the §19 hard limit of 2000 lines.
//! All `this`-keyword diagnostic checks and type resolution live here.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

use super::ExpressionDispatcher;

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    /// Compute the type of a `this` keyword expression.
    ///
    /// Handles TS2465, TS2331, TS2332, TS2683, TS2816, TS7041, TS17009 and all
    /// the contextual `this`-type resolution paths (class member, JSDoc `@this`,
    /// current-this-type stack, enclosing class, etc.).
    pub(super) fn dispatch_this_keyword(&mut self, idx: NodeIndex) -> TypeId {
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            // TS2465: 'this' cannot be referenced in a computed property name.
            // Check this first — it takes priority over other `this` errors.
            if self
                .checker
                .is_this_in_class_member_computed_property_name(idx)
            {
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                    diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                );
                return TypeId::ANY;
            }
            // TS2332: `this` inside enum member initializers is always invalid,
            // even when the enum is nested in a namespace.
            if self.checker.is_this_in_enum_member_initializer(idx) {
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION,
                    diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION,
                );
                // tsc emits the companion TS2683 when `this` is directly in
                // the enum initializer. When `this` is captured through an
                // arrow function, TS2683 is NOT emitted (the arrow's `this`
                // captures the outer context which is the enum — still invalid
                // via TS2332, but not flagged for implicit-any).
                if !self.checker.has_enclosing_arrow_before_enum(idx) {
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                    );
                }
                return TypeId::ANY;
            }
            // TS2331: 'this' cannot be referenced in a module or namespace body
            // In JS files, `namespace` is invalid syntax (TS8006) so tsc
            // doesn't emit TS2331/TS2683 for `this` in namespace bodies.
            if !self.checker.is_js_file() && self.checker.is_this_in_namespace_body(idx) {
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                    diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                );
                // TSC always emits TS2683 as a companion to TS2331 in
                // namespace bodies — `this` is inherently untyped here,
                // regardless of noImplicitThis.
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                    diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                );
                return TypeId::ANY;
            }
            // TS17009: 'super' must be called before accessing 'this'
            if self
                .checker
                .is_this_before_super_in_derived_constructor(idx)
            {
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
                    diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
                );
            }
            // TS2816: Cannot use 'this' in a static property initializer of a decorated class
            if self.checker.ctx.compiler_options.experimental_decorators
                && let Some(ref class_info) = self.checker.ctx.enclosing_class
                && class_info.in_static_property_initializer
                && !self.checker.is_this_in_nested_function_inside_class(idx)
                && let Some(class_node) = self.checker.ctx.arena.get(class_info.class_idx)
                && let Some(class_data) = self.checker.ctx.arena.get_class(class_node)
                && let Some(ref modifiers) = class_data.modifiers
            {
                let has_class_decorator =
                    modifiers.nodes.iter().any(|&mod_idx| {
                        self.checker.ctx.arena.get(mod_idx).is_some_and(|n| {
                            n.kind == tsz_parser::parser::syntax_kind_ext::DECORATOR
                        })
                    });
                if has_class_decorator {
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS,
                        diagnostic_codes::CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS,
                    );
                }
            }
        }
        let has_intermediate_function = self
            .checker
            .is_this_in_nested_function_without_own_this_binding(idx);
        let contextual_owner = self.checker.this_has_contextual_owner(idx);
        let contextual_owner_is_class = contextual_owner
            .and_then(|owner_idx| self.checker.ctx.arena.get(owner_idx))
            .is_some_and(|owner_node| {
                owner_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || owner_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            });
        if !has_intermediate_function
            && let Some(member_idx) = self.checker.find_enclosing_function(idx)
            && let Some(member_node) = self.checker.ctx.arena.get(member_idx)
            && (member_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
                || member_node.kind == syntax_kind_ext::CONSTRUCTOR)
            && let Some(class_member_this_type) = self.checker.class_member_this_type(member_idx)
        {
            return self
                .checker
                .apply_flow_narrowing(idx, class_member_this_type);
        }
        if self.checker.is_js_file()
            && let Some(func_idx) = self.checker.find_enclosing_non_arrow_function(idx)
            && let Some(jsdoc) = self.checker.get_jsdoc_for_function(func_idx)
            && let Some(this_expr) = CheckerState::extract_jsdoc_tag_type_expression(&jsdoc, "this")
            && let Some(this_type) = self.checker.resolve_jsdoc_reference(this_expr)
        {
            return self.checker.apply_flow_narrowing(idx, this_type);
        }
        if let Some(this_type) = self.checker.current_this_type() {
            let transient_this_marker = crate::query_boundaries::property_access::is_this_type(
                self.checker.ctx.types,
                this_type,
            )
                || crate::query_boundaries::common::contains_type_parameters(
                    self.checker.ctx.types,
                    this_type,
                );
            // A nested regular function creates its own `this` binding.
            // Ignore any outer contextual/class `this` unless the
            // function itself owns that binding.
            if !has_intermediate_function && (!contextual_owner_is_class || !transient_this_marker)
            {
                return self.checker.apply_flow_narrowing(idx, this_type);
            }
            // Fall through — either the nested function has its own `this`,
            // or the lexical owner is a class member whose `this` binding
            // should outrank transient call-context ThisType markers.
        }
        if let Some(class_idx) = self
            .checker
            .ctx
            .enclosing_class
            .as_ref()
            .map(|info| info.class_idx)
            .or_else(|| {
                (!has_intermediate_function
                    && !contextual_owner.is_some_and(|owner_idx| {
                        self.checker.ctx.arena.get(owner_idx).is_some_and(|owner| {
                            owner.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        })
                    }))
                .then(|| self.checker.nearest_enclosing_class(idx))
                .flatten()
            })
        {
            // Inside a class but no explicit this type on stack -
            // return the class instance/constructor type depending on static context.
            // BUT: if `this` is inside a nested regular function (not a class member),
            // that function creates its own `this` binding, so don't use the class type.
            // Walk the AST to determine static context — can't rely on
            // in_static_member flag since it's only set during check_class_member.
            let is_in_static = self.checker.is_in_static_class_member_context(idx);
            if !has_intermediate_function {
                if let Some(class_node) = self.checker.ctx.arena.get(class_idx)
                    && let Some(class_data) = self.checker.ctx.arena.get_class(class_node)
                {
                    let this_type = if is_in_static {
                        self.checker
                            .get_class_constructor_type(class_idx, class_data)
                    } else {
                        self.checker.get_class_instance_type(class_idx, class_data)
                    };
                    return self.checker.apply_flow_narrowing(idx, this_type);
                }
                TypeId::ANY
            } else {
                // Fall through to TS2683 / TS7041 checks below
                // Suppress if the nested function has an explicit `this` parameter
                // or a contextual `this` type from a parent type annotation
                if self.checker.ctx.no_implicit_this()
                    && !self
                        .checker
                        .enclosing_function_has_explicit_this_parameter(idx)
                    && !self
                        .checker
                        .enclosing_function_has_contextual_this_type(idx)
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                    );
                }
                TypeId::ANY
            }
        } else if self.checker.this_has_contextual_owner(idx).is_some()
            && !self
                .checker
                .is_this_in_nested_function_without_own_this_binding(idx)
        {
            self.checker.ctx.recover_any(
                idx,
                crate::recovery::RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember,
            )
        } else if self.checker.ctx.no_implicit_this()
            && !self.checker.is_js_file()
            && !self.checker.ctx.binder.is_external_module()
            && self.checker.is_this_in_global_capturing_arrow(idx)
        {
            // TS7041: `this` in an arrow chain with no enclosing
            // function/class/object `this` binder captures globalThis.
            // Prefer this over the generic TS2683 path.
            //
            // Only fires in *script* files. At the top level of an
            // external module (has `import`/`export`), `this` is
            // `undefined`, not `globalThis`, so a capturing arrow at
            // module top-level produces TS2532 on property access,
            // not a global-capture warning. Matches tsc.
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.checker.error_at_node(
                idx,
                diagnostic_messages::THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS,
                diagnostic_codes::THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS,
            );
            TypeId::ANY
        } else if self.checker.ctx.no_implicit_this()
            && self
                .checker
                .find_enclosing_non_arrow_function(idx)
                .is_some()
            && (!self.checker.is_js_file()
                || self
                    .checker
                    .is_this_in_nested_function_without_own_this_binding(idx))
        {
            // TS2683: 'this' implicitly has type 'any'
            // In JS files, only nested regular functions with a fresh,
            // unowned `this` binding reach this path. Constructor/prototype
            // patterns and explicit/contextual/JSDoc-owned receivers are
            // filtered out before this branch.
            // Suppress if the enclosing function has an explicit `this` parameter
            // or a contextual `this` type from a parent type annotation
            if self
                .checker
                .enclosing_function_has_explicit_this_parameter(idx)
                || self
                    .checker
                    .enclosing_function_has_contextual_this_type(idx)
            {
                TypeId::ANY
            } else {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                    diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                );
                TypeId::ANY
            }
        } else if self.checker.ctx.no_implicit_this()
            && !self.checker.is_js_file()
            && self
                .checker
                .find_enclosing_non_arrow_function(idx)
                .is_none()
        {
            // `this` at the top level of a script or module with noImplicitThis.
            //
            // In an external module (has `import`/`export`), top-level `this`
            // — including `this` inside a top-level arrow — is `undefined`.
            // Property access on `this` then produces TS2532 ("Object is
            // possibly 'undefined'.") under strictNullChecks, matching tsc.
            //
            // In a script, tsc resolves `this` to `typeof globalThis` (an
            // object type). We approximate with TypeId::OBJECT since we
            // don't have a full globalThis type yet; this ensures that
            // operations like `++this` correctly emit TS2356 (arithmetic
            // type error) instead of TS2357 (invalid lvalue) — matching
            // tsc behavior where the type check fires first and suppresses
            // the lvalue check.
            if self.checker.ctx.binder.is_external_module() {
                TypeId::UNDEFINED
            } else {
                TypeId::OBJECT
            }
        } else {
            TypeId::ANY
        }
    }
}
