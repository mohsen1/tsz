#[test]
fn test_ts2322_mapped_type_key_in_conditional_unconstrained_t() {
    // `string extends T ? { [P in T]: void } : T` — T is NOT narrowed in the
    // true branch (check type is `string`, not a type parameter), so T is still
    // unconstrained and `[P in T]` is invalid. tsc emits TS2322 here.
    let source = r"
        type B<T> = string extends T ? { [P in T]: void; } : T;
    ";
    assert!(
        has_error_with_code(source, 2322),
        "Expected TS2322 for unconstrained T in mapped type key inside conditional (string extends T)"
    );
}

#[test]
fn test_ts2322_no_false_positive_mapped_type_key_narrowed_by_conditional() {
    // `T extends string ? { [P in T]: void } : T` — T IS narrowed to `T & string`
    // in the true branch, so `[P in T]` is valid (T is string-like). No TS2322.
    let source = r"
        type A<T> = T extends string ? { [P in T]: void; } : T;
    ";
    let errors = get_all_diagnostics(source);
    assert!(
        !errors.iter().any(|(code, _)| *code == 2322),
        "Expected no TS2322 for narrowed T in mapped type key (T extends string). Got: {errors:?}"
    );
}

#[test]
fn test_ts2322_conditional_extends_distinguishes_optional_and_optional_undefined() {
    let source = r#"
        export let a: <T>() => T extends {a?: string} ? 0 : 1 = null!;
        export let b: <T>() => T extends {a?: string | undefined} ? 0 : 1 = a;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for conditional extends optional-property identity. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type '<T>() => T extends { a?: string; } ? 0 : 1' is not assignable to type '<T>() => T extends { a?: string | undefined; } ? 0 : 1'"),
        "Expected TS2322 to preserve the differing optional-property conditional signatures. Actual diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_constructor_default_value_diagnostics_do_not_timeout() {
    let source = r#"
class C {
    constructor(x);
    constructor(public x: string = 1) {
        var y = x;
    }
}

class D<T, U> {
    constructor(x: T, y: U);
    constructor(x: T = 1, public y: U = x) {
        var z = x;
    }
}

class E<T extends Date> {
    constructor(x);
    constructor(x: T = new Date()) {
        var y = x;
    }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        4,
        "Expected four TS2322 diagnostics for constructor parameter defaults, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'number' is not assignable to type 'string'")),
        "Expected string default initializer TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'number' is not assignable to type 'T'")),
        "Expected generic T default initializer TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'T' is not assignable to type 'U'")),
        "Expected generic parameter-property TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, msg)| {
            msg.ends_with("is not assignable to type 'T'.")
                && !msg.contains("Type 'number' is not assignable to type 'T'.")
        }),
        "Expected constrained default initializer TS2322 for T, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_new_date_assignment_uses_nominal_date_display() {
    let source = r#"
function foo4<T extends U, U extends V, V extends Date>(t: T, u: U, v: V) {
    t = new Date();
    u = new Date();
    v = new Date();
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        3,
        "Expected three TS2322 diagnostics for Date-constrained generic assignments, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'T'.")),
        "Expected nominal Date display for T assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'U'.")),
        "Expected nominal Date display for U assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'V'.")),
        "Expected nominal Date display for V assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().all(|(_, msg)| !msg.contains("getVarDate")),
        "Did not expect structural Date expansion in TS2322 diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn indexed_access_on_intersection_preserves_deferred_constraints() {
    // Repro from TypeScript#14723 / conformance test compiler/indexedAccessRelation.ts.
    //
    // Fixed: when evaluating (S & State<T>)["a"] in the mapped type
    // template for Pick<S & State<T>, K>, the solver now preserves deferred
    // IndexAccess types for unconstrained type parameters.
    // This ensures S["a"] is included in the result (S["a"] & (T | undefined)),
    // making T not assignable and TS2322 correctly emitted.
    //
    // tsc keeps (S & State<T>)["a"] as a deferred indexed access type,
    // which correctly rejects T as not assignable to the full expression.
    //
    // Fix requires changes to either:
    // 1. Mapped type evaluation to preserve deferred indexed access for
    //    non-homomorphic mapped types (but Application eval caching
    //    prevents the fix from taking effect), OR
    // 2. The indexed access intersection distribution to include deferred
    //    results (but this causes false positives in homomorphic mapped
    //    types like Readonly<TType & { name: string }>).
    let source = r#"
class Component<S> {
    setState<K extends keyof S>(state: Pick<S, K>) {}
}

export interface State<T> {
    a?: T;
}

class Foo {}

class Comp<T extends Foo, S> extends Component<S & State<T>>
{
    foo(a: T) {
        this.setState({ a: a });
    }
}
"#;
    let diagnostics = get_all_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for indexed access on intersection with unconstrained type parameter. Actual diagnostics: {diagnostics:?}"
    );
}

/// Regression test: arrays should NOT be assignable to interfaces that extend
/// ReadonlyArray/Array but have additional required properties.
///
/// In TypeScript, `TemplateStringsArray` extends `ReadonlyArray<string>` with
/// `readonly raw: readonly string[]`. An empty array `[]` (type `never[]`) lacks
/// the `raw` property, so `var x: TemplateStringsArray = []` should produce TS2322.
///
/// This was previously incorrectly accepted because the array-to-interface subtype
/// shortcut (`check_array_interface_subtype`) checked only `Array<T> <: target`
/// without verifying the target's extra declared properties.
#[test]
fn test_ts2322_array_not_assignable_to_interface_extending_array_with_extra_props() {
    let source = r#"
        interface ArrayWithExtra extends ReadonlyArray<string> {
            readonly raw: readonly string[];
        }
        var x: string[] = [];
        var y: ArrayWithExtra = x;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let assignability_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE  // TS2322
                || d.code == 2741  // TS2741: Property 'X' is missing
                || d.code == 2739 // TS2739: Type 'X' is missing properties
        })
        .collect();
    assert!(
        !assignability_errors.is_empty(),
        "Expected TS2322/TS2741/TS2739 when assigning string[] to interface extending ReadonlyArray with extra properties. All diagnostics: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn nested_weak_type_in_intersection_target_emits_ts2322() {
    // When assigning to an intersection target where nested properties are weak types,
    // the weak type check must still apply to the inner property comparison.
    // `in_intersection_member_check` should only suppress weak type checks at the
    // direct intersection member level, not for nested property types.
    // See: nestedExcessPropertyChecking.ts
    let source = r#"
        type A1 = { x: { a?: string } };
        type B1 = { x: { b?: string } };
        type C1 = { x: { c: string } };
        const ab1: A1 & B1 = {} as C1;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2322 || has_ts2559,
        "Expected TS2322 or TS2559 for nested weak type mismatch in intersection target. Got: {diagnostics:?}"
    );
}

#[test]
fn flat_weak_type_in_intersection_target_emits_ts2559() {
    // For flat (non-nested) weak types in an intersection, TS2559 should be emitted.
    let source = r#"
        type A2 = { a?: string };
        type B2 = { b?: string };
        type C2 = { c: string };
        const ab2: A2 & B2 = {} as C2;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for flat weak type mismatch in intersection target. Got: {diagnostics:?}"
    );
}

#[test]
fn intersection_member_weak_type_suppression_still_works() {
    // When the source has properties that overlap with one intersection member
    // but not with a weak-type member, the assignment should still pass.
    // The weak type suppression during intersection member checking should work
    // at the DIRECT level but not for nested property types.
    let source = r#"
        interface ITreeItem {
            Parent?: ITreeItem;
        }
        interface IDecl {
            Id?: number;
        }
        const x: ITreeItem & IDecl = {} as ITreeItem;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        !has_ts2322 && !has_ts2559,
        "ITreeItem should be assignable to ITreeItem & IDecl without error. Got: {diagnostics:?}"
    );
}

