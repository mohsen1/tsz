//! When a distributive mapped type produces a union, a conditional whose check
//! side is a naked type parameter bound to that mapped result must distribute
//! across the union members even when the substituted check side flows through
//! an identity alias (`Id<T> = T`) or an infer-passthrough alias
//! (`T extends infer U ? U : never`). tsc treats all three forms identically;
//! tsz previously evaluated the alias-wrapped forms as `never`.
//!
//! Each scenario asserts both halves of the rule:
//!   1. The legitimate per-member assignment compiles with no diagnostics,
//!      proving distribution returned the correct branch.
//!   2. A wrong-type and a wrong-property reference on the same value fire
//!      TS2322 and TS2339 respectively, proving the result is neither `any`
//!      (which would silence the type mismatch) nor the other union branch
//!      (which would silence the missing-property error).

use tsz_checker::test_utils::check_source_diagnostics;

/// The source under test is constructed so that, when the rule holds, the
/// checker emits exactly the diagnostics listed in `expected_codes` and no
/// others. Importantly, the harness does NOT filter `TS2304` / `TS2318`: a
/// typo in a test-local identifier (`Extract`, `ReplaceKeys`, `Id`, ...)
/// would surface as an unfiltered missing-name diagnostic and fail the test.
#[track_caller]
fn assert_diagnostic_codes(source: &str, expected_codes: &[u32]) {
    let diagnostics = check_source_diagnostics(source);
    let actual: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        actual, expected_codes,
        "Expected diagnostic codes {expected_codes:?}, got: {diagnostics:#?}",
    );
}

const PREAMBLE: &str = r#"
type Extract<T, U> = T extends U ? T : never;
type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T
    ? K extends keyof Y
      ? Y[K]
      : never
    : U[K]
};
type NodeA = { type: "A"; name: string };
type NodeB = { type: "B"; id: number };
type Replaced = ReplaceKeys<NodeA | NodeB, "name", { name: number }>;
"#;

/// Standard shape-proving probes for a value bound to the A-branch of
/// `Replaced` (i.e., `{ type: "A"; name: number }`):
///   - line 1: legitimate `number` assignment compiles (no diagnostic).
///   - line 2: `string` assignment fires TS2322 — proves the property type is
///     `number`, not `any`.
///   - line 3: `.id` access fires TS2339 — proves the extracted member is the
///     A branch (which has no `id`), not the B branch.
const A_BRANCH_SHAPE_PROBES: &str = r#"
const aName: number = a.name;
const aWrongType: string = a.name;
const aWrongBranch: number = a.id;
"#;

const A_BRANCH_EXPECTED: &[u32] = &[2322, 2339];

/// Baseline: the distributive Extract directly against the mapped result.
#[test]
fn extract_on_distributive_mapped_result_direct() {
    let source = format!(
        r#"{PREAMBLE}
type ExtractedA = Extract<Replaced, {{ type: "A" }}>;
declare const a: ExtractedA;
{A_BRANCH_SHAPE_PROBES}
"#,
    );
    assert_diagnostic_codes(&source, A_BRANCH_EXPECTED);
}

/// Identity alias around the distributive mapped result: `Extract<Id<R>, ...>`.
#[test]
fn extract_through_identity_alias_distributes() {
    let source = format!(
        r#"{PREAMBLE}
type Id<T> = T;
type ExtractedA = Extract<Id<Replaced>, {{ type: "A" }}>;
declare const a: ExtractedA;
{A_BRANCH_SHAPE_PROBES}
"#,
    );
    assert_diagnostic_codes(&source, A_BRANCH_EXPECTED);
}

/// Infer-passthrough alias around the distributive mapped result.
#[test]
fn extract_through_infer_passthrough_alias_distributes() {
    let source = format!(
        r#"{PREAMBLE}
type Unwrap<T> = T extends infer U ? U : never;
type ExtractedA = Extract<Unwrap<Replaced>, {{ type: "A" }}>;
declare const a: ExtractedA;
{A_BRANCH_SHAPE_PROBES}
"#,
    );
    assert_diagnostic_codes(&source, A_BRANCH_EXPECTED);
}

/// Same-file co-use of the direct and identity-wrapped forms. Previously the
/// alias-wrapped form's incorrect result was returned for the direct form too
/// because the Application cache shared a single key for `Extract<Id<R>, ...>`
/// after the alias body had been substituted to `R`.
#[test]
fn extract_direct_and_identity_alias_coexist() {
    let source = format!(
        r#"{PREAMBLE}
type Id<T> = T;
type ExtractedDirect = Extract<Replaced, {{ type: "A" }}>;
type ExtractedAliased = Extract<Id<Replaced>, {{ type: "A" }}>;
declare const a: ExtractedDirect;
declare const b: ExtractedAliased;
const directOk: number = a.name;
const directWrong: number = a.id;
const aliasedOk: number = b.name;
const aliasedWrong: number = b.id;
"#,
    );
    // Two TS2339 diagnostics — one per `.id` access — and no `any` escape.
    assert_diagnostic_codes(&source, &[2339, 2339]);
}

/// Iteration variable renamed (`P` instead of `K`) — the rule must be
/// structural, not keyed on a specific identifier.
#[test]
fn extract_through_identity_alias_renamed_iteration_var() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type ReplaceKeys<U, T, Y> = {
  [P in keyof U]: P extends T
    ? P extends keyof Y
      ? Y[P]
      : never
    : U[P]
};
type NodeA = { type: "A"; name: string };
type NodeB = { type: "B"; id: number };
type Replaced = ReplaceKeys<NodeA | NodeB, "name", { name: number }>;
type Id<T> = T;
type ExtractedA = Extract<Id<Replaced>, { type: "A" }>;
declare const a: ExtractedA;
const aName: number = a.name;
const aWrongType: string = a.name;
const aWrongBranch: number = a.id;
"#;
    assert_diagnostic_codes(source, A_BRANCH_EXPECTED);
}

/// Three-member union under the alias-wrapped Extract. The extracted member
/// (`C`) carries `cProp: boolean`, so the negative probes are typed against
/// the A-branch and B-branch properties, both of which must be missing.
#[test]
fn extract_through_identity_alias_three_member_union() {
    let source = r#"
type Extract<T, U> = T extends U ? T : never;
type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T
    ? K extends keyof Y
      ? Y[K]
      : never
    : U[K]
};
type NA = { kind: "a"; aProp: string };
type NB = { kind: "b"; bProp: number };
type NC = { kind: "c"; cProp: boolean };
type Replaced = ReplaceKeys<NA | NB | NC, "aProp", { aProp: number }>;
type Id<T> = T;
type ExtractedC = Extract<Id<Replaced>, { kind: "c" }>;
declare const c: ExtractedC;
const cOk: boolean = c.cProp;
const cWrongType: number = c.cProp;
const cFromA: string = c.aProp;
const cFromB: number = c.bProp;
"#;
    // TS2322 from `number = boolean`; TS2551 ("did you mean") for both
    // missing props because the property names share a 4-char prefix with
    // `cProp`. Both diagnostic codes are the checker's missing-property
    // surface. Proves C is the unique extracted branch.
    assert_diagnostic_codes(source, &[2322, 2551, 2551]);
}
