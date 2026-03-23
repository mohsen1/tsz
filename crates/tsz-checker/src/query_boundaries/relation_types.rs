//! Checker-facing structured failure types for relation queries.
//!
//! Wraps solver's `SubtypeFailureReason` with additional checker context
//! so diagnostic rendering has everything it needs without re-running
//! the relation.

use tsz_common::interner::Atom;
use tsz_solver::{SubtypeFailureReason, TypeId};

// ---------------------------------------------------------------------------
// PropertyClassification: structured property-level analysis for EPC/missing
// ---------------------------------------------------------------------------

/// Structured property-level classification of an object compatibility check.
///
/// This is the canonical boundary output for property-level analysis. The checker
/// uses this to decide WHICH diagnostic to emit and WHERE, without re-implementing
/// property existence/compatibility logic.
#[derive(Debug, Clone, Default)]
pub(crate) struct PropertyClassification {
    /// Properties that exist in source but not in target (excess).
    pub excess_properties: Vec<Atom>,
    /// Properties that exist in target but not in source (missing required).
    pub missing_properties: Vec<(Atom, TypeId)>,
    /// Properties that exist in both but have incompatible types.
    pub incompatible_properties: Vec<(Atom, TypeId, TypeId)>,
    /// Whether the target has an index signature that accepts arbitrary keys.
    pub target_has_index_signature: bool,
    /// Whether the target is a type parameter (EPC should be skipped).
    pub target_is_type_parameter: bool,
    /// Whether the target is the global Object/Function interface (EPC skipped).
    pub target_is_global_object_or_function: bool,
    /// Whether the target is an empty object type `{}` (accepts anything).
    pub target_is_empty_object: bool,
    /// Whether ALL properties that exist in both source and target have
    /// compatible (assignable) types. When `true` and `excess_properties`
    /// is non-empty, the relation failure is caused ONLY by excess properties.
    /// This enables `should_skip_weak_union_error` to make its decision
    /// without re-enumerating properties and re-checking assignability.
    pub all_matching_compatible: bool,
    /// Whether a trimmed source (only matching properties) would be assignable
    /// to the target. `false` when structural factors beyond property names
    /// (e.g., deferred conditionals) prevent assignability.
    pub trimmed_source_assignable: bool,
    /// Whether any target member has a number index signature.
    /// Used to suppress EPC for numeric property names.
    pub target_has_number_index: bool,
}

/// Checker-facing classification of a relation failure.
///
/// Groups solver-level details into the categories the checker's diagnostic
/// renderer needs.  Not a 1:1 copy of `SubtypeFailureReason`.
#[derive(Debug, Clone)]
pub(crate) enum RelationFailure {
    /// A required property is missing from the source type.
    MissingProperty {
        property_name: Atom,
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Multiple required properties are missing.
    MissingProperties {
        property_names: Vec<Atom>,
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Source has a property not declared in target (excess property).
    ExcessProperty {
        property_name: Atom,
        target_type: TypeId,
    },
    /// A property exists in both but types are incompatible.
    IncompatiblePropertyValue {
        property_name: Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested: Option<Box<RelationFailure>>,
    },
    /// No applicable call/construct signature matched.
    NoApplicableSignature {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// Tuple arity mismatch.
    TupleArityMismatch {
        source_count: usize,
        target_count: usize,
    },
    /// Return type incompatibility.
    ReturnTypeMismatch {
        source_return: TypeId,
        target_return: TypeId,
        nested: Option<Box<RelationFailure>>,
    },
    /// Parameter type incompatibility.
    ParameterTypeMismatch {
        param_index: usize,
        source_param: TypeId,
        target_param: TypeId,
    },
    /// Function parameter count mismatch.
    ParameterCountMismatch {
        source_count: usize,
        target_count: usize,
    },
    /// Property modifier mismatch (optional/readonly/visibility/nominal).
    PropertyModifierMismatch { property_name: Atom },
    /// Weak union violation (no common properties).
    WeakUnionViolation {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// General type mismatch (catch-all for solver reasons we don't
    /// need to classify further at the checker level).
    TypeMismatch {
        source_type: TypeId,
        target_type: TypeId,
    },
}

impl RelationFailure {
    /// Convert a solver `SubtypeFailureReason` into checker-facing `RelationFailure`.
    pub(crate) fn from_solver_reason(reason: SubtypeFailureReason) -> Self {
        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => Self::MissingProperty {
                property_name,
                source_type,
                target_type,
            },
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => Self::MissingProperties {
                property_names,
                source_type,
                target_type,
            },
            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type,
            } => Self::ExcessProperty {
                property_name,
                target_type,
            },
            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => Self::IncompatiblePropertyValue {
                property_name,
                source_property_type,
                target_property_type,
                nested: nested_reason.map(|r| Box::new(Self::from_solver_reason(*r))),
            },
            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => Self::TupleArityMismatch {
                source_count,
                target_count,
            },
            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => Self::ReturnTypeMismatch {
                source_return,
                target_return,
                nested: nested_reason.map(|r| Box::new(Self::from_solver_reason(*r))),
            },
            SubtypeFailureReason::ParameterTypeMismatch {
                param_index,
                source_param,
                target_param,
            } => Self::ParameterTypeMismatch {
                param_index,
                source_param,
                target_param,
            },
            SubtypeFailureReason::NoCommonProperties {
                source_type,
                target_type,
            } => Self::WeakUnionViolation {
                source_type,
                target_type,
            },
            SubtypeFailureReason::NoUnionMemberMatches { source_type, .. } => Self::TypeMismatch {
                source_type,
                target_type: TypeId::ERROR,
            },
            SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            }
            | SubtypeFailureReason::IntrinsicTypeMismatch {
                source_type,
                target_type,
            }
            | SubtypeFailureReason::LiteralTypeMismatch {
                source_type,
                target_type,
            }
            | SubtypeFailureReason::ErrorType {
                source_type,
                target_type,
            }
            | SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type,
                target_type,
            }
            | SubtypeFailureReason::NoIntersectionMemberMatches {
                source_type,
                target_type,
            } => Self::TypeMismatch {
                source_type,
                target_type,
            },
            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => Self::TypeMismatch {
                source_type: source_element,
                target_type: target_element,
            },
            SubtypeFailureReason::IndexSignatureMismatch {
                source_value_type,
                target_value_type,
                ..
            } => Self::TypeMismatch {
                source_type: source_value_type,
                target_type: target_value_type,
            },
            SubtypeFailureReason::TooManyParameters {
                source_count,
                target_count,
            }
            | SubtypeFailureReason::ParameterCountMismatch {
                source_count,
                target_count,
            } => Self::ParameterCountMismatch {
                source_count,
                target_count,
            },
            SubtypeFailureReason::OptionalPropertyRequired { property_name }
            | SubtypeFailureReason::ReadonlyPropertyMismatch { property_name }
            | SubtypeFailureReason::PropertyNominalMismatch { property_name } => {
                Self::PropertyModifierMismatch { property_name }
            }
            SubtypeFailureReason::PropertyVisibilityMismatch { property_name, .. } => {
                Self::PropertyModifierMismatch { property_name }
            }
            SubtypeFailureReason::TupleElementTypeMismatch {
                source_element,
                target_element,
                ..
            } => Self::TypeMismatch {
                source_type: source_element,
                target_type: target_element,
            },
            SubtypeFailureReason::MissingIndexSignature { .. }
            | SubtypeFailureReason::RecursionLimitExceeded => Self::TypeMismatch {
                source_type: TypeId::ERROR,
                target_type: TypeId::ERROR,
            },
        }
    }
}
