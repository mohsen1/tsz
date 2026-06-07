//! Regression: a local declared inside a closure that is passed to a generic
//! call must widen its initializer literals by the declaration's own rules,
//! not inherit the generic call's `preserve_literal_types` flag. See #10663
//! (kysely false positives: `state.isCommitted = true` →
//! "'true' is not assignable to 'false'").
use tsz_checker::context::CheckerOptions;

fn codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn boolean_local_in_generic_callback_widens_and_is_reassignable() {
    let codes = codes(
        r#"
declare function provide<T>(cb: (c: number) => Promise<T>): Promise<T>;
declare function work(): Promise<void>;
async function run() {
  return provide(async (connection) => {
    const state = { isCommitted: false, isRolledBack: false };
    await work();
    state.isCommitted = true;
    state.isRolledBack = true;
  });
}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no TS2322 expected; widened booleans are reassignable, got {codes:?}"
    );
}

#[test]
fn number_and_string_locals_in_generic_callback_widen() {
    let codes = codes(
        r#"
declare function pg<T>(cb: () => T): T;
function f() {
  return pg(() => {
    const s = { a: 0 };
    s.a = 5;
    const obj = { k: "lit" };
    obj.k = "other";
    return 1;
  });
}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "no TS2322 expected; number/string props widen, got {codes:?}"
    );
}

#[test]
fn sync_generic_callback_local_object_mutation_ok() {
    let codes = codes(
        r#"
declare function applyGen<T>(cb: (c: number) => T): T;
function e() {
  return applyGen((connection) => {
    const state = { isCommitted: false };
    state.isCommitted = true;
    return 1;
  });
}
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got {codes:?}");
}

#[test]
fn const_assertion_inside_generic_callback_still_preserved() {
    // Guard: clearing the flag for body locals must NOT defeat `as const`.
    // `d.x` is `readonly 1`; mutating it is TS2540, and the literal stays `1`.
    let codes = codes(
        r#"
declare function pg<T>(cb: () => T): T;
function f() {
  return pg(() => {
    const d = { x: 1 } as const;
    d.x = 2;
    return d;
  });
}
"#,
    );
    assert!(
        codes.contains(&2540),
        "as const readonly must still be enforced (TS2540), got {codes:?}"
    );
}
