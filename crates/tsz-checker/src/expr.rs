//! Expression Type Checking
//!
//! This module handles type inference and checking for expressions.
//! It follows the "Check Fast, Explain Slow" pattern where we first
//! infer types, then use the solver to explain any failures.
//!
//! ## Integration with `CheckerState`
//!
//! `ExpressionChecker` serves as the primary dispatcher for expression types.
//! Simple expressions are handled directly here, while complex expressions
//! that need full `CheckerState` context are reported as
//! [`ExprCheckResult::Delegate`] so `CheckerState::compute_type_of_node`
//! can handle them.
//!
//! Delegation is control flow, not a type — it indicates "this expression
//! is not handled here, ask `CheckerState`". It must never appear as a
//! `TypeId` in `ctx.node_types` or `ctx.request_node_types`.
//!
//! ### Expressions handled directly:
//! - Simple literals without contextual typing (null)
//! - typeof expressions (always string)
//! - void expressions (always undefined)
//! - Parenthesized expressions (pass through in TS files)
//!
//! ### Expressions delegated to `CheckerState`:
//! - Literals with contextual typing (numeric, string, boolean, template)
//! - Identifiers, this, super (need symbol resolution)
//! - Binary expressions (need operator overloading, narrowing)
//! - Call/new expressions (need signature resolution)
//! - Property/element access (need object type resolution)
//! - Function/arrow expressions (need signature building)
//! - Object/array literals (need contextual typing)
//! - Type assertions (as/satisfies) (need type node resolution)
//! - Conditional expressions (need union type building)
//! - Await expressions (need Promise unwrapping)

use super::context::{CheckerContext, RequestCacheKey, TypingRequest};

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use tsz_solver::recursion::{DepthCounter, RecursionProfile};

/// Result of dispatching an expression through [`ExpressionChecker`].
///
/// Either the expression was handled directly here and produced a real
/// [`TypeId`], or the expression requires full [`CheckerState`] context
/// and must be delegated.
///
/// [`CheckerState`]: crate::state::CheckerState
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExprCheckResult {
    /// Expression was fully resolved to a concrete [`TypeId`].
    ///
    /// Only values of this variant may be written into `ctx.node_types`
    /// or `ctx.request_node_types`.
    Type(TypeId),
    /// Expression must be handled by [`CheckerState`] — this is control
    /// flow, not a type, and must never reach a type cache.
    ///
    /// [`CheckerState`]: crate::state::CheckerState
    Delegate,
}

impl ExprCheckResult {
    /// Return the concrete type if fully resolved, else `None`.
    #[inline]
    pub const fn type_id(self) -> Option<TypeId> {
        match self {
            Self::Type(ty) => Some(ty),
            Self::Delegate => None,
        }
    }

    /// Return `true` when the result represents a delegation marker.
    #[inline]
    pub const fn is_delegate(self) -> bool {
        matches!(self, Self::Delegate)
    }
}

/// Expression type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All type inference for expressions goes through this checker.
pub struct ExpressionChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
    /// Recursion depth counter for stack overflow protection.
    depth: DepthCounter,
}

impl<'a, 'ctx> ExpressionChecker<'a, 'ctx> {
    const fn is_audited_contextual_request_cache_kind(kind: u16) -> bool {
        kind == SyntaxKind::NullKeyword as u16
            || kind == syntax_kind_ext::TYPE_OF_EXPRESSION
            || kind == syntax_kind_ext::VOID_EXPRESSION
    }

    /// Create a new expression checker with a mutable context reference.
    pub const fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self {
            ctx,
            depth: DepthCounter::with_profile(RecursionProfile::ExpressionCheck),
        }
    }

    /// Dispatch an expression, caching only concrete results.
    ///
    /// Returns [`ExprCheckResult::Type`] for expressions handled directly,
    /// [`ExprCheckResult::Delegate`] for expressions that must be resolved
    /// by [`CheckerState`]. Delegation markers are never written to any
    /// cache.
    ///
    /// [`CheckerState`]: crate::state::CheckerState
    pub fn check(&mut self, idx: NodeIndex) -> ExprCheckResult {
        self.check_with_context(idx, None)
    }

    /// Dispatch an expression with a contextual type hint.
    ///
    /// Contextual types enable downward inference where the expected type
    /// influences the inferred type. For example:
    /// - `const x: string = expr` — `expr` is checked with context `string`
    /// - `const f: (x: number) => void = (x) => {}` — `x` is inferred as `number`
    ///
    /// Only real types ([`ExprCheckResult::Type`]) are ever cached in
    /// `ctx.node_types`/`ctx.request_node_types`. Delegation results
    /// bypass all caches, so repeated calls on delegated nodes always
    /// return [`ExprCheckResult::Delegate`] rather than a cached sentinel.
    pub fn check_with_context(
        &mut self,
        idx: NodeIndex,
        context_type: Option<TypeId>,
    ) -> ExprCheckResult {
        // Stack overflow protection
        if !self.depth.enter() {
            return ExprCheckResult::Type(TypeId::ERROR);
        }

        let result = if let Some(ctx_type) = context_type {
            let request = TypingRequest::with_contextual_type(ctx_type);
            let cache_key = self.ctx.arena.get(idx).and_then(|node| {
                Self::is_audited_contextual_request_cache_kind(node.kind)
                    .then(|| RequestCacheKey::from_request(&request))
                    .flatten()
            });
            if let Some(key) = cache_key
                && let Some(&cached) = self.ctx.request_node_types.get(&(idx.0, key))
            {
                self.ctx.request_cache_counters.request_cache_hits += 1;
                self.depth.leave();
                return ExprCheckResult::Type(cached);
            }
            if cache_key.is_some() {
                self.ctx.request_cache_counters.request_cache_misses += 1;
            }

            let result = self.compute_type_with_context(idx, ctx_type);
            match result {
                ExprCheckResult::Type(ty) => {
                    if let Some(key) = cache_key {
                        debug_assert_ne!(
                            ty,
                            TypeId::DELEGATE,
                            "DELEGATE sentinel must never be inserted into request_node_types"
                        );
                        self.ctx.request_node_types.insert((idx.0, key), ty);
                    } else {
                        // Keep unaudited contextual direct checks uncached until
                        // their request dependencies are made explicit and reviewed.
                        self.ctx.request_cache_counters.contextual_cache_bypasses += 1;
                    }
                }
                ExprCheckResult::Delegate => {
                    self.ctx.request_cache_counters.contextual_cache_bypasses += 1;
                }
            }
            result
        } else {
            // Check cache first for non-contextual checks
            if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                self.depth.leave();
                return ExprCheckResult::Type(cached);
            }

            // Compute; cache only concrete results. Delegation is control
            // flow, not a type, so the cache contract forbids storing it.
            let result = self.compute_type(idx);
            if let ExprCheckResult::Type(ty) = result {
                debug_assert_ne!(
                    ty,
                    TypeId::DELEGATE,
                    "DELEGATE sentinel must never be inserted into node_types"
                );
                self.ctx.node_types.insert(idx.0, ty);
            }
            result
        };

        self.depth.leave();
        result
    }

    /// Try to compute an expression's type without touching caches.
    ///
    /// Returns [`ExprCheckResult::Delegate`] for expressions that need
    /// [`CheckerState`] context for proper resolution.
    ///
    /// [`CheckerState`]: crate::state::CheckerState
    pub fn try_compute_expr_type(&mut self, idx: NodeIndex) -> ExprCheckResult {
        self.compute_type_impl(idx, None)
    }

    /// Try to compute an expression's type under a contextual type, without caching.
    pub fn try_compute_expr_type_with_context(
        &mut self,
        idx: NodeIndex,
        context_type: Option<TypeId>,
    ) -> ExprCheckResult {
        self.compute_type_impl(idx, context_type)
    }

    /// Back-compat alias for [`Self::try_compute_expr_type`].
    ///
    /// Prefer the `try_compute_expr_type*` names — they advertise that the
    /// result may be a delegation marker.
    #[inline]
    pub fn compute_type_uncached(&mut self, idx: NodeIndex) -> ExprCheckResult {
        self.try_compute_expr_type(idx)
    }

    /// Back-compat alias for [`Self::try_compute_expr_type_with_context`].
    #[inline]
    pub fn compute_type_uncached_with_context(
        &mut self,
        idx: NodeIndex,
        context_type: Option<TypeId>,
    ) -> ExprCheckResult {
        self.try_compute_expr_type_with_context(idx, context_type)
    }

    /// Compute the type of an expression with contextual typing (no caching).
    ///
    /// This is called when a contextual type is available (e.g., from variable
    /// declarations, assignments, function parameters). The contextual type
    /// influences how the expression is inferred.
    fn compute_type_with_context(
        &mut self,
        idx: NodeIndex,
        context_type: TypeId,
    ) -> ExprCheckResult {
        self.compute_type_impl(idx, Some(context_type))
    }

    /// Compute the type of an expression (internal, not cached).
    fn compute_type(&mut self, idx: NodeIndex) -> ExprCheckResult {
        self.compute_type_impl(idx, None)
    }

    /// Core implementation for computing expression types.
    ///
    /// Returns [`ExprCheckResult::Delegate`] for complex expressions that
    /// need [`CheckerState`].
    ///
    /// # Parameters
    /// - `idx`: The node index to check
    /// - `context_type`: Optional contextual type hint for downward inference
    ///
    /// [`CheckerState`]: crate::state::CheckerState
    fn compute_type_impl(
        &mut self,
        idx: NodeIndex,
        _context_type: Option<TypeId>,
    ) -> ExprCheckResult {
        let Some(node) = self.ctx.arena.get(idx) else {
            // Return ERROR for missing arena nodes (typically cross-file references)
            // to suppress cascading false diagnostics like TS18046.
            return ExprCheckResult::Type(TypeId::ERROR);
        };

        match node.kind {
            // =====================================================================
            // Simple expressions handled directly
            // =====================================================================

            // Null literal - always TypeId::NULL (context doesn't affect null)
            k if k == SyntaxKind::NullKeyword as u16 => ExprCheckResult::Type(TypeId::NULL),

            // typeof expression always returns string (context doesn't affect typeof)
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => ExprCheckResult::Type(TypeId::STRING),

            // void expression always returns undefined (context doesn't affect void)
            k if k == syntax_kind_ext::VOID_EXPRESSION => ExprCheckResult::Type(TypeId::UNDEFINED),

            // Parenthesized expression - pass through context to inner expression.
            // In JS files, parenthesized expressions may carry JSDoc type casts
            // (e.g., `/** @type {T} */(expr)`) that need full CheckerState handling.
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if self.ctx.is_js_file() {
                    // Delegate to CheckerState which handles JSDoc @type and @satisfies
                    return ExprCheckResult::Delegate;
                }
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    // Check if expression is missing (parse error: empty parentheses)
                    if paren.expression.is_none() {
                        // Parse error - return ERROR to suppress cascading errors
                        return ExprCheckResult::Type(TypeId::ERROR);
                    }
                    // Recursively check inner expression with same context
                    self.compute_type_impl(paren.expression, _context_type)
                } else {
                    // Let CheckerState handle malformed nodes
                    ExprCheckResult::Delegate
                }
            }

            // =====================================================================
            // Literals with contextual typing - delegate to CheckerState.
            // These need contextual typing analysis to decide between literal types
            // (e.g., `42` as literal) vs widened types (e.g., `number`).
            // =====================================================================
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                ExprCheckResult::Delegate
            }

            // =====================================================================
            // Expressions requiring symbol resolution
            // =====================================================================
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 =>
            {
                ExprCheckResult::Delegate
            }

            // =====================================================================
            // Complex expressions
            // =====================================================================
            k if k == syntax_kind_ext::BINARY_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::VARIABLE_DECLARATION
                || k == syntax_kind_ext::FUNCTION_DECLARATION =>
            {
                ExprCheckResult::Delegate
            }

            // =====================================================================
            // Type nodes - delegate to CheckerState.
            // These are not expressions but may be passed through get_type_of_node.
            // =====================================================================
            k if k == syntax_kind_ext::TYPE_REFERENCE
                || k == syntax_kind_ext::UNION_TYPE
                || k == syntax_kind_ext::INTERSECTION_TYPE
                || k == syntax_kind_ext::ARRAY_TYPE
                || k == syntax_kind_ext::TYPE_OPERATOR
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::TYPE_QUERY
                || k == syntax_kind_ext::QUALIFIED_NAME =>
            {
                ExprCheckResult::Delegate
            }

            // Type keywords - delegate for consistency.
            k if k == SyntaxKind::NumberKeyword as u16
                || k == SyntaxKind::StringKeyword as u16
                || k == SyntaxKind::BooleanKeyword as u16
                || k == SyntaxKind::VoidKeyword as u16
                || k == SyntaxKind::AnyKeyword as u16
                || k == SyntaxKind::NeverKeyword as u16
                || k == SyntaxKind::UnknownKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::ObjectKeyword as u16
                || k == SyntaxKind::BigIntKeyword as u16
                || k == SyntaxKind::SymbolKeyword as u16 =>
            {
                ExprCheckResult::Delegate
            }

            // JSX elements
            k if k == syntax_kind_ext::JSX_ELEMENT
                || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_FRAGMENT =>
            {
                ExprCheckResult::Delegate
            }

            // =====================================================================
            // Default - unknown node type, delegate to CheckerState
            // =====================================================================
            _ => ExprCheckResult::Delegate,
        }
    }

    /// Get the context reference (for read-only access).
    pub const fn context(&self) -> &CheckerContext<'ctx> {
        self.ctx
    }
}

#[cfg(test)]
#[path = "../tests/expr.rs"]
mod tests;
