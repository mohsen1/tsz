//! Tests for overloaded method signatures in type literals.
//!
//! The structural rule: when multiple method signatures with the same name appear
//! in an object type literal, they must be merged into a single Callable type
//! (for ≥2 signatures) or Function type (for 1 signature) to enable proper
//! overload resolution at call sites.
//!
//! Adjacent cases covered:
//! 1. Inline callbacks (the reported repro — RxJS-style pipe)
//! 2. Aliased operators (pre-existing working case)
//! 3. Type-alias intermediary
//! 4. Renamed type parameters (K, U, etc. instead of T)
//! 5. Nested generic wrappers
//! 6. Negative cases: wrong arity still errors

use tsz_checker::context::CheckerOptions;

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn no_errors(source: &str) {
    let diags = relevant_diagnostics(source);
    assert!(
        diags.is_empty(),
        "expected no errors, got: {diags:#?}\nsource:\n{source}"
    );
}

fn has_error(source: &str, code: u32) {
    let diags = relevant_diagnostics(source);
    assert!(
        diags.iter().any(|(c, _)| *c == code),
        "expected TS{code} error, got: {diags:#?}\nsource:\n{source}"
    );
}

// ── Baseline: single method signature ──────────────────────────────────────

#[test]
fn single_method_sig_no_overload() {
    // A single-signature method should work as before.
    no_errors(
        r#"
type Foo = {
    transform(x: number): string;
};
declare const foo: Foo;
const r: string = foo.transform(42);
"#,
    );
}

// ── The reported repro: RxJS-style pipe ────────────────────────────────────

#[test]
fn rxjs_pipe_inline_callbacks_no_error() {
    // Multiple overloads with same name in a type literal must be merged so
    // that overload resolution can find the matching 2-arg overload.
    no_errors(
        r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: OperatorFunction<T, A>): Observable<A>;
    pipe<A, B>(op1: OperatorFunction<T, A>, op2: OperatorFunction<A, B>): Observable<B>;
    pipe<A, B, C>(
        op1: OperatorFunction<T, A>,
        op2: OperatorFunction<A, B>,
        op3: OperatorFunction<B, C>,
    ): Observable<C>;
};
type OperatorFunction<T, R> = (source: Observable<T>) => Observable<R>;
type MonoTypeOperatorFunction<T> = OperatorFunction<T, T>;

declare function of<T>(...items: T[]): Observable<T>;
declare function map<T, R>(fn: (x: T) => R): OperatorFunction<T, R>;
declare function filter<T>(predicate: (x: T) => boolean): MonoTypeOperatorFunction<T>;

const result1 = of(1, 2, 3).pipe(
    map(x => x.toString()),
    filter(x => x.length > 0),
);
"#,
    );
}

// ── Aliased operators: pre-existing working case ───────────────────────────

#[test]
fn rxjs_pipe_aliased_operators_no_error() {
    no_errors(
        r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: OperatorFunction<T, A>): Observable<A>;
    pipe<A, B>(op1: OperatorFunction<T, A>, op2: OperatorFunction<A, B>): Observable<B>;
};
type OperatorFunction<T, R> = (source: Observable<T>) => Observable<R>;
type MonoTypeOperatorFunction<T> = OperatorFunction<T, T>;

declare function of<T>(...items: T[]): Observable<T>;
declare function map<T, R>(fn: (x: T) => R): OperatorFunction<T, R>;
declare function filter<T>(predicate: (x: T) => boolean): MonoTypeOperatorFunction<T>;

const mapped = map((x: number) => x.toString());
const filtered = filter((x: string) => x.length > 0);
const result2 = of(1, 2, 3).pipe(mapped, filtered);
"#,
    );
}

// ── Type alias intermediary ────────────────────────────────────────────────

#[test]
fn rxjs_pipe_type_alias_intermediary_no_error() {
    no_errors(
        r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: OperatorFunction<T, A>): Observable<A>;
    pipe<A, B>(op1: OperatorFunction<T, A>, op2: OperatorFunction<A, B>): Observable<B>;
};
type OperatorFunction<T, R> = (source: Observable<T>) => Observable<R>;

declare function of<T>(...items: T[]): Observable<T>;
declare function map<T, R>(fn: (x: T) => R): OperatorFunction<T, R>;

type AliasedOp<T, R> = OperatorFunction<T, R>;
declare const aliasedMap: AliasedOp<number, string>;
declare const aliasedFilter: AliasedOp<string, string>;
const result3 = of(1, 2, 3).pipe(aliasedMap, aliasedFilter);
"#,
    );
}

// ── Renamed type parameters ────────────────────────────────────────────────
// The fix must work regardless of what names the type parameters use.

#[test]
fn overloaded_method_renamed_type_params_no_error() {
    // Uses K, U, V instead of T, A, B to prove the fix is structural.
    no_errors(
        r#"
type Stream<K> = {
    transform(): Stream<K>;
    transform<U>(op1: Xform<K, U>): Stream<U>;
    transform<U, V>(op1: Xform<K, U>, op2: Xform<U, V>): Stream<V>;
};
type Xform<K, U> = (s: Stream<K>) => Stream<U>;

declare function source<K>(...items: K[]): Stream<K>;
declare function lift<K, U>(fn: (x: K) => U): Xform<K, U>;
declare function pick<K>(pred: (x: K) => boolean): Xform<K, K>;

const r = source(1, 2, 3).transform(
    lift((x: number) => x.toString()),
    pick((x: string) => x.length > 0),
);
"#,
    );
}

// ── Zero-arg overload stays valid ─────────────────────────────────────────

#[test]
fn zero_arg_overload_accepted() {
    no_errors(
        r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: (s: Observable<T>) => Observable<A>): Observable<A>;
};

declare function of<T>(...items: T[]): Observable<T>;
const r: Observable<number> = of(1, 2).pipe();
"#,
    );
}

// ── Three-arg overload ────────────────────────────────────────────────────

#[test]
fn three_arg_overload_accepted() {
    no_errors(
        r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: (s: Observable<T>) => Observable<A>): Observable<A>;
    pipe<A, B>(op1: (s: Observable<T>) => Observable<A>, op2: (s: Observable<A>) => Observable<B>): Observable<B>;
    pipe<A, B, C>(op1: (s: Observable<T>) => Observable<A>, op2: (s: Observable<A>) => Observable<B>, op3: (s: Observable<B>) => Observable<C>): Observable<C>;
};

declare function of<T>(...items: T[]): Observable<T>;
declare const a: (s: Observable<number>) => Observable<string>;
declare const b: (s: Observable<string>) => Observable<boolean>;
declare const c: (s: Observable<boolean>) => Observable<number[]>;

const r: Observable<number[]> = of(1).pipe(a, b, c);
"#,
    );
}

// ── Negative: arity mismatch still errors ─────────────────────────────────

#[test]
fn wrong_arity_still_errors() {
    // The type only defines 0, 1, and 2-arg pipe overloads; passing 3 args
    // should still produce TS2554.
    has_error(
        r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: (s: Observable<T>) => Observable<A>): Observable<A>;
    pipe<A, B>(op1: (s: Observable<T>) => Observable<A>, op2: (s: Observable<A>) => Observable<B>): Observable<B>;
};

declare function of<T>(...items: T[]): Observable<T>;
declare const a: (s: Observable<number>) => Observable<string>;
declare const b: (s: Observable<string>) => Observable<boolean>;
declare const c: (s: Observable<boolean>) => Observable<number[]>;

// 3 args but no 3-arg overload - should error
const r = of(1).pipe(a, b, c);
"#,
        2554,
    );
}

// ── Inline callbacks match in all scenarios ───────────────────────────────

#[test]
fn inline_vs_aliased_same_behavior() {
    // Both inline callbacks and aliased operators should produce the same
    // (no-error) behavior for the same overload selection.
    let inline = r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: (s: Observable<T>) => Observable<A>): Observable<A>;
    pipe<A, B>(op1: (s: Observable<T>) => Observable<A>, op2: (s: Observable<A>) => Observable<B>): Observable<B>;
};

declare function of<T>(...items: T[]): Observable<T>;
declare function toStr(): (s: Observable<number>) => Observable<string>;
declare function onlyNonEmpty(): (s: Observable<string>) => Observable<string>;

const r1 = of(1, 2).pipe(toStr(), onlyNonEmpty());
"#;
    let aliased = r#"
type Observable<T> = {
    pipe(): Observable<T>;
    pipe<A>(op1: (s: Observable<T>) => Observable<A>): Observable<A>;
    pipe<A, B>(op1: (s: Observable<T>) => Observable<A>, op2: (s: Observable<A>) => Observable<B>): Observable<B>;
};

declare function of<T>(...items: T[]): Observable<T>;
declare function toStr(): (s: Observable<number>) => Observable<string>;
declare function onlyNonEmpty(): (s: Observable<string>) => Observable<string>;

const op1 = toStr();
const op2 = onlyNonEmpty();
const r2 = of(1, 2).pipe(op1, op2);
"#;
    no_errors(inline);
    no_errors(aliased);
}

// ── Multiple distinct overloaded methods in same type literal ─────────────

#[test]
fn multiple_overloaded_methods_in_same_literal() {
    no_errors(
        r#"
type Processor<T> = {
    map(): Processor<T>;
    map<U>(fn: (x: T) => U): Processor<U>;
    filter(pred: (x: T) => boolean): Processor<T>;
    filter(): Processor<T>;
    zip<U>(other: Processor<U>): Processor<[T, U]>;
};

declare function processor<T>(...items: T[]): Processor<T>;

const r = processor(1, 2, 3)
    .map((x: number) => x.toString())
    .filter((x: string) => x.length > 0)
    .zip(processor("a", "b"));
"#,
    );
}
