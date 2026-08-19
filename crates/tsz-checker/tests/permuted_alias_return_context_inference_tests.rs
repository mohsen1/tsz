//! Return-context inference through parameter-permuting alias arms.
//!
//! `tsc` treats a type alias whose body applies another generic base over the
//! alias's own parameters as transparent during return-context inference even
//! when the body PERMUTES (`type FlipRow<X, Y> = PairRow<Y, X>`) or REPEATS
//! (`type DupRow<T> = PairRow<T, T>`) those parameters: an ambiguous same-base
//! contextual union combines every arm's aligned argument into one union
//! candidate, and the return assignability check then reports TS2322. The
//! declared-order passthrough half of this transparency landed in #17677;
//! these fences pin the permuting/repeating remainder that #17677 documented
//! as deliberately excluded. Every case is oracle-pinned against
//! `tsc 6.0.2 --strict` (exactly one TS2322 on each error case, byte-checked
//! top line recorded in the case comments).

use tsz_checker::test_utils::{
    DiagnosticShape, assert_diagnostic_shapes_exactly, check_source_diagnostics,
    diagnostic_code_message_refs,
};

const PRELUDE: &str = r#"
interface TemplateStringsArray {
  readonly raw: readonly string[]
}

interface PairRow<First, Second> {
  readonly first: First | undefined
  readonly second: Second | undefined
  readonly isPairRow: true
}

interface OtherRow<Left, Right> {
  readonly left: Left | undefined
  readonly right: Right | undefined
  readonly isOtherRow: true
}

interface PairTag {
  <A = unknown, B = unknown>(parts: TemplateStringsArray, ...values: unknown[]): PairRow<A, B>
}

declare const pairSql: PairTag
declare function pairCall<A = unknown, B = unknown>(): PairRow<A, B>

type FlipRow<X, Y> = PairRow<Y, X>
type FlipAgain<P, Q> = FlipRow<Q, P>
type DupRow<T> = PairRow<T, T>
type OtherFlip<X, Y> = OtherRow<Y, X>
"#;

fn check(body: &str) -> Vec<tsz_common::diagnostics::Diagnostic> {
    check_source_diagnostics(&format!("{PRELUDE}\n{body}"))
}

fn assert_clean(body: &str, context: &str) {
    let diagnostics = check(body);
    assert!(
        diagnostics.is_empty(),
        "{context}: expected no diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

fn assert_single_ts2322(body: &str, message_fragment: &'static str) {
    let source = format!("{PRELUDE}\n{body}");
    let diagnostics = check_source_diagnostics(&source);
    assert_diagnostic_shapes_exactly(
        &source,
        &diagnostics,
        &[DiagnosticShape {
            code: 2322,
            line: None,
            column: None,
            message_fragment: Some(message_fragment),
            related_min: None,
        }],
    );
}

/// tsc: `Type 'PairRow<string | number, string | number>' is not assignable
/// to type 'FlipRow<string, number> | PairRow<string, number>'.`
#[test]
fn permuting_alias_arm_merges_with_direct_arm() {
    assert_single_ts2322(
        r#"
function build(): FlipRow<string, number> | PairRow<string, number> {
  return pairCall()
}
"#,
        "Type 'PairRow<string | number, string | number>' is not assignable to type 'FlipRow<string, number> | PairRow<string, number>'.",
    );
}

/// The single permuted arm is unambiguous: the alias hop must ALIGN the
/// arguments (`A := number`, `B := string`), not just recognize the base —
/// a wrong-order mapping turns this clean case into a false positive.
#[test]
fn single_permuting_alias_arm_binds_through_permutation() {
    assert_clean(
        r#"
function build(): FlipRow<string, number> | null {
  return pairCall()
}
"#,
        "single permuting alias arm + null",
    );
}

/// tsc: `Type 'PairRow<string | number, string | boolean>' is not assignable
/// to type 'DupRow<string> | PairRow<number, boolean>'.`
#[test]
fn repeating_alias_arm_merges_with_direct_arm() {
    assert_single_ts2322(
        r#"
function build(): DupRow<string> | PairRow<number, boolean> {
  return pairCall()
}
"#,
        "Type 'PairRow<string | number, string | boolean>' is not assignable to type 'DupRow<string> | PairRow<number, boolean>'.",
    );
}

/// tsc: `Type 'FlipRow<string | number, string | number>' is not assignable
/// to type 'PairRow<string, number> | PairRow<number, string>'.` The target
/// half is pinned byte-exact; the source half is a known display residual —
/// tsz renders the forwarded `PairRow<string | number, string | number>`
/// where tsc preserves the written alias spelling `FlipRow<...>` (display
/// layer, not inference; the argument order inside is correct).
#[test]
fn permuting_alias_declared_return_merges_direct_arms() {
    assert_single_ts2322(
        r#"
declare function flipCall<A = unknown, B = unknown>(): FlipRow<A, B>
function build(): PairRow<string, number> | PairRow<number, string> {
  return flipCall()
}
"#,
        "is not assignable to type 'PairRow<string, number> | PairRow<number, string>'.",
    );
}

/// tsc: `Type 'PairRow<string | number, string | number>' is not assignable
/// to type 'FlipRow<string, number> | FlipRow<number, string>'.`
#[test]
fn two_permuting_alias_arms_merge() {
    assert_single_ts2322(
        r#"
function build(): FlipRow<string, number> | FlipRow<number, string> {
  return pairCall()
}
"#,
        "Type 'PairRow<string | number, string | number>' is not assignable to type 'FlipRow<string, number> | FlipRow<number, string>'.",
    );
}

/// tsc: `Type 'PairRow<string | number, string | number>' is not assignable
/// to type 'FlipAgain<string, number> | PairRow<number, string>'.` The
/// two-hop chain composes the remaps (`FlipAgain<P, Q>` ≡ `PairRow<P, Q>`).
#[test]
fn two_hop_permuting_alias_chain_arm_merges() {
    assert_single_ts2322(
        r#"
function build(): FlipAgain<string, number> | PairRow<number, string> {
  return pairCall()
}
"#,
        "Type 'PairRow<string | number, string | number>' is not assignable to type 'FlipAgain<string, number> | PairRow<number, string>'.",
    );
}

/// A permuting alias of a DIFFERENT base must not merge with the direct arm;
/// the lone aligned arm binds and the return stays clean (tsc: no error).
#[test]
fn permuting_alias_of_different_base_keeps_per_arm_path() {
    assert_clean(
        r#"
function build(): OtherFlip<string, number> | PairRow<string, number> {
  return pairCall()
}
"#,
        "different-base permuting alias arm + direct arm",
    );
}

/// tsc: `Type 'PairRow<string | number, string | number>' is not assignable
/// to type 'PairRow<string, number> | FlipRow<string, number>'.`
#[test]
fn tagged_template_form_merges_permuting_alias_arm() {
    assert_single_ts2322(
        r#"
function build(): FlipRow<string, number> | PairRow<string, number> {
  return pairSql`pair`
}
"#,
        "Type 'PairRow<string | number, string | number>' is not assignable to type 'FlipRow<string, number> | PairRow<string, number>'.",
    );
}

/// Renamed binders end-to-end (interface params, alias params, call params
/// all disjoint names). tsc: `Type 'Krate<string | boolean, string |
/// boolean>' is not assignable to type 'Swap<boolean, string> |
/// Krate<boolean, string>'.` The source half is pinned byte-exact; the
/// target half is a known display residual — tsz orders the written union's
/// arms as `Krate<boolean, string> | Swap<boolean, string>` (union-target
/// display order family, not inference).
#[test]
fn renamed_binders_permuting_alias_arm_merges() {
    assert_single_ts2322(
        r#"
interface Krate<Inner, Outer> {
  readonly a: Inner | undefined
  readonly b: Outer | undefined
  readonly isKrate: true
}
declare function krate<Alpha = unknown, Beta = unknown>(): Krate<Alpha, Beta>
type Swap<Lhs, Rhs> = Krate<Rhs, Lhs>
function build(): Swap<boolean, string> | Krate<boolean, string> {
  return krate()
}
"#,
        "Type 'Krate<string | boolean, string | boolean>' is not assignable to type",
    );
}

/// A direct arm whose arguments are DISJOINT from the alias arm's as-written
/// spelling (`FlipRow<string, number> | PairRow<boolean, symbol>`): the
/// forwarded decomposition contributes `A := number`, so no accidental
/// agreement with the direct arm can pin the ambiguous union. tsc:
/// `Type 'PairRow<number | boolean, string | symbol>' is not assignable to
/// type 'FlipRow<string, number> | PairRow<boolean, symbol>'.`
#[test]
fn disjoint_direct_arm_stays_unpinned_and_merges() {
    assert_single_ts2322(
        r#"
function probe(): FlipRow<string, number> | PairRow<boolean, symbol> {
  return pairCall()
}
"#,
        "Type 'PairRow<number | boolean, string | symbol>' is not assignable to type 'FlipRow<string, number> | PairRow<boolean, symbol>'.",
    );
}
