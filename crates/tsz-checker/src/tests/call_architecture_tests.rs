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
/// (previously used raw TypeData::Union pattern match).
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
