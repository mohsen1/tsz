//! Regression coverage for #16068 item 2: does a non-async `await` reached
//! through a *nested* statement inside a class static block report TS18037
//! (tsc's rule) or the ordinary TS1308 (`await` outside an async function)?
//!
//! `parse_await_expression` (`crates/tsz-parser/src/parser/state_expressions_unary.rs`)
//! already emits TS18037 for any `await` parsed while `in_static_block_context()`
//! is set, regardless of how deeply the `await` sits inside intervening
//! statements (`while`, `for`, `switch`, ...) — the context flag survives
//! statement nesting and is only cleared at a function/class boundary. So this
//! family matches tsc: exactly TS18037, never an accompanying TS1308.
//!
//! **The reason it matches changed in #16367, and the old reason was a bug.**
//! It used to be that the parser's TS18037 set `has_syntax_parse_errors`, which
//! suppressed `check_await_expression`'s TS1308 walk — for the *whole file*,
//! not just for the static block. tsc emits TS18037 from the checker, so its
//! `hasParseDiagnostics(sourceFile)` stays false and every other `await` in the
//! file still reports: `class C { static { await 4 } }` next to an ordinary
//! non-async `await` gives tsc two diagnostics and gave tsz one.
//!
//! The suppression is gone (TS18037 is now non-suppressing, like every other
//! parser-emitted checker-grammar code), and the exclusivity is stated where
//! tsc states it instead: `checkAwaitExpression` opens with an `if` on the
//! containing function-or-class-static-block being a class static block, and
//! the entire TS1308/TS1375/TS1378/TS1309 family lives in its `else if`.
//! `await_container_is_class_static_block` in `core_statement_checks.rs` is
//! that test. The assertions below are unchanged; what they prove is not.
//!
//! #16068 reported the opposite (TS1308 instead of TS18037) — that read came
//! from `test_utils::check_source_codes`, which never wires real parser
//! diagnostics or `has_syntax_parse_errors` into the `CheckerState` it builds
//! (see `test_utils::check_source_with_parse_health`'s doc comment), so it
//! can neither see the parser's TS18037 nor let it suppress the checker's
//! TS1308. The tests written for #16068 therefore use the parse-health-aware
//! helper, so they reflect what the compiled CLI actually reports. The three
//! added by #16367 at the end of this file deliberately use the blind helper
//! for the opposite reason — with the suppression removed, the blind helper is
//! what shows the checker walk deciding on its own.

use crate::test_utils::check_source_codes_with_parse_health;

/// #16068's literal repro: `await` in a static block reached through a
/// `while` condition, not the block's own top-level statement.
#[test]
fn static_block_while_condition_await_reports_only_ts18037() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { while (await 1) {} } }
"#,
    );
    assert!(
        codes.contains(&18037),
        "tsc reports TS18037 for `await` in a class static block; got {codes:?}"
    );
    assert!(
        !codes.contains(&1308),
        "TS18037 already covers this `await`; a second TS1308 would be extra output tsc never produces; got {codes:?}"
    );
}

/// The block's own direct top-level `await` — the shape #16068's table
/// measured with the (parse-health-blind) plain harness and read as TS1308.
#[test]
fn static_block_direct_expression_statement_await_reports_only_ts18037() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { await 1; } }
"#,
    );
    assert!(codes.contains(&18037), "got {codes:?}");
    assert!(!codes.contains(&1308), "got {codes:?}");
}

/// Adjacent case: `for` condition instead of `while`.
#[test]
fn static_block_for_condition_await_reports_only_ts18037() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { for (; await 1; ) {} } }
"#,
    );
    assert!(codes.contains(&18037), "got {codes:?}");
    assert!(!codes.contains(&1308), "got {codes:?}");
}

/// Adjacent case: `switch` discriminant.
#[test]
fn static_block_switch_discriminant_await_reports_only_ts18037() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { switch (await 1) { default: break; } } }
"#,
    );
    assert!(codes.contains(&18037), "got {codes:?}");
    assert!(!codes.contains(&1308), "got {codes:?}");
}

/// Adjacent case: `if` condition.
#[test]
fn static_block_if_condition_await_reports_only_ts18037() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { if (await 1) {} } }
"#,
    );
    assert!(codes.contains(&18037), "got {codes:?}");
    assert!(!codes.contains(&1308), "got {codes:?}");
}

/// Renamed-binder control: different class/member names, same shape.
#[test]
fn static_block_while_condition_await_renamed_binders() {
    let codes = check_source_codes_with_parse_health(
        r#"
class ConnectionPool { static { while (await 2) {} } }
"#,
    );
    assert!(codes.contains(&18037), "got {codes:?}");
    assert!(!codes.contains(&1308), "got {codes:?}");
}

/// Positive/fallback control: an `async` static block's `await` is legal
/// (tsc: `static { }` blocks cannot themselves be `async`, but a nested
/// async function's own `await` is fine and must stay clean).
#[test]
fn static_block_nested_async_function_await_is_clean() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate {
    static {
        (async () => {
            while (await Promise.resolve(true)) {}
        })();
    }
}
"#,
    );
    assert!(
        !codes.contains(&18037) && !codes.contains(&1308),
        "an async function nested inside a static block licenses its own `await`; got {codes:?}"
    );
}

/// Negative control: no `await` anywhere, must stay clean.
#[test]
fn static_block_without_await_reports_nothing() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { let x = 1; while (x) { x = 0; } } }
"#,
    );
    assert!(
        !codes.contains(&18037) && !codes.contains(&1308),
        "no `await` present; got {codes:?}"
    );
}

/// TS18054's sibling family: `await using` (rather than a bare `await`
/// expression) parsed directly inside a class static block.
/// `parse_variable_declaration_list` (`state_statements.rs`) now emits
/// TS18054 for the plain-statement shape, mirroring TS18037's
/// `in_static_block_context()` gate. tsc never pairs this with TS2853
/// ("...only allowed at the top level of a file...") — the two are mutually
/// exclusive by container (`getContainingFunctionOrClassStaticBlock`
/// resolving to the static block short-circuits tsc's top-level-await-using
/// eligibility check entirely). `check_variable_declaration_list_with_request`
/// declines the whole TS2852/TS2853/TS2854 family behind
/// `await_container_is_class_static_block`, the same container gate the
/// bare-`await` family uses.
///
/// This helper is parse-health-*aware*, so TS18054 sets `has_syntax_parse_errors`
/// and the await-using walk is suppressed regardless of the container gate; the
/// `..._without_suppression` test below is the one that actually exercises the
/// gate (see #16598).
#[test]
fn static_block_direct_await_using_reports_only_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { await using x = 1; } }
"#,
    );
    assert!(codes.contains(&18054), "got {codes:?}");
    assert!(
        !codes.contains(&2853),
        "TS18054 and TS2853 are mutually exclusive in tsc — a static block is never eligible for the top-level-await-using family; got {codes:?}"
    );
}

/// Adjacent case: `for (await using x of ...)` — a for-of head is parsed by
/// an entirely separate declaration-list constructor
/// (`parse_for_variable_declaration` in `state_declarations_exports.rs`),
/// so this exercises `report_await_using_static_block_for_initializer`
/// rather than the plain-statement call site.
#[test]
fn static_block_for_of_await_using_reports_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { for (await using x of []) { } } }
"#,
    );
    assert!(codes.contains(&18054), "got {codes:?}");
}

/// Adjacent case: a C-style `for (await using x = 1; ; )` head — same
/// initializer constructor as the for-of case, different loop shape.
#[test]
fn static_block_c_style_for_await_using_reports_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { for (await using x = 1; ; ) { break; } } }
"#,
    );
    assert!(codes.contains(&18054), "got {codes:?}");
}

/// Adjacent negative case: a `for...in` head reports only TS1494 (the
/// left-hand-side-cannot-be-`await using` family) — tsc's
/// `checkGrammarVariableDeclarationList` returns at that for-in-specific
/// diagnostic before ever reaching the await-grammar check, so TS18054
/// must not also fire. `report_await_using_static_block_for_initializer`
/// is explicitly skipped when the next token is `in`.
#[test]
fn static_block_for_in_await_using_reports_only_ts1494_not_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { for (await using x in {}) { } } }
"#,
    );
    assert!(codes.contains(&1494), "got {codes:?}");
    assert!(
        !codes.contains(&18054),
        "for-in already answers TS1494 exclusively; a TS18054 alongside it would be extra output tsc never produces; got {codes:?}"
    );
}

/// Positive/fallback control: an `await using` inside a nested async arrow
/// (itself inside the static block) is licensed by its own container, not
/// the static block — same shape as TS18037's
/// `static_block_nested_async_function_await_is_clean` control.
#[test]
fn static_block_nested_async_function_await_using_is_clean_of_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate {
    static {
        (async () => {
            await using y = 1;
        })();
    }
}
"#,
    );
    assert!(
        !codes.contains(&18054),
        "an async function nested inside a static block licenses its own `await using`; got {codes:?}"
    );
}

/// Module-ness does not matter: a static block disallows `await using`
/// regardless of whether the file already has `export {}` (which would
/// otherwise exempt a top-level `await using` from TS2853).
#[test]
fn static_block_await_using_in_module_still_reports_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
export {};
class Gate { static { await using x = 1; } }
"#,
    );
    assert!(codes.contains(&18054), "got {codes:?}");
    assert!(!codes.contains(&2853), "got {codes:?}");
}

/// Renamed-binder control: different class/member/binding names, same shape.
#[test]
fn static_block_direct_await_using_renamed_binders() {
    let codes = check_source_codes_with_parse_health(
        r#"
class ConnectionPool { static { await using resource = 1; } }
"#,
    );
    assert!(codes.contains(&18054), "got {codes:?}");
}

/// Negative control: a plain (non-`await`) `using` declaration is legal
/// inside a static block — only the `await` half is disallowed, since a
/// static block can never itself be async.
#[test]
fn static_block_plain_using_does_not_report_ts18054() {
    let codes = check_source_codes_with_parse_health(
        r#"
class Gate { static { using x = 1; } }
"#,
    );
    assert!(
        !codes.contains(&18054),
        "a plain `using` (not `await using`) is unaffected by the static-block await restriction; got {codes:?}"
    );
}

/// The exclusivity, isolated from parse-health suppression entirely (#16367).
///
/// `check_source_codes` is the parse-health-*blind* helper: it never wires the
/// parser's diagnostics or `has_syntax_parse_errors` into the `CheckerState`
/// (see `test_utils::check_source_with_parse_health`'s doc comment). That makes
/// it exactly the right instrument here — it shows what the checker's
/// await-grammar walk decides on its own, with nothing suppressing it.
///
/// Before #16367 this reported TS1308 (or the TS1375/TS1378 top-level pair,
/// depending on module settings) and only the CLI's file-wide suppression hid
/// it. Now `await_container_is_class_static_block` declines at the source, so
/// the walk is silent for a static block's own `await` whether or not anything
/// is suppressing.
#[test]
fn static_block_await_is_silent_in_the_checker_walk_without_suppression() {
    let codes = crate::test_utils::check_source_codes(
        r#"
class Gate { static { await 1; } }
"#,
    );
    assert!(
        !codes.contains(&1308) && !codes.contains(&1375) && !codes.contains(&1378),
        "tsc's checkAwaitExpression puts the whole TS1308/TS1375/TS1378 family in \
         the `else if` of its class-static-block test, so the walk must decline \
         here on its own rather than relying on a file-wide suppression; got {codes:?}"
    );
}

/// The same walk must still speak for an `await` that is *not* in a static
/// block, in a file that also has one.
///
/// This is the shape the old suppression got wrong: it keyed on the file, so
/// one static block silenced every other `await` in it. Renamed binders
/// throughout, and the sibling `await` sits in a nested function expression so
/// it cannot be confused with the static block's container.
#[test]
fn sibling_await_outside_a_static_block_still_reports_in_the_checker_walk() {
    let codes = crate::test_utils::check_source_codes(
        r#"
class Latch { static { const seeded = await 4; } }
function makeReader() { const read = function () { return await 1; }; return read; }
"#,
    );
    assert!(
        codes.contains(&1308),
        "the non-async `await` in `makeReader`'s inner function expression is \
         unaffected by the sibling static block; tsc reports TS1308 for it \
         alongside the static block's TS18037; got {codes:?}"
    );
}

/// Container-walk boundary: an `await` inside a function *nested* in a static
/// block answers from its own function, not from the static block.
///
/// `await_container_is_class_static_block` stops at the first
/// function-like-or-static-block ancestor rather than searching upward for a
/// static block anywhere, which is what `getContainingFunctionOrClassStaticBlock`
/// does. Without that stop, this `await` would be silently swallowed.
#[test]
fn await_in_a_non_async_function_nested_in_a_static_block_still_reports() {
    let codes = crate::test_utils::check_source_codes(
        r#"
class Harness { static { const build = function () { return await 2; }; void build; } }
"#,
    );
    assert!(
        codes.contains(&1308),
        "the nested function expression is its own container, so its non-async \
         `await` answers TS1308 rather than deferring to the enclosing static \
         block; got {codes:?}"
    );
}

/// #16598: the `await using` sibling of
/// `static_block_await_is_silent_in_the_checker_walk_without_suppression`.
///
/// `check_source_codes` is the parse-health-*blind* helper: it leaves
/// `has_syntax_parse_errors` false, exactly as the compiled CLI does once
/// TS18054 became a non-suppressing parser-grammar code (#16597). With the
/// suppression gone, the checker's await-using grammar walk runs on its own —
/// and before #16598 it fired TS2853 (or the nested TS2852, depending on
/// module settings) for `await using` inside a class static block, because
/// `is_directly_at_source_file_top_level` climbs past the static block to the
/// source file. `await_container_is_class_static_block` now declines the whole
/// family at the source, matching tsc's container short-circuit. This test is
/// non-vacuous: reverting the gate makes it fail with a TS2853.
#[test]
fn static_block_await_using_is_silent_in_the_checker_walk_without_suppression() {
    let codes = crate::test_utils::check_source_codes(
        r#"
class Gate { static { await using x = 1; } }
"#,
    );
    assert!(
        !codes.contains(&2853) && !codes.contains(&2852) && !codes.contains(&2854),
        "tsc's checkGrammarAwaitOrAwaitUsing puts the whole TS2852/TS2853/TS2854 \
         family in the `else` of its class-static-block test, so the walk must \
         decline here on its own rather than relying on a parse-error \
         suppression; got {codes:?}"
    );
}

/// The same walk must still speak for an `await using` that is *not* in a
/// static block, in a file that also has one — the container gate keys on the
/// declaration's own container, never on the file. Here the sibling
/// `await using` sits at the source-file top level of a non-module file, so it
/// still answers TS2853; renamed binders throughout.
#[test]
fn sibling_await_using_outside_a_static_block_still_reports_in_the_checker_walk() {
    let codes = crate::test_utils::check_source_codes(
        r#"
class Latch { static { await using seeded = 1; } }
await using loose = 2;
"#,
    );
    assert!(
        codes.contains(&2853),
        "the top-level `await using` in a non-module file is unaffected by the \
         sibling static block; tsc reports TS2853 for it alongside the static \
         block's TS18054; got {codes:?}"
    );
}

/// Container-walk boundary: an `await using` inside a non-async function
/// *nested* in a static block answers from its own function (TS2852, nested
/// outside async), not from the static block. Without stopping the walk at the
/// first function-like ancestor, this would be silently swallowed.
#[test]
fn await_using_in_a_non_async_function_nested_in_a_static_block_still_reports() {
    let codes = crate::test_utils::check_source_codes(
        r#"
class Harness { static { function build() { await using r = 1; void r; } void build; } }
"#,
    );
    assert!(
        codes.contains(&2852),
        "the nested function is its own container, so its `await using` answers \
         TS2852 rather than deferring to the enclosing static block; got {codes:?}"
    );
}
