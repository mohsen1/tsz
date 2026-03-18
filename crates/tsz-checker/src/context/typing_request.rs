//! Explicit typing request objects for expression type computation.
//!
//! Replaces ambient save/restore of `ctx.contextual_type`,
//! `ctx.contextual_type_is_assertion`, and `ctx.skip_flow_narrowing`
//! with explicit request objects threaded through checker entry points.
//!
//! # Migration status
//!
//! Files fully migrated to request-first APIs should no longer assign
//! `ctx.contextual_type`, `ctx.contextual_type_is_assertion`, or
//! `ctx.skip_flow_narrowing` directly. See architecture contract tests
//! in `tests/architecture_contract_tests.rs` for enforcement.

use tsz_solver::TypeId;

// ---------------------------------------------------------------------------
// FlowIntent — replaces `skip_flow_narrowing: bool`
// ---------------------------------------------------------------------------

/// Describes whether flow narrowing should apply to an expression check.
///
/// Replaces the boolean `ctx.skip_flow_narrowing` with an explicit enum
/// so callers express *intent* rather than toggling a global flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowIntent {
    /// Normal read: apply flow narrowing (the common case).
    #[default]
    Read,
    /// Write / assignment target: skip flow narrowing so the declared
    /// (pre-narrowed) type is used.  E.g. `foo[x] = 1` after
    /// `if (foo[x] === undefined)` needs `number | undefined`, not `undefined`.
    Write,
}

impl FlowIntent {
    /// Returns `true` when flow narrowing should be skipped.
    #[inline]
    pub const fn skip_flow_narrowing(self) -> bool {
        matches!(self, FlowIntent::Write)
    }
}

// ---------------------------------------------------------------------------
// ContextualOrigin — replaces `contextual_type_is_assertion: bool`
// ---------------------------------------------------------------------------

/// Describes where a contextual type comes from.
///
/// Replaces the boolean `ctx.contextual_type_is_assertion` with an enum
/// that distinguishes normal contextual typing from assertion contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextualOrigin {
    /// Normal contextual typing (variable declaration, assignment, etc.).
    /// Function body return types are checked against the contextual type.
    #[default]
    Normal,
    /// Type assertion (`as T`, `<T>expr`, JSDoc `@type {T}`).
    /// Function body return types are NOT checked against this contextual type —
    /// only TS2352 is emitted at the assertion site.
    Assertion,
}

impl ContextualOrigin {
    /// Returns `true` when the contextual type comes from a type assertion.
    #[inline]
    pub const fn is_assertion(self) -> bool {
        matches!(self, ContextualOrigin::Assertion)
    }
}

// ---------------------------------------------------------------------------
// TypingRequest — the request object
// ---------------------------------------------------------------------------

/// Explicit request for expression type computation.
///
/// Carries all context that was previously smuggled through mutable globals
/// on `CheckerContext`. Callers build a request and pass it to
/// `get_type_of_node_with_request` instead of saving/restoring fields.
///
/// # Examples
///
/// ```ignore
/// // Before (ambient state):
/// let prev = self.ctx.contextual_type;
/// self.ctx.contextual_type = Some(expected);
/// let ty = self.get_type_of_node(expr);
/// self.ctx.contextual_type = prev;
///
/// // After (request-first):
/// let req = TypingRequest::with_contextual_type(expected);
/// let ty = self.get_type_of_node_with_request(expr, &req);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypingRequest {
    /// The contextual type to use for this expression, if any.
    pub contextual_type: Option<TypeId>,
    /// Where the contextual type originated (normal vs assertion context).
    pub origin: ContextualOrigin,
    /// Whether flow narrowing should apply.
    pub flow: FlowIntent,
}

impl Default for TypingRequest {
    fn default() -> Self {
        Self::NONE
    }
}

impl TypingRequest {
    /// No contextual type, normal origin, read flow. The "do nothing special" request.
    pub const NONE: Self = Self {
        contextual_type: None,
        origin: ContextualOrigin::Normal,
        flow: FlowIntent::Read,
    };

    /// Request with only a contextual type (normal origin, read flow).
    #[inline]
    pub const fn with_contextual_type(ty: TypeId) -> Self {
        Self {
            contextual_type: Some(ty),
            origin: ContextualOrigin::Normal,
            flow: FlowIntent::Read,
        }
    }

    /// Request for an assertion context (`as T`, JSDoc `@type`).
    #[inline]
    pub const fn for_assertion(ty: TypeId) -> Self {
        Self {
            contextual_type: Some(ty),
            origin: ContextualOrigin::Assertion,
            flow: FlowIntent::Read,
        }
    }

    /// Request that skips flow narrowing (write / assignment target context).
    #[inline]
    pub const fn for_write_context() -> Self {
        Self {
            contextual_type: None,
            origin: ContextualOrigin::Normal,
            flow: FlowIntent::Write,
        }
    }

    /// Request with contextual type in a write context.
    #[inline]
    pub const fn with_contextual_type_write(ty: TypeId) -> Self {
        Self {
            contextual_type: Some(ty),
            origin: ContextualOrigin::Normal,
            flow: FlowIntent::Write,
        }
    }

    /// Builder: set the contextual type.
    #[inline]
    pub const fn contextual(mut self, ty: TypeId) -> Self {
        self.contextual_type = Some(ty);
        self
    }

    /// Builder: mark as assertion origin.
    #[inline]
    pub const fn assertion(mut self) -> Self {
        self.origin = ContextualOrigin::Assertion;
        self
    }

    /// Builder: set write (skip-flow) intent.
    #[inline]
    pub const fn write(mut self) -> Self {
        self.flow = FlowIntent::Write;
        self
    }

    /// Returns true if this request has no contextual type and uses defaults.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.contextual_type.is_none()
            && matches!(self.origin, ContextualOrigin::Normal)
            && matches!(self.flow, FlowIntent::Read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::TypeId;

    #[test]
    fn default_request_is_none() {
        let req = TypingRequest::default();
        assert_eq!(req, TypingRequest::NONE);
        assert!(req.is_empty());
        assert!(!req.flow.skip_flow_narrowing());
        assert!(!req.origin.is_assertion());
    }

    #[test]
    fn with_contextual_type_sets_type() {
        let req = TypingRequest::with_contextual_type(TypeId::STRING);
        assert_eq!(req.contextual_type, Some(TypeId::STRING));
        assert!(!req.origin.is_assertion());
        assert!(!req.flow.skip_flow_narrowing());
        assert!(!req.is_empty());
    }

    #[test]
    fn for_assertion_sets_origin() {
        let req = TypingRequest::for_assertion(TypeId::NUMBER);
        assert_eq!(req.contextual_type, Some(TypeId::NUMBER));
        assert!(req.origin.is_assertion());
        assert!(!req.flow.skip_flow_narrowing());
    }

    #[test]
    fn for_write_context_skips_flow() {
        let req = TypingRequest::for_write_context();
        assert!(req.flow.skip_flow_narrowing());
        assert!(req.contextual_type.is_none());
    }

    #[test]
    fn builder_chain() {
        let req = TypingRequest::NONE
            .contextual(TypeId::BOOLEAN)
            .assertion()
            .write();
        assert_eq!(req.contextual_type, Some(TypeId::BOOLEAN));
        assert!(req.origin.is_assertion());
        assert!(req.flow.skip_flow_narrowing());
    }
}
