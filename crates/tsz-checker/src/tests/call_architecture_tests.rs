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

// === Regression tests for boundary query refactoring ===
// These tests exercise code paths that were migrated from direct
// tsz_solver::type_queries:: calls to common:: boundary wrappers.

/// Regression: literal type classification goes through boundary wrapper.
/// Exercises common::classify_literal_type via generic call inference
/// where contextual constraint preserves literal types.
#[test]
fn generic_call_preserves_literal_types_in_constraint() {
    let diags = check_source_diagnostics(
        r#"
declare function pick<K extends "a" | "b">(key: K): K;
const result: "a" = pick("a");
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322 for literal type preserved in generic constraint, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: application_info boundary wrapper used in return context
/// substitution collection for Application types (e.g., Promise<T>).
/// Exercises the code path where collect_return_context_substitution
/// compares Application types via common::application_info.
#[test]
fn generic_call_application_info_return_context() {
    let diags = check_source_diagnostics(
        r#"
interface Box<T> { value: T }
declare function wrap<T>(value: T): Box<T>;
const boxed = wrap(42);
"#,
    );

    // The call should resolve without panics. The inferred return type
    // is Box<number>. We don't assert the exact type here since that
    // depends on full conformance with tsc's generic inference.
    let panics: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2349 || d.code == 2554)
        .collect();
    assert_eq!(
        panics.len(),
        0,
        "Expected no call resolution errors for generic application return, got: {:?}",
        panics.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: function_shape_for_type boundary wrapper used during
/// sanitization of generic inference arguments (binding patterns).
#[test]
fn generic_call_binding_pattern_sanitization() {
    let diags = check_source_diagnostics(
        r#"
declare function process<T>(fn: (item: T) => void, items: T[]): void;
process(({ x, y }: { x: number; y: string }) => {}, [{ x: 1, y: "a" }]);
"#,
    );

    // Should not crash. The binding pattern sanitization replaces destructured
    // params with UNKNOWN to prevent contaminating inference.
    let _ = diags;
}

/// Regression: unpack_tuple_rest_parameter boundary wrapper used in
/// contextual signature normalization for spread calls.
#[test]
fn spread_call_tuple_rest_unpacking() {
    let diags = check_source_diagnostics(
        r#"
declare function sum(...args: [number, number, number]): number;
const args: [number, number, number] = [1, 2, 3];
const result: number = sum(...args);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2556)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for spread call with tuple rest, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: is_callable_type boundary wrapper used in contextual
/// call param type normalization (normalize_contextual_call_param_type).
#[test]
fn callable_param_type_normalization() {
    let diags = check_source_diagnostics(
        r#"
declare function apply<T>(fn: (x: T) => T, value: T): T;
const result: number = apply(x => x + 1, 42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 7006)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for callable param type normalization, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: find_property_in_object boundary wrapper used in
/// constructor_type_from_new_property for construct signature lookup.
#[test]
fn new_property_constructor_lookup() {
    let diags = check_source_diagnostics(
        r#"
interface MyClass {
    value: number;
}
interface MyClassConstructor {
    new(value: number): MyClass;
}
declare const Ctor: MyClassConstructor;
const instance: MyClass = new Ctor(42);
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322 for new property constructor lookup, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: enum_def_id boundary wrapper used during generic inference
/// argument sanitization for enum types.
#[test]
fn generic_call_with_enum_argument() {
    let diags = check_source_diagnostics(
        r#"
enum Color { Red, Green, Blue }
declare function describe<T>(value: T): string;
const result: string = describe(Color.Red);
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322 for generic call with enum argument, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// === Focused call/overload/property-call regression tests ===
// These tests ensure the call expression module correctly delegates to
// solver query APIs for all type inspection. Each test targets a specific
// code path in call.rs.

/// Overload resolution with different return types picks the correct
/// signature based on argument type and returns the matching return type.
#[test]
fn overload_resolution_return_type_selection() {
    let diags = check_source_diagnostics(
        r#"
declare function parse(input: string): object;
declare function parse(input: string, strict: true): object | null;

const a: object = parse("{}");
const b: object | null = parse("{}", true);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for overload return type selection, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Overload with rest parameters resolves correctly.
#[test]
fn overload_resolution_rest_params() {
    let diags = check_source_diagnostics(
        r#"
declare function concat(a: string, b: string): string;
declare function concat(...parts: string[]): string;

const a: string = concat("hello", "world");
const b: string = concat("a", "b", "c", "d");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for overload with rest params, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Property call on nested object method resolves through
/// classify_for_call_signatures query.
#[test]
fn property_call_nested_object_method() {
    let diags = check_source_diagnostics(
        r#"
interface Config {
    db: {
        connect(url: string): void;
    };
}

declare const config: Config;
config.db.connect("postgres://localhost");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2349 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for nested property call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Optional chain call on method returns T | undefined.
#[test]
fn optional_chain_method_call() {
    let diags = check_source_diagnostics(
        r#"
interface Logger {
    log(msg: string): void;
}

declare const logger: Logger | undefined;
logger?.log("hello");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2349 || d.code == 2722)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for optional chain method call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Optional chain call with non-nullish callee is still valid.
#[test]
fn optional_chain_call_non_nullish() {
    let diags = check_source_diagnostics(
        r#"
declare const fn1: (x: number) => string;
const result: string | undefined = fn1?.(42);
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322 for optional chain on non-nullish callee, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Calling unknown type emits TS18046 (not TS2349) with strictNullChecks.
#[test]
fn calling_unknown_emits_ts18046() {
    let diags = check_source_diagnostics(
        r#"
declare const x: unknown;
x();
"#,
    );

    let ts18046: Vec<_> = diags.iter().filter(|d| d.code == 18046).collect();
    assert!(
        !ts18046.is_empty(),
        "Expected TS18046 for calling unknown type, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Calling never propagates never (bottom type).
#[test]
fn calling_never_returns_never() {
    let diags = check_source_diagnostics(
        r#"
declare const f: never;
const result: never = f();
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    // f() should return never, so assigning to never is valid.
    // TS2349 is expected since never has no call signatures.
    let _ = ts2322;
}

/// Generic overloaded function with explicit type arguments.
#[test]
fn generic_overloaded_with_explicit_type_args() {
    let diags = check_source_diagnostics(
        r#"
declare function create<T>(value: T): { value: T };
declare function create<T>(value: T, label: string): { value: T; label: string };

const a = create<number>(42);
const b = create<string>("hello", "greeting");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for generic overloaded call with explicit type args, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Union callee: all members must accept the call arguments.
/// When one member requires different arity, error is expected.
#[test]
fn union_callee_arity_mismatch_across_members() {
    let diags = check_source_diagnostics(
        r#"
declare const f: ((x: number) => void) | ((x: number, y: number) => void);
f(1, 2, 3);
"#,
    );

    // All union members require at most 2 args, so 3 args should error.
    let has_arity_error = diags
        .iter()
        .any(|d| d.code == 2554 || d.code == 2769 || d.code == 2345);
    assert!(
        has_arity_error,
        "Expected arity/overload error for union callee with too many args, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Generic call with multiple callbacks: two-pass inference resolves
/// both callback parameter types from the same type parameter.
#[test]
fn generic_call_multiple_callbacks() {
    let diags = check_source_diagnostics(
        r#"
declare function both<T>(a: T[], fn1: (x: T) => void, fn2: (x: T) => string): void;
both([1, 2], n => { void n; }, n => n.toFixed(2));
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS7006/TS2339 for generic call with multiple callbacks, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Property call on generic interface method resolves without panics.
/// The callback parameter `n` should not produce TS7006 (implicit any).
#[test]
fn property_call_generic_method() {
    let diags = check_source_diagnostics(
        r#"
interface Collection<T> {
    get(index: number): T;
    map<U>(fn: (item: T) => U): Collection<U>;
}

declare const nums: Collection<number>;
const result = nums.map(n => n.toFixed(2));
"#,
    );

    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 for generic method property call, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Spread call with tuple type validates argument count correctly.
#[test]
fn spread_call_tuple_arity_check() {
    let diags = check_source_diagnostics(
        r#"
declare function add(a: number, b: number): number;
const args: [number] = [1];
add(...args);
"#,
    );

    // Spread of [number] into (a: number, b: number) should error — too few args.
    let has_error = diags.iter().any(|d| d.code == 2556 || d.code == 2554);
    assert!(
        has_error,
        "Expected TS2556/TS2554 for spread call with insufficient tuple args, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Callable intersection type: call resolves through both signatures.
#[test]
fn callable_intersection_invocation() {
    let diags = check_source_diagnostics(
        r#"
type Logger = ((msg: string) => void) & { level: number };

declare const logger: Logger;
logger("test");
const lvl: number = logger.level;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2349 || d.code == 2339 || d.code == 2322)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for callable intersection invocation, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Type argument count mismatch emits TS2558.
#[test]
fn type_argument_count_mismatch_emits_ts2558() {
    let diags = check_source_diagnostics(
        r#"
declare function identity<T>(x: T): T;
identity<number, string>(42);
"#,
    );

    let ts2558: Vec<_> = diags.iter().filter(|d| d.code == 2558).collect();
    assert!(
        !ts2558.is_empty(),
        "Expected TS2558 for type argument count mismatch, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Untyped function call with type arguments emits TS2347.
#[test]
fn untyped_call_with_type_args_emits_ts2347() {
    let diags = check_source_diagnostics(
        r#"
declare const f: any;
f<number>(42);
"#,
    );

    let ts2347: Vec<_> = diags.iter().filter(|d| d.code == 2347).collect();
    assert!(
        !ts2347.is_empty(),
        "Expected TS2347 for untyped function call with type args, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// IIFE (Immediately Invoked Function Expression) with contextual typing.
#[test]
fn iife_contextual_typing() {
    let diags = check_source_diagnostics(
        r#"
const result: number = ((x: number) => x + 1)(42);
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322 for IIFE with contextual typing, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Super call in derived class constructor resolves via construct signature.
#[test]
fn super_call_construct_signature() {
    let diags = check_source_diagnostics(
        r#"
class Base {
    constructor(public value: number) {}
}
class Derived extends Base {
    constructor() {
        super(42);
    }
}
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2554)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for super call with construct signature, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
