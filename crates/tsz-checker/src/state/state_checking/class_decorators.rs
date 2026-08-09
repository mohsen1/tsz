//! Class decorator checks: decorator global-type availability (`TS2318`),
//! accessor-pair decorator rules, ES decorator call-signature/arity checks, and
//! the decorator grammar check.
//!
//! Extracted verbatim from `class.rs` to keep that module under the 2000-LOC
//! architecture cap; behavior is unchanged. The four helpers called from the
//! class-declaration/expression paths in `class.rs`
//! (`first_decorator_in_modifiers`, `check_decorators_on_accessor_pairs`,
//! `check_class_decorator_call_signature`, `check_es_class_decorator_arity`)
//! are `pub(super)` so those sibling-module callers keep reaching them; their
//! visibility was widened only to permit the file split.

use crate::query_boundaries::class_type as class_query;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check for decorator-related global types (TS2318).
    ///
    /// When experimentalDecorators is enabled and a method or accessor has decorators,
    /// TypeScript requires the `TypedPropertyDescriptor` type to be available.
    /// If it's not available (e.g., with noLib), we emit TS2318.
    pub(crate) fn check_decorator_global_types(&mut self, members: &[NodeIndex]) {
        // Only check if experimentalDecorators is enabled
        if !self.ctx.compiler_options.experimental_decorators {
            return;
        }

        // Check if any method or accessor has decorators
        let mut has_method_or_accessor_decorator = false;
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            let modifiers = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .and_then(|m| m.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .and_then(|a| a.modifiers.as_ref())
                }
                _ => continue,
            };

            if let Some(mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                        && mod_node.kind == syntax_kind_ext::DECORATOR
                    {
                        has_method_or_accessor_decorator = true;
                        break;
                    }
                }
            }
            if has_method_or_accessor_decorator {
                break;
            }
        }

        if !has_method_or_accessor_decorator {
            return;
        }

        // Check if TypedPropertyDescriptor is available
        let type_name = "TypedPropertyDescriptor";
        if self.ctx.has_name_in_lib(type_name) {
            return; // Type is available from lib
        }
        if self.ctx.binder.file_locals.has(type_name) {
            return; // Type is declared locally
        }

        // TypedPropertyDescriptor is not available - emit TS2318
        // TSC emits this error twice for method decorators
        let file_name = self.ctx.file_name.clone();
        self.error_global_type_missing_at_position(type_name, file_name.clone(), 0, 0);
        self.error_global_type_missing_at_position(type_name, file_name, 0, 0);
    }

    pub(super) fn check_decorators_on_accessor_pairs(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if !self.ctx.compiler_options.experimental_decorators {
            return;
        }

        let mut decorated_accessors: FxHashMap<(bool, String), bool> = FxHashMap::default();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::GET_ACCESSOR
                && node.kind != syntax_kind_ext::SET_ACCESSOR
            {
                continue;
            }

            let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                continue;
            };
            let Some(decorator_idx) = self.first_decorator_in_modifiers(&accessor.modifiers) else {
                continue;
            };
            let Some(property_name) = self.get_property_name_resolved(accessor.name) else {
                continue;
            };

            let is_static = self
                .ctx
                .arena
                .has_modifier(&accessor.modifiers, tsz_scanner::SyntaxKind::StaticKeyword);
            let is_get = node.kind == syntax_kind_ext::GET_ACCESSOR;
            let key = (is_static, property_name);
            if let Some(seen_is_get) = decorated_accessors.get(&key) {
                if *seen_is_get != is_get {
                    self.error_at_node(
                        decorator_idx,
                        diagnostic_messages::DECORATORS_CANNOT_BE_APPLIED_TO_MULTIPLE_GET_SET_ACCESSORS_OF_THE_SAME_NAME,
                        diagnostic_codes::DECORATORS_CANNOT_BE_APPLIED_TO_MULTIPLE_GET_SET_ACCESSORS_OF_THE_SAME_NAME,
                    );
                }
                continue;
            }

            decorated_accessors.insert(key, is_get);
        }
    }

    /// TS18036: report on `first_decorator_idx` when any of `members` is a static
    /// `PropertyDeclaration`/`MethodDeclaration`/get-or-set-accessor named with a
    /// private identifier — mirrors tsc's
    /// `some(node.members, p => hasStaticModifier(p) && isPrivateIdentifierClassElementDeclaration(p))`.
    /// Caller has already gated on `experimental_decorators` and a present first decorator.
    pub(super) fn check_class_decorator_static_private_identifier(
        &mut self,
        first_decorator_idx: NodeIndex,
        members: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let has_static_private_member = members.iter().any(|&member_idx| {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    self.ctx.arena.get_property_decl(node).is_some_and(|decl| {
                        self.has_static_modifier(&decl.modifiers)
                            && self.is_private_identifier_name(decl.name)
                    })
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.ctx.arena.get_method_decl(node).is_some_and(|decl| {
                        self.has_static_modifier(&decl.modifiers)
                            && self.is_private_identifier_name(decl.name)
                    })
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx.arena.get_accessor(node).is_some_and(|decl| {
                        self.has_static_modifier(&decl.modifiers)
                            && self.is_private_identifier_name(decl.name)
                    })
                }
                _ => false,
            }
        });

        if has_static_private_member {
            self.error_at_node(
                first_decorator_idx,
                diagnostic_messages::CLASS_DECORATORS_CANT_BE_USED_WITH_STATIC_PRIVATE_IDENTIFIER_CONSIDER_REMOVING_T,
                diagnostic_codes::CLASS_DECORATORS_CANT_BE_USED_WITH_STATIC_PRIVATE_IDENTIFIER_CONSIDER_REMOVING_T,
            );
        }
    }

    pub(super) fn first_decorator_in_modifiers(
        &self,
        modifiers: &Option<NodeList>,
    ) -> Option<NodeIndex> {
        let modifiers = modifiers.as_ref()?;
        modifiers.nodes.iter().copied().find(|&modifier_idx| {
            self.ctx
                .arena
                .get(modifier_idx)
                .is_some_and(|modifier| modifier.kind == syntax_kind_ext::DECORATOR)
        })
    }

    /// TS1238: Check that a class decorator expression has a compatible call signature.
    ///
    /// For experimental decorators, the decorator is called as `decoratorExpr(classConstructor)`.
    /// If the decorator type has no call signatures, or if call resolution against the class
    /// constructor type fails, emit TS1238.
    pub(super) fn check_class_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use crate::query_boundaries::common::call_signatures_for_type;

        // Skip validation for error types or any — these won't produce meaningful diagnostics
        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        // Resolve Lazy(DefId) references and evaluate applications so that
        // type queries can see the underlying type shape.
        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);

        // After evaluation, any/unknown/error → skip
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        // Mirror tsc's `isUntypedFunctionCall`: a decorator typed as the
        // global `Function` interface has no explicit call signatures but is
        // treated as callable. Without this fallback, a class decorator
        // factory returning `Function` would produce a spurious TS1238.
        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        // Check if the decorator type is callable.
        // TypeData::Function has a single call signature (function declarations/expressions).
        // TypeData::Callable has overloaded call/construct signatures (interfaces).
        let has_call_signatures = class_query::has_function_shape(self.ctx.types, resolved)
            || call_signatures_for_type(self.ctx.types, resolved)
                .is_some_and(|sigs| !sigs.is_empty());

        if !has_call_signatures {
            // No call signatures at all (e.g., a class used as decorator — has construct
            // signatures but no call signatures, or a bare primitive). tsc attaches the
            // "This expression is not callable." / "Type 'X' has no call signatures."
            // chain beneath TS1238 (oracle-verified, typescript@7.0.2).
            let anchor = self.decorator_failure_anchor(decorator_node, resolved, 1);
            self.emit_class_decorator_not_callable(anchor, resolved);
            return;
        }

        // Has call signatures — try to resolve the call with the class constructor type.
        // resolve_call handles both Function and Callable types internally.
        // If resolution fails (type mismatch, arity error), emit TS1238.
        let class_constructor_type = self.get_class_constructor_type(class_idx, class);
        if class_constructor_type == TypeId::ERROR {
            return;
        }

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[class_constructor_type],
            false,
            None,
            None,
        );

        let crate::query_boundaries::common::CallResult::Success(return_type) = &result else {
            let anchor = self.decorator_failure_anchor(decorator_node, resolved, 1);
            self.emit_decorator_signature_error(
                anchor,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                &result,
            );
            return;
        };

        let return_type = self.evaluate_type_for_assignability(*return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return;
        }

        let expected_return = self
            .ctx
            .types
            .factory()
            .union2(TypeId::VOID, class_constructor_type);
        if !self
            .return_relation_outcome(return_type, expected_return)
            .related
        {
            let return_str = self.format_type_diagnostic(return_type);
            let expected_str = self.format_type_diagnostic(expected_return);
            let message = format_message(
                diagnostic_messages::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&return_str, &expected_str],
            );
            let anchor = self.decorator_failure_anchor(decorator_node, resolved, 1);
            self.error_at_node(
                anchor,
                &message,
                diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
    }

    /// TS1238 for ES decorators: check that a class decorator doesn't require
    /// more than 2 parameters.
    ///
    /// ES decorators receive `(value, context)` — at most 2 arguments.
    /// If the decorator function's call signature has more than 2 required
    /// parameters, the call will fail. Emit TS1238.
    pub(super) fn check_es_class_decorator_arity(
        &mut self,
        decorator_node: NodeIndex,
        decorator_expression: NodeIndex,
        decorator_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        // Mirror tsc's `isUntypedFunctionCall`: a decorator typed as the
        // global `Function` interface has no explicit call signatures but is
        // still callable (see `check_class_decorator_call_signature`, whose
        // legacy-mode counterpart of this same check applies the identical
        // fallback).
        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        // No call signature at all (bare primitive, a class used as a
        // decorator, ...): tsc's TS1238 "not callable" chain fires
        // identically here as it does under `--experimentalDecorators`
        // (oracle-verified, typescript@7.0.2). Previously this whole
        // function only inspected `function_shape`, so any decorator type
        // without a single-signature function shape (a primitive, a class,
        // an overloaded callable) silently passed arity validation with no
        // diagnostic at all.
        let has_call_signatures =
            crate::query_boundaries::class_type::has_function_shape(self.ctx.types, resolved)
                || crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    resolved,
                )
                .is_some_and(|sigs| !sigs.is_empty());
        if !has_call_signatures {
            self.emit_class_decorator_not_callable(decorator_expression, resolved);
            return;
        }

        // Check the function shape for required parameter count
        if let Some(shape) =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, resolved)
        {
            let required_params = shape
                .params
                .iter()
                .filter(|p| !p.optional && !p.rest)
                .count();
            // ES decorators are invoked with `(value, context)`.
            //
            // * When the factory has no parameters at all, the runtime call
            //   `f(value, context)` passes extra args; tsc anchors the error
            //   at the decorator expression (excluding `@`).
            // * When the factory requires more than two parameters, the call
            //   cannot supply them; tsc anchors the error at the whole
            //   decorator (including `@`).
            if shape.params.is_empty() {
                self.error_at_node(
                    decorator_expression,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                );
            } else if required_params > 2 {
                self.error_at_node(
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                );
            }
        }
    }

    /// TS1497: Check that a decorator expression follows the valid grammar.
    ///
    /// Valid decorator expressions are:
    /// - `@identifier`
    /// - `@identifier.name.name`  (property access chain)
    /// - `@identifier.name()`     (single call at the top)
    /// - `@(expression)`          (parenthesized)
    ///
    /// Invalid (TS1497) examples: `@x().y`, `@new x`, `@x?.y`, @x\`\`,
    /// `@x?.()`, `@x?.["y"]`, `@x["y"]`.
    ///
    /// Matches tsc's `checkGrammarDecorator`. Only checked when the source file
    /// has no parse diagnostics.
    pub(crate) fn check_grammar_decorator(&mut self, expression_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip if the source file has parse diagnostics (matches tsc's hasParseDiagnostics gate)
        if self.ctx.has_parse_errors {
            return;
        }

        let Some(expr_node) = self.ctx.arena.get(expression_idx) else {
            return;
        };

        // DecoratorParenthesizedExpression: ( Expression )
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return;
        }

        let mut current = expression_idx;
        let mut can_have_call = true;
        let mut error_node: Option<NodeIndex> = None;

        while let Some(node) = self.ctx.arena.get(current) {
            // Allow TS syntax: ExpressionWithTypeArguments, NonNullExpression
            if node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
                if let Some(data) = self.ctx.arena.get_expr_type_args(node) {
                    current = data.expression;
                    continue;
                }
                break;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
                if let Some(data) = self.ctx.arena.get_unary_expr_ex(node) {
                    current = data.expression;
                    continue;
                }
                break;
            }

            // DecoratorCallExpression: DecoratorMemberExpression Arguments
            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                if !can_have_call {
                    error_node = Some(current);
                }
                // Check for optional chaining on call: x?.()
                if node.is_optional_chain() {
                    // Optional chaining — always an error, even if we already have one
                    error_node = Some(current);
                }
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    current = call.expression;
                    can_have_call = false;
                    continue;
                }
                break;
            }

            // DecoratorMemberExpression: DecoratorMemberExpression . IdentifierName
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    if access.question_dot_token {
                        // Optional chaining — always error
                        error_node = Some(current);
                    }
                    current = access.expression;
                    can_have_call = false;
                    continue;
                }
                break;
            }

            // If it's not an identifier, it's invalid
            if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                error_node = Some(current);
            }

            break;
        }

        if error_node.is_some() {
            self.error_at_node(
                expression_idx,
                diagnostic_messages::EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR,
                diagnostic_codes::EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR,
            );
        }
    }
}
