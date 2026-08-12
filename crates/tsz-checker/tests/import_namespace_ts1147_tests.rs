//! A plain `import ... from "m"` declaration directly inside a non-ambient
//! namespace reports TS1147 alone — never TS2307/TS2305, and never a
//! co-occurring binder-level diagnostic like TS1214.
//!
//! tsc's `checkExternalImportOrExportDeclaration` reports this at the module
//! specifier and returns `false` immediately when the enclosing module
//! element is a namespace (a `ModuleBlock` whose declaration name is not a
//! string literal). The caller gates everything downstream — module
//! resolution, named-export validation, and even the reserved-word binding
//! check — on that `false`, so TS1147 is the *only* diagnostic. This mirrors
//! the same-shaped TS1194 gate already implemented for `export ... from` in a
//! namespace (`check_export_declaration`).
//!
//! `declare module "m" { ... }` is a distinct, valid module-element context
//! (string-literal ambient module) and is excluded by
//! `is_inside_namespace_declaration` itself; `declare global { ... }` is a
//! separate augmentation form that reports TS2667 instead of TS1147.
//!
//! All expectations were measured against `typescript@7.0.2` with
//! `--strict --lib es2022 --target es2022 --module esnext --moduleResolution
//! bundler`.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single source with unresolved-import reporting on, so a module
/// specifier naming a nonexistent module would report TS2307/TS2305 if it
/// were resolved.
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
// TS1147 alone, across the shapes that would otherwise reach resolution or a
// binder-level check.
// ---------------------------------------------------------------------------

#[test]
fn named_import_in_namespace_reports_ts1147_without_ts2307() {
    assert_codes(
        r#"namespace N {
  import { a } from "nonexistent-module";
}"#,
        &[1147],
        "the primary repro from #16410 item 1",
    );
}

#[test]
fn default_import_in_namespace_reports_ts1147_without_ts2307() {
    assert_codes(
        r#"namespace N {
  import Default from "nonexistent-module";
}"#,
        &[1147],
        "a default import binding takes the same gate",
    );
}

#[test]
fn namespace_star_import_in_namespace_reports_ts1147_without_ts2307() {
    assert_codes(
        r#"namespace N {
  import * as ns from "nonexistent-module";
}"#,
        &[1147],
        "a namespace-form binding takes the same gate",
    );
}

#[test]
fn side_effect_import_in_namespace_reports_ts1147_without_ts2307() {
    assert_codes(
        r#"namespace N {
  import "nonexistent-module";
}"#,
        &[1147],
        "a bare side-effect import still has a module specifier to anchor on",
    );
}

#[test]
fn reserved_word_binding_in_namespace_reports_ts1147_alone() {
    // Oracle-confirmed: `namespace N { import { eval } from "m"; }` under
    // `--strict` is TS1147 alone, not TS1147 + TS1214. The reserved-word
    // check never runs because tsc's checkImportDeclaration returns before
    // reaching it.
    assert_codes(
        r#"namespace N {
  import { eval } from "nonexistent-module";
}"#,
        &[1147],
        "TS1147 suppresses the downstream reserved-word (TS1214) check too",
    );
}

#[test]
fn nested_namespace_import_reports_ts1147() {
    assert_codes(
        r#"namespace Outer {
  namespace Inner {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1147],
        "a doubly-nested namespace is still a namespace context",
    );
}

// ---------------------------------------------------------------------------
// Negative controls. These must keep resolving / reporting normally.
// ---------------------------------------------------------------------------

#[test]
fn import_at_top_level_still_reports_ts2307() {
    assert_codes(
        r#"import { a } from "nonexistent-module";"#,
        &[2307],
        "the control: a well-placed import must still resolve its specifier",
    );
}

#[test]
fn import_inside_ambient_module_still_reports_ts2307() {
    // `declare module "amb"` is a string-literal ambient module, a distinct
    // and valid module-element context — `is_inside_namespace_declaration`
    // excludes it, so resolution still runs.
    assert_codes(
        r#"declare module "amb" {
  import { a } from "nonexistent-module";
}"#,
        &[2307],
        "an ambient module body is a valid context; resolution still runs",
    );
}

#[test]
fn import_nested_in_block_within_namespace_reports_ts1232_not_ts1147() {
    // Oracle-confirmed (typescript@7.0.2, re-verified for #17203): further
    // nested in a block inside the namespace, tsc reports TS1232 ALONE — the
    // namespace-specific TS1147 gate correctly defers to it via
    // `!in_wrong_context` (no TS1147 below), and module resolution is also
    // suppressed after the TS1232 placement error, so no TS2307 either. The
    // residual-TS2307 gap this test used to pin has since been closed
    // elsewhere (`position_invalid_import_resolves_specifier` in
    // `declaration_check_body.rs`); this was a stale expectation, not a
    // regression — tsz now matches tsc exactly.
    assert_codes(
        r#"namespace N {
  if (true) {
    import { a } from "nonexistent-module";
  }
}"#,
        &[1232],
        "a block nested inside the namespace is not a direct module-element context; TS1147 must not also fire, and TS2307 must not fire either once TS1232 has already flagged the placement",
    );
}

#[test]
fn import_inside_declare_global_reports_ts2307_not_ts1147() {
    // `declare global {}` is a distinct augmentation form (not a namespace);
    // tsc reports TS2667 (imports not permitted in module augmentations) AND
    // still resolves the specifier (TS2307), oracle-confirmed. tsz's
    // `is_inside_global_augmentation` guard keeps the namespace-specific
    // TS1147 gate from also firing here (the assertion this test protects),
    // but TS2667 itself is only wired for the `import x = require(...)` form
    // today (`declarations/import/equals.rs`), not this plain `import ...
    // from` form — a distinct, pre-existing gap, pinned rather than hidden.
    assert_codes(
        r#"declare global {
  import { a } from "nonexistent-module";
}
export {};"#,
        &[2307],
        "declare global is not a namespace context for TS1147 purposes",
    );
}

// ---------------------------------------------------------------------------
// Two-file case: the module resolves, but the imported member does not
// exist. tsc still reports TS1147 alone (not TS1147 + TS2305) because
// resolution never runs at all.
// ---------------------------------------------------------------------------

#[test]
fn named_import_of_missing_member_in_namespace_reports_ts1147_without_ts2305() {
    let dep_source = "export const present = 1;\n";
    let entry_source = r#"namespace N {
  import { missing } from "./dep";
}"#;

    let dep_name = "dep.ts";
    let entry_name = "entry.ts";
    let module_specifier = "./dep";

    let mut parser_dep = ParserState::new(dep_name.to_string(), dep_source.to_string());
    let root_dep = parser_dep.parse_source_file();
    let mut binder_dep = BinderState::new();
    binder_dep.bind_source_file(parser_dep.get_arena(), root_dep);

    let mut parser_entry = ParserState::new(entry_name.to_string(), entry_source.to_string());
    let root_entry = parser_entry.parse_source_file();
    let mut binder_entry = BinderState::new();
    binder_entry.bind_source_file(parser_entry.get_arena(), root_entry);

    if let Some(exports) = binder_dep.module_exports.get(dep_name).cloned() {
        Arc::make_mut(&mut binder_entry.module_exports)
            .insert(module_specifier.to_string(), exports);
    }

    let arena_entry = Arc::new(parser_entry.get_arena().clone());
    let all_arenas = Arc::new(vec![
        Arc::new(parser_dep.get_arena().clone()),
        Arc::clone(&arena_entry),
    ]);
    let binder_entry = Arc::new(binder_entry);
    let all_binders = Arc::new(vec![Arc::new(binder_dep), Arc::clone(&binder_entry)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_entry.as_ref(),
        binder_entry.as_ref(),
        &types,
        entry_name.to_string(),
        CheckerOptions {
            module: tsz_common::common::ModuleKind::ESNext,
            target: tsz_common::common::ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.report_unresolved_imports = true;

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, module_specifier.to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_specifier.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_entry);

    let mut codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        vec![1147],
        "a resolvable module with a missing member is still TS1147 alone, not TS1147 + TS2305"
    );
}
