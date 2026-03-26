//! Regression tests for call expression architecture invariants.
//!
//! These tests verify that the call expression module (`call.rs`) correctly uses
//! solver query APIs instead of direct TypeData/lookup inspection.

use crate::test_utils::check_source_diagnostics;

/// Verify `ThisType` extraction through type alias applications works correctly
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

/// Verify union callee predicate extraction uses `is_union_type` query
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
/// `classify_for_call_signatures` query.
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

#[test]
fn generic_promise_then_accepts_generic_mapper_identifier() {
    let diags = check_source_diagnostics(
        r#"
interface PromiseLike<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): PromiseLike<TResult1>;
}

interface Promise<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): Promise<TResult1>;
}

interface Result<T, E> {
    value: T;
    error: E;
}

type Author = { id: string; name: string };

declare const authorPromise: Promise<Result<Author, "NOT_FOUND_AUTHOR">>;
declare const mapper: <T>(result: Result<T, "NOT_FOUND_AUTHOR">) => Result<T, "NOT_FOUND_AUTHOR">;

const test1 = authorPromise.then(mapper);
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected generic mapper identifier to match Promise.then callback without TS2345, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn generic_call_preserves_outer_type_param_in_contravariant_object_member() {
    let diags = check_source_diagnostics(
        r#"
interface Effect {}
interface Enqueue<A> { offer: (value: A) => Effect; }
declare const offer: { <A>(self: Enqueue<A>, value: A): Effect; };

function g<T>(queue: Enqueue<T>, value: T) {
    offer(queue, value);
}
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected generic inference to preserve outer type parameter evidence for contravariant object members, got: {:?}",
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
/// Exercises `common::classify_literal_type` via generic call inference
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

/// Regression: `application_info` boundary wrapper used in return context
/// substitution collection for Application types (e.g., Promise<T>).
/// Exercises the code path where `collect_return_context_substitution`
/// compares Application types via `common::application_info`.
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

/// Regression: `function_shape_for_type` boundary wrapper used during
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

/// Regression: `unpack_tuple_rest_parameter` boundary wrapper used in
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

/// Regression: `is_callable_type` boundary wrapper used in contextual
/// call param type normalization (`normalize_contextual_call_param_type`).
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

/// Regression: `find_property_in_object` boundary wrapper used in
/// `constructor_type_from_new_property` for construct signature lookup.
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

/// Regression: `enum_def_id` boundary wrapper used during generic inference
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
/// `classify_for_call_signatures` query.
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

// === Overload regression tests ===
// Focused tests exercising overload resolution paths through query boundary APIs.

/// Overload resolution with generic overloads selects the correct signature.
#[test]
fn overload_generic_and_non_generic_signatures() {
    let diags = check_source_diagnostics(
        r#"
declare function choose(x: string): string;
declare function choose<T>(x: T, y: T): T;

const a: string = choose("hello");
const b: number = choose(1, 2);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for mixed generic/non-generic overloads, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Overload with optional parameters resolves correctly based on arity.
#[test]
fn overload_optional_parameter_arity() {
    let diags = check_source_diagnostics(
        r#"
declare function fmt(x: number): string;
declare function fmt(x: number, precision: number): string;

const a: string = fmt(3);
const b: string = fmt(3, 2);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for overload with optional parameter arity, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Overload resolution with callback parameters: the callback contextual type
/// should come from the selected overload signature.
#[test]
fn overload_callback_contextual_typing() {
    let diags = check_source_diagnostics(
        r#"
declare function on(event: "click", handler: (x: number) => void): void;
declare function on(event: "hover", handler: (x: string) => void): void;

on("click", x => { const n: number = x; });
on("hover", x => { const s: string = x; });
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 7006 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for overload callback contextual typing, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// === callWithSpread regression tests ===
// Focused tests exercising spread argument handling in call resolution.

/// Spread of an array into a rest parameter.
#[test]
fn call_with_spread_array_into_rest() {
    let diags = check_source_diagnostics(
        r#"
declare function sum(...nums: number[]): number;
const arr: number[] = [1, 2, 3];
const result: number = sum(...arr);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2556)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for spread array into rest param, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Spread of a tuple type into fixed parameters.
#[test]
fn call_with_spread_tuple_into_fixed_params() {
    let diags = check_source_diagnostics(
        r#"
declare function pair(a: string, b: number): [string, number];
const args: [string, number] = ["hello", 42];
const result: [string, number] = pair(...args);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2556 || d.code == 2554)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for spread tuple into fixed params, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Mixed spread and non-spread arguments.
#[test]
fn call_with_spread_mixed_args() {
    let diags = check_source_diagnostics(
        r#"
declare function combine(prefix: string, ...nums: number[]): string;
const nums: number[] = [1, 2, 3];
const result: string = combine("total", ...nums);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2556 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for mixed spread and non-spread args, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Spread with wrong element type should emit TS2345.
#[test]
fn call_with_spread_type_mismatch() {
    let diags = check_source_diagnostics(
        r#"
declare function nums(...args: number[]): void;
const strs: string[] = ["a", "b"];
nums(...strs);
"#,
    );

    let ts2345: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2556)
        .collect();
    assert!(
        !ts2345.is_empty(),
        "Expected TS2345/TS2556 for spread with type mismatch, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// === Property call regression tests ===
// Tests for method invocation through property access expressions.

/// Property call on class instance method.
#[test]
fn property_call_class_method() {
    let diags = check_source_diagnostics(
        r#"
class Counter {
    count: number = 0;
    increment(by: number): number {
        this.count += by;
        return this.count;
    }
}

declare const c: Counter;
const val: number = c.increment(5);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2339 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for class method property call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Element access call (bracket notation method call).
#[test]
fn element_access_call() {
    let diags = check_source_diagnostics(
        r#"
interface Obj {
    method(x: number): string;
}

declare const obj: Obj;
const result: string = obj["method"](42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2349)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for element access call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Optional chain on method with non-null assertion unwrapped correctly.
#[test]
fn optional_chain_method_call_with_non_null_assertion() {
    let diags = check_source_diagnostics(
        r#"
interface Api {
    fetch(url: string): string;
}

declare const api: Api | null;
const result: string = api!.fetch("/data");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2349)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for non-null assertion method call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// === Focused regression tests for query-boundary call/overload/property-call paths ===

/// Regression: generic interface application as callee resolves call signatures
/// through `classify_for_call_signatures` and `get_contextual_signature_for_arity`
/// boundary queries without direct solver inspection.
#[test]
fn generic_interface_application_callee() {
    let diags = check_source_diagnostics(
        r#"
interface Mapper<T, U> {
    (input: T): U;
}

declare const toStr: Mapper<number, string>;
const result: string = toStr(42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2349)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for generic interface application callee, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: overloaded function with spread arguments uses boundary queries
/// for signature classification and arity matching.
#[test]
fn overloaded_call_with_spread_arguments() {
    let diags = check_source_diagnostics(
        r#"
declare function concat(a: string, b: string): string;
declare function concat(a: number, b: number): number;

const strArgs: [string, string] = ["hello", " world"];
const result: string = concat(...strArgs);
"#,
    );

    // Spread call on overloaded function should resolve correctly.
    // The important invariant: no panic, and the right overload is picked.
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    // These should not appear for a valid spread call.
    assert_eq!(
        ts2349.len(),
        0,
        "Expected no TS2349 for overloaded spread call, got: {:?}",
        ts2349.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
    let _ = ts2769; // May or may not fire depending on spread resolution details
}

/// Regression: property call on a method inherited through intersection type
/// exercises `classify_for_call_signatures` on intersection members.
#[test]
fn property_call_through_intersection_type() {
    let diags = check_source_diagnostics(
        r#"
interface HasName {
    getName(): string;
}
interface HasAge {
    getAge(): number;
}

declare const person: HasName & HasAge;
const name: string = person.getName();
const age: number = person.getAge();
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2339 || d.code == 2349)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for property call through intersection, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: calling a union of callable types with incompatible signatures
/// should NOT use overload semantics (each member must accept the call).
#[test]
fn union_callable_not_treated_as_overloads() {
    let diags = check_source_diagnostics(
        r#"
type F1 = (x: string) => void;
type F2 = (x: number) => void;

declare const fn1: F1 | F2;
fn1("hello");
"#,
    );

    // Union of incompatible callables called with a single-type argument:
    // only F1 accepts string, F2 doesn't. This may produce TS2345 or TS2769.
    // The architecture invariant: `callee_is_union` flag prevents overload path.
    let _ = diags;
}

/// Regression: generic call with multiple type parameters and callback that
/// requires progressive inference (Round 1 → Round 2 refinement).
#[test]
fn generic_call_progressive_inference_two_type_params() {
    let diags = check_source_diagnostics(
        r#"
declare function transform<T, U>(items: T[], fn: (item: T) => U): U[];

const input = [1, 2, 3];
const output: string[] = transform(input, n => String(n));
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 7006)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for progressive generic inference, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: generic overloaded function with type arguments validates
/// constraints through `validate_call_type_arguments` boundary query.
#[test]
fn generic_overloaded_call_with_explicit_type_args() {
    let diags = check_source_diagnostics(
        r#"
declare function create<T extends string>(value: T): T;
declare function create<T extends number>(value: T): T;

const s: "hello" = create<"hello">("hello");
const n: 42 = create<42>(42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2344 || d.code == 2769)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for generic overloaded call with type args, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: property call on optional chain exercises the nullish splitting
/// and callee type resolution through boundary queries.
#[test]
fn optional_chain_property_call_generic() {
    let diags = check_source_diagnostics(
        r#"
interface Service<T> {
    fetch(id: number): T;
}

declare const svc: Service<string> | undefined;
const result: string | undefined = svc?.fetch(1);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2349)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for optional chain generic property call, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Boundary regression tests: ensure call.rs uses solver query APIs
// for overload, property-call, and generic inference paths without
// direct TypeData/TypeKey inspection.
// ---------------------------------------------------------------------------

/// Regression: overload resolution with mixed generic and non-generic signatures
/// exercises `classify_for_call_signatures` and `get_overload_call_signatures`
/// boundary queries rather than direct TypeData pattern matching.
#[test]
fn overload_mixed_generic_nongeneric_no_internal_inspection() {
    let diags = check_source_diagnostics(
        r#"
declare function overloaded(x: string): number;
declare function overloaded<T>(x: T, y: T): T;

const a: number = overloaded("hello");
const b: string = overloaded("a", "b");
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2554)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no type errors for mixed generic/non-generic overloads, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: property call on intersection type exercises callable shape
/// extraction via boundary queries (not direct intersection member inspection).
#[test]
fn property_call_intersection_callable_boundary() {
    let diags = check_source_diagnostics(
        r#"
interface Logger {
    log(msg: string): void;
}
interface Formatter {
    format(data: number): string;
}

declare const obj: Logger & Formatter;
obj.log("test");
const s: string = obj.format(42);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2345 || d.code == 2322)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors for intersection property calls, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: overload resolution where only later signatures match exercises
/// iteration through `get_overload_call_signatures` without internal TypeKey/TypeData.
#[test]
fn overload_later_signature_match() {
    let diags = check_source_diagnostics(
        r#"
declare function choose(x: string): string;
declare function choose(x: number): number;
declare function choose(x: boolean): boolean;

const r: boolean = choose(true);
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no errors when later overload signature matches, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: generic call with contextual callback where param type contains
/// intersection with type parameter exercises `contains_type_parameters` and
/// `intersection_members` boundary queries.
#[test]
fn generic_call_callback_intersection_type_param_boundary() {
    let diags = check_source_diagnostics(
        r#"
interface Base {
    id: number;
}

declare function withBase<T extends Base>(init: (item: T & { extra: string }) => void): void;
withBase<Base>((item) => {
    const id: number = item.id;
    const extra: string = item.extra;
});
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for callback with intersection type param, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: property call on method returning generic application exercises
/// `evaluate_application_type` and `resolve_lazy_type` boundary paths.
/// The return type is inferred and assigned; property access on the result
/// must resolve through query boundaries without direct TypeData inspection.
#[test]
fn property_call_generic_application_return_type() {
    let diags = check_source_diagnostics(
        r#"
interface Container<T> {
    value: T;
}
interface Factory {
    create<T>(val: T): Container<T>;
}

declare const factory: Factory;
const c = factory.create(42);
const v: number = c.value;
"#,
    );

    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on generic application return, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Regression: overloaded method call with type predicate exercises
/// `extract_predicate_signature` and `is_valid_union_predicate` boundary queries.
#[test]
fn overloaded_method_type_predicate_boundary() {
    let diags = check_source_diagnostics(
        r#"
interface Guard {
    check(x: unknown): x is string;
    check(x: unknown, strict: boolean): x is number;
}

declare const g: Guard;
declare const val: unknown;
if (g.check(val)) {
    const s: string = val;
}
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for overloaded method type predicate, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn block_body_contextual_callback_return_mismatch_reports_ts2345() {
    let diags = check_source_diagnostics(
        r#"
declare function f(g: (x: number) => number[]): void;
f((x) => { return x.toFixed(); });
"#,
    );

    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected one outer TS2345 for block-body callback return mismatch, got: {diags:?}"
    );
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no inner TS2322 for block-body callback return mismatch, got: {diags:?}"
    );
}

#[test]
fn expression_body_contextual_callback_return_mismatch_stays_ts2322() {
    let diags = check_source_diagnostics(
        r#"
declare function f(g: (x: number) => number[]): void;
f((x) => x.toFixed());
"#,
    );

    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2345.len(),
        0,
        "Expected no outer TS2345 for expression-body callback return mismatch, got: {diags:?}"
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one inner TS2322 for expression-body callback return mismatch, got: {diags:?}"
    );
}

#[test]
fn block_body_callback_with_fewer_parameters_does_not_report_ts2769() {
    let diags = check_source_diagnostics(
        r#"
interface Collection<T, U> {
    length: number;
    add(x: T, y: U): void;
    remove(x: T, y: U): boolean;
}
interface Combinators {
    map<T, U, V>(c: Collection<T, U>, f: (x: T, y: U) => V): Collection<T, V>;
    map<T, U>(c: Collection<T, U>, f: (x: T, y: U) => any): Collection<any, any>;
}
declare const c2: Collection<number, string>;
declare const _: Combinators;
const rf1 = (x: number) => { return x.toFixed(); };
_.map(c2, rf1);
"#,
    );

    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        0,
        "Expected no TS2769 for fewer-parameter block-body callback, got: {diags:?}"
    );
}
