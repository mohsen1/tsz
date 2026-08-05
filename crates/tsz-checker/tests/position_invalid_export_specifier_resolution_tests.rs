//! A position-invalid `export ... from "m"` resolves its module specifier only
//! when no declaration scope encloses it.
//!
//! tsc's `checkExportDeclaration` reports the placement diagnostic (TS1233) and
//! `return`s, so `resolveExternalModuleName` — the only TS2307/TS2305 site — never
//! runs. That return is reached when a *declaration scope* encloses the
//! declaration: a function-like body, or a namespace/ambient-module body the
//! declaration does not directly belong to. A container that opens no declaration
//! scope (a bare block, an `if`/loop/`try` body, a labeled statement, a `switch`
//! clause) leaves the declaration in the source file's own scope, and tsc keeps
//! resolving there.
//!
//! Outside a declaration scope that answer is not the whole rule. The check has
//! already returned, so whatever still resolves comes from a later pass over the
//! symbol table the binder used — and which table that is depends on the file. In
//! an external module the file symbol carries an export table whose computation
//! resolves an `export *` entry eagerly, while a named or namespace clause stays a
//! lazily-resolved alias nothing references; in a file that is not a module there
//! is no export table to compute, so `export *` is never resolved and only a named
//! clause's specifiers bind as aliases a later pass reaches. The two forms swap
//! roles across that one axis (#16495).
//!
//! Every expectation below is measured against the pinned `typescript@7.0.2`
//! oracle through `scripts/conformance/oracle.sh`, which pins the
//! `--singleThreaded --stableTypeOrdering` invocation the conformance cache
//! generator uses — the disagreement recorded here as #16413 was an artifact of
//! comparing against a bare `tsc` run, and is settled.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single source with unresolved-import reporting on, so a specifier
/// naming a nonexistent module reports TS2307 if it is resolved at all.
fn check(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    let mut codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn assert_codes(source: &str, expected: &[u32], what: &str) {
    let actual = check(source);
    assert_eq!(actual, expected, "{what}\nsource:\n{source}");
}

// ---------------------------------------------------------------------------
// Function-like containers suppress resolution. Both threading modes agree.
// ---------------------------------------------------------------------------

#[test]
fn export_star_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "a function body is a declaration scope",
    );
}

#[test]
fn export_named_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  export { a } from "nonexistent-module";
}"#,
        &[1233],
        "the named form takes the same path as `export *`",
    );
}

#[test]
fn export_star_as_ns_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  export * as ns from "nonexistent-module";
}"#,
        &[1233],
        "`export * as ns` takes the same path",
    );
}

#[test]
fn export_in_block_nested_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233],
        "a block inside a function body is still inside the function's scope",
    );
}

#[test]
fn export_in_arrow_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"const g = () => {
  export { a } from "nonexistent-module";
};"#,
        &[1233],
        "an arrow body is a declaration scope",
    );
}

#[test]
fn export_in_method_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"class C {
  m() {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233],
        "a method body is a declaration scope",
    );
}

#[test]
fn export_in_class_static_block_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"class C {
  static {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233],
        "a class static block is function-like for scope purposes",
    );
}

#[test]
fn export_in_function_nested_in_a_top_level_block_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"{
  function f() {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233],
        "the innermost declaration scope decides, not the outermost container",
    );
}

#[test]
fn renamed_binders_do_not_change_the_verdict_in_a_function_body() {
    // Same shape as the `function f` row with every user-chosen name changed.
    assert_codes(
        r#"function zzTop() {
  export { qqq } from "no-such-package-here";
}"#,
        &[1233],
        "the rule is structural; no identifier or specifier text participates",
    );
}

// ---------------------------------------------------------------------------
// Namespace containers suppress resolution. Both threading modes agree.
// ---------------------------------------------------------------------------

#[test]
fn export_from_directly_in_a_namespace_body_reports_ts1194_without_ts2307() {
    assert_codes(
        r#"namespace P {
  export { a } from "nonexistent-module";
}"#,
        &[1194],
        "tsc reports TS1194 from checkExternalImportOrExportDeclaration, which \
         then returns false and skips resolution",
    );
}

#[test]
fn export_star_directly_in_a_namespace_body_reports_ts1194_without_ts2307() {
    assert_codes(
        r#"namespace P {
  export * from "nonexistent-module";
}"#,
        &[1194],
        "the star form takes the same TS1194 path",
    );
}

#[test]
fn export_in_a_block_inside_a_namespace_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"namespace P {
  {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233],
        "a namespace body is a declaration scope for a block nested inside it",
    );
}

#[test]
fn export_in_a_function_inside_a_namespace_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"namespace P {
  function f() {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233],
        "two nested declaration scopes still suppress",
    );
}

#[test]
fn renamed_namespace_binder_does_not_change_the_verdict() {
    assert_codes(
        r#"namespace WhateverName {
  {
    export { zzz } from "no-such-package-here";
  }
}"#,
        &[1233],
        "the namespace's own name does not participate",
    );
}

// ---------------------------------------------------------------------------
// Negative controls: containers that must KEEP resolving.
// ---------------------------------------------------------------------------

#[test]
fn a_top_level_export_still_reports_ts2307() {
    assert_codes(
        r#"export { a } from "nonexistent-module";"#,
        &[2307],
        "a valid module element context resolves normally",
    );
}

#[test]
fn an_export_directly_in_an_ambient_module_body_still_reports_ts2307() {
    assert_codes(
        r#"declare module "amb" {
  export { a } from "nonexistent-module";
}"#,
        &[2307],
        "an ambient module body IS a module-element context — the falsifying \
         control for any ordering-based 'silence TS2307 after a placement error' fix",
    );
}

#[test]
fn a_bare_top_level_block_keeps_resolving_unchanged() {
    // A named clause in a file that is not a module is the one cell of the
    // #16495 table that really does resolve: with no export table to compute,
    // the specifiers bind as ordinary aliases that a later pass reaches. This
    // row was previously pinned on the belief that typescript@7.0.2's two
    // threading modes disagree about it (#16413) — they do not, and it is
    // oracle-measured now rather than pinned to whatever `main` happened to do.
    assert_codes(
        r#"{
  export { a } from "nonexistent-module";
}"#,
        &[1233, 2307],
        "a bare top-level block opens no declaration scope",
    );
}

#[test]
fn an_if_block_at_top_level_keeps_resolving_unchanged() {
    assert_codes(
        r#"if (1) {
  export { a } from "nonexistent-module";
}"#,
        &[1233, 2307],
        "an `if` body opens no declaration scope",
    );
}

#[test]
fn a_loop_body_at_top_level_keeps_resolving_unchanged() {
    assert_codes(
        r#"for (;;) {
  export { a } from "nonexistent-module";
}"#,
        &[1233, 2307],
        "a loop body opens no declaration scope",
    );
}

#[test]
fn a_labeled_block_at_top_level_keeps_resolving_unchanged() {
    assert_codes(
        r#"lbl: {
  export { a } from "nonexistent-module";
}"#,
        &[1233, 2307],
        "a labeled statement opens no declaration scope",
    );
}

#[test]
fn a_nested_bare_block_at_top_level_keeps_resolving_unchanged() {
    assert_codes(
        r#"{
  {
    export { a } from "nonexistent-module";
  }
}"#,
        &[1233, 2307],
        "nesting plain blocks never crosses a declaration scope",
    );
}

// ---------------------------------------------------------------------------
// Outside a declaration scope, the export clause and the file's module-ness
// decide together (#16495).
//
// The check has already returned at the placement diagnostic, so anything that
// still resolves comes from a later pass over whichever symbol table the binder
// used. In an external module the file symbol has an export table, and computing
// it resolves the export-star entry eagerly while a named/namespace clause stays
// a lazily-resolved alias nothing references. With no export table to compute,
// the export-star is never resolved at all and only a named clause's individual
// specifiers bind as ordinary aliases a later pass reaches.
//
// So the two forms swap roles across that one axis. Every row below is measured
// against the pinned `typescript@7.0.2` oracle through `scripts/conformance/oracle.sh`.
// ---------------------------------------------------------------------------

#[test]
fn export_star_in_a_bare_top_level_block_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"{
  export * from "nonexistent-module";
}"#,
        &[1233],
        "no export table to compute, so the export-star entry is never resolved",
    );
}

#[test]
fn export_star_in_an_if_body_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"if (1) {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "an `if` body opens no declaration scope, but the file is not a module",
    );
}

#[test]
fn export_star_in_a_loop_body_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"for (;;) {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "a loop body opens no declaration scope, but the file is not a module",
    );
}

#[test]
fn export_star_in_a_while_body_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"while (1) {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "a `while` body behaves as the `for` body does",
    );
}

#[test]
fn export_star_in_a_try_body_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"try {
  export * from "nonexistent-module";
} catch {}"#,
        &[1233],
        "a `try` body behaves as any other non-declaration-scope container",
    );
}

#[test]
fn export_star_in_a_labeled_block_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"lbl: {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "a labeled statement opens no declaration scope",
    );
}

#[test]
fn export_star_in_a_nested_bare_block_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"{
  {
    export * from "nonexistent-module";
  }
}"#,
        &[1233],
        "nesting plain blocks never crosses a declaration scope, and never \
         creates an export table either",
    );
}

#[test]
fn export_star_as_ns_in_a_bare_top_level_block_of_a_script_reports_ts1233_alone() {
    assert_codes(
        r#"{
  export * as ns from "nonexistent-module";
}"#,
        &[1233],
        "a namespace export clause is an alias, so it resolves in neither file kind",
    );
}

#[test]
fn a_renamed_namespace_export_binder_does_not_change_the_verdict() {
    assert_codes(
        r#"{
  export * as zzQq from "nonexistent-module";
}"#,
        &[1233],
        "the verdict is over the clause kind, never over the binder's spelling",
    );
}

#[test]
fn export_star_in_a_bare_top_level_block_of_a_module_reports_ts2307() {
    assert_codes(
        r#"export {};
{
  export * from "nonexistent-module";
}"#,
        &[1233, 2307],
        "THE DISCRIMINATOR: the same block in a module has an export table, and \
         computing it resolves the export-star entry",
    );
}

#[test]
fn export_star_in_an_if_body_of_a_module_reports_ts2307() {
    assert_codes(
        r#"export {};
if (1) {
  export * from "nonexistent-module";
}"#,
        &[1233, 2307],
        "the module-ness axis is independent of which non-declaration container it is",
    );
}

#[test]
fn a_module_indicator_other_than_export_braces_also_flips_the_export_star() {
    assert_codes(
        r#"export const q = 1;
{
  export * from "nonexistent-module";
}"#,
        &[1233, 2307],
        "the gate reads the file's module-ness, not the `export {}` spelling",
    );
}

#[test]
fn export_named_in_a_bare_top_level_block_of_a_module_reports_ts1233_alone() {
    assert_codes(
        r#"export {};
{
  export { a } from "nonexistent-module";
}"#,
        &[1233],
        "THE INVERSION: a named clause resolves in a script and stays silent in a \
         module — exactly the opposite of the export-star above",
    );
}

#[test]
fn export_star_as_ns_in_a_bare_top_level_block_of_a_module_reports_ts1233_alone() {
    assert_codes(
        r#"export {};
{
  export * as ns from "nonexistent-module";
}"#,
        &[1233],
        "a namespace export clause stays an unreferenced alias in a module too",
    );
}

#[test]
fn a_declaration_scope_still_wins_over_module_ness() {
    assert_codes(
        r#"export {};
function f() {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "the declaration-scope answer is final; module-ness only refines the case \
         where no declaration scope encloses the declaration",
    );
}

#[test]
fn a_class_static_block_in_a_module_still_wins_over_module_ness() {
    assert_codes(
        r#"export {};
class C {
  static {
    export * from "nonexistent-module";
  }
}"#,
        &[1233],
        "a `static { }` block is a declaration scope in a module as well",
    );
}

// ---------------------------------------------------------------------------
// The import side is untouched: it already matched, and it is a different
// production (`checkImportDeclaration`, gated by its own container proxy).
// ---------------------------------------------------------------------------

#[test]
fn an_import_in_a_function_body_is_unchanged() {
    assert_codes(
        r#"function f() {
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "the import path already suppressed here before this change",
    );
}

#[test]
fn an_import_in_a_bare_top_level_block_is_unchanged() {
    assert_codes(
        r#"{
  import { a } from "nonexistent-module";
}"#,
        &[1232, 2307],
        "the import path already resolved here before this change",
    );
}

// ---------------------------------------------------------------------------
// TS1184 is a modifier diagnostic, not a placement diagnostic: the wrapped
// declaration keeps being checked either way.
// ---------------------------------------------------------------------------

#[test]
fn an_export_modifier_in_a_function_body_still_checks_the_wrapped_declaration() {
    let codes = check(
        r#"function f() {
  export class C {
    m(): NotAType {}
  }
}"#,
    );
    assert!(
        codes.contains(&1184),
        "expected TS1184 for the export modifier, got {codes:?}"
    );
    assert!(
        codes.contains(&2304),
        "expected the wrapped class to still be checked (TS2304), got {codes:?}"
    );
}
