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
//! Every expectation below is a row where `typescript@7.0.2` returns the same
//! answer in **both** threading modes (default and `--singleThreaded`). The
//! top-level-block family, where the two modes disagree, is deliberately left
//! alone — see #16413 — and is pinned here as a control asserting the behavior
//! `main` already has.

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
    // The top-level-block family is where typescript@7.0.2's two threading modes
    // disagree (#16413): the default scheduler reports TS1233 alone, and
    // `--singleThreaded` — the mode `generate-tsc-cache.rs` uses, and therefore
    // the mode the conformance corpus is scored in — reports TS1233 + TS2307.
    // This fix changes nothing here; the row is pinned so a later widening of the
    // gate cannot silently move it without the disagreement being settled first.
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
