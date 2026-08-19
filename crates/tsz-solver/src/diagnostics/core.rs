//! Core implementation for solver diagnostics.
//!
//! Contains failure reasons, lazy diagnostics, diagnostic codes,
//! and core diagnostic data types. Re-exported from the parent `diagnostics` module.

use crate::types::{TypeId, TypePredicate, Visibility};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

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
///
/// Pre-classified tuple arity mismatch family (`TS2618`–`TS2621`).
///
/// `tsc` (`tupleTypesRelated` in `checker.ts`) gates a tuple-to-tuple relation
/// on four length comparisons that use the *arity* (total element slots,
/// including a single variadic/rest slot) and *minimum length* (count of
/// required elements) of each side together with whether each side carries a
/// rest element. Each branch reports a distinct message with its own wording,
/// argument count, and diagnostic code, so the failing relation records the
/// resolved family here rather than re-deriving it from raw element counts in
/// the renderer (which cannot distinguish a fixed slot from a variadic slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TupleArity {
    /// `TS2618` — "Source has {0} element(s) but target requires {1}." The
    /// source is a closed tuple too short to satisfy the target's required
    /// elements. Carries `(source_arity, target_min)`.
    SourceTooFew {
        source_arity: usize,
        target_min: usize,
    },
    /// `TS2619` — "Source has {0} element(s) but target allows only {1}." The
    /// target is a closed tuple and the source's minimum length already exceeds
    /// it. Carries `(source_min, target_arity)`.
    SourceTooMany {
        source_min: usize,
        target_arity: usize,
    },
    /// `TS2620` — "Target requires {0} element(s) but source may have fewer."
    /// The target is closed and requires more elements than the (variadic)
    /// source is guaranteed to provide. Carries `target_min`.
    TargetRequiresMore { target_min: usize },
    /// `TS2621` — "Target allows only {0} element(s) but source may have more."
    /// The target is closed and the variadic source may overflow it. Carries
    /// `target_arity`.
    TargetAllowsFewer { target_arity: usize },
}

impl TupleArity {
    /// The diagnostic code for this arity family.
    pub const fn diagnostic_code(self) -> u32 {
        match self {
            Self::SourceTooFew { .. } => codes::SOURCE_HAS_ELEMENT_S_BUT_TARGET_REQUIRES,
            Self::SourceTooMany { .. } => codes::SOURCE_HAS_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY,
            Self::TargetRequiresMore { .. } => {
                codes::TARGET_REQUIRES_ELEMENT_S_BUT_SOURCE_MAY_HAVE_FEWER
            }
            Self::TargetAllowsFewer { .. } => {
                codes::TARGET_ALLOWS_ONLY_ELEMENT_S_BUT_SOURCE_MAY_HAVE_MORE
            }
        }
    }

    /// The catalog message template for this arity family. Derived from the
    /// shared diagnostic catalog via [`diagnostic_code`](Self::diagnostic_code)
    /// so the wording and code can never drift apart, and so the rendering layer
    /// does not need its own variant-to-message mapping.
    pub fn diagnostic_message(self) -> &'static str {
        get_message_template(self.diagnostic_code())
    }

    /// The numeric message arguments, in catalog order. `SourceTooFew` and
    /// `SourceTooMany` take two; the `may-have` variants take one.
    pub fn message_args(self) -> Vec<usize> {
        match self {
            Self::SourceTooFew {
                source_arity,
                target_min,
            } => vec![source_arity, target_min],
            Self::SourceTooMany {
                source_min,
                target_arity,
            } => vec![source_min, target_arity],
            Self::TargetRequiresMore { target_min } => vec![target_min],
            Self::TargetAllowsFewer { target_arity } => vec![target_arity],
        }
    }
}

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
    /// ES private identifier (`#name`) member originates from a different
    /// declaring class, so the target's slot is unreachable from the source
    /// (TS18015: "refers to a different member").
    PrivateIdentifierMemberMismatch { property_name: Atom },
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
        /// Why the inner check between `target_param` and `source_param`
        /// failed (contravariant in strict-function-types mode).
        /// Carried so callers can elaborate the failure shape — for
        /// example, distinguishing a callback's inner return-type
        /// failure from an inner parameter failure.
        inner_reason: Option<Box<Self>>,
    },
    /// A target function/method's return carries a type predicate (`x is T`,
    /// `this is T`) that the source signature cannot satisfy.
    ///
    /// `source_predicate: None` is tsc's `Signature_0_must_be_a_type_predicate`
    /// (TS1224) — the target requires a type guard and the source has no
    /// predicate at all (an assertion-only target, `asserts x`, is compatible
    /// without one and never reaches this variant). `source_signature` is the
    /// interned whole-function type of the source, rendered in
    /// `signatureToString` colon form for that message's `'{0}'` argument.
    ///
    /// `source_predicate: Some(_)` is `Type_predicate_0_is_not_assignable_to_1`
    /// (TS1226) — both sides declare a predicate but they target different
    /// parameters, mix a type guard with an assertion, or narrow to
    /// incompatible types. `nested_reason` carries the inner
    /// `source_predicate.type_id`/`target_predicate.type_id` mismatch when
    /// both predicates narrow to a type and those types are merely related,
    /// not the same/subtype.
    TypePredicateMismatch {
        source_predicate: Option<TypePredicate>,
        target_predicate: TypePredicate,
        source_signature: Option<TypeId>,
        nested_reason: Option<Box<Self>>,
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
    /// Tuple arity mismatch pre-classified to tsc's `TS2618`–`TS2621` family.
    ///
    /// Unlike [`Self::TupleElementMismatch`] (which only carries raw element
    /// counts and so cannot tell a fixed slot from a variadic one), this variant
    /// records the exact `tsc` branch — including whether the count refers to a
    /// minimum length or a full arity — so a variadic source like
    /// `[boolean, ...number[]]` reports its required length (`1`) rather than its
    /// slot count (`2`).
    TupleArityMismatch(TupleArity),
    /// Tuple element type mismatch.
    ///
    /// When the related tuple has **more than one** element (`multi_element`),
    /// tsc disambiguates the failing slot with TS2626
    /// `Type at position <index> in source is not compatible with type at
    /// position <index> in target.`, nested beneath the outer
    /// `Type 'S' is not assignable to type 'T'.` line, then the inner element
    /// failure carried in `nested_reason`.
    ///
    /// When the tuple has a **single** element there is no position to
    /// disambiguate, so tsc omits the positional line and relates the element
    /// types directly with the standard `Type 'se' is not assignable to type
    /// 'te'.` message, recursing through `nested_reason`. `multi_element`
    /// records this structural distinction (`source.len() > 1`) so the renderer
    /// can reproduce the exact chain shape.
    TupleElementTypeMismatch {
        index: usize,
        /// Target-side position. Differs from `index` when the failing target
        /// element trails a rest slot (`[...number[], boolean]`: a one-element
        /// source fails at source position 0 against TARGET position 1 —
        /// tsc numbers the positions independently).
        target_index: usize,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<Box<Self>>,
        /// `true` when the related tuple has more than one element and the
        /// positional disambiguation line is warranted.
        multi_element: bool,
    },
    /// Tuple element type mismatch inside the region that aligns to a target
    /// **rest** element (a variadic slot), where the source occupies a *span* of
    /// positions while the target is a single rest slot.
    ///
    /// Unlike [`Self::TupleElementTypeMismatch`] — a fixed element where the
    /// source and target share one position — `tsc` here renders the plural
    /// `Type at positions <start> through <end> in source is not compatible with
    /// type at position <target> in target.` (TS2627), or the singular
    /// `Type at position <start> in source ... position <target> in target.`
    /// (TS2626) when the span is a single element, with the failing element
    /// relation nested beneath. The target position is the rest slot index, which
    /// generally differs from the source span.
    TupleVariadicPositionMismatch {
        /// First source element index aligned to the target rest slot.
        source_start: usize,
        /// Last source element index aligned to the target rest slot (inclusive).
        source_end: usize,
        /// The target rest slot's element index.
        target_position: usize,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<Box<Self>>,
    },
    /// An unbounded array source provides no value to bind a target tuple's
    /// required (`TS2623`) or variadic (`TS2624`) element at a given position.
    ///
    /// `tsc` reports this when the target carries a rest element — so the
    /// closed-target arity gate (`Self::TupleArityMismatch`) does *not* fire —
    /// yet a fixed slot the open-ended source cannot guarantee a value for
    /// precedes the rest coverage. Closed-target arity gaps (no target rest)
    /// surface through `Self::TupleArityMismatch` (`TS2620`/`TS2621`) instead.
    /// `variadic` selects between the required-element message (`TS2623`,
    /// `false`) and the variadic-element message (`TS2624`, `true`).
    SourceProvidesNoMatch { position: usize, variadic: bool },
    /// Array element type mismatch.
    ///
    /// Like a single-element tuple, an array relation fails through its element
    /// type, and `tsc` elaborates the failure by relating the element types
    /// directly beneath the `Type 'se[]' …'te[]'` line — recursing into the
    /// element's own reason via `nested_reason` (e.g. `number[][]` →
    /// `string[][]` walks one array level at a time, and `{ b: T }[]` drills
    /// into the offending property). When the element relation is a terminal
    /// scalar leaf, `nested_reason` is `None` and the renderer emits the plain
    /// `Type 'se' …'te'` line.
    ArrayElementMismatch {
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<Box<Self>>,
    },
    /// Index signature value type mismatch.
    IndexSignatureMismatch {
        index_kind: &'static str, // "string", "number", or "symbol"
        source_value_type: TypeId,
        target_value_type: TypeId,
        /// Nested failure explaining why the value types are incompatible.
        nested_reason: Option<Box<Self>>,
        /// The failing member's own property name when the incompatibility is a
        /// named source **property** measured against the target's index
        /// signature (`tsc` renders TS2530 "Property '{name}' is incompatible
        /// with index signature."). `None` when the incompatibility is a source
        /// **index signature** vs the target index signature (`tsc` renders
        /// TS2634 "'{kind}' index signatures are incompatible."). The head code
        /// is TS2322/TS2345 in both cases; this only selects the elaboration.
        property_name: Option<Atom>,
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
    /// Two distinct type parameters used as keys of structurally-identical
    /// indexed-access types: `S[T1]` against `S[T2]`. Even when `T1`'s
    /// constraint is assignable to `T2`'s constraint, the parameter
    /// identities differ and the index-access subtypes do not hold. tsc
    /// elaborates this as the TS2322 chain plus the TS5075 message.
    IndexAccessTypeParameterMismatch {
        /// The source type parameter used as the source index access key.
        source_param: TypeId,
        /// The target type parameter used as the target index access key.
        target_param: TypeId,
        /// The target parameter's constraint, surfaced in the TS5075
        /// elaboration. `None` only when the target parameter is
        /// unconstrained (no useful elaboration to emit).
        target_constraint: Option<TypeId>,
    },
    /// An abstract constructor type was assigned to a non-abstract
    /// constructor type. The relation correctly fails, but the only
    /// explanation tsc gives is the TS2517 elaboration line after the
    /// top-level TS2322/TS2345 message. The abstractness decision needs
    /// checker symbol context, so this reason is produced at the checker
    /// boundary rather than by the structural subtype walk. The source and
    /// target types come from the diagnostic context, so this variant carries
    /// no payload.
    AbstractConstructorAssignment,
    /// Two applications of the **same generic target** (`C<A..>` vs `C<B..>`)
    /// whose differing type **arguments** are the cause of the failure.
    ///
    /// tsc elaborates this by comparing the failing type argument directly,
    /// emitting a single nested line (e.g. `Type 'number' is not assignable to
    /// type 'string'.`) beneath the top-level TS2322/TS2345 message, with no
    /// intermediate `Types of property 'x' are incompatible.` wrapper that a
    /// structural-member walk would otherwise produce.
    ///
    /// `source_arg`/`target_arg` are the failing argument pair, already
    /// oriented for the parameter's variance (for a contravariant parameter
    /// these are the target/source arguments respectively). `nested_reason`
    /// explains that argument relation and is rendered one level deeper.
    TypeArgumentMismatch {
        source_arg: TypeId,
        target_arg: TypeId,
        nested_reason: Box<Self>,
    },
    /// A union **source** is not assignable to the target because one of its
    /// members is not assignable.
    ///
    /// tsc elaborates this with the top-level `Type 'A | B' is not assignable to
    /// type 'T'.` line followed by the first failing member's relation
    /// (`Type 'B' is not assignable to type 'T'.`) carried in `nested_reason`
    /// and rendered one level deeper. This keeps the root mismatch visible
    /// instead of stopping at the bare union-to-target line.
    ///
    /// `member_type` is the first union member that fails against the target;
    /// `nested_reason` explains that member's relation.
    UnionSourceMismatch {
        source_type: TypeId,
        target_type: TypeId,
        member_type: TypeId,
        nested_reason: Box<Self>,
    },
    /// A source is not assignable to a union **target** because it fails to
    /// match any member; the best-matching member's own failure is carried for
    /// elaboration.
    ///
    /// tsc relates an object source against a union by selecting the
    /// best-matching constituent (`getBestMatchingType`: a written unit
    /// discriminant first, then `findMostOverlappyType` — the member sharing
    /// the most property-name keys with the source, ties broken by the *last*
    /// such member) and re-runs the failed relation against it with errors
    /// enabled. The top-level line stays `Type 'S' is not assignable to type
    /// 'U'.`; beneath it, a missing required property folds directly
    /// (`Property 'x' is missing in type 'S' but required in type '<member>'.`
    /// or the multi-property TS2739 form), while any other failure elaborates
    /// under a `Type 'S' is not assignable to type '<member>'.` member frame
    /// followed by the member relation's own drill (`Types of property 'm'
    /// are incompatible.` …) — instead of stopping at the bare union line.
    ///
    /// `member_type` is the best-matching union member; `nested_reason` is the
    /// failure of `source_type` against that member.
    UnionTargetMismatch {
        source_type: TypeId,
        target_type: TypeId,
        member_type: TypeId,
        nested_reason: Box<Self>,
    },
    /// A deferred conditional type relation failed because one of its branches
    /// fails the corresponding branch relation.
    ///
    /// Without this variant, `T extends U ? X : Y` relations that cannot be
    /// resolved at evaluation time collapse to a bare `TypeMismatch` and the
    /// diagnostic chain stops at the outer
    /// `Type 'S' is not assignable to type 'T extends U ? X : Y'.` line —
    /// hiding the actual branch-level reason (for example, `"yes"` not being
    /// assignable to `"x"` for `T extends string ? "yes" : "no" <: "x"`).
    ///
    /// The variant covers the three structural shapes the conditional rules
    /// distinguish:
    ///
    /// 1. Concrete source vs deferred-conditional target: source must be
    ///    `<:` both branches; `branch_source` is the original source and
    ///    `branch_target` is the failing branch (`true_type` or `false_type`).
    /// 2. Deferred-conditional source vs concrete target: both branches must
    ///    be `<:` target; `branch_source` is the failing branch and
    ///    `branch_target` is the original target.
    /// 3. Conditional vs conditional (matching extends shape): branches are
    ///    compared pairwise (`source.true_type <: target.true_type`, etc.);
    ///    the variant carries the failing pair.
    ///
    /// `nested_reason` explains the failing branch relation and is rendered
    /// one level deeper, preserving the full chain (a literal mismatch, a
    /// missing property, a deeper conditional, etc.).
    ConditionalBranchMismatch {
        /// The original conditional-shaped source (or its concrete value when
        /// the conditional is on the target side).
        source_type: TypeId,
        /// The original conditional-shaped target (or its concrete value when
        /// the conditional is on the source side).
        target_type: TypeId,
        /// The source half of the failing branch relation.
        branch_source: TypeId,
        /// The target half of the failing branch relation. Callers needing to
        /// distinguish the true vs false branch can compare this against the
        /// originating conditional's `true_type` / `false_type`.
        branch_target: TypeId,
        /// Why the branch's relation failed.
        nested_reason: Box<Self>,
    },
    /// A type-parameter source failed to relate to the target, and the failure
    /// is explained through the parameter's declared (base) constraint.
    ///
    /// `tsc` elaborates `T <: X` (when `T` is a type parameter) by first
    /// stating the top-level `Type 'T' is not assignable to type 'X'.` and then
    /// recursing on the parameter's base constraint:
    /// `Type '<constraint>' is not assignable to type 'X'.`, drilling further
    /// into whatever structural reason that relation fails for. Without this
    /// variant a type-parameter source matches none of the structural arms in
    /// the explain path (it has no object/tuple/union/primitive shape of its
    /// own) and collapses to a bare [`Self::TypeMismatch`], hiding the
    /// constraint-level root that is the actual reason the relation fails.
    ///
    /// This mirrors `tsc`'s `getBaseConstraintOfType` elaboration and is
    /// independent of the target shape: the target may be a primitive, an
    /// object, a union, or an evaluated conditional/mapped/alias result. Nested
    /// constraints (`U extends T`, `T extends string`) recurse naturally, each
    /// adding one indent level.
    TypeParameterConstraintMismatch {
        /// The type-parameter source (rendered at the top, e.g. `T`).
        source_type: TypeId,
        /// The target the parameter failed to relate to, in its evaluated
        /// (apparent) form so the displayed target matches `tsc` (e.g. the
        /// concrete result of an instantiated conditional alias rather than the
        /// unevaluated `Alias<Arg>` spelling).
        target_type: TypeId,
        /// The parameter's resolved base constraint — the source half of the
        /// child relation (`<constraint> <: target_type`).
        constraint_type: TypeId,
        /// Why the constraint-level relation failed.
        nested_reason: Box<Self>,
    },
    /// A source is not assignable to a target **intersection** because it fails
    /// one of the intersection's constituents.
    ///
    /// `tsc` relates a source to each constituent of a target intersection
    /// `C1 & C2 & …` in written order (`typeRelatedToEachType`) and elaborates
    /// the **first** failing constituent: the top-level `Type 'S' is not
    /// assignable to type 'C1 & C2 & …'.` line is followed by the full relation
    /// of `S <: Ci` one level deeper — `Type 'S' is not assignable to type
    /// 'Ci'.` plus its own drill for a structural failure, or directly
    /// `Property 'p' is missing in type 'S' but required in type 'Ci'.` for a
    /// missing-property leaf (which already names `Ci`).
    ///
    /// Without this variant the intersection target is structurally merged into
    /// a single object before the reason is built (`evaluate_type_for_assignability`),
    /// so the chain skips straight to the merged property mismatch and drops the
    /// constituent context that explains *which* member of the intersection
    /// requires the failing shape.
    ///
    /// `constituent_type` is the first failing constituent; `nested_reason`
    /// explains the `S <: constituent_type` relation and is rendered against
    /// `(source_type, constituent_type)`, so the nested reason's own top line
    /// becomes the constituent frame. `original_reason` is the merged-target
    /// reason this wraps — only its **headline** is rendered, so the top
    /// `Type 'S' is not assignable to type 'C1 & C2 & …'.` line stays
    /// byte-identical to the pre-wrap output (which is the only line the
    /// conformance harness fingerprints); the constituent frame and drill
    /// replace its elaboration.
    IntersectionTargetMismatch {
        source_type: TypeId,
        target_type: TypeId,
        constituent_type: TypeId,
        nested_reason: Box<Self>,
        original_reason: Box<Self>,
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
    /// The candidate signature type this diagnostic describes, when it is one
    /// overload's applicability failure within a `NoOverloadMatch` set. The
    /// checker reporter uses it to render tsc's per-overload `TS2772`
    /// (`Overload N of M, '<signature>', gave the following error.`) wrapper.
    /// `None` for every non-overload diagnostic.
    pub overload_signature: Option<TypeId>,
    /// Positional index of the first non-assignable argument, when this
    /// diagnostic is one overload candidate's argument-applicability failure
    /// (`TS2345`). tsc anchors the top-level `TS2769` at the last
    /// argument-error candidate's failing argument; the checker maps this
    /// index onto the call's logical argument nodes to recover that anchor.
    /// AST-agnostic (a plain positional index), so it stays inside the solver.
    /// `None` for non-argument diagnostics.
    pub argument_index: Option<usize>,
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
            overload_signature: None,
            argument_index: None,
        }
    }

    /// Tag this diagnostic with the overload candidate signature it describes,
    /// so the reporter can wrap it in the `TS2772` per-overload elaboration.
    pub const fn with_overload_signature(mut self, signature: TypeId) -> Self {
        self.overload_signature = Some(signature);
        self
    }

    /// Record the positional index of the first non-assignable argument for
    /// this overload candidate's failure, so the checker can anchor the
    /// top-level `TS2769` at that argument (tsc's `candidatesForArgumentError`
    /// last-candidate rule).
    pub const fn with_argument_index(mut self, index: usize) -> Self {
        self.argument_index = Some(index);
        self
    }

    /// Whether this diagnostic reports an argument-arity failure
    /// (`TS2554`/`TS2555`), tsc's `candidatesForArgumentError` exclusion.
    /// Owned here so the code set stays beside the builders that assign it;
    /// the rendering policy it feeds lives in the checker's
    /// `error_no_overload_matches_at`.
    pub const fn is_arity_failure(&self) -> bool {
        matches!(
            self.code,
            codes::ARG_COUNT_MISMATCH | codes::ARG_COUNT_AT_LEAST_MISMATCH
        )
    }

    /// The `(source, target)` pair of a two-type diagnostic (e.g. `TS2345`
    /// argument-not-assignable), when the first two message args are types.
    /// Owns the positional arg encoding so consumers do not.
    pub fn type_pair(&self) -> Option<(TypeId, TypeId)> {
        match (self.args.first(), self.args.get(1)) {
            (Some(DiagnosticArg::Type(source)), Some(DiagnosticArg::Type(target))) => {
                Some((*source, *target))
            }
            _ => None,
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

    /// Attach `span` when present; no-op when `None`.
    pub fn with_optional_span(self, span: Option<SourceSpan>) -> Self {
        if let Some(s) = span {
            self.with_span(s)
        } else {
            self
        }
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
    /// Nesting depth within the parent diagnostic's elaboration chain.
    /// `0` is the first elaboration line (rendered at 2 spaces of indent);
    /// each deeper level adds 2 more spaces. Non-chain related entries (e.g.
    /// genuine cross-location pointers like "see declaration here") stay at
    /// `0`.
    pub depth: u8,
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

    /// Add related information at depth 0.
    pub fn with_related(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.related.push(RelatedInformation {
            span,
            message: message.into(),
            depth: 0,
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
    pub use dc::CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE as ABSTRACT_CONSTRUCTOR_ASSIGNMENT;
    pub use dc::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY as READONLY_PROPERTY;
    pub use dc::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE as EXCESS_PROPERTY;
    pub use dc::PROPERTY_IN_TYPE_REFERS_TO_A_DIFFERENT_MEMBER_THAT_CANNOT_BE_ACCESSED_FROM_WITHI as PRIVATE_IDENTIFIER_MEMBER_MISMATCH;
    pub use dc::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE as PROPERTY_MISSING;
    pub use dc::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE as PROPERTY_OPTIONAL_BUT_REQUIRED;
    pub use dc::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS as PROPERTY_VISIBILITY_MISMATCH;
    pub use dc::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A as PROPERTY_NOMINAL_MISMATCH;
    pub use dc::SIGNATURE_MUST_BE_A_TYPE_PREDICATE;
    pub use dc::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE as READONLY_TO_MUTABLE;
    pub use dc::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE as NO_COMMON_PROPERTIES;
    pub use dc::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE as MISSING_PROPERTIES;
    pub use dc::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE as TYPE_NOT_ASSIGNABLE;
    pub use dc::TYPE_PREDICATE_IS_NOT_ASSIGNABLE_TO as TYPE_PREDICATE_NOT_ASSIGNABLE_TO;

    pub use dc::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE as MISSING_INDEX_SIGNATURE;
    pub use dc::IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE as TYPE_PARAM_INSTANTIATED_WITH_DIFFERENT_SUBTYPE;
    pub use dc::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET as TUPLE_ELEMENT_POSITION_MISMATCH;
    pub use dc::TYPE_AT_POSITIONS_THROUGH_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_T as TUPLE_ELEMENT_POSITION_SPAN_MISMATCH;
    pub use dc::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE as PROPERTY_TYPE_MISMATCH;

    // Tuple arity mismatch family (TS2618–TS2621).
    pub use dc::SOURCE_HAS_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY;
    pub use dc::SOURCE_HAS_ELEMENT_S_BUT_TARGET_REQUIRES;
    pub use dc::TARGET_ALLOWS_ONLY_ELEMENT_S_BUT_SOURCE_MAY_HAVE_MORE;
    pub use dc::TARGET_REQUIRES_ELEMENT_S_BUT_SOURCE_MAY_HAVE_FEWER;

    // Open-array source vs tuple target, required/variadic slot (TS2623/TS2624).
    pub use dc::SOURCE_PROVIDES_NO_MATCH_FOR_REQUIRED_ELEMENT_AT_POSITION_IN_TARGET as SOURCE_NO_MATCH_REQUIRED_ELEMENT;
    pub use dc::SOURCE_PROVIDES_NO_MATCH_FOR_VARIADIC_ELEMENT_AT_POSITION_IN_TARGET as SOURCE_NO_MATCH_VARIADIC_ELEMENT;

    // Function/call errors
    pub use dc::CANNOT_FIND_NAME;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB as CANNOT_FIND_NAME_TARGET_LIB;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2 as CANNOT_FIND_NAME_DOM;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N_2 as CANNOT_FIND_NAME_TEST_RUNNER;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE_2 as CANNOT_FIND_NAME_BUN;
    pub use dc::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2 as CANNOT_FIND_NAME_NODE;
    pub use dc::EXPECTED_ARGUMENTS_BUT_GOT as ARG_COUNT_MISMATCH;
    pub use dc::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT as ARG_COUNT_AT_LEAST_MISMATCH;
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

/// Diagnostic code for the open-array-source vs tuple-target no-match family:
/// `TS2624` for a variadic target slot, `TS2623` for a required one. Single
/// source of truth shared by `diagnostic_code` and `to_diagnostic`.
const fn source_no_match_code(variadic: bool) -> u32 {
    if variadic {
        codes::SOURCE_NO_MATCH_VARIADIC_ELEMENT
    } else {
        codes::SOURCE_NO_MATCH_REQUIRED_ELEMENT
    }
}

impl SubtypeFailureReason {
    /// Return the primary diagnostic code for this failure reason.
    ///
    /// This is the single source of truth for mapping `SubtypeFailureReason` variants
    /// to diagnostic codes. Both the solver's `to_diagnostic` and the checker's
    /// `render_failure_reason` should use this to stay in sync.
    pub const fn diagnostic_code(&self) -> u32 {
        match self {
            Self::MissingProperty { .. } => codes::PROPERTY_MISSING,
            // A present-but-optional source property assigned to a required
            // target is TS2327, not the absent-property message TS2741.
            Self::OptionalPropertyRequired { .. } => codes::PROPERTY_OPTIONAL_BUT_REQUIRED,
            Self::MissingProperties { .. } => codes::MISSING_PROPERTIES,
            Self::PropertyTypeMismatch { .. } => codes::PROPERTY_TYPE_MISMATCH,
            Self::ReadonlyPropertyMismatch { .. } => codes::READONLY_PROPERTY,
            Self::PropertyVisibilityMismatch { .. } => codes::PROPERTY_VISIBILITY_MISMATCH,
            Self::PropertyNominalMismatch { .. } => codes::PROPERTY_NOMINAL_MISMATCH,
            Self::PrivateIdentifierMemberMismatch { .. } => {
                codes::PRIVATE_IDENTIFIER_MEMBER_MISMATCH
            }
            Self::TypePredicateMismatch {
                source_predicate: None,
                ..
            } => codes::SIGNATURE_MUST_BE_A_TYPE_PREDICATE,
            Self::TypePredicateMismatch {
                source_predicate: Some(_),
                ..
            } => codes::TYPE_PREDICATE_NOT_ASSIGNABLE_TO,
            Self::ReturnTypeMismatch { .. }
            | Self::ParameterTypeMismatch { .. }
            | Self::TupleElementMismatch { .. }
            | Self::TupleElementTypeMismatch { .. }
            | Self::TupleVariadicPositionMismatch { .. }
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
            | Self::TooManyParameters { .. }
            | Self::IndexAccessTypeParameterMismatch { .. }
            | Self::TypeArgumentMismatch { .. }
            | Self::UnionSourceMismatch { .. }
            | Self::UnionTargetMismatch { .. }
            | Self::ConditionalBranchMismatch { .. }
            | Self::TypeParameterConstraintMismatch { .. }
            | Self::IntersectionTargetMismatch { .. }
            | Self::AbstractConstructorAssignment => codes::TYPE_NOT_ASSIGNABLE,
            Self::TupleArityMismatch(arity) => arity.diagnostic_code(),
            Self::SourceProvidesNoMatch { variadic, .. } => source_no_match_code(*variadic),
            Self::NoCommonProperties { .. } => codes::NO_COMMON_PROPERTIES,
            Self::ExcessProperty { .. } => codes::EXCESS_PROPERTY,
            Self::ReadonlyToMutableAssignment { .. } => codes::READONLY_TO_MUTABLE,
        }
    }

    /// Attach the failing inner element relation beneath `diag`: drill into the
    /// structured `nested_reason` when present, otherwise emit the flat
    /// `Type 'se' is not assignable to type 'te'.` leaf. Shared by the tuple
    /// element, variadic-span, and array element arms, which all relate a single
    /// inner element pair (`source_element`/`target_element`) the same way.
    fn with_element_relation(
        diag: PendingDiagnostic,
        nested_reason: &Option<Box<Self>>,
        source_element: TypeId,
        target_element: TypeId,
    ) -> PendingDiagnostic {
        match nested_reason {
            Some(nested) => diag.with_related(nested.to_diagnostic(source_element, target_element)),
            None => diag.with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source_element.into(), target_element.into()],
            )),
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
                // The source property is present but optional while the target
                // requires it. tsc reports TS2327 ("Property 'x' is optional in
                // type 'S' but required in type 'T'."), not the absent-property
                // message TS2741.
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::PROPERTY_OPTIONAL_BUT_REQUIRED,
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

            Self::PrivateIdentifierMemberMismatch { property_name } => {
                // TS18015: Property '#x' in type 'A' refers to a different
                // member that cannot be accessed from within type 'B'
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::PRIVATE_IDENTIFIER_MEMBER_MISMATCH,
                    vec![(*property_name).into(), source.into(), target.into()],
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
                inner_reason: _,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_param).into(), (*target_param).into()],
            )),

            Self::TypePredicateMismatch {
                source_predicate: _,
                target_predicate: _,
                source_signature: _,
                nested_reason,
            } => {
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );
                if let Some(nested) = nested_reason {
                    diag = diag.with_related(nested.to_diagnostic(source, target));
                }
                diag
            }

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

            Self::TupleArityMismatch(arity) => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                arity.diagnostic_code(),
                arity.message_args().into_iter().map(|n| n.into()).collect(),
            )),

            Self::SourceProvidesNoMatch { position, variadic } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![source.into(), target.into()],
            )
            .with_related(PendingDiagnostic::error(
                source_no_match_code(*variadic),
                vec![(*position).into()],
            )),

            Self::TupleElementTypeMismatch {
                index,
                target_index,
                source_element,
                target_element,
                nested_reason,
                multi_element,
            } => {
                // Multi-element tuples disambiguate the failing slot with the
                // TS2626 positional line; single-element tuples omit it and
                // relate the element types directly (see the variant docs).
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );
                if *multi_element {
                    diag = diag.with_related(PendingDiagnostic::error(
                        codes::TUPLE_ELEMENT_POSITION_MISMATCH,
                        vec![(*index).into(), (*target_index).into()],
                    ));
                }
                Self::with_element_relation(diag, nested_reason, *source_element, *target_element)
            }

            Self::TupleVariadicPositionMismatch {
                source_start,
                source_end,
                target_position,
                source_element,
                target_element,
                nested_reason,
            } => {
                // A single-element span uses the singular TS2626 positional line;
                // a multi-element span uses the plural TS2627 "positions X through
                // Y" line. The target position is the rest slot, distinct from the
                // source span.
                let positional = if source_start == source_end {
                    PendingDiagnostic::error(
                        codes::TUPLE_ELEMENT_POSITION_MISMATCH,
                        vec![(*source_start).into(), (*target_position).into()],
                    )
                } else {
                    PendingDiagnostic::error(
                        codes::TUPLE_ELEMENT_POSITION_SPAN_MISMATCH,
                        vec![
                            (*source_start).into(),
                            (*source_end).into(),
                            (*target_position).into(),
                        ],
                    )
                };
                let diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(positional);
                Self::with_element_relation(diag, nested_reason, *source_element, *target_element)
            }

            Self::ArrayElementMismatch {
                source_element,
                target_element,
                nested_reason,
            } => {
                let diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );
                Self::with_element_relation(diag, nested_reason, *source_element, *target_element)
            }

            Self::IndexSignatureMismatch {
                index_kind: _,
                source_value_type,
                target_value_type,
                nested_reason: _,
                property_name: _,
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
                vec![(*index_kind).into(), source.into()],
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
            Self::IndexAccessTypeParameterMismatch {
                source_param,
                target_param,
                target_constraint,
            } => {
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );
                diag = diag.with_related(PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![(*source_param).into(), (*target_param).into()],
                ));
                if let Some(constraint) = target_constraint {
                    diag = diag.with_related(PendingDiagnostic::error(
                        codes::TYPE_PARAM_INSTANTIATED_WITH_DIFFERENT_SUBTYPE,
                        vec![
                            (*source_param).into(),
                            (*target_param).into(),
                            (*constraint).into(),
                        ],
                    ));
                }
                diag
            }
            Self::AbstractConstructorAssignment => {
                // TS2322 top-level + TS2517 elaboration explaining the failure.
                PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                )
                .with_related(PendingDiagnostic::error(
                    codes::ABSTRACT_CONSTRUCTOR_ASSIGNMENT,
                    vec![],
                ))
            }
            Self::TypeArgumentMismatch {
                source_arg,
                target_arg,
                nested_reason,
            } => {
                // Top-level TS2322 for the application pair, then the failing
                // type argument relation directly beneath it. tsc does not emit
                // a `Types of property` wrapper for same-generic argument
                // mismatches, so the nested argument failure is the only
                // elaboration line.
                let mut diag = PendingDiagnostic::error(
                    codes::TYPE_NOT_ASSIGNABLE,
                    vec![source.into(), target.into()],
                );
                diag = diag.with_related(nested_reason.to_diagnostic(*source_arg, *target_arg));
                diag
            }

            Self::UnionSourceMismatch {
                source_type,
                target_type,
                member_type,
                nested_reason,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            )
            .with_related(nested_reason.to_diagnostic(*member_type, *target_type)),
            Self::UnionTargetMismatch {
                source_type,
                target_type,
                member_type,
                nested_reason,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            )
            .with_related(nested_reason.to_diagnostic(*source_type, *member_type)),
            Self::ConditionalBranchMismatch {
                source_type,
                target_type,
                branch_source,
                branch_target,
                nested_reason,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            )
            .with_related(nested_reason.to_diagnostic(*branch_source, *branch_target)),
            Self::TypeParameterConstraintMismatch {
                source_type,
                target_type,
                constraint_type,
                nested_reason,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            )
            .with_related(nested_reason.to_diagnostic(*constraint_type, *target_type)),
            // Render the intersection headline against the full intersection,
            // then the failing constituent's relation one level deeper. Passing
            // `(source_type, constituent_type)` to the nested reason makes its own
            // top line the constituent frame (`Type 'S' is not assignable to type
            // 'Ci'.`) for structural failures, while a missing-property leaf
            // renders directly with `Ci` named (its own `target_type` field).
            Self::IntersectionTargetMismatch {
                source_type,
                target_type,
                constituent_type,
                nested_reason,
                original_reason: _,
            } => PendingDiagnostic::error(
                codes::TYPE_NOT_ASSIGNABLE,
                vec![(*source_type).into(), (*target_type).into()],
            )
            .with_related(nested_reason.to_diagnostic(*source_type, *constituent_type)),
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
