//! Effective parameter identity for constructor-signature deduplication.
//!
//! TypeScript compares signatures through `getParameterCount`,
//! `getMinArgumentCount`, `hasEffectiveRestParameter`, and
//! `getTypeAtPosition`. In particular, a tuple-typed rest parameter is first
//! exposed as its positional parameter list:
//!
//! ```text
//! new (...args: [string, number]) => R
//! new (text: string, count: number) => R
//! ```
//!
//! Those signatures are identical even though their raw `ParamInfo` vectors
//! differ.

use super::transparent_alias::expose_transparent_alias_once;
use crate::canonicalize::Canonicalizer;
use crate::construction::TypeDatabase;
use crate::def::DefKind;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::TypeResolver;
use crate::types::{CallSignature, ParamInfo, TupleElement, TypeData, TypeId, TypePredicateTarget};

/// Alpha-normalized effective value-parameter shape used by
/// `compareSignaturesIdentical`-style constructor deduplication.
#[derive(PartialEq, Eq, Hash)]
pub(super) struct EffectiveConstructParameters {
    min_argument_count: usize,
    has_effective_rest: bool,
    position_types: Vec<TypeId>,
}

#[derive(PartialEq, Eq, Hash)]
pub(super) struct ConstructPredicateIdentity {
    asserts: bool,
    target_is_this: bool,
    type_id: Option<TypeId>,
    parameter_index: Option<usize>,
}

#[derive(PartialEq, Eq, Hash)]
pub(super) struct ConstructSignatureCoarseIdentity {
    type_param_constraints: Vec<TypeId>,
    type_param_defaults: Vec<TypeId>,
    params: EffectiveConstructParameters,
    result: ConstructSignatureResultIdentity,
}

#[derive(PartialEq, Eq, Hash)]
enum ConstructSignatureResultIdentity {
    Return(TypeId),
    Predicate(ConstructPredicateIdentity),
}

pub(super) struct ConstructSignatureIdentity {
    pub(super) coarse: ConstructSignatureCoarseIdentity,
    pub(super) this_type: Option<TypeId>,
}

/// Alpha-normalized, structurally canonical identity used by tsc's
/// intersection `appendSignatures`.
///
/// Type-parameter and value-parameter names are cosmetic. Constraints,
/// parameter types/arity, `this`, return, and predicate shape remain semantic;
/// `const` modifiers are inference inputs and are not identity-bearing.
pub(super) fn construct_signature_identity<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    canonicalizer: &mut Canonicalizer<'_, R>,
    signature: &CallSignature,
) -> ConstructSignatureIdentity {
    let mut substitution = TypeSubstitution::for_signature_domain(&signature.type_params);
    let type_param_count = signature.type_params.len();
    for (index, type_param) in signature.type_params.iter().enumerate() {
        substitution.insert(
            type_param.name,
            db.bound_parameter((type_param_count - index - 1) as u32),
        );
    }
    let effective_rest_shape = signature
        .params
        .last()
        .filter(|parameter| parameter.rest)
        .map(|parameter| instantiate_type(db, parameter.type_id, &substitution));
    let mut normalize_type =
        |type_id| canonicalizer.canonicalize(instantiate_type(db, type_id, &substitution));

    let this_type = signature.this_type.map(&mut normalize_type);
    let type_param_constraints = signature
        .type_params
        .iter()
        .map(|type_param| normalize_type(type_param.constraint.unwrap_or(TypeId::UNKNOWN)))
        .collect();
    let type_param_defaults = signature
        .type_params
        .iter()
        .map(|type_param| normalize_type(type_param.default.unwrap_or(TypeId::UNKNOWN)))
        .collect();
    let params = effective_construct_parameters(
        db,
        resolver,
        signature,
        effective_rest_shape,
        &mut normalize_type,
    );
    let result = match signature.type_predicate {
        Some(predicate) => {
            ConstructSignatureResultIdentity::Predicate(ConstructPredicateIdentity {
                asserts: predicate.asserts,
                target_is_this: matches!(predicate.target, TypePredicateTarget::This),
                type_id: predicate.type_id.map(&mut normalize_type),
                parameter_index: predicate.parameter_index,
            })
        }
        None => ConstructSignatureResultIdentity::Return(normalize_type(signature.return_type)),
    };

    ConstructSignatureIdentity {
        this_type,
        coarse: ConstructSignatureCoarseIdentity {
            type_param_constraints,
            type_param_defaults,
            params,
            result,
        },
    }
}

/// Whether resolver-free projection memoization can change this identity.
///
/// Only transparent aliases depend on resolver-backed canonicalization. Known
/// nominal definitions keep their `Lazy(DefId)` identity under both the noop
/// and real resolver; unknown kinds are conservatively treated as sensitive.
pub(super) fn construct_signature_identity_requires_resolver<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    signature: &CallSignature,
) -> bool {
    let mut identity_types =
        Vec::with_capacity(signature.type_params.len() * 2 + signature.params.len() + 3);
    for type_param in &signature.type_params {
        if let Some(constraint) = type_param.constraint {
            identity_types.push(constraint);
        }
        if let Some(default) = type_param.default {
            identity_types.push(default);
        }
    }
    identity_types.extend(signature.params.iter().map(|parameter| parameter.type_id));
    identity_types.extend(signature.this_type);
    identity_types.push(signature.return_type);
    if let Some(predicate_type) = signature
        .type_predicate
        .and_then(|predicate| predicate.type_id)
    {
        identity_types.push(predicate_type);
    }

    identity_types.into_iter().any(|type_id| {
        let mut requires_resolver = false;
        crate::visitor::walk_referenced_types(db, type_id, |reachable| {
            if requires_resolver {
                return;
            }
            if let Some(TypeData::Lazy(def_id)) = db.lookup(reachable) {
                requires_resolver = match resolver.get_def_kind(def_id) {
                    Some(DefKind::TypeAlias) | None => true,
                    Some(
                        DefKind::Interface
                        | DefKind::Class
                        | DefKind::Enum
                        | DefKind::Namespace
                        | DefKind::Function
                        | DefKind::Variable
                        | DefKind::ClassConstructor,
                    ) => false,
                };
            }
        });
        requires_resolver
    })
}

pub(super) fn effective_construct_parameters<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    signature: &CallSignature,
    effective_rest_shape: Option<TypeId>,
    mut normalize_type: impl FnMut(TypeId) -> TypeId,
) -> EffectiveConstructParameters {
    let mut min_argument_count = signature
        .params
        .iter()
        .enumerate()
        .filter(|(_, param)| param.is_required())
        .map(|(index, _)| index + 1)
        .max()
        .unwrap_or(0);
    let mut has_effective_rest = signature.params.last().is_some_and(|param| param.rest);
    let mut position_types = Vec::with_capacity(signature.params.len());

    let tuple_rest =
        effective_rest_shape.and_then(|type_id| actual_tuple_elements(db, resolver, type_id));
    if let Some(elements) = tuple_rest {
        let fixed_parameter_count = signature.params.len() - 1;
        position_types.extend(
            signature.params[..fixed_parameter_count]
                .iter()
                .map(|parameter| fixed_parameter_read_type(db, parameter)),
        );

        let first_variable = elements.iter().position(|element| element.rest);
        let fixed_tuple_count = first_variable.unwrap_or(elements.len());
        position_types.extend(
            elements[..fixed_tuple_count]
                .iter()
                .map(|element| tuple_element_read_type(db, element)),
        );

        if let Some(variable_index) = first_variable {
            position_types.push(tuple_variable_position_type(db, &elements, variable_index));
            has_effective_rest = true;
        } else {
            has_effective_rest = false;
        }

        // tsc's `getMinArgumentCount` uses the required prefix of an actual
        // tuple rest. It deliberately does not flatten a fixed suffix after a
        // variadic element into extra required parameters.
        let first_non_required = elements.iter().position(|element| !element.is_required());
        let required_tuple_count = first_non_required.unwrap_or(fixed_tuple_count);
        if required_tuple_count > 0 {
            min_argument_count = fixed_parameter_count + required_tuple_count;
        }
    } else {
        position_types.extend(signature.params.iter().map(|parameter| {
            if parameter.rest {
                let normalized_rest = normalize_type(parameter.type_id);
                non_tuple_rest_position_type(db, normalized_rest)
            } else {
                fixed_parameter_read_type(db, parameter)
            }
        }));
    }

    let position_types: Vec<TypeId> = position_types
        .into_iter()
        .map(&mut normalize_type)
        .collect();
    while min_argument_count > 0 && type_accepts_void(db, position_types[min_argument_count - 1]) {
        min_argument_count -= 1;
    }

    EffectiveConstructParameters {
        min_argument_count,
        has_effective_rest,
        position_types,
    }
}

/// `getTypeAtPosition` for a fixed parameter.
///
/// Optional parameters read as `T | undefined`, independent of their declared
/// surface type. This makes `value?: void` identical to a required trailing
/// `value: void | undefined` once minimum arity is reduced for the latter.
fn fixed_parameter_read_type(db: &dyn TypeDatabase, parameter: &ParamInfo) -> TypeId {
    if parameter.optional {
        db.union2(parameter.type_id, TypeId::UNDEFINED)
    } else {
        parameter.type_id
    }
}

/// `getTypeAtPosition(signature, 0)` for a non-tuple effective rest.
///
/// Direct arrays expose their element, and `NoInfer` has already been erased
/// by signature-identity canonicalization. Generic or substitution rest types
/// retain a literal-zero indexed access rather than following constraints:
/// raw `...T` is `T[0]`, distinct from variadic tuple `[...T]`'s `T[number]`.
fn non_tuple_rest_position_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id == TypeId::ANY || type_id == TypeId::NEVER {
        return type_id;
    }

    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner)) => non_tuple_rest_position_type(db, inner),
        Some(TypeData::Array(element)) => element,
        Some(TypeData::Tuple(list_id)) => {
            let elements = db.tuple_list(list_id);
            let Some(first) = elements.first() else {
                return TypeId::UNDEFINED;
            };
            if first.rest {
                tuple_variable_position_type(db, &elements, 0)
            } else {
                tuple_element_read_type(db, first)
            }
        }
        _ => db.index_access(type_id, db.literal_number(0.0)),
    }
}

fn type_accepts_void(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::VOID {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&member| type_accepts_void(db, member)),
        _ => false,
    }
}

/// Return a tuple only when the parameter's semantic outer shape is an actual
/// tuple. Shared tuple helpers intentionally follow type-parameter constraints,
/// substitutions, intersections, and tuple-union compatibility views; tsc's
/// `getParameterCount`/`hasEffectiveRestParameter` do none of those.
fn actual_tuple_elements<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<Vec<TupleElement>> {
    let mut current = type_id;
    for _ in 0..32 {
        match db.lookup(current) {
            Some(TypeData::ReadonlyType(inner)) => current = inner,
            Some(TypeData::Tuple(list_id)) => {
                return Some(db.tuple_list(list_id).to_vec());
            }
            Some(TypeData::Lazy(def_id)) => {
                if resolver
                    .get_def_kind(def_id)
                    .is_some_and(|kind| kind != DefKind::TypeAlias)
                {
                    return None;
                }
                let exposed = expose_transparent_alias_once(db, resolver, current)?;
                if exposed == current {
                    return None;
                }
                current = exposed;
            }
            Some(TypeData::Application(application_id)) => {
                let application = db.type_application(application_id);
                let Some(TypeData::Lazy(def_id)) = db.lookup(application.base) else {
                    return None;
                };
                if resolver
                    .get_def_kind(def_id)
                    .is_some_and(|kind| kind != DefKind::TypeAlias)
                {
                    return None;
                }
                let exposed = expose_transparent_alias_once(db, resolver, current)?;
                if exposed == current {
                    return None;
                }
                current = exposed;
            }
            _ => return None,
        }
    }
    None
}

fn tuple_element_read_type(db: &dyn TypeDatabase, element: &TupleElement) -> TypeId {
    if element.optional {
        db.union2(element.type_id, TypeId::UNDEFINED)
    } else {
        element.type_id
    }
}

/// Type at tsc's one effective variadic position.
///
/// `getParameterCount` exposes only the fixed tuple prefix plus one position
/// for the entire variable suffix. Numeric indexing at that position unions
/// the spread operand's element type with every fixed suffix element. A generic
/// spread must remain `T[number]`; reducing through its constraint here would
/// make `...T` identical to `...T[]`.
fn tuple_variable_position_type(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
    variable_index: usize,
) -> TypeId {
    let mut members = Vec::with_capacity(elements.len() - variable_index);
    for element in &elements[variable_index..] {
        let type_id = if element.rest {
            exact_spread_element_type(db, element.type_id)
        } else {
            tuple_element_read_type(db, element)
        };
        members.push(type_id);
    }
    db.union(members)
}

fn exact_spread_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id == TypeId::ANY || type_id == TypeId::NEVER {
        return type_id;
    }

    let mut current = type_id;
    loop {
        match db.lookup(current) {
            Some(TypeData::ReadonlyType(inner)) => current = inner,
            Some(TypeData::Array(element)) => return element,
            Some(TypeData::Tuple(list_id)) => {
                let elements = db.tuple_list(list_id);
                let members = elements
                    .iter()
                    .map(|element| {
                        if element.rest {
                            exact_spread_element_type(db, element.type_id)
                        } else {
                            tuple_element_read_type(db, element)
                        }
                    })
                    .collect();
                return db.union(members);
            }
            _ => return db.index_access(type_id, TypeId::NUMBER),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::signatures_and_advanced::{
        get_construct_signatures, get_construct_signatures_with_resolver,
    };
    use crate::canonicalize::Canonicalizer;
    use crate::construction::TypeInterner;
    use crate::def::{DefId, DefKind};
    use crate::relations::subtype::TypeEnvironment;
    use crate::types::{
        CallSignature, CallableShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo,
    };

    fn fixed(type_id: TypeId, optional: bool) -> ParamInfo {
        ParamInfo {
            name: None,
            type_id,
            optional,
            rest: false,
        }
    }

    fn rest(type_id: TypeId) -> ParamInfo {
        ParamInfo {
            name: None,
            type_id,
            optional: false,
            rest: true,
        }
    }

    fn constructor(db: &TypeInterner, params: Vec<ParamInfo>) -> TypeId {
        let signature = CallSignature::new(params, TypeId::BOOLEAN);
        db.callable(CallableShape {
            construct_signatures: vec![signature],
            ..CallableShape::default()
        })
    }

    #[test]
    fn tuple_rest_and_positional_construct_signatures_deduplicate() {
        let db = TypeInterner::new();
        let tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::fixed(TypeId::NUMBER),
        ]);
        let tuple_rest = constructor(&db, vec![rest(tuple)]);
        let positional = constructor(
            &db,
            vec![fixed(TypeId::STRING, false), fixed(TypeId::NUMBER, false)],
        );

        let intersection = db.intersection2(tuple_rest, positional);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "fixed tuple rests and their positional spelling are one tsc signature identity"
        );
        assert!(
            signatures[0].params[0].rest,
            "appendSignatures keeps the first identical signature"
        );
    }

    #[test]
    fn tuple_rest_minimum_arity_remains_identity_bearing() {
        let db = TypeInterner::new();
        let optional_tuple = db.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        }]);
        let optional = constructor(&db, vec![rest(optional_tuple)]);
        let required = constructor(&db, vec![fixed(TypeId::STRING, false)]);

        let intersection = db.intersection2(optional, required);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            2,
            "different effective minimum argument counts are not identical"
        );
    }

    #[test]
    fn trailing_void_reduces_effective_minimum_arity() {
        let db = TypeInterner::new();
        let void_or_undefined = db.union2(TypeId::VOID, TypeId::UNDEFINED);
        let required = constructor(&db, vec![fixed(void_or_undefined, false)]);
        let optional = constructor(&db, vec![fixed(TypeId::VOID, true)]);

        let intersection = db.intersection2(required, optional);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "required trailing `void | undefined` and optional trailing `void` have the same effective identity"
        );
    }

    #[test]
    fn union_of_tuple_rest_stays_distinct_from_optional_parameter() {
        let db = TypeInterner::new();
        let empty = db.tuple(Vec::new());
        let single = db.tuple(vec![TupleElement::fixed(TypeId::STRING)]);
        let tuple_union = db.union(vec![empty, single]);
        let union_rest = constructor(&db, vec![rest(tuple_union)]);
        let optional = constructor(&db, vec![fixed(TypeId::STRING, true)]);

        let intersection = db.intersection2(union_rest, optional);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            2,
            "tsc's exact identity keeps a union-of-tuples rest effective even when relation helpers view it as one optional parameter"
        );
    }

    #[test]
    fn variadic_tuple_suffix_matches_one_effective_rest_position() {
        let db = TypeInterner::new();
        let variadic_tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::rest(db.array(TypeId::NUMBER)),
            TupleElement::fixed(TypeId::BOOLEAN),
        ]);
        let tuple_rest = constructor(&db, vec![rest(variadic_tuple)]);
        let effective_rest = constructor(
            &db,
            vec![
                fixed(TypeId::STRING, false),
                rest(db.array(db.union2(TypeId::NUMBER, TypeId::BOOLEAN))),
            ],
        );

        let intersection = db.intersection2(tuple_rest, effective_rest);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "tsc exposes a variadic tuple's fixed prefix plus one rest position whose type includes the fixed suffix"
        );
    }

    #[test]
    fn generic_variadic_tuple_is_not_an_array_of_the_type_parameter() {
        let db = TypeInterner::new();
        let t_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("T"))
        };
        let t = db.type_param(t_info);
        let mut tuple_signature = CallSignature::new(
            vec![rest(db.tuple(vec![TupleElement::rest(t)]))],
            TypeId::BOOLEAN,
        );
        tuple_signature.type_params.push(t_info);

        let u_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("U"))
        };
        let u = db.type_param(u_info);
        let mut array_signature = CallSignature::new(vec![rest(db.array(u))], TypeId::BOOLEAN);
        array_signature.type_params.push(u_info);

        let tuple_constructor = db.callable(CallableShape {
            construct_signatures: vec![tuple_signature],
            ..CallableShape::default()
        });
        let array_constructor = db.callable(CallableShape {
            construct_signatures: vec![array_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(tuple_constructor, array_constructor);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            2,
            "`[...T]` contributes `T[number]`, whereas `...T[]` contributes `T`"
        );
    }

    #[test]
    fn generic_variadic_tuple_suffix_matches_indexed_effective_rest() {
        let db = TypeInterner::new();
        let t_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("T"))
        };
        let t = db.type_param(t_info);
        let tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::rest(t),
            TupleElement::fixed(TypeId::BOOLEAN),
        ]);
        let mut tuple_signature = CallSignature::new(vec![rest(tuple)], TypeId::BOOLEAN);
        tuple_signature.type_params.push(t_info);

        let u_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("U"))
        };
        let u = db.type_param(u_info);
        let effective_tail = db.union2(db.index_access(u, TypeId::NUMBER), TypeId::BOOLEAN);
        let mut effective_signature = CallSignature::new(
            vec![fixed(TypeId::STRING, false), rest(db.array(effective_tail))],
            TypeId::BOOLEAN,
        );
        effective_signature.type_params.push(u_info);

        let tuple_constructor = db.callable(CallableShape {
            construct_signatures: vec![tuple_signature],
            ..CallableShape::default()
        });
        let effective_constructor = db.callable(CallableShape {
            construct_signatures: vec![effective_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(tuple_constructor, effective_constructor);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "the effective variadic slot preserves `T[number]` and unions the fixed suffix"
        );
    }

    #[test]
    fn constrained_type_parameter_rest_is_not_an_actual_tuple_rest() {
        let db = TypeInterner::new();
        let tuple_constraint = db.tuple(vec![TupleElement::fixed(TypeId::STRING)]);
        let t_info = TypeParamInfo {
            constraint: Some(tuple_constraint),
            ..TypeParamInfo::simple(db.intern_string("T"))
        };
        let t = db.type_param(t_info);
        let mut constrained_rest = CallSignature::new(vec![rest(t)], TypeId::BOOLEAN);
        constrained_rest.type_params.push(t_info);

        let u_info = TypeParamInfo {
            constraint: Some(tuple_constraint),
            ..TypeParamInfo::simple(db.intern_string("U"))
        };
        let mut fixed_signature =
            CallSignature::new(vec![fixed(TypeId::STRING, false)], TypeId::BOOLEAN);
        fixed_signature.type_params.push(u_info);

        let constrained_constructor = db.callable(CallableShape {
            construct_signatures: vec![constrained_rest],
            ..CallableShape::default()
        });
        let fixed_constructor = db.callable(CallableShape {
            construct_signatures: vec![fixed_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(constrained_constructor, fixed_constructor);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            2,
            "tsc does not use a type parameter's tuple constraint for `getParameterCount` or `hasEffectiveRestParameter`"
        );
    }

    #[test]
    fn optional_tuple_slot_matches_optional_parameter_read_type() {
        let db = TypeInterner::new();
        let optional_tuple = db.tuple(vec![TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        }]);
        let tuple_rest = constructor(&db, vec![rest(optional_tuple)]);
        let optional_parameter = constructor(
            &db,
            vec![fixed(db.union2(TypeId::STRING, TypeId::UNDEFINED), true)],
        );

        let intersection = db.intersection2(tuple_rest, optional_parameter);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "reading an optional tuple slot includes `undefined`, like an optional positional parameter"
        );
    }

    #[test]
    fn resolver_exposes_direct_renamed_and_readonly_tuple_aliases() {
        let db = TypeInterner::new();
        let tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::fixed(TypeId::NUMBER),
        ]);
        let direct_def = DefId(100);
        let renamed_def = DefId(101);
        let readonly_def = DefId(102);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def(direct_def, tuple);
        resolver.insert_def_kind(direct_def, DefKind::TypeAlias);
        resolver.insert_def(renamed_def, db.lazy(direct_def));
        resolver.insert_def_kind(renamed_def, DefKind::TypeAlias);
        resolver.insert_def(readonly_def, db.readonly_type(db.lazy(renamed_def)));
        resolver.insert_def_kind(readonly_def, DefKind::TypeAlias);

        let positional = constructor(
            &db,
            vec![fixed(TypeId::STRING, false), fixed(TypeId::NUMBER, false)],
        );
        for alias_def in [direct_def, renamed_def, readonly_def] {
            let alias_rest = constructor(&db, vec![rest(db.lazy(alias_def))]);
            let intersection = db.intersection2(alias_rest, positional);

            let resolver_free = get_construct_signatures(&db, intersection)
                .expect("resolver-free construct signatures");
            assert_eq!(
                resolver_free.len(),
                2,
                "a resolver-free projection cannot expose the alias"
            );

            let resolved = get_construct_signatures_with_resolver(&db, &resolver, intersection)
                .expect("resolver-aware construct signatures");
            assert_eq!(
                resolved.len(),
                1,
                "a prior resolver-free memo must not shadow tuple-alias identity"
            );
        }
    }

    #[test]
    fn noinfer_constructor_wrapper_is_transparent_to_signature_projection() {
        let db = TypeInterner::new();
        let wrapped = constructor(&db, vec![fixed(TypeId::STRING, false)]);
        let plain = constructor(&db, vec![fixed(TypeId::NUMBER, false)]);
        let no_infer = db.no_infer(wrapped);

        let wrapped_signatures =
            get_construct_signatures(&db, no_infer).expect("wrapped construct signature");
        assert_eq!(wrapped_signatures.len(), 1);
        assert_eq!(wrapped_signatures[0].params[0].type_id, TypeId::STRING);

        let intersection = db.intersection2(no_infer, plain);
        assert!(
            matches!(db.lookup(intersection), Some(TypeData::Intersection(_))),
            "subtype reduction must preserve a constructor wrapped in `NoInfer`"
        );
        let signatures =
            get_construct_signatures(&db, intersection).expect("ordered construct signatures");
        assert_eq!(
            signatures.len(),
            2,
            "tsc projects both `NoInfer<Constructor>` and the ordinary constructor"
        );
        assert_eq!(signatures[0].params[0].type_id, TypeId::STRING);
        assert_eq!(signatures[1].params[0].type_id, TypeId::NUMBER);
    }

    #[test]
    fn resolver_projects_constructor_alias_containers_without_caching_partial_results() {
        let db = TypeInterner::new();
        let alias_def = DefId(105);
        let alias_constructor = constructor(&db, vec![fixed(TypeId::STRING, false)]);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def(alias_def, alias_constructor);
        resolver.insert_def_kind(alias_def, DefKind::TypeAlias);
        let lazy_alias = db.lazy(alias_def);

        assert!(
            get_construct_signatures(&db, lazy_alias).is_none(),
            "the resolver-free query cannot expose a lazy alias"
        );
        assert_eq!(
            get_construct_signatures_with_resolver(&db, &resolver, lazy_alias)
                .expect("resolved alias signature")
                .len(),
            1
        );

        let plain = constructor(&db, vec![fixed(TypeId::NUMBER, false)]);
        let partial_intersection = db.intersection2(lazy_alias, plain);
        assert_eq!(
            get_construct_signatures(&db, partial_intersection)
                .expect("resolver-free direct signature")
                .len(),
            1
        );
        let complete = get_construct_signatures_with_resolver(&db, &resolver, partial_intersection)
            .expect("resolver-aware overload set");
        assert_eq!(
            complete.len(),
            2,
            "a resolver-free partial projection must not poison the shared memo"
        );
        assert_eq!(complete[0].params[0].type_id, TypeId::STRING);
        assert_eq!(complete[1].params[0].type_id, TypeId::NUMBER);

        let negative_intersection = db.intersection(vec![lazy_alias, TypeId::OBJECT]);
        assert!(get_construct_signatures(&db, negative_intersection).is_none());
        assert_eq!(
            get_construct_signatures_with_resolver(&db, &resolver, negative_intersection)
                .expect("resolver-aware signature after a resolver-free miss")
                .len(),
            1,
            "a resolver-free negative projection must not be published"
        );
    }

    #[test]
    fn resolver_projects_generic_constructor_alias_application() {
        let db = TypeInterner::new();
        let alias_def = DefId(106);
        let alias_param = TypeParamInfo::simple(db.intern_string("Value"));
        let alias_param_type = db.type_param(alias_param);
        let alias_body = constructor(&db, vec![fixed(alias_param_type, false)]);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def_with_params(alias_def, alias_body, vec![alias_param]);
        resolver.insert_def_kind(alias_def, DefKind::TypeAlias);

        let application = db.application(db.lazy(alias_def), vec![TypeId::STRING]);
        let signatures = get_construct_signatures_with_resolver(&db, &resolver, application)
            .expect("generic constructor alias application");
        assert_eq!(signatures.len(), 1);
        assert_eq!(
            signatures[0].params[0].type_id,
            TypeId::STRING,
            "the alias body is instantiated before signature projection"
        );
    }

    #[test]
    fn distinct_tuple_aliases_share_one_resolved_signature_identity() {
        let db = TypeInterner::new();
        let tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::fixed(TypeId::NUMBER),
        ]);
        let left_def = DefId(110);
        let right_def = DefId(111);
        let mut resolver = TypeEnvironment::new();
        for def_id in [left_def, right_def] {
            resolver.insert_def(def_id, tuple);
            resolver.insert_def_kind(def_id, DefKind::TypeAlias);
        }

        let left = constructor(&db, vec![rest(db.lazy(left_def))]);
        let right = constructor(&db, vec![rest(db.lazy(right_def))]);
        let intersection = db.intersection2(left, right);
        let signatures = get_construct_signatures_with_resolver(&db, &resolver, intersection)
            .expect("resolver-aware construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "distinct transparent aliases to the same tuple are one tsc identity"
        );
    }

    #[test]
    fn resolver_canonicalizes_aliases_in_ordinary_parameters_and_returns() {
        let db = TypeInterner::new();
        let parameter_left = DefId(120);
        let parameter_right = DefId(121);
        let return_left = DefId(122);
        let return_right = DefId(123);
        let mut resolver = TypeEnvironment::new();
        for def_id in [parameter_left, parameter_right, return_left, return_right] {
            resolver.insert_def(def_id, TypeId::STRING);
            resolver.insert_def_kind(def_id, DefKind::TypeAlias);
        }

        let left_signature = CallSignature::new(
            vec![fixed(db.lazy(parameter_left), false)],
            db.lazy(return_left),
        );
        let right_signature = CallSignature::new(
            vec![fixed(db.lazy(parameter_right), false)],
            db.lazy(return_right),
        );
        let left = db.callable(CallableShape {
            construct_signatures: vec![left_signature],
            ..CallableShape::default()
        });
        let right = db.callable(CallableShape {
            construct_signatures: vec![right_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(left, right);

        assert_eq!(
            get_construct_signatures(&db, intersection)
                .expect("resolver-free construct signatures")
                .len(),
            2
        );
        assert_eq!(
            get_construct_signatures_with_resolver(&db, &resolver, intersection)
                .expect("resolver-aware construct signatures")
                .len(),
            1,
            "resolver-backed canonical identity covers ordinary parameters and returns"
        );
    }

    #[test]
    fn generic_tuple_alias_application_expands_before_effective_arity() {
        let db = TypeInterner::new();
        let alias_def = DefId(130);
        let alias_param = TypeParamInfo::simple(db.intern_string("Element"));
        let alias_param_type = db.type_param(alias_param);
        let alias_body = db.tuple(vec![
            TupleElement::fixed(alias_param_type),
            TupleElement::fixed(TypeId::NUMBER),
        ]);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def_with_params(alias_def, alias_body, vec![alias_param]);
        resolver.insert_def_kind(alias_def, DefKind::TypeAlias);

        let alias_application = db.application(db.lazy(alias_def), vec![TypeId::STRING]);
        let alias_rest = constructor(&db, vec![rest(alias_application)]);
        let positional = constructor(
            &db,
            vec![fixed(TypeId::STRING, false), fixed(TypeId::NUMBER, false)],
        );
        let intersection = db.intersection2(alias_rest, positional);
        let signatures = get_construct_signatures_with_resolver(&db, &resolver, intersection)
            .expect("resolver-aware construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "generic tuple aliases instantiate before tsc's effective-parameter comparison"
        );
    }

    #[test]
    fn noinfer_tuple_alias_remains_an_effective_rest_parameter() {
        let db = TypeInterner::new();
        let tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::fixed(TypeId::NUMBER),
        ]);
        let alias_def = DefId(140);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def(alias_def, db.no_infer(tuple));
        resolver.insert_def_kind(alias_def, DefKind::TypeAlias);

        let alias_rest = constructor(&db, vec![rest(db.lazy(alias_def))]);
        let positional = constructor(
            &db,
            vec![fixed(TypeId::STRING, false), fixed(TypeId::NUMBER, false)],
        );
        let intersection = db.intersection2(alias_rest, positional);
        let signatures = get_construct_signatures_with_resolver(&db, &resolver, intersection)
            .expect("resolver-aware construct signatures");

        assert_eq!(
            signatures.len(),
            2,
            "alias exposure stops at the identity-bearing `NoInfer` wrapper"
        );
    }

    #[test]
    fn concrete_constructor_intersection_survives_until_effective_identity_projection() {
        let db = TypeInterner::new();
        let tuple = db.tuple(vec![
            TupleElement::fixed(TypeId::STRING),
            TupleElement::fixed(TypeId::NUMBER),
        ]);
        let wrapped_rest = constructor(&db, vec![rest(db.no_infer(tuple))]);
        let positional = constructor(
            &db,
            vec![fixed(TypeId::STRING, false), fixed(TypeId::NUMBER, false)],
        );
        let intersection = db.intersection2(wrapped_rest, positional);
        assert!(
            matches!(db.lookup(intersection), Some(TypeData::Intersection(_))),
            "subtype reduction must not erase an ordered constructor candidate before projection"
        );
        let evaluated = crate::evaluation::evaluate::evaluate_type(&db, intersection);
        assert!(
            matches!(db.lookup(evaluated), Some(TypeData::Intersection(_))),
            "evaluation-time subtype simplification must preserve the same overload set"
        );

        let signatures = get_construct_signatures(&db, evaluated)
            .expect("both construct signatures remain available");
        assert_eq!(
            signatures.len(),
            2,
            "`NoInfer<[string, number]>` has non-tuple effective-rest identity despite transparent assignability"
        );
    }

    #[test]
    fn resolver_sensitive_gate_excludes_known_nominal_lazy_types() {
        let db = TypeInterner::new();
        let alias_def = DefId(150);
        let class_def = DefId(151);
        let unknown_def = DefId(152);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def_kind(alias_def, DefKind::TypeAlias);
        resolver.insert_def_kind(class_def, DefKind::Class);

        let alias_signature =
            CallSignature::new(vec![fixed(db.lazy(alias_def), false)], TypeId::BOOLEAN);
        let class_signature =
            CallSignature::new(vec![fixed(db.lazy(class_def), false)], TypeId::BOOLEAN);
        let unknown_signature =
            CallSignature::new(vec![fixed(db.lazy(unknown_def), false)], TypeId::BOOLEAN);

        assert!(super::construct_signature_identity_requires_resolver(
            &db,
            &resolver,
            &alias_signature
        ));
        assert!(
            !super::construct_signature_identity_requires_resolver(
                &db,
                &resolver,
                &class_signature
            ),
            "known nominal definitions have the same Lazy identity with or without a resolver"
        );
        assert!(
            super::construct_signature_identity_requires_resolver(
                &db,
                &resolver,
                &unknown_signature
            ),
            "unknown definition kinds conservatively bypass resolver-free projection memos"
        );
    }

    #[test]
    fn canonicalizer_modes_preserve_or_erase_nested_noinfer() {
        let db = TypeInterner::new();
        let resolver = TypeEnvironment::new();
        let wrapped = db.no_infer(TypeId::STRING);
        let nested = db.array(wrapped);
        let plain_nested = db.array(TypeId::STRING);

        let mut default_canonicalizer = Canonicalizer::new(&db, &resolver);
        assert_eq!(
            default_canonicalizer.canonicalize(nested),
            nested,
            "the default canonical form preserves `NoInfer`"
        );

        let mut identity_canonicalizer = Canonicalizer::for_signature_identity(&db, &resolver);
        assert_eq!(
            identity_canonicalizer.canonicalize(nested),
            plain_nested,
            "signature identity recursively treats `NoInfer<T>` as `T`"
        );
    }

    #[test]
    fn noinfer_is_transparent_after_effective_parameter_shape_is_fixed() {
        let db = TypeInterner::new();
        let wrapped = db.no_infer(TypeId::STRING);
        let wrapped_signature = CallSignature::new(vec![fixed(wrapped, false)], wrapped);
        let plain_signature =
            CallSignature::new(vec![fixed(TypeId::STRING, false)], TypeId::STRING);
        let wrapped_constructor = db.callable(CallableShape {
            construct_signatures: vec![wrapped_signature],
            ..CallableShape::default()
        });
        let plain_constructor = db.callable(CallableShape {
            construct_signatures: vec![plain_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(wrapped_constructor, plain_constructor);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "`NoInfer` is transparent to parameter/return type identity once arity has been computed"
        );
    }

    #[test]
    fn noinfer_array_rests_match_direct_arrays_through_aliases() {
        let db = TypeInterner::new();
        let array = db.array(TypeId::STRING);
        let direct_wrapped = constructor(&db, vec![rest(db.no_infer(array))]);
        let direct_array = constructor(&db, vec![rest(array)]);
        let direct_intersection = db.intersection2(direct_wrapped, direct_array);
        assert_eq!(
            get_construct_signatures(&db, direct_intersection)
                .expect("construct signatures")
                .len(),
            1,
            "`NoInfer<string[]>` and `string[]` expose the same effective rest position"
        );

        let alias_def = DefId(160);
        let mut resolver = TypeEnvironment::new();
        resolver.insert_def(alias_def, db.no_infer(db.no_infer(array)));
        resolver.insert_def_kind(alias_def, DefKind::TypeAlias);
        let aliased = constructor(&db, vec![rest(db.lazy(alias_def))]);
        let aliased_intersection = db.intersection2(aliased, direct_array);
        assert_eq!(
            get_construct_signatures_with_resolver(&db, &resolver, aliased_intersection)
                .expect("resolver-aware construct signatures")
                .len(),
            1,
            "alias and nested `NoInfer` wrappers remain transparent for array-element identity"
        );
    }

    #[test]
    fn raw_generic_rest_keeps_literal_index_distinct_from_variadic_tuple() {
        let db = TypeInterner::new();
        let t_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("T"))
        };
        let t = db.type_param(t_info);
        let mut raw_signature = CallSignature::new(vec![rest(t)], TypeId::BOOLEAN);
        raw_signature.type_params.push(t_info);

        let u_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("U"))
        };
        let u = db.type_param(u_info);
        let mut tuple_signature = CallSignature::new(
            vec![rest(db.tuple(vec![TupleElement::rest(u)]))],
            TypeId::BOOLEAN,
        );
        tuple_signature.type_params.push(u_info);

        let raw_constructor = db.callable(CallableShape {
            construct_signatures: vec![raw_signature],
            ..CallableShape::default()
        });
        let tuple_constructor = db.callable(CallableShape {
            construct_signatures: vec![tuple_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(raw_constructor, tuple_constructor);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            2,
            "raw `...T` contributes `T[0]`, while tuple `[...T]` contributes `T[number]`"
        );

        let mut indexed_array_signature = CallSignature::new(
            vec![rest(db.array(db.index_access(u, TypeId::NUMBER)))],
            TypeId::BOOLEAN,
        );
        indexed_array_signature.type_params.push(u_info);
        let indexed_array_constructor = db.callable(CallableShape {
            construct_signatures: vec![indexed_array_signature],
            ..CallableShape::default()
        });
        let indexed_intersection = db.intersection2(raw_constructor, indexed_array_constructor);
        assert_eq!(
            get_construct_signatures(&db, indexed_intersection)
                .expect("construct signatures")
                .len(),
            2,
            "raw `...T` must not follow its constraint or widen `T[0]` to `T[number]`"
        );
    }

    #[test]
    fn variadic_tuple_matches_array_of_generic_numeric_index() {
        let db = TypeInterner::new();
        let t_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("T"))
        };
        let t = db.type_param(t_info);
        let mut tuple_signature = CallSignature::new(
            vec![rest(db.tuple(vec![TupleElement::rest(t)]))],
            TypeId::BOOLEAN,
        );
        tuple_signature.type_params.push(t_info);

        let u_info = TypeParamInfo {
            constraint: Some(db.array(TypeId::ANY)),
            ..TypeParamInfo::simple(db.intern_string("U"))
        };
        let u = db.type_param(u_info);
        let mut indexed_array_signature = CallSignature::new(
            vec![rest(db.array(db.index_access(u, TypeId::NUMBER)))],
            TypeId::BOOLEAN,
        );
        indexed_array_signature.type_params.push(u_info);

        let tuple_constructor = db.callable(CallableShape {
            construct_signatures: vec![tuple_signature],
            ..CallableShape::default()
        });
        let indexed_array_constructor = db.callable(CallableShape {
            construct_signatures: vec![indexed_array_signature],
            ..CallableShape::default()
        });
        let intersection = db.intersection2(tuple_constructor, indexed_array_constructor);
        let signatures = get_construct_signatures(&db, intersection).expect("construct signatures");

        assert_eq!(
            signatures.len(),
            1,
            "tuple `[...T]` and array rest `T[number][]` share one effective position"
        );
    }
}
