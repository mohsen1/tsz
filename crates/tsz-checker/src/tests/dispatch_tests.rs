use crate::context::{CheckerOptions, ScriptTarget};
use crate::diagnostics::Diagnostic;
use crate::test_utils::{
    check_js_source_diagnostics, check_source, check_source_diagnostics, diagnostic_codes,
};
use tsz_common::checker_options::JsxMode;

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

fn diagnostic_refs_with_code<'a>(diagnostics: &[&'a Diagnostic], code: u32) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .copied()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

fn diagnostic_count_with_code(diagnostics: &[Diagnostic], code: u32) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

fn diagnostic_messages<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

fn diagnostic_summaries(diagnostics: &[Diagnostic]) -> Vec<(u32, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
        .collect()
}

fn diagnostic_code_starts(diagnostics: &[Diagnostic]) -> Vec<(u32, u32)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.start))
        .collect()
}

fn diagnostic_ref_summaries<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<(u32, &'a str)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
        .collect()
}

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
}

#[test]
fn ts2344_recursive_conditional_type_arg_defers_base_constraint() {
    let diags = check_source_diagnostics(
        r#"
type Loop<T> = T extends any ? Loop<T> : never;
type NeedsObject<T extends object> = T;
type X<T> = NeedsObject<Loop<T>>;
"#,
    );

    let ts2344 = diagnostics_with_code(&diags, 2344);
    assert!(
        ts2344.is_empty(),
        "Recursive conditional type arguments should defer TS2344 base-constraint checks, got: {diags:?}"
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

#[test]
fn jsdoc_type_tag_with_generic_interface_preserves_args_in_diagnostic() {
    // Regression: assignability messages must preserve `Name<Args>` for
    // generic interface/class refs (not just `@typedef`s) referenced from
    // a JSDoc `@type` annotation. See: subclassThisTypeAssignable01
    // conformance test where
    // `/** @type {ClassComponent<any>} */ const test9 = new C();`
    // previously produced "...is not assignable to type 'ClassComponent'."
    // instead of "...is not assignable to type 'ClassComponent<any>'."
    use crate::CheckerOptions;
    use crate::test_utils::check_source;
    let diags = check_source(
        r#"
interface Box<T> { value: T }
class C { constructor() { this.q = 1; } }

/** @type {Box<string>} */
const b = new C();
"#,
        "test.ts",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    // Must mention the instantiated alias name with type arguments.
    let assignability_codes = [2322u32, 2741];
    assert!(
        diags.iter().any(
            |d| assignability_codes.contains(&d.code) && d.message_text.contains("Box<string>")
        ),
        "Expected an assignability message to mention `Box<string>`, got: {diags:?}"
    );
    // Must NOT show the bare `Box` (without type arguments) in any
    // assignability-class diagnostic.
    let has_bare = diags.iter().any(|d| {
        assignability_codes.contains(&d.code)
            && d.message_text.contains(" 'Box'")
            && !d.message_text.contains("Box<")
    });
    assert!(
        !has_bare,
        "Expected no assignability message to show bare `Box`, got: {diags:?}"
    );
}

#[test]
fn jsdoc_callback_nested_params_build_one_object_parameter() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @callback WorksWithPeopleCallback
 * @param {Object} person
 * @param {string} person.name
 * @param {number} [person.age]
 * @returns {void}
 */

/**
 * @param {WorksWithPeopleCallback} callback
 * @returns {void}
 */
function eachPerson(callback) {
    callback({ name: "Empty" });
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2554 || d.code == 2345)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected nested callback params to shape a single object parameter, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_optional_properties_stay_optional_in_param_tags() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @typedef {Object} Opts
 * @property {string} x
 * @property {string=} y
 * @property {string} [z]
 * @property {string} [w="hi"]
 *
 * @param {Opts} opts
 */
function foo(opts) {
    opts.x;
}

foo({ x: "abc" });
"#,
    );
    let relevant = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        relevant.len(),
        0,
        "Expected optional typedef properties to stay optional at param-tag call sites, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_property_name_then_type_syntax_stays_optional_in_param_tags() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @typedef {Object} AnotherOpts
 * @property anotherX {string}
 * @property anotherY {string=}
 *
 * @param {AnotherOpts} opts
 */
function foo(opts) {
    opts.anotherX;
}

foo({ anotherX: "world" });
"#,
    );
    let relevant = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        relevant.len(),
        0,
        "Expected alternate @property name {{type}} syntax to preserve optionality at param-tag call sites, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_prop_alias_uses_same_property_parser() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @typedef {Object} AliasOpts
 * @prop aliasX {string}
 * @prop [aliasY="hi"] {string}
 *
 * @param {AliasOpts} opts
 */
function foo(opts) {
    opts.aliasX;
}

foo({ aliasX: "world" });
"#,
    );
    let relevant = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        relevant.len(),
        0,
        "Expected @prop alias tags to share typedef property parsing semantics, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_constructor_template_scope_flows_to_prototype_methods() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @constructor
 * @template {string} K
 * @template V
 */
function Multimap() {
    /** @type {Object<string, V>} */
    this._map = {};
}

Multimap.prototype = {
    /**
     * @param {K} key
     * @returns {V}
     */
    get(key) {
        return this._map[key + ""];
    }
};

/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    /** @type {T} */
    this.u;
    this.t = t;
}

/**
 * @param {T} v
 * @param {object} o
 * @param {T} o.nested
 */
Zet.prototype.add = function(v, o) {
    this.u = v || o.nested;
    return this.u;
};

/** @type {number} */
let answer = new Zet(1).add(3, { nested: 4 });
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2339 | 7006 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected constructor @template scope to flow to prototype methods, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_constructor_identifier_argument_uses_typeof_source_display() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @param {function(new: { length: number }, number): number} c
 * @return {function(new: { length: number }, number): number}
 */
function id2(c) {
    return c;
}

/**
 * @constructor
 * @param {number} n
 */
var E = function(n) {
  this.not_length_on_purpose = n;
};

id2(E);
"#,
    );
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(ts2345.len(), 1, "Expected one TS2345, got: {diags:?}");
    let message = &ts2345[0].message_text;
    assert!(
        message.contains("Argument of type 'typeof E'"),
        "Expected JS constructor identifier source display to use `typeof E`, got: {message:?}"
    );
    assert!(
        !message.contains("new (n: number)"),
        "Expected diagnostic not to expand the constructor signature, got: {message:?}"
    );
}

#[test]
fn jsdoc_generic_constructor_prototype_object_literal_methods_use_instance_this() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @class
 * @template T
 * @param {T} t
 */
function Cp(t) {
    this.x = 1;
    this.y = t;
}
Cp.prototype = {
    m1() { return this.x; },
    m2() { this.z = this.x + 1; return this.y; }
};
var cp = new Cp(1);

/** @type {number} */
var n = cp.x;
/** @type {number} */
var n = cp.y;
/** @type {number} */
var n = cp.m1();
/** @type {number} */
var n = cp.m2();
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected generic JS constructor prototype object literal methods to use instance `this`, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_property_unknown_template_name_emits_ts2304() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    this.t = t;
}

/**
 * @typedef {Object} A
 * @property {T} value
 */
/** @type {A} */
const options = { value: null };
"#,
    );
    let ts2304 = diagnostics_with_code(&diags, 2304);
    assert_eq!(
        ts2304.len(),
        1,
        "Expected one TS2304 for out-of-scope typedef property template name, got: {diags:?}"
    );
}

#[test]
fn jsdoc_broken_typedef_body_recovers_alias_as_any() {
    let diags = check_js_source_diagnostics(
        r#"
/** @typedef {U} T */
/**
 * @returns {T}
 */
function f() {
    return 1;
}
/** @type {T} */
const x = 3;
"#,
    );
    let ts2304_messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| d.message_text.to_string())
        .collect();
    assert!(
        ts2304_messages.iter().any(|m| m.contains("'U'")),
        "Expected TS2304 for unresolved typedef body name, got: {diags:?}"
    );
    assert!(
        !ts2304_messages.iter().any(|m| m.contains("'T'")),
        "Broken typedef body should not make the alias name unresolved, got: {diags:?}"
    );
}

#[test]
fn tagged_template_contextual_typing_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function tag(strs: TemplateStringsArray, f: (n: number) => void): void;

tag`${n => n.toFixed()}`;
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected tagged-template contextual typing to stay on the request path, got: {relevant:?}"
    );
}

#[test]
fn yield_contextual_typing_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
interface Generator<Y, R, N> {}

function* gen(): Generator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected yield contextual typing to use request path, got: {relevant:?}"
    );
}

#[test]
fn arrow_expression_body_literal_union_return_no_false_ts2322() {
    // Concise arrow `() => "bar"` assigned to a variable with type `() => "foo" | "bar"`
    // should NOT emit TS2322 — "bar" is a member of the union "foo" | "bar".
    let diags = check_source_diagnostics(
        r#"
type FnType = () => "foo" | "bar";
const f2: FnType = () => "bar";
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for literal arrow return assignable to union, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn dotted_namespace_class_merge_same_file_no_ts2351() {
    // Dotted namespace `X.Y` with class+namespace merge in same file.
    let diags = check_source_diagnostics(
        r#"
namespace X.Y {
    export class Point {
        constructor(x: number, y: number) {
            this.x = x;
            this.y = y;
        }
        x: number;
        y: number;
    }
}
namespace X.Y {
    export namespace Point {
        export var Origin = new Point(0, 0);
    }
}
"#,
    );
    let ts2351 = diagnostics_with_code(&diags, 2351);
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for dotted namespace class merge, got: {:?}",
        diagnostic_messages(&ts2351)
    );
}

#[test]
fn ts2540_as_const_object_method_this_readonly() {
    // When an object literal is declared `as const`, `this` inside methods
    // should see readonly properties.  Assigning to `this.x` must produce
    // TS2540 ("Cannot assign to 'x' because it is a read-only property"),
    // not TS2322 ("Type '20' is not assignable to type '10'").
    let diags = check_source_diagnostics(
        r#"
let o = { x: 10, foo() { this.x = 20 } } as const;
"#,
    );
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        1,
        "Expected 1 TS2540 for readonly property assignment via this in as-const object, got codes: {:?}",
        diagnostic_codes(&diags)
    );
    // Must NOT emit TS2322 — the readonly check takes precedence.
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 when TS2540 (readonly) applies, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn ts2540_as_const_object_method_this_readonly_no_false_positive() {
    // Reading from `this.x` inside an as-const method should NOT produce
    // any error — only writes should trigger TS2540.
    let diags = check_source_diagnostics(
        r#"
let o = { x: 10, foo() { return this.x } } as const;
"#,
    );
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        0,
        "Expected no TS2540 for readonly property read, got: {:?}",
        diagnostic_messages(&ts2540)
    );
}

#[test]
fn ts2540_as_const_nested_method_this_readonly() {
    // Multiple properties in an as-const object with a method that assigns
    // to different properties should all produce TS2540.
    let diags = check_source_diagnostics(
        r#"
let o = {
    x: 10,
    y: "hello",
    foo() {
        this.x = 20;
        this.y = "world";
    }
} as const;
"#,
    );
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        2,
        "Expected 2 TS2540 for readonly property assignments in as-const method, got codes: {:?}",
        diagnostic_summaries(&diags)
    );
}

#[test]
fn no_ts2540_without_const_assertion() {
    // Without `as const`, properties are mutable, so `this.x = 20` should
    // NOT produce TS2540.
    let diags = check_source_diagnostics(
        r#"
let o = { x: 10, foo() { this.x = 20 } };
"#,
    );
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        0,
        "Expected no TS2540 without as-const, got: {:?}",
        diagnostic_messages(&ts2540)
    );
}

#[test]
fn ts2322_typeof_in_type_alias_respects_control_flow_narrowing() {
    // When `typeof c` appears inside a type alias within a narrowed scope,
    // the flow-narrowed type should be used (string, not string | number).
    // This ensures `{ bar: 1 }` is rejected when assigned to type C which
    // has `[key: string]: typeof c` where c has been narrowed to string.
    let diags = check_source_diagnostics(
        r#"
declare let c: string | number;
if (typeof c === 'string') {
    type C = { [key: string]: typeof c };
    const boo1: C = { bar: 'works' };
    const boo2: C = { bar: 1 };
}
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for number not assignable to string, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_tuple_inference_through_conditional_template() {
    // When a mapped type's template is a conditional type like
    // `Tuple[Key] extends Tuple[number] ? MyMappedType<Tuple[Key]> : never`,
    // reverse-mapped inference should be able to reverse through the
    // conditional's true branch to infer Tuple from the argument types.
    // Regression test: previously, reverse_infer_through_template returned
    // None for conditional templates, causing Tuple to default to any[].
    let diags = check_source_diagnostics(
        r#"
type MyMappedType<Primitive extends any> = {
    primitive: Primitive;
};
type TupleMapper<Tuple extends any[]> = {
    [Key in keyof Tuple]: Tuple[Key] extends Tuple[number] ? MyMappedType<Tuple[Key]> : never;
};
declare function extractPrimitives<Tuple extends any[]>(...mappedTypes: TupleMapper<Tuple>): Tuple;
const result: [string, number] = extractPrimitives({ primitive: "" }, { primitive: 0 });
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for reverse-mapped tuple inference through conditional template, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn generic_tuple_rest_argument_infers_union_from_all_rest_elements() {
    let diags = check_source_diagnostics(
        r#"
declare function f0<T, U>(x: [T, ...U[]]): [T, U];
f0([1, "hello", true]);
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 when tuple rest inference merges string | boolean, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_array_return_rejects_mapped_element_to_type_parameter_array() {
    let diags = check_source_diagnostics(
        r#"
interface Stuff {
    field: number;
    anotherField: string;
}
function doStuffWithStuffArr<T extends Stuff>(arr: { [K in keyof T & keyof Stuff]: T[K] }[]): T[] {
    return arr;
}
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.iter().any(|d| {
            d.message_text.contains(
                "Type '{ [K in keyof T & keyof Stuff]: T[K]; }[]' is not assignable to type 'T[]'",
            )
        }),
        "Expected TS2322 for reverse-mapped array return, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_dependent_default_uses_inferred_literal_not_constraint() {
    let diags = check_source_diagnostics(
        r#"
type Record<K extends string, T> = { [P in K]: T };
type StateConfig<TAction extends string> = {
  entry?: TAction;
  states?: Record<string, StateConfig<TAction>>;
};
declare function createMachine<
  TConfig extends StateConfig<TAction>,
  TAction extends string = TConfig["entry"] extends string ? TConfig["entry"] : string,
>(config: { [K in keyof TConfig & keyof StateConfig<any>]: TConfig[K] }): [TAction, TConfig];
createMachine({
  entry: "foo",
  states: {
    a: {
      entry: "bar",
    },
  },
});
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.iter().any(|d| {
            d.message_text
                .contains("Type '\"bar\"' is not assignable to type '\"foo\"'")
        }),
        "Expected nested entry to be checked against inferred literal \"foo\", got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_excess_property_display_matches_nested_and_asserted_branches() {
    let diags = check_source_diagnostics(
        r#"
interface WithNestedProp {
  prop: string;
  nested: { prop: string; };
  other: { prop: string; };
}
declare function withNestedProp<T extends WithNestedProp>(props: {[K in keyof T & keyof WithNestedProp]: T[K]}): T;
withNestedProp({prop: "foo", nested: { prop: "bar" }, other: { prop: "baz" }, extra: 10 });

type IsLiteralString<T extends string> = string extends T ? false : true;
interface ProvidedActor {
  src: string;
  logic: () => unknown;
}
type DistributeActors<TActor> = TActor extends { src: infer TSrc } ? { src: TSrc; } : never;
interface MachineConfig<TActor extends ProvidedActor> {
  types?: { actors?: TActor; };
  invoke: IsLiteralString<TActor["src"]> extends true ? DistributeActors<TActor> : { src: string; };
}
declare function createXMachine<
  const TConfig extends MachineConfig<TActor>,
  TActor extends ProvidedActor = TConfig extends { types: { actors: ProvidedActor} } ? TConfig["types"]["actors"] : ProvidedActor,
>(config: {[K in keyof MachineConfig<any> & keyof TConfig]: TConfig[K]}): TConfig;
const child = () => "foo";
createXMachine({
  types: {} as {
    actors: {
      src: "str";
      logic: typeof child;
    };
  },
  invoke: {
    src: "str",
  },
  extra: 10
});
"#,
    );

    let ts2353 = diagnostics_with_code(&diags, 2353);
    assert!(
        ts2353.iter().any(|d| {
            d.message_text.contains(
                "type '{ prop: \"foo\"; nested: { prop: string; }; other: { prop: string; }; }'",
            )
        }),
        "Expected anonymous nested object excess display to preserve top literal and structurally widen nested props, got: {:?}",
        diagnostic_messages(&ts2353)
    );
    assert!(
        ts2353.iter().any(|d| {
            d.message_text.contains(
                "types: { actors: { src: \"str\"; logic: () => string; }; }; invoke: { readonly src: \"str\"; };",
            )
        }),
        "Expected asserted types branch to strip readonly while invoke remains readonly, got: {:?}",
        diagnostic_messages(&ts2353)
    );
}

#[test]
fn ts7006_emitted_for_intra_binding_pattern_reference() {
    // When a destructuring binding element's default references another binding in the
    // same pattern (intra-binding-pattern reference), the contextual type for that
    // property should not flow to the RHS object literal. This matches tsc behavior
    // (TypeScript#59177): `fn2 = fn1` references `fn1` from the same pattern, so the
    // contextual type for `fn2: x => x + 2` is absent and TS7006 fires for `x`.
    let diags = check_source_diagnostics(
        r#"
const { fn1 = (x: number) => 0, fn2 = fn1 } = { fn1: x => x + 1, fn2: x => x + 2 };
"#,
    );
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts7006.len(),
        1,
        "Expected exactly 1 TS7006 for 'x' in fn2's arrow (intra-binding ref), got: {:?}",
        diagnostic_messages(&ts7006)
    );
}

#[test]
fn ts2352_tuple_different_length_assertion() {
    // Same-length tuples with incompatible element types
    let diags = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = x as [number, number];"#,
    );
    assert_eq!(
        diagnostic_count_with_code(&diags, 2352),
        1,
        "Expected TS2352 for [number, string] as [number, number]"
    );

    // Different-length tuples (shorter to longer)
    let diags2 = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = x as [number, string, boolean];"#,
    );
    assert_eq!(
        diagnostic_count_with_code(&diags2, 2352),
        1,
        "Expected TS2352 for [number, string] as [number, string, boolean]"
    );

    // Angle bracket syntax
    let diags3 = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = <[number, string, boolean]>x;"#,
    );
    assert_eq!(
        diagnostic_count_with_code(&diags3, 2352),
        1,
        "Expected TS2352 for <[number, string, boolean]>x"
    );
}

// =============================================================================
// Property access narrowing (this.X after equality checks)
// =============================================================================

#[test]
fn no_false_ts2322_typeof_this_property_after_equality_narrowing() {
    // After `if (this.no === 1)`, both `typeof this.no` and `this.no` in value
    // position should be narrowed to `1`. Without property access narrowing,
    // `typeof this.no` resolves to `1` but `this.no` stays `number`, causing
    // a spurious TS2322: "Type 'number' is not assignable to type '1'".
    let diags = check_source(
        r#"
class Test9 {
    no = 0;

    g() {
        if (this.no === 1) {
            const no: typeof this.no = this.no;
        }
    }
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for `typeof this.no = this.no` inside equality guard, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn no_false_ts2322_typeof_this_property_named_this_after_equality_narrowing() {
    // Same test but for a property literally named `this` — the property access
    // `this.this` should also be narrowed after `if (this.this === 1)`.
    let diags = check_source(
        r#"
class Test9 {
    this = 0;

    g() {
        if (this.this === 1) {
            const no: typeof this.this = this.this;
        }
    }
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for `typeof this.this = this.this` inside equality guard, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn regex_named_groups_emit_target_and_missing_backreference_diagnostics() {
    let diags = check_source(
        r#"
const regex = /(?<foo>)\k<Foo>/;
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&1503),
        "Expected TS1503 for named capture groups under ES2015, got {codes:?}"
    );
    assert!(
        codes.contains(&1532),
        "Expected TS1532 for unknown named backreference, got {codes:?}"
    );
}

#[test]
fn ts2416_interface_class_merge_method_override_incompatible() {
    // When a class and interface share the same name (declaration merging),
    // the derived class override check must see interface members from the base.
    // Here Bar.method returns string | undefined (from optionalProperty?)
    // but interface Foo declares method(a: number): string — TS2416 should fire.
    let diags = check_source_diagnostics(
        r#"
interface Foo {
    method(a: number): string;
    optionalMethod?(a: number): string;
    property: string;
    optionalProperty?: string;
}

class Foo {
    additionalProperty!: string;

    additionalMethod(a: number): string {
        return this.method(0);
    }
}

class Bar extends Foo {
    method(a: number) {
        return this.optionalProperty;
    }
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert_eq!(
        ts2416.len(),
        1,
        "Expected TS2416 for Bar.method incompatible with merged interface Foo.method, got: {:?}",
        diagnostic_messages(&ts2416)
    );
    assert!(
        ts2416[0].message_text.contains("method"),
        "TS2416 should reference the 'method' property, got: {}",
        ts2416[0].message_text
    );
}

#[test]
fn ts2416_interface_class_merge_property_override_incompatible() {
    // Property signatures from merged interfaces should also be visible
    // in the base chain summary. Here Bar.prop is number but interface
    // Foo declares prop: string — TS2416 should fire.
    let diags = check_source_diagnostics(
        r#"
interface Foo {
    prop: string;
}

class Foo {
    extra!: number;
}

class Bar extends Foo {
    prop: number = 42;
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert_eq!(
        ts2416.len(),
        1,
        "Expected TS2416 for Bar.prop incompatible with merged interface Foo.prop, got: {:?}",
        diagnostic_messages(&ts2416)
    );
    assert!(
        ts2416[0].message_text.contains("prop"),
        "TS2416 should reference the 'prop' property, got: {}",
        ts2416[0].message_text
    );
}

#[test]
fn no_false_ts2416_interface_class_merge_compatible_override() {
    // When the derived override IS compatible with the merged interface member,
    // TS2416 should NOT fire.
    let diags = check_source_diagnostics(
        r#"
interface Foo {
    method(a: number): string;
}

class Foo {
    extra!: string;
}

class Bar extends Foo {
    method(a: number): string {
        return "hello";
    }
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert_eq!(
        ts2416.len(),
        0,
        "Expected no TS2416 for compatible override, got: {:?}",
        diagnostic_messages(&ts2416)
    );
}

#[test]
fn ts2416_this_predicate_inheritance_not_suppressed() {
    // Regression for typePredicateInherit.ts: tsc never infers `this is T`
    // predicates from a method body, so a class method without an explicit
    // return type annotation that happens to return `boolean` must NOT be
    // suppressed when the interface (or base class) it satisfies declares a
    // `this is X` predicate. tsc reports TS2416 for each such mismatch.
    let diags = check_source_diagnostics(
        r#"
interface A {
  method1(): this is { a: 1 };
  method2(): boolean;
  method3(): this is { a: 1 };
}
class B implements A {
  method1() { }
  method2() { }
  method3() { return true; }
}
class C {
  method1(): this is { a: 1 } { return true; }
  method3(): this is { a: 1 } { return true; }
}
class D extends C {
  method1(): void { }
  method3(): boolean { return true; }
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    let messages = diagnostic_messages(&ts2416);
    assert_eq!(
        ts2416.len(),
        5,
        "Expected 5 TS2416 (B.method1/2/3 + D.method1/3), got: {messages:?}"
    );
    for name in ["method1", "method2", "method3"] {
        assert!(
            ts2416
                .iter()
                .any(|d| d.message_text.contains(&format!("Property '{name}'"))),
            "Expected TS2416 mentioning Property '{name}', got: {messages:?}"
        );
    }
}

#[test]
fn ts2352_string_enum_comparable_in_nested_assertion() {
    // Repro from comparableRelationBidirectional.ts:
    // When asserting an object literal `as UserSettings` where a nested property
    // has a string enum type, the comparable relation should recognize overlap
    // between the string literal `""` and the string enum `AutomationMode` (which
    // has NONE = ""). TS2352 should NOT fire because the types overlap at the
    // property level even though direct assignability fails (string enums are
    // nominally strict for assignments but comparable for type assertions).
    let diags = check_source_diagnostics(
        r#"
enum AutomationMode {
    NONE = "",
    TIME = "time",
    SYSTEM = "system",
    LOCATION = "location",
}
interface Automation {
    mode: AutomationMode;
}
interface UserSettings {
    presets: string[];
    automation: Automation;
}
const x = {
    presets: [],
    automation: {
        mode: "",
    },
} as UserSettings;
"#,
    );
    let ts2352 = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        ts2352.len(),
        0,
        "Expected no TS2352 for string enum comparable assertion, got: {:?}",
        diagnostic_messages(&ts2352)
    );
}

#[test]
fn unknown_array_destructuring_ts2571_anchors_only_empty_pattern() {
    let source = r#"
declare function f<T>(): T;
const [] = f();
const [e1, e2] = f();
"#;
    let diags = check_source_diagnostics(source);

    let ts2571 = diagnostics_with_code(&diags, 2571);
    assert_eq!(
        ts2571.len(),
        1,
        "Expected exactly one TS2571 for unknown array destructuring, got: {:?}",
        diagnostic_code_starts(&diags)
    );

    let empty_start = source.find("[]").expect("expected empty array pattern") as u32;
    assert_eq!(
        ts2571[0].start, empty_start,
        "TS2571 should anchor at the empty array pattern"
    );

    let ts2488 = diagnostics_with_code(&diags, 2488);
    assert_eq!(
        ts2488.len(),
        2,
        "Expected TS2488 on both unknown array destructuring patterns, got: {:?}",
        diagnostic_code_starts(&diags)
    );
}

#[test]
fn catch_array_destructuring_unknown_suppresses_ts2571() {
    let diags = check_source_diagnostics(
        r#"
try {} catch ([x]) {}
"#,
    );

    let ts2571 = diagnostics_with_code(&diags, 2571);
    assert_eq!(
        ts2571.len(),
        0,
        "Expected no TS2571 for catch-clause array destructuring, got: {:?}",
        diagnostic_code_starts(&diags)
    );
    let ts2488 = diagnostics_with_code(&diags, 2488);
    assert_eq!(
        ts2488.len(),
        1,
        "Expected TS2488 for catch-clause array destructuring, got: {:?}",
        diagnostic_code_starts(&diags)
    );
}

#[test]
fn interface_with_construct_signature_no_ts2351() {
    // An interface with a construct signature (like ProxyConstructor) should
    // be constructable via `new` without TS2351.
    let diags = check_source_diagnostics(
        r#"
interface MyHandler<T extends object> {
    get?(target: T, p: string): any;
}
interface MyConstructor {
    new <T extends object>(target: T, handler: MyHandler<T>): T;
}
declare var MyProxy: MyConstructor;
var t: object = {};
var p = new MyProxy(t, {});
"#,
    );
    let ts2351 = diagnostics_with_code(&diags, 2351);
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for interface with construct signature, got: {:?}",
        diagnostic_messages(&ts2351)
    );
}

#[test]
fn no_false_ts2339_on_generic_class_self_referencing_parameter() {
    // Regression test: property access on a generic class type used as a
    // parameter type within the same class's method should not produce false
    // TS2339 errors. The class instance type cache must not be corrupted by
    // ERROR values during re-entrant class checking.
    //
    // Matches tsc behavior for genericClasses4.ts: no errors expected.
    let diags = check_source_diagnostics(
        r#"
class Vec2_T<A> {
    constructor(public x: A, public y: A) { }
    fmap<B>(f: (a: A) => B): Vec2_T<B> {
        var x:B = f(this.x);
        var y:B = f(this.y);
        var retval: Vec2_T<B> = new Vec2_T(x, y);
        return retval;
    }
    apply<B>(f: Vec2_T<(a: A) => B>): Vec2_T<B> {
        var x:B = f.x(this.x);
        var y:B = f.y(this.y);
        var retval: Vec2_T<B> = new Vec2_T(x, y);
        return retval;
    }
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on generic class self-reference, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn no_false_ts2339_on_class_param_with_same_class_type() {
    // A method that takes a parameter of the same class type should be able to
    // access properties on that parameter, even when another method returns
    // the same class type (triggering class instance type cache invalidation).
    let diags = check_source_diagnostics(
        r#"
class Foo<A> {
    constructor(public x: A) {}
    bar(): Foo<any> { return this; }
    test(f: Foo<string>): void {
        let v = f.x;
    }
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for f.x where f: Foo<string>, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn no_false_ts2339_on_self_cast_in_generic_class_property_initializer() {
    let diags = check_source_diagnostics(
        r#"
class Bar<T> {
    num!: number;
    Field: number = (this as Bar<any>).num;
    Value = (this as Bar<any>).num;
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for self-cast property initializer, got: {:?}",
        diagnostic_messages(&ts2339)
    );

    let missing_diags = check_source_diagnostics(
        r#"
class Bar<T> {
    Value = (this as Bar<any>).missing;
}
"#,
    );
    assert!(
        missing_diags.iter().any(|d| d.code == 2339),
        "Expected TS2339 for genuinely missing self-cast member, got: {:?}",
        diagnostic_summaries(&missing_diags)
    );
}

#[test]
fn getter_returning_this_no_false_ts2339() {
    // When a class getter returns `this` without an explicit type annotation,
    // the inferred return type must be the polymorphic `ThisType` — not the
    // partial class instance type. Without the syntactic `returns_only_this`
    // fallback, return-type widening (ObjectWithIndex → Object) can produce
    // a TypeId mismatch, causing the getter property to be omitted from the
    // final class instance type and triggering false TS2339 errors.
    let diags = check_source_diagnostics(
        r#"
class C<T> {
    constructor() {}
    get y() { return this; }
    z: T;
}
declare var c: C<string>;
var r = c.y;
r.y;
r.z;
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for getter returning this, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn no_false_ts2339_for_getter_this_type_after_constructor() {
    // Getter returning `this` declared after constructor should not produce
    // false TS2339 when the getter's return type is accessed through a variable.
    // Previously, the cached_instance_this_type in enclosing_class was stale
    // (set to the Phase 0 prescan type), causing `this` in the getter body to
    // resolve to a partial type missing the getter property itself.
    let diags = check_source_diagnostics(
        r#"
class C<T> {
    x = this;
    constructor(x: T) {}
    get y() { return this; }
    z: T;
}

declare var c: C<string>;
var r2 = c.y;
r2.y;
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for r2.y where r2 = c.y, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn getter_returning_this_after_constructor_resolves_to_this_type() {
    // When a getter that returns `this` is declared after the constructor,
    // the inferred return type might not match the Phase 3 partial type by
    // TypeId equality. The syntactic `method_body_returns_only_this` fallback
    // ensures the getter still gets polymorphic `ThisType`, so that accessing
    // getter properties on the result works correctly.
    let diags = check_source_diagnostics(
        r#"
class C<T> {
    foo() { return this; }
    constructor(x: T) {
        this.z = x;
    }
    get y() { return this; }
    z: T;
}

var c: C<string> = new C("hello");
// Getter result should have all class members including y itself
var result = c.y;
result.y;
result.foo;
result.z;

// Method result should also have getter y
var r2 = c.foo();
r2.y;
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for getter `this` return type on class with getter after constructor, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn enum_in_namespace_typeof_property_access() {
    // When accessing an enum export through a typeof namespace variable,
    // the enum should resolve to its namespace type (with member properties)
    // not the enum instance type (the union of enum values).
    // This is the pattern from conformance test `instantiatedModule.ts`.
    let diags = check_source_diagnostics(
        r#"
namespace M3 {
    export enum Color { Blue, Red }
}
var m3: typeof M3;
var m3 = M3;
var a3: typeof M3.Color;
var a3 = m3.Color;
var a3 = M3.Color;
var blue: M3.Color = a3.Blue;
var p3: M3.Color;
var p3 = M3.Color.Red;
var p3 = m3.Color.Blue;
"#,
    );
    // TS2339: Property does not exist on type
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for enum member access through typeof namespace, got: {:?}",
        diagnostic_messages(&ts2339)
    );
    // TS2403: Subsequent variable declarations must have the same type
    let ts2403 = diagnostics_with_code(&diags, 2403);
    assert_eq!(
        ts2403.len(),
        0,
        "Expected no TS2403 for enum typeof mismatch, got: {:?}",
        diagnostic_messages(&ts2403)
    );
}

#[test]
fn ts2345_readonly_array_preserves_readonly_in_message() {
    // When a readonly array is passed where a mutable array is expected,
    // the TS2345 message should display 'readonly number[]' not 'number[]'.
    let diags = check_source_diagnostics(
        r#"
declare const a: readonly number[];
declare function fn(x: number[]): void;
fn(a);
"#,
    );
    let matching = diagnostics_with_code(&diags, 2345);
    assert_eq!(matching.len(), 1, "Expected one TS2345, got: {diags:?}");

    let msg = &matching[0].message_text;
    assert!(
        msg.contains("'readonly number[]'"),
        "Expected 'readonly number[]' in TS2345 message, got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'number[]'"),
        "Expected 'number[]' as target type, got: {msg}"
    );
}

#[test]
fn no_ts2339_for_computed_property_with_circular_class_reference() {
    let diags = check_source_diagnostics(
        r#"
declare const rC: RC<"a">;
rC.x;
declare class RC<T extends "a" | "b"> {
    x: T;
    [rC.x]: "b";
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on class with circular computed property, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn satisfies_preserves_literal_type_for_direct_literal() {
    // `1 satisfies number` should have type `1` (preserved), not `number` (widened).
    // tsc: `checkSatisfiesExpressionWorker` calls `checkExpression` which returns
    // fresh literal types from `checkNumericLiteral` regardless of contextual type.
    // Assignment to a literal target `true` then shows source `'1'`, not `'number'`.
    let diags = check_source_diagnostics(
        r#"
const a: true = 1 satisfies number;
const b: true = "foo" satisfies string;
const c: 2 = 1 satisfies number;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        3,
        "Expected 3 TS2322 errors for satisfies literal assignments, got: {:?}",
        diagnostic_messages(&ts2322)
    );
    // All three should preserve the source literal in the diagnostic (not widen).
    assert!(
        ts2322[0].message_text.contains("Type '1'"),
        "Expected `Type '1'` preserved for `1 satisfies number`, got: {}",
        ts2322[0].message_text
    );
    assert!(
        ts2322[1].message_text.contains("Type '\"foo\"'"),
        "Expected `Type '\"foo\"'` preserved for `\"foo\" satisfies string`, got: {}",
        ts2322[1].message_text
    );
    assert!(
        ts2322[2].message_text.contains("Type '1'"),
        "Expected `Type '1'` preserved for `1 satisfies number` assigned to `2`, got: {}",
        ts2322[2].message_text
    );
}

#[test]
fn satisfies_widens_source_for_ts1360_when_target_is_primitive() {
    // For TS1360 (`Type X does not satisfy the expected type Y`), when Y is not a
    // literal-sensitive type (e.g. `boolean`, `number`), tsc widens a bare literal
    // source for display: `Type 'number' does not satisfy the expected type 'boolean'.`
    // This preserves our existing match with tsc even though the internal type
    // of `1 satisfies boolean` is now `1` (preserved literal) rather than `number`.
    let diags = check_source_diagnostics(
        r#"
const x = 1 satisfies boolean;
"#,
    );
    let ts1360 = diagnostics_with_code(&diags, 1360);
    assert_eq!(
        ts1360.len(),
        1,
        "Expected 1 TS1360 error for `1 satisfies boolean`, got: {:?}",
        diagnostic_messages(&ts1360)
    );
    assert!(
        ts1360[0].message_text.contains("Type 'number'"),
        "Expected source widened to 'number' in TS1360 message (target is non-literal `boolean`), got: {}",
        ts1360[0].message_text
    );
    assert!(
        ts1360[0].message_text.contains("'boolean'"),
        "Expected target `boolean` in TS1360 message, got: {}",
        ts1360[0].message_text
    );
}

#[test]
fn satisfies_array_literal_elaborates_per_element() {
    // `[10, "20"] satisfies number[]` should elaborate per-element rather than
    // emitting a generic TS1360 on the whole expression. tsc emits TS2322 at
    // the offending `"20"` element with `Type 'string' is not assignable to
    // type 'number'.`, matching its `elaborateElementwise` behavior.
    //
    // Iteration variable / property names are deliberately varied across
    // assertions to avoid fingerprinting a specific spelling — the rule is
    // structural over array literal sources, not over specific identifiers.
    let diags = check_source_diagnostics(
        r#"
declare function take(...args: unknown[]): void;
take(10, ...([10, "20"] satisfies number[]));
take(10, ...([1, 2, "x", 4] satisfies number[]));
take(10, ...(([1, "wrapped"]) satisfies number[]));
take(10, ...(([1, "asserted"] as (number | string)[]) satisfies number[]));
"#,
    );

    // First satisfies has one bad element: "20" (string).
    // Second satisfies has one bad element: "x" (string).
    // The wrapped cases prove source unwrapping reaches the same array-literal
    // element path for parenthesized and asserted array sources.
    // Each source should emit TS2322 at the bad element, NOT TS1360 on the whole satisfies.
    let ts2322 = diagnostics_with_code(&diags, 2322);
    let ts1360 = diagnostics_with_code(&diags, 1360);

    assert_eq!(
        ts1360.len(),
        0,
        "Expected NO TS1360 generic-satisfies error; expected per-element TS2322 instead, got TS1360s: {:?}",
        diagnostic_messages(&ts1360)
    );
    assert_eq!(
        ts2322.len(),
        4,
        "Expected exactly 4 TS2322 elaborations (one per bad element), got: {:?}",
        diagnostic_messages(&ts2322)
    );
    for diag in &ts2322 {
        assert!(
            diag.message_text.contains("'string'") && diag.message_text.contains("'number'"),
            "Expected TS2322 message about string -> number, got: {}",
            diag.message_text
        );
    }
}

#[test]
fn satisfies_array_literal_all_elements_compatible_no_diagnostic() {
    // Sanity check: when every element of an array literal satisfies the
    // target's element type, no diagnostic should be reported. This guards
    // against the new array-elaboration path firing on assignable sources.
    let diags = check_source_diagnostics(
        r#"
declare function take(...args: unknown[]): void;
take(10, ...([1, 2, 3] satisfies number[]));
"#,
    );
    assert_eq!(
        diags.len(),
        0,
        "Expected no diagnostics for fully-compatible array literal, got: {:?}",
        diagnostic_summaries(&diags)
    );
}

#[test]
fn satisfies_result_type_is_assignable_to_target_literal_union() {
    // `"A" satisfies string` should have type `"A"` so it remains assignable to
    // a parameter of type `"A" | "B"`. Widening to `string` (the previous
    // behavior) would produce a false TS2345.
    let diags = check_source_diagnostics(
        r#"
declare function fn(s: "A" | "B"): void;
fn("A" satisfies string);
fn("C" satisfies string);
"#,
    );
    // First call should succeed; second should fail with TS2345 (string literal
    // "C" is not assignable to "A" | "B").
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "Expected exactly 1 TS2345 for the `\"C\"` call (not the `\"A\"` call), got: {:?}",
        diagnostic_messages(&ts2345)
    );
}

#[test]
fn ts2322_nested_generic_alias_two_levels() {
    // Box<Box<number>> should not be assignable to Box<Box<string>>
    let diags = check_source_diagnostics(
        r#"
type Box<T> = { value: T };
declare const x: Box<Box<number>>;
declare let y: Box<Box<string>>;
y = x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for Box<Box<number>> vs Box<Box<string>>, got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn ts2322_nested_fn_alias_four_levels() {
    // Cb<Cb<Cb<Cb<number>>>> should not be assignable to Cb<Cb<Cb<Cb<string>>>>
    // where Cb<T> = {noAlias: () => T}["noAlias"]
    let diags = check_source_diagnostics(
        r#"
type Cb<T> = {noAlias: () => T}["noAlias"];
declare const x: Cb<Cb<Cb<Cb<number>>>>;
declare let y: Cb<Cb<Cb<Cb<string>>>>;
y = x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for Cb<Cb<Cb<Cb<number>>>> vs Cb<Cb<Cb<Cb<string>>>>, got: {:?}",
        diagnostic_codes(&diags)
    );
    // Both source and target must be shown in structurally-expanded form.
    // tsc does not preserve alias names when the alias body is an IndexedAccess type.
    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("() => () => () => () => number"),
        "Expected source to expand to '() => () => () => () => number', got: {msg}"
    );
    assert!(
        msg.contains("() => () => () => () => string"),
        "Expected target to expand to '() => () => () => () => string', got: {msg}"
    );
}

// Regression: a property-name identifier that happens to share a name with the
// enclosing variable must not be treated as a self-reference for TS7023.
//
// Rule: when a function-like initializer scans its body for self-references
// to detect circular return-type inference, identifiers in non-value name
// positions (property access RHS, qualified-name RHS, property/method/accessor
// names) are property keys, not lexical references — they must not match
// the enclosing variable's symbol.
#[test]
fn ts7023_no_false_positive_on_property_name_collision_assign() {
    // `Object.assign` inside an arrow body is a property name on the right of
    // a property access. The lexical `assign` variable is not referenced.
    let diags = check_source_diagnostics(
        r#"
const assign = <T, U>(a: T, b: U) => Object.assign(a, b);
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 for property-name collision with enclosing variable, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts7023_no_false_positive_on_property_name_collision_alt_name() {
    // Same rule with a different variable name to prove the fix is structural,
    // not name-specific.
    let diags = check_source_diagnostics(
        r#"
const merge = <T, U>(a: T, b: U) => Object.merge(a, b);
declare namespace Object { function merge<A, B>(a: A, b: B): A & B; }
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 for `merge` colliding with property name, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts7023_still_fires_on_genuine_self_reference() {
    // Sanity: a real recursive call inside a function-like initializer
    // without a return type annotation must still produce TS7023.
    let diags = check_source_diagnostics(
        r#"
const recur = (n: number) => recur(n);
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert_eq!(
        ts7023.len(),
        1,
        "Expected TS7023 for genuine recursive arrow without return annotation, got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn ts7022_ts7023_do_not_fire_for_void_expression_return_operand() {
    let diags = check_source_diagnostics(
        r#"
type HowlErrorCallback = (soundId: number, error: unknown) => void;

interface HowlOptions {
  onplayerror?: HowlErrorCallback | undefined;
}

class Howl {
  constructor(public readonly options: HowlOptions) {}
  once(name: "unlock", fn: () => void) {
    console.log(name, fn);
  }
}

const instance = new Howl({
  onplayerror: () => void instance.once("unlock", () => {}),
});
"#,
    );
    let circularity: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7022 | 7023))
        .collect();
    assert!(
        circularity.is_empty(),
        "Expected no TS7022/TS7023 for self-reference under void return expression, got: {:?}",
        diagnostic_ref_summaries(&circularity)
    );
}

#[test]
fn ts7023_no_false_positive_when_property_key_matches_outer_var() {
    // The key in an object literal (also a non-value name position) must not
    // be treated as a lexical reference to a same-named outer variable.
    let diags = check_source_diagnostics(
        r#"
const wrap = (x: number) => ({ wrap: x });
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 when an object property key matches the enclosing variable name, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts2322_no_false_positive_merged_type_alias_and_const_return() {
    // Two name variants guard against name-hardcoding regressions (§25).
    for source in [
        r#"
type Foo = { type: "foo" };
const Foo = {
  make: (): Foo => {
    return { type: "foo" };
  }
};
"#,
        r#"
type MyAlias = { kind: "ok" };
const MyAlias = {
  build: (): MyAlias => {
    return { kind: "ok" };
  }
};
"#,
    ] {
        let diags = check_source_diagnostics(source);
        let ts2322 = diagnostics_with_code(&diags, 2322);
        assert!(
            ts2322.is_empty(),
            "Expected no TS2322 for merged type-alias+const return, got: {:?}",
            diagnostic_messages(&ts2322)
        );
    }
}

#[test]
fn ts2322_real_error_still_reported_for_merged_type_alias_and_const_wrong_return() {
    let diags = check_source_diagnostics(
        r#"
type Status = { code: "ok" };
const Status = {
  make: (): Status => {
    return { code: "wrong" };
  }
};
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for wrong literal in merged type-alias+const return, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}
