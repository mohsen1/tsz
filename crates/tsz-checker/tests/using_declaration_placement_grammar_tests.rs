//! `using` / `await using` declaration *placement* grammar — TS1545, TS1546,
//! TS1547, TS1548.
//!
//! tsc keys all four on the declaration *list* rather than on the declaration or
//! the `using` keyword: an ambient context rejects the form outright (TS1545 /
//! TS1546), and a `case` or `default` clause rejects it unless a block intervenes
//! (TS1547 / TS1548). Reporting one of them ends the list's grammar checking, so
//! the `await using` placement family (TS2852 / TS2853) does not also fire.
//!
//! Every expectation here is pinned against the vendored oracle
//! (`typescript@7.0.2`, `--target es2022 --module esnext --lib esnext`), including
//! the negatives.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ESNext,
        ..CheckerOptions::default()
    }
}

fn check_codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", options())
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn check_codes_in_declaration_file(source: &str) -> Vec<u32> {
    check_source(source, "test.d.ts", options())
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

// ---------------------------------------------------------------------------
// TS1545 / TS1546 — ambient contexts
// ---------------------------------------------------------------------------

#[test]
fn using_in_ambient_namespace_emits_ts1545() {
    let codes = check_codes("declare namespace N { using a = null; }");
    assert!(codes.contains(&1545), "expected TS1545, got {codes:?}");
}

#[test]
fn renamed_binders_using_in_ambient_namespace_emits_ts1545() {
    let codes = check_codes("declare namespace Outer { using resource = null; }");
    assert!(codes.contains(&1545), "expected TS1545, got {codes:?}");
}

#[test]
fn await_using_in_ambient_namespace_emits_ts1546() {
    let codes = check_codes("declare namespace N { await using b = null; }");
    assert!(codes.contains(&1546), "expected TS1546, got {codes:?}");
}

#[test]
fn using_in_ambient_module_declaration_emits_ts1545() {
    let codes = check_codes("declare module \"m\" { using c = null; }");
    assert!(codes.contains(&1545), "expected TS1545, got {codes:?}");
}

#[test]
fn using_in_nested_ambient_namespace_emits_ts1545() {
    let codes = check_codes("declare namespace A { namespace B { using d = null; } }");
    assert!(codes.contains(&1545), "expected TS1545, got {codes:?}");
}

#[test]
fn using_at_declaration_file_top_level_emits_ts1545() {
    let codes = check_codes_in_declaration_file("using e = null;");
    assert!(codes.contains(&1545), "expected TS1545, got {codes:?}");
}

#[test]
fn await_using_at_declaration_file_top_level_emits_ts1546() {
    let codes = check_codes_in_declaration_file("await using f = null;");
    assert!(codes.contains(&1546), "expected TS1546, got {codes:?}");
}

// ---------------------------------------------------------------------------
// TS1547 / TS1548 — `case` and `default` clauses
// ---------------------------------------------------------------------------

#[test]
fn using_directly_in_case_clause_emits_ts1547() {
    let codes = check_codes(
        r#"
declare const x: number;
switch (x) {
    case 1:
        using a = null;
        break;
}
"#,
    );
    assert!(codes.contains(&1547), "expected TS1547, got {codes:?}");
}

#[test]
fn using_directly_in_default_clause_emits_ts1547() {
    let codes = check_codes(
        r#"
declare const x: number;
switch (x) {
    default:
        using a = null;
}
"#,
    );
    assert!(codes.contains(&1547), "expected TS1547, got {codes:?}");
}

#[test]
fn await_using_directly_in_case_clause_emits_ts1548() {
    let codes = check_codes(
        r#"
declare const x: number;
async function f() {
    switch (x) {
        case 1:
            await using a = null;
            break;
    }
}
"#,
    );
    assert!(codes.contains(&1548), "expected TS1548, got {codes:?}");
}

#[test]
fn await_using_directly_in_default_clause_emits_ts1548() {
    let codes = check_codes(
        r#"
declare const x: number;
async function f() {
    switch (x) {
        default:
            await using a = null;
    }
}
"#,
    );
    assert!(codes.contains(&1548), "expected TS1548, got {codes:?}");
}

#[test]
fn multi_declarator_using_in_case_clause_reports_ts1547_once() {
    let codes = check_codes(
        r#"
declare const x: number;
switch (x) {
    case 1:
        using a = null, b = null;
        break;
}
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == 1547).count(),
        1,
        "TS1547 is keyed on the declaration list, not on each declarator, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negatives — the rule is about placement, so everything else must stay clean
// ---------------------------------------------------------------------------

#[test]
fn using_inside_a_block_in_a_case_clause_is_clean() {
    let codes = check_codes(
        r#"
declare const x: number;
switch (x) {
    case 1: {
        using a = null;
        break;
    }
}
"#,
    );
    assert!(
        !codes.contains(&1547) && !codes.contains(&1548),
        "a block between the clause and the list makes it legal, got {codes:?}"
    );
}

#[test]
fn using_in_a_for_of_head_in_a_case_clause_is_clean() {
    let codes = check_codes(
        r#"
declare const x: number;
declare const xs: any[];
switch (x) {
    case 1:
        for (using a of xs) { }
        break;
}
"#,
    );
    assert!(
        !codes.contains(&1547),
        "a for-of head is not a variable statement, got {codes:?}"
    );
}

#[test]
fn using_in_an_ordinary_function_body_is_clean() {
    let codes = check_codes("function f() { using a = null; }");
    assert!(
        !codes.contains(&1545) && !codes.contains(&1547),
        "a plain block body is neither ambient nor a clause, got {codes:?}"
    );
}

#[test]
fn using_in_a_non_ambient_namespace_is_clean() {
    let codes = check_codes("export {};\nnamespace N { using a = null; }");
    assert!(
        !codes.contains(&1545),
        "a namespace without `declare` is not an ambient context, got {codes:?}"
    );
}

#[test]
fn const_let_and_var_in_an_ambient_namespace_are_clean() {
    let codes = check_codes("declare namespace N { const a = 1; let b: any; var c: any; }");
    assert!(
        !codes.contains(&1545) && !codes.contains(&1546),
        "only using / await using are restricted, got {codes:?}"
    );
}

#[test]
fn a_binder_named_using_in_an_ambient_namespace_is_clean() {
    let codes = check_codes("declare namespace N { const using = 1; }");
    assert!(
        !codes.contains(&1545) && !codes.contains(&1546),
        "`using` as a binder name is not a using declaration, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The suppression that follows from tsc reporting at most one list-level error
// ---------------------------------------------------------------------------

#[test]
fn ambient_await_using_reports_ts1546_instead_of_ts2852() {
    let codes = check_codes("declare namespace N { await using b = null; }");
    assert!(
        codes.contains(&1546) && !codes.contains(&2852),
        "TS1546 ends the list's grammar checking, got {codes:?}"
    );
}

#[test]
fn declaration_file_await_using_reports_ts1546_instead_of_ts2853() {
    let codes = check_codes_in_declaration_file("await using f = null;");
    assert!(
        codes.contains(&1546) && !codes.contains(&2853),
        "TS1546 ends the list's grammar checking, got {codes:?}"
    );
}

#[test]
fn top_level_await_using_in_a_script_still_reports_ts2853() {
    let codes = check_codes("await using topA = null;");
    assert!(
        codes.contains(&2853),
        "the await-using placement family must survive where no placement error fires, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// TS1039 must not fire for an ambient `using` initializer
// ---------------------------------------------------------------------------

#[test]
fn ambient_using_with_a_literal_initializer_does_not_emit_ts1039() {
    let codes = check_codes("declare namespace N { using a = 1; }");
    assert!(
        !codes.contains(&1039),
        "using is const-like, so it takes the TS1254 arm of the ambient-initializer \
         check and never the TS1039 arm, got {codes:?}"
    );
}

#[test]
fn ambient_using_with_a_non_literal_initializer_reports_ts1254_alone() {
    let codes = check_codes("declare namespace N { using a = null; }");
    assert!(
        codes.contains(&1254) && !codes.contains(&1039),
        "expected the const-like arm (TS1254) and not TS1039, got {codes:?}"
    );
}

#[test]
fn ambient_annotated_const_still_emits_ts1039() {
    let codes = check_codes("declare namespace N { const a: any = {}; }");
    assert!(
        codes.contains(&1039),
        "an annotated ambient const still takes the TS1039 arm, got {codes:?}"
    );
}
