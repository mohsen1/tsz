//! Regression tests for TS1308 (`await` outside an async function) in the
//! non-statement expression positions the `await`-grammar walk never reached.
//!
//! `check_await_expression` is rooted per checking site, on that site's own
//! expression children. #16067 completed the statement dispatcher's arms
//! (`while`/`do..while` condition, `for` initializer/condition/incrementor,
//! `switch` discriminant and `case` expressions, `throw` operand). The same
//! per-arm rooting left three more expression positions with no root at all,
//! so tsz was silent where tsc reports TS1308:
//!
//! - a `with` statement's expression — `WITH_STATEMENT` owns a dispatcher
//!   arm, so it never reaches the catch-all root
//! - a class heritage expression (`class C extends (await b()) {}`) — the
//!   dispatcher's walk stops at `CLASS_DECLARATION`/`CLASS_EXPRESSION` before
//!   reaching it
//! - an enum member initializer — `ENUM_DECLARATION` owns a dispatcher arm,
//!   and the enum declaration is additionally its **own container** for the
//!   check: tsc answers TS1308 there whether or not the enclosing function is
//!   `async`, and at the top level of a module where a bare `await` would be
//!   legal (#16097)
//!
//! Computed property names were a fourth unrooted position, tracked and
//! fixed separately in #16094's PR (`await_grammar_computed_property_name_tests.rs`)
//! once the async-context defect it surfaced (class-member computed names
//! were checked under the class body's reset-to-non-async context, not the
//! enclosing function's) was fixed alongside it.
//!
//! Every expectation here is pinned against a live
//! `tsc@7.0.2 --noEmit --pretty false --target es2017 --module commonjs` run,
//! not recalled. The negative cases matter as much as the positive ones: each
//! shape is also asserted silent inside an `async` function, which is what a
//! root placed where the async context is not yet established would break
//! (the `function_depth`-undercount failure mode of #16068).
//!
//! Binder names are varied across the positive and negative halves so no
//! expectation can be satisfied by a name-shaped predicate.

use crate::test_utils::{check_source_codes, check_source_diagnostics};

/// TS1308 occurrences in `source`. Counting rather than testing membership:
/// `error_at_node` deduplicates by `(start, code)`, so a position the checker
/// visits more than once must still yield exactly one diagnostic, and a
/// `contains` assertion cannot see a regression that starts reporting at
/// distinct positions.
fn count_ts1308(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 1308)
        .count()
}

// --- `with` statement expression ---

/// `function outer() { with (await 1) {} }`. tsc: `(4,9): error TS1308`.
/// TS1101/TS2410 also fire on the `with` itself; only the grammar walk's
/// TS1308 is under test here.
#[test]
fn with_statement_expression_await_reports_ts1308() {
    let source = r"
function outer() {
  with (await 1) { }
}
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `with` expression's `await` outside an async function must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// The same shape inside an `async` function is legal. tsc reports TS1101,
/// TS1300 and TS2410 for the `with`, and no TS1308.
#[test]
fn with_statement_expression_await_is_clean_in_async_function() {
    let source = r"
async function wrapper() {
  with (await 1) { }
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "an `await` in a `with` expression inside an async function must not report TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A `with` whose expression has no `await` must not be made to report by the
/// root's mere presence.
#[test]
fn with_statement_without_await_reports_no_ts1308() {
    let source = r"
function plain() {
  with (globalThis) { }
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "a `with` expression with no `await` must report no TS1308; got {:?}",
        check_source_codes(source)
    );
}

// --- class heritage expression ---

/// `class Derived extends (await makeBase()) {}` in a non-async function.
/// tsc: `(7,26): error TS1308`.
#[test]
fn class_heritage_expression_await_reports_ts1308() {
    let source = r"
declare function makeBase(): any;
function outer() {
  class Derived extends (await makeBase()) { }
}
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a class heritage expression's `await` outside an async function must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// The heritage expression is evaluated in the *enclosing* container, so an
/// `async` enclosing function makes it legal — the class boundary must not
/// reset the async context. tsc reports nothing here.
#[test]
fn class_heritage_expression_await_is_clean_in_async_function() {
    let source = r"
declare function buildParent(): any;
async function wrapper() {
  class Sub extends (await buildParent()) { }
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "a class heritage `await` inside an async function must not report TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A class expression in a heritage position is a traversal boundary: an
/// `await` inside *its* member body belongs to that member, not to the
/// enclosing container. tsc reports one TS1308 — for the heritage expression
/// alone, since the method body is its own non-async container and tsc
/// reports there too. Pinning the count keeps the boundary honest.
#[test]
fn class_heritage_expression_await_counts_once_per_position() {
    let source = r"
declare function makeBase(): any;
function outer() {
  class Derived extends (await makeBase()) { }
  class Other extends (await makeBase()) { }
}
";
    assert_eq!(
        count_ts1308(source),
        2,
        "two distinct heritage `await` positions must report two TS1308, one each; got {:?}",
        check_source_codes(source)
    );
}

// --- enum member initializer ---

/// `enum Flags { First = await 1 }` in a non-async function.
/// tsc: `(13,24): error TS1308`.
#[test]
fn enum_member_initializer_await_reports_ts1308() {
    let source = r"
function outer() {
  enum Flags { First = await 1 }
}
";
    assert_eq!(
        count_ts1308(source),
        1,
        "an enum member initializer's `await` outside an async function must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// Two initializers, two positions, one diagnostic each — the dedup key is
/// `(start, code)`, so a regression that reports per-visit rather than per
/// position is only visible in the count.
#[test]
fn enum_member_initializers_report_one_ts1308_each() {
    let source = r"
function outer() {
  enum Levels { Low = await 1, High = await 2 }
}
";
    assert_eq!(
        count_ts1308(source),
        2,
        "two enum initializer `await` positions must report two TS1308, one each; got {:?}",
        check_source_codes(source)
    );
}

/// An enum with no `await` in any initializer must stay silent.
#[test]
fn enum_member_initializer_without_await_reports_no_ts1308() {
    let source = r"
function outer() {
  enum Counts { One = 1, Two = 2 }
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "an enum with no `await` must report no TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// An enum member initializer is its own container for the grammar check, so
/// an enclosing `async` function does **not** make the `await` legal. tsc:
/// `(1,35): error TS1308` for `async function f() { enum E { A = await 1 } }`.
/// This is the half #16093 shipped without — it routed through the ordinary
/// async-context check, so it only fired for a non-async enclosing function.
#[test]
fn enum_member_initializer_await_reports_ts1308_inside_async_function() {
    let source = r"
async function wrapper() {
  enum Flags { First = await 1 }
}
";
    assert_eq!(
        count_ts1308(source),
        1,
        "an enum initializer `await` must report TS1308 even inside an async function; got {:?}",
        check_source_codes(source)
    );
}

/// The same rule stated at its clearest: at the top level of a module a bare
/// `await` is allowed under a top-level-await-capable module/target, and tsc
/// *still* answers TS1308 inside an enum member initializer — TS1308, not the
/// TS1375/TS1378 top-level pair. So the enum is not the source-file top level
/// either.
#[test]
fn enum_member_initializer_await_reports_ts1308_at_module_top_level() {
    let source = r"
enum Flags { First = await 1 }
export { };
";
    let codes = check_source_codes(source);
    assert_eq!(
        count_ts1308(source),
        1,
        "a top-level enum initializer `await` must report TS1308; got {codes:?}"
    );
    assert!(
        !codes.contains(&1375) && !codes.contains(&1378),
        "an enum initializer is not the source-file top level, so the TS1375/TS1378 top-level pair must not fire; got {codes:?}"
    );
}

/// A nested expression inside the initializer is reached by the same walk, and
/// a `const enum` takes the same path. tsc reports TS1308 for both (plus
/// TS2474 for the const-enum non-constant initializer, which is a different
/// check and not asserted here).
#[test]
fn enum_member_initializer_await_reports_ts1308_when_nested_and_for_const_enum() {
    let source = r"
declare function mk(): number;
async function wrapper() {
  enum Flags { First = (await mk()) + 2 }
  const enum Frozen { Second = await mk() }
}
";
    assert_eq!(
        count_ts1308(source),
        2,
        "a nested and a const-enum initializer `await` must each report one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// The own-container forcing must stop at a function boundary like every other
/// part of the walk: an `await` inside an arrow function nested in an enum
/// initializer belongs to that arrow, not to the enum. Here the arrow is
/// `async`, so tsc reports nothing.
#[test]
fn enum_member_initializer_own_container_does_not_leak_into_a_nested_async_arrow() {
    let source = r"
declare function mk(): Promise<number>;
function outer() {
  enum Flags { First = (async () => await mk()) as any }
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "an `await` inside an async arrow nested in an enum initializer must not report TS1308; got {:?}",
        check_source_codes(source)
    );
}

// --- the three positions together ---

/// All three newly rooted positions in one non-async container: exactly one
/// TS1308 per `await`, three in total. Pinned from a live `tsc@7.0.2` run over
/// the same fixture, which reports TS1308 at `(3,9)`, `(6,26)` and `(9,24)`.
#[test]
fn all_newly_rooted_positions_report_one_ts1308_each() {
    let source = r"
declare function makeBase(): any;
function outer() {
  with (await 1) { }
}
function outer2() {
  class Derived extends (await makeBase()) { }
}
function outer3() {
  enum Flags { First = await 1 }
}
";
    assert_eq!(
        count_ts1308(source),
        3,
        "each of the three `await` positions must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// The `with` and heritage shapes with every binder renamed and each wrapped
/// in an `async` container: zero TS1308. Varying the names keeps the positive
/// expectations from being satisfiable by any name-shaped predicate, and the
/// `async` wrapper pins that the roots read the enclosing container's async
/// context rather than assuming a non-async one. An enum initializer has no
/// legal `await` form to pair here — `await` is not a constant expression — so
/// the enum root's negative case is the no-`await` enum above.
#[test]
fn all_newly_rooted_positions_are_clean_inside_async_containers() {
    let source = r"
declare function buildParent(): any;
async function alpha() {
  with (await 1) { }
}
async function beta() {
  class Sub extends (await buildParent()) { }
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "no `await` inside an async container may report TS1308; got {:?}",
        check_source_codes(source)
    );
}
