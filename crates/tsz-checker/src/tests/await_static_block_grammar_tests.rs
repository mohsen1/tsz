//! Regression coverage for #16068 item 2: does a non-async `await` reached
//! through a *nested* statement inside a class static block report TS18037
//! (tsc's rule) or the ordinary TS1308 (`await` outside an async function)?
//!
//! `parse_await_expression` (`crates/tsz-parser/src/parser/state_expressions_unary.rs`)
//! already emits TS18037 for any `await` parsed while `in_static_block_context()`
//! is set, regardless of how deeply the `await` sits inside intervening
//! statements (`while`, `for`, `switch`, ...) — the context flag survives
//! statement nesting and is only cleared at a function/class boundary.
//!
//! #16068 reported the opposite (TS1308 instead of TS18037) — that read came
//! from `test_utils::check_source_codes`, which never wires real parser
//! diagnostics or `has_syntax_parse_errors` into the `CheckerState` it builds
//! (see `test_utils::check_source_with_parse_health`'s doc comment), so it
//! can neither see the parser's TS18037 nor let it suppress the checker's
//! TS1308. These tests use the parse-health-aware helper instead, so they
//! reflect what the compiled CLI actually reports.
//!
//! `static_block_own_node_no_double_report_when_not_globally_suppressed` and
//! `static_block_await_does_not_suppress_unrelated_ts1308_program_wide` below
//! instead build a `CheckerState` directly with `has_syntax_parse_errors`
//! computed the way `tsz-cli`'s `check_file.rs`/`check_utils.rs::is_non_suppressing_parse_error`
//! compute it in production (TS18037 excluded), rather than the
//! `!parse_diagnostics.is_empty()` coarse signal
//! `check_source_codes_with_parse_health` uses. That coarse signal makes
//! `has_syntax_parse_errors` true for *any* file containing an `await` inside
//! a static block, same as before #16360's fix — so it cannot exercise the
//! cross-file suppression bug #16360 reported (a static block's TS18037
//! deleting an unrelated function's TS1308 elsewhere in the same file) or its
//! fix (`is_non_suppressing_parse_error` no longer includes TS18037; the
//! static block's own node is instead skipped explicitly via
//! `find_enclosing_static_block` in `check_await_expression_in_container`,
//! matching tsc's `checkAwaitExpression`, which returns after its own
//! static-block diagnostic without falling through to the "only allowed
//! within async functions" check).

use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::check_source_codes_with_parse_health;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

/// Parse, bind, and check `source` with `has_syntax_parse_errors` computed
/// the way `tsz-cli` computes it in production: TS18037 (check-time grammar
/// in tsc, parser-emitted in tsz — see `check_utils.rs::is_non_suppressing_parse_error`)
/// does not count as a "real" syntax error. Returns `(parser codes, checker
/// codes)` like `test_utils::check_source_with_parse_health`.
fn check_with_production_suppression(source: &str) -> (Vec<u32>, Vec<u32>) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();
    let parse_diagnostics = parser.get_diagnostics().to_vec();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.set_lib_contexts(Vec::new());
    let real_syntax_errors: Vec<u32> = parse_diagnostics
        .iter()
        .filter(|d| d.code != 18037)
        .map(|d| d.start)
        .collect();
    checker.ctx.has_parse_errors = !real_syntax_errors.is_empty();
    checker.ctx.has_syntax_parse_errors = !real_syntax_errors.is_empty();
    checker.ctx.syntax_parse_error_positions = real_syntax_errors;
    checker.ctx.all_parse_error_positions =
        parse_diagnostics.iter().map(|diag| diag.start).collect();
    checker.check_source_file(source_file);

    let parse_codes = parse_diagnostics.iter().map(|diag| diag.code).collect();
    let checker_codes = checker
        .ctx
        .diagnostics
        .iter()
        .map(|diag| diag.code)
        .collect();
    (parse_codes, checker_codes)
}

/// #16360: a class static block's `await` must not delete an unrelated
/// function's TS1308 elsewhere in the same file. tsc reports all three
/// diagnostics (TS1308, TS18037, TS1308) for this file; before #16360's fix,
/// tsz reported only TS18037 — the static block's TS18037 parse diagnostic
/// set `has_syntax_parse_errors` file-wide, which gates `check_await_expression_in_container`'s
/// entire TS1308/TS1375/TS1378 walk.
#[test]
fn static_block_await_does_not_suppress_unrelated_ts1308_program_wide() {
    let (parse_codes, checker_codes) = check_with_production_suppression(
        r#"
function outer() { const g = function () { return await 1; }; return g; }
class C { static { const c = await 4; } }
function* gen() { const d = await 5; return d; }
"#,
    );
    assert!(
        parse_codes.contains(&18037),
        "got parse codes {parse_codes:?}"
    );
    assert_eq!(
        checker_codes.iter().filter(|&&c| c == 1308).count(),
        2,
        "tsc reports TS1308 for both `outer`'s and `gen`'s awaits — the static block's own TS18037 must not suppress either; got checker codes {checker_codes:?}"
    );
}

/// #16360's second defect: `is_directly_at_source_file_top_level` did not
/// stop its container walk at `CLASS_STATIC_BLOCK_DECLARATION`, so a static
/// block's `await` walked all the way up to `SourceFile` and was
/// misclassified as top-level `await` (TS1375/TS1378) instead of TS1308 —
/// latent and unobservable before #16360's fix because
/// `has_syntax_parse_errors` already suppressed this whole branch whenever a
/// static block's `await` was present. This is a pure checker-classification
/// bug: reproducible even with `has_syntax_parse_errors` correctly cleared.
#[test]
fn static_block_own_node_no_double_report_when_not_globally_suppressed() {
    let (parse_codes, checker_codes) = check_with_production_suppression(
        r#"
class C { static { await 1; } }
"#,
    );
    assert!(
        parse_codes.contains(&18037),
        "got parse codes {parse_codes:?}"
    );
    assert!(
        !checker_codes.contains(&1375) && !checker_codes.contains(&1378),
        "a static block's `await` is never at the source file's top level; got checker codes {checker_codes:?}"
    );
    assert!(
        !checker_codes.contains(&1308),
        "TS18037 (parser) already covers this `await`; a second TS1308 (checker) would be extra output tsc never produces; got checker codes {checker_codes:?}"
    );
}

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
/// eligibility check entirely) — which also exercises the
/// `is_directly_at_source_file_top_level` fix that added
/// `CLASS_STATIC_BLOCK_DECLARATION` to its disqualifying-container list.
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
