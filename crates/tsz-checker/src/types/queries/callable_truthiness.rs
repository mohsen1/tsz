//! Truthiness and callable/awaitable-truthiness checks (TS2774, TS2801, TS2872, TS2873).
//!
//! This module implements:
//! - Syntactic truthiness analysis (`getSyntacticTruthySemantics` in tsc)
//! - TS2872: "This kind of expression is always truthy"
//! - TS2873: "This kind of expression is always falsy"
//! - TS2774: "This condition will always return true since this function is
//!   always defined. Did you mean to call it instead?"
//! - TS2801: "This condition will always return true since this '{type}' is
//!   always defined." (for Promise/awaitable types in conditions)

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::type_queries::{LiteralTypeKind, classify_literal_type, get_enum_member_type};

/// Result of tsc's `getSyntacticTruthySemantics` — purely syntactic truthiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntacticTruthiness {
    AlwaysTruthy,
    AlwaysFalsy,
    Sometimes,
}

impl SyntacticTruthiness {
    /// Combine two branches (e.g., both arms of a conditional expression).
    fn combine(self, other: Self) -> Self {
        if self == other { self } else { Self::Sometimes }
    }
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Truthiness Checks
    // =========================================================================

    /// TS2872: "This kind of expression is always truthy."
    ///
    /// Emitted when a truthy-checked expression is syntactically always truthy.
    /// Matches tsc's `getSyntacticTruthySemantics` — purely syntactic, never type-based.
    /// TS2872: Check if expression is syntactically always truthy.
    /// Used for left side of `||` and `??` operators.
    ///
    /// Gated on `strictNullChecks`: without it, all types implicitly include
    /// `null | undefined`, so nothing is semantically "always truthy."
    #[allow(dead_code)]
    pub(crate) fn check_always_truthy(&mut self, node_idx: NodeIndex) {
        if !self.ctx.compiler_options.strict_null_checks {
            return;
        }
        if self.get_syntactic_truthy_semantics(node_idx) == SyntacticTruthiness::AlwaysTruthy {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                node_idx,
                "This kind of expression is always truthy.",
                diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY,
            );
        }
    }

    /// TS2872/TS2873: Check if condition is syntactically always truthy or falsy.
    /// Used for if/while/for conditions. Includes TS2845 enum member checks.
    pub(crate) fn check_truthy_or_falsy(&mut self, node_idx: NodeIndex) {
        let ty = self.get_type_of_node(node_idx);
        self.check_truthy_or_falsy_with_type_inner(node_idx, ty, true);
    }

    /// Same as `check_truthy_or_falsy`, but reuses a caller-provided type.
    ///
    /// Both the `void` TS1345 check and the TS2872/TS2873 syntactic checks are
    /// gated on `strictNullChecks`. Without strict null checks, `void` is
    /// effectively `undefined` (which is a valid falsy value), and all types
    /// implicitly include `null | undefined`, so truthiness checks are not meaningful.
    ///
    /// `check_enum_members` controls TS2845 enum member truthiness diagnostics.
    /// tsc only emits TS2845 for enum members in condition contexts (if/while/for/
    /// ternary), NOT for `||`/`&&` left sides or `!` operands.
    pub(crate) fn check_truthy_or_falsy_with_type(&mut self, node_idx: NodeIndex, ty: TypeId) {
        self.check_truthy_or_falsy_with_type_inner(node_idx, ty, true);
    }

    /// Variant that skips TS2845 enum member checks. Used for `||`/`&&` left
    /// sides and `!` operands where tsc does not emit TS2845.
    pub(crate) fn check_truthy_or_falsy_with_type_no_enum(
        &mut self,
        node_idx: NodeIndex,
        ty: TypeId,
    ) {
        self.check_truthy_or_falsy_with_type_inner(node_idx, ty, false);
    }

    fn check_truthy_or_falsy_with_type_inner(
        &mut self,
        node_idx: NodeIndex,
        ty: TypeId,
        check_enum_members: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // TS1345 (void truthiness) requires strictNullChecks
        if self.ctx.compiler_options.strict_null_checks && ty == TypeId::VOID {
            self.error_at_node(
                node_idx,
                "An expression of type 'void' cannot be tested for truthiness.",
                diagnostic_codes::AN_EXPRESSION_OF_TYPE_VOID_CANNOT_BE_TESTED_FOR_TRUTHINESS,
            );
        }

        if check_enum_members
            && let Some(condition_result) = self.enum_member_condition_result(node_idx, ty)
        {
            self.error_at_node_msg(
                node_idx,
                diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN,
                &[condition_result],
            );
        }

        match self.get_syntactic_truthy_semantics(node_idx) {
            SyntacticTruthiness::AlwaysTruthy => {
                // Defer TS2872 to survive speculative call-resolution rollbacks.
                // The diagnostic is purely syntactic and must not be lost when
                // generic inference re-evaluates arguments.
                if let Some((start, end)) = self.get_node_span(node_idx) {
                    let raw_length = end.saturating_sub(start);
                    let (start, length) = self.normalized_anchor_span(node_idx, start, raw_length);
                    self.ctx.deferred_truthiness_diagnostics.push(
                        crate::diagnostics::Diagnostic::error(
                            self.ctx.file_name.clone(),
                            start,
                            length,
                            "This kind of expression is always truthy.".to_string(),
                            diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY,
                        ),
                    );
                }
            }
            SyntacticTruthiness::AlwaysFalsy => {
                // Defer TS2873 to survive speculative call-resolution rollbacks.
                if let Some((start, end)) = self.get_node_span(node_idx) {
                    let raw_length = end.saturating_sub(start);
                    let (start, length) = self.normalized_anchor_span(node_idx, start, raw_length);
                    self.ctx.deferred_truthiness_diagnostics.push(
                        crate::diagnostics::Diagnostic::error(
                            self.ctx.file_name.clone(),
                            start,
                            length,
                            "This kind of expression is always falsy.".to_string(),
                            diagnostic_codes::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY,
                        ),
                    );
                }
            }
            SyntacticTruthiness::Sometimes => {}
        }
    }

    fn enum_member_condition_result(
        &mut self,
        node_idx: NodeIndex,
        ty: TypeId,
    ) -> Option<&'static str> {
        let node = self.ctx.arena.get(node_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            let inner_ty = self.get_type_of_node(paren.expression);
            return self.enum_member_condition_result(paren.expression, inner_ty);
        }

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.ctx.arena.get_unary_expr_ex(node)?;
            let inner_ty = self.get_type_of_node(unary.expression);
            return self.enum_member_condition_result(unary.expression, inner_ty);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.ctx.arena.get_type_assertion(node)?;
            let inner_ty = self.get_type_of_node(assertion.expression);
            return self.enum_member_condition_result(assertion.expression, inner_ty);
        }

        let ty = self.evaluate_type_with_env(ty);
        let sym_id = self.ctx.resolve_type_to_symbol_id(ty)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::ENUM_MEMBER) == 0 {
            return None;
        }

        let underlying = get_enum_member_type(self.ctx.types, ty)?;
        match classify_literal_type(self.ctx.types, underlying) {
            LiteralTypeKind::Number(value) => Some(if value == 0.0 { "false" } else { "true" }),
            LiteralTypeKind::String(value) => Some(if value.is_none() { "false" } else { "true" }),
            LiteralTypeKind::Boolean(value) => Some(if value { "true" } else { "false" }),
            _ => None,
        }
    }

    /// Matches tsc's `getSyntacticTruthySemantics`.
    ///
    /// Returns whether an expression is syntactically always truthy, always falsy,
    /// or sometimes either. This is a purely syntactic check — it examines node kinds
    /// and literal values, never the resolved type.
    fn get_syntactic_truthy_semantics(&self, node_idx: NodeIndex) -> SyntacticTruthiness {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return SyntacticTruthiness::Sometimes;
        };

        // Skip parenthesized expressions (tsc's skipOuterExpressions)
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                return self.get_syntactic_truthy_semantics(paren.expression);
            }
            return SyntacticTruthiness::Sometimes;
        }

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                return self.get_syntactic_truthy_semantics(unary.expression);
            }
            return SyntacticTruthiness::Sometimes;
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                return self.get_syntactic_truthy_semantics(assertion.expression);
            }
            return SyntacticTruthiness::Sometimes;
        }

        match node.kind {
            // Numeric literals: 0 and 1 are "sometimes" (allows while(0)/while(1)),
            // all others are always truthy
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.ctx.arena.get_literal(node)
                    && (lit.text == "0" || lit.text == "1")
                {
                    return SyntacticTruthiness::Sometimes;
                }
                SyntacticTruthiness::AlwaysTruthy
            }
            // These expression kinds are always truthy (they produce objects/functions)
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == SyntaxKind::BigIntLiteral as u16
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == SyntaxKind::RegularExpressionLiteral as u16 =>
            {
                SyntacticTruthiness::AlwaysTruthy
            }
            // void and null are always falsy
            // Note: Our parser represents `void expr` as PREFIX_UNARY_EXPRESSION
            // with VoidKeyword operator, not as a separate VOID_EXPRESSION node.
            k if k == syntax_kind_ext::VOID_EXPRESSION || k == SyntaxKind::NullKeyword as u16 => {
                SyntacticTruthiness::AlwaysFalsy
            }
            // Handle void as a prefix unary (our parser emits void as PREFIX_UNARY_EXPRESSION)
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node)
                    && unary.operator == SyntaxKind::VoidKeyword as u16
                {
                    return SyntacticTruthiness::AlwaysFalsy;
                }
                SyntacticTruthiness::Sometimes
            }
            // `undefined` identifier is always falsy
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.ctx.arena.get_identifier(node)
                    && ident.escaped_text == "undefined"
                {
                    SyntacticTruthiness::AlwaysFalsy
                } else {
                    SyntacticTruthiness::Sometimes
                }
            }
            // String/template literals: truthy if non-empty, falsy if empty
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                if let Some(lit) = self.ctx.arena.get_literal(node) {
                    if lit.text.is_empty() {
                        SyntacticTruthiness::AlwaysFalsy
                    } else {
                        SyntacticTruthiness::AlwaysTruthy
                    }
                } else {
                    SyntacticTruthiness::Sometimes
                }
            }
            // Conditional expressions: combine both branches
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    let when_true = self.get_syntactic_truthy_semantics(cond.when_true);
                    let when_false = self.get_syntactic_truthy_semantics(cond.when_false);
                    when_true.combine(when_false)
                } else {
                    SyntacticTruthiness::Sometimes
                }
            }
            // Everything else (identifiers, call expressions, etc.) is "sometimes"
            _ => SyntacticTruthiness::Sometimes,
        }
    }

    // =========================================================================
    // TS2774: Truthiness check for callable types
    // =========================================================================

    /// TS2774: "This condition will always return true since this function is
    /// always defined. Did you mean to call it instead?"
    ///
    /// Matches tsc's `checkTestingKnownTruthyCallableOrAwaitableOrEnumMemberType`.
    /// Emitted when a non-nullable function type is tested for truthiness in a
    /// condition but the function is never called or referenced in the body.
    pub(crate) fn check_callable_truthiness(
        &mut self,
        cond_expr: NodeIndex,
        body: Option<NodeIndex>,
    ) {
        if !self.ctx.compiler_options.strict_null_checks {
            return;
        }
        self.check_callable_truthiness_inner(cond_expr, cond_expr, body);
    }

    /// Inner helper that handles logical chains recursively.
    fn check_callable_truthiness_inner(
        &mut self,
        cond_expr: NodeIndex,
        top_cond: NodeIndex,
        body: Option<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(cond_expr) else {
            return;
        };

        // Skip parenthesized expressions
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                self.check_callable_truthiness_inner(paren.expression, top_cond, body);
            }
            return;
        }

        // For logical/coalescing binary expressions, recurse into operands
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                let op = bin.operator_token;
                if op == SyntaxKind::AmpersandAmpersandToken as u16
                    || op == SyntaxKind::BarBarToken as u16
                    || op == SyntaxKind::QuestionQuestionToken as u16
                {
                    self.check_callable_truthiness_inner(bin.left, top_cond, body);
                    self.check_callable_truthiness_inner(bin.right, top_cond, body);
                }
            }
            return;
        }

        // Leaf expression — check if it's a callable type
        self.check_callable_truthiness_leaf(cond_expr, top_cond, body);
    }

    /// Check a single leaf expression for TS2774 (callable) and TS2801 (awaitable/Promise).
    fn check_callable_truthiness_leaf(
        &mut self,
        location: NodeIndex,
        top_cond: NodeIndex,
        body: Option<NodeIndex>,
    ) {
        let ty = self.get_type_of_node(location);

        // Skip nullable/error/top types
        if ty == TypeId::ANY || ty == TypeId::UNKNOWN || ty == TypeId::ERROR || ty.is_nullable() {
            return;
        }

        // Skip if type may contain null/undefined (optional params, union with undefined)
        if tsz_solver::is_nullish_type(self.ctx.types.as_type_database(), ty) {
            return;
        }

        let is_callable =
            tsz_solver::type_queries::is_callable_type(self.ctx.types.as_type_database(), ty);
        let is_awaitable = !is_callable && self.is_awaitable_type(ty);

        if !is_callable && !is_awaitable {
            return;
        }

        // Determine what to match: for identifiers, use symbol resolution;
        // for property accesses, use source text span matching;
        // for call expressions, never suppress (each call produces a new value).
        let Some(node) = self.ctx.arena.get(location) else {
            return;
        };
        let is_property_access = node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION;
        let is_call_expression = node.kind == syntax_kind_ext::CALL_EXPRESSION;

        // Skip TS2774 when the property access base involves a type assertion.
        // e.g., `(result as I).method` — the developer is using a cast to check
        // if a property exists at runtime, which is a valid truthiness check pattern.
        // tsc suppresses TS2774 in this case.
        if is_callable
            && is_property_access
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && self.expression_has_type_assertion(access.expression)
        {
            return;
        }

        // For call expressions producing Promises, always emit TS2801
        // (each call returns a new Promise, so body usage doesn't suppress).
        if is_awaitable && is_call_expression {
            self.emit_awaitable_truthiness_error(location, ty);
            return;
        }

        // For identifiers, resolve the symbol
        let tested_sym = if !is_property_access {
            if node.kind == SyntaxKind::Identifier as u16 {
                self.ctx.binder.resolve_identifier(self.ctx.arena, location)
            } else if is_call_expression {
                // For callable types in call expressions, skip (not applicable)
                return;
            } else {
                return; // Only identifiers and property accesses are checked
            }
        } else {
            None // Property accesses use span matching instead
        };

        if !is_property_access && tested_sym.is_none() {
            return;
        }

        // Check if the tested expression is used in the body or binary chain
        let is_used = if is_property_access {
            // For property accesses, use structural chain matching
            self.is_prop_access_in_body_or_chain(location, top_cond, body)
        } else {
            let sym_id =
                tested_sym.expect("non-property-access path requires tested_sym to be Some");
            self.is_callable_symbol_used(sym_id, location, top_cond, body)
        };

        if !is_used {
            if is_awaitable {
                self.emit_awaitable_truthiness_error(location, ty);
            } else {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    location,
                    "This condition will always return true since this function is always defined. Did you mean to call it instead?",
                    diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_FUNCTION_IS_ALWAYS_DEFINED_DID,
                );
            }
        }
    }

    /// Check if a type is awaitable (Promise-like) for TS2801 detection.
    /// Uses solver query boundaries to check for Promise application types.
    fn is_awaitable_type(&self, ty: TypeId) -> bool {
        // Check for Application with PROMISE_BASE via solver query
        if let Some(base) =
            tsz_solver::type_queries::get_application_base(self.ctx.types.as_type_database(), ty)
        {
            if base == TypeId::PROMISE_BASE {
                return true;
            }
        }
        // Also check via the global Promise type classification
        self.is_global_promise_type(ty)
    }

    /// Emit TS2801 for awaitable/Promise types used in truthiness checks.
    fn emit_awaitable_truthiness_error(&mut self, location: NodeIndex, ty: TypeId) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_str = self.format_type(ty);
        let message = format_message(
            diagnostic_messages::THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_IS_ALWAYS_DEFINED,
            &[&type_str],
        );
        self.error_at_node(
            location,
            &message,
            diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_IS_ALWAYS_DEFINED,
        );
    }

    /// Check if a symbol is used in the body or binary expression chain.
    fn is_callable_symbol_used(
        &self,
        sym_id: tsz_binder::SymbolId,
        location: NodeIndex,
        top_cond: NodeIndex,
        body: Option<NodeIndex>,
    ) -> bool {
        if let Some(body_idx) = body
            && self.is_symbol_in_subtree(sym_id, body_idx)
        {
            return true;
        }
        self.is_symbol_in_binary_chain_rhs(sym_id, location, top_cond)
    }

    /// For property access expressions, check if the same access chain appears
    /// in the body or binary chain.
    fn is_prop_access_in_body_or_chain(
        &self,
        location: NodeIndex,
        top_cond: NodeIndex,
        body: Option<NodeIndex>,
    ) -> bool {
        // Build the property access chain for the condition expression
        let chain = self.build_access_chain(location);
        if chain.is_empty() {
            return false;
        }

        if let Some(body_idx) = body
            && self.is_access_chain_in_subtree(&chain, body_idx)
        {
            return true;
        }
        self.is_access_chain_in_binary_rhs(&chain, location, top_cond)
    }

    /// Build a property access chain as a list of identifier names.
    /// For `a.b.c`, returns `["a", "b", "c"]`.
    fn build_access_chain(&self, idx: NodeIndex) -> Vec<String> {
        let mut chain = Vec::new();
        self.build_access_chain_inner(idx, &mut chain);
        chain
    }

    fn build_access_chain_inner(&self, idx: NodeIndex, chain: &mut Vec<String>) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(id) = self.ctx.arena.get_identifier(node) {
                chain.push(self.ctx.arena.resolve_identifier_text(id).to_string());
            }
        } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                self.build_access_chain_inner(access.expression, chain);
                if let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                    && let Some(id) = self.ctx.arena.get_identifier(name_node)
                {
                    chain.push(self.ctx.arena.resolve_identifier_text(id).to_string());
                }
            }
        } else if node.kind == SyntaxKind::ThisKeyword as u16 {
            chain.push("this".to_string());
        }
    }

    /// Check if a property access chain exists anywhere in a subtree.
    fn is_access_chain_in_subtree(&self, target: &[String], subtree: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(subtree) else {
            return false;
        };

        // Check if this node matches the target chain
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == SyntaxKind::Identifier as u16
        {
            let chain = self.build_access_chain(subtree);
            if chain == target {
                return true;
            }
        }

        // Recurse into children
        let children = self.ctx.arena.get_children(subtree);
        for child in children {
            if self.is_access_chain_in_subtree(target, child) {
                return true;
            }
        }
        false
    }

    /// Check binary chain RHS for property access chain matches.
    fn is_access_chain_in_binary_rhs(
        &self,
        target: &[String],
        location: NodeIndex,
        top_cond: NodeIndex,
    ) -> bool {
        let Some(top_node) = self.ctx.arena.get(top_cond) else {
            return false;
        };

        if top_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }

        let Some(bin) = self.ctx.arena.get_binary_expr(top_node) else {
            return false;
        };

        let op = bin.operator_token;
        if op != SyntaxKind::AmpersandAmpersandToken as u16
            && op != SyntaxKind::BarBarToken as u16
            && op != SyntaxKind::QuestionQuestionToken as u16
        {
            return false;
        }

        if self.span_contains(bin.left, location)
            && self.is_access_chain_in_subtree(target, bin.right)
        {
            return true;
        }

        let Some(left_node) = self.ctx.arena.get(bin.left) else {
            return false;
        };
        if left_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self.is_access_chain_in_binary_rhs(target, location, bin.left);
        }

        false
    }

    /// Recursively check if a symbol is referenced in a subtree.
    fn is_symbol_in_subtree(&self, sym_id: tsz_binder::SymbolId, subtree: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(subtree) else {
            return false;
        };

        // Check identifier nodes
        if node.kind == SyntaxKind::Identifier as u16
            && let Some(resolved) = self.ctx.binder.resolve_identifier(self.ctx.arena, subtree)
            && resolved == sym_id
        {
            return true;
        }

        // Recurse into children, but skip function/arrow/class scopes
        // that may shadow the symbol with a parameter of the same name
        let children = self.ctx.arena.get_children(subtree);
        for child in children {
            if let Some(child_node) = self.ctx.arena.get(child)
                && (child_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || child_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || child_node.kind == syntax_kind_ext::CLASS_EXPRESSION)
                && self.func_shadows_symbol(child, sym_id)
            {
                continue;
            }
            if self.is_symbol_in_subtree(sym_id, child) {
                return true;
            }
        }

        false
    }

    /// Check if a function/arrow expression has a parameter that shadows
    /// the given symbol (same name).
    fn func_shadows_symbol(&self, func_idx: NodeIndex, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let sym_name = &symbol.escaped_name;

        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        let Some(func_data) = self.ctx.arena.get_function(func_node) else {
            return false;
        };
        let param_list = &func_data.parameters;
        for &param_idx in &param_list.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param_data) = self.ctx.arena.get_parameter(param_node)
                && let Some(name_node) = self.ctx.arena.get(param_data.name)
                && let Some(id_data) = self.ctx.arena.get_identifier(name_node)
            {
                let param_name = self.ctx.arena.resolve_identifier_text(id_data);
                if param_name == sym_name.as_str() {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a symbol is used in the right-hand side of a binary chain
    /// containing `location`.
    fn is_symbol_in_binary_chain_rhs(
        &self,
        sym_id: tsz_binder::SymbolId,
        location: NodeIndex,
        top_cond: NodeIndex,
    ) -> bool {
        let Some(top_node) = self.ctx.arena.get(top_cond) else {
            return false;
        };

        if top_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }

        let Some(bin) = self.ctx.arena.get_binary_expr(top_node) else {
            return false;
        };

        let op = bin.operator_token;
        if op != SyntaxKind::AmpersandAmpersandToken as u16
            && op != SyntaxKind::BarBarToken as u16
            && op != SyntaxKind::QuestionQuestionToken as u16
        {
            return false;
        }

        // If location is in the left subtree, check the right subtree
        if self.span_contains(bin.left, location) && self.is_symbol_in_subtree(sym_id, bin.right) {
            return true;
        }

        // Recurse into nested binary expressions on the left
        let Some(left_node) = self.ctx.arena.get(bin.left) else {
            return false;
        };
        if left_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self.is_symbol_in_binary_chain_rhs(sym_id, location, bin.left);
        }

        false
    }

    /// Check if `outer` node's span contains `inner` node.
    fn span_contains(&self, outer: NodeIndex, inner: NodeIndex) -> bool {
        let Some(outer_node) = self.ctx.arena.get(outer) else {
            return false;
        };
        let Some(inner_node) = self.ctx.arena.get(inner) else {
            return false;
        };
        outer_node.pos <= inner_node.pos && inner_node.end <= outer_node.end
    }

    /// Check if an expression contains a type assertion (as, satisfies, or angle-bracket cast),
    /// unwrapping through parenthesized expressions.
    fn expression_has_type_assertion(&self, expr: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::AS_EXPRESSION
            | syntax_kind_ext::SATISFIES_EXPRESSION
            | syntax_kind_ext::TYPE_ASSERTION => true,
            syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.expression_has_type_assertion(paren.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
