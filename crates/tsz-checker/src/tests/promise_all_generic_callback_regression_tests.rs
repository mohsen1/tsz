//! Regression tests for issue #6436:
//! "False positive TS2322: Promise.all + map with generic transform callback"
//!
//! Two sub-bugs surface together in the reported reproduction:
//!
//! 1. `Promise.all(items)` where `items: Promise<T>[]` returns an unreduced
//!    homomorphic mapped type whose template still wraps `Awaited<T[P]>`
//!    around an indexed access of the substituted argument. The same source
//!    type written inline (without a generic call) is recognised as `T[]`,
//!    but the call-result variant fails the assignability check.
//!
//! 2. Calling that function with a fresh array literal (`[1, 2, 3]`) fails
//!    to widen the inferred literal union before propagating it into a
//!    sibling callback's contextual return type, so `async (n) => n * 2` is
//!    checked against `Promise<1 | 2 | 3>` instead of `Promise<number>`.
//!
//! See the issue for the full substitution trace. Tests are `#[ignore]`'d
//! until the solver fix lands; removing the gate is the completion criterion.

use crate::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_common::diagnostics::data::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
use tsz_common::options::checker::CheckerOptions;

fn checker_options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES2022,
        ..strict_checker_options()
    }
}

fn assert_no_ts2322(source: &str) {
    let libs = load_default_lib_files();
    let messages =
        check_source_with_libs_code_messages(source, "test.ts", checker_options(), &libs);
    let offenders: Vec<&str> = messages
        .iter()
        .filter_map(|(code, msg)| (*code == TYPE_IS_NOT_ASSIGNABLE_TO_TYPE).then_some(msg.as_str()))
        .collect();
    assert!(
        offenders.is_empty(),
        "expected no TS2322 diagnostics; got: {offenders:?}"
    );
}

#[test]
#[ignore = "issue #6436: Promise.all return type retains unreduced Awaited application"]
fn promise_all_with_generic_transform_callback_typechecks() {
    assert_no_ts2322(
        r#"
async function processAsync<T>(
  items: T[],
  transform: (item: T) => Promise<T>
): Promise<T[]> {
  return Promise.all(items.map(transform));
}
"#,
    );
}

#[test]
#[ignore = "issue #6436: Promise.all return type retains unreduced Awaited application"]
fn promise_all_with_generic_transform_callback_typechecks_renamed() {
    assert_no_ts2322(
        r#"
async function processAsync<K>(
  items: K[],
  transform: (item: K) => Promise<K>
): Promise<K[]> {
  return Promise.all(items.map(transform));
}
"#,
    );
}

/// Synchronous slice — isolates the tuple-overload-return-type substitution
/// problem from the literal-widening problem.
#[test]
#[ignore = "issue #6436: Promise.all<Promise<T>[]> return type does not reduce"]
fn promise_all_of_promise_array_assigns_to_promise_array() {
    assert_no_ts2322(
        r#"
function p2<T>(items: Promise<T>[]): Promise<T[]> {
  return Promise.all(items);
}
"#,
    );
}

/// Generalises the rule: covers `{ [P in keyof X]: Awaited<X[P]> }` over any
/// inferred array, not just `Promise<T>[]` directly.
#[test]
#[ignore = "issue #6436: mapped-with-Awaited return after generic call does not reduce"]
fn generic_mapped_awaited_indexed_return_assigns_to_inner_array() {
    assert_no_ts2322(
        r#"
declare function combine<X extends readonly unknown[] | []>(
    values: X
): { -readonly [P in keyof X]: Awaited<X[P]> };

function call<U>(items: Promise<U>[]): U[] {
  return combine(items);
}
"#,
    );
}

/// Sub-issue 2: literal-array argument should widen before becoming the
/// contextual type for a sibling callback parameter.
#[test]
#[ignore = "issue #6436: literal widening across sibling generic argument is missing"]
fn fresh_array_literal_widens_for_sibling_callback_contextual_return() {
    assert_no_ts2322(
        r#"
async function processAsync<T>(
  items: T[],
  transform: (item: T) => Promise<T>
): Promise<T[]> {
  return Promise.all(items.map(transform));
}

const result = processAsync([1, 2, 3], async (n) => n * 2);
"#,
    );
}

#[test]
#[ignore = "issue #6436: literal widening across sibling generic argument is missing"]
fn fresh_array_literal_widens_for_sibling_callback_renamed() {
    assert_no_ts2322(
        r#"
async function each<V>(
  values: V[],
  step: (value: V) => Promise<V>
): Promise<V[]> {
  return Promise.all(values.map(step));
}

const out = each([10, 20, 30], async (v) => v + 1);
"#,
    );
}
