//! A position-invalid import/export in a **top-level** block / `if` body / loop
//! body resolves its module specifier on two axes the container-scope walk
//! cannot see: whether the file is an external module, and the production kind.
//!
//! Once no declaration scope encloses the element (so tsc's
//! `checkImportDeclaration`/`checkExportDeclaration` does not `return` at the
//! placement diagnostic), `resolveExternalModuleName` runs under tsc's
//! `markAliasReferenced` rule — an import binding's specifier is resolved when
//! the binding is *used*, wherever it sits — plus two facts a use cannot
//! express: a **script** resolves its bound-but-unused top-level-block imports
//! and its `export { } from`, while a **module** resolves its top-level-block
//! `export *` (only a module's export set is ever computed). A side-effect
//! `import "m"` binds nothing, so nothing ever marks it referenced.
//!
//! Every expectation was measured with `scripts/conformance/oracle.sh`
//! (`typescript@7.0.2` with the `--singleThreaded --stableTypeOrdering` flags
//! the conformance cache generator uses), flags
//! `--strict --lib es2022 --target es2022 --module esnext --moduleResolution bundler`.
//! This is the import-side / module-axis companion to
//! `position_invalid_export_specifier_resolution_tests` (#16495, #16505).

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single source with unresolved-import reporting on, so a specifier
/// naming a nonexistent module reports TS2307 (or TS2882 for a side-effect
/// import) exactly when it is resolved at all.
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

/// A leading `export {};` makes the file an external module without adding any
/// diagnostic of its own; its absence leaves the file a script.
const MODULE: &str = "export {};\n";

// ---------------------------------------------------------------------------
// Import productions, bound but UNUSED, in a top-level block.
//   script -> resolve (TS2307);  module -> suppress (TS1232 alone).
// ---------------------------------------------------------------------------

#[test]
fn named_import_unused_in_top_level_block_resolves_in_a_script() {
    assert_codes(
        r#"{ import { a } from "nonexistent-module"; }"#,
        &[1232, 2307],
        "a script resolves a bound-but-unused top-level-block import",
    );
}

#[test]
fn named_import_unused_in_top_level_block_suppresses_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import {{ a }} from "nonexistent-module"; }}"#),
        &[1232],
        "a module suppresses a bound-but-unused top-level-block import (#16505)",
    );
}

#[test]
fn default_import_unused_in_top_level_block_suppresses_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import a from "nonexistent-module"; }}"#),
        &[1232],
        "the default form takes the same module-axis path",
    );
}

#[test]
fn namespace_import_unused_in_top_level_block_suppresses_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import * as ns from "nonexistent-module"; }}"#),
        &[1232],
        "the namespace form takes the same module-axis path",
    );
}

#[test]
fn type_only_import_unused_in_top_level_block_suppresses_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import type {{ a }} from "nonexistent-module"; }}"#),
        &[1232],
        "a type-only import binds a name and takes the same path",
    );
}

#[test]
fn if_body_and_loop_body_match_the_bare_block_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}if (1) {{ import {{ a }} from "nonexistent-module"; }}"#),
        &[1232],
        "an `if` body opens no declaration scope",
    );
    assert_codes(
        &format!(r#"{MODULE}for (;;) {{ import {{ a }} from "nonexistent-module"; }}"#),
        &[1232],
        "a loop body opens no declaration scope",
    );
}

// ---------------------------------------------------------------------------
// Import productions, bound and USED: resolve wherever they sit.
//   A used binding reaches `resolveExternalModuleName` in a module block AND
//   in a function body (the latter was a tsz false negative before this fix).
// ---------------------------------------------------------------------------

#[test]
fn used_named_import_in_top_level_block_resolves_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import {{ a }} from "nonexistent-module"; a; }}"#),
        &[1232, 2307],
        "a used binding resolves even in a module top-level block",
    );
}

#[test]
fn used_named_import_in_a_function_body_resolves() {
    assert_codes(
        &format!(r#"{MODULE}function f() {{ import {{ a }} from "nonexistent-module"; a; }}"#),
        &[1232, 2307],
        "a used binding resolves inside a function body — markAliasReferenced, \
         not position, is the discriminator",
    );
}

#[test]
fn unused_named_import_in_a_function_body_suppresses() {
    assert_codes(
        &format!(r#"{MODULE}function f() {{ import {{ a }} from "nonexistent-module"; }}"#),
        &[1232],
        "an unused binding in a declaration scope suppresses",
    );
}

// Every clause-bearing form shares the one `markAliasReferenced` axis — a used
// default or namespace binding in a module block resolves exactly like the
// named form, and there is no separate module-ness gate that would re-suppress
// it (#16522). These lock the single-gate invariant across clause kinds.

#[test]
fn used_default_import_in_top_level_block_resolves_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import a from "nonexistent-module"; a; }}"#),
        &[1232, 2307],
        "a used default binding resolves in a module top-level block",
    );
}

#[test]
fn used_namespace_import_in_top_level_block_resolves_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import * as ns from "nonexistent-module"; ns; }}"#),
        &[1232, 2307],
        "a used namespace binding resolves in a module top-level block",
    );
}

#[test]
fn used_import_equals_in_a_function_body_resolves_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}function f() {{ import x = require("nonexistent-module"); x; }}"#),
        &[1232, 2307],
        "a used `import =` alias resolves inside a function body in a module — \
         markAliasReferenced, not position or module-ness, is the discriminator",
    );
}

// ---------------------------------------------------------------------------
// `import x = require(...)`: same markAliasReferenced rule.
// ---------------------------------------------------------------------------

#[test]
fn import_equals_unused_in_top_level_block_resolves_in_a_script() {
    assert_codes(
        r#"{ import x = require("nonexistent-module"); }"#,
        &[1232, 2307],
        "a script resolves a bound-but-unused `import =` in a top-level block",
    );
}

#[test]
fn import_equals_unused_in_top_level_block_suppresses_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import x = require("nonexistent-module"); }}"#),
        &[1232],
        "a module suppresses a bound-but-unused `import =` in a top-level block",
    );
}

#[test]
fn used_import_equals_in_top_level_block_resolves_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ import x = require("nonexistent-module"); x; }}"#),
        &[1232, 2307],
        "a used `import =` resolves in a module top-level block",
    );
}

// ---------------------------------------------------------------------------
// Colliding `import x = ...` aliases (#16527 item 2): a group resolves at most
// one specifier — the first declaration by source position — and the module
// axis still gates the first. In a module there is no top-level-block
// auto-resolve, so an unused colliding group resolves nothing; a referenced one
// resolves only its first. Measured against the pinned oracle.
// ---------------------------------------------------------------------------

#[test]
fn colliding_import_equals_unused_in_top_level_block_resolves_none_in_a_module() {
    assert_codes(
        &format!(
            r#"{MODULE}{{ import x = require("nonexistent-a"); import x = require("nonexistent-b"); }}"#
        ),
        &[1232, 1232, 2300, 2300],
        "a module has no top-level-block auto-resolve, so an unused colliding group is silent",
    );
}

#[test]
fn colliding_import_equals_used_in_top_level_block_resolves_only_first_in_a_module() {
    assert_codes(
        &format!(
            r#"{MODULE}{{ import x = require("nonexistent-a"); import x = require("nonexistent-b"); x; }}"#
        ),
        &[1232, 1232, 2300, 2300, 2307],
        "a referenced colliding group resolves only its first declaration in a module too",
    );
}

// ---------------------------------------------------------------------------
// Side-effect `import "m"`: binds no name, so it never resolves in a wrong
// context (script or module) — only at a valid position.
// ---------------------------------------------------------------------------

#[test]
fn side_effect_import_in_top_level_block_suppresses_in_both_kinds() {
    assert_codes(
        r#"{ import "nonexistent-module"; }"#,
        &[1232],
        "a script side-effect import in a block binds nothing to mark referenced",
    );
    assert_codes(
        &format!(r#"{MODULE}{{ import "nonexistent-module"; }}"#),
        &[1232],
        "a module side-effect import in a block likewise suppresses",
    );
}

#[test]
fn side_effect_import_at_a_valid_position_still_reports_ts2882() {
    assert_codes(
        r#"import "nonexistent-module";"#,
        &[2882],
        "a valid-position side-effect import resolves and reports TS2882 — the \
         falsifying control for an over-broad suppression",
    );
}

// ---------------------------------------------------------------------------
// Export productions in a top-level block: the inversion.
//   export { } from -> resolve in a script, suppress in a module.
//   export * from   -> suppress in a script, resolve in a module.
//   export * as ns  -> never.
// ---------------------------------------------------------------------------

#[test]
fn export_named_from_in_top_level_block_resolves_in_a_script() {
    assert_codes(
        r#"{ export { a } from "nonexistent-module"; }"#,
        &[1233, 2307],
        "a script resolves `export { } from` in a top-level block",
    );
}

#[test]
fn export_named_from_in_top_level_block_suppresses_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ export {{ a }} from "nonexistent-module"; }}"#),
        &[1233],
        "a module suppresses `export { } from` in a top-level block (#16495)",
    );
}

#[test]
fn export_star_from_in_top_level_block_resolves_in_a_module() {
    assert_codes(
        &format!(r#"{MODULE}{{ export * from "nonexistent-module"; }}"#),
        &[1233, 2307],
        "a module resolves `export * from` in a top-level block — the inversion",
    );
}

#[test]
fn export_star_from_in_top_level_block_suppresses_in_a_script() {
    assert_codes(
        r#"{ export * from "nonexistent-module"; }"#,
        &[1233],
        "a script suppresses `export * from` in a top-level block (#16495)",
    );
}

#[test]
fn export_star_as_ns_in_top_level_block_never_resolves() {
    assert_codes(
        r#"{ export * as ns from "nonexistent-module"; }"#,
        &[1233],
        "`export * as ns from` never resolves at a position-invalid site (script)",
    );
    assert_codes(
        &format!(r#"{MODULE}{{ export * as ns from "nonexistent-module"; }}"#),
        &[1233],
        "`export * as ns from` never resolves at a position-invalid site (module)",
    );
}

// ---------------------------------------------------------------------------
// Anti-hardcoding: the rule is structural. Renaming every user-chosen binder
// and specifier must not change the verdict.
// ---------------------------------------------------------------------------

#[test]
fn renamed_binders_do_not_change_the_module_axis_verdict() {
    assert_codes(
        &format!(r#"{MODULE}{{ import {{ qqzz }} from "no-such-package-here"; }}"#),
        &[1232],
        "no identifier or specifier text participates in the module-axis rule",
    );
    assert_codes(
        &format!(r#"{MODULE}{{ import {{ qqzz }} from "no-such-package-here"; qqzz; }}"#),
        &[1232, 2307],
        "the reference discriminator is symbol identity, not the chosen name",
    );
}
