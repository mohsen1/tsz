//! A module-element declaration in a non-module-element context is checked no
//! further — in particular its module specifier is never resolved.
//!
//! tsc's `checkImportDeclaration` / `checkExportDeclaration` / `checkExportAssignment`
//! report the placement diagnostic and then `return`. Nothing downstream of that
//! point runs, and `resolveExternalModuleName` — the only TS2307 site — is
//! downstream of it. So `function f() { export * from "nope"; }` is TS1233 alone,
//! not TS1233 + TS2307.
//!
//! Two shapes are deliberately *not* short-circuited, both pinned below:
//!
//! * TS1184. When the `export` prefixes a class/function/variable declaration in a
//!   block, tsc rejects the modifier and still checks the wrapped declaration.
//! * A `ModuleBlock` parent that is an ambient module. `declare module "amb" { ... }`
//!   is a valid module-element context, so resolution runs and TS2307 is correct.
//!
//! All expectations were taken from `typescript@7.0.2` with
//! `--strict --lib es2022 --target es2022 --module esnext --moduleResolution bundler`.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single source with unresolved-import reporting on, so that a module
/// specifier naming a nonexistent module would report TS2307 if it were resolved.
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
// Export declarations: TS1233 alone, across every non-module-element container.
// ---------------------------------------------------------------------------

#[test]
fn export_star_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "a function body is not a module element context",
    );
}

#[test]
fn export_star_in_bare_block_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"{
  export * from "nonexistent-module";
}"#,
        &[1233],
        "a bare block behaves exactly like a function body here",
    );
}

#[test]
fn export_star_in_if_block_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"if (true) {
  export * from "nonexistent-module";
}"#,
        &[1233],
        "an if-statement block is not a module element context",
    );
}

#[test]
fn export_star_in_method_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"class C {
  m() {
    export * from "nonexistent-module";
  }
}"#,
        &[1233],
        "a method body is not a module element context",
    );
}

#[test]
fn named_reexport_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  export { a } from "nonexistent-module";
}
const a = 1;"#,
        &[1233],
        "the named-reexport form short-circuits like the star form",
    );
}

#[test]
fn namespace_reexport_in_function_body_reports_ts1233_without_ts2307() {
    assert_codes(
        r#"function f() {
  export * as ns from "nonexistent-module";
}"#,
        &[1233],
        "the `export * as ns` form short-circuits like the star form",
    );
}

// ---------------------------------------------------------------------------
// Import declarations: the same rule, as the adjacent binder axis.
// ---------------------------------------------------------------------------

#[test]
fn import_in_bare_block_reports_ts1232_without_ts2307() {
    assert_codes(
        r#"{
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "an import in a bare block resolves nothing, same as in a function body",
    );
}

#[test]
fn import_in_function_body_reports_ts1232_without_ts2307() {
    assert_codes(
        r#"function f() {
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "an import in a function body resolves nothing",
    );
}

#[test]
fn namespace_import_in_loop_body_reports_ts1232_without_ts2307() {
    assert_codes(
        r#"for (;;) {
  import * as x from "nonexistent-module";
}"#,
        &[1232],
        "a loop body is not a module element context",
    );
}

#[test]
fn type_only_import_grammar_is_also_short_circuited_in_a_block() {
    // tsc reports TS1232 alone here: TS1363 ("a type-only import can specify a
    // default import or named bindings, but not both") lives downstream of the
    // placement `return`, so it never fires.
    assert_codes(
        r#"{
  import type A, { B } from "nonexistent-module";
}"#,
        &[1232],
        "sibling grammar checks on the declaration are short-circuited too",
    );
}

// ---------------------------------------------------------------------------
// Export assignment / default export: the remaining placement codes.
// ---------------------------------------------------------------------------

#[test]
fn export_assignment_in_function_body_reports_ts1231_alone() {
    assert_codes(
        r#"function f() {
  export = undefinedName;
}"#,
        &[1231],
        "TS1231 ends the check, so the exported name is never resolved (no TS2304)",
    );
}

/// The `export default` half of the same rule. Still failing: the placement
/// short-circuit lands (TS1258 fires and `check_export_declaration` is skipped),
/// but a *second* walker types the default-exported expression and reports
/// TS2552 for the unknown name, so tsz emits `[1258, 2552]` where tsc emits
/// `[1258]` alone. That walker is not the module-specifier resolution path this
/// file is about, and it is reached for a top-level `export default` too — so
/// gating it belongs with whoever owns the default-export expression pass, not
/// here. Pinned to tsc's answer so closing it is deleting this attribute.
#[test]
#[ignore = "residual: a second walker types the default-exported expression; see doc comment"]
fn export_default_in_bare_block_reports_ts1258_alone() {
    assert_codes(
        r#"{
  export default undefinedName;
}"#,
        &[1258],
        "TS1258 ends the check, so the exported expression is never resolved",
    );
}

// ---------------------------------------------------------------------------
// A namespace body is a ModuleBlock, so it takes the TS1194 arm instead — and
// tsc's `checkExternalImportOrExportDeclaration` returns false there too.
// ---------------------------------------------------------------------------

#[test]
fn export_star_in_namespace_reports_ts1194_without_ts2307() {
    assert_codes(
        r#"namespace N {
  export * from "nonexistent-module";
}"#,
        &[1194],
        "TS1194 gates module resolution the same way TS1233 does",
    );
}

#[test]
fn named_reexport_in_namespace_reports_ts1194_without_ts2307() {
    assert_codes(
        r#"namespace N {
  export { a } from "nonexistent-module";
}
const a = 1;"#,
        &[1194],
        "the named-reexport form takes the same TS1194 gate",
    );
}

// ---------------------------------------------------------------------------
// Negative controls. These must keep resolving.
// ---------------------------------------------------------------------------

#[test]
fn export_star_at_top_level_still_reports_ts2307() {
    assert_codes(
        r#"export * from "nonexistent-module";"#,
        &[2307],
        "the control: a well-placed re-export must still resolve its specifier",
    );
}

#[test]
fn export_star_inside_ambient_module_still_reports_ts2307() {
    // The falsifying control for an ordering-based fix. `declare module "amb"` is a
    // valid module-element context, so nothing is suppressed here — a fix keyed on
    // "a placement diagnostic was already emitted for this file" would wrongly
    // silence this row.
    assert_codes(
        r#"declare module "amb" {
  export * from "nonexistent-module";
}"#,
        &[2307],
        "an ambient module body is a valid context; resolution still runs",
    );
}

#[test]
fn import_at_top_level_still_reports_ts2307() {
    assert_codes(
        r#"import { a } from "nonexistent-module";"#,
        &[2307],
        "the control on the import side",
    );
}

#[test]
fn exported_class_in_a_block_still_checks_the_wrapped_declaration() {
    // TS1184 rejects the MODIFIER, not the statement's placement. tsc keeps checking
    // the class, so the unresolved return-type annotation must still report TS2304.
    let codes = check(
        r#"{
  export class C { m(): NotAType { return 1; } }
}"#,
    );
    assert!(
        codes.contains(&1184),
        "expected TS1184 for the export modifier, got {codes:?}"
    );
    assert!(
        codes.contains(&2304),
        "TS1184 must not short-circuit the wrapped declaration, got {codes:?}"
    );
}

#[test]
fn exported_variable_in_a_block_still_checks_the_wrapped_declaration() {
    let codes = check(
        r#"{
  export const v: NotAType = 1;
}"#,
    );
    assert!(
        codes.contains(&1184),
        "expected TS1184 for the export modifier, got {codes:?}"
    );
    assert!(
        codes.contains(&2304),
        "TS1184 must not short-circuit the wrapped declaration, got {codes:?}"
    );
}

#[test]
fn exported_function_in_a_block_still_checks_the_wrapped_declaration() {
    let codes = check(
        r#"{
  export function f(): NotAType { return 1; }
}"#,
    );
    assert!(
        codes.contains(&1184),
        "expected TS1184 for the export modifier, got {codes:?}"
    );
    assert!(
        codes.contains(&2304),
        "TS1184 must not short-circuit the wrapped declaration, got {codes:?}"
    );
}

#[test]
fn exported_namespace_in_a_block_still_checks_its_body() {
    // `export namespace M {}` gets TS1235 from `check_module_declaration`, so the
    // placement check declines to report and must not short-circuit: the inner
    // re-export still reports its own TS1194.
    let codes = check(
        r#"{
  export namespace M { export * from "nonexistent-module"; }
}"#,
    );
    assert!(
        codes.contains(&1235),
        "expected TS1235 for the misplaced namespace, got {codes:?}"
    );
    assert!(
        codes.contains(&1194),
        "the namespace body must still be checked, got {codes:?}"
    );
}
