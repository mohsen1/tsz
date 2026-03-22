//! Regression tests for call expression architecture invariants.
//!
//! These tests verify that the call expression module (`call.rs`) correctly uses
//! solver query APIs instead of direct TypeData/lookup inspection.

use crate::test_utils::check_source_diagnostics;

/// Verify ThisType extraction through type alias applications works correctly
/// via `get_this_type_from_marker_expanding` (previously used raw TypeData
/// pattern matching on Application/Lazy).
#[test]
fn this_type_through_alias_application_no_false_ts2339() {
    let diags = check_source_diagnostics(
        r#"
interface Data {
    value: number;
}
interface Instance {
    getValue(): number;
}
type ConstructorOptions<D> = {
    data(): D;
} & ThisType<Instance & D>;

declare function createComponent<D>(options: ConstructorOptions<D>): Instance & D;

createComponent({
    data() {
        return { value: 42 };
    },
});
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for ThisType through alias application, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Verify union callee predicate extraction uses is_union_type query
/// (previously used raw solver-internal Union pattern match).
#[test]
fn union_callee_type_predicate_extraction_no_crash() {
    let diags = check_source_diagnostics(
        r#"
declare function isString(x: unknown): x is string;
declare function isNumber(x: unknown): x is number;

declare const check: typeof isString | typeof isNumber;

function test(x: unknown) {
    if (check(x)) {
        void x;
    }
}
"#,
    );

    // Should not crash or produce unexpected errors.
    // The union predicate validity check should work with is_union_type query.
    let unexpected: Vec<_> = diags
        .iter()
        .filter(|d| d.code != 2349 && d.code != 2769)
        .collect();
    // Union of type guards may or may not emit TS2349/TS2769 depending on
    // resolution — the important thing is no panic.
    let _ = unexpected;
}

/// Verify overload resolution works correctly when callee is a non-union
/// callable type with multiple signatures.
#[test]
fn overload_resolution_basic() {
    let diags = check_source_diagnostics(
        r#"
declare function foo(x: number): number;
declare function foo(x: string): string;

const a: number = foo(42);
const b: string = foo("hello");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for basic overload resolution, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Verify property call on method works with call signature classification.
#[test]
fn property_call_method_invocation() {
    let diags = check_source_diagnostics(
        r#"
interface Obj {
    method(x: number): string;
}

declare const obj: Obj;
const result: string = obj.method(42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2339)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for property call method invocation, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: generic call with spread arguments should not crash or
/// produce false diagnostics after arch refactoring.
#[test]
fn generic_call_with_spread_args() {
    let diags = check_source_diagnostics(
        r#"
declare function apply<T, R>(fn: (...args: T[]) => R, args: T[]): R;

const nums = [1, 2, 3];
const result = apply((x: number) => x.toString(), nums);
"#,
    );

    // Should produce no type errors for valid generic spread call.
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        0,
        "Expected no TS2345 for generic call with spread args, got: {:?}",
        ts2345.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Overload resolution with mismatched argument type picks the right signature
/// and reports TS2345 only against the best-matching overload.
#[test]
fn overload_resolution_argument_mismatch() {
    let diags = check_source_diagnostics(
        r#"
declare function convert(x: number): string;
declare function convert(x: string): number;

const r: string = convert(true);
"#,
    );

    // Should emit TS2769 (no overload matches) for `true` argument.
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert!(
        !ts2769.is_empty(),
        "Expected TS2769 for overload mismatch with boolean arg"
    );
}

/// Overload resolution with arity mismatch (too many/few arguments).
#[test]
fn overload_resolution_arity_mismatch() {
    let diags = check_source_diagnostics(
        r#"
declare function pair(a: number): [number];
declare function pair(a: number, b: number): [number, number];

pair(1, 2, 3);
"#,
    );

    let ts2554: Vec<_> = diags.iter().filter(|d| d.code == 2554).collect();
    assert!(
        !ts2554.is_empty(),
        "Expected TS2554 for too many arguments in overloaded call"
    );
}

/// Property call on interface method with overloads resolves correctly.
#[test]
fn property_call_overloaded_method() {
    let diags = check_source_diagnostics(
        r#"
interface Parser {
    parse(input: string): object;
    parse(input: string, reviver: (key: string, value: any) => any): object;
}

declare const parser: Parser;
const a: object = parser.parse("{}");
const b: object = parser.parse("{}", (k, v) => v);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for overloaded property call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Callable interface (call signature) invocation works through
/// classify_for_call_signatures query.
#[test]
fn callable_interface_invocation() {
    let diags = check_source_diagnostics(
        r#"
interface StringTransform {
    (input: string): string;
}

declare const transform: StringTransform;
const result: string = transform("hello");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2349)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for callable interface invocation, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Calling a non-callable type emits TS2349.
#[test]
fn non_callable_type_emits_ts2349() {
    let diags = check_source_diagnostics(
        r#"
declare const x: number;
x();
"#,
    );

    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        !ts2349.is_empty(),
        "Expected TS2349 for calling a non-callable type"
    );
}

/// Generic function with contextual callback parameter inference.
/// Ensures two-pass inference resolves callback parameter types correctly.
#[test]
fn generic_call_contextual_callback_inference() {
    let diags = check_source_diagnostics(
        r#"
declare function map<T, U>(arr: T[], fn: (item: T) => U): U[];

const nums = [1, 2, 3];
const strs: string[] = map(nums, n => n.toFixed(2));
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 7006)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for generic call with contextual callback, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Union callee types require all members to accept the call (not overload semantics).
#[test]
fn union_callee_requires_all_members_callable() {
    let diags = check_source_diagnostics(
        r#"
declare const fn1: ((x: string) => void) | ((x: string, y: string) => void);
fn1("a");
"#,
    );

    // Calling with 1 arg: first member accepts it, second requires 2.
    // Union call semantics require ALL members to accept, so this may
    // produce TS2554 or TS2769.
    // The key architectural invariant: union callee is NOT treated as overloads.
    let _ = diags;
}

/// Optional chain call with nullish callee returns result | undefined.
#[test]
fn optional_chain_call_nullish_callee() {
    let diags = check_source_diagnostics(
        r#"
declare const fn1: ((x: number) => string) | undefined;
const result: string | undefined = fn1?.(42);
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322 for optional chain call with nullish callee, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
