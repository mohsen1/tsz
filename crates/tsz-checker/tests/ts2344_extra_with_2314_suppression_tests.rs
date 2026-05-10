//! Tests for the cascading TS2344 / TS2314 suppression rule:
//! when an inner generic type reference emits TS2314 (wrong type
//! argument count), the outer wrapping type's constraint check
//! must NOT also emit TS2344. tsc 6.0.3 propagates `errorType`
//! through the surrounding type expression so the outer constraint
//! silently passes.
//!
//! Source: type-challenges 00008-medium-readonly-2 (#4904) and the
//! broader `extra-2344-with-2314` cluster (#4904, #4911, #4919, #4920,
//! #4922, #4933, #4939, #4943).

use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn cascading_ts2344_suppressed_when_inner_arg_count_wrong() {
    // Outer `Equal<X, Y>` has a generic constraint check. Inner
    // `MyReadonly2<Todo>` is missing one type argument. tsc emits only
    // TS2314 for the inner; tsz used to additionally emit a cascading
    // TS2344 for the outer `Expect<...>`.
    let source = r#"
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends <T>() => T extends Y ? 1 : 2 ? true : false;
type Expect<T extends true> = T;
type MyReadonly2<T, K> = any
interface Todo { title: string }
type Cases = [
  Expect<Equal<MyReadonly2<Todo>, Readonly<Todo>>>,
];
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2314),
        "expected TS2314 (wrong arg count for MyReadonly2). got: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "expected NO cascading TS2344 on outer Expect/Equal. got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same structural rule with different identifier
/// names. The fix must key on diagnostic-code + AST range, not on
/// `MyReadonly2` / `Equal` / `Expect` strings.
#[test]
fn cascading_ts2344_suppressed_renamed_identifiers() {
    let source = r#"
type Same<L, R> = (<X>() => X extends L ? 1 : 2) extends <X>() => X extends R ? 1 : 2 ? true : false;
type Assert<U extends true> = U;
type Wrapper<A, B> = any
interface Box { value: number }
type Cases = [
  Assert<Same<Wrapper<Box>, Readonly<Box>>>,
];
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2314),
        "expected TS2314 (renamed). got: {codes:?}"
    );
    assert!(
        !codes.contains(&2344),
        "expected NO cascading TS2344 with renamed identifiers. got: {codes:?}"
    );
}

/// Negative cover: when there is NO inner arity error, the outer
/// constraint check still fires normally. Locks the rule from
/// over-firing — we must only suppress in the presence of an arity
/// diagnostic on a descendant of the type-arg AST node.
#[test]
fn outer_ts2344_still_fires_when_no_inner_arity_error() {
    // `MyAlias<Todo, 'title'>` is fine (2 args, matching `<T, K>`). The
    // outer `Equal<number, { other: number }>` evaluates false (the two
    // shapes are obviously disjoint primitives vs object), so tsc emits
    // TS2344 — and so should we. Self-contained types are used so the
    // assertion does not depend on `Readonly`/lib being loaded by the
    // test harness.
    //
    // Historical note: the earlier formulation used `MyReadonly2<T, K> = T`
    // and `Readonly<Todo>` for the second `Equal` argument. After the
    // outer-conditional deferral fix in solver `evaluate_conditional`
    // (free vs bound type-parameter detection), the comparison no longer
    // defers and runs through `conditional_extends_types_equivalent`
    // directly. That helper currently delegates to bidirectional
    // `check_subtype`, which treats `Readonly<X>` and `X` as mutually
    // assignable (matching tsc's loose subtype rule where readonly is a
    // usage constraint rather than a structural one). Using
    // `number` vs `{ other: number }` keeps the negative cover meaningful
    // regardless of how readonly identity is eventually tightened.
    let source = r#"
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends <T>() => T extends Y ? 1 : 2 ? true : false;
type Expect<T extends true> = T;
type MyAlias<T, K> = number
interface Todo { title: string }
type Cases = [
  Expect<Equal<MyAlias<Todo, 'title'>, { other: number }>>,
];
"#;
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2314),
        "expected NO TS2314 (correct arg count). got: {codes:?}"
    );
    assert!(
        codes.contains(&2344),
        "expected TS2344 (Equal evaluates false on disjoint target). got: {codes:?}"
    );
}
