//! Regression coverage for #16072's "wrong branch" half: two constructs
//! disqualify a node from being "directly at the top level of the source
//! file" for the `await` / `await using` grammar question, without being
//! function-like and without affecting `function_depth`'s own
//! `break`/`continue` jump-boundary question (#16070) — a class property
//! initializer and a namespace body.
//!
//! Before this fix `check_await_expression`
//! (`crates/tsz-checker/src/types/type_checking/core_statement_checks.rs`)
//! and the `await using` check
//! (`crates/tsz-checker/src/types/type_checking/core.rs`) both used
//! `function_depth == 0` as "am I at the top level of the file", which is
//! only true incidentally: these two containers never bump
//! `function_depth`, so a non-async `await` inside them wrongly took the
//! top-level branch (TS1375/TS1378, or TS2853/TS2854 for `await using`)
//! instead of tsc's TS1308 (TS2852 for `await using`).
//!
//! #16072's other two rows — `await` in a method/constructor parameter
//! default — are a *different*, deeper bug (not covered here): tsz's
//! `parse_await_expression` (`crates/tsz-parser/src/parser/state_expressions_unary.rs:190-192`)
//! deliberately excludes `in_parameter_default_context()` from
//! `AwaitExpression` construction outside an async context, so no AST node
//! exists for this checker-level walk to visit — `await` parses as a bare
//! `Identifier` there instead, and the observed TS2524 comes from a wholly
//! separate special-case in identifier resolution
//! (`crates/tsz-checker/src/types/computation/identifier/resolution.rs:404-413`),
//! not from a grammar walk. That is parser-owned, filed separately as
//! #16078.
//!
//! Every expectation here is pinned against a live `tsc@7.0.2 --noEmit
//! --strict --pretty false --target es2022 --module esnext` run, not
//! recalled.

use crate::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

// --- class property initializers ---

#[test]
fn instance_property_initializer_await_reports_ts1308_not_ts1375() {
    let out = codes("class K { p = await 1; }");
    assert!(out.contains(&1308), "got {out:?}");
    assert!(!out.contains(&1375), "got {out:?}");
    assert!(!out.contains(&1378), "got {out:?}");
}

#[test]
fn static_property_initializer_await_reports_ts1308_not_ts1375() {
    let out = codes("class K { static p = await 1; }");
    assert!(out.contains(&1308), "got {out:?}");
    assert!(!out.contains(&1375), "got {out:?}");
    assert!(!out.contains(&1378), "got {out:?}");
}

/// Renamed-binder control: different class/property names, different
/// awaited literal.
#[test]
fn instance_property_initializer_await_reports_ts1308_renamed_binders() {
    let out = codes("class Gate { ready = await 2; }");
    assert!(out.contains(&1308), "got {out:?}");
    assert!(!out.contains(&1375), "got {out:?}");
}

// --- namespace bodies ---

#[test]
fn namespace_body_await_reports_ts1308_not_ts1375() {
    let out = codes("namespace N { await 1; }");
    assert!(out.contains(&1308), "got {out:?}");
    assert!(!out.contains(&1375), "got {out:?}");
    assert!(!out.contains(&1378), "got {out:?}");
}

/// Renamed-binder control for the namespace case.
#[test]
fn namespace_body_await_reports_ts1308_renamed_binders() {
    let out = codes("namespace Config { await 1; }");
    assert!(out.contains(&1308), "got {out:?}");
    assert!(!out.contains(&1375), "got {out:?}");
}

/// `await using` inside a namespace body: tsc reports TS2852 (the
/// non-top-level form), not TS2853/TS2854 (the top-level-only forms).
#[test]
fn namespace_body_await_using_reports_ts2852_not_ts2853() {
    let out = codes(
        r"
namespace N { await using x = getResource(); }
function getResource(): any { return {}; }
",
    );
    assert!(out.contains(&2852), "got {out:?}");
    assert!(!out.contains(&2853), "got {out:?}");
    assert!(!out.contains(&2854), "got {out:?}");
}

// --- negative controls: unaffected constructs stay unaffected ---

/// A genuine top-level `await` outside a module still reports TS1375 — the
/// new walk-based query must still say "yes, this is at the top level of
/// the file" for the file's own direct statements.
#[test]
fn genuine_top_level_await_still_reports_ts1375() {
    let out = codes("await 1;");
    assert!(out.contains(&1375), "got {out:?}");
    assert!(!out.contains(&1308), "got {out:?}");
}

/// `namespace N { break; }` is still TS1105 (a namespace body IS a
/// `break`/`continue` top-level boundary) — the jump-boundary question owned
/// by `function_depth` must stay untouched by this fix.
#[test]
fn namespace_body_break_still_reports_ts1105() {
    let out = codes("namespace N { break; }");
    assert!(out.contains(&1105), "got {out:?}");
}

/// A non-async method body's own direct `await` (not inside a nested
/// initializer/default) is unaffected: still the ordinary TS1308 through the
/// pre-existing `function_depth > 0` path.
#[test]
fn method_body_await_still_reports_ts1308() {
    let out = codes("class K { m() { await 1; } }");
    assert!(out.contains(&1308), "got {out:?}");
    assert!(!out.contains(&1375), "got {out:?}");
}

/// A class static block's own direct `await` never takes the top-level
/// branch (TS1375/TS1378) — `function_depth > 0` from
/// `enter_class_member_body` routes it away from the new walk entirely,
/// same as an ordinary method body.
#[test]
fn static_block_await_does_not_report_ts1375() {
    let out = codes("class K { static { await 1; } }");
    assert!(!out.contains(&1375), "got {out:?}");
    assert!(!out.contains(&1378), "got {out:?}");
}
