//! Defaulted-parameter `undefined`-strip for DEFERRED annotation types.
//!
//! Structural rule: a parameter with a default initializer has `undefined`
//! stripped from its declared type (tsc's `getTypeForVariableLikeDeclaration`
//! -> `getTypeWithFacts(type, ~Undefined)`), so the body sees the non-undefined
//! type. tsz's strip runs at the solver boundary and no-ops on a DEFERRED
//! annotation (indexed-access / conditional / mapped) that is not yet a surface
//! union — leaking `undefined` into the body and producing false TS18048 /
//! TS2488 / TS2322 (witness: jotai `src/babel/utils.ts` `customAtomNames:
//! PluginOptions['customAtomNames'] = []`). The fix resolves the deferred
//! annotation (via the checker context as the type resolver) before stripping.
//!
//! Owner: `flow/flow_analysis/definite.rs` defaulted-parameter initial-type step.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS18048: u32 = 18048; // 'x' is possibly 'undefined'.
const TS2488: u32 = 2488; // Type must have a '[Symbol.iterator]()' method.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn defaulted_param_with_indexed_access_annotation_strips_undefined() {
    // `names: Opts['names'] = []` then `names.length` — tsc clean, was TS18048.
    let codes = check_strict(
        r#"
interface Opts { names?: string[]; }
function read(names: Opts["names"] = []) {
  return names.length;
}
"#,
    );
    assert_eq!(
        count(&codes, TS18048),
        0,
        "deferred-default must strip undefined: {codes:?}"
    );
}

#[test]
fn defaulted_param_with_conditional_annotation_strips_undefined() {
    let codes = check_strict(
        r#"
type Cond<B extends boolean> = B extends true ? string[] | undefined : number;
function read(xs: Cond<true> = []) {
  return xs.length;
}
"#,
    );
    assert_eq!(
        count(&codes, TS18048),
        0,
        "conditional deferred-default must strip undefined: {codes:?}"
    );
}

#[test]
fn defaulted_param_with_mapped_index_annotation_strips_undefined() {
    let codes = check_strict(
        r#"
interface Opts { names?: string[]; }
type Pick1<T, K extends keyof T> = { [P in K]: T[P] }[K];
function read(xs: Pick1<Opts, "names"> = []) {
  return xs.length;
}
"#,
    );
    assert_eq!(
        count(&codes, TS18048),
        0,
        "mapped-index deferred-default must strip undefined: {codes:?}"
    );
}

#[test]
fn eager_union_default_control_stays_clean() {
    // Already-resolved union annotation — worked before the fix, must stay clean.
    let codes = check_strict(
        r#"
type U = string[] | undefined;
function read(names: U = []) {
  return names.length;
}
"#,
    );
    assert_eq!(count(&codes, TS18048), 0);
}

// (The "no-default optional keeps undefined" true-positive is verified at the
// CLI/canary level; it is structurally guaranteed here because the fix is gated
// on `param.initializer.is_some()`, so a no-default parameter never reaches the
// strip. It is omitted as a unit test because the unit-test lib resolves the
// possibly-undefined access differently from the full CLI.)

#[test]
fn defaulted_param_deferred_spread_is_iterable() {
    // The spread form of the same root (was false TS2488 on `string[] | undefined`).
    let codes = check_strict(
        r#"
interface Opts { names?: string[]; }
function read(names: Opts["names"] = []) {
  return [...names];
}
"#,
    );
    assert_eq!(
        count(&codes, TS2488),
        0,
        "deferred-default spread must be iterable: {codes:?}"
    );
}
