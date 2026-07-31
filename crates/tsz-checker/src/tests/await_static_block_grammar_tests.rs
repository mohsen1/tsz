//! Regression coverage for #16068 item 2: does a non-async `await` reached
//! through a *nested* statement inside a class static block report TS18037
//! (tsc's rule) or the ordinary TS1308 (`await` outside an async function)?
//!
//! `parse_await_expression` (`crates/tsz-parser/src/parser/state_expressions_unary.rs`)
//! already emits TS18037 for any `await` parsed while `in_static_block_context()`
//! is set, regardless of how deeply the `await` sits inside intervening
//! statements (`while`, `for`, `switch`, ...) — the context flag survives
//! statement nesting and is only cleared at a function/class boundary. That
//! parser diagnostic sets `has_syntax_parse_errors`, which suppresses
//! `check_await_expression`'s own TS1308 grammar walk
//! (`core_statement_checks.rs`) for the same file. So on current `main` this
//! family already matches tsc: exactly TS18037, never an accompanying TS1308.
//!
//! #16068 reported the opposite (TS1308 instead of TS18037) — that read came
//! from `test_utils::check_source_codes`, which never wires real parser
//! diagnostics or `has_syntax_parse_errors` into the `CheckerState` it builds
//! (see `test_utils::check_source_with_parse_health`'s doc comment), so it
//! can neither see the parser's TS18037 nor let it suppress the checker's
//! TS1308. These tests use the parse-health-aware helper instead, so they
//! reflect what the compiled CLI actually reports.

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
