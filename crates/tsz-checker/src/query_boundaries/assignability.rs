use tsz_solver::{QueryDatabase, SubtypeFailureReason, TypeDatabase, TypeId};

pub(crate) use super::common::{contains_type_parameters, object_shape_for_type};

// ---------------------------------------------------------------------------
// RelationRequest: unified policy descriptor for relation queries
// ---------------------------------------------------------------------------

/// The kind of relation being checked. Different kinds imply different
/// default policies for freshness, excess properties, and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationKind {
    /// Variable/parameter assignment: `const x: T = expr`
    Assign,
    /// Function call argument: `fn(expr)` where param expects T
    CallArg,
    /// Return statement: `return expr` where function returns T
    Return,
    /// JSX props: `<Comp prop={expr} />`
    JsxProps,
    /// Destructuring: `const { a, b } = expr`
    Destructuring,
    /// Satisfies expression: `expr satisfies T`
    Satisfies,
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
    pub source: TypeId,
    pub target: TypeId,
    pub kind: RelationKind,
    pub excess_property_mode: ExcessPropertyMode,
    pub missing_property_mode: MissingPropertyMode,
    /// Whether the source is a fresh object literal.
    pub source_is_fresh: bool,
}

impl RelationRequest {
    fn new(source: TypeId, target: TypeId, kind: RelationKind) -> Self {
        Self {
            source,
            target,
            kind,
            excess_property_mode: ExcessPropertyMode::Skip,
            missing_property_mode: MissingPropertyMode::Report,
            source_is_fresh: false,
        }
    }

    pub(crate) fn assign(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Assign)
    }

    pub(crate) fn call_arg(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::CallArg)
    }

    pub(crate) fn return_stmt(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Return)
    }

    pub(crate) fn satisfies(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Satisfies)
    }

    pub(crate) fn destructuring(source: TypeId, target: TypeId) -> Self {
        Self::new(source, target, RelationKind::Destructuring)
    }

    /// Mark the source as a fresh object literal, enabling EPC.
    pub(crate) fn with_fresh_source(mut self) -> Self {
        self.source_is_fresh = true;
        self.excess_property_mode = ExcessPropertyMode::Check;
        self
    }

    /// Mark the source as a spread expression, enabling explicit-only EPC.
    pub(crate) fn with_spread_source(mut self) -> Self {
        self.excess_property_mode = ExcessPropertyMode::CheckExplicitOnly;
        self
    }

    /// Override excess property mode.
    pub(crate) fn with_excess_property_mode(mut self, mode: ExcessPropertyMode) -> Self {
        self.excess_property_mode = mode;
        self
    }

    /// Override missing property mode.
    pub(crate) fn with_missing_property_mode(mut self, mode: MissingPropertyMode) -> Self {
        self.missing_property_mode = mode;
        self
    }
}

// ---------------------------------------------------------------------------
// Existing boundary helpers
// ---------------------------------------------------------------------------

/// Boundary-safe flag constants for relation cache keys.
///
/// Wraps the flag constants from `tsz_solver::RelationCacheKey` without exposing
/// the internal `RelationCacheKey` struct itself. Checker code should use these
/// constants when constructing relation policy flags (e.g., in `pack_relation_flags`).
pub(crate) struct RelationFlags;

impl RelationFlags {
    pub const STRICT_NULL_CHECKS: u16 = tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    pub const STRICT_FUNCTION_TYPES: u16 = tsz_solver::RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    pub const EXACT_OPTIONAL_PROPERTY_TYPES: u16 =
        tsz_solver::RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES;
    pub const NO_UNCHECKED_INDEXED_ACCESS: u16 =
        tsz_solver::RelationCacheKey::FLAG_NO_UNCHECKED_INDEXED_ACCESS;
    pub const NO_ERASE_GENERICS: u16 = tsz_solver::RelationCacheKey::FLAG_NO_ERASE_GENERICS;
    pub const DISABLE_METHOD_BIVARIANCE: u16 =
        tsz_solver::RelationCacheKey::FLAG_DISABLE_METHOD_BIVARIANCE;
}
pub(crate) use tsz_solver::type_queries::{
    AssignabilityEvalKind, ExcessPropertiesKind, get_allowed_keys, get_keyof_type,
    get_string_literal_value, get_union_members, is_keyof_type, is_type_parameter_like,
    keyof_object_properties, map_compound_members,
};

pub(crate) fn classify_for_assignability_eval(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AssignabilityEvalKind {
    tsz_solver::type_queries::classify_for_assignability_eval(db, type_id)
}

pub(crate) fn is_relation_cacheable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    !tsz_solver::type_queries::contains_infer_types_db(db, source)
        && !tsz_solver::type_queries::contains_infer_types_db(db, target)
}

pub(crate) fn contains_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_infer_types_db(db, type_id)
}

pub(crate) fn contains_free_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_free_infer_types(db, type_id)
}

pub(crate) fn contains_any_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_any_type(db, type_id)
}

pub(crate) fn has_recursive_type_parameter_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::visitor::type_param_info(db, type_id).is_some_and(|info| {
        info.constraint.is_some_and(|constraint| {
            tsz_solver::visitor::contains_type_parameter_named_shallow(db, constraint, info.name)
        })
    })
}

pub(crate) fn is_any_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_any_type(db, type_id)
}

pub(crate) fn classify_for_excess_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ExcessPropertiesKind {
    tsz_solver::type_queries::classify_for_excess_properties(db, type_id)
}

/// Perform a fresh subtype check that bypasses the `QueryDatabase` cache.
/// This is needed after generic inference when the cache may contain stale
/// entries from intermediate inference steps.
pub(crate) fn is_fresh_subtype_of(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    tsz_solver::is_subtype_of(db, source, target)
}

pub(crate) fn get_function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_return_type(db, type_id)
}

pub(crate) fn rewrite_function_error_slots_to_any(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::type_queries::rewrite_function_error_slots_to_any(db, type_id)
}

pub(crate) fn replace_function_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    new_return: TypeId,
) -> TypeId {
    tsz_solver::type_queries::replace_function_return_type(db, type_id, new_return)
}

pub(crate) fn erase_function_type_params_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::erase_function_type_params_to_any(db, type_id)
}

pub(crate) fn are_types_overlapping_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags: u16 = 0;
    if strict_null_checks {
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    let policy = tsz_solver::RelationPolicy::from_flags(flags);
    tsz_solver::query_relation_with_resolver(
        db,
        env,
        left,
        right,
        tsz_solver::RelationKind::Overlap,
        policy,
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

pub(crate) fn is_assignable_with_overrides<R: tsz_solver::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::AssignabilityOverrideProvider,
) -> tsz_solver::RelationResult {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
    } = *inputs;
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_overrides(tsz_solver::RelationQueryInputs {
        interner: db.as_type_database(),
        resolver,
        source,
        target,
        kind: tsz_solver::RelationKind::Assignable,
        policy,
        context,
        overrides,
    })
}

/// Like `is_assignable_with_overrides` but skips weak type checks (TS2559).
///
/// This matches tsc's `isTypeAssignableTo` behavior, which does NOT
/// include the weak type check. Used by the flow narrowing guard.
pub(crate) fn is_assignable_no_weak_checks<R: tsz_solver::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::AssignabilityOverrideProvider,
) -> bool {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
    } = *inputs;
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode)
        .with_skip_weak_type_checks(true);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_overrides(tsz_solver::RelationQueryInputs {
        interner: db.as_type_database(),
        resolver,
        source,
        target,
        kind: tsz_solver::RelationKind::Assignable,
        policy,
        context,
        overrides,
    })
    .is_related()
}

#[derive(Clone, Copy)]
pub(crate) struct AssignabilityQueryInputs<'a, R: tsz_solver::TypeResolver> {
    pub db: &'a dyn QueryDatabase,
    pub resolver: &'a R,
    pub source: TypeId,
    pub target: TypeId,
    pub flags: u16,
    pub inheritance_graph: &'a tsz_solver::InheritanceGraph,
    pub sound_mode: bool,
}

pub(crate) fn is_assignable_bivariant_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    sound_mode: bool,
) -> bool {
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::AssignableBivariantCallbacks,
        policy,
        context,
    )
    .is_related()
}

pub(crate) fn is_subtype_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    class_check: Option<&dyn Fn(tsz_solver::SymbolRef) -> bool>,
) -> tsz_solver::RelationResult {
    let policy = tsz_solver::RelationPolicy::from_flags(flags);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::Subtype,
        policy,
        context,
    )
}

pub(crate) fn is_redeclaration_identical_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    sound_mode: bool,
) -> bool {
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::RedeclarationIdentical,
        policy,
        context,
    )
    .is_related()
}

pub(crate) struct AssignabilityFailureAnalysis {
    pub weak_union_violation: bool,
    pub failure_reason: Option<SubtypeFailureReason>,
}

pub(crate) struct AssignabilityGateResult {
    pub related: bool,
    pub analysis: Option<AssignabilityFailureAnalysis>,
}

pub(crate) fn check_assignable_gate_with_overrides<R: tsz_solver::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::AssignabilityOverrideProvider,
    ctx: Option<&crate::context::CheckerContext<'_>>,
    collect_failure_analysis: bool,
) -> AssignabilityGateResult {
    let related = is_assignable_with_overrides(inputs, overrides).is_related();

    if !collect_failure_analysis || related {
        return AssignabilityGateResult {
            related,
            analysis: None,
        };
    }

    let analysis = ctx.map(|ctx| {
        analyze_assignability_failure_with_context(
            inputs.db.as_type_database(),
            ctx,
            inputs.resolver,
            inputs.source,
            inputs.target,
        )
    });

    AssignabilityGateResult { related, analysis }
}

// ---------------------------------------------------------------------------
// RelationOutcome: structured result from executing a RelationRequest
// ---------------------------------------------------------------------------

/// Structured outcome from executing a `RelationRequest` through the
/// canonical boundary.
///
/// Combines the relation result, structured failure classification, and
/// weak-union violation detection into a single response so that callers
/// do not need to issue multiple solver round-trips for the same logical
/// question.
#[derive(Debug)]
pub(crate) struct RelationOutcome {
    /// Whether the relation holds (source is assignable to target).
    pub related: bool,
    /// Whether the solver's recursion depth limit was exceeded during
    /// the relation check. When true, the caller should emit TS2859
    /// ("Excessive complexity comparing types").
    pub depth_exceeded: bool,
    /// Structured failure classification when `related` is false.
    /// Converted from the solver's `SubtypeFailureReason`.
    pub failure: Option<super::relation_types::RelationFailure>,
    /// Whether the failure is a weak-union violation (TS2559).
    /// When true, the checker should emit excess-property diagnostics
    /// instead of the standard assignability error.
    pub weak_union_violation: bool,
    /// Structured property-level classification for object compatibility.
    /// Populated when the request has `source_is_fresh` set and the source/target
    /// are object types. Provides the canonical excess/missing/incompatible lists
    /// so the checker does not need to re-derive them.
    pub property_classification: Option<super::relation_types::PropertyClassification>,
}

/// Execute a `RelationRequest` through the canonical boundary.
///
/// This is the single authoritative entry point for relation queries that
/// need structured failure information. It:
///
/// 1. Runs the assignability check via the solver.
/// 2. When not related, collects a structured failure reason.
/// 3. Detects weak-union violations.
///
/// All policy dimensions (freshness, excess-property mode, missing-property
/// mode) are encoded in the `request`; the boundary translates them to
/// solver-level knobs.
pub(crate) fn execute_relation<R: tsz_solver::TypeResolver>(
    request: &RelationRequest,
    db: &dyn QueryDatabase,
    resolver: &R,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    overrides: &dyn tsz_solver::AssignabilityOverrideProvider,
    ctx: Option<&crate::context::CheckerContext<'_>>,
    sound_mode: bool,
) -> RelationOutcome {
    let inputs = AssignabilityQueryInputs {
        db,
        resolver,
        source: request.source,
        target: request.target,
        flags,
        inheritance_graph,
        sound_mode,
    };

    let relation_result = is_assignable_with_overrides(&inputs, overrides);
    let related = relation_result.is_related();
    let depth_exceeded = relation_result.depth_exceeded;

    if related {
        return RelationOutcome {
            related: true,
            depth_exceeded,
            failure: None,
            weak_union_violation: false,
            property_classification: None,
        };
    }

    // Relation failed — collect structured failure analysis.
    let analysis = ctx.map(|ctx| {
        analyze_assignability_failure_with_context(
            db.as_type_database(),
            ctx,
            resolver,
            request.source,
            request.target,
        )
    });

    let (weak_union_violation, failure) = match analysis {
        Some(a) => (
            a.weak_union_violation,
            a.failure_reason
                .map(super::relation_types::RelationFailure::from_solver_reason),
        ),
        None => (false, None),
    };

    // Always populate property classification when the relation fails.
    // This provides the canonical property-level analysis that callers like
    // `should_skip_weak_union_error` need without re-enumerating properties.
    let property_classification =
        classify_object_properties(db.as_type_database(), request.source, request.target);

    // Suppress ExcessProperty failure when the target has structural features
    // that make EPC inapplicable. This centralizes the policy that was previously
    // duplicated in `analyze_assignability_failure`.
    let failure =
        suppress_excess_property_failure_if_needed(failure, db.as_type_database(), request.target);

    RelationOutcome {
        related: false,
        depth_exceeded,
        failure,
        weak_union_violation,
        property_classification,
    }
}

/// Suppress an `ExcessProperty` failure reason when the target's structure
/// makes EPC inapplicable:
/// 1. Target contains a deferred conditional type → structural mismatch, not EPC.
/// 2. Target intersection has primitive or type-parameter members → EPC skipped.
///
/// This is the canonical boundary-level policy, replacing the checker-local
/// re-analysis that was in `analyze_assignability_failure`.
fn suppress_excess_property_failure_if_needed(
    failure: Option<super::relation_types::RelationFailure>,
    db: &dyn TypeDatabase,
    target: TypeId,
) -> Option<super::relation_types::RelationFailure> {
    use super::common::is_type_parameter_like;

    let is_excess = matches!(
        &failure,
        Some(super::relation_types::RelationFailure::ExcessProperty { .. })
    );
    if !is_excess {
        return failure;
    }

    // Check for deferred conditional members.
    if tsz_solver::has_deferred_conditional_member(db, target) {
        return None;
    }

    // Check for non-EPC intersection members (primitives/type-params).
    if let Some(members) = tsz_solver::type_queries::data::get_intersection_members(db, target)
        && members.iter().any(|member| {
            tsz_solver::is_primitive_type(db, *member) || is_type_parameter_like(db, *member)
        })
    {
        return None;
    }

    failure
}

// ---------------------------------------------------------------------------
// classify_object_properties: canonical property-level classification
// ---------------------------------------------------------------------------

/// Classify properties between source and target object types.
///
/// This is the authoritative boundary function for property-level analysis.
/// It replaces the duplicated property enumeration logic that was previously
/// spread across `state_checking/property.rs` (excess checking) and
/// `assignability_diagnostics.rs` (`should_skip_weak_union_error`).
///
/// Returns `None` when the source or target is not an object type with
/// extractable properties.
pub(crate) fn classify_object_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> Option<super::relation_types::PropertyClassification> {
    use super::common::{
        intersection_members, is_type_parameter_like, object_shape_for_type, union_members,
    };
    use super::relation_types::PropertyClassification;

    // Cannot classify if target is a type parameter.
    if is_type_parameter_like(db, target) {
        return Some(PropertyClassification {
            target_is_type_parameter: true,
            ..Default::default()
        });
    }

    let source_shape = object_shape_for_type(db, source)?;
    let source_props = source_shape.properties.as_slice();

    if source_props.is_empty() {
        return Some(PropertyClassification::default());
    }

    // Collect all target property names from all branches (union/intersection/object).
    let target_property_names = collect_target_property_names(db, target);

    let mut classification = PropertyClassification::default();

    // Check for index signatures, empty object targets, and special shapes.
    if let Some(target_shape) = object_shape_for_type(db, target) {
        if target_shape.string_index.is_some() {
            classification.target_has_index_signature = true;
        }
        if target_shape.properties.is_empty()
            && target_shape.string_index.is_none()
            && target_shape.number_index.is_none()
        {
            classification.target_is_empty_object = true;
        }
        if target_shape.number_index.is_some() {
            classification.target_has_index_signature = true;
            classification.target_has_number_index = true;
        }
        if is_global_object_or_function_shape(db, &target_shape) {
            classification.target_is_global_object_or_function = true;
        }
    } else if let Some(members) = union_members(db, target) {
        // For unions, check if any member has index signatures or is special.
        for &member in &members {
            if let Some(shape) = object_shape_for_type(db, member) {
                if shape.string_index.is_some() {
                    classification.target_has_index_signature = true;
                }
                if shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
                {
                    classification.target_is_empty_object = true;
                }
                if shape.number_index.is_some() {
                    classification.target_has_number_index = true;
                }
                if is_global_object_or_function_shape(db, &shape) {
                    classification.target_is_global_object_or_function = true;
                }
            }
            if member == TypeId::OBJECT {
                classification.target_is_empty_object = true;
            }
        }
    } else if let Some(members) = intersection_members(db, target) {
        for &member in members.iter() {
            if is_type_parameter_like(db, member) {
                classification.target_is_type_parameter = true;
            }
            if let Some(shape) = object_shape_for_type(db, member) {
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    classification.target_has_index_signature = true;
                }
                if shape.number_index.is_some() {
                    classification.target_has_number_index = true;
                }
            }
        }
    }

    // Collect target properties for compatibility checking.
    let target_props = collect_target_properties(db, target);

    // Classify each source property and check compatibility of matching ones.
    let mut all_matching_compatible = true;
    let mut matching_props = Vec::new();

    for source_prop in source_props {
        let name_str = db.resolve_atom_ref(source_prop.name);
        if target_property_names.contains(name_str.as_ref()) {
            // Property exists in target — check type compatibility.
            if let Some(target_prop_type) = target_props.get(name_str.as_ref()).copied() {
                // Account for optional properties: target `prop?: T` accepts `T | undefined`.
                let effective_target_type = target_prop_type;
                if !tsz_solver::is_subtype_of(db, source_prop.type_id, effective_target_type) {
                    all_matching_compatible = false;
                    classification.incompatible_properties.push((
                        source_prop.name,
                        source_prop.type_id,
                        effective_target_type,
                    ));
                } else {
                    matching_props.push(source_prop.clone());
                }
            } else {
                matching_props.push(source_prop.clone());
            }
        } else if !classification.target_has_index_signature
            && !classification.target_is_empty_object
            && !classification.target_is_global_object_or_function
            && !classification.target_is_type_parameter
        {
            classification.excess_properties.push(source_prop.name);
        }
    }

    classification.all_matching_compatible = all_matching_compatible;

    // When there are excess properties and all matching ones are compatible,
    // check if a trimmed source (only matching properties) would be assignable.
    // This catches structural incompatibilities beyond property names (e.g.,
    // deferred conditional types in the target).
    if !classification.excess_properties.is_empty() && all_matching_compatible {
        let trimmed_source = db.object(matching_props);
        classification.trimmed_source_assignable =
            tsz_solver::is_subtype_of(db, trimmed_source, target);
    }

    Some(classification)
}

/// Collect all property names and their types from a target type.
///
/// Returns a map from property name to type for type compatibility checking.
/// For unions, uses the type from the first member that has the property.
/// For intersections, uses the type from the first member that has the property.
fn collect_target_properties(
    db: &dyn TypeDatabase,
    target: TypeId,
) -> std::collections::HashMap<String, TypeId> {
    use super::common::{intersection_members, object_shape_for_type, union_members};
    let mut props = std::collections::HashMap::new();

    if let Some(shape) = object_shape_for_type(db, target) {
        for prop in shape.properties.iter() {
            let name = db.resolve_atom(prop.name);
            props.entry(name).or_insert(prop.type_id);
        }
    }

    if let Some(members) = union_members(db, target) {
        for &member in &members {
            if let Some(shape) = object_shape_for_type(db, member) {
                for prop in shape.properties.iter() {
                    let name = db.resolve_atom(prop.name);
                    props.entry(name).or_insert(prop.type_id);
                }
            }
        }
    }

    if let Some(members) = intersection_members(db, target) {
        for &member in members.iter() {
            if let Some(shape) = object_shape_for_type(db, member) {
                for prop in shape.properties.iter() {
                    let name = db.resolve_atom(prop.name);
                    props.entry(name).or_insert(prop.type_id);
                }
            }
        }
    }

    props
}

/// Collect all property names from a target type (handling unions/intersections).
fn collect_target_property_names(
    db: &dyn TypeDatabase,
    target: TypeId,
) -> std::collections::HashSet<String> {
    use super::common::{intersection_members, object_shape_for_type, union_members};
    let mut names = std::collections::HashSet::new();

    if let Some(shape) = object_shape_for_type(db, target) {
        for prop in shape.properties.iter() {
            names.insert(db.resolve_atom(prop.name));
        }
    }

    if let Some(members) = union_members(db, target) {
        for &member in &members {
            if let Some(shape) = object_shape_for_type(db, member) {
                for prop in shape.properties.iter() {
                    names.insert(db.resolve_atom(prop.name));
                }
            }
        }
    }

    if let Some(members) = intersection_members(db, target) {
        for &member in members.iter() {
            if let Some(shape) = object_shape_for_type(db, member) {
                for prop in shape.properties.iter() {
                    names.insert(db.resolve_atom(prop.name));
                }
            }
        }
    }

    names
}

/// Check if an object shape represents the global Object or Function interface.
///
/// These types have only inherited method properties and should suppress
/// excess property checking. This is the canonical boundary-level check,
/// replacing the checker-local `is_global_object_or_function_shape`.
///
/// Public boundary variant for checker code that needs to check a pre-resolved shape.
pub(crate) fn is_global_object_or_function_shape_boundary(
    db: &dyn TypeDatabase,
    shape: &tsz_solver::ObjectShape,
) -> bool {
    is_global_object_or_function_shape(db, shape)
}

fn is_global_object_or_function_shape(
    db: &dyn TypeDatabase,
    shape: &tsz_solver::ObjectShape,
) -> bool {
    static OBJECT_PROTO: &[&str] = &[
        "constructor",
        "toString",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
    ];
    static FUNCTION_PROTO: &[&str] = &[
        "apply",
        "call",
        "bind",
        "toString",
        "length",
        "arguments",
        "caller",
        "prototype",
        "constructor",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
    ];

    if shape.properties.is_empty() {
        return false;
    }

    shape.properties.iter().all(|prop| {
        let name = db.resolve_atom_ref(prop.name);
        OBJECT_PROTO.contains(&name.as_ref()) || FUNCTION_PROTO.contains(&name.as_ref())
    })
}

pub(crate) fn analyze_assignability_failure_with_context<R: tsz_solver::TypeResolver>(
    db: &dyn TypeDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> AssignabilityFailureAnalysis {
    let analysis = tsz_solver::analyze_assignability_failure_with_resolver(
        db,
        resolver,
        source,
        target,
        |checker| ctx.configure_compat_checker(checker),
    );
    AssignabilityFailureAnalysis {
        weak_union_violation: analysis.weak_union_violation,
        failure_reason: analysis.failure_reason,
    }
}

/// Variance-aware Application-to-Application assignability check.
///
/// When both source and target are Applications with the same base type,
/// uses computed variance to check arguments without structural expansion.
/// Must be called BEFORE types are evaluated/expanded.
///
/// Returns `Some(true/false)` if conclusive, `None` to fall through.
pub(crate) fn check_application_variance_assignability<R: tsz_solver::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
) -> Option<bool> {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
    } = *inputs;
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::check_application_variance(
        db.as_type_database(),
        resolver,
        Some(db),
        source,
        target,
        policy,
        context,
    )
}
