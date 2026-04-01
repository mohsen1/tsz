//! Core implementation for solver diagnostics.
//!
//! Contains tracer pattern, failure reasons, lazy diagnostics, diagnostic codes,
//! and core diagnostic data types. Re-exported from the parent `diagnostics` module.

use crate::types::{TypeId, Visibility};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

// =============================================================================
// Tracer Pattern: Zero-Cost Diagnostic Abstraction
// =============================================================================

/// A trait for tracing subtype check failures.
///
/// This trait enables the same subtype checking logic to be used for both
/// fast boolean checks and detailed diagnostics.
///
/// The key insight is that failure reasons are constructed lazily via a closure,
/// so fast-path implementations can skip the allocation entirely while diagnostic
/// implementations collect detailed information.
///
/// # Example
///
/// ```text
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
    /// the failure reason, allowing fast-path implementations to skip
    /// allocation entirely.
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

/// Object-safe version of `SubtypeTracer` for dynamic dispatch.
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

/// Blanket implementation for all `SubtypeTracer` types.
impl<T: SubtypeTracer> DynSubtypeTracer for T {
    fn on_mismatch_dyn(&mut self, reason: SubtypeFailureReason) -> bool {
        self.on_mismatch(|| reason)
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
    /// Multiple required properties are missing in the source type (TS2739).
    MissingProperties {
        property_names: Vec<Atom>,
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Property types are incompatible.
    PropertyTypeMismatch {
        property_name: Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested_reason: Option<Box<Self>>,
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
        nested_reason: Option<Box<Self>>,
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
    /// Missing index signature.
    MissingIndexSignature { index_kind: &'static str },
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
    /// Readonly type assigned to mutable target (TS4104).
    /// Emitted when a readonly array/tuple is assigned to a mutable array/tuple.
    ReadonlyToMutableAssignment {
        source_type: TypeId,
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
/// (`TypeId`, `SymbolId`, etc.) and only format when rendering.
#[derive(Clone, Debug)]
pub enum DiagnosticArg {
    /// A type reference (will be formatted via `TypeFormatter`)
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
    pub related: Vec<Self>,
}

impl PendingDiagnostic {
    /// Create a new pending error diagnostic.
    pub const fn error(code: u32, args: Vec<DiagnosticArg>) -> Self {
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
    pub fn with_related(mut self, related: Self) -> Self {
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
        Self {
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
        Self {
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
    pub use dc::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE as ARG_NOT_ASSIGNABLE;
    pub use dc::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY as READONLY_PROPERTY;
    pub use dc::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE as EXCESS_PROPERTY;
    pub use dc::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE as PROPERTY_MISSING;
    pub use dc::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS as PROPERTY_VISIBILITY_MISMATCH;
    pub use dc::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A as PROPERTY_NOMINAL_MISMATCH;
    pub use dc::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE as READONLY_TO_MUTABLE;
    pub use dc::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE as NO_COMMON_PROPERTIES;
    pub use dc::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE as MISSING_PROPERTIES;
    pub use dc::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE as TYPE_NOT_ASSIGNABLE;

    pub use dc::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE as MISSING_INDEX_SIGNATURE;
    pub use dc::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE as PROPERTY_TYPE_MISMATCH;

    // Function/call errors
    pub use dc::CANNOT_FIND_NAME;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB as CANNOT_FIND_NAME_TARGET_LIB;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2 as CANNOT_FIND_NAME_DOM;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2 as CANNOT_FIND_NAME_TEST_RUNNER;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE_2 as CANNOT_FIND_NAME_BUN;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2 as CANNOT_FIND_NAME_NODE;
    pub use dc::EXPECTED_ARGUMENTS_BUT_GOT as ARG_COUNT_MISMATCH;
    pub use dc::PROPERTY_DOES_NOT_EXIST_ON_TYPE as PROPERTY_NOT_EXIST;
    pub use dc::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN as PROPERTY_NOT_EXIST_DID_YOU_MEAN;
    pub use dc::THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE as THIS_TYPE_MISMATCH;
    pub use dc::THIS_EXPRESSION_IS_NOT_CALLABLE as NOT_CALLABLE;

    // Null/undefined errors

    // Implicit any errors (7xxx series)
    // These aliases intentionally keep the solver's public diagnostics API stable
    // even when the underlying `tsz-common` names are not referenced from this
    // crate.
    #[allow(unused_imports)]
    pub use dc::FUNCTION_EXPRESSION_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN as IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION;
    #[allow(unused_imports)]
    pub use dc::MEMBER_IMPLICITLY_HAS_AN_TYPE as IMPLICIT_ANY_MEMBER;
    #[allow(unused_imports)]
    pub use dc::PARAMETER_IMPLICITLY_HAS_AN_TYPE as IMPLICIT_ANY_PARAMETER;
    #[allow(unused_imports)]
    pub use dc::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE as IMPLICIT_ANY_RETURN;
}

/// Map well-known names to their specialized "cannot find name" diagnostic codes.
///
/// TypeScript emits different error codes for well-known globals that are missing
/// because they require specific type definitions or target library changes:
/// - Node.js globals (require, process, Buffer, etc.) → TS2591
/// - Test runner globals (describe, it, test, etc.) → TS2582
/// - Target library types (Promise, Symbol, Map, etc.) → TS2583
/// - DOM globals (document, console) → TS2584
pub(crate) fn cannot_find_name_code(name: &str) -> u32 {
    match name {
        // Node.js globals → TS2591
        "require" | "exports" | "module" | "process" | "Buffer" | "__filename" | "__dirname" => {
            codes::CANNOT_FIND_NAME_NODE
        }
        // Test runner globals → TS2582
        "describe" | "suite" | "it" | "test" => codes::CANNOT_FIND_NAME_TEST_RUNNER,
        // Target library types → TS2583
        "Promise" | "Symbol" | "Map" | "Set" | "Reflect" | "Iterator" | "AsyncIterator"
        | "SharedArrayBuffer" => codes::CANNOT_FIND_NAME_TARGET_LIB,
        // DOM globals → TS2584
        "document" | "console" => codes::CANNOT_FIND_NAME_DOM,
        // Bun globals → TS2868
        "Bun" => codes::CANNOT_FIND_NAME_BUN,
        // Everything else → TS2304
        _ => codes::CANNOT_FIND_NAME,
    }
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
    tsz_common::diagnostics::get_message_template(code).unwrap_or("Unknown diagnostic")
}

// =============================================================================
// Pending Diagnostic Builder (LAZY)
// =============================================================================

/// Builder for creating lazy pending diagnostics.
///
/// This builder creates `PendingDiagnostic` instances that defer expensive
/// string formatting until rendering time.
pub struct PendingDiagnosticBuilder;

// =============================================================================
// SubtypeFailureReason to PendingDiagnostic Conversion
// =============================================================================

impl SubtypeFailureReason {
    /// Return the primary diagnostic code for this failure reason.
    ///
    /// This is the single source of truth for mapping `SubtypeFailureReason` variants
    /// to diagnostic codes. Both the solver's `to_diagnostic` and the checker's
    /// `render_failure_reason` should use this to stay in sync.
    pub const fn diagnostic_code(&self) -> u32 {
        match self {
            Self::MissingProperty { .. } | Self::OptionalPropertyRequired { .. } => {
                codes::PROPERTY_MISSING
            }
            Self::MissingProperties { .. } => codes::MISSING_PROPERTIES,
            Self::PropertyTypeMismatch { .. } => codes::PROPERTY_TYPE_MISMATCH,
            Self::ReadonlyPropertyMismatch { .. } => codes::READONLY_PROPERTY,
            Self::PropertyVisibilityMismatch { .. } => codes::PROPERTY_VISIBILITY_MISMATCH,
            Self::PropertyNominalMismatch { .. } => codes::PROPERTY_NOMINAL_MISMATCH,
            Self::ReturnTypeMismatch { .. }
            | Self::ParameterTypeMismatch { .. }
            | Self::TupleElementMismatch { .. }
            | Self::TupleElementTypeMismatch { .. }
            | Self::ArrayElementMismatch { .. }
            | Self::IndexSignatureMismatch { .. }
            | Self::MissingIndexSignature { .. }
            | Self::NoUnionMemberMatches { .. }
            | Self::NoIntersectionMemberMatches { .. }
            | Self::TypeMismatch { .. }
            | Self::IntrinsicTypeMismatch { .. }
            | Self::LiteralTypeMismatch { .. }
            | Self::ErrorType { .. }
            | Self::RecursionLimitExceeded
            | Self::ParameterCountMismatch { .. }
            | Self::TooManyParameters { .. } => codes::TYPE_NOT_ASSIGNABLE,
            Self::NoCommonProperties { .. } => codes::NO_COMMON_PROPERTIES,
            Self::ExcessProperty { .. } => codes::EXCESS_PROPERTY,
            Self::ReadonlyToMutableAssignment { .. } => codes::READONLY_TO_MUTABLE,
        }
    }

    /// Convert this failure reason to a `PendingDiagnostic`.
    ///
    /// This is the "explain slow" path - called only when we need to report
    /// an error and want a detailed message about why the type check failed.
    pub fn to_diagnostic(&self, source: TypeId, target: TypeId) -> PendingDiagnostic {
        match self {
            Self::MissingProperty {
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

            Self::MissingProperties {
                property_names: _,
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::MISSING_PROPERTIES,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            Self::PropertyTypeMismatch {
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

                // Add elaboration: Types of property 'x' are incompatible (TS2326)
                let elaboration = PendingDiagnostic::error(
                    codes::PROPERTY_TYPE_MISMATCH,
                    vec![(*property_name).into()],
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

            Self::OptionalPropertyRequired { property_name } => {
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

            Self::ReadonlyPropertyMismatch { property_name } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::READONLY_PROPERTY,
                vec![(*property_name).into()],
            )),

            Self::PropertyVisibilityMismatch {
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
                        format!("{source_visibility:?}").into(),
                        format!("{target_visibility:?}").into(),
                    ],
                ))
            }

            Self::PropertyNominalMismatch { property_name } => {
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

            Self::ReturnTypeMismatch {
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

            Self::ParameterTypeMismatch {
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

            Self::TooManyParameters {
                source_count: _,
                target_count: _,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            ),

            Self::TupleElementMismatch {
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

            Self::TupleElementTypeMismatch {
                index: _,
                source_element,
                target_element,
            }
            | Self::ArrayElementMismatch {
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

            Self::IndexSignatureMismatch {
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

            Self::MissingIndexSignature { index_kind } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::MISSING_INDEX_SIGNATURE,
                vec![index_kind.to_string().into(), source.into()],
            )),

            Self::NoUnionMemberMatches {
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

            Self::NoIntersectionMemberMatches {
                source_type,
                target_type,
            }
            | Self::TypeMismatch {
                source_type,
                target_type,
            }
            | Self::IntrinsicTypeMismatch {
                source_type,
                target_type,
            }
            | Self::LiteralTypeMismatch {
                source_type,
                target_type,
            }
            | Self::ErrorType {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            Self::NoCommonProperties {
                source_type,
                target_type,
            } => PendingDiagnostic::error(
                codes::NO_COMMON_PROPERTIES,
                vec![(*source_type).into(), (*target_type).into()],
            ),

            Self::RecursionLimitExceeded => {
                // Recursion limit - use the source/target from the call site
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
            }

            Self::ParameterCountMismatch {
                source_count: _,
                target_count: _,
            } => {
                // Parameter count mismatch
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
            }

            Self::ExcessProperty {
                property_name,
                target_type,
            } => {
                // TS2353: Object literal may only specify known properties
                PendingDiagnostic::error(
                    codes::EXCESS_PROPERTY,
                    vec![(*property_name).into(), (*target_type).into()],
                )
            }
            Self::ReadonlyToMutableAssignment {
                source_type,
                target_type,
            } => {
                // TS4104: The type 'X' is 'readonly' and cannot be assigned to the mutable type 'Y'.
                PendingDiagnostic::error(
                    codes::READONLY_TO_MUTABLE,
                    vec![(*source_type).into(), (*target_type).into()],
                )
            }
        }
    }
}

impl PendingDiagnosticBuilder {
    /// Create an "Argument not assignable" pending diagnostic.
    pub fn argument_not_assignable(arg_type: TypeId, param_type: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::ARG_NOT_ASSIGNABLE,
            vec![arg_type.into(), param_type.into()],
        )
    }

    /// Create an "Expected N arguments but got M" pending diagnostic.
    /// When `expected_min < expected_max`, formats as "Expected 1-3 arguments".
    pub fn argument_count_mismatch(
        expected_min: usize,
        expected_max: usize,
        got: usize,
    ) -> PendingDiagnostic {
        let expected_arg: DiagnosticArg = if expected_min < expected_max {
            DiagnosticArg::String(format!("{expected_min}-{expected_max}").into())
        } else {
            expected_max.into()
        };
        PendingDiagnostic::error(codes::ARG_COUNT_MISMATCH, vec![expected_arg, got.into()])
    }

    /// Create a "This type mismatch" pending diagnostic.
    pub fn this_type_mismatch(expected_this: TypeId, actual_this: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::THIS_TYPE_MISMATCH,
            vec![actual_this.into(), expected_this.into()],
        )
    }
}

#[cfg(test)]
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

    /// Create a "Cannot find name" pending diagnostic.
    pub fn cannot_find_name(name: &str) -> PendingDiagnostic {
        let code = cannot_find_name_code(name);
        PendingDiagnostic::error(code, vec![name.into()])
    }

    /// Create a "Type is not callable" pending diagnostic.
    pub fn not_callable(type_id: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(codes::NOT_CALLABLE, vec![type_id.into()])
    }

    pub fn this_type_mismatch(expected_this: TypeId, actual_this: TypeId) -> PendingDiagnostic {
        PendingDiagnostic::error(
            codes::THIS_TYPE_MISMATCH,
            vec![actual_this.into(), expected_this.into()],
        )
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

#[cfg(test)]
use crate::types::*;

#[cfg(test)]
#[path = "../../tests/diagnostics_tests.rs"]
mod tests;
