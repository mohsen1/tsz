//! Declaration-emit type-shape walks behind the solver boundary.
//!
//! The declaration emitter needs structural answers about inferred types —
//! "does this type contain a mapped type", "does this type apply an alias
//! whose body is a conditional", "which applied alias defs match a policy" —
//! and previously recursed `TypeData` by hand to compute them. These walkers
//! own that traversal inside the solver. Environment knowledge stays with the
//! caller through callbacks: the emitter resolves lazy `DefId`s through its
//! type-cache view and applies binder-backed policies, while the solver owns
//! the recursion shape over type structure.

use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::types::{ConditionalType, FunctionShape, TypeData, TypeId};
use crate::visitors::visitor_extract::{conditional_type_id, lazy_def_id, literal_value};
use rustc_hash::FxHashSet;

/// Depth fuel shared by the declaration-emit walks.
///
/// Inferred declaration surfaces are shallow; the fuel bounds pathological
/// self-referential shapes without a visited set, matching the historical
/// emitter-side limit so converted call sites stay behavior-identical.
const DECLARATION_WALK_DEPTH_LIMIT: usize = 16;

/// Check whether a type contains a mapped type anywhere in its declaration
/// surface, resolving `Lazy(DefId)` references through `resolve_lazy`.
///
/// Walks applications (base and args), unions/intersections, arrays,
/// readonly/`NoInfer` wrappers, `keyof`, index accesses, and string
/// intrinsics. Lazy references recurse into the callback-resolved body; an
/// unresolved lazy reference contributes `false`.
pub fn contains_mapped_type_through_lazy(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    resolve_lazy: &mut dyn FnMut(DefId) -> Option<TypeId>,
) -> bool {
    contains_mapped_type_inner(db, type_id, resolve_lazy, 0)
}

fn contains_mapped_type_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    resolve_lazy: &mut dyn FnMut(DefId) -> Option<TypeId>,
    depth: usize,
) -> bool {
    if depth > DECLARATION_WALK_DEPTH_LIMIT {
        return false;
    }
    let Some(type_data) = db.lookup(type_id) else {
        return false;
    };

    match type_data {
        TypeData::Mapped(_) => true,
        TypeData::Lazy(def_id) => resolve_lazy(def_id).is_some_and(|resolved| {
            contains_mapped_type_inner(db, resolved, resolve_lazy, depth + 1)
        }),
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            contains_mapped_type_inner(db, app.base, resolve_lazy, depth + 1)
                || app
                    .args
                    .iter()
                    .copied()
                    .any(|arg| contains_mapped_type_inner(db, arg, resolve_lazy, depth + 1))
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => db
            .type_list(list_id)
            .iter()
            .copied()
            .any(|member| contains_mapped_type_inner(db, member, resolve_lazy, depth + 1)),
        TypeData::Array(elem)
        | TypeData::ReadonlyType(elem)
        | TypeData::KeyOf(elem)
        | TypeData::NoInfer(elem) => contains_mapped_type_inner(db, elem, resolve_lazy, depth + 1),
        TypeData::IndexAccess(object, index) => {
            contains_mapped_type_inner(db, object, resolve_lazy, depth + 1)
                || contains_mapped_type_inner(db, index, resolve_lazy, depth + 1)
        }
        TypeData::StringIntrinsic { type_arg, .. } => {
            contains_mapped_type_inner(db, type_arg, resolve_lazy, depth + 1)
        }
        _ => false,
    }
}

/// Check whether a type applies an alias whose lazy-resolved body is a
/// conditional type, anywhere in its declaration surface.
///
/// `resolve_lazy` supplies the alias body for an application base's `DefId`;
/// the walk recurses through application args, function shapes (params,
/// `this`, return), and conditional arms.
pub fn contains_conditional_alias_application_through_lazy(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    resolve_lazy: &mut dyn FnMut(DefId) -> Option<TypeId>,
) -> bool {
    contains_conditional_alias_application_inner(db, type_id, resolve_lazy, 0)
}

fn contains_conditional_alias_application_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    resolve_lazy: &mut dyn FnMut(DefId) -> Option<TypeId>,
    depth: usize,
) -> bool {
    if depth > DECLARATION_WALK_DEPTH_LIMIT {
        return false;
    }
    let Some(type_data) = db.lookup(type_id) else {
        return false;
    };

    match type_data {
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            if let Some(def_id) = lazy_def_id(db, app.base)
                && let Some(body) = resolve_lazy(def_id)
                && conditional_type_id(db, body).is_some()
            {
                return true;
            }
            app.args.iter().copied().any(|arg| {
                contains_conditional_alias_application_inner(db, arg, resolve_lazy, depth + 1)
            })
        }
        TypeData::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            shape.params.iter().any(|param| {
                contains_conditional_alias_application_inner(
                    db,
                    param.type_id,
                    resolve_lazy,
                    depth + 1,
                )
            }) || shape.this_type.is_some_and(|this_type| {
                contains_conditional_alias_application_inner(db, this_type, resolve_lazy, depth + 1)
            }) || contains_conditional_alias_application_inner(
                db,
                shape.return_type,
                resolve_lazy,
                depth + 1,
            )
        }
        TypeData::Conditional(cond_id) => {
            let cond = db.get_conditional(cond_id);
            contains_conditional_alias_application_inner(
                db,
                cond.check_type,
                resolve_lazy,
                depth + 1,
            ) || contains_conditional_alias_application_inner(
                db,
                cond.extends_type,
                resolve_lazy,
                depth + 1,
            ) || contains_conditional_alias_application_inner(
                db,
                cond.true_type,
                resolve_lazy,
                depth + 1,
            ) || contains_conditional_alias_application_inner(
                db,
                cond.false_type,
                resolve_lazy,
                depth + 1,
            )
        }
        _ => false,
    }
}

/// Collect the `DefId`s of lazy application bases for which `include_def`
/// holds, reachable through application bases/args and union/intersection
/// members.
///
/// The walk intentionally covers only positions where an applied alias can
/// surface in an inferred declaration; the caller's `include_def` policy
/// decides which defs to keep (e.g. function-local type aliases known to its
/// binder).
pub fn collect_lazy_application_base_defs_matching(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    include_def: &mut dyn FnMut(DefId) -> bool,
) -> FxHashSet<DefId> {
    let mut defs = FxHashSet::default();
    collect_lazy_application_base_defs_inner(db, type_id, include_def, &mut defs, 0);
    defs
}

fn collect_lazy_application_base_defs_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    include_def: &mut dyn FnMut(DefId) -> bool,
    defs: &mut FxHashSet<DefId>,
    depth: usize,
) {
    if depth > DECLARATION_WALK_DEPTH_LIMIT {
        return;
    }
    let Some(type_data) = db.lookup(type_id) else {
        return;
    };

    match type_data {
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            if let Some(def_id) = lazy_def_id(db, app.base)
                && include_def(def_id)
            {
                defs.insert(def_id);
            }
            collect_lazy_application_base_defs_inner(db, app.base, include_def, defs, depth + 1);
            for arg in app.args.iter().copied() {
                collect_lazy_application_base_defs_inner(db, arg, include_def, defs, depth + 1);
            }
        }
        TypeData::Union(members) | TypeData::Intersection(members) => {
            for member in db.type_list(members).iter().copied() {
                collect_lazy_application_base_defs_inner(db, member, include_def, defs, depth + 1);
            }
        }
        _ => {}
    }
}

/// Rebuild a type with alias applications reduced through a caller policy.
///
/// At every visited node, `reduce_application` may replace the node (e.g. the
/// declaration emitter instantiates and evaluates conditional-alias
/// applications); a successful reduction recurses on the result. Otherwise
/// the walk rebuilds application args, function shapes, and conditional arms
/// with reduced children, interning replacements only when a child changed. A
/// rebuilt conditional is passed through `evaluate` before the walk recurses
/// on it, so reduced arms can collapse to a concrete branch.
pub fn rebuild_with_reduced_alias_applications(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    reduce_application: &mut dyn FnMut(TypeId) -> Option<TypeId>,
    evaluate: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    rebuild_reduced_inner(db, type_id, reduce_application, evaluate, 0)
}

fn rebuild_reduced_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    reduce_application: &mut dyn FnMut(TypeId) -> Option<TypeId>,
    evaluate: &mut dyn FnMut(TypeId) -> TypeId,
    depth: usize,
) -> TypeId {
    if depth > DECLARATION_WALK_DEPTH_LIMIT {
        return type_id;
    }

    if let Some(reduced) = reduce_application(type_id)
        && reduced != type_id
    {
        return rebuild_reduced_inner(db, reduced, reduce_application, evaluate, depth + 1);
    }

    let Some(type_data) = db.lookup(type_id) else {
        return type_id;
    };
    match type_data {
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            let mut changed = false;
            let args = app
                .args
                .iter()
                .copied()
                .map(|arg| {
                    let reduced =
                        rebuild_reduced_inner(db, arg, reduce_application, evaluate, depth + 1);
                    changed |= reduced != arg;
                    reduced
                })
                .collect::<Vec<_>>();
            if changed {
                db.application(app.base, args)
            } else {
                type_id
            }
        }
        TypeData::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            let mut changed = false;
            let params = shape
                .params
                .iter()
                .copied()
                .map(|mut param| {
                    let reduced = rebuild_reduced_inner(
                        db,
                        param.type_id,
                        reduce_application,
                        evaluate,
                        depth + 1,
                    );
                    changed |= reduced != param.type_id;
                    param.type_id = reduced;
                    param
                })
                .collect::<Vec<_>>();
            let this_type = shape.this_type.map(|this_type| {
                let reduced =
                    rebuild_reduced_inner(db, this_type, reduce_application, evaluate, depth + 1);
                changed |= reduced != this_type;
                reduced
            });
            let return_type = rebuild_reduced_inner(
                db,
                shape.return_type,
                reduce_application,
                evaluate,
                depth + 1,
            );
            changed |= return_type != shape.return_type;
            if changed {
                db.function(FunctionShape {
                    type_params: shape.type_params.clone(),
                    params,
                    this_type,
                    return_type,
                    type_predicate: shape.type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                })
            } else {
                type_id
            }
        }
        TypeData::Conditional(cond_id) => {
            let cond = db.get_conditional(cond_id);
            let check_type =
                rebuild_reduced_inner(db, cond.check_type, reduce_application, evaluate, depth + 1);
            let extends_type = rebuild_reduced_inner(
                db,
                cond.extends_type,
                reduce_application,
                evaluate,
                depth + 1,
            );
            let true_type =
                rebuild_reduced_inner(db, cond.true_type, reduce_application, evaluate, depth + 1);
            let false_type =
                rebuild_reduced_inner(db, cond.false_type, reduce_application, evaluate, depth + 1);
            if check_type == cond.check_type
                && extends_type == cond.extends_type
                && true_type == cond.true_type
                && false_type == cond.false_type
            {
                return type_id;
            }
            let rebuilt = db.conditional(ConditionalType {
                check_type,
                extends_type,
                true_type,
                false_type,
                is_distributive: cond.is_distributive,
            });
            let evaluated = evaluate(rebuilt);
            rebuild_reduced_inner(db, evaluated, reduce_application, evaluate, depth + 1)
        }
        _ => type_id,
    }
}

/// Classify whether a lazily-referenced def body should resolve through
/// `Lazy(DefId)` during declaration-emit evaluation that must keep printable
/// named surfaces.
///
/// Deferred/structural kinds (union, intersection, lazy, conditional, index
/// access, `keyof`, template literal), intrinsics, and literals resolve so the
/// evaluator can reduce them; object-like bodies stay unresolved behind their
/// name so the printer can reference the declaration instead of inlining it.
pub fn lazy_body_resolves_for_declaration_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(
            TypeData::Union(_)
            | TypeData::Intersection(_)
            | TypeData::Lazy(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::KeyOf(_)
            | TypeData::TemplateLiteral(_),
        ) => true,
        _ if type_id.is_intrinsic() => true,
        _ => literal_value(db, type_id).is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::types::{MappedType, ParamInfo, TypeParamInfo};

    fn param(interner: &TypeInterner, name: &str, type_id: TypeId) -> ParamInfo {
        ParamInfo::required(interner.intern_string(name), type_id)
    }

    fn sample_mapped(interner: &TypeInterner) -> TypeId {
        let key = interner.intern_string("K");
        interner.mapped(MappedType {
            type_param: TypeParamInfo::simple(key),
            constraint: TypeId::STRING,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        })
    }

    #[test]
    fn contains_mapped_type_resolves_lazy_through_callback() {
        let interner = TypeInterner::new();
        let mapped = sample_mapped(&interner);
        let def = DefId(7);
        let lazy = interner.lazy(def);
        let array_of_lazy = interner.array(lazy);

        let mut resolve = |def_id: DefId| (def_id == def).then_some(mapped);
        assert!(contains_mapped_type_through_lazy(
            &interner,
            array_of_lazy,
            &mut resolve
        ));

        let mut unresolved = |_: DefId| None;
        assert!(!contains_mapped_type_through_lazy(
            &interner,
            array_of_lazy,
            &mut unresolved
        ));
        assert!(!contains_mapped_type_through_lazy(
            &interner,
            interner.array(TypeId::STRING),
            &mut unresolved
        ));
    }

    #[test]
    fn contains_mapped_type_walks_application_args_and_unions() {
        let interner = TypeInterner::new();
        let mapped = sample_mapped(&interner);
        let base = interner.lazy(DefId(3));
        let app = interner.application(base, vec![mapped]);
        let union = interner.union(vec![TypeId::STRING, app]);

        let mut resolve = |_: DefId| None;
        assert!(contains_mapped_type_through_lazy(
            &interner,
            union,
            &mut resolve
        ));
    }

    #[test]
    fn contains_mapped_type_respects_depth_fuel() {
        let interner = TypeInterner::new();
        let mut current = sample_mapped(&interner);
        for _ in 0..(DECLARATION_WALK_DEPTH_LIMIT + 2) {
            current = interner.array(current);
        }
        let mut resolve = |_: DefId| None;
        assert!(!contains_mapped_type_through_lazy(
            &interner,
            current,
            &mut resolve
        ));
    }

    #[test]
    fn conditional_alias_application_detected_through_lazy_body() {
        let interner = TypeInterner::new();
        let cond_body = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let def = DefId(11);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let func = interner.function(FunctionShape::new(
            vec![param(&interner, "value", TypeId::STRING)],
            app,
        ));

        let mut resolve = |def_id: DefId| (def_id == def).then_some(cond_body);
        assert!(contains_conditional_alias_application_through_lazy(
            &interner,
            func,
            &mut resolve
        ));

        let mut non_conditional = |_: DefId| Some(TypeId::STRING);
        assert!(!contains_conditional_alias_application_through_lazy(
            &interner,
            func,
            &mut non_conditional
        ));
    }

    #[test]
    fn collect_application_base_defs_applies_policy() {
        let interner = TypeInterner::new();
        let keep = DefId(21);
        let drop = DefId(22);
        let kept_app = interner.application(interner.lazy(keep), vec![TypeId::STRING]);
        let dropped_app = interner.application(interner.lazy(drop), vec![kept_app]);
        let union = interner.union(vec![dropped_app, TypeId::NUMBER]);

        let mut include = |def_id: DefId| def_id == keep;
        let defs = collect_lazy_application_base_defs_matching(&interner, union, &mut include);
        assert!(defs.contains(&keep));
        assert!(!defs.contains(&drop));
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn rebuild_reduces_applications_inside_function_shapes() {
        let interner = TypeInterner::new();
        let def = DefId(31);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let func = interner.function(FunctionShape::new(
            vec![param(&interner, "value", app)],
            TypeId::VOID,
        ));

        let mut reduce = |ty: TypeId| (ty == app).then_some(TypeId::NUMBER);
        let mut evaluate = |ty: TypeId| ty;
        let rebuilt =
            rebuild_with_reduced_alias_applications(&interner, func, &mut reduce, &mut evaluate);
        assert_ne!(rebuilt, func);
        let expected = interner.function(FunctionShape::new(
            vec![param(&interner, "value", TypeId::NUMBER)],
            TypeId::VOID,
        ));
        assert_eq!(rebuilt, expected);

        let mut no_reduce = |_: TypeId| None;
        let unchanged =
            rebuild_with_reduced_alias_applications(&interner, func, &mut no_reduce, &mut evaluate);
        assert_eq!(unchanged, func);
    }

    #[test]
    fn rebuild_evaluates_rebuilt_conditionals() {
        let interner = TypeInterner::new();
        let def = DefId(41);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let cond = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: app,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });

        let mut reduce = |ty: TypeId| (ty == app).then_some(TypeId::NUMBER);
        let mut evaluate = |_: TypeId| TypeId::NUMBER;
        let rebuilt =
            rebuild_with_reduced_alias_applications(&interner, cond, &mut reduce, &mut evaluate);
        assert_eq!(rebuilt, TypeId::NUMBER);
    }

    #[test]
    fn lazy_body_display_resolution_classifies_kinds() {
        let interner = TypeInterner::new();
        let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        assert!(lazy_body_resolves_for_declaration_display(&interner, union));
        assert!(lazy_body_resolves_for_declaration_display(
            &interner,
            interner.keyof(TypeId::STRING)
        ));
        assert!(lazy_body_resolves_for_declaration_display(
            &interner,
            TypeId::STRING
        ));
        assert!(lazy_body_resolves_for_declaration_display(
            &interner,
            interner.literal_string("draft")
        ));
        let object = interner.object(vec![]);
        assert!(!lazy_body_resolves_for_declaration_display(
            &interner, object
        ));
        let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
        assert!(!lazy_body_resolves_for_declaration_display(&interner, func));
    }
}
