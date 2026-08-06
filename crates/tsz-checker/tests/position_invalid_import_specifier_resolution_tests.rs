//! A position-invalid `import ... from "m"` or `import x = require("m")`
//! outside a declaration scope resolves its module specifier only when the
//! file is *not* an external module — the import-side half of #16495's
//! inversion (#16505), split out once the export side landed in #16504.
//!
//! `tsc`'s `checkImportDeclaration` reports the placement diagnostic (TS1232)
//! and returns, so `resolveExternalModuleName` never runs from that call.
//! Outside a declaration scope (a bare block, an `if`/loop body — a function
//! body, a method, a `static { }` block and a namespace body are still
//! declaration scopes and are unaffected by this file, see
//! `position_invalid_import_equals_container_scope_tests.rs` and
//! `import_namespace_ts1147_tests.rs`), what still resolves comes from a
//! later pass that only exists when the file is *not* an external module.
//!
//! Every clause-bearing form (`import { a }`, `import a`, `import * as ns`,
//! `import a, { b }`, `import type { a }`, and `import x = require(...)`)
//! shares this one axis; there is no clause-kind split the way `export ...
//! from` has one (#16504). A side-effect-only `import "m"` is the exception:
//! its own diagnostic (TS2882/TS2307) never fires outside a declaration
//! scope, in a script or a module alike.
//!
//! Every expectation below is measured against the pinned `typescript@7.0.2`
//! oracle through `scripts/conformance/oracle.sh`, with
//! `--strict --lib es2022 --target es2022 --module esnext --moduleResolution
//! bundler`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single source with unresolved-import reporting on, so a module
/// specifier naming a nonexistent module reports TS2307/TS2882 if it is
/// resolved at all.
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
            module: tsz_common::common::ModuleKind::ESNext,
            target: tsz_common::common::ScriptTarget::ESNext,
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
// A script (no module indicator): every form still resolves in a bare block,
// an `if` body and a loop body — unchanged by this PR.
// ---------------------------------------------------------------------------

#[test]
fn named_import_in_a_bare_block_of_a_script_reports_ts2307() {
    assert_codes(
        r#"{
  import { a } from "nonexistent-module";
}"#,
        &[1232, 2307],
        "a script still has no export/import table gate; the named form keeps resolving",
    );
}

#[test]
fn default_import_in_an_if_body_of_a_script_reports_ts2307() {
    assert_codes(
        r#"if (true) {
  import a from "nonexistent-module";
}"#,
        &[1232, 2307],
        "container kind does not matter, only module-ness",
    );
}

#[test]
fn namespace_import_in_a_loop_body_of_a_script_reports_ts2307() {
    assert_codes(
        r#"for (;;) {
  import * as ns from "nonexistent-module";
}"#,
        &[1232, 2307],
        "the namespace-import form takes the same path as the named form on this axis",
    );
}

#[test]
fn default_plus_named_import_in_a_bare_block_of_a_script_reports_ts2307() {
    assert_codes(
        r#"{
  import a, { b } from "nonexistent-module";
}"#,
        &[1232, 2307],
        "combined default+named clause behaves like the plain named clause",
    );
}

#[test]
fn type_only_import_in_a_bare_block_of_a_script_reports_ts2307() {
    assert_codes(
        r#"{
  import type { a } from "nonexistent-module";
}"#,
        &[1232, 2307],
        "a type-only clause is still a clause for this axis",
    );
}

#[test]
fn import_equals_require_in_a_bare_block_of_a_script_reports_ts2307() {
    assert_codes(
        r#"{
  import x = require("nonexistent-module");
}"#,
        &[1232, 2307],
        "import-equals shares the clause-bearing axis, not the export side's own rule",
    );
}

// ---------------------------------------------------------------------------
// THE DISCRIMINATOR: the same forms in an external module report TS1232
// alone. This is the bug #16505 tracks.
// ---------------------------------------------------------------------------

#[test]
fn named_import_in_a_bare_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
{
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "an external module never reaches a later pass for a position-invalid named import",
    );
}

#[test]
fn default_import_in_an_if_body_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
if (true) {
  import a from "nonexistent-module";
}"#,
        &[1232],
        "the module-ness axis is independent of which non-declaration container it is",
    );
}

#[test]
fn namespace_import_in_a_loop_body_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
for (;;) {
  import * as ns from "nonexistent-module";
}"#,
        &[1232],
        "the namespace-import form swaps roles with the export side's own `export * as ns`",
    );
}

#[test]
fn default_plus_named_import_in_a_bare_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
{
  import a, { b } from "nonexistent-module";
}"#,
        &[1232],
        "combined default+named clause behaves like the plain named clause",
    );
}

#[test]
fn type_only_import_in_a_bare_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
{
  import type { a } from "nonexistent-module";
}"#,
        &[1232],
        "a type-only clause is still a clause for this axis",
    );
}

#[test]
fn import_equals_require_in_a_bare_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
{
  import x = require("nonexistent-module");
}"#,
        &[1232],
        "import-equals shares the clause-bearing axis, via its own gate in equals.rs",
    );
}

#[test]
fn a_module_indicator_other_than_export_braces_also_flips_the_named_import() {
    assert_codes(
        r#"export const q = 1;
{
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "the gate reads the file's module-ness, not the `export {}` spelling",
    );
}

// ---------------------------------------------------------------------------
// A side-effect-only import is the one form that answers differently: it
// never resolves outside a declaration scope, in a script or a module alike.
// ---------------------------------------------------------------------------

#[test]
fn side_effect_import_in_a_bare_block_of_a_script_reports_ts1232_alone() {
    assert_codes(
        r#"{
  import "nonexistent-module";
}"#,
        &[1232],
        "a side-effect import's own diagnostic never fires outside a declaration \
         scope, unlike every clause-bearing form above",
    );
}

#[test]
fn side_effect_import_in_a_bare_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
{
  import "nonexistent-module";
}"#,
        &[1232],
        "module-ness does not matter for a side-effect import either",
    );
}

#[test]
fn a_side_effect_import_at_a_valid_position_still_resolves() {
    assert_codes(
        r#"import "nonexistent-module";"#,
        &[2882],
        "control: a valid-position side-effect import still reports its own \
         TS2882, so this PR only changes the wrong-context answer",
    );
}

// ---------------------------------------------------------------------------
// Declaration scopes still win over module-ness, exactly as they did before
// this PR — a function body, a method body and a `static { }` block suppress
// regardless of the file's module-ness.
// ---------------------------------------------------------------------------

#[test]
fn named_import_in_a_function_body_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
function f() {
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "a declaration scope is unaffected by this PR's module-ness refinement",
    );
}

#[test]
fn named_import_in_a_class_static_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
class C {
  static {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "a `static { }` block is a declaration scope in a module as well",
    );
}

#[test]
fn named_import_in_a_function_body_of_a_script_reports_ts1232_alone() {
    assert_codes(
        r#"function f() {
  import { a } from "nonexistent-module";
}"#,
        &[1232],
        "the function-body answer does not depend on module-ness either",
    );
}

// ---------------------------------------------------------------------------
// Resolvability of the specifier does not change the verdict, on either side
// of the module-ness axis — matching what #16504 already established for the
// export side rather than assuming it carries over unmeasured.
// ---------------------------------------------------------------------------

#[test]
fn a_resolvable_named_import_in_a_bare_block_of_a_script_still_reports_the_member_error() {
    assert_codes(
        r#"declare module "amb" {
  export const a: number;
}
{
  import { b } from "amb";
}"#,
        &[1232, 2305],
        "a script keeps resolving even when the specifier resolves; the member \
         lookup then reports its own TS2305",
    );
}

#[test]
fn a_resolvable_named_import_in_a_bare_block_of_a_module_reports_ts1232_alone() {
    assert_codes(
        r#"export {};
declare module "amb" {
  export const a: number;
}
{
  import { b } from "amb";
}"#,
        &[1232, 2664],
        "a module suppresses the member lookup too — resolvability of the \
         specifier never enters into it; TS2664 is the unrelated ambient-module \
         augmentation diagnostic and fires regardless of this gate",
    );
}
