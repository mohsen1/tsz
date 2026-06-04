#[test]
fn test_narrow_object_intrinsic_by_typeof_number_yields_never() {
    let interner = TypeInterner::new();

    // `typeof a === "number"` where `a: object` → never
    // object is not a number type, so narrowing should produce never
    let narrowed = narrow_by_typeof(&interner, TypeId::OBJECT, "number");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_object_intrinsic_by_typeof_object_yields_object() {
    let interner = TypeInterner::new();

    // `typeof a === "object"` where `a: object` → object
    let narrowed = narrow_by_typeof(&interner, TypeId::OBJECT, "object");
    assert_eq!(narrowed, TypeId::OBJECT);
}

#[test]
fn test_narrow_object_or_null_by_typeof_negation_object_yields_never() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // `typeof b !== "object"` where `b: object | null` → never
    // Both `object` (typeof === "object") and `null` (typeof === "object") are excluded
    let union = interner.union(vec![TypeId::OBJECT, TypeId::NULL]);
    let narrowed = ctx.narrow_by_typeof_negation(union, "object");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_object_or_string_by_typeof_negation_object_yields_string() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // `typeof x !== "object"` where `x: object | string` → string
    let union = interner.union(vec![TypeId::OBJECT, TypeId::STRING]);
    let narrowed = ctx.narrow_by_typeof_negation(union, "object");
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_narrow_object_by_typeof_negation_number_keeps_object() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // `typeof x !== "number"` where `x: object` → object
    // object is not a number, so it survives the exclusion
    let narrowed = ctx.narrow_by_typeof_negation(TypeId::OBJECT, "number");
    assert_eq!(narrowed, TypeId::OBJECT);
}

#[test]
fn test_remove_undefined_from_union() {
    let interner = TypeInterner::new();
    // string | undefined → string
    let union = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
    let result = remove_undefined(&interner, union);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_remove_undefined_from_triple_union() {
    let interner = TypeInterner::new();
    // string | number | undefined → string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    let result = remove_undefined(&interner, union);
    let expected = interner.union2(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(result, expected);
}

#[test]
fn test_remove_undefined_preserves_null() {
    let interner = TypeInterner::new();
    // string | null | undefined → string | null
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let result = remove_undefined(&interner, union);
    let expected = interner.union2(TypeId::STRING, TypeId::NULL);
    assert_eq!(result, expected);
}

#[test]
fn test_remove_undefined_no_undefined_noop() {
    let interner = TypeInterner::new();
    // string | number → string | number (unchanged)
    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let result = remove_undefined(&interner, union);
    assert_eq!(result, union);
}

#[test]
fn test_remove_undefined_bare_undefined() {
    let interner = TypeInterner::new();
    // undefined → never
    let result = remove_undefined(&interner, TypeId::UNDEFINED);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_remove_undefined_non_union_noop() {
    let interner = TypeInterner::new();
    // string → string (unchanged)
    let result = remove_undefined(&interner, TypeId::STRING);
    assert_eq!(result, TypeId::STRING);
}

/// Regression: when the extracted instance type is `any`, instanceof narrowing
/// must NOT filter union members. tsc keeps the source type unchanged because
/// every type is assignable to `any` so the check provides no information.
///
/// Mirrors the `interface FConstructor { new (): any }` case in
/// `typeGuardsWithInstanceOfByConstructorSignature.ts`. Before this fix,
/// `narrow_by_instance_type` dropped primitive members (e.g., `string`),
/// wrongly narrowing `F | string` to `F` and silencing the expected TS2339
/// diagnostics on `obj11.foo` / `obj11.bar`.
#[test]
fn test_narrow_by_instance_type_any_target_returns_source_unchanged() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Build an interface-like object type to stand in for `F`.
    let foo_name = interner.intern_string("foo");
    let f_like = interner.object(vec![PropertyInfo::new(foo_name, TypeId::STRING)]);

    let union = interner.union2(f_like, TypeId::STRING);

    // Instance type extracted from `new (): any` is `any`.
    let narrowed = ctx.narrow_by_instance_type(union, TypeId::ANY);

    // Both members must be preserved — string MUST NOT be filtered out.
    assert_eq!(
        narrowed, union,
        "narrow_by_instance_type should preserve the union when the instance type is `any` \
         (got {narrowed:?}, expected {union:?})"
    );
}

/// When two non-class union members are mutually-incompatible interfaces and
/// the instance-type filter rules out both directions of structural
/// assignability, the unrelated member must be **dropped** — not preserved as
/// `member & instance_type`. The earlier intersection fallback was leaking
/// forms like `C2 & C1` into TS2322 displays for the
/// `typeGuardOfFormInstanceOfOnInterface` repro (interfaces with conflicting
/// `prototype` and named property shapes), where tsc drops the unrelated
/// member and prints `'false | D1'` instead of `'false | D1 | C2 & C1'`.
///
/// The two interfaces here mirror the test's `C1` / `C2` shapes — disjoint
/// `prototype` literal types and a clashing named property — so neither
/// direction of `is_assignable_to` succeeds. The narrowed result must be the
/// related member alone (the one whose `prototype` matches the instance
/// type), with no intersection of the unrelated member.
#[test]
fn test_narrow_by_instance_type_drops_unrelated_interface_member() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let prototype_name = interner.intern_string("prototype");
    let p1_name = interner.intern_string("p1");
    let p2_name = interner.intern_string("p2");

    // `C1 = { prototype: number; p1: string }` — stand-in for the
    // self-referential `prototype: C1` shape that comes through the binder
    // for the conformance repro. The structural details (number vs symbol)
    // don't matter; what matters is that the two interfaces have *disjoint*
    // prototype types AND distinct extra properties so neither direction of
    // assignability holds.
    let c1 = interner.object(vec![
        PropertyInfo::new(prototype_name, TypeId::NUMBER),
        PropertyInfo::new(p1_name, TypeId::STRING),
    ]);
    let c2 = interner.object(vec![
        PropertyInfo::new(prototype_name, TypeId::SYMBOL),
        PropertyInfo::new(p2_name, TypeId::NUMBER),
    ]);

    let union = interner.union2(c1, c2);

    // Use C1 itself as the instance type. C2 is structurally unrelated to C1
    // (incompatible prototype and a different extra property), so C2 must be
    // dropped from the narrowed result rather than retained as `C2 & C1`.
    let narrowed = ctx.narrow_by_instance_type(union, c1);

    assert_eq!(
        narrowed, c1,
        "narrow_by_instance_type should drop the unrelated interface member \
         (got {narrowed:?}, expected {c1:?})"
    );
}

/// Same regression via the public `narrow_type` API with `TypeGuard::Instanceof`.
/// Confirms the fix is reachable through the checker's actual entry point.
#[test]
fn test_narrow_type_instanceof_any_target_returns_source_unchanged() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let foo_name = interner.intern_string("foo");
    let f_like = interner.object(vec![PropertyInfo::new(foo_name, TypeId::STRING)]);
    let union = interner.union2(f_like, TypeId::STRING);

    let guard = TypeGuard::Instanceof(TypeId::ANY, false);
    let narrowed = ctx.narrow_type(union, &guard, GuardSense::Positive);

    assert_eq!(
        narrowed, union,
        "TypeGuard::Instanceof(any) on the true branch should not filter union members"
    );
}

/// Build a constructor whose `[Symbol.hasInstance]` method has the given
/// predicate target, with an optional construct signature return type.
fn make_constructor_with_has_instance(
    interner: &TypeInterner,
    construct_return: Option<TypeId>,
    predicate_target: Option<TypeId>,
    predicate_asserts: bool,
    param_name: &str,
) -> TypeId {
    use crate::types::{
        CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
        TypePredicateTarget,
    };

    let name_atom = interner.intern_string(param_name);
    let has_instance_atom = interner.intern_string("[Symbol.hasInstance]");

    let has_instance_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::required(name_atom, TypeId::UNKNOWN)],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: predicate_target.map(|target| TypePredicate {
            asserts: predicate_asserts,
            target: TypePredicateTarget::Identifier(name_atom),
            type_id: Some(target),
            parameter_index: Some(0),
        }),
        is_constructor: false,
        is_method: true,
    });

    let construct_signatures = construct_return
        .map(|ret| vec![CallSignature::new(vec![], ret)])
        .unwrap_or_default();

    interner.callable(CallableShape {
        construct_signatures,
        properties: vec![PropertyInfo::method(has_instance_atom, has_instance_fn)],
        ..CallableShape::default()
    })
}
