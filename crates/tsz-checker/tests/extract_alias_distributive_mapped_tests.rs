//! When a distributive mapped type produces a union, a conditional whose check
//! side is a naked type parameter bound to that mapped result must distribute
//! across the union members even when the substituted check side flows through
//! an identity alias (`Id<T> = T`) or an infer-passthrough alias
//! (`T extends infer U ? U : never`). tsc treats all three forms identically;
//! tsz previously evaluated the alias-wrapped forms as `never`.

use tsz_checker::test_utils::check_source_diagnostics;

fn no_relevant_diagnostics(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    // The lib-free harness fires TS2318 / TS2304 for missing built-in names
    // like `Object`; only the type-rule failures matter for these tests.
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no diagnostics, got: {relevant:#?}"
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

/// Baseline: the distributive Extract directly against the mapped result.
#[test]
fn extract_on_distributive_mapped_result_direct() {
    no_relevant_diagnostics(&format!(
        r#"{PREAMBLE}
type ExtractedA = Extract<Replaced, {{ type: "A" }}>;
declare const a: ExtractedA;
const aName: number = a.name;
"#,
    ));
}

/// Identity alias around the distributive mapped result: `Extract<Id<R>, ...>`.
#[test]
fn extract_through_identity_alias_distributes() {
    no_relevant_diagnostics(&format!(
        r#"{PREAMBLE}
type Id<T> = T;
type ExtractedA = Extract<Id<Replaced>, {{ type: "A" }}>;
declare const a: ExtractedA;
const aName: number = a.name;
"#,
    ));
}

/// Infer-passthrough alias around the distributive mapped result.
#[test]
fn extract_through_infer_passthrough_alias_distributes() {
    no_relevant_diagnostics(&format!(
        r#"{PREAMBLE}
type Unwrap<T> = T extends infer U ? U : never;
type ExtractedA = Extract<Unwrap<Replaced>, {{ type: "A" }}>;
declare const a: ExtractedA;
const aName: number = a.name;
"#,
    ));
}

/// Same-file co-use of the direct and identity-wrapped forms. Previously the
/// alias-wrapped form's incorrect result was returned for the direct form too
/// because the Application cache shared a single key for `Extract<Id<R>, ...>`
/// after the alias body had been substituted to `R`.
#[test]
fn extract_direct_and_identity_alias_coexist() {
    no_relevant_diagnostics(&format!(
        r#"{PREAMBLE}
type Id<T> = T;
type ExtractedDirect = Extract<Replaced, {{ type: "A" }}>;
type ExtractedAliased = Extract<Id<Replaced>, {{ type: "A" }}>;
declare const direct: ExtractedDirect;
declare const aliased: ExtractedAliased;
const a1: number = direct.name;
const a2: number = aliased.name;
"#,
    ));
}

/// Iteration variable renamed (`P` instead of `K`) — the rule must be
/// structural, not keyed on a specific identifier.
#[test]
fn extract_through_identity_alias_renamed_iteration_var() {
    no_relevant_diagnostics(
        r#"
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
"#,
    );
}

/// Three-member union under the alias-wrapped Extract.
#[test]
fn extract_through_identity_alias_three_member_union() {
    no_relevant_diagnostics(
        r#"
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
const ok: boolean = c.cProp;
"#,
    );
}
