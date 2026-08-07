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
//! 4. **A wrong async-context rule for type literals** (#16103, fixed after
//!    the above). (1) gave *every* computed-name position the "evaluated in
//!    the enclosing scope, so an enclosing `async` makes `await` legal" rule.
//!    That is right for class and interface members but wrong for a type
//!    literal: `tsc` gates TS1308 on the parser's `NodeFlags.AwaitContext`,
//!    and `parseType` runs under `doOutsideOfContext(TypeExcludesFlags)`,
//!    which clears that flag for the whole type. `parseInterfaceDeclaration`
//!    reaches its members through `parseObjectTypeMembers` instead, so an
//!    interface keeps the flag — which is why the two disagree. Fixed via
//!    `AwaitContainerKind::OutsideAwaitContext`, keyed structurally on the
//!    owning member's parent being a `TypeLiteral` so it holds for every way
//!    of reaching one (type-alias body, variable or parameter annotation,
//!    nested inside an `interface` member's type).
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
    // #16103. This assertion previously read `[1170]` — added in #16099 by
    // analogy with the class and interface siblings above, never checked
    // against a live oracle, and wrong. `tsc@7.0.2` reports **both** codes:
    // a type literal's members are parsed under `parseType`, which runs
    // inside `doOutsideOfContext(TypeExcludesFlags)` and clears
    // `NodeFlags.AwaitContext`, so the enclosing `async` never reaches the
    // computed name. The class and interface siblings really do inherit it —
    // that asymmetry is the whole rule, and it is pinned by the two control
    // tests directly above and below this one.
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Shape2 = { [await key]: number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
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

// --- #16104: `export` on a namespace class must not change `await` legality.
// ---
// `tsc@7.0.2` has a bug here: adding `export` to a class inside a namespace
// suppresses TS1308 on the class member's *computed name*. `export` cannot
// change whether `await` is legal at a position, so tsz is correct to keep
// reporting TS1308 — this must NOT be patched toward tsc's under-report. These
// rows pin that correct divergence so a future "tsz over-reports TS1308 in a
// namespace" row is recognized as tsc under-reporting, not re-derived as a tsz
// defect.
//
// The witness is n2 (below); the plain non-export baseline is n1
// (`class_computed_name_inside_namespace_still_reports_ts1308`, above). The
// n3/n4/n5 controls below each kill a specific benign explanation — and, read
// as tsz pins, each guards against a specific wrong shape a patch-toward-the-bug
// could take: keying the suppression on the namespace merely *having* an export
// (n3), on the exported body being skipped (n4), or on one-shot dedup (n5).

/// n2 (the witness): `export` on the class owning the computed name is exactly
/// what makes `tsc` drop TS1308. tsz keeps it — the namespace body is not the
/// top level of a module, so TS1308 is the correct answer.
#[test]
fn export_namespace_class_computed_name_await_still_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const x: string;
namespace N { export class C { [await x]() {} } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

/// n3: the namespace has an `export` (of a *different* member — the class `C`
/// itself is not exported), yet TS1308 still fires. This is the row n1 cannot
/// cover: it guards specifically against a patch that keys the suppression on
/// the namespace *containing an export* rather than on the owning class being
/// exported. `tsc` also reports TS1308 here (its bug needs the class itself
/// exported), which is what rules out "the namespace became instantiated".
#[test]
fn export_sibling_member_leaves_namespace_class_computed_name_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const x: string;
namespace N { export const q = 1; class C { [await x]() {} } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

/// n4: an `export class`'s property *initializer* is genuinely not top level
/// and still reports TS1308 in `tsc` too — rules out "the exported class body
/// is not checked". The `await` here is in an initializer, not a computed
/// name, so there is no TS1166.
#[test]
fn export_namespace_class_property_initializer_await_reports_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const x: string;
namespace N { export class C { p = await x; } }
"#,
    );
    assert_eq!(codes, vec![1308], "got {codes:?}");
}

/// n5: two exported classes each suppress a diagnostic in `tsc`, so its bug is
/// not a one-shot `error_at_node` dedup on a shared position. tsz reports both
/// — the correct answer, one TS1308 per computed name.
#[test]
fn two_export_namespace_classes_each_report_their_own_ts1308() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const x: string;
namespace N {
  export class A { [await x]() {} }
  export class B { [await x]() {} }
}
"#,
    );
    assert_eq!(codes, vec![1308, 1308], "got {codes:?}");
}

/// Binder-name-invariance control for the export rows (anti-hardcoding): the
/// rule is structural, not keyed on `N`/`C`/`x`.
#[test]
fn export_namespace_class_computed_name_await_is_binder_name_invariant() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const tokenValue: string;
namespace Registry { export class ConnectionPool { [await tokenValue]() {} } }
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

// --- #16103: a type literal is parsed outside await context, wherever it
// --- appears. Every expectation below is pinned against a live
// --- `tsc@7.0.2 --noEmit --pretty false --target es2017 --module commonjs`
// --- run made for this change, not recalled.

/// The rule is keyed on the *type literal*, not on the type alias that
/// happens to be the most common way to spell one. A literal reached through
/// an `interface` member's type annotation answers the same way — even
/// though the enclosing `interface` member's own computed name would not
/// (see `async_wrapper_interface_computed_name_reports_only_ts1169`).
#[test]
fn async_wrapper_type_literal_inside_interface_member_reports_ts1170_and_ts1308() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { interface Outer { inner: { [await key]: number } } }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

/// A type literal in a variable's type annotation — no type alias involved.
#[test]
fn async_wrapper_type_literal_in_variable_annotation_reports_ts1170_and_ts1308() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { let slot: { [await key]: number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

/// A type literal nested one level deeper inside a type alias body.
#[test]
fn async_wrapper_nested_type_literal_reports_ts1170_and_ts1308() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Outer = { inner: { [await key]: number } }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

/// A method signature is a separate arm of the type-literal member walk from
/// the property signature every other case here exercises.
#[test]
fn async_wrapper_type_literal_method_signature_reports_ts1170_and_ts1308() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { type Shape3 = { [await key](): number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

/// Renamed-binder control: nothing here may depend on the spelling of the
/// key, the alias, or the wrapper.
#[test]
fn async_wrapper_type_literal_computed_name_is_binder_name_independent() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const zzTag: string;
async function qqOuter() { type MmShape = { [await zzTag]: number }; }
"#,
    ));
    assert_eq!(sorted(codes.clone()), vec![1170, 1308], "got {codes:?}");
}

/// **The row that proves this is a routing fix and not a suppression.**
/// Clearing await context does not change `isInTopLevelContext`, so a type
/// literal at the source file's own top level must still answer the
/// TS1375/TS1378 top-level pair — never TS1308. Only a fix that actually
/// routes to the top-level branch can produce this; a fix that force-reports
/// TS1308 for type literals would fail exactly here.
#[test]
fn script_top_level_type_literal_computed_name_reports_top_level_await_pair() {
    let codes = check_source_codes_with_parse_health(
        r#"
declare const key: string;
type Shape4 = { [await key]: number };
"#,
    );
    assert_eq!(
        sorted(codes.clone()),
        vec![1170, 1375, 1378],
        "got {codes:?}"
    );
}

/// Negative control on the other side: an **object literal**'s computed name
/// is an ordinary value position and must keep inheriting the enclosing
/// `async`. tsc reports nothing here. This is the case a fix keyed on
/// "computed name in a braced member list" rather than on the type literal
/// would break.
#[test]
fn async_wrapper_object_literal_computed_name_stays_clean() {
    let codes = without_missing_promise_lib(check_source_codes_with_parse_health(
        r#"
declare const key: string;
async function wrapper() { const bag = { [await key]: 1 }; }
"#,
    ));
    assert!(
        codes.is_empty(),
        "an object literal is a value position and keeps the enclosing async context; got {codes:?}"
    );
}
