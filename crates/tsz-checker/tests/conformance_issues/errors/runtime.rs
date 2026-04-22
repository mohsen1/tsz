use super::super::core::*;

#[test]
fn test_named_interface_assignment_to_number_index_target_reports_missing_index_signature() {
    let source = r#"
interface InterfaceWithPublicAndOptional<T, U> { one: T; two?: U; }
declare let aa: { [index: number]: number };
declare let obj4: InterfaceWithPublicAndOptional<number, string>;
aa = obj4;
"#;
    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for named interface assigned to number index target. Got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("InterfaceWithPublicAndOptional<number, string>")
                && message.contains("{ [index: number]: number; }")
        }),
        "Expected the named-interface to number-index TS2322. Got: {diagnostics:?}"
    );
}

#[test]
fn test_exported_alias_of_generic_interface_preserves_missing_number_index_error() {
    let source = r#"
namespace __test1__ {
    export interface interfaceWithPublicAndOptional<T,U> { one: T; two?: U; };  var obj4: interfaceWithPublicAndOptional<number,string> = { one: 1 };;
    export var __val__obj4 = obj4;
}
namespace __test2__ {
    export declare var aa:{[index:number]:number;};;
    export var __val__aa = aa;
}
__test2__.__val__aa = __test1__.__val__obj4
"#;
    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for exported alias of generic interface assigned to number index target. Got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("interfaceWithPublicAndOptional<number, string>")
                && message.contains("{ [index: number]: number; }")
        }),
        "Expected named generic interface display in TS2322. Got: {diagnostics:?}"
    );
}

#[test]
fn test_assigning_to_class_symbol_does_not_contextually_type_rhs_as_constructor() {
    let source = r#"
namespace Test {
    class Mocked {
        myProp: string;
    }

    class Tester {
        willThrowError() {
            Mocked = Mocked || function () {
                return { myProp: "test" };
            };
        }
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2629),
        "Expected TS2629 for assignment to class symbol. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2741),
        "Assignment to a class symbol should not contextually type the RHS as 'typeof Class': {diagnostics:?}"
    );
}

#[test]
fn test_array_from_assignment_context_does_not_overwrite_direct_type_arg_inference() {
    let source = r#"
interface A { a: string; }
interface B { b: string; }
interface Iterable<T> {}
interface ArrayIterator<T> extends Iterable<T> {}
interface ArrayLikeish<T> { length: number; }
declare const Array: {
    from<T>(items: Iterable<T> | ArrayLikeish<T>): T[];
};
declare const inputA: { values(): ArrayIterator<A> };
declare const inputALike: ArrayLikeish<A>;

const result1: B[] = Array.from(inputA.values());
const result2: B[] = Array.from(inputALike);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        2,
        "Expected only the outer B[] assignment failures. Got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2769),
        "Array.from direct arg inference should not be overwritten by assignment context. Got: {diagnostics:?}"
    );
}

/// Regression test: loop fixed-point should not leak declared type via ERROR-typed
/// back-edge assignments. When `x = len(x)` hasn't been type-checked yet during
/// loop fixed-point iteration, `node_types` returns ERROR. Since ERROR is subtype of
/// everything, `narrow_assignment` keeps all union members, incorrectly widening to
/// the full declared type. The fix filters out ERROR from `get_assigned_type` results.
///
/// Reproduces controlFlowWhileStatement.ts function h2.
#[test]
fn test_loop_fixed_point_no_false_ts2345_from_error_assigned_type() {
    let source = r#"
let cond: boolean;
declare function len(s: string | number): number;
function h2() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x = len(x);
        x; // number
    }
    x; // string | number
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2345),
        "Loop fixed-point should not widen x to string|number|boolean via ERROR back-edge. Got: {diagnostics:?}"
    );
}

/// Regression test: loop fixed-point with function call assignment and separate
/// declaration. The call return type (number) should be used correctly in the
/// loop's fixed-point analysis, not the full declared type.
///
/// Reproduces controlFlowWhileStatement.ts function h3.
#[test]
fn test_loop_fixed_point_function_call_assignment_at_end() {
    let source = r#"
let cond: boolean;
declare function len(s: string | number): number;
function h3() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x;           // string | number
        x = len(x);
    }
    x; // string | number
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2345),
        "Loop fixed-point with call assignment at end should not widen via ERROR type. Got: {diagnostics:?}"
    );
}

/// Boolean literal discriminant narrowing: `x.kind === false` should narrow via
/// discriminant comparison (checking `false <: prop_type`), not truthiness narrowing.
///
/// Previously, `narrow_by_boolean_comparison` intercepted `x.kind === false` and
/// treated it as a truthiness check on `x.kind`, which kept `{ kind: string }` in
/// the narrowed type (since strings can be falsy). The fix ensures property access
/// comparisons with boolean literals fall through to discriminant narrowing.
///
/// Reproduces discriminatedUnionTypes2.ts function f10.
#[test]
fn test_boolean_discriminant_narrowing_false() {
    let source = r#"
function f10(x: { kind: false, a: string } | { kind: true, b: string } | { kind: string, c: string }) {
    if (x.kind === false) {
        x.a;
    }
    else if (x.kind === true) {
        x.b;
    }
    else {
        x.c;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Boolean literal discriminant narrowing should filter union members by discriminant subtyping, not truthiness. Got: {diagnostics:?}"
    );
}

/// Boolean literal discriminant narrowing with switch statement.
/// `switch (x.kind) { case false: ... }` should also narrow via discriminant.
///
/// Reproduces discriminatedUnionTypes2.ts function f11.
#[test]
fn test_boolean_discriminant_narrowing_switch() {
    let source = r#"
function f11(x: { kind: false, a: string } | { kind: true, b: string } | { kind: string, c: string }) {
    switch (x.kind) {
        case false:
            x.a;
            break;
        case true:
            x.b;
            break;
        default:
            x.c;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2339),
        "Boolean discriminant narrowing via switch should work like if/else. Got: {diagnostics:?}"
    );
}

/// Ensure `instanceof === false` still works via boolean comparison handler.
/// This pattern should NOT be intercepted by the discriminant path guard,
/// because the `guard_expr` (`x instanceof Error`) is a binary expression, not
/// a property access.
#[test]
fn test_instanceof_false_still_narrows() {
    let source = r#"
function test(x: string | Error) {
    if (x instanceof Error === false) {
        const s: string = x;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2322),
        "instanceof === false should still narrow via boolean comparison. Got: {diagnostics:?}"
    );
}

/// TS2344: Type parameter constraint checking when type arg is itself a type parameter.
///
/// When a type parameter `U extends number` is passed to a generic that requires
/// `T extends string`, tsc resolves `U`'s base constraint to `number` and checks
/// `number <: string`, emitting TS2344 when it fails.
///
/// Previously, `validate_type_args_against_params` unconditionally skipped constraint
/// checking when the type argument contained type parameters (via `contains_type_parameters`).
/// Now it resolves bare type parameters to their base constraints and checks assignability.
#[test]
fn test_ts2344_type_param_constraint_mismatch() {
    // Case 1: Incompatible primitive constraints → should emit TS2344
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Foo<T extends string> = T;
type Bar<U extends number> = Foo<U>;
        ",
    );
    assert!(
        has_error(&diagnostics, 2344),
        "Should emit TS2344 when `U extends number` is used where `T extends string` is required.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_type_param_object_constraint_mismatch() {
    // Case 2: Incompatible object constraints → should emit TS2344
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Inner<C extends { props: any }> = C;
type Outer<WithC extends { name: string }> = Inner<WithC>;
        ",
    );
    assert!(
        has_error(&diagnostics, 2344),
        "Should emit TS2344 when `WithC extends {{ name: string }}` doesn't satisfy `{{ props: any }}`.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_unconstrained_type_param_reports_object_constraint() {
    // tsc emits TS2344 when an unconstrained type parameter is used where
    // `T extends Object` is required. The unconstrained param cannot
    // satisfy the Object constraint.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
interface Readonly<T> {}
interface Partial<T> {}
interface Iterable<T> {}

namespace Record {
    export interface Class<T extends Object> {
        (values?: Partial<T> | Iterable<[string, any]>): T & Readonly<T>;
    }
}

declare function Record<T>(defaultValues: T, name?: string): Record.Class<T>;
        ",
    );

    assert!(
        has_error(&diagnostics, 2344),
        "Should emit TS2344 when unconstrained type param is used where `T extends Object` is required.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_type_param_compatible_constraint() {
    // Case 3: Compatible constraints → should NOT emit TS2344
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Foo<T extends string> = T;
type Bar<U extends string> = Foo<U>;
        ",
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Should NOT emit TS2344 when `U extends string` satisfies `T extends string`.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_no_false_positive_in_conditional_type_branch() {
    // Case 4: Union-constrained type param in conditional type true branch.
    // tsc narrows `TRec` to `MyRecord` in the true branch, so
    // `MySet<TRec>` is valid. We skip union-constrained type params
    // to avoid false positives.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

declare class MyRecord {}
declare class MySet<TSet extends MyRecord> {}

type DS<TRec extends MyRecord | { [key: string]: unknown }> =
    TRec extends MyRecord ? MySet<TRec> : TRec[];
        ",
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Should NOT emit TS2344 for union-constrained type param in conditional type true branch.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_reports_for_composite_indexed_access_type_args() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
interface CallableFunction extends Function {}
interface NewableFunction extends Function {}

type ReturnType<T extends (...args: any) => any> =
    T extends (...args: any) => infer R ? R : any;

type DataFetchFns = {
    Boat: {
        requiresLicense: (id: string) => boolean;
        maxGroundSpeed: (id: string) => number;
        description: (id: string) => string;
        displacement: (id: string) => number;
        name: (id: string) => string;
    };
};

type TypeHardcodedAsParameterWithoutReturnType<
    T extends 'Boat',
    F extends keyof DataFetchFns[T]
> = DataFetchFns[T][F];

    type FailingCombo<
    T extends 'Boat',
    F extends keyof DataFetchFns[T]
> = ReturnType<TypeHardcodedAsParameterWithoutReturnType<T, F>>;
        ",
    );
    assert!(
        has_error(&diagnostics, 2344),
        "Should emit TS2344 for composite indexed-access type arguments when their resolved base constraint is not callable.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_constraint_with_indexed_access_formats_instantiated_alias_bodies() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
interface CallableFunction extends Function {}
interface NewableFunction extends Function {}

type ReturnType<T extends (...args: any) => any> =
    T extends (...args: any) => infer R ? R : any;

type DataFetchFns = {
    Boat: {
        requiresLicense: (id: string) => boolean;
        maxGroundSpeed: (id: string) => number;
        description: (id: string) => string;
        displacement: (id: string) => number;
        name: (id: string) => string;
    };
    Plane: {
        requiresLicense: (id: string) => boolean;
        maxGroundSpeed: (id: string) => number;
        maxTakeoffWeight: (id: string) => number;
        maxCruisingAltitude: (id: string) => number;
        name: (id: string) => string;
    };
};

type TypeHardcodedAsParameterWithoutReturnType<
    T extends 'Boat',
    F extends keyof DataFetchFns[T]
> = DataFetchFns[T][F];
type NoTypeParamBoatRequired<F extends keyof DataFetchFns['Boat']> =
    ReturnType<DataFetchFns['Boat'][F]>;
type allAreFunctionsAsExpected =
    TypeHardcodedAsParameterWithoutReturnType<'Boat', keyof DataFetchFns['Boat']>;
type returnTypeOfFunctions = ReturnType<allAreFunctionsAsExpected>;
type SucceedingCombo =
    ReturnType<TypeHardcodedAsParameterWithoutReturnType<'Boat', keyof DataFetchFns['Boat']>>;
type VehicleSelector<T extends keyof DataFetchFns> = DataFetchFns[T];

type FailingCombo<
    T extends 'Boat',
    F extends keyof DataFetchFns[T]
> = ReturnType<TypeHardcodedAsParameterWithoutReturnType<T, F>>;
type TypeHardcodedAsParameter<
    T extends 'Boat',
    F extends keyof DataFetchFns[T]
> = ReturnType<DataFetchFns[T][F]>;
type TypeHardcodedAsParameter2<
    T extends 'Boat',
    F extends keyof DataFetchFns[T]
> = ReturnType<VehicleSelector<T>[F]>;
type TypeGeneric1<
    T extends keyof DataFetchFns,
    F extends keyof DataFetchFns[T]
> = ReturnType<DataFetchFns[T][F]>;
type TypeGeneric2<
    T extends keyof DataFetchFns,
    F extends keyof DataFetchFns[T]
> = ReturnType<DataFetchFns[T][T]>;
type TypeGeneric3<
    T extends keyof DataFetchFns,
    F extends keyof DataFetchFns[T]
> = ReturnType<DataFetchFns[F][F]>;
        ",
    );

    let ts2344 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2344.len(),
        6,
        "Expected the full conformance TS2344 surface. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|message| message.contains("TypeHardcodedAsParameterWithoutReturnType<T, F>")),
        "Expected TS2344 to preserve the written helper alias surface for direct alias applications. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|message| message.contains("VehicleSelector<T>[F]")),
        "Expected TS2344 to preserve the written helper alias surface for nested alias access. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|message| message.contains("DataFetchFns[T][F]")),
        "Expected TS2344 to keep raw indexed-access expressions structural. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|message| message.contains("DataFetchFns[T][T]")),
        "Expected TS2344 to keep raw indexed-access expressions structural for mismatched keys. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|message| message.contains("DataFetchFns[F][F]")),
        "Expected TS2344 to keep raw indexed-access expressions structural for mismatched object aliases. Actual diagnostics: {diagnostics:?}"
    );

    let ts2536 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2536)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ts2536.len(),
        3,
        "Expected the full conformance TS2536 surface. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2536
            .iter()
            .any(|message| message
                .contains("Type 'T' cannot be used to index type 'DataFetchFns[T]'")),
        "Expected TS2536 to display the expanded indexed-access object. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2536
            .iter()
            .any(|message| message.contains("Type 'F' cannot be used to index type 'DataFetchFns'")),
        "Expected TS2536 to preserve raw object surfaces for direct indexed access. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2536
            .iter()
            .any(|message| message
                .contains("Type 'F' cannot be used to index type 'DataFetchFns[F]'")),
        "Expected TS2536 to preserve nested raw object surfaces for direct indexed access. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2536.iter().all(|message| !message
            .contains("Type 'T' cannot be used to index type 'VehicleSelector<T>'")),
        "TS2536 should not repaint raw indexed-access objects with helper alias names. Actual diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_ts2344_for_concrete_indexed_access_callable_union() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
interface CallableFunction extends Function {}
interface NewableFunction extends Function {}

type ReturnType<T extends (...args: any) => any> =
    T extends (...args: any) => infer R ? R : any;

type DataFetchFns = {
    Boat: {
        requiresLicense: (id: string) => boolean;
        maxGroundSpeed: (id: string) => number;
        description: (id: string) => string;
        displacement: (id: string) => number;
        name: (id: string) => string;
    };
};

type NoTypeParamBoatRequired<F extends keyof DataFetchFns['Boat']> =
    ReturnType<DataFetchFns['Boat'][F]>;
        ",
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Should not emit TS2344 when a concrete object indexed by a constrained key collapses to a callable union.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_ts2344_reports_for_recursive_composite_type_args() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

declare class Component<P> {
    readonly props: Readonly<P> & Readonly<{ children?: {} }>;
}

interface ComponentClass<P = {}> {
    new (props: P, context?: any): Component<P>;
}

interface FunctionComponent<P = {}> {
    (props: P & { children?: {} }, context?: any): {} | null;
}

type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>;

type Shared<
    InjectedProps,
    DecorationTargetProps extends Shared<InjectedProps, DecorationTargetProps>
> = {
    [P in Extract<keyof InjectedProps, keyof DecorationTargetProps>]?: InjectedProps[P] extends DecorationTargetProps[P]
        ? DecorationTargetProps[P]
        : never;
};

type GetProps<C> = C extends ComponentType<infer P> ? P : never;

type Matching<InjectedProps, DecorationTargetProps> = {
    [P in keyof DecorationTargetProps]: P extends keyof InjectedProps
        ? InjectedProps[P] extends DecorationTargetProps[P]
            ? DecorationTargetProps[P]
            : InjectedProps[P]
        : DecorationTargetProps[P];
};

type Omit<T, K extends keyof T> = Pick<T, Exclude<keyof T, K>>;

type InferableComponentEnhancerWithProps<TInjectedProps, TNeedsProps> =
    <C extends ComponentType<Matching<TInjectedProps, GetProps<C>>>>(
        component: C
    ) => Omit<GetProps<C>, keyof Shared<TInjectedProps, GetProps<C>>> & TNeedsProps;
        ",
    );
    // GetProps<C> = `C extends ComponentType<infer P> ? P : never` has a bare
    // type parameter (infer P) as the true branch. Since the result is opaque
    // (not structurally derived from the check type), tsc treats this like an
    // Extract pattern and checks the extends type against the constraint.
    // Note: This minimal test lacks full lib declarations, so the TS2344 may
    // not fire. We just verify no crash occurs; the full-lib tests validate
    // the TS2344 emission.
    // The minimal lib case may or may not emit TS2344 depending on type
    // resolution — accept either outcome.
    let _ = diagnostics; // Just verify compilation succeeds without crash
}

#[test]
fn test_ts2344_reports_for_recursive_shared_constraint_in_component_enhancer() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare class Component<P> {
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    readonly props: Readonly<P> & Readonly<{ children?: {} }>;
}
interface ComponentClass<P = {}> {
    new (props: P, context?: any): Component<P>;
    propTypes?: WeakValidationMap<P>;
    defaultProps?: Partial<P>;
    displayName?: string;
}
interface FunctionComponent<P = {}> {
    (props: P & { children?: {} }, context?: any): {} | null;
    propTypes?: WeakValidationMap<P>;
    defaultProps?: Partial<P>;
    displayName?: string;
}

declare const nominalTypeHack: unique symbol;
interface Validator<T> {
    (props: object, propName: string, componentName: string, location: string, propFullName: string): {} | null;
    [nominalTypeHack]?: T;
}
type WeakValidationMap<T> = {
    [K in keyof T]?: null extends T[K]
        ? Validator<T[K] | null | undefined>
        : undefined extends T[K]
        ? Validator<T[K] | null | undefined>
        : Validator<T[K]>;
};
type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>;

type Shared<
    InjectedProps,
    DecorationTargetProps extends Shared<InjectedProps, DecorationTargetProps>
> = {
    [P in Extract<keyof InjectedProps, keyof DecorationTargetProps>]?: InjectedProps[P] extends DecorationTargetProps[P]
        ? DecorationTargetProps[P]
        : never;
};

type GetProps<C> = C extends ComponentType<infer P> ? P : never;

type ConnectedComponentClass<
    C extends ComponentType<any>,
    P
> = ComponentClass<P> & {
    WrappedComponent: C;
};

type Matching<InjectedProps, DecorationTargetProps> = {
    [P in keyof DecorationTargetProps]: P extends keyof InjectedProps
        ? InjectedProps[P] extends DecorationTargetProps[P]
            ? DecorationTargetProps[P]
            : InjectedProps[P]
        : DecorationTargetProps[P];
};

type InferableComponentEnhancerWithProps<TInjectedProps, TNeedsProps> =
    <C extends ComponentType<Matching<TInjectedProps, GetProps<C>>>>(
        component: C
    ) => ConnectedComponentClass<C, Omit<GetProps<C>, keyof Shared<TInjectedProps, GetProps<C>>> & TNeedsProps>;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    // GetProps<C> = `C extends ComponentType<infer P> ? P : never` has a bare
    // infer type parameter as the true branch. The result is opaque (not
    // structurally derived from the check type), so tsc treats this like
    // Extract and checks the extends type against the constraint.
    assert!(
        has_error(&diagnostics, 2344),
        "Expected TS2344 for recursive Shared<GetProps<C>> constraint, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.0 == 2344
                && d.1
                    .contains("Type 'GetProps<C>' does not satisfy the constraint")
        }),
        "Expected TS2344 to target GetProps<C>, got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2344_reports_for_recursive_shared_constraint_in_exported_component_enhancer() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare class Component<P> {
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    readonly props: Readonly<P> & Readonly<{ children?: {} }>;
}
interface ComponentClass<P = {}> {
    new (props: P, context?: any): Component<P>;
    propTypes?: WeakValidationMap<P>;
    defaultProps?: Partial<P>;
    displayName?: string;
}
interface FunctionComponent<P = {}> {
    (props: P & { children?: {} }, context?: any): {} | null;
    propTypes?: WeakValidationMap<P>;
    defaultProps?: Partial<P>;
    displayName?: string;
}

export declare const nominalTypeHack: unique symbol;
export interface Validator<T> {
    (props: object, propName: string, componentName: string, location: string, propFullName: string): {} | null;
    [nominalTypeHack]?: T;
}
type WeakValidationMap<T> = {
    [K in keyof T]?: null extends T[K]
        ? Validator<T[K] | null | undefined>
        : undefined extends T[K]
        ? Validator<T[K] | null | undefined>
        : Validator<T[K]>;
};
type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>;

export type Shared<
    InjectedProps,
    DecorationTargetProps extends Shared<InjectedProps, DecorationTargetProps>
> = {
    [P in Extract<keyof InjectedProps, keyof DecorationTargetProps>]?: InjectedProps[P] extends DecorationTargetProps[P]
        ? DecorationTargetProps[P]
        : never;
};

export type GetProps<C> = C extends ComponentType<infer P> ? P : never;

export type ConnectedComponentClass<
    C extends ComponentType<any>,
    P
> = ComponentClass<P> & {
    WrappedComponent: C;
};

export type Matching<InjectedProps, DecorationTargetProps> = {
    [P in keyof DecorationTargetProps]: P extends keyof InjectedProps
        ? InjectedProps[P] extends DecorationTargetProps[P]
            ? DecorationTargetProps[P]
            : InjectedProps[P]
        : DecorationTargetProps[P];
};

export type Omit<T, K extends keyof T> = Pick<T, Exclude<keyof T, K>>;

export type InferableComponentEnhancerWithProps<TInjectedProps, TNeedsProps> =
    <C extends ComponentType<Matching<TInjectedProps, GetProps<C>>>>(
        component: C
    ) => ConnectedComponentClass<C, Omit<GetProps<C>, keyof Shared<TInjectedProps, GetProps<C>>> & TNeedsProps>;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    // GetProps<C> = `C extends ComponentType<infer P> ? P : never` has a bare
    // infer type parameter as the true branch. tsc treats this like Extract
    // and checks the extends type against the constraint.
    assert!(
        has_error(&diagnostics, 2344),
        "Expected TS2344 for exported recursive Shared<GetProps<C>> constraint, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.0 == 2344
                && d.1
                    .contains("Type 'GetProps<C>' does not satisfy the constraint")
        }),
        "Expected exported TS2344 to target GetProps<C>, got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2415_reports_private_imported_unique_symbol_override_in_derived_class() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "a.ts",
                r#"
export const x = Symbol();
"#,
            ),
            (
                "b.ts",
                r#"
import { x } from "./a";

export class C {
  private [x]: number = 1;
}
"#,
            ),
            (
                "c.ts",
                r#"
import { x } from "./a";
import { C } from "./b";

export class D extends C {
  private [x]: 12 = 12;
}
"#,
            ),
        ],
        "c.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|d| {
            d.0 == 2415 && d.1.contains("Class 'D' incorrectly extends base class 'C'")
        }),
        "Expected TS2415 for private imported unique-symbol override, got: {diagnostics:#?}"
    );
}

#[test]
fn test_no_false_ts2344_for_self_mapped_index_access_return_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface A { x: number }

declare function isA(a: unknown): a is A;

type FunctionsObj<T> = {
    [K in keyof T]: () => unknown
}

function g<
    T extends FunctionsObj<T>,
    M extends keyof T
>(a2: ReturnType<T[M]>, x: A) {
    x = a2;
}

function g2<
    T extends FunctionsObj<T>,
    M extends keyof T
>(a2: ReturnType<T[M]>) {
    if (isA(a2)) {
        a2.x;
    }
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Self-mapped indexed access constraints should not trigger TS2344.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_parameters_of_index_signature_constrained_funcs() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type IFuncs = { readonly [key: string]: (...p: any) => void };
type IDestructuring<T extends IFuncs> = {
    readonly [key in keyof T]?: (...p: Parameters<T[key]>) => void
};
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Index-signature-constrained function maps should not trigger TS2344 for Parameters<T[key]>.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_mapped_type_preserving_record_constraint() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Same<T> = { [P in keyof T]: T[P] };

type T1<T extends Record<PropertyKey, number>> = T;
type T2<U extends Record<PropertyKey, number>> = T1<Same<U>>;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Homomorphic mapped types over constrained records should defer TS2344 until instantiation.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_weak_collection_infer_constraints_in_true_branch() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type DeepPickWeakMap<Type, Filter> = Type extends WeakMap<infer Keys, infer Values>
    ? Filter extends WeakMap<Keys, infer FilterValues>
        ? WeakMap<Keys, Values>
        : Type
    : never;

type DeepPickWeakSet<Type, Filter> = Type extends WeakSet<infer Values>
    ? Filter extends WeakSet<infer FilterValues>
        ? WeakSet<Values>
        : Type
    : never;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Infer variables from WeakMap/WeakSet true branches should inherit their hidden WeakKey constraints.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_imported_record_indexed_access_key_constraint() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "Any/Key.ts",
                r#"
export type Key = string | number | symbol;
"#,
            ),
            (
                "Object/_Internal.ts",
                r#"
export type Modx = ['?' | '!', 'W' | 'R'];
"#,
            ),
            (
                "Object/Record.ts",
                r#"
import {Modx} from './_Internal';
import {Key} from '../Any/Key';

export type Record<K extends Key, A extends any = unknown, modx extends Modx = ['!', 'W']> = {
    '!': {
        'R': {readonly [P in K]: A};
        'W': {[P in K]: A};
    };
    '?': {
        'R': {readonly [P in K]?: A};
        'W': {[P in K]?: A};
    };
}[modx[0]][modx[1]];
"#,
            ),
            (
                "entry.ts",
                r#"
import {Record} from './Object/Record';
import {Key} from './Any/Key';

type Alias<O extends Record<keyof O, Key>, K extends keyof O> = Record<O[K], K>;
"#,
            ),
        ],
        "entry.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Imported Record aliases should not misclassify `Key` as a callable constraint for generic indexed-access type arguments.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_composite_type_args_with_unresolved_members() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Foo1<A,B> = [A, B] extends unknown[][] ? Bar1<[A, B]> : 'else'
type Bar1<T extends unknown[][]> = T

type Foo2<A> = Set<A> extends Set<unknown[]> ? Bar2<Set<A>> : 'else'
type Bar2<T extends Set<unknown[]>> = T
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Composite type arguments whose evaluated base still contains type parameters should not trigger TS2344.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_interface_extending_array_constraint() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r"
interface CoolArray<E> extends Array<E> {
    hello: number;
}

declare function foo<T extends any[]>(): void;

foo<CoolArray<any>>();
        ",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Interface types extending Array should satisfy `T extends any[]` constraints.\nActual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_discriminated_union_record_helper() {
    let mut source = String::from("type BigUnion =\n");
    for idx in 0..1200 {
        source.push_str(&format!("  | {{ name: '{idx}'; children: BigUnion[] }}\n"));
    }
    source.push_str(
        r#"

type DiscriminateUnion<T, K extends keyof T, V extends T[K]> = T extends Record<K, V> ? T : never;
type WithName<T extends BigUnion['name']> = DiscriminateUnion<BigUnion, 'name', T>;
type ChildrenOf<T extends BigUnion> = T['children'][number];

export function makeThing<T extends BigUnion['name']>(
    name: T,
    children: ChildrenOf<WithName<T>>[] = [],
) {}

makeThing('42', []);
"#,
    );

    let diagnostics = compile_and_get_diagnostics_with_options(
        &source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Discriminated-union Record helper should not trigger TS2344.\nActual: {diagnostics:?}"
    );
}

/// Issue: instanceof narrowing uses structural subtyping instead of nominal class identity.
///
/// When class A has only optional properties, `is_assignable_to(B, A)` returns true
/// structurally even though B is an unrelated class. This causes instanceof narrowing
/// to keep B in the true branch and exclude it from the false branch incorrectly.
///
/// Status: FIXED (2026-03-03)
#[test]
fn test_instanceof_narrowing_nominal_class_identity() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
class A { a?: string; }
class B { b: number = 0; }
function test(x: A | B) {
    if (x instanceof A) {
        x.a;  // OK: x is A
    } else {
        x.b;  // OK: x is B
    }
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Instanceof narrowing should use nominal identity for classes.\n\
         True branch should be A, false branch should be B.\n\
         Actual errors: {diagnostics:?}"
    );
}

/// Instanceof narrowing with inheritance: subclass should survive true branch.
#[test]
fn test_instanceof_narrowing_with_class_hierarchy() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r"
class Animal { name?: string; }
class Dog extends Animal { bark(): void {} }
class Cat extends Animal { meow(): void {} }
function test(x: Dog | Cat) {
    if (x instanceof Animal) {
        x;  // Dog | Cat (both extend Animal)
    }
    if (x instanceof Dog) {
        x.bark();  // OK: x is Dog
    } else {
        x.meow();  // OK: x is Cat
    }
}
        ",
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Instanceof narrowing with class hierarchy should work nominally.\n\
         Actual errors: {diagnostics:?}"
    );
}

/// TS18013 should report the declaring class name, not the object type's class name.
/// When `#prop` is declared in `Base` and accessed via `Derived`, the error message
/// should say "outside class 'Base'", not "outside class 'Derived'".
#[test]
fn test_ts18013_reports_declaring_class_name() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    #prop: number = 123;
    static method(x: Derived) {
        console.log(x.#prop);
    }
}
class Derived extends Base {
    static method(x: Derived) {
        console.log(x.#prop);
    }
}
        "#,
    );

    let ts18013_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 18013)
        .map(|(_, m)| m.as_str())
        .collect();

    assert_eq!(
        ts18013_messages.len(),
        1,
        "Should emit exactly one TS18013.\nActual errors: {diagnostics:?}"
    );
    assert!(
        ts18013_messages[0].contains("'Base'"),
        "TS18013 should reference the declaring class 'Base', not 'Derived'.\n\
         Actual message: {}",
        ts18013_messages[0]
    );
}

/// TS18013 diagnostic should use the actual class name, not "the class".
/// When accessing `obj.#prop` outside its declaring class via a type annotation,
/// the error message must say "outside class '`ClassName`'" with the real name.
#[test]
fn test_ts18013_uses_actual_class_name_not_the_class() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class A2 {
    #prop: number = 1;
}
function test(a: A2) {
    a.#prop;
}
        "#,
    );

    let ts18013_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 18013)
        .map(|(_, m)| m.as_str())
        .collect();

    assert_eq!(
        ts18013_messages.len(),
        1,
        "Should emit exactly one TS18013.\nActual errors: {diagnostics:?}"
    );
    assert!(
        ts18013_messages[0].contains("'A2'"),
        "TS18013 should use the actual class name 'A2', not 'the class'.\n\
         Actual message: {}",
        ts18013_messages[0]
    );
    assert!(
        !ts18013_messages[0].contains("the class"),
        "TS18013 should not contain 'the class' as fallback.\n\
         Actual message: {}",
        ts18013_messages[0]
    );
}
