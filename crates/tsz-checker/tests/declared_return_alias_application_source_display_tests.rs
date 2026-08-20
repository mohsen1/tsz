//! Source display of an instantiated alias-application declared return type.
//!
//! When a call's declared return type is a generic alias application
//! (`declare function flipCall<A, B>(): FlipRow<A, B>` with
//! `type FlipRow<X, Y> = PairRow<Y, X>`) and return-context inference fills
//! the alias's own parameters, `tsc` renders the assignability SOURCE with
//! the written alias spelling and the inferred arguments substituted in the
//! alias's parameter positions (`FlipRow<number | symbol, string | boolean>`),
//! while each per-arm elaboration line renders the instantiated underlying
//! base (`PairRow<string | boolean, number | symbol>`). tsz must do the same
//! through the solver's instantiation display provenance; before the fix it
//! rendered the forwarded base in the head too.
//!
//! Oracle: typescript@7.0.2 via `scripts/conformance/oracle.sh` (`--strict`),
//! byte-pinned chains recorded per case. This is the SOURCE-half sibling of
//! the target-half alias display fences (#17756/#17775); the inference
//! behavior itself is `permuted_alias_return_context_inference_tests.rs`'s
//! subject (#17677/#17696).

use tsz_checker::test_utils::{check_with_options, strict_checker_options};
use tsz_common::diagnostics::Diagnostic;

/// Assert the single diagnostic of `code` renders exactly this elaboration
/// chain: the primary message at depth 0 followed by its related-information
/// `(depth + 1, text)` pairs.
fn assert_exact_chain(source: &str, code: u32, expected: &[(u8, &str)]) {
    let diags = check_with_options(source, strict_checker_options());
    let matching: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut chain = vec![(0u8, matching[0].message_text.clone())];
    chain.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| (info.depth + 1, info.message_text.clone())),
    );
    let rendered: Vec<(u8, &str)> = chain.iter().map(|(d, m)| (*d, m.as_str())).collect();
    assert_eq!(rendered, expected, "chain mismatch for:\n{source}");
}

/// As [`assert_exact_chain`], but checks only the first `expected.len()`
/// chain entries — used where a later drill line has its own pinned residual.
fn assert_chain_prefix(source: &str, code: u32, expected: &[(u8, &str)]) {
    let diags = check_with_options(source, strict_checker_options());
    let matching: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut chain = vec![(0u8, matching[0].message_text.clone())];
    chain.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| (info.depth + 1, info.message_text.clone())),
    );
    let rendered: Vec<(u8, &str)> = chain
        .iter()
        .take(expected.len())
        .map(|(d, m)| (*d, m.as_str()))
        .collect();
    assert_eq!(rendered, expected, "chain prefix mismatch for:\n{source}");
}

fn assert_clean(source: &str, context: &str) {
    let diags = check_with_options(source, strict_checker_options());
    assert!(
        diags.is_empty(),
        "{context}: expected no diagnostics, got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

const PAIR: &str = r#"
interface PairRow<First, Second> {
  readonly first: First | undefined
  readonly second: Second | undefined
  readonly isPairRow: true
}
"#;

/// Symmetric inferred arguments: the head keeps the written `FlipRow`
/// spelling; the per-arm elaboration renders the instantiated base.
#[test]
fn permuting_alias_declared_return_head_keeps_alias_spelling() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
declare function flipCall<A = unknown, B = unknown>(): FlipRow<A, B>
function build(): PairRow<string, number> | PairRow<number, string> {{
  return flipCall()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'FlipRow<string | number, string | number>' is not assignable to type 'PairRow<string, number> | PairRow<number, string>'.",
            ),
            (
                1,
                "Type 'PairRow<string | number, string | number>' is not assignable to type 'PairRow<string, number>'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

/// Asymmetric inferred arguments pin the ARGUMENT ORDER: the head substitutes
/// the inferred `A`/`B` in the alias's own parameter positions
/// (`FlipRow<A, B>` with `A := number | symbol`, `B := string | boolean`);
/// the elaboration's base view swaps them (`PairRow<B, A>`).
#[test]
fn asymmetric_arguments_keep_alias_parameter_order() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
declare function flipCall<A = unknown, B = unknown>(): FlipRow<A, B>
function build(): PairRow<string, number> | PairRow<boolean, symbol> {{
  return flipCall()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'FlipRow<number | symbol, string | boolean>' is not assignable to type 'PairRow<string, number> | PairRow<boolean, symbol>'.",
            ),
            (
                1,
                "Type 'PairRow<string | boolean, number | symbol>' is not assignable to type 'PairRow<string, number>'.",
            ),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'string'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

/// A declared-order passthrough alias (`Fwd<X, Y> = PairRow<X, Y>`) keeps its
/// own spelling too — alias transparency is an inference rule, not a display
/// rewrite.
#[test]
fn declared_order_forwarding_alias_keeps_alias_spelling() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type Fwd<X, Y> = PairRow<X, Y>
declare function fwdCall<A = unknown, B = unknown>(): Fwd<A, B>
function build(): PairRow<string, number> | PairRow<number, string> {{
  return fwdCall()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'Fwd<string | number, string | number>' is not assignable to type 'PairRow<string, number> | PairRow<number, string>'.",
            ),
            (
                1,
                "Type 'PairRow<string | number, string | number>' is not assignable to type 'PairRow<string, number>'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

/// A repeating alias (`DupRow<T> = PairRow<T, T>`) renders its single
/// combined argument.
#[test]
fn repeating_alias_declared_return_keeps_alias_spelling() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type DupRow<T> = PairRow<T, T>
declare function dupCall<T = unknown>(): DupRow<T>
function build(): PairRow<string, number> | PairRow<number, string> {{
  return dupCall()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'DupRow<string | number>' is not assignable to type 'PairRow<string, number> | PairRow<number, string>'.",
            ),
            (
                1,
                "Type 'PairRow<string | number, string | number>' is not assignable to type 'PairRow<string, number>'.",
            ),
            (
                2,
                "Type 'string | number' is not assignable to type 'string'.",
            ),
            (3, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

/// A two-hop permuting chain (`FlipAgain<P, Q> = FlipRow<Q, P>`) keeps the
/// OUTERMOST written alias; the elaboration composes both remaps
/// (`FlipAgain<P, Q>` ≡ `PairRow<P, Q>`).
#[test]
fn two_hop_permuting_chain_keeps_outer_alias_spelling() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
type FlipAgain<P, Q> = FlipRow<Q, P>
declare function flip2Call<A = unknown, B = unknown>(): FlipAgain<A, B>
function build(): PairRow<string, number> | PairRow<boolean, symbol> {{
  return flip2Call()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'FlipAgain<string | boolean, number | symbol>' is not assignable to type 'PairRow<string, number> | PairRow<boolean, symbol>'.",
            ),
            (
                1,
                "Type 'PairRow<string | boolean, number | symbol>' is not assignable to type 'PairRow<string, number>'.",
            ),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'string'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

/// Alias arms in the TARGET keep their written spelling while the per-arm
/// elaboration still renders the SOURCE through its base view. The drill
/// beneath the frame has its own pinned residual (next test), so only the
/// head + frame are asserted here.
#[test]
fn alias_target_arms_render_written_spelling_with_base_view_elaboration() {
    assert_chain_prefix(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
declare function flipCall<A = unknown, B = unknown>(): FlipRow<A, B>
function build(): FlipRow<string, number> | FlipRow<boolean, symbol> {{
  return flipCall()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'FlipRow<string | boolean, number | symbol>' is not assignable to type 'FlipRow<string, number> | FlipRow<boolean, symbol>'.",
            ),
            (
                1,
                "Type 'PairRow<number | symbol, string | boolean>' is not assignable to type 'FlipRow<string, number>'.",
            ),
        ],
    );
}

/// Residual (`#[ignore]`d, red on main): the member frame's DRILL pair.
/// Oracle 7.0.2 drills the first mismatching argument of the base-aligned
/// pair (`PairRow<number | symbol, string | boolean>` vs
/// `PairRow<number, string>`: `number | symbol` vs `number`); tsz's
/// same-base relation drill picks the pair through the alias-application
/// alignment (`string | boolean` vs `string`). Owner: the solver's same-base
/// argument-drill selection for alias-of-application sources (relation
/// failure reason, not display) — the display halves above are fixed.
#[test]
#[ignore = "solver same-base argument drill picks the alias-aligned pair; oracle 7.0.2 drills the base-aligned first mismatch"]
fn alias_target_arms_drill_aligns_through_base_view() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
declare function flipCall<A = unknown, B = unknown>(): FlipRow<A, B>
function build(): FlipRow<string, number> | FlipRow<boolean, symbol> {{
  return flipCall()
}}
"
        ),
        2322,
        &[
            (
                0,
                "Type 'FlipRow<string | boolean, number | symbol>' is not assignable to type 'FlipRow<string, number> | FlipRow<boolean, symbol>'.",
            ),
            (
                1,
                "Type 'PairRow<number | symbol, string | boolean>' is not assignable to type 'FlipRow<string, number>'.",
            ),
            (
                2,
                "Type 'number | symbol' is not assignable to type 'number'.",
            ),
            (3, "Type 'symbol' is not assignable to type 'number'."),
        ],
    );
}

/// Renamed binders end-to-end (interface, alias, and call parameters all
/// disjoint names) — the rule keys on structure, not names.
#[test]
fn renamed_binders_keep_alias_parameter_order() {
    assert_exact_chain(
        r#"
interface Krate<Inner, Outer> {
  readonly a: Inner | undefined
  readonly b: Outer | undefined
  readonly isKrate: true
}
type Swap<Lhs, Rhs> = Krate<Rhs, Lhs>
declare function swapCall<Alpha = unknown, Beta = unknown>(): Swap<Alpha, Beta>
function build(): Krate<string, number> | Krate<boolean, symbol> {
  return swapCall()
}
"#,
        2322,
        &[
            (
                0,
                "Type 'Swap<number | symbol, string | boolean>' is not assignable to type 'Krate<string, number> | Krate<boolean, symbol>'.",
            ),
            (
                1,
                "Type 'Krate<string | boolean, number | symbol>' is not assignable to type 'Krate<string, number>'.",
            ),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'string'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

/// TS2345 argument position: an annotated const whose type IS the written
/// alias application keeps the alias in the argument head the same way. The
/// TS2345 member frame has its own pinned residual (next test), so only the
/// head is asserted here.
#[test]
fn ts2345_argument_position_keeps_alias_spelling() {
    assert_chain_prefix(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
declare function take(row: PairRow<string, number> | PairRow<boolean, symbol>): void
declare const flipped: FlipRow<number | symbol, string | boolean>
take(flipped)
"
        ),
        2345,
        &[(
            0,
            "Argument of type 'FlipRow<number | symbol, string | boolean>' is not assignable to parameter of type 'PairRow<string, number> | PairRow<boolean, symbol>'.",
        )],
    );
}

/// Residual (`#[ignore]`d, red on main): the TS2345 argument elaboration's
/// member frame renders the source alias application as written
/// (`FlipRow<...>`); oracle 7.0.2 renders the base view
/// (`PairRow<string | boolean, number | symbol>`), exactly like the TS2322
/// member frame fixed in this suite. Owner: the TS2345 union-parameter
/// elaboration route (it does not flow through
/// `render_union_target_member_frame_mismatch`, so it misses the base-view
/// hop applied there).
#[test]
#[ignore = "TS2345 member frame keeps the alias spelling; oracle 7.0.2 renders the base view"]
fn ts2345_argument_member_frame_renders_base_view() {
    assert_exact_chain(
        &format!(
            "{PAIR}
type FlipRow<X, Y> = PairRow<Y, X>
declare function take(row: PairRow<string, number> | PairRow<boolean, symbol>): void
declare const flipped: FlipRow<number | symbol, string | boolean>
take(flipped)
"
        ),
        2345,
        &[
            (
                0,
                "Argument of type 'FlipRow<number | symbol, string | boolean>' is not assignable to parameter of type 'PairRow<string, number> | PairRow<boolean, symbol>'.",
            ),
            (
                1,
                "Type 'PairRow<string | boolean, number | symbol>' is not assignable to type 'PairRow<string, number>'.",
            ),
            (
                2,
                "Type 'string | boolean' is not assignable to type 'string'.",
            ),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

/// Negative control: an alias whose body mixes a concrete argument
/// (`Half<X> = PairRow<X, number>`) declines the transparency hop and the
/// whole return stays clean, matching tsc.
#[test]
fn concrete_argument_alias_body_stays_clean() {
    assert_clean(
        &format!(
            "{PAIR}
type Half<X> = PairRow<X, number>
declare function halfCall<A = unknown>(): Half<A>
function build(): PairRow<string, number> | PairRow<boolean, number> {{
  return halfCall()
}}
"
        ),
        "concrete-argument alias body",
    );
}
