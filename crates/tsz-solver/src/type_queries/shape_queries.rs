//! Recursive structural predicates over `TypeId` projection paths.
//!
//! "Projection path" = the structural decomposition whose value depends on
//! the components: unions, intersections, generic-application arguments,
//! and indexed-access (object + index) components. The walks here do
//! **not** descend into mapped templates, conditional branches, object
//! property types, or function signature parts — that substructure is
//! orthogonal to the questions these predicates answer.

use std::cell::RefCell;

use crate::construction::TypeDatabase;
use crate::def::resolver::TypeResolver;
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashMap;

use super::data::{contains_type_parameters_db, get_mapped_type, get_type_parameter_constraint};

// Thread-local memo pool for `shape_contains_conditional_type_db`. The
// walker is called multiple times per TS2339 diagnostic; pooling the memo
// avoids per-call `FxHashMap` allocation and grow reallocations. Mirrors
// `with_predicate_buffers` in `visitors/visitor_predicates.rs`. Reentrant
// calls fall through to a fresh allocation because `take()` empties the
// slot.
thread_local! {
    static SHAPE_MEMO_POOL: RefCell<Option<FxHashMap<TypeId, bool>>> = const { RefCell::new(None) };
}

#[inline]
fn with_shape_memo<R>(f: impl FnOnce(&mut FxHashMap<TypeId, bool>) -> R) -> R {
    let mut memo = SHAPE_MEMO_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    memo.clear();
    let r = f(&mut memo);
    SHAPE_MEMO_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => memo.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(memo);
        }
    });
    r
}

/// Recursively check whether `type_id` contains a conditional type along
/// any *projection path*.
///
/// A projection path follows the structural decomposition of compound
/// types whose value depends on their components — unions, intersections,
/// generic application arguments, and indexed-access (object and index)
/// components. The walk deliberately does **not** descend into:
///
/// - mapped-type templates, constraints, or name types,
/// - conditional branches (check / extends / true / false),
/// - object / tuple / array property or element types,
/// - function / call signature parameters or return types.
///
/// The motivating use case is suppressing TS2339 on generic projections
/// like `Parameters<T>["length"]`, where the *outer* shape is what
/// determines whether a conditional may still be unresolved. Walking into
/// unrelated substructure (e.g. a property whose type happens to be a
/// conditional) would over-suppress diagnostics.
///
/// Cycles in the type graph terminate via the per-call memo: each entry
/// is recorded after its subtree has been answered, and shared DAG nodes
/// are only walked once.
pub fn shape_contains_conditional_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    with_shape_memo(|memo| walk(db, type_id, memo))
}

fn walk(db: &dyn TypeDatabase, type_id: TypeId, memo: &mut FxHashMap<TypeId, bool>) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(&cached) = memo.get(&type_id) {
        return cached;
    }
    // Record `false` before recursing so an indirect cycle through the
    // same node terminates with `false` rather than re-entering.
    memo.insert(type_id, false);

    let result = match db.lookup(type_id) {
        Some(TypeData::Conditional(_)) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| walk(db, m, memo))
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            app.args.iter().any(|&a| walk(db, a, memo))
        }
        Some(TypeData::IndexAccess(obj, idx)) => walk(db, obj, memo) || walk(db, idx, memo),
        _ => false,
    };

    // Only overwrite when the answer flipped to `true`; the `false` we
    // recorded pre-recursion already memoizes the dominant negative path.
    if result {
        memo.insert(type_id, true);
    }
    result
}

/// Returns `true` when `type_id` is a *generic* mapped type — one whose
/// key constraint (or remap `name_type`) still references unresolved
/// type parameters. Mapped types with fully concrete key spaces resolve
/// to a statically-known object shape and therefore return `false`.
///
/// Mirrors `tsc`'s `isGenericMappedType`. The mapped template is
/// intentionally **not** inspected: it always references the mapped
/// type's own iteration variable, which is bound (not an external
/// unresolved parameter).
pub fn is_generic_mapped_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(mapped) = get_mapped_type(db, type_id) else {
        return false;
    };
    contains_type_parameters_db(db, mapped.constraint)
        || mapped
            .name_type
            .is_some_and(|nt| contains_type_parameters_db(db, nt))
}

/// Returns `true` when `type_id` is a type parameter (or `Infer`)
/// whose constraint contains a conditional type along a projection path.
///
/// This is the canonical check used to suppress false-positive TS2339
/// when accessing properties through a generic conditional surface like
/// `Parameters<T>["length"]`: the resolved property set is not knowable
/// until the parameter is instantiated.
pub fn type_parameter_has_conditional_constraint_db(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    get_type_parameter_constraint(db, type_id)
        .is_some_and(|c| shape_contains_conditional_type_db(db, c))
}

/// Returns `true` when `type_id` is a type parameter (or `Infer`)
/// whose constraint is a generic mapped type — the resolved property
/// set is not knowable until the parameter is instantiated.
pub fn type_parameter_has_mapped_constraint_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    get_type_parameter_constraint(db, type_id).is_some_and(|c| is_generic_mapped_type_db(db, c))
}

/// Returns `true` when `type_id` is a generic `Application` whose aliased
/// body reduces to a generic mapped type after substituting the supplied
/// type arguments. Bridges resolver-backed lazy resolution so callers
/// don't need to reimplement the substitute-then-classify pattern.
pub fn is_generic_mapped_application_db<R: TypeResolver>(
    db: &dyn crate::construction::QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type_cached};

    // Cheap precondition: only `Application` types can satisfy the
    // predicate. Skip the resolver/substitution work otherwise.
    if type_id.is_intrinsic() {
        return false;
    }
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return false;
    };
    let app = db.type_application(app_id);
    let Some(def_id) = super::classifiers::get_lazy_def_id(db, app.base) else {
        return false;
    };
    let Some(type_params) = resolver.get_lazy_type_params(def_id) else {
        return false;
    };
    if type_params.is_empty() {
        return false;
    }
    let Some(body) = resolver.resolve_lazy(def_id, db.as_type_database()) else {
        return false;
    };
    let substitution = TypeSubstitution::from_args(db.as_type_database(), &type_params, &app.args);
    let instantiated =
        instantiate_type_cached(db.as_type_database(), Some(db), body, &substitution);
    is_generic_mapped_type_db(db.as_type_database(), instantiated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefId;
    use crate::intern::TypeInterner;
    use crate::types::{ConditionalType, MappedType, TypeParamInfo};

    fn make_conditional(interner: &TypeInterner) -> TypeId {
        interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::BOOLEAN,
            false_type: TypeId::NULL,
            is_distributive: false,
        })
    }

    fn make_type_param(interner: &TypeInterner, name: &str) -> TypeId {
        interner.type_param(TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
        })
    }

    #[test]
    fn intrinsic_short_circuit() {
        let interner = TypeInterner::new();
        assert!(!shape_contains_conditional_type_db(
            &interner,
            TypeId::STRING
        ));
        assert!(!shape_contains_conditional_type_db(
            &interner,
            TypeId::NUMBER
        ));
        assert!(!shape_contains_conditional_type_db(&interner, TypeId::ANY));
    }

    #[test]
    fn direct_conditional_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        assert!(shape_contains_conditional_type_db(&interner, cond));
    }

    #[test]
    fn conditional_inside_union_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let union = interner.union(vec![TypeId::STRING, cond]);
        assert!(shape_contains_conditional_type_db(&interner, union));
    }

    #[test]
    fn conditional_inside_intersection_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let inter = interner.intersection(vec![TypeId::STRING, cond]);
        assert!(shape_contains_conditional_type_db(&interner, inter));
    }

    #[test]
    fn conditional_inside_application_arg_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(42));
        let app = interner.application(base, vec![cond]);
        assert!(shape_contains_conditional_type_db(&interner, app));
    }

    #[test]
    fn conditional_inside_index_access_object_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let ia = interner.index_access(cond, TypeId::STRING);
        assert!(shape_contains_conditional_type_db(&interner, ia));
    }

    #[test]
    fn conditional_inside_index_access_index_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(42));
        let ia = interner.index_access(base, cond);
        assert!(shape_contains_conditional_type_db(&interner, ia));
    }

    #[test]
    fn deeply_nested_projection_matches() {
        // union(intersection(application_args(conditional))) — every layer
        // is a projection path and must be walked.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(42));
        let app = interner.application(base, vec![cond]);
        let inter = interner.intersection(vec![TypeId::STRING, app]);
        let union = interner.union(vec![TypeId::NUMBER, inter]);
        assert!(shape_contains_conditional_type_db(&interner, union));
    }

    #[test]
    fn conditional_buried_in_mapped_template_does_not_match() {
        // Mapped templates are NOT a projection path — descending into them
        // would over-suppress diagnostics.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            name_type: None,
            template: cond,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(!shape_contains_conditional_type_db(&interner, mapped));
    }

    #[test]
    fn no_conditional_returns_false() {
        let interner = TypeInterner::new();
        let base = interner.lazy(DefId(1));
        let app = interner.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
        let union = interner.union(vec![TypeId::STRING, app]);
        assert!(!shape_contains_conditional_type_db(&interner, union));
    }

    #[test]
    fn shared_dag_subtree_is_walked_once() {
        // The same `Application` is reachable through two `Union` branches.
        // The memo proves we visit the shared subtree once — the result
        // is consistent regardless of sharing.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(1));
        let shared = interner.application(base, vec![cond]);
        let left = interner.intersection(vec![TypeId::STRING, shared]);
        let right = interner.intersection(vec![TypeId::NUMBER, shared]);
        let union = interner.union(vec![left, right]);
        assert!(shape_contains_conditional_type_db(&interner, union));
    }

    #[test]
    fn generic_mapped_type_when_constraint_has_type_param() {
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        let keyof_t = interner.keyof(t_param);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(keyof_t),
                default: None,
                is_const: false,
            },
            constraint: keyof_t,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(is_generic_mapped_type_db(&interner, mapped));
    }

    #[test]
    fn concrete_mapped_type_is_not_generic() {
        let interner = TypeInterner::new();
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(!is_generic_mapped_type_db(&interner, mapped));
    }

    #[test]
    fn generic_mapped_type_when_name_type_has_type_param() {
        // {[K in "a" | "b" as `prefix-${T}`]: number} — name_type drives genericness.
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        let concrete_keys = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(concrete_keys),
                default: None,
                is_const: false,
            },
            constraint: concrete_keys,
            name_type: Some(t_param),
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(is_generic_mapped_type_db(&interner, mapped));
    }

    #[test]
    fn renaming_iteration_var_does_not_change_classification() {
        // Confirm the rule is structural — different iteration var names
        // ("P" vs "K") give the same answer.
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        let keyof_t = interner.keyof(t_param);

        let mapped_k = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(keyof_t),
                default: None,
                is_const: false,
            },
            constraint: keyof_t,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let mapped_p = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("P"),
                constraint: Some(keyof_t),
                default: None,
                is_const: false,
            },
            constraint: keyof_t,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });

        assert_eq!(
            is_generic_mapped_type_db(&interner, mapped_k),
            is_generic_mapped_type_db(&interner, mapped_p),
        );
    }

    #[test]
    fn non_mapped_type_is_not_generic_mapped() {
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        assert!(!is_generic_mapped_type_db(&interner, t_param));
        assert!(!is_generic_mapped_type_db(&interner, TypeId::STRING));
        let cond = make_conditional(&interner);
        assert!(!is_generic_mapped_type_db(&interner, cond));
    }

    #[test]
    fn type_parameter_with_conditional_constraint() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let tp = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(cond),
            default: None,
            is_const: false,
        });
        assert!(type_parameter_has_conditional_constraint_db(&interner, tp));
    }

    #[test]
    fn type_parameter_with_conditional_via_projection_constraint() {
        // T extends Foo<Cond> — constraint is Application, args contain conditional.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(1));
        let app = interner.application(base, vec![cond]);
        let tp = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(app),
            default: None,
            is_const: false,
        });
        assert!(type_parameter_has_conditional_constraint_db(&interner, tp));
    }

    #[test]
    fn type_parameter_without_constraint() {
        let interner = TypeInterner::new();
        let tp = make_type_param(&interner, "T");
        assert!(!type_parameter_has_conditional_constraint_db(&interner, tp));
        assert!(!type_parameter_has_mapped_constraint_db(&interner, tp));
    }

    #[test]
    fn type_parameter_with_mapped_constraint() {
        let interner = TypeInterner::new();
        let u_param = make_type_param(&interner, "U");
        let keyof_u = interner.keyof(u_param);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(keyof_u),
                default: None,
                is_const: false,
            },
            constraint: keyof_u,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        let tp = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(mapped),
            default: None,
            is_const: false,
        });
        assert!(type_parameter_has_mapped_constraint_db(&interner, tp));
    }
}
