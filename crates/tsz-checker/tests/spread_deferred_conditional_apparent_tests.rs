//! A value whose type is a deferred (uninstantiated generic) conditional that is
//! spread as a rest argument (`f(...v)`, `arr.push(...v)`) must be iterated
//! through its *apparent* type — the union of its branch base-constraints —
//! before the spread element is related to the rest-parameter element. `tsc`
//! reduces the conditional to its apparent type; relating the whole conditional
//! produces a false `TS2345`. See issue #14946.
//!
//! These run against the real default lib bundle (target ES2020) so `Array`,
//! `Iterable`, and `Symbol.iterator` resolve exactly as in a real check. Binder
//! names are varied across tests to keep the rule structural, not name-driven.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn strict_codes(source: &str) -> Vec<u32> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    let libs = LIBS.get_or_init(load_default_lib_files);
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2020,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        libs,
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn push_spread_of_deferred_conditional_is_iterated_to_element() {
    // The exact witness from #14946 (binder names preserved from the report).
    let codes = strict_codes(
        r#"
type Cond<P> = P extends string ? [P] : (string | number)[];
function g<P>(p: P) {
  const result: (string | number)[] = [];
  const v: Cond<P> = null as any;
  result.push(...v);
}
"#,
    );
    assert!(
        !codes.contains(&2345),
        "spread of a deferred conditional must reduce to its apparent element type, got {codes:?}",
    );
}

#[test]
fn call_spread_of_deferred_conditional_uses_apparent_branch_union() {
    // Same rule through a plain rest-parameter call with renamed binders. The
    // true branch narrows the check parameter to `string`, so the apparent
    // element union stays assignable to the `string | number` rest element.
    let codes = strict_codes(
        r#"
type Pick2<Q> = Q extends string ? [Q, Q] : (string | number)[];
declare function sink(...entries: (string | number)[]): void;
function relay<Q>(seed: Q) {
  const payload: Pick2<Q> = null as any;
  sink(...payload);
}
"#,
    );
    assert!(
        !codes.contains(&2345),
        "deferred conditional spread into a rest call must not emit TS2345, got {codes:?}",
    );
}

#[test]
fn deferred_conditional_with_array_branches_both_sides_assignable() {
    // Both branches are concrete array-likes; the apparent element union must
    // stay assignable to the wider rest element. Renamed binders again.
    let codes = strict_codes(
        r#"
type Branchy<R> = R extends boolean ? boolean[] : number[];
function collect<R>(flag: R) {
  const bucket: (number | boolean)[] = [];
  const items: Branchy<R> = null as any;
  bucket.push(...items);
}
"#,
    );
    assert!(
        !codes.contains(&2345),
        "both array branches of a deferred conditional must iterate to assignable elements, got {codes:?}",
    );
}

#[test]
fn nongeneric_array_alias_spread_still_clean_control() {
    // Control: a non-conditional generic alias resolving to an array spreads
    // without any change in behavior.
    let codes = strict_codes(
        r#"
type Plain<S> = S[];
function feed<S extends string>(token: S) {
  const out: string[] = [];
  const xs: Plain<S> = null as any;
  out.push(...xs);
}
"#,
    );
    assert!(
        !codes.contains(&2345),
        "non-conditional array alias spread must remain clean, got {codes:?}",
    );
}

#[test]
fn deferred_conditional_element_mismatch_still_reported() {
    // Negative parity guard: when the apparent element type genuinely does not
    // fit the rest-parameter element, the call must still be rejected. The false
    // branch iterates to `boolean`, which is not assignable to a `number` rest
    // element, so TS2345 must remain.
    let codes = strict_codes(
        r#"
type Mixed<U> = U extends never ? number[] : boolean[];
declare function onlyNumbers(...values: number[]): void;
function dispatch<U>(key: U) {
  const data: Mixed<U> = null as any;
  onlyNumbers(...data);
}
"#,
    );
    assert!(
        codes.contains(&2345),
        "an apparent element that does not fit the rest element must still emit TS2345, got {codes:?}",
    );
}
