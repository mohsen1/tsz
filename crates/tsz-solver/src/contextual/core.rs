use crate::construction::TypeDatabase;

use crate::contextual::extractors::{
    ApplicationArgExtractor, ArrayElementExtractor, ParameterExtractor, ParameterForCallExtractor,
    PropertyExtractor, RestOrOptionalTailPositionExtractor, RestParameterExtractor,
    RestPositionCheckExtractor, ReturnTypeExtractor, ThisTypeExtractor, ThisTypeMarkerExtractor,
    TupleElementExtractor, collect_from_intersection, collect_single_or_union,
    collect_single_or_union_no_reduce, extract_param_type_at_for_call,
};

#[cfg(test)]
use crate::types::*;

use crate::types::{IntrinsicKind, TypeData, TypeId};

/// Context for contextual typing.
/// Holds the expected type and provides methods to extract type information.
pub struct ContextualTypeContext<'a> {
    interner: &'a dyn TypeDatabase,
    /// The expected type (contextual type)
    expected: Option<TypeId>,
    /// Whether noImplicitAny is enabled (affects contextual typing for multi-signature functions)
    no_implicit_any: bool,
}

/// Extract the per-argument contextual type from a rest parameter type.
///
/// For array rest params like `...args: Foo[]`, this returns `Foo`.
/// For tuple rest params, this returns the trailing rest element type when present.
/// Evaluatable wrappers such as `ConstructorParameters<T>` are normalized first so
/// generic call round-2 contextual typing doesn't pass the whole tuple application
/// through as a single argument type.
pub fn rest_argument_element_type(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    fn rest_argument_element_type_inner(
        db: &dyn crate::construction::TypeDatabase,
        type_id: TypeId,
        depth: usize,
    ) -> TypeId {
        if depth == 0 {
            return type_id;
        }
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Fast path: intrinsics aren't `ReadonlyType` / `NoInfer` /
        // `TypeParameter` / `Infer` / `Union` / `Array` / `Tuple` /
        // `Application` / `Conditional` / `Mapped` / `Lazy` / `IndexAccess`,
        // so the function falls through to `_ => type_id` for them.
        if type_id.is_intrinsic() {
            return type_id;
        }

        match db.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                rest_argument_element_type_inner(db, inner, depth - 1)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .filter(|&constraint| constraint != type_id)
                .map(|constraint| rest_argument_element_type_inner(db, constraint, depth - 1))
                .unwrap_or(type_id),
            Some(TypeData::Union(members_id)) => {
                let members = db.type_list(members_id);
                let extracted: Vec<_> = members
                    .iter()
                    .map(|&member| rest_argument_element_type_inner(db, member, depth - 1))
                    .collect();
                crate::utils::union_or_single(db, extracted)
            }
            Some(TypeData::Array(elem)) => elem,
            Some(TypeData::Tuple(elements_id)) => {
                let elements = db.tuple_list(elements_id);
                if let Some(last) = elements.last() {
                    if last.rest {
                        match db.lookup(last.type_id) {
                            Some(TypeData::Array(elem)) => elem,
                            _ => last.type_id,
                        }
                    } else {
                        last.type_id
                    }
                } else {
                    type_id
                }
            }
            Some(
                TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::Mapped(_)
                | TypeData::Lazy(_)
                | TypeData::IndexAccess(_, _),
            ) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
                if evaluated != type_id {
                    rest_argument_element_type_inner(db, evaluated, depth - 1)
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    rest_argument_element_type_inner(db, type_id, 8)
}

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");

/// Substitute K with the index literal in a homomorphic mapped type's
/// template, recovering per-element contextual info when evaluation cannot
/// reduce the mapped to a concrete tuple (e.g., source X is still generic).
/// Refuses when key remapping or constraint shape would misalign positional
/// indices with the mapped's key domain.
fn try_mapped_per_index_template(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    index: usize,
) -> Option<TypeId> {
    let mapped = db.mapped_type(mapped_id);

    if !crate::type_queries::is_identity_name_mapping(db, &mapped) {
        return None;
    }
    if !constraint_iterates_positional_keys(db, mapped.constraint) {
        return None;
    }
    if !crate::type_queries::template_references_iter_param(
        db,
        mapped.template,
        mapped.type_param.name,
    ) {
        return None;
    }
    if template_has_nested_same_name_source_key_collision(
        db,
        mapped.template,
        mapped.type_param.name,
    ) {
        return None;
    }

    let key_literal = db.literal_number(index as f64);
    Some(
        crate::type_queries::instantiate_mapped_template_for_property(
            db,
            mapped.template,
            mapped.type_param.name,
            key_literal,
        ),
    )
}

/// Per-index contextual typing substitutes by the mapped key name. A direct
/// `T[K]` template has a structural fast path in
/// `instantiate_mapped_template_for_property`, but nested shapes such as
/// `(v: P[P]) => void` can otherwise replace an outer source `P` as well as
/// the mapped key `P`. Refuse those nested collisions so callers fall back to
/// the existing non-positional contextual path instead of producing a wrong
/// per-element type.
fn template_has_nested_same_name_source_key_collision(
    db: &dyn TypeDatabase,
    template: TypeId,
    iter_name: tsz_common::Atom,
) -> bool {
    if template.is_intrinsic() {
        return false;
    }
    if matches!(db.lookup(template), Some(TypeData::IndexAccess(_, _))) {
        return false;
    }

    crate::contains_type_matching(db, template, |key| match key {
        TypeData::IndexAccess(object, index) => {
            crate::contains_type_parameter_named_shallow(db, *object, iter_name)
                && crate::contains_type_parameter_named_shallow(db, *index, iter_name)
        }
        _ => false,
    })
}

/// Whether the mapped's iteration domain includes positional numeric keys —
/// `keyof X`, the `number` intrinsic, or an intersection of those. Intersections
/// are canonicalized/flattened so a single level of recursion is sufficient.
fn constraint_iterates_positional_keys(db: &dyn TypeDatabase, constraint: TypeId) -> bool {
    if constraint == TypeId::NUMBER {
        return true;
    }
    if constraint.is_intrinsic() {
        return false;
    }
    match db.lookup(constraint) {
        Some(TypeData::KeyOf(_)) => true,
        Some(TypeData::Intersection(members)) => db
            .type_list(members)
            .iter()
            .any(|&m| constraint_iterates_positional_keys(db, m)),
        _ => false,
    }
}

/// Apply contextual type to infer a more specific type.
///
/// This implements bidirectional type inference:
/// 1. If `expr_type` is any/unknown/error, use contextual type
/// 2. If `expr_type` is a literal and contextual type is a union containing that literal's base type, preserve literal
/// 3. If `expr_type` is assignable to contextual type and is more specific, use `expr_type`
/// 4. Otherwise, prefer `expr_type` (don't widen to contextual type)
pub fn apply_contextual_type(
    interner: &dyn TypeDatabase,
    expr_type: TypeId,
    contextual_type: Option<TypeId>,
) -> TypeId {
    let ctx_type = match contextual_type {
        Some(t) => t,
        None => return expr_type,
    };

    // If expression type is any, unknown, or error, use contextual type
    if expr_type.is_any_or_unknown() || expr_type.is_error() {
        return ctx_type;
    }

    // If expression type is the same, just return it
    if expr_type == ctx_type {
        return expr_type;
    }

    // Check if expr_type is a literal type that should be preserved
    // When contextual type is a union like string | number, we should preserve literal types
    if let Some(expr_key) = interner.lookup(expr_type) {
        // Literal types should be preserved when context is a union
        if matches!(expr_key, TypeData::Literal(_))
            && let Some(ctx_key) = interner.lookup(ctx_type)
            && matches!(ctx_key, TypeData::Union(_))
        {
            // Preserve the literal type - it's more specific than the union
            return expr_type;
        }
    }

    // PERF: Reuse a single SubtypeChecker across all subtype checks in this function
    let mut checker = crate::relations::subtype::SubtypeChecker::new(interner);

    // Check if contextual type is a union
    if let Some(TypeData::Union(members)) = interner.lookup(ctx_type) {
        let members = interner.type_list(members);
        // If expr_type is in the union, it's valid - use the more specific expr_type
        for &member in members.iter() {
            if member == expr_type {
                return expr_type;
            }
        }
        // If expr_type is assignable to any union member, use expr_type
        for &member in members.iter() {
            checker.reset();
            if checker.is_subtype_of(expr_type, member) {
                return expr_type;
            }
        }
    }

    // If expr_type is assignable to contextual type, use expr_type (it's more specific)
    checker.reset();
    if checker.is_subtype_of(expr_type, ctx_type) {
        return expr_type;
    }

    // Default: prefer the expression type.
    //
    // When the contextual type is narrower than the expression type (e.g.,
    // ctx = "foo", expr = string), we must NOT substitute the contextual type.
    // The expression genuinely has the wider type at runtime, and substituting
    // the narrower contextual type would mask real assignability errors like
    // TS2322: Type 'string' is not assignable to type '"foo"'.
    //
    // The assignability checker is responsible for catching mismatches between
    // the expression type and the target type — this function should not
    // pre-narrow the expression type to hide those mismatches.
    expr_type
}

#[cfg(test)]
#[path = "../../tests/contextual_tests.rs"]
mod tests;
