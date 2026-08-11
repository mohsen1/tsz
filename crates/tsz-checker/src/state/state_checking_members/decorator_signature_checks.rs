//! Class-member decorator signature validation helpers.

use crate::diagnostics::{
    Diagnostic, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::error_reporter::DiagnosticAnchorKind;
use crate::query_boundaries::checkers::decorators as decorator_query;
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

/// Decorator-type sentinels that short-circuit signature validation. `ERROR`
/// is an unresolved type (we have already reported it elsewhere); `ANY` and
/// `UNKNOWN` are explicitly permissive and tsc does not emit a follow-on
/// TS1239/TS1240 for them.
#[inline]
const fn decorator_type_is_unchecked(t: TypeId) -> bool {
    matches!(t, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN)
}

impl<'a> CheckerState<'a> {
    /// TS1278/TS1279 elaboration for a decorator signature-resolution
    /// failure: "The runtime will invoke the decorator with {1} arguments,
    /// but the decorator expects {0}[ at least]." tsc always attaches this as
    /// a message-chain link (no location of its own) alongside the primary
    /// TS1238/1239/1240/1241 when the failure is an argument-count mismatch.
    /// Other failure shapes (type mismatch, no overload match, ...) get no
    /// elaboration here: tsc attaches a different, not-yet-wired shape there
    /// (a `TS2345`-style "not assignable" line), so this deliberately leaves
    /// those diagnostics exactly as before rather than attaching the wrong
    /// kind of related information.
    ///
    /// "At least N" (TS1279) fires only when the decorator's own declared
    /// arity is genuinely open-ended (`expected_max` is `None`, e.g. a
    /// trailing `...rest: any[]`) and too few arguments were supplied; a
    /// fixed-length rest tuple (`...rest: [string, number]`) still has a
    /// concrete `expected_max` and takes the exact-N wording (TS1278).
    ///
    /// When the failure is *too few* arguments, a second cross-location
    /// pointer is attached at the first parameter the runtime cannot supply
    /// (see [`Self::decorator_missing_argument_pointer`]).
    fn decorator_arity_related_info(
        &self,
        decorator_node: NodeIndex,
        result: &CallResult,
    ) -> Vec<DiagnosticRelatedInformation> {
        let &CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } = result
        else {
            return Vec::new();
        };
        let actual_str = actual.to_string();
        let (code, message_template, expected) = if actual < expected_min && expected_max.is_none()
        {
            (
                    diagnostic_codes::THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS_A,
                    diagnostic_messages::THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS_A,
                    expected_min,
                )
        } else {
            (
                    diagnostic_codes::THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS,
                    diagnostic_messages::THE_RUNTIME_WILL_INVOKE_THE_DECORATOR_WITH_ARGUMENTS_BUT_THE_DECORATOR_EXPECTS,
                    expected_max.unwrap_or(expected_min),
                )
        };
        let expected_str = expected.to_string();
        let mut related = vec![Diagnostic::related_message(
            code,
            self.ctx.file_name.clone(),
            0,
            0,
            format_message(message_template, &[&expected_str, &actual_str]),
        )];
        // Only a *too-few-arguments* failure attaches the "argument not
        // provided" pointer; a "too many" failure supplies every declared
        // parameter, so tsc adds no second line there.
        if actual < expected_min
            && let Some(pointer) = self.decorator_missing_argument_pointer(decorator_node, actual)
        {
            related.push(pointer);
        }
        related
    }

    /// Cross-location pointer (`tsc`'s `relatedInformation`) beneath the
    /// decorator signature-resolution error, anchored at the first declared
    /// parameter the runtime invocation cannot supply.
    ///
    /// Mirrors tsc's `getArgumentArityError`, which — after the primary
    /// TS1238/1239/1240/1241 and its TS1278/TS1279 elaboration — attaches a
    /// pointer at `parameters[argCount]` (the first position the fixed decorator
    /// calling convention leaves unfilled):
    /// - TS6210 `An argument for '{0}' was not provided.` for an ordinary
    ///   named parameter,
    /// - TS6236 `Arguments for the rest parameter '{0}' were not provided.`
    ///   when that parameter is variadic,
    /// - TS6211 `An argument matching this binding pattern was not provided.`
    ///   when it is a destructured (binding-pattern) parameter.
    ///
    /// Returns `None` when the decorator does not reference a single locatable
    /// function-like declaration (an overloaded, imported, factory-call, or
    /// property-access decorator): tsc anchors at the resolved signature's
    /// declaration there, but without a unique local one the exact anchor
    /// cannot be reproduced, so no pointer is attached rather than a wrong one.
    fn decorator_missing_argument_pointer(
        &self,
        decorator_node: NodeIndex,
        actual: usize,
    ) -> Option<DiagnosticRelatedInformation> {
        let params = self.decorator_declaration_value_parameter_nodes(decorator_node)?;
        let param_idx = *params.get(actual)?;
        let param_node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        let anchor = self.resolve_diagnostic_anchor(param_idx, DiagnosticAnchorKind::Exact)?;

        let name_node = self.ctx.arena.get(param.name);
        let name_ident = name_node.and_then(|n| self.ctx.arena.get_identifier(n));
        let (code, message) = if param.dot_dot_dot_token {
            let name = name_ident?.escaped_text.to_string();
            (
                diagnostic_codes::ARGUMENTS_FOR_THE_REST_PARAMETER_WERE_NOT_PROVIDED,
                format_message(
                    diagnostic_messages::ARGUMENTS_FOR_THE_REST_PARAMETER_WERE_NOT_PROVIDED,
                    &[&name],
                ),
            )
        } else if let Some(ident) = name_ident {
            let name = ident.escaped_text.to_string();
            (
                diagnostic_codes::AN_ARGUMENT_FOR_WAS_NOT_PROVIDED,
                format_message(
                    diagnostic_messages::AN_ARGUMENT_FOR_WAS_NOT_PROVIDED,
                    &[&name],
                ),
            )
        } else {
            // A destructured parameter has no single name to quote.
            (
                diagnostic_codes::AN_ARGUMENT_MATCHING_THIS_BINDING_PATTERN_WAS_NOT_PROVIDED,
                diagnostic_messages::AN_ARGUMENT_MATCHING_THIS_BINDING_PATTERN_WAS_NOT_PROVIDED
                    .to_string(),
            )
        };
        Some(Diagnostic::related_pointer(
            code,
            self.ctx.file_name.clone(),
            anchor.start,
            anchor.length,
            message,
        ))
    }

    /// The value-parameter declaration nodes (any leading explicit `this`
    /// parameter removed, so the list is indexed the way the runtime supplies
    /// value arguments) of the single function-like declaration a decorator
    /// expression references, or `None` when the decorator is not a bare
    /// `@name` bound to exactly one locatable function-like declaration.
    fn decorator_declaration_value_parameter_nodes(
        &self,
        decorator_node: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
        let ident = self.decorator_callee_reference_identifier(decorator_node)?;
        let sym_id = self.resolve_identifier_symbol_without_tracking(ident)?;
        let decls = self.ctx.binder.get_symbol(sym_id)?.declarations.clone();
        let mut found: Option<Vec<NodeIndex>> = None;
        for decl in decls {
            if let Some(params) = self.function_like_value_parameter_nodes(decl) {
                if found.is_some() {
                    // Overloaded / multiply-declared: the resolved signature is
                    // ambiguous from the node alone, so decline rather than guess.
                    return None;
                }
                found = Some(params);
            }
        }
        found
    }

    /// Parameter declaration nodes of a function-like declaration — a function,
    /// function expression, arrow, or method, or a variable initialized with
    /// one — with any leading explicit `this` parameter removed.
    fn function_like_value_parameter_nodes(&self, decl: NodeIndex) -> Option<Vec<NodeIndex>> {
        let node = self.ctx.arena.get(decl)?;
        let params = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                &self.ctx.arena.get_function(node)?.parameters
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                &self.ctx.arena.get_method_decl(node)?.parameters
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                let init = self.ctx.arena.get_variable_declaration(node)?.initializer;
                if init.is_none() {
                    return None;
                }
                return self.function_like_value_parameter_nodes(init);
            }
            _ => return None,
        };
        Some(
            params
                .nodes
                .iter()
                .copied()
                .filter(|&p| !self.parameter_is_this(p))
                .collect(),
        )
    }

    /// Whether a parameter declaration is an explicit `this: T` parameter,
    /// which tsc excludes from the value-argument positions. The `this` name
    /// parses as a `ThisKeyword` (not an ordinary identifier), so this defers
    /// to the shared [`Self::is_this_parameter_name`] check.
    fn parameter_is_this(&self, param_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(param_idx)
            .and_then(|n| self.ctx.arena.get_parameter(n))
            .is_some_and(|p| self.is_this_parameter_name(p.name))
    }

    /// The identifier a decorator expression directly references, for a bare
    /// `@name` (optionally parenthesized). Property-access (`@ns.dec`),
    /// factory-call (`@make()`), and other forms return `None`.
    fn decorator_callee_reference_identifier(
        &self,
        decorator_node: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decorator_node)?;
        let expr = self.ctx.arena.get_decorator(node)?.expression;
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let expr_node = self.ctx.arena.get(expr)?;
        (expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16).then_some(expr)
    }

    /// Emit `message`/`code` at `anchor`, attaching `related` as an
    /// elaboration chain only when it is non-empty. Centralizes the
    /// "diagnostic with optional related information" branch shared by every
    /// decorator signature-error emitter.
    fn error_at_node_maybe_related(
        &mut self,
        anchor: NodeIndex,
        message: &str,
        code: u32,
        related: Vec<DiagnosticRelatedInformation>,
    ) {
        if related.is_empty() {
            self.error_at_node(anchor, message, code);
        } else {
            self.error_at_node_with_related(anchor, message, code, related);
        }
    }

    /// Emit a decorator signature-resolution error, attaching
    /// [`Self::decorator_arity_related_info`] when `result` is an
    /// argument-count mismatch. `decorator_node` is the `@`-decorator syntax
    /// node, used to locate the declared parameter the missing-argument pointer
    /// anchors at.
    pub(crate) fn emit_decorator_signature_error(
        &mut self,
        anchor: NodeIndex,
        decorator_node: NodeIndex,
        message: &str,
        code: u32,
        result: &CallResult,
    ) {
        let related = self.decorator_arity_related_info(decorator_node, result);
        self.error_at_node_maybe_related(anchor, message, code, related);
    }

    /// Related-information chain for a decorator whose callee has no call
    /// signatures, mirroring tsc's `invocationErrorDetails` beneath the
    /// TS1238/TS1239/TS1240/TS1241 head:
    ///
    /// ```text
    ///   This expression is not callable.
    ///     Type 'X' has no call signatures.
    /// ```
    ///
    /// Chain-link entries carry no real position (matching
    /// [`Self::decorator_arity_related_info`]'s `(0, 0)` convention) since
    /// `RelatedInformationKind::ChainLink` renders by depth, not location.
    fn decorator_not_callable_related_info(
        &mut self,
        resolved: TypeId,
    ) -> Vec<DiagnosticRelatedInformation> {
        let mut related = vec![Diagnostic::related_message(
            diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE,
            self.ctx.file_name.clone(),
            0,
            0,
            diagnostic_messages::THIS_EXPRESSION_IS_NOT_CALLABLE,
        )];
        if let Some(mut detail) = self.invocation_signature_detail(
            resolved,
            crate::error_reporter::operator_errors::InvocationSignatureKind::Call,
            0,
            0,
        ) {
            detail.depth = 1;
            related.push(detail);
        }
        related
    }

    /// Related-information line for a decorator whose call fails because an
    /// argument type is not assignable to the corresponding parameter: tsc's
    /// `TS2345`-shaped "Argument of type '{0}' is not assignable to parameter
    /// of type '{1}'." chained beneath the primary TS1238/… head. Returns
    /// empty for every other [`CallResult`] shape so the arity and
    /// not-callable paths keep ownership of theirs.
    fn decorator_argument_mismatch_related_info(
        &mut self,
        result: &CallResult,
    ) -> Vec<DiagnosticRelatedInformation> {
        let &CallResult::ArgumentTypeMismatch {
            expected, actual, ..
        } = result
        else {
            return Vec::new();
        };
        let actual_str = self.format_type_diagnostic(actual);
        let expected_str = self.format_type_diagnostic(expected);
        vec![Diagnostic::related_message(
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            self.ctx.file_name.clone(),
            0,
            0,
            format_message(
                diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
                &[&actual_str, &expected_str],
            ),
        )]
    }

    /// Emit a class-decorator signature-resolution failure (TS1238) with the
    /// full tsc elaboration for the specific [`CallResult`] shape:
    ///
    /// - argument-count mismatch -> TS1278/TS1279 ("The runtime will invoke
    ///   the decorator with {1} arguments, but the decorator expects {0}[ at
    ///   least]."), plus the TS6210/TS6236 missing-argument pointer;
    /// - non-callable callee -> the not-callable / no-call-signatures chain;
    /// - argument type mismatch -> the TS2345 "not assignable" line.
    ///
    /// Any other shape falls back to the bare primary message. The class-
    /// decorator sites feed the *real* value/context argument types
    /// (`typeof C`, `ClassDecoratorContext<typeof C>`), so the rendered
    /// argument-mismatch type matches tsc exactly — unlike the legacy member
    /// paths, which supply synthetic placeholders and therefore keep using
    /// [`Self::emit_decorator_signature_error`] (arity elaboration only).
    pub(crate) fn emit_class_decorator_signature_error(
        &mut self,
        decorator_node: NodeIndex,
        result: &CallResult,
        resolved: TypeId,
    ) {
        let anchor = self.class_decorator_failure_anchor(decorator_node, result);
        // Each failure shape owns exactly one elaboration; dispatch directly
        // rather than probing each helper in turn.
        let related = match result {
            CallResult::ArgumentCountMismatch { .. }
            | CallResult::OverloadArgumentCountMismatch { .. } => {
                self.decorator_arity_related_info(decorator_node, result)
            }
            CallResult::ArgumentTypeMismatch { .. } => {
                self.decorator_argument_mismatch_related_info(result)
            }
            CallResult::NotCallable { .. } => self.decorator_not_callable_related_info(resolved),
            _ => Vec::new(),
        };
        self.error_at_node_maybe_related(
            anchor,
            diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            related,
        );
    }

    /// The anchor tsc uses for a class-decorator call failure, keyed on the
    /// failure kind rather than a parameter-count heuristic: the whole
    /// decorator (spanning the leading `@`) only when the decorator was given
    /// *too few* arguments (`decorator(classConstructor[, context])` cannot
    /// supply a required parameter), and the decorator expression alone for a
    /// too-many-arguments count, an argument type mismatch, or a non-callable
    /// callee. This mirrors tsc's call-node anchoring and, unlike the
    /// parameter-count heuristic used for member decorators, is not fooled by
    /// a rest parameter inflating the declared count on a type-mismatch
    /// failure.
    fn class_decorator_failure_anchor(
        &self,
        decorator_node: NodeIndex,
        result: &CallResult,
    ) -> NodeIndex {
        let too_few = match result {
            CallResult::ArgumentCountMismatch {
                expected_min,
                actual,
                ..
            } => actual < expected_min,
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                ..
            } => actual < expected_low,
            _ => false,
        };
        if too_few {
            decorator_node
        } else {
            self.decorator_expression_anchor(decorator_node)
        }
    }

    /// tsc's TS1238 "not callable" shape for a class decorator whose resolved
    /// type carries no call signature at all — a bare primitive, a class
    /// (construct signatures only, no call signatures), or any other
    /// non-callable value. Emits the primary TS1238 anchored at `anchor` with
    /// the "This expression is not callable." / "Type 'X' has no call
    /// signatures." chain beneath it. Oracle-verified (`typescript@7.0.2`) to
    /// render identically under `--experimentalDecorators` and ES (TC39
    /// stage-3) class decorators.
    pub(crate) fn emit_class_decorator_not_callable(
        &mut self,
        anchor: NodeIndex,
        resolved: TypeId,
    ) {
        let related = self.decorator_not_callable_related_info(resolved);
        self.error_at_node_maybe_related(
            anchor,
            diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_CLASS_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            related,
        );
    }

    /// Whether a decorator expression is a bare referenceable entity — an
    /// identifier, a (possibly nested) property-access chain, or a call
    /// expression (`@d()`, whose zero-arg *result* can itself be the
    /// "forgot to call it" shape one level deeper) — rather than a
    /// parenthesized or inline function expression.
    ///
    /// Observed tsc behavior (oracle-verified against `typescript@7.0.2`): the
    /// TS1329 "did you mean to call it first" hint fires for referenceable
    /// decorators (`@d`, `@ns.d`, `@d()`). A parenthesized/inline factory
    /// (`@(() => {})`, `@(d)`) instead reports the plain arity/signature
    /// failure. The class path gates the shared zero-argument-factory check
    /// ([`Self::decorator_has_zero_arg_factory_shape`]) on this predicate; the
    /// member/parameter paths do not front-guard it, so this deliberately does
    /// not attempt to unify their (separate) handling.
    pub(crate) fn decorator_expression_is_reference(&self, expr: NodeIndex) -> bool {
        self.ctx.arena.get(expr).is_some_and(|node| {
            node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::CALL_EXPRESSION
        })
    }

    /// tsc anchors member/parameter decorator failures (TS1239/TS1240/TS1241)
    /// at the decorator's EXPRESSION (one column after the `@`) for
    /// type-mismatch and extra-argument failures, but at the whole DECORATOR
    /// (spanning the `@`) when the failure is missing required arguments —
    /// mirroring call-expression arity anchoring (decoratorOnClassProperty6
    /// vs 7).
    pub(crate) fn decorator_expression_anchor(&self, decorator_node: NodeIndex) -> NodeIndex {
        self.ctx
            .arena
            .get(decorator_node)
            .and_then(|n| self.ctx.arena.get_decorator(n))
            .map(|d| d.expression)
            .filter(|e| e.is_some())
            .unwrap_or(decorator_node)
    }

    fn decorator_failure_anchor(
        &self,
        decorator_node: NodeIndex,
        resolved: TypeId,
        provided_args: usize,
    ) -> NodeIndex {
        // "Too few arguments" = every declared signature demands more
        // parameters than the decorator protocol provides at this position.
        let too_few_args = Self::decorator_signature_param_counts(self.ctx.types, resolved)
            .is_some_and(|counts| {
                !counts.is_empty() && counts.iter().all(|&count| count > provided_args)
            });
        if too_few_args {
            decorator_node
        } else {
            self.decorator_expression_anchor(decorator_node)
        }
    }

    /// TS1240 for ES class-member decorators (TC39 stage 3).
    ///
    /// The runtime calling convention for the first argument varies by member kind:
    ///
    /// - Plain field (`x = …`): `undefined`
    /// - Auto-accessor (`accessor x = …`): a `ClassAccessorDecoratorTarget<This, V>`
    ///   object — `{ get(this: This): V; set(this: This, value: V): void }`
    ///
    /// Callers select the first-arg type per member kind; this helper resolves
    /// the decorator type and verifies it is callable with `(first_arg, ANY)`,
    /// emitting TS1240 otherwise. The second argument (the decorator context)
    /// is `ANY` because the calling convention is distinguished by the first
    /// argument shape alone — the context object differs by kind but tsc
    /// reports the same TS1240 either way.
    ///
    /// The synthetic argument list is truncated to the decorator's own declared
    /// parameter count via [`Self::es_member_decorator_argument_count`], mirroring
    /// tsc's `getDecoratorArgumentCount` (`min(max(paramCount, 1), 2)`). A
    /// 1-parameter decorator therefore only receives `first_arg`; tsc never
    /// passes a trailing context argument the decorator did not declare, so
    /// requiring exact 2-arity here produced a spurious TS1240/TS1241.
    ///
    /// A decorator whose every signature accepts zero arguments is reported as a
    /// "did you mean to call it" TS1329 (the decorator-factory hint), matching
    /// tsc's `isPotentiallyUncalledDecorator`, instead of the generic TS1240.
    pub(crate) fn check_es_member_decorator_call_signature(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        first_arg: TypeId,
        actual_this_type: Option<TypeId>,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        if self.decorator_has_zero_arg_factory_shape(decorator_expr, resolved, decorator_node) {
            return;
        }

        let all_args = [first_arg, TypeId::ANY];
        // `es_member_decorator_argument_count` returns 1..=2, so this never panics.
        let args = &all_args[..self.es_member_decorator_argument_count(resolved)];
        let (result, _, _) =
            self.resolve_call_with_checker_adapter(resolved, args, false, None, actual_this_type);

        if !matches!(result, CallResult::Success(_)) {
            self.emit_decorator_signature_error(
                self.decorator_failure_anchor(decorator_node, resolved, args.len()),
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                &result,
            );
        }
    }

    /// Mirror of tsc's `isUntypedFunctionCall` for decorator-callee resolution.
    ///
    /// tsc treats a non-callable callee as "untyped" — and skips signature
    /// validation — only when the callee has *no* call signatures, *no*
    /// construct signatures, is not a union, and is **assignable to the
    /// global `Function` type**. The full Function-shaped check is stricter
    /// than "has a `bind` member": objects like `{ bind: any }` are not
    /// assignable to `Function` (which also requires `apply`, `call`,
    /// `prototype`, `length`, …) and must still emit TS1238/1239/1240/1241.
    ///
    /// Without this fallback, decorator factories whose declared return type
    /// is `Function` would produce a spurious decorator-signature diagnostic
    /// because the `Function` interface has no explicit call signatures of
    /// its own.
    ///
    /// Returns `Some(t)` when the caller should continue with the callee, or
    /// `None` when the callee is Function-typed and the caller should skip
    /// the call check entirely.
    ///
    /// Hot path: most decorators are explicit function types with a known
    /// call signature, so `has_function_shape` short-circuits before the
    /// more expensive Function-membership probe.
    pub(crate) fn prepare_decorator_callee(&mut self, decorator_type: TypeId) -> Option<TypeId> {
        if crate::query_boundaries::common::has_function_shape(self.ctx.types, decorator_type) {
            return Some(decorator_type);
        }
        if self.decorator_callee_is_untyped_function(decorator_type) {
            return None;
        }
        Some(decorator_type)
    }

    /// True when `decorator_type` would qualify as an "untyped function call"
    /// callee under tsc's `isUntypedFunctionCall`: no call signatures, no
    /// construct signatures, not a union, and assignable to the global
    /// `Function` type. Callers use this to skip signature validation when
    /// tsc would.
    fn decorator_callee_is_untyped_function(&mut self, decorator_type: TypeId) -> bool {
        // Reject unions outright — tsc's `isUntypedFunctionCall` explicitly
        // excludes them ("a union of function types that happen to have no
        // common signatures" is still a typed call).
        if crate::query_boundaries::common::union_members(self.ctx.types, decorator_type).is_some()
        {
            return false;
        }

        // Reject callees with call signatures (handled by the caller's
        // fast path, but be defensive) or construct signatures.
        let has_calls = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            decorator_type,
        )
        .is_some_and(|sigs| !sigs.is_empty());
        if has_calls {
            return false;
        }
        let has_constructs = crate::query_boundaries::class_type::construct_signatures_for_type(
            self.ctx.types,
            decorator_type,
        )
        .is_some_and(|sigs| !sigs.is_empty());
        if has_constructs {
            return false;
        }

        // The direct global `Function` type (and `typeof v` where v: Function)
        // is the only common decorator-typed-as-Function case. Match it
        // narrowly via the existing `is_global_function_type` query, which
        // compares via the canonical Function `DefId`.
        if self.is_global_function_type(decorator_type) {
            return true;
        }

        // For rare cases like `interface SubFunc extends Function {}` used
        // as a decorator return type, fall back to a structural
        // assignability check against the global `Function` interface.
        let Some(function_type) = self.global_function_type_id() else {
            return false;
        };
        self.decorator_callee_relation_outcome(decorator_type, function_type)
            .related
    }

    fn global_function_type_id(&mut self) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs("Function", &lib_binders)?;
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        Some(decorator_query::decorator_global_type_ref(
            self.ctx.types,
            def_id,
        ))
    }

    /// Resolve `ClassAccessorDecoratorTarget<any, any>` from the lib globals.
    ///
    /// Returns `None` if the lib is not available (e.g. `--noLib`); callers
    /// fall back to a permissive shape so absent libs do not cause false
    /// positives.
    pub(crate) fn resolve_class_accessor_decorator_target_any(&mut self) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs("ClassAccessorDecoratorTarget", &lib_binders)?;
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        Some(decorator_query::class_accessor_decorator_target_any(
            self.ctx.types,
            def_id,
        ))
    }

    /// tsc's `getDecoratorArgumentCount` for ES (TC39 stage-3) member
    /// decorators: the decorator is invoked with `min(max(paramCount, 1), 2)`
    /// arguments, where `paramCount` is the decorator function's own declared
    /// parameter count.
    ///
    /// This is what lets a 1-parameter decorator (`(value: any) => any`) be
    /// accepted on a method or field: only the `value`/`target` argument is
    /// supplied and the trailing context argument is dropped. A decorator that
    /// declares 3+ required parameters still under-flows the 2-argument cap and
    /// fails, exactly as in tsc.
    ///
    /// Overloaded (multi-signature) decorators keep the historical 2-argument
    /// behavior: tsc resolves the arity per candidate signature, which a single
    /// fixed synthetic argument list cannot reproduce, so the longer 2-argument
    /// form is supplied rather than risk dropping an argument a wider overload
    /// requires.
    pub(crate) fn es_member_decorator_argument_count(&self, decorator_type: TypeId) -> usize {
        // `min(max(paramCount, 1), 2)` for a single declared signature; an
        // unknown shape or an overload set falls back to the full two-argument
        // call so exotic callees behave as they did before.
        match Self::decorator_signature_param_counts(self.ctx.types, decorator_type).as_deref() {
            Some(&[count]) => count.clamp(1, 2),
            _ => 2,
        }
    }

    /// Declared value-parameter counts, one entry per call signature, for a
    /// decorator callee — or `None` when the callee has no statically known
    /// function/callable shape. A plain function type yields a single-element
    /// list; an overloaded callable yields one entry per call signature (which
    /// may be empty when the callable carries no call signatures).
    ///
    /// Shared by the decorator-arity helpers in this module so the
    /// `function_shape` → `callable_shape_for_type` probe is written once.
    fn decorator_signature_param_counts(
        db: &dyn tsz_solver::construction::TypeDatabase,
        decorator_type: TypeId,
    ) -> Option<Vec<usize>> {
        if let Some(shape) = crate::query_boundaries::class_type::function_shape(db, decorator_type)
        {
            return Some(vec![shape.params.len()]);
        }
        let callable =
            crate::query_boundaries::class_type::callable_shape_for_type(db, decorator_type)?;
        Some(
            callable
                .call_signatures
                .iter()
                .map(|sig| sig.params.len())
                .collect(),
        )
    }

    pub(crate) fn check_method_or_accessor_decorator_call_signature(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
        member_node: NodeIndex,
        experimental_decorators: bool,
        actual_this_type: Option<TypeId>,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        if self.decorator_has_zero_arg_factory_shape(decorator_expr, resolved, decorator_node) {
            return;
        }

        let arg_types = if experimental_decorators {
            // tsc's `getLegacyDecoratorArgumentCount` adapts the supplied
            // argument count to the decorator's signature for method/accessor
            // decorators: 2 args when every call signature has ≤ 2 parameters,
            // 3 args otherwise. Without this adaptation, a 2-parameter legacy
            // decorator factory like `(target: object, key: PropertyKey) =>
            // void` produces a spurious TS1241 when applied to a method.
            if Self::legacy_method_decorator_uses_two_args(self.ctx.types, resolved) {
                vec![TypeId::ANY, TypeId::STRING]
            } else {
                let descriptor_type = self
                    .legacy_method_or_accessor_descriptor_type(member_node)
                    .unwrap_or(TypeId::ANY);
                vec![TypeId::ANY, TypeId::STRING, descriptor_type]
            }
        } else {
            // ES (TC39) member decorators: truncate the synthetic
            // `(value, context)` argument list to the decorator's own declared
            // parameter count so a 1-parameter decorator is not rejected for an
            // unrequested trailing context argument. See
            // `es_member_decorator_argument_count`.
            let mut args = self
                .es_method_or_accessor_decorator_args(member_node)
                .unwrap_or_else(|| vec![TypeId::ANY, TypeId::OBJECT]);
            args.truncate(self.es_member_decorator_argument_count(resolved));
            args
        };

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &arg_types,
            false,
            None,
            actual_this_type,
        );

        let return_type = match &result {
            CallResult::Success(return_type) => Some(*return_type),
            _ => {
                self.emit_decorator_signature_error(
                    self.decorator_failure_anchor(decorator_node, resolved, arg_types.len()),
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    &result,
                );
                self.recover_decorator_return_type_with_any_args(resolved)
                    .or_else(|| {
                        crate::query_boundaries::checkers::call::stable_call_recovery_return_type(
                            self.ctx.types,
                            resolved,
                        )
                    })
            }
        };

        self.check_method_or_accessor_decorator_return_type(
            decorator_node,
            member_node,
            experimental_decorators,
            return_type,
        );
    }

    /// Mirror of tsc's `getLegacyDecoratorArgumentCount` for the
    /// method/accessor decorator case. Returns `true` when every call
    /// signature on the decorator has ≤ 2 parameters, indicating that tsc
    /// would supply only 2 arguments (target, propertyKey) instead of the
    /// usual 3 (target, propertyKey, descriptor).
    ///
    /// The decision is made over *all* call signatures so that an overloaded
    /// decorator with a 3-parameter signature still receives the descriptor
    /// argument. This matches tsc's overload semantics: the resolved
    /// signature drives the arity, and any signature that needs the
    /// descriptor will be chosen when 3 args are supplied.
    fn legacy_method_decorator_uses_two_args(
        db: &dyn tsz_solver::construction::TypeDatabase,
        decorator_type: TypeId,
    ) -> bool {
        // An empty signature list (callable shape with no call signatures) or an
        // unknown shape keeps the historical 3-arg default so recovery-path
        // diagnostics stay stable; otherwise every declared signature must fit
        // within the 2-arg (target, propertyKey) call.
        Self::decorator_signature_param_counts(db, decorator_type)
            .is_some_and(|counts| !counts.is_empty() && counts.iter().all(|&n| n <= 2))
    }

    /// TS1329: Check if a class-member decorator accepts too few arguments.
    ///
    /// Member decorators are invoked with at least one argument (the value or
    /// target) in stage-3 mode and two/three arguments in legacy mode. If every
    /// call signature has zero parameters, tsc reports the decorator-factory
    /// hint ("did you mean to call it") instead of the generic
    /// method/property-decorator signature failure. This mirrors tsc's
    /// `isPotentiallyUncalledDecorator` for the common zero-parameter case and
    /// applies uniformly to method, accessor, field, and (see
    /// `class_decorators.rs`) class decorators.
    pub(crate) fn decorator_has_zero_arg_factory_shape(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
    ) -> bool {
        if decorator_type_is_unchecked(decorator_type) {
            return false;
        }

        // tsc's `isPotentiallyUncalledDecorator` only substitutes the TS1329
        // hint for a bare identifier, property-access chain, or call
        // expression — a parenthesized decorator expression (`@(() => {})`)
        // keeps the generic TS1238/1239/1240/1241 arity elaboration instead.
        let is_parenthesized = self
            .ctx
            .arena
            .get(decorator_expr)
            .is_some_and(|n| n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION);
        if is_parenthesized {
            return false;
        }

        if Self::decorator_signature_accepts_no_arguments(self.ctx.types, decorator_type) {
            let name = self.get_decorator_expression_name(decorator_expr);
            let msg = diagnostic_messages::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT
                .replace("{0}", &name);
            self.error_at_node(
                decorator_node,
                &msg,
                diagnostic_codes::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT,
            );
            return true;
        }

        false
    }

    /// Whether every call signature of a decorator expression takes zero
    /// parameters — the shape of a decorator factory written without its
    /// call (`@factory` rather than `@factory()`).
    ///
    /// Mirrors tsc's `isPotentiallyUncalledDecorator` for the common
    /// zero-parameter case. tsc reports only the TS1329 "did you mean to call
    /// it first" hint for these, so every other decorator diagnostic that
    /// would otherwise fall out of the failed call — the signature failure and
    /// the return-type check alike — must stand down.
    fn decorator_signature_accepts_no_arguments(
        db: &dyn tsz_solver::construction::TypeDatabase,
        decorator_type: TypeId,
    ) -> bool {
        Self::decorator_signature_param_counts(db, decorator_type)
            .is_some_and(|counts| !counts.is_empty() && counts.iter().all(|&n| n == 0))
    }

    fn es_method_or_accessor_decorator_args(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<Vec<TypeId>> {
        let member = self.ctx.arena.get(member_idx)?;
        match member.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                let value_type = self.method_decorator_value_type(member_idx)?;
                let context_type = self
                    .resolve_decorator_context_type(
                        "ClassMethodDecoratorContext",
                        vec![TypeId::ANY, value_type],
                    )
                    .unwrap_or(TypeId::OBJECT);
                Some(vec![value_type, context_type])
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR => {
                let value_type = self.accessor_decorator_value_type(member_idx)?;
                let value = self
                    .accessor_value_type_argument(member_idx)
                    .unwrap_or(TypeId::ANY);
                let context_type = self
                    .resolve_decorator_context_type(
                        "ClassGetterDecoratorContext",
                        vec![TypeId::ANY, value],
                    )
                    .unwrap_or(TypeId::OBJECT);
                Some(vec![value_type, context_type])
            }
            k if k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR => {
                let value_type = self.accessor_decorator_value_type(member_idx)?;
                let value = self
                    .accessor_value_type_argument(member_idx)
                    .unwrap_or(TypeId::ANY);
                let context_type = self
                    .resolve_decorator_context_type(
                        "ClassSetterDecoratorContext",
                        vec![TypeId::ANY, value],
                    )
                    .unwrap_or(TypeId::OBJECT);
                Some(vec![value_type, context_type])
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_decorator_context_type(
        &mut self,
        name: &str,
        args: Vec<TypeId>,
    ) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)?;
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        Some(decorator_query::decorator_context_application(
            self.ctx.types,
            def_id,
            args,
        ))
    }

    fn method_decorator_value_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        let method = self.ctx.arena.get_method_decl(member)?;
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        let return_type = if method.type_annotation.is_some() {
            self.get_type_from_type_node(method.type_annotation)
        } else if method.body.is_some() {
            self.infer_return_type_from_body(member_idx, method.body, None)
        } else {
            TypeId::ANY
        };
        self.pop_type_parameters(type_param_updates);

        Some(decorator_query::method_decorator_value_type(
            self.ctx.types,
            type_params,
            params,
            this_type,
            return_type,
        ))
    }

    fn accessor_decorator_value_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        let accessor = self.ctx.arena.get_accessor(member)?;
        let (params, this_type) = self.extract_params_from_parameter_list(&accessor.parameters);
        let return_type = if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
            if accessor.type_annotation.is_some() {
                self.get_type_from_type_node(accessor.type_annotation)
            } else if accessor.body.is_some() {
                self.infer_return_type_from_body(member_idx, accessor.body, None)
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::VOID
        };

        Some(decorator_query::accessor_decorator_value_type(
            self.ctx.types,
            params,
            this_type,
            return_type,
        ))
    }

    fn accessor_value_type_argument(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        let accessor = self.ctx.arena.get_accessor(member)?;
        if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
            if accessor.type_annotation.is_some() {
                return Some(self.get_type_from_type_node(accessor.type_annotation));
            }
            if accessor.body.is_some() {
                return Some(self.infer_return_type_from_body(member_idx, accessor.body, None));
            }
            return Some(TypeId::ANY);
        }

        let first_param = accessor.parameters.nodes.first().copied()?;
        let param_node = self.ctx.arena.get(first_param)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        if param.type_annotation.is_some() {
            Some(self.get_type_from_type_node(param.type_annotation))
        } else {
            Some(TypeId::ANY)
        }
    }

    fn check_method_or_accessor_decorator_return_type(
        &mut self,
        decorator_node: NodeIndex,
        member_idx: NodeIndex,
        experimental_decorators: bool,
        return_type: Option<TypeId>,
    ) {
        let Some(return_type) = return_type else {
            return;
        };
        let return_type = self.evaluate_type_for_assignability(return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return;
        }

        let Some(expected_return) = self
            .method_or_accessor_decorator_expected_return_type(member_idx, experimental_decorators)
        else {
            return;
        };
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
            self.error_at_node(
                decorator_node,
                &message,
                diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
    }

    fn method_or_accessor_decorator_expected_return_type(
        &mut self,
        member_idx: NodeIndex,
        experimental_decorators: bool,
    ) -> Option<TypeId> {
        if !experimental_decorators {
            let value_type = self.method_or_accessor_decorator_value_type(member_idx)?;
            return Some(decorator_query::decorator_void_or_replacement_type(
                self.ctx.types,
                value_type,
            ));
        }

        let descriptor_type = self.legacy_method_or_accessor_descriptor_type(member_idx)?;
        Some(decorator_query::decorator_void_or_replacement_type(
            self.ctx.types,
            descriptor_type,
        ))
    }

    fn method_or_accessor_decorator_value_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        match member.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                self.method_decorator_value_type(member_idx)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR =>
            {
                self.accessor_decorator_value_type(member_idx)
            }
            _ => None,
        }
    }

    fn legacy_method_or_accessor_descriptor_type(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<TypeId> {
        let descriptor_value = self.legacy_method_or_accessor_descriptor_value_type(member_idx)?;
        self.resolve_decorator_context_type("TypedPropertyDescriptor", vec![descriptor_value])
    }

    fn legacy_method_or_accessor_descriptor_value_type(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        match member.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                self.method_decorator_value_type(member_idx)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR =>
            {
                self.accessor_value_type_argument(member_idx)
            }
            _ => None,
        }
    }

    /// TS1240/TS1271 for legacy property decorators.
    ///
    /// Under `experimentalDecorators`, plain fields use the legacy property
    /// decorator ABI `(target, propertyKey)`, while `accessor` fields use
    /// `(target, propertyKey, descriptor)`. Both forms require the decorator
    /// return type to be `void` or `any`.
    pub(crate) fn check_legacy_property_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        is_auto_accessor: bool,
        actual_this_type: Option<TypeId>,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        // A zero-parameter decorator function is a factory used uncalled:
        // tsc reports only the TS1329 call-it-first hint, not TS1240/TS1271.
        if self.decorator_has_zero_arg_factory_shape(decorator_expr, resolved, decorator_node) {
            return;
        }

        let arg_types: &[TypeId] = if is_auto_accessor {
            &[TypeId::ANY, TypeId::STRING, TypeId::ANY]
        } else {
            &[TypeId::ANY, TypeId::STRING]
        };
        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            arg_types,
            false,
            None,
            actual_this_type,
        );

        let return_type = match &result {
            CallResult::Success(return_type) => Some(*return_type),
            _ => {
                self.emit_decorator_signature_error(
                    self.decorator_failure_anchor(decorator_node, resolved, arg_types.len()),
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    &result,
                );
                self.recover_decorator_return_type_with_any_args(resolved)
                    .or_else(|| {
                        crate::query_boundaries::checkers::call::stable_call_recovery_return_type(
                            self.ctx.types,
                            resolved,
                        )
                    })
            }
        };

        let Some(return_type) = return_type else {
            return;
        };
        let return_type = self.evaluate_type_for_assignability(return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY) {
            return;
        }
        if !self
            .return_relation_outcome(return_type, TypeId::VOID)
            .related
        {
            let return_str = self.format_type_diagnostic(return_type);
            let message = format_message(
                diagnostic_messages::DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY,
                &[&return_str],
            );
            self.error_at_node(
                decorator_node,
                &message,
                diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY,
            );
        }
    }

    fn recover_decorator_return_type_with_any_args(
        &mut self,
        decorator_type: TypeId,
    ) -> Option<TypeId> {
        let arg_count =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, decorator_type)
                .map(|shape| shape.params.len())
                .or_else(|| {
                    crate::query_boundaries::class_type::callable_shape_for_type(
                        self.ctx.types,
                        decorator_type,
                    )
                    .and_then(|shape| {
                        shape
                            .call_signatures
                            .first()
                            .map(|signature| signature.params.len())
                    })
                })?;

        let args = vec![TypeId::ANY; arg_count];
        let (result, _, _) =
            self.resolve_call_with_checker_adapter(decorator_type, &args, false, None, None);
        match result {
            CallResult::Success(return_type) => Some(return_type),
            _ => None,
        }
    }

    /// TS1239: Check that a parameter decorator expression has a compatible
    /// call signature for the runtime invocation
    /// `decorator(target, propertyKey, parameterIndex)`.
    ///
    /// For experimental decorators, the runtime calling convention differs
    /// between constructor parameters and method/accessor parameters:
    ///
    /// - Constructor parameters: `decorator(classCtor, undefined, paramIndex)`
    /// - Method/accessor parameters:
    ///   `decorator(prototype, methodName, paramIndex)`
    ///
    /// When the decorator's resolved signature cannot be called with the
    /// shape that matches the parameter's enclosing function, tsc emits
    /// TS1239 ("Unable to resolve signature of parameter decorator when
    /// called as an expression."). The most common case is a decorator
    /// like `(target: Object, key: string, idx: number) => void` applied to
    /// a constructor parameter — `key: string` rejects `undefined`.
    ///
    /// `is_constructor_parameter` selects the `key` argument shape:
    /// `TypeId::UNDEFINED` for constructor params, `TypeId::STRING` for
    /// method/accessor params. We pass `TypeId::ANY` for `target` and the
    /// concrete-enough `TypeId::NUMBER` for `parameterIndex`; the call only
    /// needs to reject decorators whose param TYPES disagree with the
    /// runtime shape.
    pub(crate) fn check_parameter_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        is_constructor_parameter: bool,
        actual_this_type: Option<TypeId>,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        // Per the runtime calling convention above, only the key argument
        // shape varies by parameter position.
        let key_arg = if is_constructor_parameter {
            TypeId::UNDEFINED
        } else {
            TypeId::STRING
        };

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[TypeId::ANY, key_arg, TypeId::NUMBER],
            false,
            None,
            actual_this_type,
        );

        let return_type = match &result {
            CallResult::Success(return_type) => Some(*return_type),
            _ => {
                self.emit_decorator_signature_error(
                    self.decorator_failure_anchor(decorator_node, resolved, 3),
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_PARAMETER_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_PARAMETER_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    &result,
                );
                // tsc keeps checking the return type after a failed signature
                // resolution — `@bad` whose call fails AND whose return type is
                // not `void` draws TS1239 and TS1271 at the same anchor. Recover
                // the return type the same way the property-decorator path does.
                self.recover_decorator_return_type_with_any_args(resolved)
                    .or_else(|| {
                        crate::query_boundaries::checkers::call::stable_call_recovery_return_type(
                            self.ctx.types,
                            resolved,
                        )
                    })
            }
        };

        self.check_parameter_decorator_return_type(decorator_node, resolved, return_type);
    }

    /// TS1271 for legacy parameter decorators.
    ///
    /// The runtime discards a parameter decorator's result, so tsc requires the
    /// return type to be assignable to `void` (`any` short-circuits). This is
    /// the sibling of the identical check
    /// [`Self::check_legacy_property_decorator_call_signature`] runs for
    /// property decorators; the parameter half was never wired, so a parameter
    /// decorator returning a value was silently accepted.
    fn check_parameter_decorator_return_type(
        &mut self,
        decorator_node: NodeIndex,
        resolved: TypeId,
        return_type: Option<TypeId>,
    ) {
        // An uncalled zero-parameter decorator factory draws only the TS1329
        // "did you mean to call it first" hint; its (unreachable) return type is
        // not judged.
        if Self::decorator_signature_accepts_no_arguments(self.ctx.types, resolved) {
            return;
        }

        let Some(return_type) = return_type else {
            return;
        };
        let return_type = self.evaluate_type_for_assignability(return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY) {
            return;
        }
        if self
            .return_relation_outcome(return_type, TypeId::VOID)
            .related
        {
            return;
        }

        let return_str = self.format_type_diagnostic(return_type);
        let message = format_message(
            diagnostic_messages::DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY,
            &[&return_str],
        );
        self.error_at_node(
            self.decorator_expression_anchor(decorator_node),
            &message,
            diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY,
        );
    }

    fn get_decorator_expression_name(&self, expr: NodeIndex) -> String {
        if let Some(node) = self.ctx.arena.get(expr)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            return ident.escaped_text.to_string();
        }
        "decorator".to_string()
    }
}
