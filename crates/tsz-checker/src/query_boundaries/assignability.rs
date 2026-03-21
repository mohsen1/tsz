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

pub(crate) fn contains_any_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_any_type(db, type_id)
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
    let related = is_assignable_with_overrides(inputs, overrides);

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
