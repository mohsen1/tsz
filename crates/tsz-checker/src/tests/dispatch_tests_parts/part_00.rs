#[test]
fn structural_nodes_do_not_poison_expression_dispatch() {
    let diags = check_source_diagnostics(
        r#"
export const value = 1;
export { value };

const run: () => void = () => {
    value;
};
"#,
    );
    assert_eq!(
        diags.len(),
        0,
        "Expected block bodies and named exports to remain structural, got: {:?}",
        diagnostic_summaries(&diags)
    );
}

#[test]
fn ts7006_false_positive_arrow_in_generic_call() {
    // Arrow functions in object literal properties within generic indexed-access
    // calls should receive contextual typing from the inferred type parameter.
    // This tests that TS7006 is NOT falsely emitted for `r` in `callback: (r) => {}`.
    let diags = check_source_diagnostics(
        r#"
type Events = {
    a: { callback: (r: string) => void }
};
declare function emit<T extends keyof Events>(type: T, data: Events[T]): void;
emit('a', {
    callback: (r) => {},
});
"#,
    );
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 for contextually-typed arrow param, got: {:?}",
        diagnostic_messages(&ts7006)
    );
}

#[test]
fn ts7006_no_false_positive_arrow_in_typed_parameter_default() {
    let diags = check_source_diagnostics(
        r#"
function withContextualDefault(fn: (x: number) => number = x => x * 2) {
    return fn(5);
}
"#,
    );
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 when arrow default is contextually typed by parameter annotation, got: {:?}",
        diagnostic_messages(&ts7006)
    );
}

#[test]
fn ts7006_no_false_positive_arrow_in_typed_parameter_default_alt_name() {
    let diags = check_source_diagnostics(
        r#"
function withContextualDefault(fn: (value: string) => string = value => value.toUpperCase()) {
    return fn("hello");
}
"#,
    );
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 when arrow default is contextually typed (alt name), got: {:?}",
        diagnostic_messages(&ts7006)
    );
}

#[test]
fn ts7006_still_emitted_for_unannotated_parameter_default_arrow() {
    let diags = check_source_diagnostics(
        r#"
function noAnnotation(a = (x: unknown) => x, b = y => y) {}
"#,
    );
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts7006.len(),
        1,
        "Expected exactly one TS7006 for unannotated arrow parameter in un-typed default, got: {:?}",
        diagnostic_messages(&ts7006)
    );
}

#[test]
fn ts2352_this_type_assertion_in_class() {
    let diags = check_source_diagnostics(
        r#"
class C5 {
    bar() {
        let x1 = <this>undefined;
        let x2 = undefined as this;
    }
}
"#,
    );
    let matching = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        matching.len(),
        2,
        "Expected 2 TS2352 for this type assertions, got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn ts2352_object_literal_this_property_assertion_uses_class_instance_overlap() {
    let diags = check_source_diagnostics(
        r#"
namespace M {
    export interface I {
        works: () => R;
        alsoWorks: () => R;
        doesntWork: () => R;
    }

    export interface R {
        anything: number;
        oneI: I;
    }

    export class C implements I {
        constructor(public x: number) {}
        works(): R {
            return <R>({ anything: 1 });
        }
        doesntWork(): R {
            return { anything: 1, oneI: this };
        }
        worksToo(): R {
            return <R>({ oneI: this });
        }
    }
}
"#,
    );
    let matching = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        matching.len(),
        1,
        "Expected one TS2352 for object-literal assertion with non-overlapping `this` property, got: {:?}",
        diagnostic_codes(&diags)
    );
    assert!(
        matching[0].message_text.contains("to type 'R'"),
        "Expected TS2352 target display to preserve `R`, got: {:?}",
        matching[0].message_text
    );
}

#[test]
fn ts2352_angle_bracket_type_display_no_trailing_gt() {
    // For `<T>expr`, the type node span may include `>` — verify it's stripped
    let diags = check_source_diagnostics(
        r#"
class A { foo() { return ""; } }
class B extends A { bar() { return 1; } }
function foo2<T extends A>(x: T) {
    var y = x;
    y = <T>1;
}
"#,
    );
    let matching = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2352, got: {:?}",
        diagnostic_codes(&diags)
    );
    // Verify message says "type 'T'" not "type 'T>'"
    let msg = &matching[0].message_text;
    assert!(
        msg.contains("to type 'T'"),
        "Expected 'to type 'T'' in message, got: {msg}"
    );
}

#[test]
fn ts2352_this_type_assertion_static_no_error() {
    // In static context, `this` is invalid (TS2526), so TS2352 should not fire
    let diags = check_source_diagnostics(
        r#"
class C2 {
    static y = <this>undefined;
}
"#,
    );
    let ts2352 = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        ts2352.len(),
        0,
        "Expected no TS2352 in static context, got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn ts2352_structured_target_with_type_parameter_still_reports() {
    let diags = check_source_diagnostics(
        r#"
function f<T>() {
    const x = <T[]>null;
}
"#,
    );
    // Filter out TS2318 "Cannot find global type" from missing lib declarations.
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let matching = diagnostic_refs_with_code(&relevant, 2352);
    assert_eq!(
        matching.len(),
        1,
        "Expected one TS2352 for `null as T[]`, got: {relevant:?}"
    );
    assert!(
        matching[0].message_text.contains("type 'T[]'"),
        "Expected TS2352 target display to preserve `T[]`, got: {:?}",
        matching[0]
    );
}

#[test]
fn ts2352_in_overloaded_callback_body_survives_catch_all_resolution() {
    let diags = check_source_diagnostics(
        r#"
declare function foo(a: (x: number) => string[]): typeof a;
declare function foo(a: any): any;
const r = foo(<T, U>(x: T) => <U[]>null);
"#,
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let matching = diagnostic_refs_with_code(&relevant, 2352);
    assert_eq!(
        matching.len(),
        1,
        "Expected one TS2352 for `null as U[]` inside overloaded callback, got: {relevant:?}"
    );
    assert!(
        matching[0].message_text.contains("type 'U[]'"),
        "Expected TS2352 target display to preserve `U[]`, got: {:?}",
        matching[0]
    );
}

#[test]
fn ts2352_concrete_generic_class_instantiation_still_reports() {
    let diags = check_source_diagnostics(
        r#"
class A<T> { foo(x: T) { }}
const foo = new A<number>();
const r: A<number> = <A<A<number>>>foo;
"#,
    );
    let matching = diagnostics_with_code(&diags, 2352);
    let assignment = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        matching.len(),
        1,
        "Expected one TS2352 for incompatible concrete generic instantiations, got: {:?}",
        diagnostic_codes(&diags)
    );
    assert_eq!(
        assignment.len(),
        1,
        "Expected one TS2322 for incompatible concrete generic assignment, got: {:?}",
        diagnostic_codes(&diags)
    );
    assert!(
        matching[0].message_text.contains("type 'A<A<number>>'"),
        "Expected TS2352 target display to preserve `A<A<number>>`, got: {:?}",
        matching[0]
    );
}

#[test]
fn ts2352_typeof_instantiation_expands_constructor_call_intersection() {
    let diags = check_source_diagnostics(
        r#"
class ErrImpl<E> {
    e!: E;
}

declare const Err: typeof ErrImpl & (<T>() => T);

type ErrAlias<U> = typeof Err<U>;

declare const e: ErrAlias<number>;
e as ErrAlias<string>;
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let matching = diagnostic_refs_with_code(&relevant, 2352);
    assert_eq!(matching.len(), 1, "Expected one TS2352, got: {relevant:?}");

    let message = &matching[0].message_text;
    assert!(
        message.contains("{ new (): ErrImpl<number>; prototype: ErrImpl<any>; } & (() => number)"),
        "Expected TS2352 source display to expand instantiated typeof intersection, got: {message:?}"
    );
    assert!(
        message.contains("{ new (): ErrImpl<string>; prototype: ErrImpl<any>; } & (() => string)"),
        "Expected TS2352 target display to expand instantiated typeof intersection, got: {message:?}"
    );
}

#[test]
fn ts2352_typeof_instantiation_wrapper_alias_preserves_outer_alias() {
    let diags = check_source_diagnostics(
        r#"
declare class Boxed<T> {
    value!: T;
}

declare function make<T>(): T;

type BoxedCtor<T> = typeof Boxed<T>;
type Maker<T> = typeof make<T>;
type Combined<T> = BoxedCtor<T> & Maker<T>;

declare const combined: Combined<number>;
combined as Combined<string>;
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let matching = diagnostic_refs_with_code(&relevant, 2352);
    assert_eq!(matching.len(), 1, "Expected one TS2352, got: {relevant:?}");

    let message = &matching[0].message_text;
    assert!(
        message.contains("Conversion of type 'Combined<number>' to type 'Combined<string>'"),
        "Expected TS2352 to preserve the outer alias application, got: {message:?}"
    );
    assert!(
        !message.contains("{ new (): Boxed"),
        "Wrapper aliases should not expand to the constructor/call intersection in TS2352. Got: {message:?}"
    );
}

#[test]
fn ts2344_failed_typeof_instantiation_emits_constraint_diagnostic() {
    // `typeof fn<TArgs>` is an instantiation expression. When TArgs do not
    // match any signature's type-parameter arity, tsc emits TS2635 at the
    // instantiation site AND TS2344 at the surrounding type-argument position
    // because the instantiation result is treated as errorType which does
    // not satisfy the declared type-parameter constraint.
    //
    // Mirrors `compiler/instantiationExpressionErrorNoCrash.ts` without
    // depending on lib.d.ts (which the unit-test pipeline disables).
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = T;
declare const createCacheReducer: <N extends string, QR>(q: QR) => QR;
type Cache<QR> = {
    queries: {
        [QK in keyof QR]: RT<typeof createCacheReducer<QR>>;
    };
};
"#,
    );

    let codes = diagnostic_codes(&diags);
    let ts2635 = codes.iter().filter(|&&c| c == 2635).count();
    let ts2344 = codes.iter().filter(|&&c| c == 2344).count();
    assert_eq!(
        ts2635, 1,
        "Expected one TS2635 at the instantiation expression, got diags: {diags:?}"
    );
    assert_eq!(
        ts2344, 1,
        "Expected one TS2344 against the callable type-parameter constraint, got diags: {diags:?}"
    );
    let ts2344 = diagnostics_with_code(&diags, 2344);
    let message = &ts2344[0].message_text;
    assert!(
        message.contains("Type 'typeof createCacheReducer<QR>' does not satisfy the constraint"),
        "Expected TS2344 to preserve the failed typeof-instantiation surface, got: {message:?}"
    );
    assert!(
        !message.contains("typeof <N extends string"),
        "TS2344 display must not expand the failed typeof-instantiation expression, got: {message:?}"
    );
}

#[test]
fn ts2344_function_type_arg_with_extra_required_param_fails_single_param_constraint() {
    let diags = check_source_diagnostics(
        r#"
type ArgumentType<T extends (x: any) => any> =
    T extends (a: infer A) => any ? A : any;
type Bad = ArgumentType<(x: string, y: string) => number>;
"#,
    );

    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert_eq!(ts2344.len(), 1, "Expected one TS2344, got: {diags:?}");
    assert!(
        ts2344[0]
            .message_text
            .contains("(x: string, y: string) => number"),
        "Expected TS2344 to report the function type argument, got: {:?}",
        ts2344[0]
    );
}

#[test]
fn ts2344_single_constrained_infer_fails_incompatible_true_branch_constraint() {
    let diags = check_source_diagnostics(
        r#"
type T70<T extends string> = { x: T };
type T72<T extends number> = { y: T };
type T73<T> = T extends T72<infer U> ? T70<U> : never;
"#,
    );

    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert_eq!(ts2344.len(), 1, "Expected one TS2344, got: {diags:?}");
    assert!(
        ts2344[0].message_text.contains("constraint 'string'"),
        "Expected TS2344 against the true-branch string constraint, got: {:?}",
        ts2344[0]
    );
}

#[test]
fn typeof_globalthis_does_not_satisfy_arbitrary_required_constraint() {
    let diags = check_source_diagnostics(
        r#"
type Need<T extends { definitelyMissing: string }> = T;
type Bad = Need<typeof globalThis>;
"#,
    );

    assert!(
        diags.iter().any(|diag| diag.code == 2344),
        "Expected TS2344 for typeof globalThis missing required constraint property, got: {diags:?}"
    );
}

#[test]
fn ts2635_instantiation_expression_displays_evaluated_indexed_parameter_type() {
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = any;
const createCacheReducer = <N extends string, QR>(
    queries: Cache<N, QR>["queries"],
) => {
    const queriesMap = {} as QR;
    const initialState = { queries: queriesMap };
    return (state = initialState) => state;
};
type Cache<N extends string, QR> = {
    queries: {
        [QK in keyof QR]: RT<typeof createCacheReducer<QR>>;
    };
};
"#,
    );

    let ts2635 = diagnostics_with_code(&diags, 2635);
    assert_eq!(ts2635.len(), 1, "Expected one TS2635, got: {diags:?}");
    let message = &ts2635[0].message_text;
    assert!(
        message.contains("queries: { [QK in keyof QR]: any; }"),
        "Expected TS2635 to display the evaluated indexed-access parameter type, got: {message:?}"
    );
    assert!(
        !message.contains("Lazy("),
        "TS2635 display must not leak Lazy(...) internals, got: {message:?}"
    );
}

#[test]
fn ts2635_instantiation_expression_treats_failed_typeof_as_any_in_alias_display() {
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : any;
const createCacheReducer = <N extends string, QR>(
    queries: Cache<N, QR>["queries"],
) => {
    const queriesMap = {} as QR;
    const initialState = { queries: queriesMap };
    return (state = initialState) => state;
};
type Cache<N extends string, QR> = {
    queries: {
        [QK in keyof QR]: RT<typeof createCacheReducer<QR>>;
    };
};
"#,
    );

    let ts2635 = diagnostics_with_code(&diags, 2635);
    assert_eq!(ts2635.len(), 1, "Expected one TS2635, got: {diags:?}");
    let message = &ts2635[0].message_text;
    assert!(
        message.contains("queries: { [QK in keyof QR]: any; }"),
        "Expected failed typeof-instantiation aliases to reduce through any, got: {message:?}"
    );
    assert!(
        !message.contains("[QK in keyof QR]: (state?: { queries: QR; })"),
        "TS2635 display must not expand the failed typeof-instantiation inside the parameter map, got: {message:?}"
    );
}

#[test]
fn ts2344_valid_typeof_instantiation_does_not_emit_constraint_diagnostic() {
    // Sanity check: a *successful* typeof-instantiation expression must not
    // trigger TS2344 against a callable constraint. Use a concrete type arg
    // to keep the assertion focused on the new arity check.
    let diags = check_source_diagnostics(
        r#"
type RT<T extends (...args: any) => any> = T;
declare const createReducer: <S>(s: S) => S;
type R = RT<typeof createReducer<string>>;
"#,
    );
    let ts2344 = diagnostic_count_with_code(&diags, 2344);
    assert_eq!(
        ts2344, 0,
        "Successful typeof-instantiation must not emit TS2344, got diags: {diags:?}"
    );
}

#[test]
fn ts2344_parenthesized_typeof_instantiation_does_not_emit_constraint_diagnostic() {
    let diags = check_source_diagnostics(
        r#"
type Inst<T extends abstract new (...args: any) => any> = T extends abstract new (...args: any) => infer R ? R : any;
let Anon = class <out T> {
    foo(): Inst<(typeof Anon<T>)> {
        return this;
    }
};
"#,
    );
    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert!(
        ts2344.is_empty(),
        "Parenthesized typeof-instantiation should not emit TS2344, got: {diags:?}"
    );
}

#[test]
fn ts2635_instantiation_expression_filters_union_members_like_tsc() {
    let diags = check_source_diagnostics(
        r#"
function ok(f: (<T>(a: T) => T) | { x: string }) {
    f<string>;
}
function bad(f: (<T>(a: T) => T) | ((a: string, b: number) => string[])) {
    f<string>;
}
"#,
    );

    let ts2635 = diagnostics_with_code(&diags, 2635);
    assert_eq!(ts2635.len(), 1, "Expected one TS2635, got: {diags:?}");
    assert!(
        ts2635[0]
            .message_text
            .contains("(a: string, b: number) => string[]"),
        "Expected TS2635 to report the non-generic function member, got: {:?}",
        ts2635[0].message_text
    );
}

#[test]
fn typeof_instantiation_validates_call_and_construct_constraints() {
    let diags = check_source_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> = T;
type A<U> = InstanceType<typeof Array<U>>;

declare const g2: {
    <T extends string>(a: T): T;
    new <T extends number>(b: T): T;
}

type T40<U extends string> = typeof g2<U>;
type T41<U extends number> = typeof g2<U>;
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let ts2344 = diagnostic_refs_with_code(&relevant, 2344);
    assert_eq!(
        ts2344.len(),
        2,
        "Expected two TS2344 errors, got: {relevant:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|d| d.message_text.contains("constraint 'number'")),
        "Expected construct constraint diagnostic, got: {ts2344:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|d| d.message_text.contains("constraint 'string'")),
        "Expected call constraint diagnostic, got: {ts2344:?}"
    );
}

#[test]
fn invalid_instancetype_indexed_access_suppresses_cascading_ts2344() {
    let diags = check_source_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> = T;

const Outer = class {
    Inner = class {
        value = "inner";
    };

    createInner(): InstanceType<Outer["Inner"]> {
        return new this.Inner();
    }
};
"#,
    );

    let ts2749 = diagnostics_with_code(&diags, 2749);
    assert_eq!(
        ts2749.len(),
        1,
        "Expected one TS2749 for value used as type, got: {diags:?}"
    );

    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert!(
        ts2344.is_empty(),
        "Invalid value-as-type argument should not also emit TS2344, got: {diags:?}"
    );
}

#[test]
fn instancetype_constraint_violation_still_emits_ts2344() {
    let diags = check_source_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> = T;
type Bad = InstanceType<string>;
"#,
    );

    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert_eq!(
        ts2344.len(),
        1,
        "Expected genuine InstanceType constraint violation to emit TS2344, got: {diags:?}"
    );
}

#[test]
fn instancetype_private_constructor_constraint_violation_emits_ts2344() {
    let diags = check_source_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> = T;

const WithPrivateCtor = class {
    private constructor() {}
};

type Bad = InstanceType<typeof WithPrivateCtor>;
"#,
    );

    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert_eq!(
        ts2344.len(),
        1,
        "Expected private constructor InstanceType constraint violation to emit TS2344, got: {diags:?}"
    );
}

#[test]
fn ts2352_array_assertion_anchors_first_excess_property() {
    let source = r#"
<{ id: number; }[]>[{ foo: "s" }];
"#;
    let diags = check_source_diagnostics(source);
    let matching = diagnostics_with_code(&diags, 2352);
    assert_eq!(matching.len(), 1, "Expected one TS2352, got: {diags:?}");

    let foo_pos = source.find("foo").expect("expected foo property") as u32;
    assert_eq!(
        matching[0].start, foo_pos,
        "Expected TS2352 to anchor at the excess property name, got: {matching:?}"
    );

    let ts2353 = diagnostics_with_code(&diags, 2353);
    assert!(
        ts2353.is_empty(),
        "Type assertions should not emit nested TS2353 from array elements, got: {diags:?}"
    );
}

#[test]
fn ts2352_array_assertion_with_best_common_type_does_not_emit_ts2353() {
    let diags = check_source_diagnostics(
        r#"
<{ id: number; }[]>[{ foo: "s" }, {}];
"#,
    );

    assert!(
        diags.is_empty(),
        "Expected no diagnostics when array assertion falls back to best common type, got: {diags:?}"
    );
}

#[test]
fn ts2352_merged_class_namespace_record_cast_reports_missing_string_index() {
    let diags = check_source_diagnostics(
        r#"
type Dict = { [key: string]: unknown };
class C1 { foo() {} }
new C1() as Dict;

class C2 { foo() {} }
namespace C2 { export const unrelated = 3; }
new C2() as Dict;

namespace C3 { export const unrelated = 3; }
C3 as Dict;
"#,
    );

    let ts2352 = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        ts2352.len(),
        2,
        "Expected exactly two TS2352 diagnostics, got: {diags:?}"
    );
    assert!(
        ts2352
            .iter()
            .all(|diag| diag.message_text.contains("Conversion of type")),
        "Expected TS2352 conversion diagnostics for the class assertions, got: {ts2352:?}"
    );
}

#[test]
fn ts2352_record_mapped_type_equivalent_to_direct_index_signature() {
    // Record<string, unknown> should evaluate identically to { [key: string]: unknown }.
    // Without defining Record (it's from lib.d.ts, not in check_source_diagnostics),
    // this verifies mapped type evaluation produces the same assignability result.
    let diags_record = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
class C1 { foo() {} }
let x: Record<string, unknown> = new C1();
"#,
    );
    let diags_direct = check_source_diagnostics(
        r#"
class C1 { foo() {} }
type Dict = { [key: string]: unknown };
let x: Dict = new C1();
"#,
    );
    let ts2322_record = diagnostic_count_with_code(&diags_record, 2322);
    let ts2322_direct = diagnostic_count_with_code(&diags_direct, 2322);
    assert_eq!(
        ts2322_record, ts2322_direct,
        "Record<string, unknown> and {{[key: string]: unknown}} must have identical assignability"
    );
    assert_eq!(
        ts2322_record, 1,
        "Class without index signature should not be assignable"
    );
}

#[test]
fn ts2352_merged_class_namespace_record_generic_cast() {
    // Same as ts2352_merged_class_namespace_record_cast but using Record<string, unknown>
    // (a mapped type) instead of a direct index signature. This reproduces the
    // conformance failure in mergedClassNamespaceRecordCast.ts.
    let diags = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
class C1 { foo() {} }
new C1() as Record<string, unknown>;

class C2 { foo() {} }
namespace C2 { export const unrelated = 3; }
new C2() as Record<string, unknown>;

C2.unrelated;
new C2().unrelated;

namespace C3 { export const unrelated = 3; }
C3 as Record<string, unknown>;
"#,
    );

    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339 for new C2().unrelated, got: {ts2339:?}"
    );

    let ts2352 = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        ts2352.len(),
        2,
        "Expected exactly two TS2352 diagnostics for C1 and C2 assertions, got: {diags:?}"
    );
}

#[test]
fn ts2339_property_access_anchors_property_token() {
    let source = r#"
declare const value: {};
value.missing;
"#;

    let diags = check_source_diagnostics(source);
    let matching = diagnostics_with_code(&diags, 2339);
    assert_eq!(matching.len(), 1, "Expected one TS2339, got: {diags:?}");

    let missing_pos = source.find("missing").expect("expected property token") as u32;
    assert_eq!(
        matching[0].start, missing_pos,
        "Expected TS2339 to anchor at the property token, got: {matching:?}"
    );
    assert_eq!(
        matching[0].length, 7,
        "Expected TS2339 to cover only the property token"
    );
}

#[test]
fn ts7053_element_access_anchors_full_expression() {
    let source = r#"
declare const key: string;
declare const value: {};
value[key];
"#;

    let diags = check_source_diagnostics(source);
    let matching = diagnostics_with_code(&diags, 7053);
    assert_eq!(matching.len(), 1, "Expected one TS7053, got: {diags:?}");

    let expr_pos = source.find("value[key]").expect("expected element access") as u32;
    assert_eq!(
        matching[0].start, expr_pos,
        "Expected TS7053 to anchor at the full element access expression, got: {matching:?}"
    );
}

#[test]
fn ts7015_number_index_error_anchors_index_argument() {
    let source = r#"
declare const arr: number[];
arr["name"];
"#;

    let diags = check_source_diagnostics(source);
    let matching = diagnostics_with_code(&diags, 7015);
    assert_eq!(matching.len(), 1, "Expected one TS7015, got: {diags:?}");

    let index_pos = source.find("\"name\"").expect("expected string index") as u32;
    assert_eq!(
        matching[0].start, index_pos,
        "Expected TS7015 to anchor at the index argument, got: {matching:?}"
    );
}

#[test]
fn ts2345_never_parameter_uses_non_contextual_object_literal_display() {
    let diags = check_source_diagnostics(
        r#"
declare function fn(x: never): void;
fn({ a: 1, b: 2 });
"#,
    );
    let matching = diagnostics_with_code(&diags, 2345);
    assert_eq!(matching.len(), 1, "Expected one TS2345, got: {diags:?}");

    let msg = &matching[0].message_text;
    // tsc widens literal types in object literal display for diagnostics
    assert!(
        msg.contains("Argument of type '{ a: number; b: number; }'"),
        "Expected widened object literal display (matching tsc), got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'never'"),
        "Expected never parameter display, got: {msg}"
    );
}

#[test]
fn type_params_in_object_literal_methods_no_ts2304() {
    // Type parameters in object literal method shorthands must be in scope
    // for parameter types, return types, and body type references.
    let diags = check_source_diagnostics(
        r#"
let a = {
    test<K>(x: K): K { return x; }
};
interface Bar { bar: number; }
let b = {
    test<K extends keyof Bar>(a: K, b: Bar[K]) { }
};
"#,
    );
    let ts2304 = diagnostics_with_code(&diags, 2304);
    assert_eq!(
        ts2304.len(),
        0,
        "Expected no TS2304 for type params in object literal methods, got: {:?}",
        diagnostic_messages(&ts2304)
    );
}

#[test]
fn class_namespace_merge_same_file_no_ts2351() {
    // Same-file class+namespace merge: `new A()` inside `namespace A` should
    // resolve to the class constructor, not produce TS2351.
    let diags = check_source_diagnostics(
        r#"
class A {
    id: string;
}
namespace A {
    export var Instance = new A();
}
"#,
    );
    let ts2351 = diagnostics_with_code(&diags, 2351);
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for class+namespace merge, got: {:?}",
        diagnostic_messages(&ts2351)
    );
}

#[test]
fn contextual_request_does_not_leak_between_sibling_properties() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(arg: {
    left: (s: string) => void;
    right: (n: number) => void;
}): void;

takes({
    left: s => s.toUpperCase(),
    right: n => n.toFixed(),
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected no contextual leak diagnostics, got: {relevant:?}"
    );
}

#[test]
fn write_context_access_does_not_reuse_read_cache() {
    let diags = check_source_diagnostics(
        r#"
declare const access: {
    get value(): undefined;
    set value(v: number);
};

const read1: undefined = access.value;
access.value = 1;
const read2: undefined = access["value"];
access["value"] = 1;
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2540)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected write-context accesses to use setter/write types, got: {relevant:?}"
    );
}

#[test]
fn assertion_origin_does_not_leak_outside_asserted_expression() {
    let diags = check_source_diagnostics(
        r#"
const asserted = ((x) => 1) as (x: string) => string;
const assigned: (x: string) => string = (x) => 1;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one assignment-context TS2322, got: {diags:?}"
    );
    assert_eq!(
        ts7006.len(),
        0,
        "Expected asserted expression parameters to stay contextually typed, got: {diags:?}"
    );
}

#[test]
fn speculative_overload_check_does_not_poison_successful_candidate() {
    let diags = check_source_diagnostics(
        r#"
declare function fn(cb: (s: number) => void): void;
declare function fn(cb: (s: string) => void): void;

fn(s => s.toUpperCase());
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected speculative overload rollback to avoid poisoning the successful candidate, got: {relevant:?}"
    );
}

#[test]
fn string_argument_does_not_match_generic_array_overload() {
    let diags = check_source_diagnostics(
        r#"
function first<T>(arr: T[]): T;
function first(arr: string): string;
function first(arr: any): any {
  return typeof arr === 'string' ? arr[0] : arr[0];
}

const f1: number = first([1, 2, 3]);
const f2: string = first("hello");
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|diagnostic| matches!(diagnostic.code, 2322 | 2345 | 2769))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected string argument to select string overload, got: {relevant:?}"
    );
}

#[test]
fn nested_object_literal_context_is_preserved_without_ambient_restore() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(arg: {
    outer: {
        onText: (s: string) => void;
        nested: { onNumber: (n: number) => void };
    };
}): void;

takes({
    outer: {
        onText: s => s.toUpperCase(),
        nested: {
            onNumber: n => n.toFixed(),
        },
    },
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected nested object literal contextual typing to stay isolated, got: {relevant:?}"
    );
}

#[test]
fn annotated_variable_accepts_nested_anonymous_object_literal() {
    let diags = check_source_diagnostics(
        r#"
interface User {
  id: string,
  profile: {
    name: string,
    admin: boolean
  }
}

const user: User = {
  id: "u1",
  profile: {
    name: "ada",
    admin: true
  }
}
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "expected nested anonymous object literal assignment to be valid, got: {diags:?}"
    );
}

#[test]
fn annotated_variable_accepts_named_nested_object_literal() {
    let diags = check_source_diagnostics(
        r#"
interface Profile {
  name: string;
  admin: boolean;
}

interface User {
  id: string;
  profile: Profile;
}

const user: User = {
  id: "u1",
  profile: {
    name: "ada",
    admin: true
  }
}
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "expected nested named object literal assignment to be valid, got: {diags:?}"
    );
}

#[test]
fn nested_mapped_application_property_preserves_literal_context() {
    let diags = check_source_diagnostics(
        r#"
type Required<T> = { [K in keyof T]-?: T[K] };
interface Foo<T> {
    a: Required<T>;
}
const aa: Foo<{ a?: 1; x: 1 }> = { a: { a: 1, x: 1 } };
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected nested Required<T> context to preserve literal property types, got: {diags:?}"
    );
}

#[test]
fn iife_contextual_typing_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(arg: { cb: (s: string) => void }): void;

takes((() => ({
    cb: s => s.toUpperCase(),
}))());
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected no IIFE contextual-typing regressions, got: {relevant:?}"
    );
}

#[test]
fn jsx_children_and_props_use_request_path() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
}

declare function Comp(props: { render: (s: string) => JSX.Element }): JSX.Element;

<Comp render={s => { s.toUpperCase(); return <div />; }} />;
"#,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSX request-path contextual typing to work, got: {relevant:?}"
    );
}

#[test]
fn destructuring_request_path_stays_stable_in_switch_parameter_and_variable_positions() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(cb: (arg: { value: string }) => string): void;

switch (0) {
    case 0: {
        const inferred = ({ value = "ok" } = {}) => value;
        const annotated: typeof inferred = ({ value = "ok" } = {}) => value;
        takes(({ value = "x" }) => value.toUpperCase());
        break;
    }
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected destructuring request transport to survive switch/parameter/variable paths, got: {relevant:?}"
    );
}

#[test]
fn destructuring_parameter_declaration_preserves_nested_binding_context() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(fn: ([a, b, [[c]], ...x]: [number, number, [[string]], boolean, boolean]) => void): void;

takes(([a, b, [[c]], ...x]) => {
    a.toFixed();
    b.toFixed();
    c.toUpperCase();
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322 | 2339 | 7031))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected destructuring parameter bindings to stay request-aware, got: {relevant:?}"
    );
}

#[test]
fn catch_finally_and_logical_assignment_preserve_request_intent() {
    let diags = check_source_diagnostics(
        r#"
let box: { text?: string } = {};

try {
    box.text ||= "x";
} catch ({ message = "err" }) {
    message.toUpperCase();
} finally {
    box.text &&= box.text.trim();
}

box.text = box.text || "ok";
box.text!.toUpperCase();
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected catch/finally and logical assignment request flow to stay stable, got: {relevant:?}"
    );
}

#[test]
fn nonnull_assertion_context_stays_local_to_asserted_expression() {
    let diags = check_source_diagnostics(
        r#"
const ok: (s: string) => string = ((x) => x)!;
const bad: (s: string) => number = x => x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one non-null-containment TS2322, got: {diags:?}"
    );
    assert_eq!(
        ts7006.len(),
        0,
        "Expected non-null assertion contextual typing to stay local, got: {diags:?}"
    );
}

#[test]
fn generic_contextual_function_inference_uses_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function mapValue<T, U>(value: T, fn: (x: T) => U): U;

const result = mapValue({ text: "ok" }, ({ text }) => text.toUpperCase());
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 7031 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected generic contextual inference to remain request-aware, got: {relevant:?}"
    );
}

#[test]
fn generic_mapped_method_contextual_typing_uses_request_path() {
    let diags = check_source(
        r#"
declare function f<T extends object>(
    data: T,
    handlers: { [P in keyof T]: (value: T[P], prop: P) => void },
): void;

f({ data: 0 }, {
    data(value, key) {
        value.toFixed();
        key.toUpperCase();
    },
});
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected generic mapped method shorthand to stay contextually typed, got: {relevant:?}"
    );
}

#[test]
fn computed_mapped_callback_context_uses_callable_fallback() {
    let diags = check_source(
        r#"
declare function tag(): "d";

declare function forceMatch<T>(matched: {
    [K in keyof T]: ({ key }: { key: K }) => void;
}): void;

forceMatch({
    [tag()]: ({ key }) => {
        const exact: "d" = key;
    },
});
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7031 | 7006 | 2322))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected computed mapped callbacks to keep callable context, got: {relevant:?}"
    );
}

#[test]
fn return_context_substitution_preserves_rest_tuple_callback_args() {
    let diags = check_source(
        r#"
interface Generator<Y, R, N> {}
type Covariant<A> = (_: never) => A;
interface Effect<out A> {
    readonly _A: Covariant<A>;
}

declare function lift<AEff, Args extends Array<any>>(
    body: (...args: Args) => Generator<never, AEff, never>,
): (...args: Args) => Effect<AEff>;

declare function takes(handler: (a: string) => Effect<void>): void;

takes(lift(function* (a) {
    a.toUpperCase();
}));
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected return-context substitution to preserve rest-tuple callback args, got: {relevant:?}"
    );
}

#[test]
fn nested_return_context_rest_tuple_callback_args_are_not_wrapped() {
    let diags = check_source(
        r#"
interface Generator<Y, R, N> {}
type Covariant<A> = (_: never) => A;
interface Effect<out A, out E = never, out R = never> {
    readonly _A: Covariant<A>;
    readonly _E: Covariant<E>;
    readonly _R: Covariant<R>;
}

declare function effectGen<A, E = never, R = never>(
    body: () => Generator<Effect<A, E, R>, A, never>,
): Effect<A, E, R>;

declare function effectFn<A, E, R, AEff, Args extends Array<any>>(
    body: (...args: Args) => Generator<Effect<A, E, R>, AEff, never>,
): (...args: Args) => Effect<AEff, E, R>;

const foo: Effect<{ fn: (...args: [a: string]) => Effect<void> }> = effectGen(function* () {
    return {
        fn: effectFn(function* (a) {
            a.toUpperCase();
        }),
    };
});
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2339 | 7006))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected nested contextual return rest tuple args to stay flat, got: {relevant:?}"
    );
}

#[test]
fn contextual_this_for_class_expression_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(ctor: new () => { value: string; read(): string }): void;

takes(class {
    value = "ok";
    read() {
        return this.value.toUpperCase();
    }
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2339 | 2683))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected contextual `this` in class expressions to use request transport, got: {relevant:?}"
    );
}

#[test]
fn class_expression_static_field_initializer_checks_own_this() {
    let diags = check_source_diagnostics(
        r#"
class C {
    static f = 1;
    static classExprBoundary = class { a = this.f + 3 };
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        1,
        "Expected one TS2339 for anonymous class `this.f`, got: {diags:?}"
    );
    assert!(
        ts2339[0].message_text.contains("(Anonymous class)"),
        "Expected anonymous class receiver in diagnostic, got: {:?}",
        ts2339[0]
    );
}

#[test]
fn class_property_initializer_this_uses_ast_owner_during_type_environment() {
    let diags = check_source(
        r#"
type ForwardInstance = Model;

export class Model {
    method(): string {
        return "";
    }
    alias = this.method;
}

declare const model: Model;
model.alias;
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2532 | 2683))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected class property initializer `this` to use the AST class owner during early type environment construction, got: {relevant:?}"
    );
}

#[test]
fn class_property_initializer_this_prescan_includes_accessors() {
    let diags = check_source(
        r#"
declare function needsAccessor(value: { readonly current: number }): void;

export class Model {
    get current(): number {
        return 1;
    }
    value = needsAccessor(this);
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2345 | 2739))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected class property initializer `this` prescan to include accessors, got: {relevant:?}"
    );
}

#[test]
fn static_property_initializer_this_uses_constructor_owner_during_type_environment() {
    let diags = check_source(
        r#"
type ForwardConstructor = typeof Registry;

export class Registry {
    static build(): number {
        return 1;
    }
    static create = this.build;
}

Registry.create;
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2532 | 2683))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected static property initializer `this` to use the constructor owner during early type environment construction, got: {relevant:?}"
    );
}

#[test]
fn explicit_this_current_class_does_not_use_any_cached_placeholder() {
    let diags = check_source_diagnostics(
        r#"
const C = class C {
    static getInstance() { return new C(); }
    m(this: C) {
        return this.missing;
    }
};
"#,
    );
    let ts2339: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 && d.message_text.contains("missing"))
        .collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 for explicit `this: C` missing member access, got: {diags:?}"
    );
}

#[test]
fn jsx_children_contextual_typing_uses_request_path() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
    interface ElementChildrenAttribute {
        children: {};
    }
}

declare function Panel(props: { children: (s: string) => JSX.Element }): JSX.Element;

<Panel>{s => { s.toUpperCase(); return <div />; }}</Panel>;
"#,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSX children contextual typing to stay on the request path, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_template_and_param_resolution_stay_stable_through_request_path() {
    let diags = check_source(
        r#"
/** @template T
 * @param {(value: T) => T} fn
 * @param {T} value
 */
function apply(fn, value) {
    return fn(value);
}

/** @template T */
class Box {
    /** @param {T} value */
    constructor(value) {
        this.value = value;
    }
}

/** @param {{ text: string }} value */
const useText = (value) => value.text.toUpperCase();

apply(useText, { text: "ok" });
new Box("ok");
"#,
        "test.js",
        CheckerOptions::default(),
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 7031 | 2304 | 2314 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSDoc template/param resolution to stay stable, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_generic_callback_typedef_type_tag_resolves_as_callable() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @template T
 * @callback B
 * @returns {T}
 */

/** @type {B<string>} */
let b = {};

b();
b(1);
"#,
    );
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for assigning {{}} to generic callback typedef, got: {codes:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2322 && d.message_text.contains("B<string>")),
        "Expected TS2322 to preserve the instantiated JSDoc callback alias in the message, got: {diags:?}"
    );
    assert!(
        codes.contains(&2554),
        "Expected TS2554 for calling instantiated callback typedef with an extra arg, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2349),
        "Expected instantiated callback typedef to stay callable, got: {codes:?}"
    );
}

