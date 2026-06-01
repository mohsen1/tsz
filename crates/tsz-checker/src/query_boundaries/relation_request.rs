//! Checker-facing request vocabulary for relation queries.
//!
//! `RelationRequest` carries the semantic question and checker-side policy
//! descriptors into the assignability boundary. The checker owns request
//! construction and diagnostic anchors; solver relation code owns the actual
//! compatibility decision.

use tsz_solver::TypeId;

/// The kind of relation being checked. Different kinds imply different
/// default policies for freshness, excess properties, and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationKind {
    /// Variable/parameter assignment: `const x: T = expr`
    Assign,
    /// `for...in` initializer target: key type stored into the LHS.
    ForInLhs,
    /// Function call argument: `fn(expr)` where param expects T
    CallArg,
    /// Return statement: `return expr` where function returns T
    Return,
    JsxProps,
    JsxChildren,
    /// Destructuring: `const { a, b } = expr`
    Destructuring,
    /// Rest parameter array compatibility: `function f(...args: T)`
    RestParameter,
    /// Import attribute shape compatibility: `import x from "m" with { ... }`
    ImportAttributes,
    /// Satisfies expression: `expr satisfies T`
    Satisfies,
    /// Bivariant callback assignment where function parameter types are checked bivariantly.
    BivariantCallbacks,
}

/// How excess properties (properties in source not in target) are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExcessPropertyMode {
    /// Skip excess property checking entirely (default for non-fresh sources).
    Skip,
    /// Check and report excess properties (for fresh object literals).
    Check,
    /// Check only explicitly-written properties (for spread expressions).
    CheckExplicitOnly,
}

/// How missing properties (properties in target not in source) are classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingPropertyMode {
    /// Report missing required properties (default).
    Report,
    /// Suppress missing property errors (e.g., for Partial<T> patterns).
    Suppress,
}

/// A structured request for a type relation check.
///
/// Encodes all the policy dimensions that affect how the checker interprets
/// a relation result. The checker builds a request, invokes the boundary,
/// and uses the result + failure info for diagnostics.
#[derive(Debug, Clone)]
pub(crate) struct RelationRequest {
    /// Prepared source type for the relation.
    pub source: TypeId,
    /// Prepared target type for the relation.
    pub target: TypeId,
    /// Diagnostic/tracing context. Currently advisory only.
    pub kind: RelationKind,
    /// Requested excess-property policy. Currently advisory.
    pub excess_property_mode: ExcessPropertyMode,
    /// Requested missing-property policy. Currently advisory.
    pub missing_property_mode: MissingPropertyMode,
    /// Fresh object literal marker. Currently advisory.
    pub source_is_fresh: bool,
    /// Allow targeted erased-signature retry for interface property compatibility.
    pub allow_erased_generic_signature_retry: bool,
}

impl RelationRequest {
    const fn new(source: TypeId, target: TypeId, kind: RelationKind) -> Self {
        Self {
            source,
            target,
            kind,
            excess_property_mode: ExcessPropertyMode::Skip,
            missing_property_mode: MissingPropertyMode::Report,
            source_is_fresh: false,
            allow_erased_generic_signature_retry: false,
        }
    }

    pub(crate) const fn assign(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Assign)
    }
    pub(crate) const fn for_in_lhs(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ForInLhs)
    }
    pub(crate) const fn call_arg(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallArg)
    }

    pub(crate) const fn return_stmt(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Return)
    }

    pub(crate) const fn jsx_props(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsxProps)
    }

    pub(crate) const fn jsx_children(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsxChildren)
    }

    pub(crate) const fn satisfies(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Satisfies)
    }

    pub(crate) const fn destructuring(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Destructuring)
    }

    pub(crate) const fn rest_parameter(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::RestParameter)
    }

    pub(crate) const fn import_attributes(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ImportAttributes)
    }

    pub(crate) const fn bivariant_callbacks(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::BivariantCallbacks)
    }

    /// Mark the source as a fresh object literal, enabling EPC.
    pub(crate) const fn with_fresh_source(mut self) -> Self {
        self.source_is_fresh = true;
        self.excess_property_mode = ExcessPropertyMode::Check;
        self
    }

    /// Mark the source as a spread expression, enabling explicit-only EPC.
    pub(crate) const fn with_spread_source(mut self) -> Self {
        self.excess_property_mode = ExcessPropertyMode::CheckExplicitOnly;
        self
    }

    /// Override excess property mode.
    pub(crate) const fn with_excess_property_mode(mut self, mode: ExcessPropertyMode) -> Self {
        self.excess_property_mode = mode;
        self
    }

    /// Override missing property mode.
    pub(crate) const fn with_missing_property_mode(mut self, mode: MissingPropertyMode) -> Self {
        self.missing_property_mode = mode;
        self
    }

    /// Allow a failed generic-signature inference to retry with erased signatures.
    pub(crate) const fn with_erased_generic_signature_retry(mut self) -> Self {
        self.allow_erased_generic_signature_retry = true;
        self
    }
}
