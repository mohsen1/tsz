use tsz_solver::construction::TypeDatabase;
use tsz_solver::narrowing::NullishFilter;
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::{PropertyInfo, TypeId};

/// Re-export of the solver's binary operation result type.
///
/// Wraps `tsz_solver::operations::BinaryOpResult`.
/// This is the result enum returned by binary operation evaluation.
pub(crate) use tsz_solver::operations::BinaryOpResult;

pub(crate) fn evaluate_contextual_structure_with(
    db: &dyn tsz_solver::construction::QueryDatabase,
    type_id: TypeId,
    evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    tsz_solver::type_queries::evaluate_contextual_structure_with(db, type_id, evaluate_leaf)
}

pub(crate) fn evaluate_plus_chain(
    db: &dyn tsz_solver::construction::QueryDatabase,
    operand_types: &[TypeId],
) -> Option<TypeId> {
    tsz_solver::operations::BinaryOpEvaluator::new(db).evaluate_plus_chain(operand_types)
}

pub(crate) fn is_arithmetic_operand(
    db: &dyn tsz_solver::construction::QueryDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::operations::BinaryOpEvaluator::new(db).is_arithmetic_operand(type_id)
}

pub(crate) fn is_bigint_like(
    db: &dyn tsz_solver::construction::QueryDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::operations::BinaryOpEvaluator::new(db).is_bigint_like(type_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteTargetLogicalOperator {
    LogicalOr,
    NullishCoalescing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteTargetLogicalResult {
    Type(TypeId),
    FallbackToLogicalExpression,
}

pub(crate) fn write_target_logical_result_type(
    db: &dyn tsz_solver::construction::QueryDatabase,
    operator: WriteTargetLogicalOperator,
    left_type: TypeId,
    right_type: TypeId,
) -> Option<WriteTargetLogicalResult> {
    let ctx = tsz_solver::narrowing::NarrowingContext::new(db);
    let left_result = match operator {
        WriteTargetLogicalOperator::LogicalOr => {
            let truthy_left = ctx.narrow_by_truthiness(left_type);
            let falsy_left = ctx.narrow_to_falsy(left_type);
            if truthy_left == TypeId::NEVER || falsy_left == TypeId::NEVER {
                return Some(WriteTargetLogicalResult::FallbackToLogicalExpression);
            }
            truthy_left
        }
        WriteTargetLogicalOperator::NullishCoalescing => {
            let non_nullish_left =
                ctx.narrow_by_nullishness(left_type, NullishFilter::ExcludeNullish);
            let nullish_left = ctx.narrow_by_nullishness(left_type, NullishFilter::KeepNullish);
            if non_nullish_left == TypeId::NEVER || nullish_left == TypeId::NEVER {
                return Some(WriteTargetLogicalResult::FallbackToLogicalExpression);
            }
            non_nullish_left
        }
    };
    let members = [left_result, right_type];
    let normalized =
        crate::query_boundaries::common::normalize_object_union_members_for_write_target(
            db, &members,
        )?;
    Some(WriteTargetLogicalResult::Type(
        tsz_solver::utils::union_or_single(db, normalized),
    ))
}

// ---------------------------------------------------------------------------
// Expression operation boundary wrappers
// ---------------------------------------------------------------------------

/// Compute the result type of a conditional (ternary) expression.
pub(crate) fn compute_conditional_expression_type(
    db: &dyn TypeDatabase,
    condition: TypeId,
    true_type: TypeId,
    false_type: TypeId,
) -> TypeId {
    tsz_solver::operations::expression_ops::compute_conditional_expression_type(
        db, condition, true_type, false_type,
    )
}

/// Merge a single object-spread property contribution through the solver-owned
/// AST-independent spread merge rule.
pub(crate) fn merge_object_spread_property(
    db: &dyn TypeDatabase,
    exact_optional_property_types: bool,
    earlier: Option<&PropertyInfo>,
    spread: &PropertyInfo,
) -> PropertyInfo {
    tsz_solver::operations::expression_ops::merge_object_spread_property(
        db,
        exact_optional_property_types,
        earlier,
        spread,
    )
}

/// Compute the best common type from a list of element types.
pub(crate) fn compute_best_common_type<R: TypeResolver>(
    db: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    tsz_solver::operations::expression_ops::compute_best_common_type(db, types, resolver)
}

/// Cache-aware variant: thread `&dyn QueryDatabase` so the cross-call
/// subtype-reduction cache on `QueryCache` can collapse the O(N²) loop
/// in `remove_subtypes_for_bct` for repeated BCT call sites.
pub(crate) fn compute_best_common_type_cached<R: TypeResolver>(
    db: &dyn TypeDatabase,
    query_db: Option<&dyn tsz_solver::construction::QueryDatabase>,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    tsz_solver::operations::expression_ops::compute_best_common_type_cached(
        db, query_db, types, resolver,
    )
}

/// Check whether a contextual type is suitable for template literal narrowing.
pub(crate) fn is_template_literal_contextual_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::operations::expression_ops::is_template_literal_contextual_type(db, type_id)
}

/// Compute the type of a template literal expression with contextual typing.
pub(crate) fn compute_template_expression_type_contextual(
    db: &dyn TypeDatabase,
    texts: &[String],
    parts: &[TypeId],
) -> TypeId {
    tsz_solver::operations::expression_ops::compute_template_expression_type_contextual(
        db, texts, parts,
    )
}

/// Compute the type of a template literal expression without contextual typing.
pub(crate) fn compute_template_expression_type(
    db: &dyn TypeDatabase,
    texts: &[String],
    parts: &[TypeId],
) -> TypeId {
    tsz_solver::operations::expression_ops::compute_template_expression_type(db, texts, parts)
}

pub(crate) fn is_fresh_literal_indexed_object(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(shape_id) = tsz_solver::visitor::object_with_index_shape_id(db, type_id) else {
        return false;
    };
    db.object_shape(shape_id).is_fresh_literal()
}

pub(crate) fn union_context_prefers_tuple_array_literal(
    db: &dyn TypeDatabase,
    contextual: TypeId,
) -> bool {
    let Some(members) = crate::query_boundaries::common::union_members(db, contextual) else {
        return false;
    };

    let mut saw_tuple = false;
    for member in members {
        let Some(applicable) = crate::query_boundaries::common::array_applicable_type(db, member)
        else {
            return false;
        };

        if !crate::query_boundaries::common::is_tuple_type(db, applicable) {
            return false;
        }
        saw_tuple = true;
    }

    saw_tuple
}

pub(crate) fn widen_mutable_object_literal_property_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    let Some(shape) = crate::query_boundaries::common::object_shape_for_type(db, type_id) else {
        return type_id;
    };

    let mut widened_shape = shape.as_ref().clone();
    let mut changed = false;
    for prop in &mut widened_shape.properties {
        let widened_read = crate::query_boundaries::common::widen_literal_type(db, prop.type_id);
        let widened_write =
            crate::query_boundaries::common::widen_literal_type(db, prop.write_type);
        if widened_read != prop.type_id || widened_write != prop.write_type {
            changed = true;
        }
        prop.type_id = widened_read;
        prop.write_type = widened_write;
    }

    if changed {
        db.object_with_index(widened_shape)
    } else {
        type_id
    }
}

/// Whether a contextual type is literal-permissive for object-literal property
/// widening.
///
/// `unknown`, `any`, and `never` do not constrain literal property types in
/// tsc's contextual literal check, so they should not suppress the normal
/// widening of property literals in non-fresh object contexts.
pub(crate) const fn is_literal_permissive_object_context(type_id: TypeId) -> bool {
    matches!(type_id, TypeId::UNKNOWN | TypeId::ANY | TypeId::NEVER)
}

pub(crate) fn generic_application_literal_expected_for_mismatch(
    db: &dyn TypeDatabase,
    allow_generic_literal_display: bool,
    expected: TypeId,
    argument_types: &[TypeId],
    initializer_literal_types: &[TypeId],
) -> Option<TypeId> {
    if !allow_generic_literal_display {
        return None;
    }

    let display_expected = db.get_display_alias(expected).unwrap_or(expected);
    let (base, type_args) =
        crate::query_boundaries::common::application_info(db, display_expected)?;
    if type_args.len() != 1 {
        return None;
    }

    let expected_arg = type_args[0];
    let expected_arg_base = crate::query_boundaries::common::widen_literal_type(db, expected_arg);
    if !crate::query_boundaries::common::is_primitive_type(db, expected_arg_base) {
        return None;
    }

    let mut candidates = Vec::new();
    for candidate in argument_types
        .iter()
        .copied()
        .chain(initializer_literal_types.iter().copied())
    {
        if crate::query_boundaries::common::literal_value(db, candidate).is_some()
            && crate::query_boundaries::common::widen_literal_type(db, candidate)
                == expected_arg_base
            && !candidates.contains(&candidate)
        {
            candidates.push(candidate);
        }
    }

    if candidates.len() < 2 {
        return None;
    }

    let literal_arg = db.union(candidates);
    Some(db.application(base, vec![literal_arg]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::{PropertyInfo, TupleElement};

    fn fresh_object(db: &TypeInterner, name: &str, ty: TypeId) -> TypeId {
        db.object_fresh(vec![PropertyInfo::new(db.intern_string(name), ty)])
    }

    fn union_members(db: &TypeInterner, ty: TypeId) -> Vec<TypeId> {
        tsz_solver::type_queries::get_union_members(db, ty).unwrap_or_else(|| vec![ty])
    }

    fn tuple(db: &TypeInterner, type_id: TypeId) -> TypeId {
        db.tuple(vec![TupleElement {
            type_id,
            name: None,
            optional: false,
            rest: false,
        }])
    }

    #[test]
    fn write_target_logical_or_normalizes_object_union_members() {
        let db = TypeInterner::new();
        let left_object = fresh_object(&db, "left", TypeId::STRING);
        let right_object = fresh_object(&db, "right", TypeId::NUMBER);
        let nullable_left = db.union(vec![left_object, TypeId::NULL]);

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::LogicalOr,
            nullable_left,
            right_object,
        )
        .expect("nullable object || object should normalize write-target union");
        let WriteTargetLogicalResult::Type(result) = result else {
            panic!("expected normalized write-target type");
        };

        let members = union_members(&db, result);
        assert_eq!(members.len(), 2);
        for member in members {
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "left"
            ));
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "right"
            ));
        }
    }

    #[test]
    fn union_context_prefers_tuple_when_all_array_shapes_are_tuples() {
        let db = TypeInterner::new();
        let first = tuple(&db, TypeId::STRING);
        let second = tuple(&db, TypeId::NUMBER);
        let contextual = db.union(vec![first, second]);

        assert!(union_context_prefers_tuple_array_literal(&db, contextual));
    }

    #[test]
    fn union_context_does_not_prefer_tuple_for_array_member() {
        let db = TypeInterner::new();
        let contextual = db.union(vec![tuple(&db, TypeId::STRING), db.array(TypeId::NUMBER)]);

        assert!(!union_context_prefers_tuple_array_literal(&db, contextual));
    }

    #[test]
    fn union_context_does_not_prefer_tuple_for_non_applicable_member() {
        let db = TypeInterner::new();
        let contextual = db.union(vec![tuple(&db, TypeId::STRING), TypeId::NUMBER]);

        assert!(!union_context_prefers_tuple_array_literal(&db, contextual));
    }

    #[test]
    fn non_union_context_does_not_prefer_tuple_array_literal() {
        let db = TypeInterner::new();

        assert!(!union_context_prefers_tuple_array_literal(
            &db,
            tuple(&db, TypeId::STRING)
        ));
    }

    #[test]
    fn literal_permissive_object_context_accepts_top_like_contexts() {
        assert!(is_literal_permissive_object_context(TypeId::UNKNOWN));
        assert!(is_literal_permissive_object_context(TypeId::ANY));
        assert!(is_literal_permissive_object_context(TypeId::NEVER));
    }

    #[test]
    fn literal_permissive_object_context_rejects_constraining_contexts() {
        assert!(!is_literal_permissive_object_context(TypeId::STRING));
        assert!(!is_literal_permissive_object_context(TypeId::NUMBER));
        assert!(!is_literal_permissive_object_context(TypeId::BOOLEAN));
    }

    #[test]
    fn generic_application_literal_expected_rebuilds_argument_union() {
        let db = TypeInterner::new();
        let first = db.literal_string("first");
        let second = db.literal_string("second");
        let expected = db.application(TypeId::STRING, vec![TypeId::STRING]);

        let result = generic_application_literal_expected_for_mismatch(
            &db,
            true,
            expected,
            &[first],
            &[second],
        )
        .expect("two string literal candidates should rebuild display expectation");

        let (base, args) = tsz_solver::type_queries::get_application_info(&db, result)
            .expect("result should remain an application");
        assert_eq!(base, TypeId::STRING);
        assert_eq!(args.len(), 1);
        let members = union_members(&db, args[0]);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&first));
        assert!(members.contains(&second));
    }

    #[test]
    fn generic_application_literal_expected_uses_display_alias_application() {
        let db = TypeInterner::new();
        let first = db.literal_number(1.0);
        let second = db.literal_number(2.0);
        let expected = fresh_object(&db, "value", TypeId::NUMBER);
        let alias_expected = db.application(TypeId::NUMBER, vec![TypeId::NUMBER]);
        db.store_display_alias(expected, alias_expected);

        let result = generic_application_literal_expected_for_mismatch(
            &db,
            true,
            expected,
            &[first],
            &[second],
        )
        .expect("display alias application should drive rebuilt expectation");

        let (base, args) = tsz_solver::type_queries::get_application_info(&db, result)
            .expect("result should remain an application");
        assert_eq!(base, TypeId::NUMBER);
        let members = union_members(&db, args[0]);
        assert!(members.contains(&first));
        assert!(members.contains(&second));
    }

    #[test]
    fn generic_application_literal_expected_rejects_single_unique_candidate() {
        let db = TypeInterner::new();
        let first = db.literal_string("first");
        let expected = db.application(TypeId::STRING, vec![TypeId::STRING]);

        let result = generic_application_literal_expected_for_mismatch(
            &db,
            true,
            expected,
            &[first],
            &[first],
        );

        assert_eq!(result, None);
    }

    #[test]
    fn write_target_nullish_coalescing_normalizes_object_union_members() {
        let db = TypeInterner::new();
        let left_object = fresh_object(&db, "value", TypeId::STRING);
        let right_object = fresh_object(&db, "fallback", TypeId::BOOLEAN);
        let nullish_left = db.union(vec![left_object, TypeId::NULL, TypeId::UNDEFINED]);

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::NullishCoalescing,
            nullish_left,
            right_object,
        )
        .expect("nullish object ?? object should normalize write-target union");
        let WriteTargetLogicalResult::Type(result) = result else {
            panic!("expected normalized write-target type");
        };

        let members = union_members(&db, result);
        assert_eq!(members.len(), 2);
        for member in members {
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "value"
            ));
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "fallback"
            ));
        }
    }

    #[test]
    fn write_target_logical_result_falls_back_for_primitive_members() {
        let db = TypeInterner::new();
        let nullable_left = db.union(vec![TypeId::STRING, TypeId::NULL]);

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::LogicalOr,
            nullable_left,
            TypeId::NUMBER,
        );

        assert_eq!(result, None);
    }

    #[test]
    fn write_target_logical_result_requests_logical_fallback_when_split_is_impossible() {
        let db = TypeInterner::new();

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::LogicalOr,
            TypeId::NULL,
            TypeId::NUMBER,
        );

        assert_eq!(
            result,
            Some(WriteTargetLogicalResult::FallbackToLogicalExpression)
        );
    }
}
