//! Object-destructuring "property not found" handling, including the
//! require-destructure named-import-style check: a name destructured
//! directly from a `require(...)` call is checked like an ES named import,
//! not like a generic property access — see the module doc on
//! `commonjs_require_destructure_missing_member_ts2305_tests`.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;
use crate::query_boundaries::binding_patterns;

impl<'a> CheckerState<'a> {
    /// Computes the type of an object-destructuring binding element whose
    /// property lookup on `parent_type` failed. Handles the union-of-empty-
    /// members leniency, the require-destructure `TS2305` check, computed
    /// unique-symbol keys (`TS2538`), and the fallback `TS2339`.
    pub(super) fn missing_binding_property_type(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
        prop_name_str: &str,
        computed_expr: Option<NodeIndex>,
        request: &TypingRequest,
        should_report_missing_property: bool,
    ) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        // tsc's getTypeOfDestructuredProperty uses mapType for unions where
        // all non-empty members have the property. When a union contains
        // empty object members (`{}`), those members naturally lack every
        // property. In tsc, an empty object member contributes `undefined`
        // for any property instead of failing the entire lookup. This
        // commonly arises from `x ?? {}` patterns where the right-hand `{}`
        // produces an empty member in the union.
        //
        // We only apply this per-member resolution when EVERY member that
        // lacks the property is an empty object. If a non-empty member is
        // missing the property, the standard TS2339 error should still fire.
        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
            let mut member_types = Vec::new();
            let mut any_found = false;
            let mut non_empty_missing = false;
            for &member in &members {
                let member_result = self.resolve_property_access_with_env(member, prop_name_str);
                match member_result {
                    PropertyAccessResult::Success { type_id, .. } => {
                        member_types.push(type_id);
                        any_found = true;
                    }
                    PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                        member_types.push(property_type.unwrap_or(TypeId::UNDEFINED));
                        any_found = true;
                    }
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        // Empty `{}` or fresh object-literal members lacking the
                        // property contribute implicit `undefined` (tsc
                        // getTypeOfDestructuredProperty); named, call-return, and
                        // freshness-widened const-bound members lack FRESH and error.
                        use crate::query_boundaries::common;
                        let db = self.ctx.types.as_type_database();
                        if common::is_empty_object_type(db, member)
                            || common::is_fresh_object_type(db, member)
                        {
                            member_types.push(TypeId::UNDEFINED);
                        } else {
                            non_empty_missing = true;
                            break;
                        }
                    }
                    PropertyAccessResult::IsUnknown => {
                        member_types.push(TypeId::UNDEFINED);
                    }
                }
            }
            if any_found && !non_empty_missing {
                return binding_patterns::binding_pattern_member_union_type(
                    self.ctx.types,
                    member_types,
                );
            }
        }

        let error_node = if element_data.property_name.is_some() {
            element_data.property_name
        } else if element_data.name.is_some() {
            element_data.name
        } else {
            NodeIndex::NONE
        };
        if should_report_missing_property
            && computed_expr.is_none()
            && let Some(result) = self.try_report_require_destructure_missing_member(
                pattern_idx,
                element_data,
                prop_name_str,
                error_node,
            )
        {
            return result;
        }
        if should_report_missing_property {
            // When the computed key is a unique symbol that doesn't exist on
            // the parent type, emit TS2538 ("Type 'X' cannot be used as an
            // index type") instead of TS2339 ("Property does not exist").
            // tsc treats unique symbol keys that don't match a declared
            // property as index-type errors, not property-not-found errors.
            let emitted_ts2538 = if let Some(ce) = computed_expr {
                let key_type = self.get_binding_element_computed_key_type_with_request(
                    pattern_idx,
                    ce,
                    request,
                );
                if common_query::is_symbol_or_unique_symbol(self.ctx.types, key_type) {
                    let key_type_str = self.format_type(key_type);
                    let message = crate::diagnostics::format_message(
                        crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                        &[&key_type_str],
                    );
                    self.error_at_node(
                        ce,
                        &message,
                        crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !emitted_ts2538 {
                // In tsc, destructuring uses the *apparent* type in the error
                // message: `object` → `{}`, and primitives widen to their
                // wrapper class (`string` → `String`, `number` → `Number`,
                // etc.). Match that so binding patterns like `var { a } = "s"`
                // report `type 'String'` rather than the raw `type 'string'`.
                let apparent_type_display = apparent_type_display_for_destructuring(parent_type);
                if let Some(ce) = computed_expr {
                    let type_str = apparent_type_display
                        .clone()
                        .unwrap_or_else(|| self.format_type_for_assignability_message(parent_type));
                    let message =
                        format!("Property '{prop_name_str}' does not exist on type '{type_str}'.");
                    self.error_at_node(
                        ce,
                        &message,
                        crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                } else if let Some(type_str) = apparent_type_display {
                    self.error_property_not_exist_with_apparent_type(
                        prop_name_str,
                        &type_str,
                        error_node,
                    );
                } else {
                    self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                }
            }
        }
        TypeId::ANY
    }

    /// Returns the module specifier when `pattern_idx`'s enclosing variable
    /// declaration destructures directly from a `require(...)` call, e.g.
    /// `const { funky } = require('./mod')`. A nested pattern (whose
    /// immediate parent is a `BindingElement`, not this `VariableDeclaration`)
    /// does not qualify.
    fn require_call_module_specifier_for_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(pattern_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let initializer = self
            .ctx
            .arena
            .get_variable_declaration(parent_node)?
            .initializer;
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let callee = self.ctx.arena.get_call_expr(init_node)?.expression;
        if !self.is_unshadowed_commonjs_require_identifier(callee) {
            return None;
        }
        self.get_require_module_specifier(initializer)
    }

    /// When `element_data` is a flat binding element (`{ name }` /
    /// `{ name: alias }`, not a nested pattern) destructured directly from a
    /// `require(...)` call, and `prop_name` is missing from that module's
    /// export surface, reports `TS2305` and returns the error type —
    /// matching tsc's named-import-style check for this call shape. Returns
    /// `None` when the shape doesn't qualify, so the caller falls back to
    /// ordinary property-access diagnostics.
    pub(super) fn try_report_require_destructure_missing_member(
        &mut self,
        pattern_idx: NodeIndex,
        element_data: &tsz_parser::parser::node::BindingElementData,
        prop_name: &str,
        error_node: NodeIndex,
    ) -> Option<TypeId> {
        // tsc's require-destructure check only fires for a flat binding
        // (`{ funky }` / `{ funky: f }`) — the same shape a real named
        // import specifier allows. `{ funky: { nested } }` destructures the
        // value further and falls back to ordinary property access.
        if self
            .ctx
            .arena
            .get(element_data.name)
            .is_none_or(|n| n.kind != SyntaxKind::Identifier as u16)
        {
            return None;
        }
        let module_specifier =
            self.require_call_module_specifier_for_binding_pattern(pattern_idx)?;

        // tsc checks a name destructured directly from a `require(...)` call
        // like an ES named import: a missing binding is "module has no
        // exported member" (TS2305), not a generic property-access failure,
        // and resolves to the error type so it does not cascade into
        // downstream identical-type checks (e.g. TS2403 on a later `var`
        // redeclaration).
        let quoted_module = format!("\"{module_specifier}\"");
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
            &[&quoted_module, prop_name],
        );
        self.error_at_node(
            error_node,
            &message,
            crate::diagnostics::diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
        );
        Some(TypeId::ERROR)
    }
}
