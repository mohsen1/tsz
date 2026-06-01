//! Distributive conditionals wrapped in another alias must keep their per-
//! member precision. When a non-generic alias' body is a distributive
//! conditional whose check side defers into another alias (`Lazy`), a
//! generic application (`Application`), an indexed access (`IndexAccess`),
//! or a `KeyOf`, the checker must NOT eagerly snapshot the body at the
//! declaration seam. Snapshotting freezes distribution into a single object
//! that later consumers (e.g. `Extract<V, P>`) then substitute against.
//!
//! The structural classifier lives in
//! `crates/tsz-solver/src/type_queries/classifiers.rs`
//! (`is_distributive_conditional_with_deferred_check`) and is consulted from
//! the two checker eager-eval gates:
//! - `crates/tsz-checker/src/state/type_analysis/computed/type_alias_variable_alias.rs`
//! - `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
//!
//! Adjacent cases (see CLAUDE.md §26) exercised here:
//! 1. The reported repro shape (`{kind, value}` classifier over a tuple union).
//! 2. A different bound name (`P` instead of `T`) — proves the rule is
//!    structural, not keyed on a specific identifier.
//! 3. A two-member literal union — same shape with non-tuple members.
//! 4. A negative-case probe to ensure non-distributive aliases retain their
//!    eager-eval behavior.
//!
//! Each test defines `Extract<T, U>` locally so the harness does not depend on
//! `lib.es5.d.ts`. A typo in `Extract` or `Wrap` would surface as an
//! unfiltered TS2304 and fail the test.

use tsz_checker::test_utils::check_source_diagnostics;

#[track_caller]
fn assert_diagnostic_codes(source: &str, expected_codes: &[u32]) {
    let diagnostics = check_source_diagnostics(source);
    let actual: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        actual, expected_codes,
        "Expected diagnostic codes {expected_codes:?}, got: {diagnostics:#?}",
    );
}

/// Baseline: an inline-union argument to a distributive generic flows through
/// `Extract` correctly.
#[test]
fn extract_on_inline_union_through_distributive_wrap() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type Wrap<T> = T extends unknown ? { kind: 'other'; value: T } : never;
type V = Wrap<[string, string] | [number, number] | []>;
type E = Extract<V, { value: [string, string] }>;
declare const e: E;
const ok: 'other' = e.kind;
const v0: string = e.value[0];
"#;
    assert_diagnostic_codes(source, &[]);
}

/// Renamed bound name (`P` instead of `T`) — proves the rule is structural.
#[test]
fn extract_on_inline_union_renamed_iteration_var() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type Wrap<P> = P extends unknown ? { kind: 'other'; value: P } : never;
type V = Wrap<[string, string] | [number, number] | []>;
type E = Extract<V, { value: [string, string] }>;
declare const e: E;
const ok: 'other' = e.kind;
const v0: string = e.value[0];
"#;
    assert_diagnostic_codes(source, &[]);
}

/// Two-member literal union (no tuples) — same structural rule.
#[test]
fn extract_on_literal_union_through_distributive_wrap() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type Wrap<T> = T extends unknown ? { v: T } : never;
type V = Wrap<"a" | "b">;
type E = Extract<V, { v: "a" }>;
declare const e: E;
const k: 'a' = e.v;
"#;
    assert_diagnostic_codes(source, &[]);
}

/// Non-distributive alias body (check side is a fixed alias, not a type
/// parameter, so the conditional is non-distributive in tsc): the eager
/// evaluation must still happen, and `Extract` correctly yields `never`.
/// Locks in that the new guard is narrow to *distributive* conditionals.
#[test]
fn non_distributive_alias_body_keeps_eager_eval() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type U = "a" | "b";
type V = U extends unknown ? { v: U } : never;
type E = Extract<V, { v: "a" }>;
declare const e: E;
const k: 'a' = e.v;
"#;
    // V here is non-distributive — check side is a fixed alias `U`, not a
    // type parameter — so V evaluates to `{ v: 'a' | 'b' }` and Extract over
    // it yields `never`. `e.v` is therefore a missing-property access.
    assert_diagnostic_codes(source, &[2339]);
}

/// Classify-style nested conditional — the original repro shape from #10864.
#[test]
fn classify_distributive_over_inline_tuple_union() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type Classify<T> = T extends unknown
  ? T extends string
    ? { kind: 'string'; value: T }
    : T extends number
      ? { kind: 'number'; value: T }
      : { kind: 'other'; value: T }
  : never;

type V = Classify<[string, string] | [number, number] | []>;
type E = Extract<V, { value: [string, string] }>;
declare const e: E;
const ok: 'other' = e.kind;
const v0: string = e.value[0];
"#;
    assert_diagnostic_codes(source, &[]);
}
