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
//! 4. **A `TypeLiteral`-specific async-inheritance defect**, found once (1)
//!    made the walk reachable for type literals too: unlike a class or
//!    interface member, a `TypeLiteral` member's computed name does NOT
//!    inherit the enclosing function's async-ness in `tsc` — verified
//!    against a live `tsc@7.0.2` oracle, `async function w() { type S = {
//!    [await k]: number } }` still reports TS1308, while the interface
//!    analog reports only TS1169. It still correctly answers the
//!    TS1375/TS1378 top-level pair when genuinely at the source file's top
//!    level, so this is neither the ordinary inheriting case nor an enum
//!    initializer's fully-own-container case — a third
//!    `AwaitContainerMode::TypeLiteralMember` in
//!    `core_statement_checks.rs::check_await_expression_in_container`,
//!    reached via `check_computed_property_name_type_literal_member`
//!    (`property_checker.rs`) from all three `type_alias_checking.rs`
//!    `TypeLiteral`-member call sites (property, method/call signature,
//!    accessor). Holds regardless of nesting depth — a `TypeLiteral` nested
//!    inside an `interface` member's type annotation behaves identically.
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
fn async_wrapper_type_literal_computed_name_reports_ts1170_and_ts1308() {
    // Unlike the class/interface siblings above, a `TypeLiteral` member's
    // computed name does NOT inherit the enclosing function's async-ness —
    // verified against a live `tsc@7.0.2` oracle, which reports both TS1170
    // and TS1308 here (the interface analog reports only TS1169: no TS1308).
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Shape2 = { [await key]: number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

#[test]
fn async_wrapper_type_literal_method_computed_name_reports_ts1170_and_ts1308() {
    // Adjacent member form: a call/method signature member, not a property.
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Shape2 = { [await key](): number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

#[test]
fn async_wrapper_type_literal_accessor_computed_name_reports_ts1308() {
    // Adjacent member form: an accessor. No TS1170 pairing here — a
    // `get`/`set` computed name isn't gated by the literal-form check the
    // way property/method members are (mirrors the pre-existing accessor
    // funnel, which never called `check_computed_property_requires_literal`).
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Shape2 = { get [await key](): number }; }
"#,
    ));
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

#[test]
fn async_wrapper_type_literal_nested_in_interface_property_reports_ts1170_and_ts1308() {
    // A `TypeLiteral` nested inside an `interface` member's type annotation
    // behaves identically to one nested inside a type alias's body — the
    // rule is keyed to the `TypeLiteral` node, not its enclosing declaration.
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { interface Shape { bar: { [await key]: number } } }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

#[test]
fn renamed_binder_async_wrapper_type_literal_computed_name_reports_ts1170_and_ts1308() {
    // Adjacent case: no identifier-spelling predicate drives the rule.
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const propertyToken: string;
async function makeContainer() { type Container = { [await propertyToken]: number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

#[test]
fn top_level_type_literal_computed_name_await_unaffected() {
    // Regression control: a `TypeLiteral` that really is at the source
    // file's top level still gets the TS1375/TS1378 top-level pair, not
    // TS1308 — only the async-inheritance half changed, not the top-level
    // walk. (No `--module`/`--target` flags in this harness, so only TS1375
    // fires; TS1378 is a separate module/target gate covered elsewhere.)
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
type Shape2 = { [await key]: number };
"#,
    );
    assert!(codes.contains(&1170), "got {codes:?}");
    assert!(codes.contains(&1375), "got {codes:?}");
    assert!(!codes.contains(&1308), "got {codes:?}");
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
