//! Regression tests for optional parameter relation compatibility.

use super::*;
use crate::intern::TypeInterner;
use crate::relations::compat::CompatChecker;

fn function_with_param(
    interner: &TypeInterner,
    name: &str,
    type_id: TypeId,
    optional: bool,
) -> TypeId {
    interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string(name)),
            type_id,
            optional,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

fn object_with_function_property(
    interner: &TypeInterner,
    prop_name: &str,
    param_type: TypeId,
    param_optional: bool,
) -> TypeId {
    let function = function_with_param(interner, "value", param_type, param_optional);
    interner.object(vec![PropertyInfo::new(
        interner.intern_string(prop_name),
        function,
    )])
}

fn strict_checker(interner: &TypeInterner) -> CompatChecker<'_> {
    let mut checker = CompatChecker::new(interner);
    checker.set_strict_function_types(true);
    checker.set_strict_null_checks(true);
    checker
}

#[test]
fn optional_source_param_matches_target_param_unioned_with_undefined() {
    let interner = TypeInterner::new();
    let mut checker = strict_checker(&interner);
    let number_or_undefined = interner.union2(TypeId::NUMBER, TypeId::UNDEFINED);

    let source = function_with_param(&interner, "x", TypeId::NUMBER, true);
    let target = function_with_param(&interner, "x", number_or_undefined, false);

    assert!(
        checker.is_assignable(source, target),
        "tsc accepts assigning `(x?: number) => void` to `(x: number | undefined) => void`",
    );
}

#[test]
fn renamed_optional_source_param_matches_target_param_unioned_with_undefined() {
    let interner = TypeInterner::new();
    let mut checker = strict_checker(&interner);
    let string_or_undefined = interner.union2(TypeId::STRING, TypeId::UNDEFINED);

    let source = function_with_param(&interner, "source", TypeId::STRING, true);
    let target = function_with_param(&interner, "target", string_or_undefined, false);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn target_optional_param_still_accepts_explicit_undefined_union_source() {
    let interner = TypeInterner::new();
    let mut checker = strict_checker(&interner);
    let number_or_undefined = interner.union2(TypeId::NUMBER, TypeId::UNDEFINED);

    let source = function_with_param(&interner, "x", number_or_undefined, false);
    let target = function_with_param(&interner, "x", TypeId::NUMBER, true);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn required_source_param_does_not_match_target_param_unioned_with_undefined() {
    let interner = TypeInterner::new();
    let mut checker = strict_checker(&interner);
    let number_or_undefined = interner.union2(TypeId::NUMBER, TypeId::UNDEFINED);

    let source = function_with_param(&interner, "x", TypeId::NUMBER, false);
    let target = function_with_param(&interner, "x", number_or_undefined, false);

    assert!(
        !checker.is_assignable(source, target),
        "strict contravariance still rejects `(x: number) => void` for `(x: number | undefined) => void`",
    );
}

#[test]
fn object_member_optional_source_param_matches_required_undefined_union_target() {
    let interner = TypeInterner::new();
    let mut checker = strict_checker(&interner);
    let number_or_undefined = interner.union2(TypeId::NUMBER, TypeId::UNDEFINED);

    let source = object_with_function_property(&interner, "f", TypeId::NUMBER, true);
    let target = object_with_function_property(&interner, "f", number_or_undefined, false);

    assert!(checker.is_assignable(source, target));
}
