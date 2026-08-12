//! Conservative canonical call resolution for checker flow fallback.

use super::{AssignabilityChecker, CallResult, resolve_call_with_checker};
use crate::construction::QueryDatabase;
use crate::{IntrinsicKind, TypeData, TypeId};
use rustc_hash::FxHashSet;

/// Maximum total type nodes inspected while proving that a fallback target
/// has exactly one supported generic call signature.
///
/// This is a conservative recovery path, so exhaustion must reject the
/// fallback rather than publish a result derived from a partial walk. A total
/// node budget (rather than only a recursion-depth cap) also bounds wide
/// intersections and shared acyclic graphs that revisit a child after it has
/// left the active cycle-detection set.
const MAX_SINGLE_GENERIC_CALL_WALK_STEPS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignatureGate {
    NonCallable,
    One(TypeId),
    Unsupported,
}

fn single_non_rest_generic_call<C: AssignabilityChecker>(
    db: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    seen: &mut FxHashSet<TypeId>,
    remaining_steps: &mut usize,
) -> SignatureGate {
    if *remaining_steps == 0 {
        return SignatureGate::Unsupported;
    }
    *remaining_steps -= 1;

    if !seen.insert(type_id) {
        return SignatureGate::Unsupported;
    }

    let evaluated = checker.evaluate_type(type_id);
    let result = if evaluated != type_id {
        single_non_rest_generic_call(db, checker, evaluated, seen, remaining_steps)
    } else if let Some(expanded) = checker.expand_type_alias_application(type_id)
        && expanded != type_id
    {
        single_non_rest_generic_call(db, checker, expanded, seen, remaining_steps)
    } else {
        match db.lookup(type_id) {
            Some(TypeData::Function(shape_id)) => {
                let shape = db.function_shape(shape_id);
                if shape.is_constructor {
                    SignatureGate::NonCallable
                } else if !shape.type_params.is_empty()
                    && !shape.params.iter().any(|param| param.rest)
                {
                    SignatureGate::One(type_id)
                } else {
                    SignatureGate::Unsupported
                }
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = db.callable_shape(shape_id);
                match shape.call_signatures.as_slice() {
                    [signature]
                        if !signature.type_params.is_empty()
                            && !signature.params.iter().any(|param| param.rest) =>
                    {
                        SignatureGate::One(type_id)
                    }
                    [] => SignatureGate::NonCallable,
                    _ => SignatureGate::Unsupported,
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                let mut found = None;
                let mut unsupported = false;
                for &member in db.type_list(list_id).iter() {
                    match single_non_rest_generic_call(db, checker, member, seen, remaining_steps) {
                        SignatureGate::NonCallable => {}
                        SignatureGate::One(call_type) if found.is_none() => {
                            found = Some(call_type);
                        }
                        _ => {
                            unsupported = true;
                            break;
                        }
                    }
                }
                if unsupported {
                    SignatureGate::Unsupported
                } else if let Some(call_type) = found {
                    SignatureGate::One(call_type)
                } else {
                    SignatureGate::NonCallable
                }
            }
            Some(TypeData::TypeParameter(info)) => {
                info.constraint
                    .map_or(SignatureGate::Unsupported, |constraint| {
                        single_non_rest_generic_call(db, checker, constraint, seen, remaining_steps)
                    })
            }
            Some(TypeData::Application(_) | TypeData::Conditional(_) | TypeData::Union(_)) => {
                SignatureGate::Unsupported
            }
            Some(
                TypeData::Intrinsic(
                    IntrinsicKind::Never
                    | IntrinsicKind::Void
                    | IntrinsicKind::Null
                    | IntrinsicKind::Undefined
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Number
                    | IntrinsicKind::String
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol,
                )
                | TypeData::Literal(_)
                | TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Enum(_, _)
                | TypeData::TemplateLiteral(_)
                | TypeData::UniqueSymbol(_),
            ) => SignatureGate::NonCallable,
            // Anything still deferred after evaluation may reveal another call
            // signature. Intersections may ignore only proven non-callable leaves.
            _ => SignatureGate::Unsupported,
        }
    };
    seen.remove(&type_id);
    result
}

/// Resolve the one generic call shape that flow fallback can recover without
/// performing overload selection or spread expansion.
pub fn resolve_single_non_rest_generic_call_with_compat_checker<'a, R, F>(
    db: &'a dyn QueryDatabase,
    resolver: &'a R,
    func_type: TypeId,
    arg_types: &[TypeId],
    configure_checker: F,
) -> Option<TypeId>
where
    R: crate::relations::subtype::TypeResolver,
    F: FnOnce(&mut crate::relations::compat::CompatChecker<'a, R>),
{
    let mut checker = crate::relations::compat::CompatChecker::with_resolver(db, resolver);
    configure_checker(&mut checker);
    let mut remaining_steps = MAX_SINGLE_GENERIC_CALL_WALK_STEPS;
    let SignatureGate::One(call_type) = single_non_rest_generic_call(
        db,
        &mut checker,
        func_type,
        &mut FxHashSet::default(),
        &mut remaining_steps,
    ) else {
        return None;
    };

    let (result, _, _) =
        resolve_call_with_checker(db, &mut checker, call_type, arg_types, false, None, None);
    match result {
        CallResult::Success(return_type) => Some(return_type),
        CallResult::ArgumentTypeMismatch {
            fallback_return, ..
        }
        | CallResult::NoOverloadMatch {
            fallback_return, ..
        } if fallback_return != TypeId::ERROR => Some(fallback_return),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computation::TypeEnvironment;
    use crate::intern::TypeInterner;
    use crate::{DefId, FunctionShape, ParamInfo, TypeParamInfo, TypeParamOrigin};

    fn generic_identity(db: &TypeInterner, name: &str) -> TypeId {
        let parameter = TypeParamInfo {
            name: db.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        };
        let parameter_type = db.type_param(parameter);
        db.function(FunctionShape {
            type_params: vec![parameter],
            params: vec![ParamInfo {
                name: None,
                type_id: parameter_type,
                optional: false,
                rest: false,
                arity_only_optional: false,
            }],
            this_type: None,
            return_type: parameter_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn constrained_chain(db: &TypeInterner, terminal: TypeId, links: usize) -> TypeId {
        (0..links).fold(terminal, |constraint, index| {
            db.type_param(TypeParamInfo {
                name: db.intern_string(&format!("Wrapper{index}")),
                constraint: Some(constraint),
                default: None,
                is_const: false,
                origin: TypeParamOrigin::User,
            })
        })
    }

    #[test]
    fn resolves_one_generic_signature_through_an_intersection_wrapper() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "T");
        let wrapped = db.intersection2(callable, db.object(Vec::new()));
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, Some(TypeId::STRING));
    }

    #[test]
    fn rejects_an_intersection_with_two_generic_call_signatures() {
        let db = TypeInterner::new();
        let overloaded = db.intersection2(generic_identity(&db, "T"), generic_identity(&db, "U"));
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            overloaded,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, None);
    }

    #[test]
    fn ignores_a_construct_only_member_in_an_intersection_wrapper() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "T");
        let constructor = db.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::UNKNOWN,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        let wrapped = db.intersection2(callable, constructor);
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, Some(TypeId::STRING));
    }

    #[test]
    fn rejects_an_intersection_with_an_unresolved_member() {
        let db = TypeInterner::new();
        let wrapped = db.intersection2(generic_identity(&db, "T"), db.lazy(DefId(17)));
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, None);
    }

    #[test]
    fn accepts_an_acyclic_constraint_chain_at_the_walk_cap() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "Value");
        let wrapped = constrained_chain(&db, callable, MAX_SINGLE_GENERIC_CALL_WALK_STEPS - 1);
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, Some(TypeId::STRING));
    }

    #[test]
    fn rejects_an_acyclic_constraint_chain_over_the_walk_cap() {
        let db = TypeInterner::new();
        let callable = generic_identity(&db, "Element");
        let wrapped = constrained_chain(&db, callable, MAX_SINGLE_GENERIC_CALL_WALK_STEPS);
        let result = resolve_single_non_rest_generic_call_with_compat_checker(
            &db,
            &TypeEnvironment::new(),
            wrapped,
            &[TypeId::STRING],
            |_| {},
        );
        assert_eq!(result, None);
    }
}
