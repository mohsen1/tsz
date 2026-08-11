//! Class decorator checks: decorator global-type availability (`TS2318`),
//! accessor-pair decorator rules, ES decorator call-signature/arity checks, and
//! the decorator grammar check.
//!
//! Extracted from `class.rs` to keep that module under the 2000-LOC
//! architecture cap. The helpers called from the class-declaration path in
//! `class.rs` (`first_decorator_in_modifiers`,
//! `check_decorators_on_accessor_pairs`, `check_class_decorator_signature`)
//! are `pub(super)` so those sibling-module callers keep reaching them; their
//! visibility was widened only to permit the file split.

use crate::query_boundaries::checkers::decorators as decorator_query;
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

    /// TS1238: check that a class decorator resolves as a call
    /// `decorator(value[, context])`, emitting "Unable to resolve signature of
    /// class decorator when called as an expression." with tsc's full
    /// elaboration chain on failure.
    ///
    /// This is the single owner for both decorator modes:
    ///
    /// - **`--experimentalDecorators` (legacy):** the decorator is invoked
    ///   `decorator(classConstructor)`, and the resolved return type is
    ///   additionally checked against `void | typeof C` (TS1270).
    /// - **ES (TC39 stage-3):** the decorator is invoked
    ///   `decorator(value, context)` where `value: typeof C` and
    ///   `context: ClassDecoratorContext<typeof C>`, truncated to the
    ///   decorator's own declared arity (tsc's `getDecoratorArgumentCount`).
    ///
    /// A bare-reference zero-argument factory used uncalled (`@d`, `@ns.d`)
    /// draws the TS1329 "did you mean to call it first" hint; a
    /// parenthesized/inline factory (`@(() => {})`) instead falls through to
    /// the arity failure below, matching tsc's `isPotentiallyUncalledDecorator`.
    /// A callee with no call signatures at all (a non-function value, or a
    /// class with only construct signatures) reports the not-callable chain
    /// directly, exactly as tsc's `resolveDecorator` does before it ever
    /// reaches `resolveCall`.
    pub(super) fn check_class_decorator_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_expression: NodeIndex,
        decorator_type: TypeId,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        legacy: bool,
    ) {
        use crate::query_boundaries::common::{CallResult, call_signatures_for_type};

        // Skip validation for error/any/unknown — these won't produce
        // meaningful diagnostics.
        if matches!(
            decorator_type,
            TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN
        ) {
            return;
        }

        // Resolve Lazy(DefId) references and evaluate applications so that
        // type queries can see the underlying type shape.
        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if matches!(resolved, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return;
        }

        // Mirror tsc's `isUntypedFunctionCall`: a decorator typed as the
        // global `Function` interface has no explicit call signatures but is
        // treated as callable. Without this fallback, a class decorator
        // factory returning `Function` would produce a spurious TS1238.
        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        // TS1329: a bare-reference zero-argument factory used without its
        // call. Parenthesized/inline factories are excluded and fall through
        // to the arity/signature failure below.
        if self.decorator_expression_is_reference(decorator_expression)
            && self.decorator_has_zero_arg_factory_shape(
                decorator_expression,
                resolved,
                decorator_node,
            )
        {
            return;
        }

        // No call signatures at all (a non-function value, or a class used as
        // a decorator — construct signatures but no call signatures). tsc
        // reports the not-callable chain directly from `resolveDecorator`,
        // before it ever reaches `resolveCall`, and anchors it at the
        // decorator expression.
        let has_call_signatures = class_query::has_function_shape(self.ctx.types, resolved)
            || call_signatures_for_type(self.ctx.types, resolved)
                .is_some_and(|sigs| !sigs.is_empty());
        if !has_call_signatures {
            self.emit_class_decorator_not_callable(
                self.decorator_expression_anchor(decorator_node),
                resolved,
            );
            return;
        }

        let class_constructor_type = self.get_class_constructor_type(class_idx, class);
        if class_constructor_type == TypeId::ERROR {
            return;
        }

        let args = self.class_decorator_argument_types(resolved, class_constructor_type, legacy);
        let (result, _, _) =
            self.resolve_call_with_checker_adapter(resolved, &args, false, None, None);

        let CallResult::Success(return_type) = &result else {
            self.emit_class_decorator_signature_error(decorator_node, &result, resolved);
            return;
        };

        if legacy {
            self.check_legacy_class_decorator_return_type(
                decorator_node,
                class_constructor_type,
                *return_type,
            );
        }
    }

    /// The runtime argument list a class decorator is invoked with:
    /// `[classConstructor]` under `--experimentalDecorators`, or the ES
    /// `[value, ClassDecoratorContext<typeof C>]` truncated to the decorator's
    /// own declared arity (tsc's `getDecoratorArgumentCount`, so a
    /// single-parameter ES decorator receives only `value`).
    fn class_decorator_argument_types(
        &mut self,
        resolved: TypeId,
        class_constructor_type: TypeId,
        legacy: bool,
    ) -> Vec<TypeId> {
        if legacy {
            return vec![class_constructor_type];
        }
        // A single-parameter ES decorator receives only `value`, so skip the
        // `ClassDecoratorContext<typeof C>` lib lookup entirely when the
        // context argument would be truncated away.
        if self.es_member_decorator_argument_count(resolved) <= 1 {
            return vec![class_constructor_type];
        }
        let context = self
            .resolve_decorator_context_type("ClassDecoratorContext", vec![class_constructor_type])
            .unwrap_or(TypeId::OBJECT);
        vec![class_constructor_type, context]
    }

    /// TS1270 for legacy class decorators: the resolved return type must be
    /// assignable to `void | typeof C`. `any`/`unknown`/error short-circuit.
    fn check_legacy_class_decorator_return_type(
        &mut self,
        decorator_node: NodeIndex,
        class_constructor_type: TypeId,
        return_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let return_type = self.evaluate_type_for_assignability(return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return;
        }

        // The legacy class decorator may return `void` or a replacement
        // constructor (`typeof C`); reuse the shared `void | replacement`
        // builder rather than open-coding the union.
        let expected_return = decorator_query::decorator_void_or_replacement_type(
            self.ctx.types,
            class_constructor_type,
        );
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
            // The call itself resolved successfully (only the return type is
            // wrong), so this is never a "too few arguments" shape; tsc
            // anchors at the decorator expression, not the whole `@decorator`
            // span (oracle-verified, see
            // `ts1238_experimental_decorator_anchor_tests`).
            self.error_at_node(
                self.decorator_expression_anchor(decorator_node),
                &message,
                diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
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
