//! Regression coverage for #16094: `await` in a computed member name
//! (`class`/`interface`/type-literal) is a fourth unrooted `await`-grammar
//! position, and rooting it naively exposed two independent defects.
//!
//! 1. **Async-context defect.** `state/state_checking/class.rs` resets
//!    `async_depth` to `0` before checking a class's members — correct for
//!    field initializers and static blocks, which really don't inherit
//!    `async` from the enclosing function, but wrong for a member's
//!    *computed name*, which `tsc` evaluates once, in the enclosing scope,
//!    when the class itself is defined. Fixed via
//!    `EnclosingClassInfo::enclosing_async_depth`, captured before the reset
//!    and swapped in for the duration of the computed-name check only
//!    (`types/type_checking/core.rs::check_class_member_name`).
//! 2. **A parser-level defect**, found while building this suite: the
//!    computed-name parser (`state_expressions_literals/object_members.rs::
//!    parse_property_name`) unconditionally flagged `await` in a class
//!    member's computed name as an illegal binding identifier (TS1213),
//!    pre-empting `parse_await_expression`'s own (pre-existing, and already
//!    tsc-correct) identifier-vs-`AwaitExpression` disambiguation before it
//!    ever ran. Oracle evidence (`tsc@7.0.2`) confirmed `tsc` never treats
//!    `await` this way here — not even a bare `[await]`, which just resolves
//!    as an ordinary (possibly-undefined) identifier reference (TS2304).
//!    Fixed via `is_computed_class_member_await_expression`, which
//!    unconditionally excludes `await` from the illegal-binding check in
//!    computed-name position and lets `parse_await_expression` decide, as it
//!    already correctly does for object-literal computed names.
//! 3. **A TS1170 routing gap** (Blocker B, already named in #16094): a
//!    type-literal member whose computed name failed the literal-type check
//!    (TS1170) never got the shared `check_computed_property_name` funnel at
//!    all, so `await` inside it (and separately, TS2464's property-key-type
//!    check — verified against `tsc@7.0.2`: `type U = { [b as unknown as
//!    boolean]: number }` reports both TS1170 and TS2464) was silently
//!    unrooted even after (1) and (2). Fixed in `type_alias_checking.rs` by
//!    always running the full funnel regardless of whether TS1170 fired —
//!    `tsc` reports both, they are independent grammar/type rules.
//!
//! Every expectation below is pinned against a live `tsc@7.0.2 --noEmit
//! --pretty false --target es2017 --module commonjs` run, not recalled, and
//! cross-checked against the compiled `tsz` CLI (not just this unit
//! harness) for every case that involves parser recovery or identifier
//! resolution, per the two harness gaps noted inline below.

use crate::test_utils::check_source_codes_with_parse_health;

/// The harness has no lib, so an `async function` always adds TS2318
/// ("Global type 'Promise' does not exist") independent of anything this
/// suite is testing (the compiled CLI, which does have a lib, does not
/// produce it — verified directly). Strip it so assertions read the
/// await-grammar codes only.
fn without_missing_promise_lib(mut codes: Vec<u32>) -> Vec<u32> {
    codes.retain(|&c| c != 2318);
    codes
}

fn sorted(mut codes: Vec<u32>) -> Vec<u32> {
    codes.sort_unstable();
    codes
}

#[test]
fn class_method_computed_name_await_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
function outer() { class Holder { [await key]() {} } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

#[test]
fn class_expression_method_computed_name_await_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
function outer() { const Holder = class { [await key]() {} }; }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

#[test]
fn class_property_computed_name_await_reports_ts1166_and_ts1308() {
    // tsc pairs TS1166 (class-property literal-name requirement) with
    // TS1308, exactly like the TS1169/TS1170 interface/type-literal siblings
    // below.
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
function outer() { class Holder { [await key] = 1; } }
"#,
    );
    assert_eq!(sorted(codes.clone()), vec![1166, 1308], "got {codes:?}");
}

#[test]
fn async_wrapper_class_method_computed_name_await_is_clean() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { class Bag { [await key]() {} } }
"#,
    ));
    assert!(
        codes.is_empty(),
        "the name is evaluated in wrapper's async scope, not the class body's own reset; got {codes:?}"
    );
}

#[test]
fn async_method_alongside_async_wrapper_computed_name_is_clean() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() {
  class Bag {
    [key]() {}
    async [await key]() {}
  }
}
"#,
    ));
    assert!(codes.is_empty(), "got {codes:?}");
}

#[test]
fn interface_computed_name_await_reports_ts1169_and_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
function outer() { interface Shape { [await key]: number } }
"#,
    );
    assert_eq!(sorted(codes.clone()), vec![1169, 1308], "got {codes:?}");
}

#[test]
fn async_wrapper_interface_computed_name_reports_only_ts1169() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { interface Shape { [await key]: number } }
"#,
    ));
    assert_eq!(
        codes,
        vec![1169],
        "the interface member name is evaluated in wrapper's async scope; got {codes:?}"
    );
}

#[test]
fn type_literal_computed_name_await_reports_ts1170_and_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
function outer() { type Shape2 = { [await key]: number }; }
"#,
    );
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

#[test]
fn async_wrapper_type_literal_computed_name_reports_only_ts1170() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Shape2 = { [await key]: number }; }
"#,
    ));
    assert_eq!(codes, vec![1170], "got {codes:?}");
}

#[test]
fn bare_await_class_computed_name_never_reports_ts1213() {
    // tsc: `class K { [await]() {} }` never reports the reserved-word
    // TS1213, resolved or not — verified on both an unresolved bare
    // `[await]` (tsc: TS2304, "Cannot find name") and one bound to an outer
    // const (tsc: clean). This harness's identifier resolution does not
    // reach a verdict on a bare `await` reference the way the compiled CLI
    // does (no TS2304 here either), so this test pins the invariant that
    // matters to #16094 — no false TS1213 — and the TS2304-vs-clean split is
    // separately confirmed against the compiled CLI + a live tsc oracle.
    let unresolved = check_source_codes_with_parse_health(
        r#"
class K { [await]() {} }
"#,
    );
    assert!(
        !unresolved.contains(&1213),
        "an unresolved bare `await` must not be the reserved-word TS1213; got {unresolved:?}"
    );

    let bound = check_source_codes_with_parse_health(
        r#"
declare const await: number;
class K { [await]() {} }
"#,
    );
    assert!(
        !bound.contains(&1213),
        "a bound `await` used as a bare computed name is an ordinary identifier reference; got {bound:?}"
    );
}

#[test]
fn plain_identifier_member_named_await_is_unaffected() {
    // Not a computed name at all — `await` used directly as a method name.
    // Must stay clean; this exercises a different parser path entirely.
    let codes = check_source_codes_with_parse_health(
        r#"
class K { await() {} }
"#,
    );
    assert!(codes.is_empty(), "got {codes:?}");
}

#[test]
fn generator_method_computed_yield_name_still_reports_ts1213() {
    // Negative control: `yield`'s own illegal-binding-identifier
    // disambiguation (a sibling mechanism to the one this PR adds for
    // `await`) must be unaffected by the new `await`-specific exclusion.
    let codes = check_source_codes_with_parse_health(
        r#"
class K { * [yield]() {} }
"#,
    );
    assert!(codes.contains(&1213), "got {codes:?}");
}

#[test]
fn class_method_computed_name_no_await_stays_clean() {
    // Negative control: the new root must not fire when there is no `await`
    // anywhere in the computed name.
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { [key]() {} }
"#,
    );
    assert!(codes.is_empty(), "got {codes:?}");
}

#[test]
fn renamed_binder_class_method_computed_name_await_reports_ts1308() {
    // Adjacent case: no identifier-spelling predicate drives the rule.
    let codes = check_source_codes_with_parse_health(
        r#"
declare const propertyToken: string;
function makeContainer() { class ConnectionPool { [await propertyToken]() {} } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

// --- #16100: the top-level axis of the same two-axis split ---
//
// `await` legality has two independent axes: "am I inside an `async`
// function?" and "am I at the top level of the source file?". #16099 (above)
// threaded the first through `EnclosingClassInfo::enclosing_async_depth` and
// left the second answering from the `await` node's own position, where the
// class member declaration disqualifies it. `tsc` resolves a class computed
// property name's container to the container of the *class*, skipping the
// member — so at the top level of a file the name is top level, and gets the
// TS1375/TS1378 top-level pair rather than TS1308.
//
// This harness has no lib and no module setting, so a genuinely top-level
// `await` reports both TS1375 (file is not a module) and TS1378 (module
// option does not support top-level await). Every case below is pinned
// against a live `tsc@7.0.2 --noEmit --strict --pretty false --target es2022`
// run: under `--module esnext` in a module file the four positive rows are
// CLEAN, and in a script file the same class-method row reports TS1375 —
// which is what identifies it as a top-level position at all.

/// The witness from #16100. Not TS1308: a class method's computed name at the
/// top level of the file inherits the file's own top-level `await` allowance.
#[test]
fn class_method_computed_name_at_file_top_level_answers_top_level_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { [await key]() {} }
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// Getter sibling — `tsc` clean under `--module esnext`.
#[test]
fn class_getter_computed_name_at_file_top_level_answers_top_level_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { get [await key]() { return 1; } }
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// Setter sibling.
#[test]
fn class_setter_computed_name_at_file_top_level_answers_top_level_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { set [await key](value: number) {} }
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// `static` does not change the container question.
#[test]
fn static_member_computed_name_at_file_top_level_answers_top_level_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { static [await key]() {} }
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// A class *expression* is class-like too, so the same jump applies.
#[test]
fn class_expression_computed_name_at_file_top_level_answers_top_level_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
const Holder = class { [await key]() {} };
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// Renamed-binder control (anti-hardcoding): nothing about this decision may
/// depend on the identifiers chosen.
#[test]
fn renamed_binders_computed_name_at_file_top_level_answers_top_level_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const connectionToken: string;
class ConnectionPool { [await connectionToken]() {} }
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// A property declaration's computed *name* takes the same jump, even though
/// `PROPERTY_DECLARATION` is a disqualifying container for the initializer
/// position. `tsc` reports only the TS1166 literal-form error here, no
/// TS1308 — the name-vs-initializer split is the point.
#[test]
fn property_declaration_computed_name_at_file_top_level_is_not_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { [await key] = 1; }
"#,
    );
    assert!(
        !codes.contains(&1308),
        "a computed property *name* at file top level is top level; got {codes:?}"
    );
}

/// The negative half of that split, and the reason the jump must be keyed on
/// the computed-name position rather than on the class member: a property
/// *initializer* is genuinely not top level and must keep reporting TS1308.
#[test]
fn property_initializer_await_at_file_top_level_still_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { value = await key; }
"#,
    );
    assert!(
        codes.contains(&1308),
        "a property initializer is not top level; got {codes:?}"
    );
    assert!(
        !codes.contains(&1375) && !codes.contains(&1378),
        "and must not answer the top-level pair; got {codes:?}"
    );
}

/// An object-literal computed name at file top level was already correct
/// (an object literal is an expression, so no container intervenes) and must
/// stay that way — the jump is class-only.
#[test]
fn object_literal_computed_name_at_file_top_level_is_unchanged() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
const holder = { [await key]: 1 };
"#,
    );
    assert_eq!(sorted(codes), vec![1375, 1378], "got top-level pair");
}

/// Negative control: a namespace body is not the file's top level, so the
/// jump lands on the class and the walk still finds the module block.
#[test]
fn class_computed_name_inside_namespace_still_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
namespace Registry { class Holder { [await key]() {} } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

/// Negative control: a class nested inside a method body is not top level
/// either — the jump lands on the inner class, and the walk then hits the
/// enclosing method.
#[test]
fn class_computed_name_nested_in_method_still_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Outer { build() { class Holder { [await key]() {} } } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

/// Negative control: an `await` inside an arrow *within* the computed name
/// has the arrow as its container, not the class. The walk's function-like
/// boundary still stops it.
#[test]
fn await_inside_arrow_within_computed_name_still_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
class Holder { [(() => await key)()]() {} }
"#,
    );
    assert!(
        codes.contains(&1308),
        "an arrow body is its own container; got {codes:?}"
    );
}
