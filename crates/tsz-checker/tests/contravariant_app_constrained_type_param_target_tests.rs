//! `same_base_application_to_constrained_type_param_target` must skip
//! contravariant positions.
//!
//! Structural rule: when two Application types share a base and the target's
//! arg is a type parameter `U` whose constraint equals (or is assignable to)
//! the source's arg `X`, the helper rejects the assignment up-front — sound
//! for COVARIANT or INVARIANT positions because `App<X>` may carry data
//! shapes that narrower instantiations of `U` cannot accept. But the same
//! rejection is unsound for CONTRAVARIANT positions: in that orientation,
//! `App<X> <: App<U extends X>` is exactly what contravariance permits, and
//! the variance-aware fast path immediately downstream is responsible for
//! accepting it.
//!
//! Concrete consequence: `conditionalTypes2.ts` function `f2` —
//! `interface Contravariant<T> { foo: T extends string ? keyof T : number }`
//! — emitted a spurious second TS2322 on `b = a` because this helper
//! short-circuited the variance check before contravariance could fire.

use tsz_checker::test_utils::{
    DiagnosticShape, assert_diagnostic_shape, assert_diagnostic_shapes_exactly,
    check_source_diagnostics, check_source_with_libs, load_compiled_lib_files,
};
use tsz_checker::{context::CheckerOptions, diagnostics::Diagnostic};
use tsz_common::common::ScriptTarget;

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn check_es2015_promise_source(source: &str) -> Vec<Diagnostic> {
    let libs = load_compiled_lib_files(&["lib.es5.d.ts", "lib.es2015.promise.d.ts"]);
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn check_esnext_weakref_source(source: &str) -> Vec<Diagnostic> {
    let libs = load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
        "lib.es2016.array.include.d.ts",
        "lib.es2016.intl.d.ts",
        "lib.es2017.arraybuffer.d.ts",
        "lib.es2017.date.d.ts",
        "lib.es2017.intl.d.ts",
        "lib.es2017.object.d.ts",
        "lib.es2017.sharedmemory.d.ts",
        "lib.es2017.string.d.ts",
        "lib.es2017.typedarrays.d.ts",
        "lib.es2018.asyncgenerator.d.ts",
        "lib.es2018.asynciterable.d.ts",
        "lib.es2018.intl.d.ts",
        "lib.es2018.promise.d.ts",
        "lib.es2018.regexp.d.ts",
        "lib.es2019.array.d.ts",
        "lib.es2019.intl.d.ts",
        "lib.es2019.object.d.ts",
        "lib.es2019.string.d.ts",
        "lib.es2019.symbol.d.ts",
        "lib.es2020.bigint.d.ts",
        "lib.es2020.date.d.ts",
        "lib.es2020.intl.d.ts",
        "lib.es2020.number.d.ts",
        "lib.es2020.promise.d.ts",
        "lib.es2020.sharedmemory.d.ts",
        "lib.es2020.string.d.ts",
        "lib.es2020.symbol.wellknown.d.ts",
        "lib.es2021.intl.d.ts",
        "lib.es2021.promise.d.ts",
        "lib.es2021.string.d.ts",
        "lib.es2021.weakref.d.ts",
        "lib.es2022.array.d.ts",
        "lib.es2022.error.d.ts",
        "lib.es2022.intl.d.ts",
        "lib.es2022.object.d.ts",
        "lib.es2022.regexp.d.ts",
        "lib.es2022.string.d.ts",
        "lib.es2023.array.d.ts",
        "lib.es2023.collection.d.ts",
        "lib.es2023.intl.d.ts",
        "lib.es2024.arraybuffer.d.ts",
        "lib.es2024.collection.d.ts",
        "lib.es2024.object.d.ts",
        "lib.es2024.promise.d.ts",
        "lib.es2024.regexp.d.ts",
        "lib.es2024.sharedmemory.d.ts",
        "lib.es2024.string.d.ts",
        "lib.es2025.collection.d.ts",
        "lib.esnext.array.d.ts",
        "lib.esnext.collection.d.ts",
        "lib.esnext.decorators.d.ts",
        "lib.esnext.disposable.d.ts",
        "lib.esnext.error.d.ts",
        "lib.esnext.intl.d.ts",
        "lib.es2025.iterator.d.ts",
        "lib.es2025.promise.d.ts",
        "lib.esnext.sharedmemory.d.ts",
        "lib.esnext.temporal.d.ts",
        "lib.esnext.typedarrays.d.ts",
    ]);
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

#[test]
fn contravariant_application_to_constrained_param_target_passes() {
    // The conformance source pattern. With B extends A, contravariance lets
    // `Contravariant<A>` be assigned to `Contravariant<B>` (b = a) but not
    // the reverse (a = b — that's the lone TS2322 tsc 6.0.3 reports).
    let source = r#"
interface Contravariant<T> {
    foo: T extends string ? keyof T : number;
}
function f2<A, B extends A>(a: Contravariant<A>, b: Contravariant<B>) {
    a = b;  // Error
    b = a;  // OK
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[DiagnosticShape::code(2322).at(6, 5).with_message_fragment(
            "Type 'Contravariant<B>' is not assignable to type 'Contravariant<A>'.",
        )],
    );
}

#[test]
fn contravariant_via_explicit_in_annotation_passes() {
    // Explicit `in T` declares contravariance regardless of body shape.
    // `Contra<A> <: Contra<B>` when B extends A — the canonical
    // contravariant function-parameter case.
    let source = r#"
interface Contra<in T> {
    foo: (x: T) => void;
}
function f<A, B extends A>(ca: Contra<A>, cb: Contra<B>) {
    ca = cb;  // Error: Contra<B> not assignable to Contra<A>
    cb = ca;  // OK: contravariance permits Contra<A> -> Contra<B>
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    let ts2322_count = codes.iter().filter(|c| **c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Exactly one TS2322 expected (the wrong direction). Codes: {codes:?}"
    );
}

#[test]
fn conditional_alias_callback_param_keeps_variance_acceptance() {
    // Repro from `conditionalTypes2.ts` / TypeScript #33568. The expected
    // callback parameter is `Foo3<T>` while the provided callback accepts
    // `Foo3<string>`. During callback comparison the relation checks
    // `Foo3<T>` against `Foo3<string>`; because the source application still
    // carries a free type parameter, the conditional alias must remain eligible
    // for the variance path rather than falling through to a structural
    // false-positive override/callback diagnostic.
    let source = r#"
declare function ff(x: Foo3<string>): void;
declare function gg<T>(f: (x: Foo3<T>) => void): void;
type Foo3<T> = T extends number ? { n: T } : { x: T };
gg(ff);
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    assert!(
        !codes.contains(&2416) && !codes.contains(&2322) && !codes.contains(&2345),
        "expected callback assignment to stay clean. Codes: {codes:?}"
    );
}

#[test]
fn nested_conditional_alias_callback_param_keeps_variance_acceptance() {
    // Nested repro shape from `conditionalTypes2.ts` / TypeScript #33568.
    // The public relation query must not treat the variance prepass rejection
    // as definitive when a conditional alias body forwards through wrapped
    // applications that still carry type parameters.
    let source = r#"
declare function consume(response: RootResponse<string>): void;
declare function register<Response>(callback: Callback<Response>): void;

interface Callback<Response> {
    (response: RootResponse<Response>): void;
}

type RootResponse<Response> =
    Response extends RecordLike ? RecordResponse<Response> : ValueResponse<Response>;

interface RecordLike {
    readonly Id: string;
}

declare type RecordResponse<T extends RecordLike> = ValueResponse<T> & {
    sendRecord(): void;
};

declare type ValueResponse<T> = {
    sendValue(name: keyof PropertiesOfType<T, string>): void;
};

declare type PropertyNamesOfType<T, RestrictToType> = {
    [PropertyName in Extract<keyof T, string>]: T[PropertyName] extends RestrictToType ? PropertyName : never
}[Extract<keyof T, string>];

declare type PropertiesOfType<T, RestrictToType> = Pick<
    T,
    PropertyNamesOfType<Required<T>, RestrictToType>
>;

register(consume);
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    assert!(
        !codes.contains(&2416) && !codes.contains(&2322) && !codes.contains(&2345),
        "expected nested callback assignment to stay clean. Codes: {codes:?}"
    );
}

#[test]
fn recursive_tuple_conditional_with_free_number_params_reports_declared_mismatches() {
    // `recursiveConditionalTypes.ts` reports both tuple-length assignments.
    // The diagnostic stays anchored on the declared conditional alias
    // applications instead of falling through to a reduced tuple detail.
    let source = r#"
type TupleOf<T, N extends number> = N extends N ? number extends N ? T[] : _TupleOf<T, N, []> : never;
type _TupleOf<T, N extends number, R extends unknown[]> =
    R['length'] extends N ? R : _TupleOf<T, N, [T, ...R]>;

function f22<N extends number, M extends N>(tn: TupleOf<number, N>, tm: TupleOf<number, M>) {
    tn = tm;
    tm = tn;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2322).at(7, 5).with_message_fragment(
                "Type 'TupleOf<number, M>' is not assignable to type 'TupleOf<number, N>'.",
            ),
            DiagnosticShape::code(2322).at(8, 5).with_message_fragment(
                "Type 'TupleOf<number, N>' is not assignable to type 'TupleOf<number, M>'.",
            ),
        ],
    );
}

#[test]
fn recursive_tuple_conditional_with_target_type_param_reports_declared_mismatches() {
    // The source application has a concrete tuple-length argument, while the
    // target argument is a type parameter constrained to that concrete length.
    // `tsc` still reports both assignments against the declared alias forms.
    let source = r#"
type TupleOf<T, N extends number> = N extends N ? number extends N ? T[] : _TupleOf<T, N, []> : never;
type _TupleOf<T, N extends number, R extends unknown[]> =
    R['length'] extends N ? R : _TupleOf<T, N, [T, ...R]>;

function f<N extends 1>(one: TupleOf<number, 1>, tn: TupleOf<number, N>) {
    one = tn;
    tn = one;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2322).at(7, 5).with_message_fragment(
                "Type 'TupleOf<number, N>' is not assignable to type '[number]'.",
            ),
            DiagnosticShape::code(2322).at(8, 5).with_message_fragment(
                "Type 'TupleOf<number, 1>' is not assignable to type 'TupleOf<number, N>'.",
            ),
        ],
    );
}

#[test]
fn covariant_application_to_constrained_param_target_rejects_wider_to_narrower() {
    // Anti-regression: COVARIANT containers still reject the wider-to-narrower
    // direction. `Covariant<A>` -> `Covariant<B>` fails when B extends A,
    // while `Covariant<B>` -> `Covariant<A>` is allowed.
    let source = r#"
interface Covariant<T> {
    foo: T extends string ? T : number;
}
function f<A, B extends A>(a: Covariant<A>, b: Covariant<B>) {
    a = b;  // OK (covariant: B<:A allowed)
    b = a;  // Error
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    let ts2322_count = codes.iter().filter(|c| **c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Exactly one TS2322 expected (the wrong covariant direction). Codes: {codes:?}"
    );
}

#[test]
fn invariant_application_to_constrained_param_target_rejects_both() {
    // Anti-regression: INVARIANT containers reject both directions.
    let source = r#"
interface Invariant<T> {
    foo: T extends string ? keyof T : T;
}
function f<A, B extends A>(a: Invariant<A>, b: Invariant<B>) {
    a = b;  // Error
    b = a;  // Error
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2322).at(6, 5).with_message_fragment(
                "Type 'Invariant<B>' is not assignable to type 'Invariant<A>'.",
            ),
            DiagnosticShape::code(2322).at(7, 5).with_message_fragment(
                "Type 'Invariant<A>' is not assignable to type 'Invariant<B>'.",
            ),
        ],
    );
}

#[test]
fn same_base_mapped_record_alias_keeps_tsc_variance_quirk() {
    let source = r#"
type RecordA<K extends keyof any, T> = {
    [P in K]: T;
};
type RecordB<K extends keyof any, T> = {
    [P in K]: T;
};

function sameA(x: RecordA<'a', string>, y: RecordA<string, string>) {
    x = y;
}
function sameB(x: RecordB<'a', string>, y: RecordB<string, string>) {
    x = y;
}
function mixedA(x: RecordB<'a', string>, y: RecordA<string, string>) {
    x = y;
}
function mixedB(x: RecordA<'a', string>, y: RecordB<string, string>) {
    x = y;
}
function sameGenericA<T>(x: RecordA<'a', T>, y: RecordA<string, T>) {
    x = y;
}
function sameGenericB<T>(x: RecordB<'a', T>, y: RecordB<string, T>) {
    x = y;
}
function mixedGenericA<T>(x: RecordB<'a', T>, y: RecordA<string, T>) {
    x = y;
}
function mixedGenericB<T>(x: RecordA<'a', T>, y: RecordB<string, T>) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordB<\"a\", string>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordA<\"a\", string>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordB<\"a\", T>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordA<\"a\", T>'"),
        ],
    );
}

#[test]
fn promise_like_method_callback_variance_still_rejects_unconstrained_applications() {
    let source = r#"// @target: es2015
interface Promise<T> {
    then<U>(cb: (x: T) => Promise<U>): Promise<U>;
}

interface CPromise<T extends { x: any; }> {
    then<U extends { x: any; }>(cb: (x: T) => Promise<U>): Promise<U>;
}

interface Foo { x: any; }
interface Bar { x: any; y: any; }

var a: Promise<Foo>;
declare var b: Promise<Bar>;
a = b; // ok
b = a; // ok

var a2: CPromise<Foo>;
declare var b2: CPromise<Bar>;
a2 = b2; // ok
b2 = a2; // was error
"#;
    let diags = check_es2015_promise_source(source);
    assert_diagnostic_shape(
        source,
        &diags,
        &DiagnosticShape::code(2322)
            .with_message_fragment("Type 'Promise<Foo>' is not assignable to type 'Promise<Bar>'."),
    );
}

#[test]
fn recursive_conditional_alias_reports_target_alias_not_reduced_param() {
    // Reduced from `recursiveConditionalTypes.ts`: assigning
    // `AwaitedLike<Base>` to `AwaitedLike<Derived>` is invalid, but the
    // reported target remains the alias application. Same-generic variance
    // rejection with type-parameter arguments must not skip the conditional
    // alias structural path and explain the target as the raw parameter.
    let source = r#"
type AwaitedLike<T> =
    T extends null | undefined ? T :
    T extends PromiseLike<infer Value> ? AwaitedLike<Value> :
    T;

interface PromiseLike<T> {
    then<U>(f: ((value: T) => U | PromiseLike<U>) | null | undefined): PromiseLike<U>;
}

function assign<Base, Derived extends Base>(
    baseAwaited: AwaitedLike<Base>,
    derivedAwaited: AwaitedLike<Derived>,
) {
    baseAwaited = derivedAwaited;
    derivedAwaited = baseAwaited;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[DiagnosticShape::code(2322)
            .at(16, 5)
            .with_message_fragment("is not assignable to type 'AwaitedLike<Derived>'")],
    );
}

#[test]
fn set_callback_parameter_from_instantiated_receiver_keeps_method_bivariance() {
    let source = r#"
const cleanup = ({ ref, set }: {
    readonly ref: WeakRef<object>;
    readonly set: Set<WeakRef<object>>;
}) => {
    set.delete(ref);
};

class Box<K extends object> {
    declare readonly [Symbol.toStringTag]: "Box";

    #weakMap = new WeakMap<K, { readonly ref: WeakRef<K>; value: number }>();
    #refSet = new Set<WeakRef<K>>();
    #registry = new FinalizationRegistry(cleanup);

    set(key: K, value: number): this {
        const entry = this.#weakMap.get(key);
        if (entry !== undefined) {
            entry.value = value;
        } else {
            const ref = new WeakRef(key);
            this.#weakMap.set(key, { ref, value });
            this.#refSet.add(ref);
            this.#registry.register(key, {
                set: this.#refSet,
                ref,
            }, ref);
        }
        return this;
    }

    has(key: K): boolean {
        return this.#weakMap.has(key);
    }

    get(key: K): number | undefined {
        return this.#weakMap.get(key)?.value;
    }

    delete(key: K): boolean {
        const entry = this.#weakMap.get(key);
        if (entry === undefined) {
            return false;
        }
        const { ref } = entry;
        this.#weakMap.delete(key);
        this.#refSet.delete(ref);
        this.#registry.unregister(ref);
        return true;
    }
}
"#;
    let diags = check_esnext_weakref_source(source);
    assert_diagnostic_shapes_exactly(source, &diags, &[]);
}

// =========================================================================
// #17614: two distinct mapped-type aliases with byte-identical bodies intern
// their instantiations to the same structural TypeIds. The provenance-
// recovered variance fast path must never weld those into a fictitious
// same-alias pair: whichever cross-alias assignment is checked SECOND used to
// silently lose its TS2741, in either order. The same-alias assignments keep
// tsc's variance-quirk acceptance through the declared-application path.
// =========================================================================

#[test]
fn cross_mapped_alias_second_assignment_keeps_ts2741_a_then_b() {
    let source = r#"
type RecordA<K extends keyof any, T> = {
    [P in K]: T;
};
type RecordB<K extends keyof any, T> = {
    [P in K]: T;
};
function mixedA(x: RecordB<'a', string>, y: RecordA<string, string>) {
    x = y;
}
function mixedB(x: RecordA<'a', string>, y: RecordB<string, string>) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordB<\"a\", string>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordA<\"a\", string>'"),
        ],
    );
}

#[test]
fn cross_mapped_alias_second_assignment_keeps_ts2741_b_then_a() {
    let source = r#"
type RecordA<K extends keyof any, T> = {
    [P in K]: T;
};
type RecordB<K extends keyof any, T> = {
    [P in K]: T;
};
function mixedB(x: RecordA<'a', string>, y: RecordB<string, string>) {
    x = y;
}
function mixedA(x: RecordB<'a', string>, y: RecordA<string, string>) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordA<\"a\", string>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordB<\"a\", string>'"),
        ],
    );
}

#[test]
fn cross_mapped_alias_alpha_renamed_binders_keep_both_ts2741() {
    // Alpha-renamed type parameters on the second alias: positional identity
    // still interns the instantiations to shared TypeIds, so this exercises
    // the same collision without literal-name overlap.
    let source = r#"
type RecordA<K extends keyof any, T> = {
    [P in K]: T;
};
type RecordB<K2 extends keyof any, T2> = {
    [P2 in K2]: T2;
};
function mixedA(x: RecordB<'a', string>, y: RecordA<string, string>) {
    x = y;
}
function mixedB(x: RecordA<'a', string>, y: RecordB<string, string>) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordB<\"a\", string>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordA<\"a\", string>'"),
        ],
    );
}

#[test]
fn same_alias_quirk_survives_after_cross_alias_rejections() {
    // The same-alias variance-quirk acceptance must not be lost once the
    // cross-alias assignments above have populated caches with rejections:
    // tsc accepts both same-alias assignments here and rejects only the two
    // mixed ones, in this order.
    let source = r#"
type RecordA<K extends keyof any, T> = {
    [P in K]: T;
};
type RecordB<K extends keyof any, T> = {
    [P in K]: T;
};
function mixedA(x: RecordB<'a', string>, y: RecordA<string, string>) {
    x = y;
}
function mixedB(x: RecordA<'a', string>, y: RecordB<string, string>) {
    x = y;
}
function sameA(x: RecordA<'a', string>, y: RecordA<string, string>) {
    x = y;
}
function sameB(x: RecordB<'a', string>, y: RecordB<string, string>) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordB<\"a\", string>'"),
            DiagnosticShape::code(2741)
                .with_message_fragment("required in type 'RecordA<\"a\", string>'"),
        ],
    );
}

// =========================================================================
// #17630: the #17614 provenance-ambiguity guard must not treat a display
// alias over its own underlying base (`AsyncR<T> = Promise<OK<T>>`: the
// display channel names the user alias, the eval origin names `Promise`)
// as a fictitious twin-alias weld. An `any`-instantiated override return
// must keep relating to the concrete base return through general variance
// measurement (the zod `_parse` shape), while genuinely mismatched
// arguments keep their TS2416 and the twin-alias weld tests above keep
// rejecting.
// =========================================================================

#[test]
fn any_returning_override_of_alias_over_promise_base_return_stays_clean() {
    let source = r#"
type OK<T> = { valid: true; value: T };
type AsyncR<T> = Promise<OK<T>>;
class Base<Output> {
  _parse(data: any): AsyncR<Output> { return null as any; }
}
class Child<U> extends Base<U[]> {
  _parse(data: any): AsyncR<any> { return null as any; }
}
"#;
    let diags = check_es2015_promise_source(source);
    assert_diagnostic_shapes_exactly(source, &diags, &[]);
}

#[test]
fn any_returning_override_accepts_through_two_hop_alias_chain() {
    // Extra alias hop between the display alias and `Promise`
    // (`Deferred<T> = Boxed<...>`, `Boxed<T> = Promise<T>`): the
    // transparency walk must follow the chain, not just one level.
    let source = r#"
type Won<T> = { ok: true; payload: T };
type Boxed<T> = Promise<T>;
type Deferred<T> = Boxed<Won<T>>;
class Machine<Yield> {
  step(input: string): Deferred<Yield> { return null as any; }
}
class ChainMachine<L> extends Machine<L[]> {
  step(input: string): Deferred<any> { return null as any; }
}
"#;
    let diags = check_es2015_promise_source(source);
    assert_diagnostic_shapes_exactly(source, &diags, &[]);
}

#[test]
fn concrete_mismatched_override_against_generic_base_keeps_ts2416() {
    // Negative control: a genuinely mismatched concrete argument
    // (`AsyncR<string>` vs `AsyncR<U[]>`) must still be rejected — the
    // restored variance accept is any-driven, not blanket.
    let source = r#"
type OK<T> = { valid: true; value: T };
type AsyncR<T> = Promise<OK<T>>;
class Base<Output> {
  _parse(data: any): AsyncR<Output> { return null as any; }
}
class Child<U> extends Base<U[]> {
  _parse(data: any): AsyncR<string> { return null as any; }
}
"#;
    let diags = check_es2015_promise_source(source);
    assert_diagnostic_shape(source, &diags, &DiagnosticShape::code(2416));
}

#[test]
fn concrete_mismatched_override_against_concrete_base_keeps_ts2416() {
    let source = r#"
type OK<T> = { valid: true; value: T };
type AsyncR<T> = Promise<OK<T>>;
class Base<Output> {
  _parse(data: any): AsyncR<Output> { return null as any; }
}
class Child extends Base<number> {
  _parse(data: any): AsyncR<string> { return null as any; }
}
"#;
    let diags = check_es2015_promise_source(source);
    assert_diagnostic_shape(source, &diags, &DiagnosticShape::code(2416));
}
