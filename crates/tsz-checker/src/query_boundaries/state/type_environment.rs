use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{
    CallSignature, CallableShape, MappedTypeId, ObjectShape, ParamInfo, PropertyInfo, TypeId,
    TypeParamInfo, TypeParamOrigin,
};

pub(crate) use super::super::common::{
    TypeSubstitution, callable_shape_for_type as callable_shape,
    function_shape_for_type as function_shape, is_generic_type, lazy_def_id,
    object_shape_for_type as object_shape,
};
pub(crate) use super::super::generic_instantiation::instantiate_type;
pub(crate) use tsz_solver::type_queries::{
    MappedConstraintKind, PropertyAccessResolutionKind, TypeResolutionKind,
};

pub(crate) const fn enum_namespace_member_property(
    name: Atom,
    type_id: TypeId,
    declaration_order: u32,
) -> PropertyInfo {
    let mut property = PropertyInfo::new(name, type_id);
    property.readonly = true;
    property.declaration_order = declaration_order;
    property
}

pub(crate) const fn mapped_property(
    name: Atom,
    type_id: TypeId,
    optional: bool,
    readonly: bool,
) -> PropertyInfo {
    let mut property = PropertyInfo::new(name, type_id);
    property.optional = optional;
    property.readonly = readonly;
    property
}

pub(crate) const fn global_this_surface_property(
    name: Atom,
    type_id: TypeId,
    parent_id: Option<SymbolId>,
    readonly: bool,
    declaration_order: u32,
) -> PropertyInfo {
    let mut property = PropertyInfo::new(name, type_id);
    property.write_type = type_id;
    property.readonly = readonly;
    property.parent_id = parent_id;
    property.declaration_order = declaration_order;
    property
}

pub(crate) const fn js_expando_property(
    name: Atom,
    type_id: TypeId,
    parent_id: SymbolId,
    declaration_order: u32,
) -> PropertyInfo {
    let mut property = PropertyInfo::new(name, type_id);
    property.write_type = type_id;
    property.parent_id = Some(parent_id);
    property.declaration_order = declaration_order;
    property
}

pub(crate) fn global_this_surface_object(
    db: &dyn QueryDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    // The solver factory owns the `GLOBAL_THIS_SURFACE` flag policy so checker
    // boundaries do not name `ObjectFlags` directly.
    db.factory().global_this_surface_object(properties)
}

pub(crate) fn mapped_result_object(db: &dyn TypeDatabase, properties: Vec<PropertyInfo>) -> TypeId {
    db.object(properties)
}

pub(crate) fn object_with_expando_properties(
    db: &dyn TypeDatabase,
    base_shape: &ObjectShape,
    properties: Vec<PropertyInfo>,
    fallback_symbol: SymbolId,
) -> TypeId {
    db.object_with_index(ObjectShape {
        flags: base_shape.flags,
        properties,
        string_index: base_shape.string_index,
        number_index: base_shape.number_index,
        symbol_index: base_shape.symbol_index,
        symbol: base_shape.symbol.or(Some(fallback_symbol)),
    })
}

pub(crate) fn callable_shape_for_expando_base(
    db: &dyn TypeDatabase,
    base_type: TypeId,
    symbol: SymbolId,
) -> Option<(CallableShape, u32)> {
    if let Some(shape) = callable_shape(db, base_type) {
        return Some(((*shape).clone(), shape.properties.len() as u32));
    }

    let function_shape = function_shape(db, base_type)?;
    let signature = super::super::signature_building::call_signature(
        function_shape.type_params.clone(),
        function_shape.params.clone(),
        function_shape.this_type,
        function_shape.return_type,
        function_shape.type_predicate,
        function_shape.is_method,
    );
    let callable_shape = CallableShape {
        call_signatures: if function_shape.is_constructor {
            Vec::new()
        } else {
            vec![signature.clone()]
        },
        construct_signatures: if function_shape.is_constructor {
            vec![signature]
        } else {
            Vec::new()
        },
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(symbol),
        is_abstract: false,
    };
    Some((callable_shape, 0))
}

pub(crate) fn callable_with_appended_properties(
    db: &dyn TypeDatabase,
    mut shape: CallableShape,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    shape.properties.extend(properties);
    db.callable(shape)
}

pub(crate) fn callable_with_instantiated_signatures(
    db: &dyn TypeDatabase,
    shape: &CallableShape,
    call_signatures: Option<Vec<CallSignature>>,
    construct_signatures: Option<Vec<CallSignature>>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: call_signatures.unwrap_or_else(|| shape.call_signatures.clone()),
        construct_signatures: construct_signatures
            .unwrap_or_else(|| shape.construct_signatures.clone()),
        properties: shape.properties.clone(),
        string_index: shape.string_index,
        number_index: shape.number_index,
        symbol: shape.symbol,
        is_abstract: shape.is_abstract,
    })
}

pub(crate) fn instantiate_type_environment_signatures(
    db: &dyn QueryDatabase,
    signatures: &[CallSignature],
    type_args: &[TypeId],
) -> Option<Vec<CallSignature>> {
    let mut changed = false;
    let signatures = signatures
        .iter()
        .map(|signature| {
            if signature.type_params.len() == type_args.len() && !signature.type_params.is_empty() {
                changed = true;
                instantiate_type_environment_signature(db, signature, type_args)
            } else {
                signature.clone()
            }
        })
        .collect();

    changed.then_some(signatures)
}

fn instantiate_type_environment_signature(
    db: &dyn QueryDatabase,
    signature: &CallSignature,
    type_args: &[TypeId],
) -> CallSignature {
    let substitution = TypeSubstitution::from_signature_args(
        db.as_type_database(),
        &signature.type_params,
        type_args,
    );
    let params = signature
        .params
        .iter()
        .map(|param| {
            super::super::signature_building::param_info(
                param.name,
                instantiate_type(db, param.type_id, &substitution),
                param.optional,
                param.rest,
            )
        })
        .collect();

    super::super::signature_building::call_signature(
        Vec::new(),
        params,
        signature
            .this_type
            .map(|type_id| instantiate_type(db, type_id, &substitution)),
        instantiate_type(db, signature.return_type, &substitution),
        signature.type_predicate,
        signature.is_method,
    )
}

pub(crate) fn unconstrained_type_environment_type_param(
    db: &dyn TypeDatabase,
    name: Atom,
    origin: TypeParamOrigin,
) -> TypeId {
    let info = super::super::signature_building::type_param_info(name, None, None, false, origin);
    db.type_param(info)
}

pub(crate) const fn provisional_class_expression_type_param(name: Atom) -> TypeParamInfo {
    TypeParamInfo::simple(name)
}

pub(crate) fn provisional_class_expression_constructor_type(
    db: &dyn TypeDatabase,
    type_params: Vec<TypeParamInfo>,
) -> TypeId {
    let construct_signature = super::super::signature_building::call_signature(
        type_params,
        Vec::<ParamInfo>::new(),
        None,
        TypeId::ANY,
        None,
        false,
    );
    db.callable(CallableShape {
        construct_signatures: vec![construct_signature],
        call_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    })
}

/// Collect every unique concrete `Application` of `def_id` reachable from
/// `type_id` (arguments free of type parameters). Used by the TS2589
/// convergence probe to re-examine a recursive alias's residual
/// self-applications.
pub(crate) fn collect_concrete_applications_with_def(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    def_id: tsz_solver::def::DefId,
) -> Vec<TypeId> {
    tsz_solver::visitor::collect_concrete_applications_with_def(db, type_id, def_id)
}

/// Like [`collect_concrete_applications_with_def`], but only collects residual
/// self-applications reachable through eager positions, pruning the
/// structural-deferral boundaries (object/callable member values, function
/// signatures, mapped templates) `tsc` never eagerly instantiates. Used by the
/// use-site TS2589 convergence check so a growing residual `tsc` defers is not
/// mistaken for an infinite instantiation.
pub(crate) fn collect_eager_concrete_applications_with_def(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    def_id: tsz_solver::def::DefId,
) -> Vec<TypeId> {
    tsz_solver::visitor::collect_eager_concrete_applications_with_def(db, type_id, def_id)
}

/// Total structural weight of the arguments of a concrete `Application` of
/// `def_id`, or `None` if `type_id` is not such an application. The shared
/// recursion-growth metric used to decide whether a residual self-application
/// is converging (shrinking) or diverging.
pub(crate) fn self_application_arg_weight<R: tsz_solver::relations::subtype::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    def_id: tsz_solver::def::DefId,
) -> Option<u64> {
    tsz_solver::visitor::self_application_arg_weight(db, resolver, type_id, def_id)
}

/// Thin wrapper around `tsz_solver::computation::TypeEvaluator`.
///
/// Evaluates a complex type (conditional, mapped, index access, etc.) using
/// the provided `TypeResolver` to resolve lazy references. This delegates to
/// `TypeEvaluator::with_resolver` + `evaluate` in a single call.
pub(crate) fn evaluate_type_with_resolver<R: tsz_solver::relations::subtype::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator =
        tsz_solver::computation::TypeEvaluator::with_resolver(db.as_type_database(), resolver)
            .with_query_db(db);
    evaluator.evaluate(type_id)
}

pub(crate) fn application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

pub(crate) fn for_each_direct_referenced_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    f: impl FnMut(TypeId),
) {
    tsz_solver::visitor::for_each_child_by_id(db, type_id, f);
}

pub(crate) fn mapped_type_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<MappedTypeId> {
    tsz_solver::type_queries::get_mapped_type_id(db, type_id)
}

pub(crate) fn index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

pub(crate) fn classify_mapped_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> MappedConstraintKind {
    tsz_solver::type_queries::classify_mapped_constraint(db, type_id)
}

pub(crate) fn classify_for_type_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeResolutionKind {
    tsz_solver::type_queries::classify_for_type_resolution(db, type_id)
}

pub(crate) fn classify_for_property_access_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyAccessResolutionKind {
    tsz_solver::type_queries::classify_for_property_access_resolution(db, type_id)
}

pub(crate) fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ConditionalType>> {
    tsz_solver::type_queries::get_conditional_type(db, type_id)
}

pub(crate) fn is_union_or_intersection(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_union_type(db, type_id)
        || tsz_solver::type_queries::is_intersection_type(db, type_id)
}

pub(crate) use tsz_solver::type_queries::MappedSourceKind;

/// Classify a mapped-type source for array/tuple preservation decisions.
///
/// The checker uses this to decide whether to delegate to the solver's
/// tuple/array mapped evaluation or expand as a plain object.
pub(crate) fn classify_mapped_source(db: &dyn TypeDatabase, source: TypeId) -> MappedSourceKind {
    tsz_solver::type_queries::classify_mapped_source(db, source)
}

/// Check if a mapped type's `as` clause is identity-preserving.
pub(crate) fn is_identity_name_mapping(
    db: &dyn TypeDatabase,
    mapped: &tsz_solver::MappedType,
) -> bool {
    tsz_solver::type_queries::is_identity_name_mapping(db, mapped)
}

pub(crate) fn literal_string(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::visitor::literal_string(db, type_id)
}

pub(crate) fn union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::type_queries::TypeIdList> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

/// Compute modifier values for a mapped-type property.
pub(crate) const fn compute_mapped_modifiers(
    mapped: &tsz_solver::MappedType,
    is_homomorphic: bool,
    source_optional: bool,
    source_readonly: bool,
) -> (bool, bool) {
    tsz_solver::type_queries::compute_mapped_modifiers(
        mapped,
        is_homomorphic,
        source_optional,
        source_readonly,
    )
}

/// Merge mapped properties whose `as` clause remaps multiple source keys onto
/// the same output name, mirroring tsc's `resolveMappedTypeMembers`: the value
/// contributions union while the first source key's modifiers are kept.
pub(crate) fn merge_colliding_mapped_properties(
    db: &dyn TypeDatabase,
    properties: &mut Vec<tsz_solver::PropertyInfo>,
) {
    tsz_solver::type_queries::merge_colliding_mapped_properties(db, properties);
}

/// Collect source property info for a homomorphic mapped type.
pub(crate) fn collect_homomorphic_source_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> rustc_hash::FxHashMap<tsz_common::Atom, (bool, bool, TypeId)> {
    tsz_solver::type_queries::collect_homomorphic_source_properties(db, source)
}

/// Collect ordered source properties for a homomorphic mapped type.
pub(crate) fn collect_homomorphic_source_property_infos(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> Vec<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::collect_homomorphic_source_property_infos(db, source)
}

/// Expand a mapped type with resolved finite keys into `PropertyInfo` list.
pub(crate) fn expand_mapped_type_to_properties(
    db: &dyn TypeDatabase,
    mapped: &tsz_solver::MappedType,
    string_keys: &[tsz_common::Atom],
    source_props: &rustc_hash::FxHashMap<tsz_common::Atom, (bool, bool, TypeId)>,
    is_homomorphic: bool,
) -> Vec<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::expand_mapped_type_to_properties(
        db,
        mapped,
        string_keys,
        source_props,
        is_homomorphic,
    )
}

/// Re-export identity mapped type info from solver.
pub(crate) use tsz_solver::type_queries::IdentityMappedInfo;

/// Check if a mapped type is an identity homomorphic mapped type.
///
/// Returns info about the source type parameter if the mapped type has the
/// form `{ [K in keyof T]: T[K] }`. Used by application type evaluation to
/// decide primitive passthrough behavior.
pub(crate) fn classify_identity_mapped(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> Option<IdentityMappedInfo> {
    tsz_solver::type_queries::classify_identity_mapped(db, mapped_id)
}

/// Evaluate identity mapped type passthrough for a given type argument.
///
/// For an identity homomorphic mapped type `{ [K in keyof T]: T[K] }`:
/// - Primitives pass through directly.
/// - `any` with array constraint passes through.
/// - `any` without array constraint → `{ [x: string]: any; [x: number]: any }`.
/// - `unknown`/`never`/`error` without array constraint → no passthrough.
/// - Non-identity → no passthrough.
///
/// Delegates to solver's centralized passthrough logic.
pub(crate) fn evaluate_identity_mapped_passthrough(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
    arg: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::evaluate_identity_mapped_passthrough(db, mapped_id, arg)
}

/// Get the inner type of a `keyof T` type.
///
/// Returns `Some(T)` if the type is `KeyOf(T)`, `None` otherwise.
pub(crate) fn keyof_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::keyof_inner_type(db, type_id)
}

/// Get the constraint of a type parameter.
///
/// Returns `Some(constraint)` if the type is a `TypeParameter` or `Infer`
/// with a constraint, `None` otherwise. Used by the checker to discover
/// types reachable through type parameter constraints for pre-resolution
/// into the `TypeEnvironment`, without accessing TypeData directly.
pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

/// Check if a type is an array or tuple type.
pub(crate) fn is_array_or_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_or_tuple_type(db, type_id)
}

/// Reconstruct a mapped type with a new constraint, preserving all other fields.
///
/// Used when the checker evaluates a mapped type's constraint to concrete keys
/// and needs to create a new mapped type with the resolved constraint.
pub(crate) fn reconstruct_mapped_with_constraint(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
    new_constraint: TypeId,
) -> tsz_solver::MappedTypeId {
    tsz_solver::type_queries::reconstruct_mapped_with_constraint(db, mapped_id, new_constraint)
}

/// Collect finite property names from a mapped type's resolved constraint.
///
/// Returns `Some(names)` if the constraint resolves to a finite set of string
/// literal keys, `None` if the constraint is open-ended (e.g., `string`).
pub(crate) fn collect_finite_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> Option<rustc_hash::FxHashSet<tsz_common::Atom>> {
    tsz_solver::type_queries::collect_finite_mapped_property_names(db, mapped_id)
}

/// Extract string literal keys from a type (union of string literals).
pub(crate) fn extract_string_literal_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<tsz_common::Atom> {
    tsz_solver::type_queries::extract_string_literal_keys(db, type_id)
}

/// Get the name of a type parameter (`TypeParameter` or Infer).
///
/// Returns `Some(name)` if the type is a type parameter, `None` otherwise.
/// Used by the checker to match type parameters against declared parameter
/// lists without accessing `TypeData` directly.
pub(crate) fn type_param_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<tsz_common::Atom> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id).map(|info| info.name)
}

/// Re-export the body arg preservation classification for application evaluation.
pub(crate) use tsz_solver::type_queries::BodyArgPreservation;

/// Classify a type body to decide how args should be handled during application evaluation.
///
/// Delegates to the solver's structural analysis of conditional-infer patterns.
pub(crate) fn classify_body_for_arg_preservation(
    db: &dyn TypeDatabase,
    body_type: TypeId,
) -> BodyArgPreservation {
    tsz_solver::type_queries::classify_body_for_arg_preservation(db, body_type)
}

/// Returns `true` if the generic body type contains structural type operations
/// that require type arguments to be in concrete (expanded) form.
///
/// Delegates to the solver's structural analysis. See `body_arg_requires_concrete_form`
/// in the solver for the full contract.
pub(crate) fn body_arg_requires_concrete_form(db: &dyn TypeDatabase, body_type: TypeId) -> bool {
    tsz_solver::type_queries::body_arg_requires_concrete_form(db, body_type)
}

/// Check if a type is a primitive (string, number, boolean, bigint, etc.).
pub(crate) fn is_primitive_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_primitive_type(db, type_id)
}

/// Whether this type is identity-comparable: literals, enum members,
/// `null`/`undefined`/`void`/`never`, unique symbols. tsc's `removeSubtypes`
/// short-circuits on these via TypeId equality, so they don't drive cost.
pub(crate) fn is_identity_comparable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_identity_comparable_type(db, type_id)
}

/// Check if a type contains `this` type references.
pub(crate) fn contains_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_this_type(db, type_id)
}

/// Substitute `this` type references in a type with a concrete type.
pub(crate) fn substitute_this_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    tsz_solver::computation::substitute_this_type_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        this_type,
    )
}

/// Get the intersection members of a type (if it is an intersection).
pub(crate) fn get_intersection_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::type_queries::TypeIdList> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

/// Check if a type is a discriminated object intersection.
pub(crate) fn is_discriminated_object_intersection(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_discriminated_object_intersection(db, type_id)
}

/// Check if a type contains infer types.
pub(crate) fn contains_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_infer_types(db, type_id)
}

/// Check if a type contains infer types (TypeDatabase-taking variant).
pub(crate) fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_infer_types_db(db, type_id)
}

/// Get the callable shape id from a type.
pub(crate) fn callable_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::CallableShapeId> {
    tsz_solver::callable_shape_id(db, type_id)
}

/// Check if a type is a type query symbol reference.
pub(crate) fn type_query_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::type_query_symbol(db, type_id)
}

/// Result from a cached type evaluation, including side-effects needed by the checker.
pub(crate) struct EvalWithCacheResult {
    /// The evaluated type.
    pub result: TypeId,
    /// Whether the evaluator's recursion depth was exceeded.
    pub depth_exceeded: bool,
    /// Whether any structural depth bailout was silently handled.
    ///
    /// Distinct from `depth_exceeded`: silent bails are cleared from the
    /// evaluator's sticky-exceeded state under the legitimate-finite-recursion
    /// policy (`Permutation<U>`, `Combination<U>` and similar `ts-toolbelt`
    /// recursive mapped/conditional bodies). Callers that would retry on the
    /// same root with a more powerful resolver use this signal to skip the
    /// retry — the structural type-tree walk would hit the same protection
    /// limit at the same shape.
    pub silent_depth_bailed: bool,
    /// Whether the evaluator observed an application whose base `DefId` had no
    /// resolvable body during this pass (the registration-window artifact
    /// tracked by `TypeEvaluator::is_unresolved_def_seen`).
    ///
    /// A `result` computed while a consumed `DefId` was still unresolved is a
    /// function of the registration window it ran in, not of the input
    /// `TypeId` alone. The env-eval memo (`cache_env_eval_result`) is keyed
    /// purely on the input `TypeId` with no generation guard, so a caller that
    /// would cache such a result must skip the write — otherwise the
    /// under-resolved answer permanently shadows the correct one once the
    /// declaring file registers the real body. Inert today: the eager
    /// `ensure_refs_resolved` pre-walk resolves every referenced `DefId` before
    /// a committed relation, so this flag stays `false` until the on-demand
    /// forcing rework (issue #12101) removes that pre-walk.
    pub unresolved_def_seen: bool,
    /// Cache entries produced by the evaluator (key -> evaluated value).
    ///
    /// Empty when `CacheEntryCollection::Skip` is selected. The top-level
    /// `result` and depth flags are still authoritative; these entries are only
    /// the speed-only intermediate memo used by env-eval seed/persist.
    pub cache_entries: Vec<(TypeId, TypeId)>,
}

/// Controls whether `evaluate_type_with_cache` drains the evaluator's
/// intermediate cache into the returned side-channel.
///
/// This is a speed-only residency policy. It must not affect the evaluated
/// `TypeId`, depth flags, top-level env-eval memo, or closed-eval cache writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CacheEntryCollection {
    /// Materialize intermediate `TypeId` -> `TypeId` entries for env-eval
    /// seed/persist when the structural cap says they can be reused cheaply.
    Collect,
    /// Do not materialize intermediate entries for result-only callers.
    Skip,
}

#[derive(Clone, Copy)]
pub(crate) struct EvaluateTypeWithCacheOptions<'a> {
    pub(crate) expand_application_display_alias_args: bool,
    pub(crate) query_db: Option<&'a dyn QueryDatabase>,
    pub(crate) authoritative: bool,
    pub(crate) cache_entry_collection: CacheEntryCollection,
}

impl CacheEntryCollection {
    #[inline]
    #[must_use]
    pub(crate) const fn when_enabled(enabled: bool) -> Self {
        if enabled { Self::Collect } else { Self::Skip }
    }
}

/// Evaluate a type with a resolver, optionally seeding the evaluator cache.
///
/// Returns the result plus side-effects (depth exceeded, cache drain).
/// This is the canonical boundary for `TypeEvaluator` construction with cache
/// management — checker code must not construct `TypeEvaluator` directly.
///
/// `CacheEntryCollection` controls only whether the evaluator's intermediate
/// per-run cache is drained into the result. It must not affect evaluation or
/// top-level result caching; env-eval disables collection when the structural
/// seed/persist cap says those intermediates would be discarded.
pub(crate) fn evaluate_type_with_cache<R: tsz_solver::relations::subtype::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    seed: impl Iterator<Item = (TypeId, TypeId)>,
    has_seed: bool,
    options: EvaluateTypeWithCacheOptions<'_>,
) -> EvalWithCacheResult {
    let mut evaluator = tsz_solver::computation::TypeEvaluator::with_resolver(db, resolver);
    if let Some(query_db) = options.query_db {
        evaluator = evaluator.with_query_db(query_db);
        if options.authoritative {
            // The checker's authoritative, context-free type-resolution
            // boundary (full `CheckerContext` resolver), so it is the only
            // place permitted to *write* the substitution-independent
            // `closed_eval_cache` and to persist application results
            // unconditionally.
            evaluator = evaluator.with_closed_eval_writes();
        } else {
            // A limited-resolver pass (first-pass `TypeEnvironment`): it may
            // read the cross-call caches and share resolver-independent
            // instantiations, but it must not write the `closed_eval_cache` or
            // application-eval cache.
            evaluator = evaluator.with_limited_resolver();
        }
    }
    if options.expand_application_display_alias_args {
        evaluator = evaluator.with_expanded_application_display_alias_args();
    }
    if has_seed {
        evaluator.seed_cache(seed);
    }
    let result = evaluator.evaluate(type_id);
    let cache_entry_collection = options.cache_entry_collection;
    EvalWithCacheResult {
        result,
        depth_exceeded: evaluator.is_depth_exceeded(),
        silent_depth_bailed: evaluator.is_silent_depth_bailed(),
        unresolved_def_seen: evaluator.is_unresolved_def_seen(),
        cache_entries: if matches!(cache_entry_collection, CacheEntryCollection::Collect) {
            evaluator.drain_cache().collect()
        } else {
            Vec::new()
        },
    }
}

/// Evaluate a type for TS2589 detection at type alias definition sites.
///
/// Like `evaluate_type_with_cache` but flags `depth_exceeded` when cycle
/// detection fires on an Application type. This catches self-referential
/// conditional types that produce the same Application TypeId on each
/// expansion (e.g., `type Foo<T> = T extends unknown ? Foo<T> : unknown`).
pub(crate) fn evaluate_type_for_ts2589<R: tsz_solver::relations::subtype::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> EvalWithCacheResult {
    let mut evaluator = tsz_solver::computation::TypeEvaluator::with_resolver(db, resolver)
        .with_flag_depth_on_app_cycle();
    let result = evaluator.evaluate(type_id);
    EvalWithCacheResult {
        result,
        depth_exceeded: evaluator.is_depth_exceeded(),
        silent_depth_bailed: evaluator.is_silent_depth_bailed(),
        unresolved_def_seen: evaluator.is_unresolved_def_seen(),
        cache_entries: evaluator.drain_cache().collect(),
    }
}

/// Evaluate a type while suppressing `this` binding.
///
/// Used during heritage merging where `this` must remain unbound until the
/// final derived interface is constructed.
pub(crate) fn evaluate_type_suppressing_this<R: tsz_solver::relations::subtype::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator = tsz_solver::computation::TypeEvaluator::with_resolver(db, resolver)
        .with_suppress_this_binding();
    evaluator.evaluate(type_id)
}

/// Check if a type is a generic type application (`TypeData::Application`).
///
/// Thin wrapper to avoid direct `TypeData` pattern matching in checker code.
pub(crate) fn is_application_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
}

/// Check if a type contains type query references (TypeDatabase-taking variant).
pub(crate) fn contains_type_query_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_query_db(db, type_id)
}

struct CheckerDeclarationCycleHost<'a, 'b> {
    state: &'a mut CheckerState<'b>,
}

impl tsz_solver::relations::subtype::TypeResolver for CheckerDeclarationCycleHost<'_, '_> {
    fn resolve_ref(
        &self,
        symbol: tsz_solver::SymbolRef,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.state.ctx.resolve_ref(symbol, interner)
    }

    fn resolve_lazy(
        &self,
        def_id: tsz_solver::DefId,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.state.ctx.resolve_lazy(def_id, interner)
    }

    fn get_lazy_type_params(
        &self,
        def_id: tsz_solver::DefId,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        self.state.ctx.get_lazy_type_params(def_id)
    }

    fn def_to_symbol_id(&self, def_id: tsz_solver::DefId) -> Option<tsz_binder::SymbolId> {
        self.state.ctx.def_to_symbol_id(def_id)
    }

    fn provisional_value_epoch(&self) -> u64 {
        tsz_solver::relations::subtype::TypeResolver::provisional_value_epoch(&self.state.ctx)
    }
}

impl tsz_solver::type_queries::DeclarationTypeCycleHost for CheckerDeclarationCycleHost<'_, '_> {
    fn evaluate_application_for_serialization(&mut self, type_id: TypeId) -> TypeId {
        self.state.evaluate_application_type(type_id)
    }

    fn is_application_alias_serialization_exempt(&self, base_def_id: tsz_solver::DefId) -> bool {
        if self
            .state
            .ctx
            .def_to_symbol_id(base_def_id)
            .is_some_and(|sym_id| self.state.ctx.symbol_is_from_actual_or_cloned_lib(sym_id))
        {
            return true;
        }
        // Fallback: when the alias is registered against a file other than
        // the one currently being declaration-emitted, the .d.ts emitter can
        // reference it by name and never needs to inline-walk its body for
        // cycle detection. Comparing the alias' defining file index against
        // the current source file catches remaining named-lib cases without
        // weakening same-file recursive-alias detection: the TS5088 positive
        // `arrayFakeFlatNoCrashInferenceDeclarations.ts` defines
        // `BadFlatArray` in the same file as `foo`, so its `file_idx`
        // matches `current_file_idx` and it is correctly *not* exempted.
        self.state
            .ctx
            .def_file_idx(base_def_id)
            .is_some_and(|idx| idx != self.state.ctx.current_file_idx as u32)
    }
}

pub(crate) fn declaration_type_references_cyclic_structure(
    state: &mut CheckerState<'_>,
    type_id: TypeId,
) -> bool {
    let db = state.ctx.types;
    let mut host = CheckerDeclarationCycleHost { state };
    tsz_solver::type_queries::declaration_type_references_cyclic_structure(db, &mut host, type_id)
}

#[cfg(test)]
#[path = "../../../tests/state_type_environment.rs"]
mod tests;
