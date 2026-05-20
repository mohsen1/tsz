//! Regression coverage for #8725 / templateLiteralTypes6 parity.
//!
//! Structural rule: when an outer-scope type parameter `E` flows into a
//! generic call whose corresponding callee parameter is itself a type
//! parameter `E_callee` (possibly via a template literal `${E_callee}`),
//! the substitution must preserve `E`'s `TypeId`. Walking the constraint
//! chain of `E` during instantiation must not produce a different `TypeId`
//! merely because of re-interning side effects when the call's
//! substitution doesn't actually reach any name inside that chain.
//!
//! Each test deliberately renames the inner identifiers (`T`/`U`,
//! `Scope`/`Event`, `Outer`/`Inner`, `Side`) so the structural rule cannot
//! be hardcoded to a specific spelling per CLAUDE.md §25.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn compile(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn diag_codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn keyof_constraint_flows_through_generic_call_passthrough() {
    // Minimal core repro: `f1<T, E extends keyof T>(p: E)` called with
    // `e: E` from an outer scope with the same shape.
    let source = r#"
declare function inner<TI, EI extends keyof TI>(p: EI): void;
function outer<TO, EO extends keyof TO>(_t: TO, e: EO) {
  inner(e);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "outer `EO extends keyof TO` should flow through `EI extends keyof TI` \
         without producing a TS2345; got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn keyof_constraint_flows_through_intersected_with_string() {
    // Same rule, with `& string` intersection on the constraint.
    let source = r#"
declare function inner<TA, EA extends keyof TA & string>(p: EA): void;
function outer<TB, EB extends keyof TB & string>(_t: TB, e: EB) {
  inner(e);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "intersected `& string` must not perturb passthrough; got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn keyof_constraint_flows_through_renamed_type_params() {
    // Rename axis (CLAUDE.md §25): the fix must not be keyed on `T`/`E`.
    let source = r#"
declare function inner<Alpha, Bravo extends keyof Alpha>(p: Bravo): void;
function outer<Foo, Bar extends keyof Foo>(_t: Foo, e: Bar) {
  inner(e);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "renamed type params (Alpha/Bravo, Foo/Bar) must still pass through; got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn keyof_indexed_access_dependent_constraint_passes_through() {
    // The dependent-constraint axis: `EB extends keyof RB[SB]` where both
    // `SB` and `RB[SB]` reference outer-scope names.
    let source = r#"
type Rec = { a: { a1: 1 }; b: { b1: 1 } };
declare function inner<S1 extends keyof Rec & string, E1 extends keyof Rec[S1] & string>(p: E1): void;
function outer<S2 extends keyof Rec & string, E2 extends keyof Rec[S2] & string>(_s: S2, e: E2) {
  inner(e);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "indexed-access dependent constraint must pass through unchanged; \
         got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn dependent_constraint_through_template_literal_parameter() {
    // The conformance fixture shape: a template literal parameter built
    // from two type parameters whose constraints chain through `keyof R[S]`.
    // tsc emits zero diagnostics; tsz must too.
    let source = r#"
type Reg = { a: { a1: {} }; b: { b1: {} } };
type Keyof<X> = keyof X & string;
declare function fInner<
  ScopeI extends Keyof<Reg>,
  EventI extends Keyof<Reg[ScopeI]>,
>(eventPath: `${ScopeI}:${EventI}`): void;
function fOuter<
  ScopeO extends Keyof<Reg>,
  EventO extends Keyof<Reg[ScopeO]>,
>(scope: ScopeO, event: EventO) {
  fInner(`${scope}:${event}`);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "templateLiteralTypes6-style passthrough must be accepted; got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn template_literal_with_single_dependent_type_param() {
    // A simpler variant: only one type parameter `EE` with an outer-scope
    // dependency, embedded in a template literal parameter.
    let source = r#"
type Rec = { a: { a1: 1 }; b: { b1: 1 } };
declare function inner<EE extends keyof Rec["a"] & string>(p: `${EE}`): void;
function outer<EE2 extends keyof Rec["a"] & string>(event: EE2) {
  inner(`${event}`);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "single-param template literal passthrough must work; got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn explicit_type_args_match_inferred_passthrough() {
    // Sanity check: when type arguments are explicit, the same call shape
    // must continue to pass. Establishes that the explicit-args path was
    // never the regressing case.
    let source = r#"
declare function inner<TI, EI extends keyof TI>(p: EI): void;
function outer<TO, EO extends keyof TO>(t: TO, e: EO) {
  inner<TO, EO>(e);
}
"#;
    let diags = compile(source);
    assert!(
        diags.is_empty(),
        "explicit-type-args call must continue to pass; got {:?}",
        diag_codes(&diags)
    );
}

#[test]
fn unrelated_type_mismatch_still_errors() {
    // Negative-case anchor: when the inner call truly mismatches the
    // outer-scope type parameter, the diagnostic must still fire. This
    // proves the fix is not blanket-accepting calls with dependent
    // constraints — it only preserves identity when the substitution
    // does not actually reach the outer-scope type parameter.
    let source = r#"
declare function inner<II extends "x" | "y">(p: II): void;
function outer<OO extends "a" | "b">(e: OO) {
  inner(e);
}
"#;
    let diags = compile(source);
    assert!(
        diags.iter().any(|d| d.code == 2345),
        "unrelated constraint mismatch must still surface TS2345; got {:?}",
        diag_codes(&diags)
    );
}
