use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ResolutionModeOverride};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn parse_and_bind(
    file_name: &str,
    source: &str,
) -> (
    Arc<tsz_parser::parser::NodeArena>,
    Arc<BinderState>,
    tsz_parser::parser::NodeIndex,
) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    (Arc::new(parser.get_arena().clone()), Arc::new(binder), root)
}

fn check_node16_resolution_mode(
    source: &str,
    default_target_idx: usize,
    file_is_esm: Option<bool>,
) -> Vec<Diagnostic> {
    let (arena_main, binder_main, root_main) = parse_and_bind("main.ts", source);
    let (arena_import, binder_import, _) =
        parse_and_bind("pkg-import.ts", "export interface ImportInterface {}");
    let (arena_require, binder_require, _) =
        parse_and_bind("pkg-require.ts", "export interface RequireInterface {}");

    let all_arenas = Arc::new(vec![
        Arc::clone(&arena_main),
        Arc::clone(&arena_import),
        Arc::clone(&arena_require),
    ]);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_main),
        Arc::clone(&binder_import),
        Arc::clone(&binder_require),
    ]);

    let mut resolved_module_paths = FxHashMap::default();
    resolved_module_paths.insert((0usize, "pkg".to_string()), default_target_idx);

    let mut resolved_module_request_paths = FxHashMap::default();
    resolved_module_request_paths.insert(
        (
            0usize,
            "pkg".to_string(),
            Some(ResolutionModeOverride::Import),
        ),
        1usize,
    );
    resolved_module_request_paths.insert(
        (
            0usize,
            "pkg".to_string(),
            Some(ResolutionModeOverride::Require),
        ),
        2usize,
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "main.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::Node16,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker.ctx.file_is_esm = file_is_esm;
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker
        .ctx
        .set_resolved_module_request_paths(Arc::new(resolved_module_request_paths));
    checker
        .ctx
        .set_resolved_modules(FxHashSet::from_iter(["pkg".to_string()]));
    checker.ctx.report_unresolved_imports = true;

    assert_eq!(
        checker.ctx.resolve_import_target_from_file_with_mode(
            0,
            "pkg",
            Some(ResolutionModeOverride::Import),
        ),
        Some(1)
    );
    assert_eq!(
        checker.ctx.resolve_import_target_from_file_with_mode(
            0,
            "pkg",
            Some(ResolutionModeOverride::Require),
        ),
        Some(2)
    );

    checker.check_source_file(root_main);
    checker.ctx.diagnostics.clone()
}

#[test]
fn node16_import_type_resolution_mode_stays_active() {
    let diagnostics = check_node16_resolution_mode(
        r#"
import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
"#,
        1,
        Some(false),
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2305 && d.code != 2823),
        "Expected no TS2305/TS2823 for a valid type-only import resolution-mode, got: {diagnostics:?}"
    );
}

#[test]
fn node16_invalid_type_only_resolution_mode_reports_grammar_error() {
    let diagnostics = check_node16_resolution_mode(
        r#"
import type { RequireInterface } from "pkg" with { "resolution-mode": "foobar" };
"#,
        2,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1453),
        "Expected TS1453 for an invalid type-only resolution-mode, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2823),
        "Expected TS2823 alongside TS1453 under node16, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2305),
        "Did not expect TS2305 when the default route still resolves RequireInterface, got: {diagnostics:?}"
    );
}

#[test]
fn node16_inline_type_specifier_resolution_mode_falls_back_to_default_route() {
    let diagnostics = check_node16_resolution_mode(
        r#"import { type ImportInterface as Imp } from "pkg" with { "resolution-mode": "import" };"#,
        1,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2305),
        "Expected TS2305 when node16 ignores inline import resolution-mode, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2823),
        "Expected TS2823 for inline import attributes under node16, got: {diagnostics:?}"
    );
}

#[test]
fn node16_inline_type_specifier_ignores_plain_resolver_branch_for_cjs_files() {
    let diagnostics = check_node16_resolution_mode(
        r#"import { type ImportInterface as Imp } from "pkg" with { "resolution-mode": "import" };"#,
        1,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2305),
        "Expected TS2305 even when the plain resolver points at the import branch, got: {diagnostics:?}"
    );
}

#[test]
fn node16_inline_type_specifier_ignores_plain_resolver_branch_for_esm_files() {
    let diagnostics = check_node16_resolution_mode(
        r#"import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };"#,
        2,
        Some(true),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2305),
        "Expected TS2305 when node16 falls back to ESM resolution for inline type specifiers, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2459),
        "Expected no TS2459 when the opposite branch only exports the symbol, got: {diagnostics:?}"
    );
}
