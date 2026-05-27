#[test]
fn noinfer_array_alias_widens_to_primitive() {
    let source = r#"
type NI<T> = NoInfer<T>;
declare function choose<T>(options: T[], fallback: NI<T>): T;
choose(["foo", "bar"], "baz");
choose([1, 2], 3);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "NoInfer via type alias still widens array-inferred T to primitive. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_callback_return_accepts_widened_numeric_array_inference() {
    let source = r#"
declare function process<T>(arr: T[], fn: (x: T) => T): T[];

const result = process([1, 2, 3], x => x * 2);
const check: number[] = result;
const literalOnly: (1 | 2 | 3)[] = result;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !has_diagnostic_message_containing(
            &diags,
            2322,
            "Type 'number' is not assignable to type '1 | 2 | 3'",
        ),
        "numeric array inference should widen T before checking callback return. Got: {diags:#?}"
    );
    assert!(
        has_diagnostic_message_containing(&diags, 2322, "number[]"),
        "result should be number[], not a literal-only array. Got: {diags:#?}"
    );
}

#[test]
fn noinfer_array_of_inferred_literal_accepts_same_literal() {
    let source = r#"
function test<T extends string>(value: T, options: NoInfer<T>[]): T {
    return value;
}

const t = test("hello", ["hello"]);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "NoInfer<T>[] should check against the literal inferred from the first argument without self-contradictory TS2322. Diagnostics: {diags:#?}"
    );
}

// ─── Inference with multiple callbacks ────────────────────────────────

#[test]
fn multiple_callback_params_all_get_contextual_types() {
    // When a generic function has multiple callback parameters,
    // all should receive contextual types from the inferred type arguments.
    let source = r#"
declare function bimap<T, U, V>(
    arr: T[],
    first: (x: T) => U,
    second: (x: T) => V
): [U[], V[]];
const result = bimap([1, 2, 3], x => x + 1, x => String(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "All callbacks should receive contextual types. Diagnostics: {diags:#?}"
    );
}

// ─── Explicit type arguments bypass inference ─────────────────────────

#[test]
fn explicit_type_arguments_provide_callback_context() {
    // When type arguments are explicitly provided, callback params
    // should be contextually typed from those explicit types.
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map<number, string>([1, 2], x => String(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[7006, 2345]),
        "Explicit type args should provide callback context. Diagnostics: {diags:#?}"
    );
}

// ─── Nested generic calls ─────────────────────────────────────────────

#[test]
fn nested_generic_calls_propagate_inference() {
    // Generic inference should work through nested generic calls
    let source = r#"
declare function id<T>(x: T): T;
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
const result = map([1, 2, 3], x => id(x));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "Nested generic calls should propagate inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic inference with default type parameters ───────────────────

#[test]
fn default_type_parameter_used_when_not_inferable() {
    // When a type parameter has a default and cannot be inferred,
    // the default should be used.
    let source = r#"
declare function create<T = string>(value?: T): T;
const result = create();
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Default type parameter should be used when not inferable. Diagnostics: {diags:#?}"
    );
}

#[test]
fn default_type_parameter_substitutes_inside_conditional_constraint() {
    // Regression for issue #6559: using Chainable without explicit type
    // arguments must substitute Config = {} inside the nested conditional
    // constraint for option's key parameter.
    let source = r#"
type Chainable<Config = {}> = {
  option<K extends string>(
    key: K extends keyof Config ? never : K,
    value: number
  ): void;
};

declare const explicit: Chainable<{}>;
explicit.option('foo', 123);

declare const defaulted: Chainable;
defaulted.option('foo', 123);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Default type parameter should substitute into conditional constraint. Diagnostics: {diags:#?}"
    );
}

#[test]
fn contextual_return_instantiates_defaulted_generic_call_result() {
    let source = r#"
interface Box<T> { value: T | undefined }
declare function make<O>(p: { value?: O }): Box<O>
const x: Box<string> = make({})
function f<T>(): Box<T> { return make({}) }
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Contextual generic return inference should instantiate the call result. Diagnostics: {diags:#?}"
    );
}

#[test]
fn result_union_false_branch_infers_never_and_error_type() {
    let source = r#"
type Result<T, E = unknown> =
  | { ok: true; value: T }
  | { ok: false; error: E };

function failure<E = unknown>(error: E): Result<never, E> {
  return { ok: false, error };
}

function handle<T, E>(result: Result<T, E>): T | E {
  return result.ok ? result.value : result.error;
}

const viaInline: never | string = handle(failure("error"));
const result = failure("error");
const viaAlias: never | string = handle(result);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Result-like false branch should infer T=never and E=string. Diagnostics: {diags:#?}"
    );
}

#[test]
fn unconstrained_generic_callback_parameter_reports_unknown_addition() {
    let source = r#"
function identity<T>(fn: (x: T) => T): (x: T) => T {
  return fn;
}

const increment = identity(x => x + 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        has_diagnostic_message_containing(&diags, 18046, "'x' is of type 'unknown'"),
        "unconstrained callback parameter should be unknown for arithmetic use. Diagnostics: {diags:#?}"
    );
}

#[test]
fn renamed_unconstrained_generic_callback_parameter_reports_unknown_multiply() {
    let source = r#"
function transform<T>(mapper: (val: T) => T): T {
  return mapper(undefined as any);
}

const doubled = transform(n => n * 2);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        has_diagnostic_message_containing(&diags, 18046, "'n' is of type 'unknown'"),
        "renamed unconstrained callback parameter should be unknown for arithmetic use. Diagnostics: {diags:#?}"
    );
}

#[test]
fn renamed_result_union_false_branch_inference_is_structural() {
    let source = r#"
type Outcome<A, B> =
  | { tag: "some"; data: A }
  | { tag: "none"; problem: B };

function miss<B>(problem: B): Outcome<never, B> {
  return { tag: "none", problem };
}

function unwrap<A, B>(outcome: Outcome<A, B>): A | B {
  return outcome.tag === "some" ? outcome.data : outcome.problem;
}

const outcome = miss("missing");
const value: never | string = unwrap(outcome);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Branch inference must not depend on Result/ok/error spellings. Diagnostics: {diags:#?}"
    );
}

// ─── Callable shape sanitization with overloads ───────────────────────

#[test]
fn callable_binding_pattern_param_sanitization_single_signature() {
    // A callable argument with a destructured param should not break inference.
    // The callable shape's call signature params at binding-pattern positions
    // are replaced with `unknown` to avoid polluting the inference constraint.
    let source = r#"
declare function apply<T extends { a: number; b: string }>(
    items: T[],
    fn: (item: T) => void
): void;
apply([{ a: 1, b: "x" }], ({ a, b }) => {
    const _n: number = a;
    const _s: string = b;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2345, 7031]),
        "Callable shape sanitization should not break inference. Diagnostics: {diags:#?}"
    );
}

#[test]
fn callable_binding_pattern_does_not_leak_unknown_into_inferred_type() {
    // When a callback destructures its parameter, the inferred type for the
    // generic should still be correct (not unknown).
    let source = r#"
declare function first<T>(arr: T[], fn: (item: T) => boolean): T;
const result = first([1, 2, 3], (item) => item > 0);
const check: number = result;
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Inferred T should still be number. Diagnostics: {diags:#?}"
    );
}

#[test]
fn conditional_alias_first_arg_context_types_binding_pattern_callback() {
    let source = r#"
interface TypeLambda {
    readonly In: unknown;
    readonly Out: unknown;
}
type Kind<F extends TypeLambda, In, Target> = F extends { readonly type: unknown }
    ? (F & { readonly In: In; readonly Target: Target })["type"]
    : { readonly F: F; readonly In: (_: In) => void; readonly Target: (_: Target) => Target };

declare const map: <F extends TypeLambda, R, A, B>(
    self: Kind<F, R, A>,
    f: (a: A) => B
) => Kind<F, R, B>;

declare const pair: <F extends TypeLambda, R, A, B>(
    left: Kind<F, R, A>,
    right: Kind<F, R, B>
) => Kind<F, R, [A, B]>;

function use<F extends TypeLambda, R, A, B>(
    left: Kind<F, R, A>,
    right: Kind<F, R, B>,
    f: (a: A, b: B) => string
): Kind<F, R, string> {
    return map(pair(left, right), ([a, b]) => f(a, b));
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2345, 7031]),
        "conditional alias inference should type destructured callback params. Diagnostics: {diags:#?}"
    );
}

#[test]
fn overloaded_conditional_alias_first_arg_context_types_binding_pattern_callback() {
    let source = r#"
interface TypeLambda {
    readonly In: unknown;
    readonly Out: unknown;
}
type Kind<F extends TypeLambda, In, Target> = F extends { readonly type: unknown }
    ? (F & { readonly In: In; readonly Target: Target })["type"]
    : { readonly F: F; readonly In: (_: In) => void; readonly Target: (_: Target) => Target };

interface Covariant<F extends TypeLambda> {
    readonly map: {
        <A, B>(f: (a: A) => B): <R>(self: Kind<F, R, A>) => Kind<F, R, B>;
        <R, A, B>(self: Kind<F, R, A>, f: (a: A) => B): Kind<F, R, B>;
    };
}
interface Product<F extends TypeLambda> extends Covariant<F> {
    readonly pair: <R, A, B>(
        left: Kind<F, R, A>,
        right: Kind<F, R, B>
    ) => Kind<F, R, [A, B]>;
}

function use<F extends TypeLambda, R, A, B>(
    F: Product<F>,
    left: Kind<F, R, A>,
    right: Kind<F, R, B>,
    f: (a: A, b: B) => string
): Kind<F, R, string> {
    return F.map(F.pair(left, right), ([a, b]) => f(a, b));
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2345, 7031]),
        "overloaded conditional alias inference should type destructured callback params. Diagnostics: {diags:#?}"
    );
}

#[test]
fn overloaded_higher_order_rest_any_constraint_accepts_generic_body() {
    let source = r#"
type Parameters<T extends (...args: any[]) => any> =
    T extends (...args: infer P) => any ? P : never;
interface IArguments {}

declare const dual: {
    <DataLast extends (...args: any[]) => any, DataFirst extends (...args: any[]) => any>(
        arity: Parameters<DataFirst>["length"],
        body: DataFirst
    ): DataLast & DataFirst;
    <DataLast extends (...args: any[]) => any, DataFirst extends (...args: any[]) => any>(
        isDataFirst: (args: IArguments) => boolean,
        body: DataFirst
    ): DataLast & DataFirst;
};

const make = (): {
    <A, B, C>(a: A, b: B, f: (a: A, b: B) => C): C;
} =>
    dual(3, <A, B, C>(a: A, b: B, f: (a: A, b: B) => C): C => f(a, b));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2769),
        "higher-order generic body should satisfy the rest-any function constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation edge cases ──────────────────────────────

#[test]
fn contextual_instantiation_through_readonly() {
    // Return context substitution should unwrap Readonly<T> when matching
    let source = r#"
declare function wrap<T>(value: T): Readonly<T>;
const result: Readonly<string> = wrap("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should match through Readonly. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_call_with_union_return_context() {
    // Return context substitution should handle union target types
    let source = r#"
declare function id<T>(x: T): T;
const result: string | number = id("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Return context should work with union target. Diagnostics: {diags:#?}"
    );
}

#[test]
fn generic_call_application_matching_in_return_context() {
    // When source and target are both applications of the same generic,
    // their type arguments should be matched structurally.
    let source = r#"
interface Container<T> { value: T; }
declare function box_it<T>(value: T): Container<T>;
const result: Container<number> = box_it(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Application matching should work in return context. Diagnostics: {diags:#?}"
    );
}

#[test]
fn returned_function_parameters_keep_same_application_return_context() {
    let source = r#"
type Mapper<T, U> = (x: T) => U;
declare function wrap<T, U>(cb: Mapper<T, U>): Mapper<T, U>;
declare function arrayize<T, U>(cb: Mapper<T, U>): Mapper<T, U[]>;
declare function combine<A, B, C>(f: (x: A) => B, g: (x: B) => C): (x: A) => C;
declare function foo(f: Mapper<string, number>): void;
declare const strings: { map<U>(cb: (x: string, index: number, array: string[]) => U): U[] };
declare function identity<T>(x: T): T;

let f3: Mapper<string, number[]> = arrayize(wrap(s => s.length));
let f4: Mapper<string, boolean> = combine(wrap(s => s.length), wrap(n => n >= 10));
foo(wrap(s => s.length));
let a4 = strings.map(combine(wrap(s => s.length), wrap(n => n > 10)));
let a5 = strings.map(combine(identity, wrap(s => s.length)));
let a6 = strings.map(combine(wrap(s => s.length), identity));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Returned function parameter type should be preserved for same-application return context. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual instantiation through intersections ────────────────────

#[test]
fn generic_callback_in_intersection_parameter() {
    // When a generic parameter type is an intersection involving a callback,
    // inference should still provide contextual types for callback params.
    let source = r#"
declare function register<T>(value: T, handler: (x: T) => void): void;
register(42, (x) => {
    const _n: number = x;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "Intersection callback should get contextual type. Diagnostics: {diags:#?}"
    );
}

// ─── Generic inference with mapped type return ──────────────────────────

#[test]
fn generic_inference_with_conditional_return() {
    // Generic inference should work when return type involves a conditional
    let source = r#"
declare function check<T>(x: T): T extends string ? true : false;
const result = check("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Conditional return type should not break inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with spread arguments ─────────────────────────────────

#[test]
fn generic_call_infers_from_spread_args() {
    // Spread arguments should participate in generic inference
    let source = r#"
declare function first<T>(arr: T[]): T;
const arr = [1, 2, 3];
const result = first(arr);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Spread args should participate in inference. Diagnostics: {diags:#?}"
    );
}

// ─── Widening with number literal constraint ────────────────────────────

#[test]
fn number_literal_constraint_preserves_literal_type() {
    // T extends number should preserve the literal 42, not widen to number
    let source = r#"
declare function num<T extends number>(x: T): T;
const result = num(42);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Number literal should be preserved with number constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Boolean literal constraint ──────────────────────────────────────────

#[test]
fn boolean_literal_constraint_preserves_literal_type() {
    let source = r#"
declare function bool<T extends boolean>(x: T): T;
const result = bool(true);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Boolean literal should be preserved with boolean constraint. Diagnostics: {diags:#?}"
    );
}

// ─── Contextual type propagation through Promise-like ────────────────────

#[test]
fn generic_call_with_promise_return_context() {
    // Return context from a Promise<T> target should propagate T inference
    let source = r#"
declare function resolve<T>(x: T): Promise<T>;
async function test() {
    const result: Promise<string> = resolve("hello");
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "Promise return context should propagate inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call recheck with real types ────────────────────────────────

#[test]
fn recheck_generic_call_detects_argument_mismatch_after_inference() {
    // After inference resolves T, rechecking should catch argument mismatches
    let source = r#"
declare function map<T>(arr: T[], fn: (x: T) => T): T[];
const result = map([1, 2, 3], (x) => "not a number");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_any_diagnostic_code(&diags, &[2322, 2345]),
        "Recheck should detect type mismatch after inference. Diagnostics: {diags:#?}"
    );
}

// ─── Generic call with optional parameter ────────────────────────────────

#[test]
fn generic_inference_with_optional_params() {
    let source = r#"
declare function opt<T>(required: T, optional?: T): T;
const result = opt("hello");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "Optional params should not break generic inference. Diagnostics: {diags:#?}"
    );
}

// ─── Multiple constraints interacting ────────────────────────────────────

#[test]
fn generic_with_extends_keyof_constraint() {
    // T extends keyof U should constrain T to string literal union keys of U
    let source = r#"
declare function pick<U, T extends keyof U>(obj: U, key: T): U[T];
const result = pick({ a: 1, b: "hello" }, "a");
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "keyof constraint should work with inference. Diagnostics: {diags:#?}"
    );
}

// ─── Recursive generic callback ──────────────────────────────────────────

#[test]
fn recursive_generic_callback_does_not_stack_overflow() {
    // Self-referential constraints should not cause stack overflow
    let source = r#"
interface Tree<T> { value: T; children: Tree<T>[]; }
declare function traverse<T>(tree: Tree<T>, fn: (node: Tree<T>) => void): void;
declare const tree: Tree<number>;
traverse(tree, (node) => {
    const _v: number = node.value;
});
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "Recursive generic callback should not overflow. Diagnostics: {diags:#?}"
    );
}

// ─── TS2454 does not suppress downstream type errors ────────────────

#[test]
fn ts2454_does_not_suppress_ts2322_on_generic_constraint() {
    // When a variable is used before assignment (TS2454), tsc still type-checks
    // the expression using the declared type. Property-level mismatches like
    // TS2322 must still be emitted alongside TS2454.
    // Regression: genericConstraintSatisfaction1.ts
    let source = r#"
interface I<S> {
   f: <T extends S>(x: T) => void
}

var x: I<{s: string}>
declare var x: I<{s: string}>
x.f({s: 1})
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2454),
        "Should emit TS2454 for variable used before assignment. Got: {diags:#?}"
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "Should also emit TS2322 for property type mismatch. Got: {diags:#?}"
    );
}

#[test]
fn dependent_type_parameter_constraint_checks_second_argument_against_first_inference() {
    // Regression: typeParameterAsTypeParameterConstraint2.ts
    // For <T, U extends T>, tsc fixes T from the first argument and then
    // validates the second argument's inferred U against that T.
    let source = r#"
interface NumberVariant {
    x: number;
}

var n: NumberVariant;
function foo<T, U extends T>(x: T, y: U): U { return y; }
foo(1, n);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2454),
        "Should emit TS2454 for variable used before assignment. Got: {diags:#?}"
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2345 && message.contains("NumberVariant") && message.contains("number")
        }),
        "Should also emit TS2345 for NumberVariant not assignable to number. Got: {diags:#?}"
    );
}

#[test]
fn ts2454_does_not_suppress_property_access_errors() {
    // Even with TS2454, property accesses on the declared type should
    // still produce type errors when used in incompatible contexts.
    let source = r#"
var x: {s: string}
x.f({s: 1})
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2454),
        "Should emit TS2454. Got: {diags:#?}"
    );
    // x.f doesn't exist on {s: string}, so TS2339 should fire
    assert!(
        has_diagnostic_code(&diags, 2339),
        "Should also emit TS2339 for missing property. Got: {diags:#?}"
    );
}

// ─── Union type predicates must not narrow when non-predicate members
//     return general boolean ──────────────────────────────────────────

#[test]
fn union_this_predicate_with_boolean_member_does_not_narrow() {
    // Regression: typePredicatesInUnion3.ts
    // When a union method has a `this` type predicate on one member and plain
    // boolean on another, the call is NOT a type predicate. The receiver must
    // NOT be narrowed.
    let source = r#"
type HasAttribute<T> = T & { attribute: number };

class Type1 {
    attribute: number | null = null;
    predicate(): this is HasAttribute<Type1> {
        return true;
    }
}

class Type2 {
    attribute: number | null = null;
    predicate(): boolean {
        return true;
    }
}

function assertType<T>(_val: T) {
}

declare const val: Type1 | Type2;

if (val.predicate()) {
    assertType<number>(val.attribute);  // Error: number | null not assignable to number
}
"#;
    let diags = compile_and_get_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2345),
        "Should emit TS2345 because val is not narrowed by union predicate. Got: {diags:#?}"
    );
}

#[test]
fn this_predicate_union_with_false_returning_member_narrows() {
    // When ALL non-predicate union members return exclusively `false`,
    // the union IS a valid type predicate. The predicate narrows the
    // receiver and non-predicate members are impossible in the true branch.
    let source = r#"
class Entry {
    c: number = 1;
    guard(): this is Entry { return true; }
}
class Group {
    d: string = "no";
    guard(): false { return false; }
}
declare var chunk: Entry | Group;
let x = chunk.guard() ? chunk.c : chunk.d;
"#;
    let diags = compile_and_get_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2339),
        "Should NOT emit TS2339 - chunk.c should be accessible after guard(). Got: {diags:#?}"
    );
}

// ─── Empty object type ({}) in BCT inference ────────────────────────

#[test]
fn bct_inference_recognizes_empty_object_as_supertype_of_primitives() {
    // When inference candidates include primitives and `{}`, the BCT tournament
    // must recognize `{}` as a supertype of non-nullable primitives. This ensures
    // `{}` is not dropped from inference results due to first-wins tournament logic.
    // Repro: ReadonlyArray<T> inference from union arrays containing `{}[]`.
    let source = r#"
declare function foo<T>(x: ReadonlyArray<T>): T;
declare const a: (string | number)[] | null[] | {}[];
let x = foo(a);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "ReadonlyArray<T> inference from union of arrays with {{}} should work. Got: {diags:#?}"
    );
}

#[test]
fn bivariant_inference_this_parameter_union_of_arrays() {
    // Repro from TypeScript #27337: calling a method with a generic `this`
    // parameter on a union of arrays should infer T from all union members.
    // The empty object type `{}` must be recognized as a supertype of
    // primitives in the BCT tournament to avoid false TS2684.
    let source = r#"
interface Array<T> {
    equalsShallow<T>(this: ReadonlyArray<T>, other: ReadonlyArray<T>): boolean;
}
declare const a: (string | number)[] | null[] | undefined[] | {}[];
declare const b: (string | number)[] | null[] | undefined[] | {}[];
let x = a.equalsShallow(b);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2684),
        "Method with generic this on union of arrays should not emit TS2684. Got: {diags:#?}"
    );
}

#[test]
fn union_this_type_in_functions_emits_ts2684() {
    // unionThisTypeInFunctions conformance test: calling a method with `this: this`
    // on a union type where members have incompatible `data` properties.
    // The `this` context is Real | Fake, but the method requires Real & Fake.
    let source = r#"
interface Real {
    method(this: this, n: number): void;
    data: string;
}
interface Fake {
    method(this: this, n: number): void;
    data: number;
}
function test(r: Real | Fake) {
    r.method(12);
}
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2684),
        "Should emit TS2684 for union this type mismatch. Got: {diags:#?}"
    );
}

#[test]
fn ts2684_union_method_this_message_uses_interface_names_not_outer_function() {
    // Regression for an ID-conflation bug in the property-access `this`
    // binder: when the noop TypeResolver couldn't translate an interface's
    // SymbolId to a DefId, `nominalize_object_receiver` fell back to
    // `interner.reference(SymbolRef(sym_id.0))`, which created a
    // `Lazy(DefId(sym_id.0))`. Because `SymbolId.0` and `DefId.0` are
    // independent ID spaces, this produced a Lazy that pointed at an
    // *unrelated* declaration (e.g., the enclosing `test` function), so the
    // TS2684 message rendered as `Real & test` (or `Fake & test`) instead of
    // `Real & Fake`. The fix: keep the original Object receiver when no
    // DefId mapping exists, so the formatter can recover the interface name
    // through `shape.symbol`.
    let source = r#"
interface Real {
    method(this: this, n: number): void;
    data: string;
}
interface Fake {
    method(this: this, n: number): void;
    data: number;
}
function test(r: Real | Fake) {
    r.method(12);
}
"#;
    let diags = relevant_diagnostics(source);
    let ts2684 = diags
        .iter()
        .find(|(code, _)| *code == 2684)
        .expect("expected TS2684 diagnostic");
    let msg = &ts2684.1;
    // The expected `this` should display as `Real & Fake` (interface names),
    // not as `Fake & test` or `Lazy(N) & Lazy(M)`.
    assert!(
        msg.contains("'Real & Fake'") || msg.contains("'Fake & Real'"),
        "TS2684 message should reference both interface names, not the outer function. Got: {msg}"
    );
    assert!(
        !msg.contains("test'"),
        "TS2684 message must not leak the enclosing function name `test`. Got: {msg}"
    );
    assert!(
        !msg.contains("Lazy("),
        "TS2684 message must not leak `Lazy(N)` placeholders. Got: {msg}"
    );
}

// ─── Higher-order generic contextual types (compose/flip patterns) ──────

#[test]
fn compose_with_naked_generic_function_arguments() {
    // compose(list, box) should infer <T>(x: T) => Box<T[]>
    // when assigned to a variable with that generic function annotation.
    // Source type params (T in list, V in box) appear directly (naked) as
    // parameter types, enabling proper higher-order inference.
    let source = r#"
type Box<T> = { value: T };
declare function compose<A, B, C>(f: (a: A) => B, g: (b: B) => C): (a: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): Box<V>;
const f11: <T>(x: T) => Box<T[]> = compose(list, box);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "compose(list, box) with generic contextual type should not error. Got: {diags:#?}"
    );
}

#[test]
fn compose_with_wrapped_generic_function_arguments() {
    // compose(unbox, unlist) should infer <T>(x: Box<T[]>) => T
    // when assigned to a variable with that generic function annotation.
    // Source type params (W in unbox, T in unlist) appear inside wrapper
    // types (Box<W>, T[]), requiring the contextual type to drive inference.
    let source = r#"
type Box<T> = { value: T };
declare function compose<A, B, C>(f: (a: A) => B, g: (b: B) => C): (a: A) => C;
declare function unbox<W>(x: Box<W>): W;
declare function unlist<T>(a: T[]): T;
const f13: <T>(x: Box<T[]>) => T = compose(unbox, unlist);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "compose(unbox, unlist) with generic contextual type should not error. Got: {diags:#?}"
    );
}

#[test]
fn flip_with_generic_function_argument() {
    // flip(zip) should infer <A, B>(b: B, a: A) => [A, B]
    // when assigned to a variable with that generic function annotation.
    let source = r#"
declare function zip<A, B>(a: A, b: B): [A, B];
declare function flip<X, Y, Z>(f: (x: X, y: Y) => Z): (y: Y, x: X) => Z;
const f40: <A, B>(b: B, a: A) => [A, B] = flip(zip);
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "flip(zip) with generic contextual type should not error. Got: {diags:#?}"
    );
}

#[test]
fn non_inferrable_type_propagation_not_broken() {
    // Regression guard: filter(exists(...)) in a pipe should not produce
    // false TS2345 errors. The generic function result from exists() has
    // non-naked type params that should be erased during inference.
    let source = r#"
interface Predicate<A> { (a: A): boolean }
interface Left<E> { readonly _tag: 'Left'; readonly left: E }
interface Right<A> { readonly _tag: 'Right'; readonly right: A }
type Either<E, A> = Left<E> | Right<A>;
declare const filter: {
    <A, B extends A>(refinement: { (a: A): a is B }): (as: ReadonlyArray<A>) => ReadonlyArray<B>
    <A>(predicate: Predicate<A>): <B extends A>(bs: ReadonlyArray<B>) => ReadonlyArray<B>
    <A>(predicate: Predicate<A>): (as: ReadonlyArray<A>) => ReadonlyArray<A>
};
declare function pipe<A, B>(a: A, ab: (a: A) => B): B;
declare function exists<A>(predicate: Predicate<A>): <E>(ma: Either<E, A>) => boolean;
declare const es: Either<string, number>[];
const x = pipe(es, filter(exists((n) => n > 0)));
"#;
    let diags = relevant_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "pipe(es, filter(exists(...))) should not produce TS2345. Got: {diags:#?}"
    );
}

#[test]
fn overloaded_pipe_return_context_types_chained_callback_params() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function pipe<A extends any[], B, C, D>(ab: (...args: A) => B, bc: (b: B) => C, cd: (c: C) => D): (...args: A) => D;
type Fn = (n: number) => number;
const fn30: Fn = pipe(
    x => x + 1,
    x => x * 2,
);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2362),
        "pipe return context should type chained callback parameters before checking arithmetic. Got: {diags:#?}"
    );
}

#[test]
fn curried_map_identity_preserves_array_element_type() {
    let source = r#"
interface Array<T> { map<U>(cb: (value: T) => U): U[]; }
declare const identity: <T>(value: T) => T;
declare function map<T, U>(transform: (t: T) => U): (arr: T[]) => U[];
const arr1: string[] = map(identity)(['a']);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(code, msg)| *code == 2322 && msg.contains("string[]")),
        "map(identity)(['a']) should preserve string[] assignability. Got: {diags:#?}"
    );
}

#[test]
fn pipe_preserves_self_constrained_generic_function_result() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function foo<T extends { value: T }>(x: T): T;

const g10: <T extends { value: T }>(x: T) => T = pipe(foo);
const g12: <T extends { value: T }>(x: T) => T = pipe(foo, foo);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "pipe(foo) should preserve the self-constrained generic signature without stale argument errors. Got: {diags:#?}"
    );
}

#[test]
fn pipe_preserves_generic_component_hoc_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;

type Component<P> = (props: P) => {};
declare const myHoc1: <P>(C: Component<P>) => Component<P>;
declare const myHoc2: <P>(C: Component<P>) => Component<P>;
declare const MyComponent1: Component<{ foo: 1 }>;

const enhance = pipe(myHoc1, myHoc2);
const MyComponent2 = enhance(MyComponent1);
const Preserved: Component<{ foo: 1 }> = MyComponent2;
const Wrong: Component<{ foo: 2 }> = MyComponent2;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "pipe(myHoc1, myHoc2) should preserve the component props type through the returned HOC. Got: {diags:#?}"
    );
    let ts2322_count = diagnostic_count(&diags, 2322);
    assert_eq!(
        ts2322_count, 1,
        "the returned HOC should reject incompatible props exactly once, proving props were not erased to unknown. Got: {diags:#?}"
    );
}

#[test]
fn pipe_contextual_return_flows_through_generic_function_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const g01: <T>(x: T) => { value: T[] } = pipe(list, box);
const g02: <T>(x: T) => { value: T }[] = pipe(box, list);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "pipe(list, box) and pipe(box, list) should infer the intermediate generic argument from the contextual return. Got: {diags:#?}"
    );
}

#[test]
fn pipe_contextual_return_flows_through_lambda_and_generic_function_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const g05: <T>(x: T) => { value: T[] } = pipe(x => list(x), x => box(x));
const inferred = pipe(x => list(x), x => box(x));
const keep: { value: 1[] } = inferred(1 as 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "pipe lambdas should inherit contextual return bounds through the generic calls they wrap. Got: {diags:#?}"
    );
}

#[test]
fn pipe_contextual_return_flows_through_nested_generic_call_chain() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const g06: <T>(x: T) => { value: T[] } = pipe(list, pipe(box));
const g07: <T>(x: T) => { value: T[] } = pipe(x => list(x), pipe(box));
const inferred1 = pipe(list, pipe(box));
const inferred2 = pipe(x => list(x), pipe(box));
const keep1: { value: 1[] } = inferred1(1 as 1);
const keep2: { value: 1[] } = inferred2(1 as 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "nested pipe calls should use the outer callable context to specialize the inner generic call. Got: {diags:#?}"
    );
}

#[test]
fn generic_function_rest_type_param_target_keeps_return_mismatch() {
    let source = r#"
declare function accepts<A extends any[]>(fn: (...args: A) => string): void;
declare function returnsNumber<T>(x: T): number;

accepts(returnsNumber);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("string")),
        "generic functions passed to rest type-parameter targets must still reject real return mismatches. Got: {diags:#?}"
    );
}

#[test]
fn generic_function_identifier_instantiates_against_fixed_tuple_rest_target() {
    let source = r#"
function callr<T extends unknown[], U>(args: T, f: (...args: T) => U) {
    return f(...args);
}

declare const sn: [string, number];
declare function choose<A, B>(a: A, b: B): A | B;

let value = callr(sn, choose);
let check: string | number = value;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[2322, 2345]),
        "generic function identifiers should infer from fixed tuple-rest parameters before return-context refinement. Got: {diags:#?}"
    );
}

#[test]
fn generic_function_identifier_fixed_tuple_rest_keeps_constraint_mismatch() {
    let source = r#"
function callr<T extends unknown[], U>(args: T, f: (...args: T) => U) {
    return f(...args);
}

declare const sn: [string, number];
declare function numberPair<A extends number, B extends number>(a: A, b: B): A | B;

let value = callr(sn, numberPair);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        has_diagnostic_code(&diags, 2345),
        "fixed tuple-rest refinement must still reject constrained generic parameter mismatches. Got: {diags:#?}"
    );
}

#[test]
fn pipe_nested_generic_call_keeps_parameter_mismatches() {
    let source = r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];
declare function boxNumbers(x: number[]): { value: number[] };

const bad: <T>(x: T) => { value: T[] } = pipe(list, pipe(boxNumbers));
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        has_any_diagnostic_code(&diags, &[2322, 2345]),
        "nested generic call contextual typing must not erase real parameter mismatches. Got: {diags:#?}"
    );
}

#[test]
fn return_context_refresh_keeps_callback_marker_context() {
    let source = r#"
type Values<T> = T[keyof T];
type EventObject = { type: string };

interface ActorLogic<TEvent extends EventObject> {
  transition: (ev: TEvent) => unknown;
}

type UnknownActorLogic = ActorLogic<never>;

interface ProvidedActor {
  src: string;
  logic: UnknownActorLogic;
}

interface ActionFunction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface AssignAction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface MachineConfig<TActor extends ProvidedActor> {
  entry?: ActionFunction<TActor>;
}

declare function assign<TActor extends ProvidedActor>(
  _: (spawn: (actor: TActor["src"]) => void) => {},
): AssignAction<TActor>;

type ToProvidedActor<TActors extends Record<string, UnknownActorLogic>> =
  Values<{
    [K in keyof TActors & string]: {
      src: K;
      logic: TActors[K];
    };
  }>;

declare function createMachineFactory<
  TActors extends Record<string, UnknownActorLogic>,
>(actors: TActors): {
  createMachine: <
    const TConfig extends MachineConfig<ToProvidedActor<TActors>>,
  >(
    config: TConfig,
  ) => void;
};

declare const counterLogic: ActorLogic<{ type: "INCREMENT" }>;

createMachineFactory({
  counter: counterLogic,
}).createMachine({
  entry: assign((spawn) => {
    spawn("counter");
    spawn("alarm");
    return {};
  }),
});
"#;
    let diags = relevant_strict_diagnostics(source);
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "return-context refresh should preserve the marker-property context for the nested callback. Got: {diags:#?}"
    );
    assert!(
        ts2345[0].contains("\"alarm\"") && ts2345[0].contains("\"counter\""),
        "the callback parameter should stay narrowed to the contextual actor source. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_argument_preserves_inferred_props() {
    let source = r#"
declare class Comp<P> {
    props: P;
    constructor(props: P);
}

type CompClass<P> = new (props: P) => Comp<P>;
declare function myHoc<P>(C: CompClass<P>): CompClass<P>;
type GenericProps<T> = { foo: number, stuff: T };
declare class GenericComp<T> extends Comp<GenericProps<T>> {}
declare class StringComp extends Comp<GenericProps<string>> {}

const GenericComp2 = myHoc(GenericComp);
const StringComp2 = myHoc(StringComp);
const madeString = new StringComp2({ foo: 1, stuff: "ok" });
const keepString: string = madeString.props.stuff;
const wrongString: number = madeString.props.stuff;
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "generic class constructor arguments should preserve their props inference through constructor HOFI. Got: {diags:#?}"
    );
    let ts2322_count = diagnostic_count(&diags, 2322);
    assert_eq!(
        ts2322_count, 1,
        "instantiating the returned constructor should preserve `stuff: string` and reject assignment to number exactly once. Got: {diags:#?}"
    );
}

#[test]
fn generic_static_factory_constructor_infers_method_type_parameter() {
    let source = r#"
class Container<T> {
    private value: T;
    constructor(value: T) { this.value = value; }

    static of<U>(value: U): Container<U> {
        return new Container(value);
    }
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "constructor inference inside a generic static method should preserve the method type parameter. Got: {diags:#?}"
    );
}

#[test]
fn generic_class_expression_method_contextualizes_callback_parameter() {
    let source = r#"
const Container = class<T> {
    constructor(public value: T) {}

    map<U>(fn: (v: T) => U): InstanceType<typeof Container<U>> {
        return null as any;
    }
};

const numContainer = new Container(42);
const checkNumber: number = numContainer.value;
const checkString: string = numContainer.value;
numContainer.map(n => n.toString());
numContainer.map((n: string) => n);
"#;
    let diags = relevant_strict_default_lib_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "generic class expression method should contextually type callback parameter from instantiated class type. Got: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2322 && message.contains("number")),
        "generic class expression constructor inference should preserve `value: number`. Got: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("string")),
        "generic class expression method should reject callback parameter annotations incompatible with number. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_options_infer_from_context_sensitive_object_member_return() {
    let source = r#"
declare class Connection {
    ok(): void;
}

declare class Pending<R> {
    promise: Promise<R>;
}

interface PoolOptions<R> {
    create: () => R | Promise<R>;
    destroy: (resource: R) => void;
    validate?: (resource: R) => boolean;
}

declare class Pool<R> {
    constructor(options: PoolOptions<R>);
    acquire(): Pending<R>;
    release(resource: R): void;
}

declare const tarn: {
    Pool: typeof Pool;
};

const pool = new tarn.Pool({
    create: async () => new Connection(),
    destroy: (connection) => {
        connection.ok();
    },
    validate: (connection) => true,
});

const keep: Pending<Connection> = pool.acquire();
const reject: Pending<string> = pool.acquire();
"#;
    let diags = relevant_strict_default_lib_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "generic constructor options should infer callback parameter types during Round 2. Got: {diags:#?}"
    );
    let ts2322_count = diagnostic_count(&diags, 2322);
    assert_eq!(
        ts2322_count, 1,
        "Pool should infer R = Connection from create(), accept Connection assignment, and reject string assignment exactly once. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_options_infer_through_method_signature_and_omit_spread() {
    let source = r#"
declare class Connection {
    ok(): void;
}

declare class Pending<R> {
    promise: Promise<R>;
}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

interface PoolOptions<R> {
    create(cb: (err: Error | null, resource: R) => void): any | (() => Promise<R>);
    destroy(resource: R): any;
    validate?(resource: R): boolean;
    min: number;
    max: number;
}

declare class Pool<R> {
    constructor(options: PoolOptions<R>);
    acquire(): Pending<R>;
}

declare const tarn: {
    options: Omit<PoolOptions<any>, "create" | "destroy" | "validate"> & {
        validateConnections?: false;
    };
    Pool: typeof Pool;
};

const { validateConnections, ...poolOptions } = tarn.options;

const pool: Pool<Connection> = new tarn.Pool({
    ...poolOptions,
    create: async () => new Connection(),
    destroy: async (connection) => {
        connection.ok();
    },
    validate:
        validateConnections === false
            ? undefined
            : (connection) => {
                connection.ok();
                return true;
            },
});

const keep: Pending<Connection> = pool.acquire();
"#;
    let diags = relevant_strict_default_lib_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 7006),
        "generic constructor options should contextually type callback parameters through method signatures and spreads. Got: {diags:#?}"
    );
    assert!(
        lacks_diagnostic_code(&diags, 2322),
        "Pool should infer R = Connection through method-style create and Omit spread. Got: {diags:#?}"
    );
}

#[test]
fn generic_constructor_argument_contextualizes_nested_discriminated_object_property() {
    let source = r#"
type RefinementCtx = { addIssue(message: string): void };
interface ZodTypeDef {}
type ZodTypeAny = ZodType<any, any, any>;
type input<T extends ZodType<any, any, any>> = T["_input"];
type output<T extends ZodType<any, any, any>> = T["_output"];
type ParseReturnType<T> =
    | { status: "valid"; value: T }
    | { status: "dirty"; value: T }
    | { status: "aborted" };

type RefinementEffect<T> = {
    type: "refinement";
    refinement: (arg: T, ctx: RefinementCtx) => any;
};
type TransformEffect<T> = {
    type: "transform";
    transform: (arg: T) => any;
};
type PreprocessEffect<T> = {
    type: "preprocess";
    transform: (arg: T) => any;
};
type Effect<T> = RefinementEffect<T> | TransformEffect<T> | PreprocessEffect<T>;

enum ZodFirstPartyTypeKind {
    ZodEffects = "ZodEffects",
}

interface ZodEffectsDef<T extends ZodTypeAny = ZodTypeAny> extends ZodTypeDef {
    schema: T;
    typeName: ZodFirstPartyTypeKind.ZodEffects;
    effect: Effect<any>;
}

abstract class ZodType<Output, Def extends ZodTypeDef = ZodTypeDef, Input = Output> {
    _output!: Output;
    _input!: Input;
    _def!: Def;

    abstract _parse(): ParseReturnType<Output>;

    constructor(def: Def) {}

    _refinement(refinement: RefinementEffect<Output>["refinement"]): ZodEffects<this> {
        return new ZodEffects({
            schema: this,
            typeName: ZodFirstPartyTypeKind.ZodEffects,
            effect: { type: "refinement", refinement },
        });
    }
}

class ZodEffects<
    T extends ZodTypeAny,
    Output = output<T>,
    Input = input<T>
> extends ZodType<Output, ZodEffectsDef<T>, Input> {
    _parse(): ParseReturnType<Output> {
        return null as any;
    }
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Effect<any>") && message.contains("type: string")
        }),
        "nested discriminated object literal in a constructor argument should inherit the Effect<any> context. Got: {diags:#?}"
    );
}

#[test]
fn contextual_generic_new_return_recovers_unresolved_constructor_type_params() {
    let source = r#"
type ParseReturnType<T> = { ok: true; value: T } | { ok: false };
interface BaseDef {}

abstract class Schema<Out, Def extends BaseDef = BaseDef, In = Out> {
    readonly _output!: Out;
    readonly _input!: In;
    readonly _def!: Def;
    abstract _parse(): ParseReturnType<Out>;
    constructor(def: Def) {}
}

type AnySchema = Schema<any, any, any>;
type Effect<T> = { type: "refinement"; refine: (arg: T) => unknown };

interface WrapperDef<S extends AnySchema = AnySchema> extends BaseDef {
    schema: S;
    effect: Effect<any>;
}

class Wrapper<
    S extends AnySchema,
    Out = S["_output"],
    In = S["_input"]
> extends Schema<Out, WrapperDef<S>, In> {
    _parse(): ParseReturnType<Out> {
        return null as never;
    }

    static make = <Source extends AnySchema>(
        schema: Source,
        effect: Effect<Source["_output"]>
    ): Wrapper<Source, Source["_output"]> => {
        return new Wrapper({ schema, effect });
    };
}

interface DecoratedDef<Item extends AnyCarrier = AnyCarrier> extends BaseDef {
    item: Item;
    hook: Hook<any>;
}

abstract class Carrier<Value, Def extends BaseDef = BaseDef, Raw = Value> {
    readonly value!: Value;
    readonly raw!: Raw;
    readonly def!: Def;
    abstract parse(): ParseReturnType<Value>;
    constructor(def: Def) {}
}

type AnyCarrier = Carrier<any, any, any>;
type Hook<T> = { run: (value: T) => unknown };

class Decorated<
    Item extends AnyCarrier,
    Value = Item["value"],
    Raw = Item["raw"]
> extends Carrier<Value, DecoratedDef<Item>, Raw> {
    parse(): ParseReturnType<Value> {
        return null as never;
    }

    static build = <Entity extends AnyCarrier>(
        item: Entity,
        hook: Hook<Entity["value"]>
    ): Decorated<Entity, Entity["value"]> => {
        return new Decorated({ item, hook });
    };
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().all(|(code, _)| *code != 2322 && *code != 2739),
        "contextual generic new returns should recover the enclosing application when constructor args leave class params unresolved. Got: {diags:#?}"
    );
}

#[test]
fn contextual_generic_new_return_keeps_different_constructor_base_mismatch() {
    let source = r#"
type ParseReturnType<T> = { ok: true; value: T } | { ok: false };
interface BaseDef {}

abstract class Schema<Out, Def extends BaseDef = BaseDef, In = Out> {
    readonly _output!: Out;
    readonly _input!: In;
    readonly _def!: Def;
    abstract _parse(): ParseReturnType<Out>;
    constructor(def: Def) {}
}

type AnySchema = Schema<any, any, any>;
type Effect<T> = { type: "refinement"; refine: (arg: T) => unknown };

interface WrapperDef<S extends AnySchema = AnySchema> extends BaseDef {
    schema: S;
    marker: "wrapper";
    effect: Effect<any>;
}

interface OtherDef<S extends AnySchema = AnySchema> extends BaseDef {
    other: S;
    marker: "other";
}

class Wrapper<
    S extends AnySchema,
    Out = S["_output"],
    In = S["_input"]
> extends Schema<Out, WrapperDef<S>, In> {
    _parse(): ParseReturnType<Out> {
        return null as never;
    }
}

class Other<
    S extends AnySchema,
    Out = S["_output"],
    In = S["_input"]
> extends Schema<Out, OtherDef<S>, In> {
    _parse(): ParseReturnType<Out> {
        return null as never;
    }
}

function wrong<Source extends AnySchema>(
    schema: Source,
    effect: Effect<Source["_output"]>
): Other<Source, Source["_output"]> {
    return new Wrapper({ schema, marker: "wrapper", effect });
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "contextual recovery must not hide a different constructor application base. Got: {diags:#?}"
    );
}

#[test]
fn conflicting_contextual_instantiation_keeps_enclosing_return_type_param() {
    let source = r#"
declare function accept<R>(fn: (a: string, b: number) => R): R;

function outer<X>(source: <T>(a: T, b: T) => X) {
    const out = accept(source);
    const keep: X = out;
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(_code, message)| message.contains("unknown")),
        "contextual conflict handling must not rewrite enclosing return type parameters to unknown. Got: {diags:#?}"
    );
}

#[test]
fn generic_callback_parameter_does_not_override_concrete_array_inference() {
    let source = r#"
export function keyOf<a>(value: { key: a; }): a {
    return value.key;
}
declare class Date {}
export interface Data {
    key: number;
    value: Date;
}

var data: Data[] = [];
declare function toKeys<a>(values: a[], toKey: (value: a) => string): string[];

toKeys(data, keyOf);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("Data[]")),
        "the concrete array argument should own `a`; the callback should be checked against `(value: Data) => string`. Got: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("(value: Data) => string")),
        "generic callback return mismatch should be reported at the callback parameter. Got: {diags:#?}"
    );
}

#[test]
fn contextual_parameter_self_referential_no_excess_constraint_no_false_ts2345() {
    let source = r#"
type NoExcessProperties<T, U> = T & {
  readonly [K in Exclude<keyof U, keyof T>]: never;
};

interface Effect<out A> {
  readonly EffectTypeId: {
    readonly _A: (_: never) => A;
  };
}

declare function pipe<A, B>(a: A, ab: (a: A) => B): B;

interface RepeatOptions<A> {
  until?: (_: A) => boolean;
}

declare const repeat: {
  <O extends NoExcessProperties<RepeatOptions<A>, O>, A>(
    options: O,
  ): (self: Effect<A>) => Effect<A>;
};

pipe(
  {} as Effect<boolean>,
  repeat({
    until: (x) => {
      return x;
    },
  }),
);
"#;
    let diags = relevant_lib_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "self-referential NoExcessProperties constraint should not raise false TS2345. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_nested_generic_spread_inference() {
    let source = r#"
declare function wrap<X>(x: X): { x: X };
declare function call<A extends unknown[], T>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "nested generic spread inference should not produce TS2345. Got: {diags:#?}"
    );
}

