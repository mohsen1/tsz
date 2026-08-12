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
    /// TS2322 reason-entrypoint relation truth before detailed elaboration.
    AssignabilityReason,
    /// Checker type-overlap/comparability diagnostic probe.
    TypeComparability,
    /// `for...in` initializer target: key type stored into the LHS.
    ForInLhs,
    /// Function call argument: `fn(expr)` where param expects T
    CallArg,
    /// Return statement: `return expr` where function returns T
    Return,
    /// Callable-source to union-arm return compatibility probe.
    CallableUnionReturn,
    /// Callable-source to union-arm parameter compatibility probe.
    CallableUnionParameter,
    /// Type-predicate type compatibility against its parameter type.
    TypePredicateParameter,
    /// Generic argument diagnostic suppression compatibility probe.
    GenericArgumentSuppression,
    JsxProps,
    JsxChildren,
    /// Component compatibility against user-defined `JSX.ElementType`.
    JsxElementType,
    /// Destructuring: `const { a, b } = expr`
    Destructuring,
    /// Rest parameter array compatibility: `function f(...args: T)`
    RestParameter,
    /// Import attribute shape compatibility: `import x from "m" with { ... }`
    ImportAttributes,
    /// Computed enum-member initializer compatibility for TS18033.
    ComputedEnumMember,
    /// Numeric-enum assignment override compatibility for TS2322 diagnostics.
    NumericEnumAssignment,
    /// Type-parameter default compatibility against its constraint.
    TypeParameterDefault,
    /// Index-signature key/value compatibility for TS2411/TS2413 paths.
    IndexSignature,
    /// Decorator callee compatibility against the global `Function` type.
    DecoratorCallee,
    /// JSDoc type-argument compatibility against a template constraint.
    JsdocTypeConstraint,
    /// Explicit alias type-argument constraint compatibility probes.
    ExplicitAliasConstraint,
    /// Array-like generic constraint element compatibility probes.
    ArrayLikeConstraintElement,
    /// Merged-interface sibling constraint compatibility probes.
    MergedInterfaceConstraint,
    /// Recursive heritage property conflict compatibility probes.
    RecursiveHeritageProperty,
    /// Base-union member constraint compatibility probes.
    UnionConstraintMember,
    /// Syntax-instantiated type-argument constraint compatibility probes.
    SyntaxInstantiatedConstraint,
    /// Generic type-argument constraint compatibility probes.
    TypeArgConstraint,
    /// Mapped-type key constraint key-space compatibility probes.
    MappedKeyConstraint,
    /// Indexed-access generic constraint key-space compatibility probes.
    IndexedAccessConstraintKey,
    /// Indexed-access key-space compatibility probes before fallback selection.
    IndexedAccessKeySpace,
    /// Conditional generic constraint component compatibility probes.
    ConditionalConstraintComponent,
    /// Conditional true-branch type-parameter base constraint probes.
    ConditionalTrueBaseConstraint,
    /// Conditional true-branch generic constraint compatibility probes.
    ConditionalTrueBranchConstraint,
    /// Required mapped generic constraint compatibility probes.
    RequiredMappedConstraint,
    /// Infer-result generic constraint compatibility probes.
    InferResultConstraint,
    /// Generic constraint diagnostic property compatibility probes.
    GenericConstraintProperty,
    /// Source property-name literal compatibility against an index key type.
    PropertyIndexKey,
    /// Null/undefined source compatibility against a nullable structured target.
    NullishErrorTarget,
    /// Duplicate declaration compatibility probes for TS2300/TS2717 paths.
    DuplicateIdentifier,
    /// Variable initializer compatibility probes for TS2322 elaboration paths.
    VariableInitializer,
    /// Contextual binding-default identifier flow compatibility probes.
    IdentifierBindingDefault,
    /// `keyof` target compatibility probes for TS2322 diagnostic suppression.
    KeyofDiagnosticSuppression,
    /// Diagnostic-source narrowing display probes.
    DiagnosticSourceNarrowing,
    /// Diagnostic overlap/comparability probes.
    DiagnosticOverlap,
    /// Broad mapped index-signature display suppression probes.
    BroadMappedIndexSignatureDisplay,
    /// Mapped object-literal excess-property value classification probes.
    MappedObjectLiteralExcessValue,
    /// Polymorphic `this` receiver/member compatibility probes for diagnostics.
    PolymorphicThisReceiver,
    /// Class extends index-signature value compatibility probes.
    ClassExtendsIndexValue,
    /// Class implements index-signature value compatibility probes.
    ClassImplementsIndexValue,
    /// Class implements whole-type compatibility probes.
    ClassImplementsWholeType,
    /// Class static-side compatibility probes for TS2417 diagnostics.
    ClassStaticSide,
    /// Interface heritage index-signature value compatibility probes.
    InterfaceHeritageIndexValue,
    /// Interface heritage generic-method specialization probes.
    InterfaceHeritageGenericMethod,
    /// Interface heritage property-vs-index compatibility probes.
    InterfaceHeritagePropertyIndex,
    /// JSDoc heritage object constraint property compatibility probes.
    JsdocHeritageConstraint,
    /// Missing-property read compatibility probes for assignability diagnostics.
    MissingPropertyRead,
    /// Missing-property write compatibility probes for assignability diagnostics.
    MissingPropertyWrite,
    /// Evaluated remapped mapped source compatibility before missing-property reporting.
    ConcreteRemappedMappedMissingProperty,
    /// Exact-optional source-member filtering probes for assignability diagnostics.
    ExactOptionalSourceFilter,
    /// Union excess-property fallback required-property compatibility probes.
    UnionExcessRequiredProperty,
    /// Array-literal contextual collapse override compatibility probes.
    ArrayLiteralContextualCollapse,
    /// JSX construct-return render fallback required-property compatibility probes.
    JsxRenderFallback,
    /// Object-literal mapped contextual property key compatibility probes.
    ObjectLiteralMappedContextualKey,
    /// Object-literal computed-key routing probes for index-signature buckets.
    ObjectLiteralComputedKey,
    /// Object-literal JSDoc declared-property initializer compatibility probes.
    ObjectLiteralJsdocDeclaredProperty,
    /// Contextual symbol-index value compatibility for object-literal diagnostics.
    ContextualSymbolIndexValue,
    /// `in`-operator left key compatibility against the property-key space.
    InOperatorKey,
    /// `in`-operator RHS primitive-constraint compatibility for TS2638.
    InOperatorPrimitiveConstraint,
    /// Compound-assignment RHS compatibility against the widened LHS type.
    CompoundAssignment,
    /// Deferred generic element-write compatibility against the write target.
    GenericElementWrite,
    /// Element-access receiver display compatibility against declared element type.
    PropertyReceiverElementDisplay,
    /// Element-access receiver display compatibility against declared index value type.
    PropertyReceiverIndexValueDisplay,
    /// Element-access numeric index compatibility for TS7015.
    ElementAccessNumberIndex,
    /// Element-access method suggestion index compatibility.
    ElementAccessMethodSuggestion,
    /// Call diagnostic elaboration mutual compatibility display probe.
    CallElaborationMutual,
    /// Call diagnostic display overlap probe.
    CallDisplayOverlap,
    /// Call checker generator-yield component compatibility probe.
    CallGeneratorYield,
    /// Checker-only `IteratorResult` value requirement compatibility probe.
    IteratorResultValue,
    /// Round-2 contextual call inference substitution constraint probe.
    Round2ContextualSubstitution,
    /// Generic constructor inference primitive constraint compatibility probe.
    ConstructorInferenceConstraint,
    /// Call checker adapter default compatibility probe.
    CallAdapterCompatibility,
    /// Call checker adapter lazy-resolution identity fallback probe.
    CallAdapterIdentity,
    /// Overload implementation parameter-surface compatibility probe.
    OverloadImplementationParameter,
    /// Indexed-access arithmetic operand compatibility against `number`.
    BinaryArithmeticNumber,
    /// Private member access object/declaration compatibility probe.
    PrivateMemberAccess,
    /// Function type contextual/recovery compatibility probe.
    FunctionTypeCompatibility,
    /// Namespace-module property mismatch downgrade compatibility probe.
    NamespacePropertyMismatch,
    /// Satisfies expression: `expr satisfies T`
    Satisfies,
    /// Bivariant callback assignment where function parameter types are checked bivariantly.
    BivariantCallbacks,
    /// `using`/`await using` initializer compatibility against the global
    /// `Disposable`/`AsyncDisposable` interface for TS2850 diagnostics.
    Disposable,
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
    /// Diagnostic/tracing context and relation-mode selector.
    pub kind: RelationKind,
    /// Requested excess-property policy for boundary-level property analysis.
    pub excess_property_mode: ExcessPropertyMode,
    /// Requested missing-property policy for boundary-level property analysis.
    pub missing_property_mode: MissingPropertyMode,
    /// Fresh object literal marker for excess-property classification.
    pub source_is_fresh: bool,
    /// Allow targeted erased-signature retry for interface property compatibility.
    pub allow_erased_generic_signature_retry: bool,
    /// Overload-resolution subtype pass (tsc `chooseOverload` with
    /// `subtypeRelation`): an `any` source is not related to non-`any`/
    /// non-`unknown` targets, at every nesting level. The boundary maps this
    /// to the solver's `AnySourceNotRelated` propagation mode, which also
    /// partitions the relation caches.
    pub overload_subtype_pass: bool,
    /// The caller consumes only `RelationOutcome::related` (plus the overflow
    /// flags). The boundary runs the identical decision pass but skips the
    /// structured failure analysis (`explain_failure`, weak-union detection)
    /// and property classification, which would otherwise re-traverse the
    /// failing relation graph just to populate fields the caller drops.
    pub decision_only: bool,
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
            overload_subtype_pass: false,
            decision_only: false,
        }
    }

    pub(crate) const fn assign(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Assign)
    }
    pub(crate) const fn assignability_reason(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::AssignabilityReason)
    }
    pub(crate) const fn type_comparability(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::TypeComparability)
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

    pub(crate) const fn callable_union_return(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallableUnionReturn)
    }

    pub(crate) const fn callable_union_parameter(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallableUnionParameter)
    }

    pub(crate) const fn type_predicate_parameter(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::TypePredicateParameter)
    }

    pub(crate) const fn generic_argument_suppression(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::GenericArgumentSuppression)
    }

    pub(crate) const fn jsx_props(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsxProps)
    }

    pub(crate) const fn jsx_children(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsxChildren)
    }

    pub(crate) const fn jsx_element_type(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsxElementType)
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

    pub(crate) const fn computed_enum_member(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ComputedEnumMember)
    }

    pub(crate) const fn numeric_enum_assignment(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::NumericEnumAssignment)
    }

    pub(crate) const fn type_parameter_default(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::TypeParameterDefault)
    }

    pub(crate) const fn index_signature(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::IndexSignature)
    }

    pub(crate) const fn decorator_callee(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::DecoratorCallee)
    }

    pub(crate) const fn jsdoc_type_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsdocTypeConstraint)
    }

    pub(crate) const fn explicit_alias_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ExplicitAliasConstraint)
    }

    pub(crate) const fn array_like_constraint_element(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ArrayLikeConstraintElement)
    }

    pub(crate) const fn merged_interface_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::MergedInterfaceConstraint)
    }

    pub(crate) const fn recursive_heritage_property(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::RecursiveHeritageProperty)
    }

    pub(crate) const fn union_constraint_member(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::UnionConstraintMember)
    }

    pub(crate) const fn syntax_instantiated_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::SyntaxInstantiatedConstraint)
    }

    pub(crate) const fn type_arg_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::TypeArgConstraint)
    }

    pub(crate) const fn mapped_key_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::MappedKeyConstraint)
    }

    pub(crate) const fn indexed_access_constraint_key(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::IndexedAccessConstraintKey)
    }

    pub(crate) const fn indexed_access_key_space(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::IndexedAccessKeySpace)
    }

    pub(crate) const fn conditional_constraint_component(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ConditionalConstraintComponent)
    }

    pub(crate) const fn conditional_true_base_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ConditionalTrueBaseConstraint)
    }

    pub(crate) const fn conditional_true_branch_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(
            source,
            target,
            RelationKind::ConditionalTrueBranchConstraint,
        )
    }

    pub(crate) const fn required_mapped_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::RequiredMappedConstraint)
    }

    pub(crate) const fn infer_result_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::InferResultConstraint)
    }

    pub(crate) const fn generic_constraint_property(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::GenericConstraintProperty)
    }

    pub(crate) const fn property_index_key(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::PropertyIndexKey)
    }

    pub(crate) const fn nullish_error_target(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::NullishErrorTarget)
    }

    pub(crate) const fn duplicate_identifier(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::DuplicateIdentifier)
    }

    pub(crate) const fn variable_initializer(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::VariableInitializer)
    }

    pub(crate) const fn identifier_binding_default(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::IdentifierBindingDefault)
    }

    pub(crate) const fn keyof_diagnostic_suppression(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::KeyofDiagnosticSuppression)
    }

    pub(crate) const fn diagnostic_source_narrowing(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::DiagnosticSourceNarrowing)
    }

    pub(crate) const fn diagnostic_overlap(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::DiagnosticOverlap)
    }

    pub(crate) const fn broad_mapped_index_signature_display(
        source: TypeId,
        target: TypeId,
    ) -> Self {
        Self::new(
            source,
            target,
            RelationKind::BroadMappedIndexSignatureDisplay,
        )
    }

    pub(crate) const fn mapped_object_literal_excess_value(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::MappedObjectLiteralExcessValue)
    }

    pub(crate) const fn polymorphic_this_receiver(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::PolymorphicThisReceiver)
    }

    pub(crate) const fn class_extends_index_value(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ClassExtendsIndexValue)
    }

    pub(crate) const fn class_implements_index_value(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ClassImplementsIndexValue)
    }

    pub(crate) const fn class_implements_whole_type(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ClassImplementsWholeType)
    }

    pub(crate) const fn class_static_side(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ClassStaticSide)
    }

    pub(crate) const fn interface_heritage_index_value(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::InterfaceHeritageIndexValue)
    }

    pub(crate) const fn interface_heritage_generic_method(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::InterfaceHeritageGenericMethod)
    }

    pub(crate) const fn interface_heritage_property_index(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::InterfaceHeritagePropertyIndex)
    }

    pub(crate) const fn jsdoc_heritage_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsdocHeritageConstraint)
    }

    pub(crate) const fn missing_property_read(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::MissingPropertyRead)
    }

    pub(crate) const fn missing_property_write(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::MissingPropertyWrite)
    }

    pub(crate) const fn concrete_remapped_mapped_missing_property(
        source: TypeId,
        target: TypeId,
    ) -> Self {
        Self::new(
            source,
            target,
            RelationKind::ConcreteRemappedMappedMissingProperty,
        )
    }

    pub(crate) const fn exact_optional_source_filter(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ExactOptionalSourceFilter)
    }

    pub(crate) const fn union_excess_required_property(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::UnionExcessRequiredProperty)
    }

    pub(crate) const fn array_literal_contextual_collapse(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ArrayLiteralContextualCollapse)
    }

    pub(crate) const fn jsx_render_fallback(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::JsxRenderFallback)
    }

    pub(crate) const fn object_literal_mapped_contextual_key(
        source: TypeId,
        target: TypeId,
    ) -> Self {
        Self::new(
            source,
            target,
            RelationKind::ObjectLiteralMappedContextualKey,
        )
    }

    pub(crate) const fn object_literal_computed_key(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ObjectLiteralComputedKey)
    }

    pub(crate) const fn object_literal_jsdoc_declared_property(
        source: TypeId,
        target: TypeId,
    ) -> Self {
        Self::new(
            source,
            target,
            RelationKind::ObjectLiteralJsdocDeclaredProperty,
        )
    }

    pub(crate) const fn contextual_symbol_index_value(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ContextualSymbolIndexValue)
    }

    pub(crate) const fn in_operator_key(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::InOperatorKey)
    }

    pub(crate) const fn in_operator_primitive_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::InOperatorPrimitiveConstraint)
    }

    pub(crate) const fn compound_assignment(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CompoundAssignment)
    }

    pub(crate) const fn generic_element_write(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::GenericElementWrite)
    }

    pub(crate) const fn property_receiver_element_display(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::PropertyReceiverElementDisplay)
    }

    pub(crate) const fn property_receiver_index_value_display(
        source: TypeId,
        target: TypeId,
    ) -> Self {
        Self::new(
            source,
            target,
            RelationKind::PropertyReceiverIndexValueDisplay,
        )
    }

    pub(crate) const fn element_access_number_index(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ElementAccessNumberIndex)
    }

    pub(crate) const fn element_access_method_suggestion(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ElementAccessMethodSuggestion)
    }

    pub(crate) const fn call_elaboration_mutual(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallElaborationMutual)
    }

    pub(crate) const fn call_display_overlap(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallDisplayOverlap)
    }

    pub(crate) const fn call_generator_yield(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallGeneratorYield)
    }

    pub(crate) const fn iterator_result_value(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::IteratorResultValue)
    }

    pub(crate) const fn round2_contextual_substitution(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Round2ContextualSubstitution)
    }

    pub(crate) const fn constructor_inference_constraint(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::ConstructorInferenceConstraint)
    }

    pub(crate) const fn call_adapter_compatibility(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallAdapterCompatibility)
    }

    pub(crate) const fn call_adapter_identity(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallAdapterIdentity)
    }

    pub(crate) const fn overload_implementation_parameter(source: TypeId, target: TypeId) -> Self {
        Self::new(
            source,
            target,
            RelationKind::OverloadImplementationParameter,
        )
    }

    pub(crate) const fn binary_arithmetic_number(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::BinaryArithmeticNumber)
    }

    pub(crate) const fn private_member_access(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::PrivateMemberAccess)
    }

    pub(crate) const fn function_type_compatibility(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::FunctionTypeCompatibility)
    }

    pub(crate) const fn namespace_property_mismatch(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::NamespacePropertyMismatch)
    }

    pub(crate) const fn bivariant_callbacks(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::BivariantCallbacks)
    }

    pub(crate) const fn disposable(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Disposable)
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

    pub(crate) const fn requires_property_classification(&self) -> bool {
        let checks_excess_properties = match self.excess_property_mode {
            ExcessPropertyMode::Skip => false,
            ExcessPropertyMode::Check | ExcessPropertyMode::CheckExplicitOnly => true,
        };
        let reports_missing_properties = match self.missing_property_mode {
            MissingPropertyMode::Report => true,
            MissingPropertyMode::Suppress => false,
        };

        self.source_is_fresh || checks_excess_properties || reports_missing_properties
    }

    pub(crate) fn solver_relation_policy(
        &self,
        base_flags: u16,
    ) -> (tsz_solver::relations::relation_queries::RelationKind, u16) {
        let mut flags = base_flags;
        if self.allow_erased_generic_signature_retry {
            flags |= tsz_solver::RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY.bits() as u16;
        }

        if self.kind == RelationKind::BivariantCallbacks {
            (
                tsz_solver::relations::relation_queries::RelationKind::AssignableBivariantCallbacks,
                flags & !(tsz_solver::RelationFlags::STRICT_FUNCTION_TYPES.bits() as u16),
            )
        } else {
            (
                tsz_solver::relations::relation_queries::RelationKind::Assignable,
                flags,
            )
        }
    }

    /// Memo key for the stamp-guarded assignability failure-analysis memo
    /// (issue #13243), or `None` when this request is not memo-eligible.
    ///
    /// Eligible requests run the canonical reason-collecting assignable
    /// policy: not decision-only (decision-only callers stay reason-free,
    /// #13213), not the overload subtype pass (its `any`-propagation mode
    /// changes the relation outside the packed flags), and resolving to the
    /// plain `Assignable` solver kind. The erased-generic-retry bit is part
    /// of the solver flags and therefore of the key.
    pub(crate) fn failure_memo_key(
        &self,
        base_flags: u16,
        sound_mode: bool,
    ) -> Option<crate::context::AssignabilityFailureKey> {
        if self.decision_only || self.overload_subtype_pass {
            return None;
        }
        let (kind, flags) = self.solver_relation_policy(base_flags);
        if kind != tsz_solver::relations::relation_queries::RelationKind::Assignable {
            return None;
        }
        Some((self.source, self.target, flags, sound_mode))
    }

    /// Allow a failed generic-signature inference to retry with erased signatures.
    pub(crate) const fn with_erased_generic_signature_retry(mut self) -> Self {
        self.allow_erased_generic_signature_retry = true;
        self
    }

    /// Run this relation under the overload-resolution subtype pass, where an
    /// `any` source is not related to non-`any`/`unknown` targets at every
    /// nesting level (tsc `chooseOverload` with `subtypeRelation`).
    pub(crate) const fn with_overload_subtype_pass(mut self) -> Self {
        self.overload_subtype_pass = true;
        self
    }

    /// Mark this request as decision-only: the caller reads only
    /// `RelationOutcome::related` (and the overflow flags), so the boundary
    /// must not spend time deriving structured failure reasons, weak-union
    /// detection, or property classification on failure. The decision itself
    /// is byte-identical to the full request.
    pub(crate) const fn with_decision_only(mut self) -> Self {
        self.decision_only = true;
        self
    }
}
