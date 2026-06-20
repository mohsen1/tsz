//! `typeof globalThis` is a concrete object type, not `any`.
//!
//! Structural rule: `typeof globalThis` is the type of the global object — a
//! concrete object whose members are the globally-visible value bindings
//! (`var`/`function`/`class`, plus lib globals). tsz previously lowered it to
//! `any`, which produced two `tsc` divergences:
//!   1. (FP) `typeof globalThis extends X ? T : F` distributed like `any`,
//!      yielding `T | F` instead of resolving the concrete branch — so an
//!      assignment of the apparent union to one branch wrongly emitted TS2322.
//!   2. (FN) `globalThis[key]` element access with a non-literal-node string
//!      key never reached the implicit-any diagnostic, so the TS7053 that `tsc`
//!      emits under `--strict` (`typeof globalThis` has no index signature) was
//!      missing.
//!
//! Owner: `CheckerContext::global_this_surface_type` (surface object) consulted
//! by the `typeof` lowering overrides; `types/computation/access.rs` for the
//! element-access TS7053.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2322: u32 = 2322; // Type X is not assignable to type Y.
const TS7053: u32 = 7053; // Element implicitly has an 'any' type (no index sig).

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Part A — `typeof globalThis` in a conditional check type resolves concretely.
// ---------------------------------------------------------------------------

#[test]
fn typeof_global_this_extends_object_takes_true_branch() {
    // The conditional must resolve to "TRUE"; if it stays deferred its apparent
    // type is the union of both branches and the assignment emits TS2322.
    let codes = check_strict(
        r#"
type Verdict = typeof globalThis extends object ? "TRUE" : "FALSE";
const y: "TRUE" = null as unknown as Verdict;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        0,
        "`typeof globalThis extends object` is true, so Verdict is \"TRUE\": {codes:?}"
    );
}

#[test]
fn typeof_global_this_extends_missing_member_takes_false_branch() {
    // The surface object lacks an arbitrary member, so the conditional resolves
    // to the false branch concretely (not a deferred union).
    let codes = check_strict(
        r#"
type Picked = typeof globalThis extends { __definitely_absent: number } ? "Y" : "N";
const picked: "N" = null as unknown as Picked;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        0,
        "absent member means the false branch \"N\" is taken: {codes:?}"
    );
}

#[test]
fn typeof_global_this_as_generic_argument_resolves_concretely() {
    // The same surface must flow through a generic application (a distinct
    // lowering path) — alias name varied to avoid name-coupled behavior.
    let codes = check_strict(
        r#"
type IsObjectShape<Candidate> = Candidate extends object ? 1 : 2;
const flag: IsObjectShape<typeof globalThis> = 1;
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        0,
        "Cond<typeof globalThis> must reduce to 1, not stay deferred: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Part B — `globalThis[key]` with a non-literal-node string key emits TS7053.
// ---------------------------------------------------------------------------

#[test]
fn global_this_indexed_by_missing_literal_typed_key_emits_ts7053() {
    // `slot`'s type is the string literal "missingGlobal"; the AST node is an
    // identifier, not a string literal, so the literal-node branch does not
    // fire. The key is not a global member → TS7053.
    let codes = check_strict(
        r#"
const slot = "missingGlobal";
export function read(): unknown {
  return globalThis[slot];
}
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "indexing globalThis with an absent literal-typed key must emit TS7053: {codes:?}"
    );
}

#[test]
fn global_this_indexed_by_general_string_emits_ts7053() {
    let codes = check_strict(
        r#"
declare const anyKey: string;
export function read(): unknown {
  return globalThis[anyKey];
}
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "`typeof globalThis` has no string index signature → TS7053: {codes:?}"
    );
}

#[test]
fn global_this_indexed_by_present_global_var_literal_key_is_clean() {
    // A key whose literal type names a real global `var` resolves to that
    // value's type — no TS7053. (Self-contained: declares the global in-source
    // so the assertion does not depend on which lib globals the harness loads.)
    let codes = check_strict(
        r#"
var presentGlobal: number;
const slot = "presentGlobal";
export function read(): number {
  return globalThis[slot];
}
"#,
    );
    assert_eq!(
        count(&codes, TS7053),
        0,
        "a resolvable global `var` key must not be flagged: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2322),
        0,
        "the resolved member type (number) is assignable to the return type: {codes:?}"
    );
}

#[test]
fn global_this_indexed_by_block_scoped_binding_emits_ts7053() {
    // `let`/`const` are NOT members of `typeof globalThis` (only `var`/
    // `function`/`class` are), so indexing by such a name is still TS7053.
    let codes = check_strict(
        r#"
const blockScoped = 1;
const slot = "blockScoped";
export function read(): unknown {
  return globalThis[slot];
}
"#,
    );
    assert!(
        codes.contains(&TS7053),
        "a block-scoped binding is not a globalThis member → TS7053: {codes:?}"
    );
}
