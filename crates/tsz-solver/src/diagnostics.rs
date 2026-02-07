//! Diagnostic generation for the solver.
//!
//! This module provides error message generation for type checking failures.
//! It produces human-readable diagnostics with source locations and context.
//!
//! ## Architecture: Lazy Diagnostics
//!
//! To avoid expensive string formatting during type checking (especially in tentative
//! contexts like overload resolution), this module uses a two-phase approach:
//!
//! 1. **Collection**: Store structured data in `PendingDiagnostic` with `DiagnosticArg` values
//! 2. **Rendering**: Format strings lazily only when displaying to the user
//!
//! This prevents calling `type_to_string()` thousands of times for errors that are
//! discarded during overload resolution.
//!
//! ## Tracer Pattern (Zero-Cost Abstraction)
//!
//! The tracer pattern allows the same subtype checking logic to be used for both
//! fast boolean checks and detailed diagnostic generation, eliminating logic drift.
//!
//! - **FastTracer**: Zero-cost abstraction that compiles to a simple boolean return
//! - **DiagnosticTracer**: Collects detailed `SubtypeFailureReason` for error messages

use crate::TypeDatabase;
use crate::TypeFormatter;
use crate::def::DefinitionStore;
use crate::types::*;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

#[cfg(test)]
use crate::TypeInterner;

// =============================================================================
// Tracer Pattern: Zero-Cost Diagnostic Abstraction
// =============================================================================

/// A trait for tracing subtype check failures.
///
/// This trait enables the same subtype checking logic to be used for both
/// fast boolean checks (via `FastTracer`) and detailed diagnostics (via `DiagnosticTracer`).
///
/// The key insight is that failure reasons are constructed lazily via a closure,
/// so `FastTracer` can skip the allocation entirely while `DiagnosticTracer` collects
/// detailed information.
///
/// # Example
///
/// ```rust
/// fn check_subtype_with_tracer<T: SubtypeTracer>(
///     source: TypeId,
///     target: TypeId,
///     tracer: &mut T,
/// ) -> bool {
///     if source == target {
///         return true;
///     }
///     tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch { source, target })
/// }
/// ```
pub trait SubtypeTracer {
    /// Called when a subtype mismatch is detected.
    ///
    /// The `reason` closure is only called if the tracer needs to collect
    /// the failure reason. This allows `FastTracer` to skip the allocation
    /// entirely while `DiagnosticTracer` can collect detailed information.
    ///
    /// # Returns
    ///
    /// - `true` if checking should continue (for collecting more nested failures)
    /// - `false` if checking should stop immediately (fast path)
    ///
    /// # Type Parameters
    ///
    /// The `reason` parameter is a closure that constructs the failure reason.
    /// It's wrapped in `FnOnce` so it's only called when needed.
    fn on_mismatch(&mut self, reason: impl FnOnce() -> SubtypeFailureReason) -> bool;
}

/// Object-safe version of SubtypeTracer for dynamic dispatch.
///
/// This trait is dyn-compatible and can be used as `&mut dyn DynSubtypeTracer`.
/// It has a simpler signature that takes the reason directly rather than a closure.
pub trait DynSubtypeTracer {
    /// Called when a subtype mismatch is detected.
    ///
    /// Unlike `SubtypeTracer::on_mismatch`, this takes the reason directly
    /// rather than a closure. This makes it object-safe (dyn-compatible).
    ///
    /// # Returns
    ///
    /// - `true` if checking should continue (for collecting more nested failures)
    /// - `false` if checking should stop immediately (fast path)
    fn on_mismatch_dyn(&mut self, reason: SubtypeFailureReason) -> bool;
}

/// Blanket implementation for all SubtypeTracer types.
impl<T: SubtypeTracer> DynSubtypeTracer for T {
    fn on_mismatch_dyn(&mut self, reason: SubtypeFailureReason) -> bool {
        self.on_mismatch(|| reason)
    }
}

/// Fast tracer that returns immediately on mismatch (zero-cost abstraction).
///
/// This tracer is used for fast subtype checks where we only care about the
/// boolean result. The `#[inline(always)]` attribute ensures that this compiles
/// to the same code as a simple `return false` statement with no runtime overhead.
///
/// # Zero-Cost Abstraction
///
/// ```rust
/// // With FastTracer, this compiles to:
/// // if condition { return false; }
/// if !tracer.on_mismatch(|| reason) { return false; }
/// ```
///
/// The closure is never called, so no allocations occur.
#[derive(Clone, Copy, Debug)]
pub struct FastTracer;

impl SubtypeTracer for FastTracer {
    /// Always return `false` to stop checking immediately.
    ///
    /// The `reason` closure is never called, so no `SubtypeFailureReason` is constructed.
    /// This is the zero-cost path - the compiler will optimize this to a simple boolean return.
    #[inline(always)]
    fn on_mismatch(&mut self, _reason: impl FnOnce() -> SubtypeFailureReason) -> bool {
        false
    }
}

/// Diagnostic tracer that collects detailed failure reasons.
///
/// This tracer is used when we need to generate detailed error messages.
/// It collects the first `SubtypeFailureReason` encountered and stops checking.
///
/// # Example
///
/// ```rust
/// let mut tracer = DiagnosticTracer::new();
/// check_subtype_with_tracer(source, target, &mut tracer);
/// if let Some(reason) = tracer.take_failure() {
///     // Generate error message from reason
/// }
/// ```
#[derive(Debug)]
pub struct DiagnosticTracer {
    /// The first failure reason encountered (if any).
    failure: Option<SubtypeFailureReason>,
}

impl DiagnosticTracer {
    /// Create a new diagnostic tracer.
    pub fn new() -> Self {
        Self { failure: None }
    }

    /// Take the collected failure reason, leaving `None` in its place.
    pub fn take_failure(&mut self) -> Option<SubtypeFailureReason> {
        self.failure.take()
    }

    /// Get a reference to the collected failure reason (if any).
    pub fn get_failure(&self) -> Option<&SubtypeFailureReason> {
        self.failure.as_ref()
    }

    /// Check if any failure was collected.
    pub fn has_failure(&self) -> bool {
        self.failure.is_some()
    }
}

impl Default for DiagnosticTracer {
    fn default() -> Self {
        Self::new()
    }
}

impl SubtypeTracer for DiagnosticTracer {
    /// Collect the failure reason and stop checking.
    ///
    /// The `reason` closure is called to construct the detailed failure reason,
    /// which is stored for later use in error message generation.
    ///
    /// Returns `false` to stop checking after collecting the first failure.
    /// This matches the semantics of `FastTracer` while collecting diagnostics.
    #[inline]
    fn on_mismatch(&mut self, reason: impl FnOnce() -> SubtypeFailureReason) -> bool {
        // Only collect the first failure (subsequent failures are nested details)
        if self.failure.is_none() {
            self.failure = Some(reason());
        }
        false
    }
}

/// Detailed reason for a subtype check failure.
///
/// This enum captures all the different ways a subtype check can fail,
/// with enough detail to generate helpful error messages.
///
/// # Nesting
///
/// Some variants include `nested_reason` to capture failures in nested types.
/// For example, a property type mismatch might include why the property types
/// themselves don't match.
#[derive(Clone, Debug, PartialEq)]
pub enum SubtypeFailureReason {
    /// A required property is missing in the source type.
    MissingProperty {
        property_name: Atom,
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Property types are incompatible.
    PropertyTypeMismatch {
        property_name: Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested_reason: Option<Box<SubtypeFailureReason>>,
    },
    /// Optional property cannot satisfy required property.
    OptionalPropertyRequired { property_name: Atom },
    /// Readonly property cannot satisfy mutable property.
    ReadonlyPropertyMismatch { property_name: Atom },
    /// Property visibility mismatch (private/protected vs public).
    PropertyVisibilityMismatch {
        property_name: Atom,
        source_visibility: Visibility,
        target_visibility: Visibility,
    },
    /// Property nominal mismatch (separate declarations of private/protected property).
    PropertyNominalMismatch { property_name: Atom },
    /// Return types are incompatible.
    ReturnTypeMismatch {
        source_return: TypeId,
        target_return: TypeId,
        nested_reason: Option<Box<SubtypeFailureReason>>,
    },
    /// Parameter types are incompatible.
    ParameterTypeMismatch {
        param_index: usize,
        source_param: TypeId,
        target_param: TypeId,
    },
    /// Too many parameters in source.
    TooManyParameters {
        source_count: usize,
        target_count: usize,
    },
    /// Tuple element count mismatch.
    TupleElementMismatch {
        source_count: usize,
        target_count: usize,
    },
    /// Tuple element type mismatch.
    TupleElementTypeMismatch {
        index: usize,
        source_element: TypeId,
        target_element: TypeId,
    },
    /// Array element type mismatch.
    ArrayElementMismatch {
        source_element: TypeId,
        target_element: TypeId,
    },
    /// Index signature value type mismatch.
    IndexSignatureMismatch {
        index_kind: &'static str, // "string" or "number"
        source_value_type: TypeId,
        target_value_type: TypeId,
    },
    /// No union member matches.
    NoUnionMemberMatches {
        source_type: TypeId,
        target_union_members: Vec<TypeId>,
    },
    /// No intersection member matches target (intersection requires at least one member).
    NoIntersectionMemberMatches {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// No overlapping properties for weak type target.
    NoCommonProperties {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Generic type mismatch (no more specific reason).
    TypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Intrinsic type mismatch (e.g., string vs number).
    IntrinsicTypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Literal type mismatch (e.g., "hello" vs "world" or "hello" vs 42).
    LiteralTypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Error type encountered - indicates unresolved type that should not be silently compatible.
    ErrorType {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Recursion limit exceeded during type checking.
    RecursionLimitExceeded,
    /// Parameter count mismatch.
    ParameterCountMismatch {
        source_count: usize,
        target_count: usize,
    },
    /// Excess property in object literal assignment (TS2353).
    ExcessProperty {
        property_name: Atom,
        target_type: TypeId,
    },
}

/// Diagnostic severity level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Suggestion,
    Message,
}

// =============================================================================
// Lazy Diagnostic Arguments
// =============================================================================

/// Argument for a diagnostic message template.
///
/// Instead of eagerly formatting types to strings, we store the raw data
/// (TypeId, SymbolId, etc.) and only format when rendering.
#[derive(Clone, Debug)]
pub enum DiagnosticArg {
    /// A type reference (will be formatted via TypeFormatter)
    Type(TypeId),
    /// A symbol reference (will be looked up by name)
    Symbol(SymbolId),
    /// An interned string
    Atom(Atom),
    /// A plain string
    String(Arc<str>),
    /// A number
    Number(usize),
}

macro_rules! impl_from_diagnostic_arg {
    ($($source:ty => $variant:ident),* $(,)?) => {
        $(impl From<$source> for DiagnosticArg {
            fn from(v: $source) -> Self { Self::$variant(v) }
        })*
    };
}

impl_from_diagnostic_arg! {
    TypeId   => Type,
    SymbolId => Symbol,
    Atom     => Atom,
    usize    => Number,
}

impl From<&str> for DiagnosticArg {
    fn from(s: &str) -> Self {
        Self::String(s.into())
    }
}

impl From<String> for DiagnosticArg {
    fn from(s: String) -> Self {
        Self::String(s.into())
    }
}

/// A pending diagnostic that hasn't been rendered yet.
///
/// This stores the structured data needed to generate an error message,
/// but defers the expensive string formatting until rendering time.
#[derive(Clone, Debug)]
pub struct PendingDiagnostic {
    /// Diagnostic code (e.g., 2322 for type not assignable)
    pub code: u32,
    /// Arguments for the message template
    pub args: Vec<DiagnosticArg>,
    /// Primary source location
    pub span: Option<SourceSpan>,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Related information (additional locations)
    pub related: Vec<PendingDiagnostic>,
}

impl PendingDiagnostic {
    /// Create a new pending error diagnostic.
    pub fn error(code: u32, args: Vec<DiagnosticArg>) -> Self {
        Self {
            code,
            args,
            span: None,
            severity: DiagnosticSeverity::Error,
            related: Vec::new(),
        }
    }

    /// Attach a source span to this diagnostic.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Add related information.
    pub fn with_related(mut self, related: PendingDiagnostic) -> Self {
        self.related.push(related);
        self
    }
}

/// A source location span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    /// Start position (byte offset)
    pub start: u32,
    /// Length in bytes
    pub length: u32,
    /// File path or name
    pub file: Arc<str>,
}

impl SourceSpan {
    pub fn new(file: impl Into<Arc<str>>, start: u32, length: u32) -> Self {
        SourceSpan {
            start,
            length,
            file: file.into(),
        }
    }
}

/// Related diagnostic information (e.g., "see declaration here").
#[derive(Clone, Debug)]
pub struct RelatedInformation {
    pub span: SourceSpan,
    pub message: String,
}

/// A type checking diagnostic.
#[derive(Clone, Debug)]
pub struct TypeDiagnostic {
    /// The main error message
    pub message: String,
    /// Diagnostic code (e.g., 2322 for "Type X is not assignable to type Y")
    pub code: u32,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Primary source location
    pub span: Option<SourceSpan>,
    /// Related information (additional locations)
    pub related: Vec<RelatedInformation>,
}

impl TypeDiagnostic {
    /// Create a new error diagnostic.
    pub fn error(message: impl Into<String>, code: u32) -> Self {
        TypeDiagnostic {
            message: message.into(),
            code,
            severity: DiagnosticSeverity::Error,
            span: None,
            related: Vec::new(),
        }
    }

    /// Add a source span to this diagnostic.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Add related information.
    pub fn with_related(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.related.push(RelatedInformation {
            span,
            message: message.into(),
        });
        self
    }
}

// =============================================================================
// Diagnostic Codes (matching TypeScript's)
// =============================================================================

/// TypeScript diagnostic codes for type errors.
///
/// These are re-exported from `tsz_common::diagnostics::diagnostic_codes` with
/// short aliases for ergonomic use within the solver. The canonical definitions
/// live in `tsz-common` to maintain a single source of truth.
pub mod codes {
    use tsz_common::diagnostics::diagnostic_codes as dc;

    // Type assignability
    pub use dc::ARGUMENT_NOT_ASSIGNABLE_TO_PARAMETER as ARG_NOT_ASSIGNABLE;
    pub use dc::CANNOT_ASSIGN_TO_READONLY_PROPERTY as READONLY_PROPERTY;
    pub use dc::EXCESS_PROPERTY_CHECK as EXCESS_PROPERTY;
    pub use dc::MEMBER_IS_NOT_ACCESSIBLE as PROPERTY_VISIBILITY_MISMATCH;
    pub use dc::NO_COMMON_PROPERTIES;
    pub use dc::PROPERTY_DOES_NOT_EXIST_ON_TYPE as PROPERTY_NOT_EXIST;
    pub use dc::PROPERTY_MISSING_IN_TYPE as PROPERTY_MISSING;
    pub use dc::PROPERTY_NOMINAL_MISMATCH;
    pub use dc::THIS_CONTEXT_MISMATCH;
    pub use dc::TYPE_NOT_ASSIGNABLE_TO_TYPE as TYPE_NOT_ASSIGNABLE;
    pub use dc::TYPE_PARAMETER_CONSTRAINT_NOT_SATISFIED as CONSTRAINT_NOT_SATISFIED;

    /// Same code as TYPE_NOT_ASSIGNABLE (TS2322) — used for nested property elaboration.
    pub const NESTED_TYPE_MISMATCH: u32 = dc::TYPE_NOT_ASSIGNABLE_TO_TYPE;

    // Function/call errors
    pub use dc::ASYNC_RETURN_TYPE_MUST_BE_PROMISE as NEVER_ASYNC_RETURN;
    pub use dc::CANNOT_FIND_NAME;
    pub use dc::CANNOT_INVOKE_NON_FUNCTION as NOT_CALLABLE;
    pub use dc::EXPECTED_ARGUMENTS as ARG_COUNT_MISMATCH;

    // Null/undefined errors
    pub use dc::OBJECT_IS_OF_TYPE_UNKNOWN as OBJECT_IS_UNKNOWN;
    pub use dc::OBJECT_IS_POSSIBLY_NULL as OBJECT_POSSIBLY_NULL;
    pub use dc::OBJECT_IS_POSSIBLY_UNDEFINED as OBJECT_POSSIBLY_UNDEFINED;

    // Implicit any errors (7xxx series)
    pub use dc::IMPLICIT_ANY;
    pub use dc::IMPLICIT_ANY_MEMBER;
    pub use dc::IMPLICIT_ANY_PARAMETER;
    pub use dc::IMPLICIT_ANY_RETURN;
    pub use dc::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION;

    // Type instantiation errors
    pub use dc::TYPE_INSTANTIATION_EXCESSIVELY_DEEP as INSTANTIATION_TOO_DEEP;
}

// =============================================================================
// Message Templates
// =============================================================================

/// Get the message template for a diagnostic code.
///
/// Templates use {0}, {1}, etc. as placeholders for arguments.
/// Message strings are sourced from `tsz_common::diagnostics::diagnostic_messages`
/// to maintain a single source of truth with the checker.
pub fn get_message_template(code: u32) -> &'static str {
    use tsz_common::diagnostics::diagnostic_messages as dm;

    match code {
        codes::TYPE_NOT_ASSIGNABLE => dm::TYPE_NOT_ASSIGNABLE,
        codes::ARG_NOT_ASSIGNABLE => dm::ARGUMENT_NOT_ASSIGNABLE,
        codes::PROPERTY_MISSING => dm::PROPERTY_MISSING_BUT_REQUIRED,
        codes::PROPERTY_NOT_EXIST => dm::PROPERTY_DOES_NOT_EXIST,
        codes::NO_COMMON_PROPERTIES => dm::NO_COMMON_PROPERTIES,
        codes::READONLY_PROPERTY => dm::CANNOT_ASSIGN_READONLY,
        codes::PROPERTY_VISIBILITY_MISMATCH => dm::PROPERTY_VISIBILITY_MISMATCH,
        codes::PROPERTY_NOMINAL_MISMATCH => dm::PROPERTY_NOMINAL_MISMATCH,
        codes::CONSTRAINT_NOT_SATISFIED => dm::TYPE_NOT_SATISFY_CONSTRAINT,
        codes::THIS_CONTEXT_MISMATCH => dm::THIS_CONTEXT_MISMATCH,
        codes::NEVER_ASYNC_RETURN => dm::ASYNC_RETURN_TYPE_MUST_BE_PROMISE,
        codes::CANNOT_FIND_NAME => dm::CANNOT_FIND_NAME,
        codes::NOT_CALLABLE => dm::CANNOT_INVOKE_EXPRESSION,
        codes::ARG_COUNT_MISMATCH => dm::EXPECTED_ARGUMENTS,
        codes::OBJECT_POSSIBLY_UNDEFINED => dm::OBJECT_POSSIBLY_UNDEFINED,
        codes::OBJECT_POSSIBLY_NULL => dm::OBJECT_POSSIBLY_NULL,
        codes::OBJECT_IS_UNKNOWN => dm::OBJECT_IS_OF_TYPE_UNKNOWN,
        codes::EXCESS_PROPERTY => dm::EXCESS_PROPERTY,
        codes::IMPLICIT_ANY => dm::VARIABLE_IMPLICIT_ANY,
        codes::IMPLICIT_ANY_PARAMETER => dm::PARAMETER_IMPLICIT_ANY,
        codes::IMPLICIT_ANY_MEMBER => dm::MEMBER_IMPLICIT_ANY,
        codes::IMPLICIT_ANY_RETURN => dm::IMPLICIT_ANY_RETURN,
        codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION => {
            dm::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION
        }
        codes::INSTANTIATION_TOO_DEEP => dm::TYPE_INSTANTIATION_EXCESSIVELY_DEEP,
        _ => "Unknown diagnostic",
    }
}

// =============================================================================
// Type Formatting
// =============================================================================

// TypeFormatter is now in format.rs

// =============================================================================
// Diagnostic Builder
// =============================================================================

/// Builder for creating type error diagnostics.
pub struct DiagnosticBuilder<'a> {
    #[allow(dead_code)]
    interner: &'a dyn TypeDatabase,
    formatter: TypeFormatter<'a>,
}

impl<'a> DiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        DiagnosticBuilder {
            interner,
            formatter: TypeFormatter::new(interner),
        }
    }

    /// Create a diagnostic builder with access to symbol names.
    ///
    /// This prevents "Ref(N)" fallback strings in diagnostic messages by
    /// resolving symbol references to their actual names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a tsz_binder::SymbolArena,
    ) -> Self {
        DiagnosticBuilder {
            interner,
            formatter: TypeFormatter::with_symbols(interner, symbol_arena),
        }
    }

    /// Create a diagnostic builder with access to definition store.
    ///
    /// This prevents "Lazy(N)" fallback strings in diagnostic messages by
    /// resolving DefIds to their type names.
    pub fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.formatter = self.formatter.with_def_store(def_store);
        self
    }

    /// Create a "Type X is not assignable to type Y" diagnostic.
    pub fn type_not_assignable(&mut self, source: TypeId, target: TypeId) -> TypeDiagnostic {
        let source_str = self.formatter.format(source);
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Type '{}' is not assignable to type '{}'.",
                source_str, target_str
            ),
            codes::TYPE_NOT_ASSIGNABLE,
        )
    }

    /// Create a "Property X is missing in type Y" diagnostic.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
    ) -> TypeDiagnostic {
        let source_str = self.formatter.format(source);
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Property '{}' is missing in type '{}' but required in type '{}'.",
                prop_name, source_str, target_str
            ),
            codes::PROPERTY_MISSING,
        )
    }

    /// Create a "Property X does not exist on type Y" diagnostic.
    pub fn property_not_exist(&mut self, prop_name: &str, type_id: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!(
                "Property '{}' does not exist on type '{}'.",
                prop_name, type_str
            ),
            codes::PROPERTY_NOT_EXIST,
        )
    }

    /// Create an "Argument not assignable" diagnostic.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> TypeDiagnostic {
        let arg_str = self.formatter.format(arg_type);
        let param_str = self.formatter.format(param_type);
        TypeDiagnostic::error(
            format!(
                "Argument of type '{}' is not assignable to parameter of type '{}'.",
                arg_str, param_str
            ),
            codes::ARG_NOT_ASSIGNABLE,
        )
    }

    /// Create a "Cannot find name" diagnostic.
    pub fn cannot_find_name(&mut self, name: &str) -> TypeDiagnostic {
        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors (e.g., ",", ";", "(") that were
        // added to the AST for error recovery. The parse error should have
        // already been emitted (e.g., TS1136 "Property assignment expected").
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );

        if is_obviously_invalid {
            // Return a dummy diagnostic with empty message that will be ignored
            return TypeDiagnostic::error("", 0);
        }

        TypeDiagnostic::error(
            format!("Cannot find name '{}'.", name),
            codes::CANNOT_FIND_NAME,
        )
    }

    /// Create a "Type X is not callable" diagnostic.
    pub fn not_callable(&mut self, type_id: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!("Type '{}' has no call signatures.", type_str),
            codes::NOT_CALLABLE,
        )
    }

    /// Create an "Expected N arguments but got M" diagnostic.
    pub fn argument_count_mismatch(&mut self, expected: usize, got: usize) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Expected {} arguments, but got {}.", expected, got),
            codes::ARG_COUNT_MISMATCH,
        )
    }

    /// Create a "Cannot assign to readonly property" diagnostic.
    pub fn readonly_property(&mut self, prop_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!(
                "Cannot assign to '{}' because it is a read-only property.",
                prop_name
            ),
            codes::READONLY_PROPERTY,
        )
    }

    /// Create an "Excess property" diagnostic.
    pub fn excess_property(&mut self, prop_name: &str, target: TypeId) -> TypeDiagnostic {
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Object literal may only specify known properties, and '{}' does not exist in type '{}'.",
                prop_name, target_str
            ),
            codes::EXCESS_PROPERTY,
        )
    }

    // =========================================================================
    // Implicit Any Diagnostics (TS7006, TS7008, TS7010, TS7011)
    // =========================================================================

    /// Create a "Parameter implicitly has an 'any' type" diagnostic (TS7006).
    ///
    /// This is emitted when noImplicitAny is enabled and a function parameter
    /// has no type annotation and no contextual type.
    pub fn implicit_any_parameter(&mut self, param_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Parameter '{}' implicitly has an 'any' type.", param_name),
            codes::IMPLICIT_ANY_PARAMETER,
        )
    }

    /// Create a "Parameter implicitly has a specific type" diagnostic (TS7006 variant).
    ///
    /// This is used when the implicit type is known to be something other than 'any',
    /// such as when a rest parameter implicitly has 'any[]'.
    pub fn implicit_any_parameter_with_type(
        &mut self,
        param_name: &str,
        implicit_type: TypeId,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(implicit_type);
        TypeDiagnostic::error(
            format!(
                "Parameter '{}' implicitly has an '{}' type.",
                param_name, type_str
            ),
            codes::IMPLICIT_ANY_PARAMETER,
        )
    }

    /// Create a "Member implicitly has an 'any' type" diagnostic (TS7008).
    ///
    /// This is emitted when noImplicitAny is enabled and a class/interface member
    /// has no type annotation.
    pub fn implicit_any_member(&mut self, member_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Member '{}' implicitly has an 'any' type.", member_name),
            codes::IMPLICIT_ANY_MEMBER,
        )
    }

    /// Create a "Variable implicitly has an 'any' type" diagnostic (TS7005).
    ///
    /// This is emitted when noImplicitAny is enabled and a variable declaration
    /// has no type annotation and the inferred type is 'any'.
    pub fn implicit_any_variable(&mut self, var_name: &str, var_type: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(var_type);
        TypeDiagnostic::error(
            format!(
                "Variable '{}' implicitly has an '{}' type.",
                var_name, type_str
            ),
            codes::IMPLICIT_ANY,
        )
    }

    /// Create an "implicitly has an 'any' return type" diagnostic (TS7010).
    ///
    /// This is emitted when noImplicitAny is enabled and a function declaration
    /// has no return type annotation and returns 'any'.
    pub fn implicit_any_return(&mut self, func_name: &str, return_type: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(return_type);
        TypeDiagnostic::error(
            format!(
                "'{}', which lacks return-type annotation, implicitly has an '{}' return type.",
                func_name, type_str
            ),
            codes::IMPLICIT_ANY_RETURN,
        )
    }

    /// Create a "Function expression implicitly has an 'any' return type" diagnostic (TS7011).
    ///
    /// This is emitted when noImplicitAny is enabled and a function expression
    /// has no return type annotation and returns 'any'.
    pub fn implicit_any_return_function_expression(
        &mut self,
        return_type: TypeId,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(return_type);
        TypeDiagnostic::error(
            format!(
                "Function expression, which lacks return-type annotation, implicitly has an '{}' return type.",
                type_str
            ),
            codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
        )
    }
}

// =============================================================================
// Pending Diagnostic Builder (LAZY)
// =============================================================================

/// Builder for creating lazy pending diagnostics.
///
/// This builder creates PendingDiagnostic instances that defer expensive
/// string formatting until rendering time.
pub struct PendingDiagnosticBuilder;

// =============================================================================
// SubtypeFailureReason to PendingDiagnostic Conversion
// =============================================================================

impl SubtypeFailureReason {
    /// Convert this failure reason to a PendingDiagnostic.
    ///
    /// This is the "explain slow" path - called only when we need to report
    /// an error and want a detailed message about why the type check failed.
    pub fn to_diagnostic(&self, source: TypeId, target: TypeId) -> PendingDiagnostic {
        match self {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::PROPERTY_MISSING,
                vec![
                    (*property_name).into(),
                    (*source_type).into(),
                    (*target_type).into(),
                ],
            ),

            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => {
                // Main error: Type not assignable
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );

                // Add elaboration: Types of property 'x' are incompatible
                let elaboration = PendingDiagnostic::error(
                    codes::NESTED_TYPE_MISMATCH,
                    vec![
                        (*property_name).into(),
                        (*source_property_type).into(),
                        (*target_property_type).into(),
                    ],
                );
                diag = diag.with_related(elaboration);

                // If there's a nested reason, add that too
                if let Some(nested) = nested_reason {
                    let nested_diag =
                        nested.to_diagnostic(*source_property_type, *target_property_type);
                    diag = diag.with_related(nested_diag);
                }

                diag
            }

            SubtypeFailureReason::OptionalPropertyRequired { property_name } => {
                // This is a specific case of type not assignable
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::PROPERTY_MISSING, // Close enough - property is "missing" because it's optional
                    vec![(*property_name).into(), source.into(), target.into()],
                ))
            }

            SubtypeFailureReason::ReadonlyPropertyMismatch { property_name } => {
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::READONLY_PROPERTY,
                    vec![(*property_name).into()],
                ))
            }

            SubtypeFailureReason::PropertyVisibilityMismatch {
                property_name,
                source_visibility,
                target_visibility,
            } => {
                // TS2341/TS2445: Property 'x' is private in type 'A' but not in type 'B'
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::PROPERTY_VISIBILITY_MISMATCH,
                    vec![
                        (*property_name).into(),
                        format!("{:?}", source_visibility).into(),
                        format!("{:?}", target_visibility).into(),
                    ],
                ))
            }

            SubtypeFailureReason::PropertyNominalMismatch { property_name } => {
                // TS2446: Types have separate declarations of a private property 'x'
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::PROPERTY_NOMINAL_MISMATCH,
                    vec![(*property_name).into()],
                ))
            }

            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => {
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );

                // Add: Type 'X' is not assignable to type 'Y' (for return types)
                let return_diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_return).into(), (*target_return).into()],
                );
                diag = diag.with_related(return_diag);

                if let Some(nested) = nested_reason {
                    let nested_diag = nested.to_diagnostic(*source_return, *target_return);
                    diag = diag.with_related(nested_diag);
                }

                diag
            }

            SubtypeFailureReason::ParameterTypeMismatch {
                param_index: _,
                source_param,
                target_param,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_param).into(), (*target_param).into()],
            )),

            SubtypeFailureReason::TooManyParameters {
                source_count,
                target_count,
            } => PendingDiagnostic::error(
                codes::ARG_COUNT_MISMATCH,
                vec![(*target_count).into(), (*source_count).into()],
            ),

            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::ARG_COUNT_MISMATCH,
                vec![(*target_count).into(), (*source_count).into()],
            )),

            SubtypeFailureReason::TupleElementTypeMismatch {
                index: _,
                source_element,
                target_element,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_element).into(), (*target_element).into()],
            )),

            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_element).into(), (*target_element).into()],
            )),

            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind: _,
                source_value_type,
                target_value_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_value_type).into(), (*target_value_type).into()],
            )),

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members,
            } => {
                const UNION_MEMBER_DIAGNOSTIC_LIMIT: usize = 3;
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_type).into(), target.into()],
                );
                for member in target_union_members
                    .iter()
                    .take(UNION_MEMBER_DIAGNOSTIC_LIMIT)
                {
                    diag.related.push(PendingDiagnostic::error(
                        codes::TYPE_NOT_ASSIGNABLE,
                        vec![(*source_type).into(), (*member).into()],
                    ));
                }
                diag
            }

            SubtypeFailureReason::NoIntersectionMemberMatches {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::NoCommonProperties {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::NO_COMMON_PROPERTIES,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::IntrinsicTypeMismatch {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::LiteralTypeMismatch {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            SubtypeFailureReason::ErrorType {
                source_type,
                target_type,
            } => {
                // Error types indicate unresolved types that should trigger TS2322.
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_type).into(), (*target_type).into()],
                )
            }

            SubtypeFailureReason::RecursionLimitExceeded => {
                // Recursion limit - use the source/target from the call site
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
            }

            SubtypeFailureReason::ParameterCountMismatch {
                source_count: _,
                target_count: _,
            } => {
                // Parameter count mismatch
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
            }

            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type,
            } => {
                // TS2353: Object literal may only specify known properties
                PendingDiagnostic::error(
                    codes::EXCESS_PROPERTY,
                    vec![(*property_name).into(), (*target_type).into()],
                )
            }
        }
    }
}

impl PendingDiagnosticBuilder {
    /// Create a "Type X is not assignable to type Y" pending diagnostic.
    pub fn type_not_assignable(source: TypeId, target: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::TYPE_NOT_ASSIGNABLE,
            vec![source.into(), target.into()],
        )
    }

    /// Create a "Property X is missing" pending diagnostic.
    pub fn property_missing(prop_name: &str, source: TypeId, target: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::PROPERTY_MISSING,
            vec![prop_name.into(), source.into(), target.into()],
        )
    }

    /// Create a "Property X does not exist" pending diagnostic.
    pub fn property_not_exist(prop_name: &str, type_id: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::PROPERTY_NOT_EXIST,
            vec![prop_name.into(), type_id.into()],
        )
    }

    /// Create an "Argument not assignable" pending diagnostic.
    pub fn argument_not_assignable(arg_type: TypeId, param_type: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::ARG_NOT_ASSIGNABLE,
            vec![arg_type.into(), param_type.into()],
        )
    }

    /// Create a "Cannot find name" pending diagnostic.
    pub fn cannot_find_name(name: &str) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::CANNOT_FIND_NAME, vec![name.into()])
    }

    /// Create a "Type is not callable" pending diagnostic.
    pub fn not_callable(type_id: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::NOT_CALLABLE, vec![type_id.into()])
    }

    /// Create an "Expected N arguments but got M" pending diagnostic.
    pub fn argument_count_mismatch(expected: usize, got: usize) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::ARG_COUNT_MISMATCH, vec![expected.into(), got.into()])
    }

    /// Create a "Cannot assign to readonly property" pending diagnostic.
    pub fn readonly_property(prop_name: &str) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::READONLY_PROPERTY, vec![prop_name.into()])
    }

    /// Create an "Excess property" pending diagnostic.
    pub fn excess_property(prop_name: &str, target: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::EXCESS_PROPERTY,
            vec![prop_name.into(), target.into()],
        )
    }
}

// =============================================================================
// Spanned Diagnostic Builder
// =============================================================================

/// A diagnostic builder that automatically attaches source spans.
///
/// This builder wraps `DiagnosticBuilder` and requires a file name and
/// position information for each diagnostic.
pub struct SpannedDiagnosticBuilder<'a> {
    builder: DiagnosticBuilder<'a>,
    file: Arc<str>,
}

impl<'a> SpannedDiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase, file: impl Into<Arc<str>>) -> Self {
        SpannedDiagnosticBuilder {
            builder: DiagnosticBuilder::new(interner),
            file: file.into(),
        }
    }

    /// Create a spanned diagnostic builder with access to symbol names.
    ///
    /// This prevents "Ref(N)" fallback strings in diagnostic messages by
    /// resolving symbol references to their actual names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a tsz_binder::SymbolArena,
        file: impl Into<Arc<str>>,
    ) -> Self {
        SpannedDiagnosticBuilder {
            builder: DiagnosticBuilder::with_symbols(interner, symbol_arena),
            file: file.into(),
        }
    }

    /// Add access to definition store for DefId name resolution.
    ///
    /// This prevents "Lazy(N)" fallback strings in diagnostic messages by
    /// resolving DefIds to their type names.
    pub fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.builder = self.builder.with_def_store(def_store);
        self
    }

    /// Create a span for this file.
    pub fn span(&self, start: u32, length: u32) -> SourceSpan {
        SourceSpan::new(self.file.clone(), start, length)
    }

    /// Create a "Type X is not assignable to type Y" diagnostic with span.
    pub fn type_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .type_not_assignable(source, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X is missing" diagnostic with span.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_missing(prop_name, source, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X does not exist" diagnostic with span.
    pub fn property_not_exist(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_not_exist(prop_name, type_id)
            .with_span(self.span(start, length))
    }

    /// Create an "Argument not assignable" diagnostic with span.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_not_assignable(arg_type, param_type)
            .with_span(self.span(start, length))
    }

    /// Create a "Cannot find name" diagnostic with span.
    pub fn cannot_find_name(&mut self, name: &str, start: u32, length: u32) -> TypeDiagnostic {
        self.builder
            .cannot_find_name(name)
            .with_span(self.span(start, length))
    }

    /// Create an "Expected N arguments" diagnostic with span.
    pub fn argument_count_mismatch(
        &mut self,
        expected: usize,
        got: usize,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_count_mismatch(expected, got)
            .with_span(self.span(start, length))
    }

    /// Create a "Type is not callable" diagnostic with span.
    pub fn not_callable(&mut self, type_id: TypeId, start: u32, length: u32) -> TypeDiagnostic {
        self.builder
            .not_callable(type_id)
            .with_span(self.span(start, length))
    }

    /// Create an "Excess property" diagnostic with span.
    pub fn excess_property(
        &mut self,
        prop_name: &str,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .excess_property(prop_name, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Cannot assign to readonly property" diagnostic with span.
    pub fn readonly_property(
        &mut self,
        prop_name: &str,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .readonly_property(prop_name)
            .with_span(self.span(start, length))
    }

    /// Add a related location to an existing diagnostic.
    pub fn add_related(
        &self,
        diag: TypeDiagnostic,
        message: impl Into<String>,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        diag.with_related(self.span(start, length), message)
    }
}

// =============================================================================
// Diagnostic Conversion
// =============================================================================

/// Convert a solver TypeDiagnostic to a checker Diagnostic.
///
/// This allows the solver's diagnostic infrastructure to integrate
/// with the existing checker diagnostic system.
impl TypeDiagnostic {
    /// Convert to a checker::Diagnostic.
    ///
    /// Uses the provided file_name if no span is present.
    pub fn to_checker_diagnostic(&self, default_file: &str) -> tsz_common::diagnostics::Diagnostic {
        use tsz_common::diagnostics::{
            Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
        };

        let (file, start, length) = if let Some(ref span) = self.span {
            (span.file.to_string(), span.start, span.length)
        } else {
            (default_file.to_string(), 0, 0)
        };

        let category = match self.severity {
            DiagnosticSeverity::Error => DiagnosticCategory::Error,
            DiagnosticSeverity::Warning => DiagnosticCategory::Warning,
            DiagnosticSeverity::Suggestion => DiagnosticCategory::Suggestion,
            DiagnosticSeverity::Message => DiagnosticCategory::Message,
        };

        let related_information: Vec<DiagnosticRelatedInformation> = self
            .related
            .iter()
            .map(|rel| DiagnosticRelatedInformation {
                file: rel.span.file.to_string(),
                start: rel.span.start,
                length: rel.span.length,
                message_text: rel.message.clone(),
                category: DiagnosticCategory::Message,
                code: 0,
            })
            .collect();

        Diagnostic {
            file,
            start,
            length,
            message_text: self.message.clone(),
            category,
            code: self.code,
            related_information,
        }
    }
}

// =============================================================================
// Source Location Tracker
// =============================================================================

/// Tracks source locations for AST nodes during type checking.
///
/// This struct provides a convenient way to associate type checking
/// operations with their source locations for diagnostic generation.
#[derive(Clone)]
pub struct SourceLocation {
    /// File name
    pub file: Arc<str>,
    /// Start position (byte offset)
    pub start: u32,
    /// End position (byte offset)
    pub end: u32,
}

impl SourceLocation {
    pub fn new(file: impl Into<Arc<str>>, start: u32, end: u32) -> Self {
        SourceLocation {
            file: file.into(),
            start,
            end,
        }
    }

    /// Get the length of this location.
    pub fn length(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Convert to a SourceSpan.
    pub fn to_span(&self) -> SourceSpan {
        SourceSpan::new(self.file.clone(), self.start, self.length())
    }
}

/// A diagnostic collector that accumulates diagnostics with source tracking.
pub struct DiagnosticCollector<'a> {
    interner: &'a dyn TypeDatabase,
    file: Arc<str>,
    diagnostics: Vec<TypeDiagnostic>,
}

impl<'a> DiagnosticCollector<'a> {
    pub fn new(interner: &'a dyn TypeDatabase, file: impl Into<Arc<str>>) -> Self {
        DiagnosticCollector {
            interner,
            file: file.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Get the collected diagnostics.
    pub fn diagnostics(&self) -> &[TypeDiagnostic] {
        &self.diagnostics
    }

    /// Take the collected diagnostics.
    pub fn take_diagnostics(&mut self) -> Vec<TypeDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Report a type not assignable error.
    pub fn type_not_assignable(&mut self, source: TypeId, target: TypeId, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .type_not_assignable(source, target)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a property missing error.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .property_missing(prop_name, source, target)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a property not exist error.
    pub fn property_not_exist(&mut self, prop_name: &str, type_id: TypeId, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .property_not_exist(prop_name, type_id)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report an argument not assignable error.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_not_assignable(arg_type, param_type)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a cannot find name error.
    pub fn cannot_find_name(&mut self, name: &str, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder.cannot_find_name(name).with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report an argument count mismatch error.
    pub fn argument_count_mismatch(&mut self, expected: usize, got: usize, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_count_mismatch(expected, got)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Convert all collected diagnostics to checker diagnostics.
    pub fn to_checker_diagnostics(&self) -> Vec<tsz_common::diagnostics::Diagnostic> {
        self.diagnostics
            .iter()
            .map(|d| d.to_checker_diagnostic(&self.file))
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/diagnostics_tests.rs"]
mod tests;
