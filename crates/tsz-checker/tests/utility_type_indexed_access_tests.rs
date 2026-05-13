//! Indexed access on utility-wrapped mapped types must evaluate correctly.
//!
//! When a mapped type's constraint is stored in deferred form (e.g.
//! `keyof Pick<T,K>` evaluates to `K`), indexing with `K` must succeed.
//! tsc accepts all patterns below without error; tsz must match.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

#[test]
fn required_pick_indexed_by_k_no_error() {
    let source = r#"
function requiredPick<T, K extends keyof T>(obj: Required<T>, keys: K[]): Required<Pick<T, K>> {
  const result = {} as Required<Pick<T, K>>;
  for (const k of keys) {
    result[k] = obj[k];
  }
  return result;
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for Required<Pick<T,K>>[K] assignment, got: {diags:#?}"
    );
}

/// Different type-parameter names prove the fix is structural, not name-dependent.
#[test]
fn required_pick_indexed_different_names_no_error() {
    let source = r#"
function requiredPick<S, F extends keyof S>(obj: Required<S>, keys: F[]): Required<Pick<S, F>> {
  const result = {} as Required<Pick<S, F>>;
  for (const f of keys) {
    result[f] = obj[f];
  }
  return result;
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics (names S/F), got: {diags:#?}"
    );
}

#[test]
fn required_pick_return_position_no_error() {
    let source = r#"
function getRequired<T, K extends keyof T>(obj: Required<T>, k: K): Required<Pick<T, K>>[K] {
  return obj[k];
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics in return position, got: {diags:#?}"
    );
}

#[test]
fn partial_pick_indexed_by_k_no_error() {
    let source = r#"
function partialPick<T, K extends keyof T>(obj: T, k: K): Partial<Pick<T, K>>[K] {
  return obj[k];
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for Partial<Pick<T,K>>[K], got: {diags:#?}"
    );
}

/// Different type-parameter names for the Partial variant.
#[test]
fn partial_pick_indexed_different_names_no_error() {
    let source = r#"
function partialPick<Item, Keys extends keyof Item>(obj: Item, k: Keys): Partial<Pick<Item, Keys>>[Keys] {
  return obj[k];
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics (names Item/Keys), got: {diags:#?}"
    );
}

#[test]
fn readonly_pick_indexed_by_k_no_error() {
    let source = r#"
function readonlyPick<T, K extends keyof T>(obj: T, k: K): Readonly<Pick<T, K>>[K] {
  return obj[k];
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for Readonly<Pick<T,K>>[K], got: {diags:#?}"
    );
}

/// Double-wrapping: the fix must propagate through multiple utility layers.
#[test]
fn required_readonly_pick_indexed_no_error() {
    let source = r#"
function nestedPick<T, K extends keyof T>(obj: Required<T>, k: K): T[K] {
  const rp: Required<Readonly<Pick<T, K>>> = {} as Required<Readonly<Pick<T, K>>>;
  return rp[k];
}
"#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for Required<Readonly<Pick<T,K>>>[K], got: {diags:#?}"
    );
}
