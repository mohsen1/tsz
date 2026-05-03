//! Tests for generic type parameter handling and TS2322 errors

#[test]
fn test_generic_type_argument_satisfies_constraint() {
    let source = r#"
function identity<T extends number>(x: T): T {
    return x;
}

const result1 = identity(42); // OK - 42 is number
const result2 = identity("string"); // TS2322: "string" doesn't satisfy "extends number"
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // Should emit TS2345 for string argument not assignable to number parameter.
    // tsc reports TS2345 ("Argument of type 'string' is not assignable to parameter
    // of type 'number'") because the constraint violation is at the argument level.
    let ts2345_count = diags.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error, got {ts2345_count}"
    );
}

#[test]
fn test_readonly_generic_parameter_no_ts2339() {
    let source = r"
interface Props {
    onFoo?(value: string): boolean;
}

type Readonly<T> = { readonly [K in keyof T]: T[K] };

function test<P extends Props>(props: Readonly<P>) {
    props.onFoo;
}
";

    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2339_count = diags.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 for Readonly<P> property access, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_generic_with_default_type_parameter() {
    let source = r#"
function foo<T = string>(x: T): T {
    return x;
}

const result1 = foo("hello"); // OK - T inferred as string
const result2 = foo(42); // OK - T inferred as number
const result3 = foo<number>(true); // TS2345: boolean not assignable to number
const result4 = foo<number>([]); // TS2345: never[] not assignable to number
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // tsc emits TS2345 for argument type mismatches against explicit type params
    let ts2345_count = diags.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 2,
        "Expected at least 2 TS2345 errors for explicit type arg mismatches, got {ts2345_count}"
    );
}

#[test]
fn test_empty_default_satisfies_optional_mapped_constraint() {
    let source = r#"
type Optional<T> = { [K in keyof T]?: T[K] };
type Options = {
    depth: number;
    anyArrayIndexAccessor: string;
};

type Paths<OverridePathOptions extends Optional<Options> = {}> = OverridePathOptions;
"#;
    let codes = crate::test_utils::check_source_codes(source);

    assert!(
        !codes.contains(&2344),
        "Expected no TS2344 for {{}} default against optional mapped object constraint, got {codes:?}"
    );
}

#[test]
fn test_empty_default_still_rejected_for_optional_mapped_primitive() {
    let source = r#"
type Optional<T> = { [K in keyof T]?: T[K] };
type Bad<T extends Optional<number> = {}> = T;
"#;
    let codes = crate::test_utils::check_source_codes(source);

    assert!(
        codes.contains(&2344),
        "Expected TS2344 for {{}} default against optional mapped primitive constraint, got {codes:?}"
    );
}

#[test]
fn test_required_mapped_constraint_accepts_required_source_and_defaults() {
    let source = r#"
type Required<T> = { [K in keyof T]-?: T[K] };

type CreateTypeOptions<
  Options extends Required<Options>,
  DefaultOptions extends Required<Options>,
> = {
  [Key in keyof Options]: DefaultOptions[Key];
};

type PathsOptions = {
  depth: number;
  anyArrayIndexAccessor: string;
};

type DefaultPathsOptions = {
  depth: 7;
  anyArrayIndexAccessor: "0";
};

type Paths = CreateTypeOptions<PathsOptions, DefaultPathsOptions>;
"#;
    let codes = crate::test_utils::check_source_codes(source);

    assert!(
        !codes.contains(&2344),
        "Expected no TS2344 for required object types against Required<Options>, got {codes:?}"
    );
}

#[test]
fn test_required_mapped_constraint_still_rejects_missing_default_property() {
    let source = r#"
type Required<T> = { [K in keyof T]-?: T[K] };

type CreateTypeOptions<
  Options extends Required<Options>,
  DefaultOptions extends Required<Options>,
> = {
  [Key in keyof Options]: DefaultOptions[Key];
};

type PathsOptions = {
  depth: number;
  anyArrayIndexAccessor: string;
};

type BadDefaultPathsOptions = {
  depth: 7;
};

type Paths = CreateTypeOptions<PathsOptions, BadDefaultPathsOptions>;
"#;
    let codes = crate::test_utils::check_source_codes(source);

    assert!(
        codes.contains(&2344),
        "Expected TS2344 for defaults missing required property, got {codes:?}"
    );
}

#[test]
fn test_type_alias_type_parameter_constraint_used_for_nested_type_arguments() {
    let source = r#"
type Partial<T> = { [K in keyof T]?: T[K] };
type Required<T> = { [K in keyof T]-?: T[K] };

type CreateTypeOptions<
  Options extends Required<Options>,
  OverrideOptions extends Partial<Options>,
  DefaultOptions extends Required<Options>,
> = {
  [Key in keyof Options]: OverrideOptions[Key] extends Options[Key] ? OverrideOptions[Key] : DefaultOptions[Key];
};

type PathsOptions = {
  depth: number;
  anyArrayIndexAccessor: string;
};

type DefaultPathsOptions = {
  depth: 7;
  anyArrayIndexAccessor: "0";
};

type Paths<OverridePathOptions extends Partial<PathsOptions> = {}> =
  CreateTypeOptions<PathsOptions, OverridePathOptions, DefaultPathsOptions>;
"#;
    let codes = crate::test_utils::check_source_codes(source);

    assert!(
        !codes.contains(&2344),
        "Expected no TS2344 when a type alias type parameter carries the matching Partial constraint, got {codes:?}"
    );
}

#[test]
fn test_generic_class_type_parameter_constraint() {
    let source = r#"
class Container<T extends number> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
}

const c1 = new Container(42); // OK
const c2 = new Container("hello"); // TS2345: string not assignable to number
const c3 = new Container<number>(true); // TS2345: boolean not assignable to number
const c4 = new Container<number>({}); // TS2345: {} not assignable to number
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // tsc emits TS2345 for all 3 bad arguments (c2, c3, c4)
    let ts2345_count = diags.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 3,
        "Expected at least 3 TS2345 errors for type mismatches, got {ts2345_count}"
    );
}

#[test]
fn test_generic_contravariance() {
    let source = r"
interface Producer<T> {
    produce(): T;
}

interface Consumer<T> {
    consume(value: T): void;
}

// Covariance: Producer<Derived> is assignable to Producer<Base>
interface Base {}
interface Derived extends Base {}

function useProducer(producer: Producer<Base>): void {
    producer.produce();
}

const prodBase: Producer<Base> = { produce: () => new Base() };
const prodDerived: Producer<Derived> = { produce: () => new Derived() };

useProducer(prodBase);     // OK
useProducer(prodDerived); // OK - covariant

// Contravariance: Consumer<Derived> is assignable to Consumer<Base>
function useConsumer(consumer: Consumer<Base>): void {
    consumer.consume(new Base());
}

const consBase: Consumer<Base> = { consume: (value: Base) => {} };
const consDerived: Consumer<Derived> = { consume: (value: Derived) => {} };

useConsumer(consBase);     // OK
useConsumer(consDerived); // TS2322 if invariance (should error)
";
    let _diags = crate::test_utils::check_source_diagnostics(source);
    // This test checks variance, which may or may not error
}

#[test]
fn test_generic_function_type_inference() {
    let source = r#"
function pair<T, U>(first: T, second: U): [T, U] {
    return [first, second];
}

const result1 = pair(1, "hello"); // OK - T inferred as number, U as string
const result2 = pair<number, string>(1, "hello"); // OK - explicit type args
const result3 = pair<number, number>(1, "hello"); // TS2345: string not assignable to number
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // tsc emits TS2345 for argument type mismatch
    let ts2345_count = diags.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for type mismatch, got {ts2345_count}"
    );
}

#[test]
fn test_no_type_arguments_needed_for_inferred_generics() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}

const result1 = identity(42); // OK - T inferred as number
const result2 = identity("hello"); // OK - T inferred as string
const result3 = identity<number>(42); // OK - explicit number
const result4 = identity<string>(42); // TS2345: number not assignable to string
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // tsc emits TS2345 for argument type mismatch against explicit type param
    let ts2345_count = diags.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for type mismatch, got {ts2345_count}"
    );
}

#[test]
fn test_multiple_type_parameters_with_defaults() {
    let source = r#"
function foo<T = number, U = string>(x: T, y: U): [T, U] {
    return [x, y];
}

const r1 = foo(1, "hello"); // OK - T inferred number, U inferred string
const r2 = foo<string, boolean>(true, false); // TS2345: boolean not assignable to string
const r3 = foo<number, number>(1, 2); // OK
const r4 = foo<number, boolean>(1, "hello"); // TS2345: string not assignable to boolean
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // tsc emits TS2345 for argument type mismatches (r2 and r4)
    let ts2345_count = diags.iter().filter(|d| d.code == 2345).count();
    assert!(
        ts2345_count >= 2,
        "Expected at least 2 TS2345 errors for type mismatches, got {ts2345_count}"
    );
}

#[test]
fn test_ts2313_direct_circular_constraint() {
    let source = r"
class C<T extends T> { }
function f<T extends T>() { }
interface I<T extends T> { }
";
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2313_count = diags.iter().filter(|d| d.code == 2313).count();
    assert_eq!(
        ts2313_count,
        3,
        "Expected 3 TS2313 errors for direct circular constraints, got {ts2313_count}. Diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2313_indirect_circular_constraint() {
    let source = r"
class C<U extends T, T extends U> { }
class C2<T extends U, U extends V, V extends T> { }
";
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2313_count = diags.iter().filter(|d| d.code == 2313).count();
    assert_eq!(
        ts2313_count,
        5,
        "Expected 5 TS2313 errors for indirect circular constraints (2 for C, 3 for C2), got {ts2313_count}. Diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2313_no_false_positive_for_non_circular() {
    // S extends Foo<S> is NOT circular - the constraint wraps S in Foo<>
    let source = r"
type Foo<T> = [T] extends [number] ? {} : {};
function foo<S extends Foo<S>>() {}
";
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2313_count = diags.iter().filter(|d| d.code == 2313).count();
    assert_eq!(
        ts2313_count,
        0,
        "Expected 0 TS2313 errors for non-circular constraint Foo<S>, got {ts2313_count}. Diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2313_non_cyclic_chain_not_flagged() {
    // U extends T, T extends V, V extends T: U is NOT part of the cycle
    let source = r"
class D<U extends T, T extends V, V extends T> { }
";
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2313_diags: Vec<_> = diags.iter().filter(|d| d.code == 2313).collect();

    // T and V form a cycle, but U just points to the cycle - should not be flagged
    assert_eq!(
        ts2313_diags.len(),
        2,
        "Expected 2 TS2313 errors (T and V), not U. Got: {:?}",
        ts2313_diags
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// Test that array literal elements preserve literal types during generic call inference.
///
/// When `['foo', 'bar']` is passed to a function like `f<K extends string>(list: K[]): K`,
/// K should be inferred as `"foo" | "bar"` (not `string`). This requires the array literal
/// to preserve literal element types instead of widening them via BCT.
///
/// Repro from TypeScript's isomorphicMappedTypeInference test (#29765).
#[test]
fn test_generic_call_preserves_array_literal_types() {
    let source = r#"
function getProps<T, K extends keyof T>(obj: T, list: K[]): Pick<T, K> {
    return {} as any;
}
const myAny: any = {};
const o2: { foo: any; bar: any } = getProps(myAny, ['foo', 'bar']);
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // Should have NO errors — Pick<any, "foo" | "bar"> = { foo: any; bar: any }
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "Expected no TS2322 errors for Pick<any, 'foo' | 'bar'> assignment, got {}: {:?}",
        errors.len(),
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn test_promise_all_spread_any_does_not_produce_false_ts1360() {
    let source = r#"
declare function getT<T>(): T;

Promise.all([getT<string>(), ...getT<any>()]).then((result) => {
  const tail = result.slice(1);
  tail satisfies string[];
});
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts1360_errors: Vec<_> = diags.iter().filter(|d| d.code == 1360).collect();
    assert!(
        ts1360_errors.is_empty(),
        "Expected no TS1360 for Promise.all spread-any tail satisfies check, got: {:?}",
        ts1360_errors
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// When a type argument has no properties in common with an all-optional
/// constraint (a "weak type"), tsc emits TS2559 instead of TS2344.
/// Regression test for incorrectNumberOfTypeArgumentsDuringErrorReporting.ts.
#[test]
fn test_weak_type_constraint_emits_ts2559() {
    let source = r#"
interface ObjA {
  y?: string;
}

interface ObjB { [key: string]: any }

interface Opts<A, B> { a: A; b: B }

const fn2 = <
  A extends ObjA,
  B extends ObjB = ObjB
>(opts: Opts<A, B>): string => 'Z';

interface MyObjA {
  x: string;
}

fn2<MyObjA>({
  a: { x: 'X', y: 'Y' },
  b: {},
});
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2559_count = diags.iter().filter(|d| d.code == 2559).count();
    assert!(
        ts2559_count >= 1,
        "Expected TS2559 (weak type: no common properties) for MyObjA vs ObjA constraint, got {} TS2559 errors. All errors: {:?}",
        ts2559_count,
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Should NOT emit TS2344 (constraint not satisfied) — TS2559 is more specific
    let ts2344_count = diags.iter().filter(|d| d.code == 2344).count();
    assert_eq!(
        ts2344_count, 0,
        "Expected no TS2344 when TS2559 (weak type) applies, got {ts2344_count}"
    );
}

#[test]
fn test_type_parameter_default_with_enclosing_type_arguments_is_not_circular() {
    let source = r#"
interface SelfRef<T = SelfRef> {}

interface ExtendableConfig<
  Options = any,
  Config extends ExtensionConfig<Options> | ExtendableConfig<Options> = ExtendableConfig<Options, any>
> {}

interface ExtensionConfig<Options = any>
  extends ExtendableConfig<Options, ExtensionConfig<Options>>
{}

interface ExplicitArgDefault<T = SelfRef<number>> {}
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    let ts2716_count = diags.iter().filter(|d| d.code == 2716).count();
    let all_errors = diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        ts2716_count, 1,
        "Expected only the raw circular-default case to trigger TS2716, got {ts2716_count}. All diagnostics: {all_errors:?}"
    );
}

#[test]
fn test_type_parameter_default_constraints_validate_type_parameter_references() {
    let source = r#"
declare function f04<T extends string, U extends number = T>(): void;
declare function f05<T, U extends number = T>(): void;
declare function f06<T, U extends T = number>(): void;
interface i06<T extends string, U extends number = T> { }
interface i07<T, U extends number = T> { }
interface i08<T, U extends T = number> { }
"#;

    let diagnostics = crate::test_utils::check_source_diagnostics(source);
    let ts2344_messages = diagnostics
        .iter()
        .filter(|d| d.code == 2344)
        .map(|d| d.message_text.as_str())
        .collect::<Vec<_>>();

    let default_type_param_vs_number = ts2344_messages
        .iter()
        .filter(|message| message.contains("Type 'T' does not satisfy the constraint 'number'."))
        .count();
    let number_vs_default_type_param = ts2344_messages
        .iter()
        .filter(|message| message.contains("Type 'number' does not satisfy the constraint 'T'."))
        .count();

    assert_eq!(
        ts2344_messages.len(),
        6,
        "Expected six TS2344 default-constraint diagnostics, got {ts2344_messages:?}"
    );
    assert_eq!(
        default_type_param_vs_number, 4,
        "Expected four TS2344 diagnostics for T defaulting into number constraints, got {ts2344_messages:?}"
    );
    assert_eq!(
        number_vs_default_type_param, 2,
        "Expected two TS2344 diagnostics for number defaulting into T constraints, got {ts2344_messages:?}"
    );
}

#[test]
fn test_type_parameter_default_constraints_skip_self_and_derived_type_parameter_references() {
    let source = r#"
type SelfDefault<T extends string = T> = { value: T };

interface Settable<T, V> {
    set(value: V): T;
}
interface Identity<V> extends Settable<Identity<V>, V> { }
interface Test1<V, T extends Settable<T, V> = Identity<V>> { }

type Prefixes = "foo" | "bar";
type AllPrefixData = "foo:baz" | "bar:baz";
type PrefixData<P extends Prefixes> = `${P}:baz`;
interface ITest<P extends Prefixes, E extends AllPrefixData = PrefixData<P>> { }
"#;

    let diagnostics = crate::test_utils::check_source_diagnostics(source);
    let ts2344_messages = diagnostics
        .iter()
        .filter(|d| d.code == 2344)
        .map(|d| d.message_text.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts2344_messages.is_empty(),
        "Expected no TS2344 diagnostics for self or derived generic defaults, got {ts2344_messages:?}"
    );
}

/// TS2744 position must anchor at the offending forward-referenced identifier
/// inside the type-parameter default, not at the start of the entire default
/// expression. tsc points at the identifier itself.
///
/// Regression test for `subclassThisTypeAssignable01` conformance test.
#[test]
fn test_type_parameter_default_forward_ref_anchors_on_identifier() {
    let source = r#"
interface Vnode<Attrs, State extends Lifecycle<Attrs, State> = Lifecycle<Attrs, State>> { tag: number }
interface Lifecycle<Attrs, State> { x: number }
"#;
    let diagnostics = crate::test_utils::check_source_diagnostics(source);
    let ts2744 = diagnostics
        .iter()
        .filter(|d| d.code == 2744)
        .collect::<Vec<_>>();
    assert_eq!(
        ts2744.len(),
        1,
        "Expected exactly one TS2744 forward-reference, got: {diagnostics:?}"
    );
    let diag = ts2744[0];
    // The offending identifier is the second `State` inside the default
    // `Lifecycle<Attrs, State>`. Locate the second occurrence of `State`
    // that follows the `=` in the default position.
    let bytes = source.as_bytes();
    let eq_pos = source.find('=').expect("default `=` present");
    let after_eq = &source[eq_pos..];
    // First `State` after `=` is inside `Lifecycle<Attrs, State>`.
    let rel = after_eq.find("State").expect("State identifier after `=`");
    let expected_pos = (eq_pos + rel) as u32;
    assert_eq!(
        diag.start, expected_pos,
        "Expected TS2744 to anchor at the forward-referenced `State` identifier (pos={expected_pos}), got start={} in {bytes:?}",
        diag.start
    );
}

/// Generic function references passed as callback arguments should be properly
/// instantiated, not cause the earlier arguments to be deferred from inference.
/// Regression test for: `map("", identity)` incorrectly inferred T as `unknown`
/// instead of `string` because the deferral logic skipped the string argument.
#[test]
fn test_generic_function_ref_as_callback_does_not_defer_earlier_arg_inference() {
    let source = r#"
declare function map<T, U>(array: T, func: (x: T) => U): U;
declare function identity<V>(y: V): V;
var s: string;

// All of these should be fine: T=string inferred from first arg,
// identity instantiated as (y: string) => string, U=string.
s = map("", identity);

// Dotted access
var dottedIdentity = { x: identity };
s = map("", dottedIdentity.x);

// Parenthesized
s = map("", (identity));
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        errors.is_empty(),
        "Expected no TS2322/TS2345 errors for generic function ref as callback, got: {errors:?}"
    );
}

#[test]
fn test_conflicting_direct_candidates_with_callback_emit_ts2345() {
    let source = r#"
declare function g<T>(a: T, b: T, c: (t: T) => T): T;

g("", 3, a => a);
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected one TS2345 for the conflicting second argument, got: {diags:?}"
    );
    assert!(
        (ts2345[0].message_text.contains("'3'") || ts2345[0].message_text.contains("'number'"))
            && (ts2345[0].message_text.contains("'\"\"'")
                || ts2345[0].message_text.contains("'string'")),
        "Expected TS2345 to compare the second argument against the first string candidate, got: {:?}",
        ts2345[0]
    );
}

#[test]
fn test_forward_referencing_type_param_defaults_no_spurious_ts2345() {
    // When type parameter defaults forward-reference later-declared type parameters
    // (flagged by TS2744), the defaults should resolve to error for type-checking
    // purposes, matching tsc's fillMissingTypeArguments behavior. This should NOT
    // produce spurious TS2345 errors on call arguments.
    let source = r#"
interface A { a: number; }
interface B { b: number; }
interface C { c: number; }

declare const a: A;
declare const b: B;

// U defaults to V (forward reference), V defaults to C
declare function f14<T, U = V, V = C>(a?: T, b?: U, c?: V): [T, U, V];

// These should NOT produce TS2345
f14<A>(a, b);
f14<A>(a, b, b);

// Mutually referencing defaults (U = V, V = U)
declare function f16<T, U = V, V = U>(a?: T, b?: U, c?: V): [T, U, V];

// These should NOT produce TS2345
f16<A>(a, b);
f16<A>(a, b, b);

// Forward reference in union default
declare function f18<T, U = V, V = U | C>(a?: T, b?: U, c?: V): [T, U, V];

f18<A>(a, b);
f18<A>(a, b, b);
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);

    // Should only have TS2744 (forward reference warnings), no TS2345
    let ts2345_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.clone())
        .collect();
    assert!(
        ts2345_errors.is_empty(),
        "Expected no TS2345 errors for forward-referencing type param defaults, got: {ts2345_errors:?}"
    );

    // Should have TS2744 errors for the forward references
    let ts2744_count = diags.iter().filter(|d| d.code == 2744).count();
    assert!(
        ts2744_count > 0,
        "Expected TS2744 errors for forward-referencing defaults"
    );
}

/// When type arguments are partially supplied and remaining params have defaults
/// that are fully resolved (no remaining type param references), the defaults
/// should be applied eagerly. `f12<number>("a")` should error because U defaults
/// to T=number and "a" (string) is not assignable to number.
#[test]
fn test_partial_type_args_apply_resolved_defaults() {
    let source = r#"
declare function f12<T, U = T>(a?: U): void;
f12<number>("a");  // U = T = number; "a" is string -> TS2345
"#;
    let codes = crate::test_utils::check_source_codes(source);
    assert!(
        codes.contains(&2345),
        "Expected TS2345: 'string' not assignable to 'number' when U defaults to T=number. Got: {codes:?}"
    );
}

/// TS2315: Type parameters used with type arguments should error.
/// A type parameter like `U` is not generic — it cannot accept type arguments.
/// For example, `U<string>` is invalid when `U` is a type parameter.
#[test]
fn test_ts2315_type_parameter_with_type_arguments() {
    let source = r#"
function f<U>() {
    var v: U<string>;
}
"#;
    let codes = crate::test_utils::check_source_codes(source);
    assert!(
        codes.contains(&2315),
        "Expected TS2315 for type parameter 'U' used with type arguments. Got: {codes:?}"
    );
}

/// TS2315 should NOT be emitted for actual generic types.
#[test]
fn test_no_ts2315_for_generic_type() {
    let source = r#"
interface Box<T> { value: T; }
var v: Box<string>;
"#;
    let codes = crate::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2315),
        "Should NOT emit TS2315 for generic type 'Box<string>'. Got: {codes:?}"
    );
}

/// TS2345 must fire when a value typed by a type parameter is passed to a
/// parameter typed by a *different* unrelated type parameter — neither
/// parameter has a constraint that proves the relation, so the assignment
/// is unsound (e.g. `T = string`, `U = number`).
///
/// Mirrors `genericCallbackInvokedInsideItsContainingFunction1.ts` line 12,
/// `var r12 = f(y);` where `f: (v: T) => U` and `y: U`.
#[test]
fn test_unrelated_type_param_argument_emits_ts2345() {
    let source = r#"
function foo<T, U>(x: T, y: U, f: (v: T) => U) {
    f(y);
}
"#;
    let diags = crate::test_utils::check_source_diagnostics(source);
    let ts2345 = diags
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.clone())
        .collect::<Vec<_>>();
    assert!(
        !ts2345.is_empty(),
        "Expected TS2345 for U-typed argument passed to T-typed parameter. \
         Got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        ts2345
            .iter()
            .any(|m| m.contains("'U'") && m.contains("'T'")),
        "Expected TS2345 message naming both 'U' and 'T'. Got: {ts2345:?}",
    );
}

/// Renaming the bound names (P/Q instead of T/U) must not change the
/// behaviour — the fix is structural, not name-based.
#[test]
fn test_unrelated_type_param_argument_emits_ts2345_renamed() {
    let source = r#"
function foo<P, Q>(x: P, y: Q, f: (v: P) => Q) {
    f(y);
}
"#;
    let codes = crate::test_utils::check_source_codes(source);
    assert!(
        codes.contains(&2345),
        "Expected TS2345 for unrelated type-param assignment with names P/Q. \
         Got: {codes:?}",
    );
}

/// Reflexive assignment of the *same* type parameter must remain valid.
#[test]
fn test_same_type_param_argument_no_ts2345() {
    let source = r#"
function foo<T>(x: T, f: (v: T) => void) {
    f(x);
}
"#;
    let codes = crate::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2345),
        "Reflexive T->T must not emit TS2345. Got: {codes:?}",
    );
}

/// When U *is* constrained to be a subtype of T, the assignment is sound and
/// no TS2345 should fire. This guards against regressing the constrained
/// case while we tighten the unconstrained case.
#[test]
fn test_constrained_type_param_argument_no_ts2345() {
    let source = r#"
function foo<T, U extends T>(x: T, y: U, f: (v: T) => void) {
    f(y);
}
"#;
    let codes = crate::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2345),
        "U extends T must allow U->T without TS2345. Got: {codes:?}",
    );
}
