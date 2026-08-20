//! Structural facts for assignability alias-display diagnostics.

use super::common;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::{DefId, DefKind, DefinitionStore};

/// tsc `aliasSymbol` display policy for a non-generic type alias: when the alias
/// named by `def_id` must be rendered as its underlying type (the computed
/// conditional / indexed-access / `keyof` / application / template / string
/// intrinsic body that collapses to a shared singleton, or a direct
/// intrinsic/literal body) rather than by its declared name, returns that
/// underlying `TypeId`.
///
/// This delegates to the solver's shared display policy so the checker's
/// assignability-message formatter and the solver's `TypeFormatter` agree on
/// alias rendering instead of maintaining two drifting copies of the rule.
pub(crate) fn type_alias_displayed_as_underlying(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    def_id: DefId,
) -> Option<TypeId> {
    tsz_solver::type_alias_displayed_as_underlying(db, definitions, def_id)
}

/// As [`type_alias_displayed_as_underlying`], but accepts a `TypeId` instead of
/// a `DefId`: resolves `ty` to the non-generic type alias it references — either
/// a `Lazy(DefId)` alias reference or the already-resolved structural result
/// that still maps back to the alias `DefId` via the def store — then applies
/// the shared underlying-display policy. Returns the underlying `TypeId` to
/// display in place of the alias name, or `None` when `ty` is not such an alias.
///
/// Keeping the `Lazy` resolution here (rather than at the checker call site)
/// avoids growing the `query_boundaries::common` quarantine in checker modules.
pub(crate) fn type_displayed_as_underlying(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    ty: TypeId,
) -> Option<TypeId> {
    let def_id = common::lazy_def_id(db, ty).or_else(|| definitions.find_def_for_type(ty))?;
    type_alias_displayed_as_underlying(db, definitions, def_id)
}

pub(crate) fn source_preserves_declared_generic_alias_display(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> bool {
    common::is_intersection_type(db, source) || common::object_shape_id(db, source).is_some()
}

pub(crate) fn source_can_use_declared_generic_alias_annotation(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    source: TypeId,
) -> bool {
    source_can_use_declared_generic_alias_annotation_inner(db, definitions, source, 0)
}

fn source_can_use_declared_generic_alias_annotation_inner(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    source: TypeId,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    if common::contains_conditional_type(db, source) || common::is_callable_type(db, source) {
        return true;
    }
    if let Some(app) = common::type_application(db, source)
        && lazy_alias_body_contains_conditional(db, definitions, app.base, depth + 1)
    {
        return true;
    }
    if lazy_alias_body_contains_conditional(db, definitions, source, depth + 1) {
        return true;
    }
    common::union_members(db, source).is_some_and(|members| {
        members.iter().any(|&member| {
            source_can_use_declared_generic_alias_annotation_inner(
                db,
                definitions,
                member,
                depth + 1,
            )
        })
    }) || common::intersection_members(db, source).is_some_and(|members| {
        members.iter().any(|&member| {
            source_can_use_declared_generic_alias_annotation_inner(
                db,
                definitions,
                member,
                depth + 1,
            )
        })
    })
}

fn lazy_alias_body_contains_conditional(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    type_id: TypeId,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let Some(def_id) = common::lazy_def_id(db, type_id) else {
        return false;
    };
    let Some(def) = definitions.get(def_id) else {
        return false;
    };
    if def.kind != DefKind::TypeAlias {
        return false;
    }
    let Some(body) = def.body else {
        return false;
    };
    common::contains_conditional_type(db, body)
        || source_can_use_declared_generic_alias_annotation_inner(db, definitions, body, depth + 1)
}

pub(crate) fn is_application_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::application_id(db, type_id).is_some()
}

pub(crate) fn is_object_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::object_shape_id(db, type_id).is_some()
}

pub(crate) fn contains_undefined_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::type_contains_undefined(db, type_id)
}

pub(crate) fn has_optional_parameter_undefined_surface(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    fn signature_has_optional_parameter(sig: &tsz_solver::CallSignature) -> bool {
        sig.params.iter().any(|param| param.optional)
    }

    if common::function_shape_for_type(db, type_id)
        .is_some_and(|shape| shape.params.iter().any(|param| param.optional))
    {
        return true;
    }

    if common::callable_shape_for_type(db, type_id).is_some_and(|shape| {
        shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .any(signature_has_optional_parameter)
    }) {
        return true;
    }

    common::union_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| has_optional_parameter_undefined_surface(db, member))
    }) || common::intersection_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| has_optional_parameter_undefined_surface(db, member))
    })
}

pub(crate) fn is_literal_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Unit-literal check first: the boolean literals `true`/`false` are
    // intrinsic `TypeId`s that the shared union predicate's intrinsic
    // fast-path skips, but they are literal sources for display purposes (tsc
    // widens a declared `true` to `boolean` against a non-literal target;
    // repainting it with the annotation text would undo that).
    is_unit_literal_type(db, type_id)
        || tsz_solver::type_queries::is_literal_or_literal_union_type(db, type_id)
        || common::is_template_literal_type(db, type_id)
}

/// True when `type_id` is a scalar unit literal (`0n`, `42`, `"x"`, `true`) —
/// i.e. it carries a single `LiteralValue`. Unlike [`is_literal_for_alias_display`]
/// this excludes literal *unions* and template-literal types, so the
/// assignability source-display widening only fires for a lone literal whose
/// base primitive `tsc` shows against a non-literal-sensitive target.
pub(crate) fn is_unit_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::literal_value(db, type_id).is_some()
}

/// A deferred string-mapping intrinsic (`Uppercase`/`Lowercase`/`Capitalize`/
/// `Uncapitalize` over a non-literal argument). tsc always renders these in
/// their structural `Intrinsic<arg>` form in assignability diagnostics, never
/// via a type-alias name, so the declared-annotation source rewrite must not
/// repaint such a source with its alias name.
pub(crate) fn is_string_intrinsic_for_alias_display(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    common::is_string_intrinsic_type(db, type_id)
}

/// The nested-relation member-frame view of an alias-application SOURCE.
///
/// `tsc` re-enters a failed union-member relation with the source's alias
/// erased (`getNormalizedType`), so while the headline keeps the written alias
/// spelling (`FlipRow<A, B>`, or a non-generic alias name `MyRow`), the member
/// frame renders the underlying application (`PairRow<B, A>`). Accepts either
/// the alias application itself or an evaluated structural result that maps
/// back to an application through its display alias; returns the composed
/// underlying application, or `None` when the source has no application view
/// at all (the caller keeps its existing display).
///
/// Two provenance shapes reach here:
/// * The source (or its display alias) is a *forwarding-alias* application
///   (`FlipRow<A, B>` with `type FlipRow<X, Y> = PairRow<Y, X>`): the solver
///   composes the underlying application by remapping the arguments.
/// * The source is an evaluated structural result whose display alias is
///   *already* the underlying base application (annotation-lowered sources
///   store the composed `PairRow<...>` as display provenance while the head
///   repaints the written spelling): the normalized member-frame view is that
///   stored application itself.
pub(crate) fn nested_relation_source_base_application_view(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    ty: TypeId,
) -> Option<TypeId> {
    let application = if common::type_application(db, ty).is_some() {
        ty
    } else {
        db.get_display_alias(ty)?
    };
    if let Some(forwarded) =
        tsz_solver::forwarded_alias_application_display_view(db, definitions, application)
    {
        return Some(forwarded);
    }
    (application != ty).then_some(application)
}

/// Re-point an eagerly evaluated call return's display alias at the call's own
/// declared-return application when the existing claim is provably the same
/// type family.
///
/// The checker eagerly evaluates a monomorphic application call return; the
/// evaluated structural result's display alias is first-writer-wins, and an
/// inference-internal application interned during the same call's
/// return-context merge scan (the per-position-union merge of the contextual
/// union's arms) can claim it first — repainting the diagnostic head with the
/// forwarded base (`PairRow<...>`) where `tsc` renders the declared alias
/// application (`FlipRow<...>`). Replace the claim only when it equals the
/// declared application's own forwarded view, so an unrelated first writer is
/// never repainted.
pub(crate) fn repoint_evaluated_call_return_display_alias(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    evaluated: TypeId,
    return_application: TypeId,
) {
    if evaluated == return_application {
        return;
    }
    let Some(existing) = db.get_display_alias(evaluated) else {
        return;
    };
    if existing == return_application {
        return;
    }
    if tsz_solver::forwarded_alias_application_display_view(db, definitions, return_application)
        == Some(existing)
    {
        db.store_display_alias(evaluated, return_application);
    }
}
