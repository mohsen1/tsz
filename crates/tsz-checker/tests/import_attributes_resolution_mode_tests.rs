use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ResolutionModeOverride, ResolutionRequestKind};
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
    check_resolution_mode(
        "main.ts",
        source,
        default_target_idx,
        ModuleKind::Node16,
        file_is_esm,
    )
}

fn check_resolution_mode(
    main_file_name: &str,
    source: &str,
    default_target_idx: usize,
    module: ModuleKind,
    file_is_esm: Option<bool>,
) -> Vec<Diagnostic> {
    check_resolution_mode_with_targets(
        main_file_name,
        source,
        default_target_idx,
        module,
        file_is_esm,
        ("pkg-import.ts", "export interface ImportInterface {}"),
        ("pkg-require.ts", "export interface RequireInterface {}"),
    )
}

fn check_resolution_mode_with_targets(
    main_file_name: &str,
    source: &str,
    default_target_idx: usize,
    module: ModuleKind,
    file_is_esm: Option<bool>,
    import_target: (&str, &str),
    require_target: (&str, &str),
) -> Vec<Diagnostic> {
    check_resolution_mode_with_targets_and_file_map(
        main_file_name,
        source,
        default_target_idx,
        module,
        file_is_esm,
        None,
        import_target,
        require_target,
    )
}

fn check_resolution_mode_with_targets_and_file_map(
    main_file_name: &str,
    source: &str,
    default_target_idx: usize,
    module: ModuleKind,
    file_is_esm: Option<bool>,
    file_is_esm_map: Option<FxHashMap<String, bool>>,
    import_target: (&str, &str),
    require_target: (&str, &str),
) -> Vec<Diagnostic> {
    let (arena_main, binder_main, root_main) = parse_and_bind(main_file_name, source);
    let (arena_import, binder_import, _) = parse_and_bind(import_target.0, import_target.1);
    let (arena_require, binder_require, _) = parse_and_bind(require_target.0, require_target.1);

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
            ResolutionRequestKind::EsmImport,
        ),
        1usize,
    );
    resolved_module_request_paths.insert(
        (
            0usize,
            "pkg".to_string(),
            Some(ResolutionModeOverride::Require),
            ResolutionRequestKind::CjsRequire,
        ),
        2usize,
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        main_file_name.to_string(),
        CheckerOptions {
            module,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker.ctx.file_is_esm = file_is_esm;
    checker.ctx.file_is_esm_map = file_is_esm_map.map(Arc::new);
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

fn check_json_module_import(
    main_file_name: &str,
    source: &str,
    module: ModuleKind,
    file_is_esm: Option<bool>,
) -> Vec<Diagnostic> {
    check_json_module_import_with_resolve_json_module(
        main_file_name,
        source,
        module,
        file_is_esm,
        true,
    )
}

fn check_json_module_import_with_resolve_json_module(
    main_file_name: &str,
    source: &str,
    module: ModuleKind,
    file_is_esm: Option<bool>,
    resolve_json_module: bool,
) -> Vec<Diagnostic> {
    let (arena_main, binder_main, root_main) = parse_and_bind(main_file_name, source);
    let (arena_json, binder_json, _) = parse_and_bind("config.json", r#"{ "version": 1 }"#);

    let all_arenas = Arc::new(vec![Arc::clone(&arena_main), Arc::clone(&arena_json)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_main), Arc::clone(&binder_json)]);

    let mut resolved_module_paths = FxHashMap::default();
    resolved_module_paths.insert((0usize, "./config.json".to_string()), 1usize);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        main_file_name.to_string(),
        CheckerOptions {
            module,
            no_lib: true,
            resolve_json_module,
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
        .set_resolved_modules(FxHashSet::from_iter(["./config.json".to_string()]));
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root_main);
    checker.ctx.diagnostics.clone()
}

#[test]
fn preserve_plain_ts_imports_use_import_branch_without_attributes() {
    let diagnostics = check_resolution_mode(
        "main.ts",
        r#"import { ImportInterface, RequireInterface } from "pkg";"#,
        2,
        ModuleKind::Preserve,
        None,
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2305),
        "Expected TS2305 when preserve-mode .ts import stays on the import branch, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("RequireInterface")),
        "Expected the missing export to be RequireInterface from the require-only branch, got: {diagnostics:?}"
    );
}

#[test]
fn preserve_plain_ts_imports_ignore_cjs_file_map_for_es_imports() {
    let diagnostics = check_resolution_mode_with_targets_and_file_map(
        "main.ts",
        r#"import { ImportInterface, RequireInterface } from "pkg";"#,
        2,
        ModuleKind::Preserve,
        Some(false),
        Some(FxHashMap::from_iter([("main.ts".to_string(), false)])),
        ("pkg-import.ts", "export interface ImportInterface {}"),
        ("pkg-require.ts", "export interface RequireInterface {}"),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2305),
        "Expected TS2305 when preserve-mode .ts import keeps using the import branch even if file_is_esm_map marks the file CJS, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message_text.contains("RequireInterface")),
        "Expected the missing export to stay on the require-only symbol, got: {diagnostics:?}"
    );
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
fn node18_type_only_json_import_attribute_reports_ts2857_not_ts1463() {
    let diagnostics = check_resolution_mode(
        "main.mts",
        r#"import type Config from "pkg" with { type: "json" };"#,
        1,
        ModuleKind::Node18,
        Some(true),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2857),
        "Expected TS2857 for type-only import attributes without resolution-mode, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1463),
        "Did not expect TS1463 for type-only JSON import attributes, got: {diagnostics:?}"
    );
}

#[test]
fn node18_cts_import_attributes_report_ts2856() {
    let diagnostics = check_resolution_mode(
        "main.cts",
        r#"import value from "pkg" with { type: "json" };"#,
        1,
        ModuleKind::Node18,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2856),
        "Expected TS2856 for import attributes on a CJS-emitting import, got: {diagnostics:?}"
    );
}

#[test]
fn node18_cts_export_attributes_report_ts2856() {
    let diagnostics = check_resolution_mode(
        "main.cts",
        r#"export { value } from "pkg" with { type: "json" };"#,
        1,
        ModuleKind::Node18,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2856),
        "Expected TS2856 for export attributes on a CJS-emitting export, got: {diagnostics:?}"
    );
}

#[test]
fn node18_cts_type_only_import_from_esm_requires_resolution_mode() {
    let diagnostics = check_resolution_mode_with_targets(
        "main.cts",
        r#"import type { ImportInterface } from "pkg";"#,
        1,
        ModuleKind::Node18,
        Some(false),
        ("pkg-import.mts", "export interface ImportInterface {}"),
        ("pkg-require.mts", "export interface RequireInterface {}"),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1541),
        "Expected TS1541 for type-only import from ESM in a CJS file, got: {diagnostics:?}"
    );
}

#[test]
fn node18_cts_typeof_import_from_esm_requires_resolution_mode() {
    let diagnostics = check_resolution_mode_with_targets(
        "main.cts",
        r#"type T = typeof import("pkg");"#,
        1,
        ModuleKind::Node18,
        Some(false),
        ("pkg-import.mts", "export const value = 1;"),
        ("pkg-require.mts", "export const value = 1;"),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1542),
        "Expected TS1542 for typeof import from ESM in a CJS file, got: {diagnostics:?}"
    );
}

#[test]
fn node18_cts_type_import_with_resolution_mode_suppresses_ts1542() {
    let diagnostics = check_resolution_mode_with_targets(
        "main.cts",
        r#"type T = typeof import("pkg", { with: { "resolution-mode": "import" } });"#,
        1,
        ModuleKind::Node18,
        Some(false),
        ("pkg-import.mts", "export const value = 1;"),
        ("pkg-require.cts", "export const value = 1;"),
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 1542),
        "Did not expect TS1542 when resolution-mode is present, got: {diagnostics:?}"
    );
}

#[test]
fn node18_esm_default_json_import_without_attribute_reports_ts1543() {
    let diagnostics = check_json_module_import(
        "main.mts",
        r#"import config from "./config.json";"#,
        ModuleKind::Node18,
        Some(true),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1543),
        "Expected TS1543 for ESM JSON default import without type=json, got: {diagnostics:?}"
    );
}

#[test]
fn node18_esm_namespace_json_import_without_attribute_reports_ts1543() {
    let diagnostics = check_json_module_import(
        "main.mts",
        r#"import * as config from "./config.json";"#,
        ModuleKind::Node18,
        Some(true),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1543),
        "Expected TS1543 for ESM JSON namespace import without type=json, got: {diagnostics:?}"
    );
}

#[test]
fn node18_esm_named_json_import_reports_ts1544_not_ts2614() {
    let diagnostics = check_json_module_import(
        "main.mts",
        r#"import { version } from "./config.json" with { type: "json" };"#,
        ModuleKind::Node18,
        Some(true),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 1544),
        "Expected TS1544 for ESM named import from JSON, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2614),
        "Did not expect TS2614 for ESM named import from JSON, got: {diagnostics:?}"
    );
}

#[test]
fn nodenext_esm_json_type_attribute_without_resolve_json_module_does_not_emit_module_errors() {
    let diagnostics = check_json_module_import_with_resolve_json_module(
        "main.mts",
        r#"import config from "./config.json" with { type: "json" };"#,
        ModuleKind::NodeNext,
        Some(true),
        false,
    );

    assert!(
        diagnostics
            .iter()
            .all(|d| !matches!(d.code, 1192 | 2306 | 2732)),
        "Did not expect JSON module/default diagnostics for a NodeNext ESM import with type=json, got: {diagnostics:?}"
    );
}

#[test]
fn node18_esm_json_namespace_property_message_expands_default_shape() {
    let diagnostics = check_json_module_import(
        "main.mts",
        r#"
import * as config from "./config.json" with { type: "json" };
config.version;
"#,
        ModuleKind::Node18,
        Some(true),
    );

    let ts2339 = diagnostics
        .iter()
        .find(|d| d.code == 2339)
        .expect("expected TS2339 for named property on ESM JSON namespace");
    assert!(
        ts2339
            .message_text
            .contains("{ default: { version: number; }; }"),
        "Expected JSON namespace diagnostic to print the synthesized default object shape, got: {ts2339:?}"
    );
    assert!(
        !ts2339.message_text.contains("typeof import"),
        "Expected JSON namespace diagnostic not to use typeof import display, got: {ts2339:?}"
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

/// Regression: when an `import type` whole-declaration uses a `resolution-mode`
/// override that resolves the name in the alternate branch, the alias's
/// type-resolution path must honor the override. Otherwise the generic
/// "no exported member" emitter fires a duplicate (or false-positive) TS2305
/// anchored at the `IMPORT_SPECIFIER`, even though `check_imported_members`
/// (the canonical syntactic site) correctly suppressed the diagnostic.
///
/// The aliases must be USED in the source so the type-resolver actually runs;
/// without a use site the bug doesn't reproduce because alias types are
/// computed lazily.
#[test]
fn node16_import_type_resolution_mode_alias_use_does_not_emit_ts2305() {
    let diagnostics = check_node16_resolution_mode(
        r#"
import type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
import type { ImportInterface } from "pkg" with { "resolution-mode": "import" };

export interface Local extends RequireInterface, ImportInterface {}
"#,
        1, // default route is `pkg-import.ts` (only ImportInterface)
        Some(true),
    );

    let ts2305: Vec<_> = diagnostics.iter().filter(|d| d.code == 2305).collect();
    assert!(
        ts2305.is_empty(),
        "Expected no TS2305 when whole-declaration `import type` resolution-mode overrides resolve the name in the alternate branch, got: {ts2305:?}"
    );
}

/// Regression: an inline-type-only specifier (`import {type X as Y}`) does
/// NOT have an effective resolution-mode override under node16, so
/// `check_imported_members` rightly emits TS2305 at the imported identifier
/// when the default branch lacks the symbol. The alias type-resolution path
/// must NOT emit a *second* TS2305 anchored at the `IMPORT_SPECIFIER` node
/// (which would wrap the `type` keyword as well as the identifier).
#[test]
fn node16_inline_type_specifier_emits_single_ts2305_per_missing_name() {
    let diagnostics = check_node16_resolution_mode(
        r#"
import { type RequireInterface as Req } from "pkg" with { "resolution-mode": "require" };

export interface Local extends Req {}
"#,
        1, // default route is `pkg-import.ts` (no RequireInterface)
        Some(true),
    );

    let ts2305: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2305 && d.message_text.contains("RequireInterface"))
        .collect();
    assert_eq!(
        ts2305.len(),
        1,
        "Expected exactly one TS2305 for the missing `RequireInterface` from the inline-type specifier (the canonical syntactic anchor); duplicates from the alias type-resolver indicate a regression. Got: {ts2305:?}"
    );
}

/// Regression case: when `has_default_binding` was
/// computed from `json_default_only` (which is gated on
/// `current_file_uses_esm_import_syntax()`), CommonJS files importing a JSON
/// module by default emitted a spurious TS1192 "no default export" error.
/// `has_default_binding` must remain anchored on `has_json_default_export`
/// regardless of whether the importing file uses ESM syntax.
#[test]
fn cjs_json_default_import_does_not_emit_ts1192() {
    let diagnostics = check_json_module_import(
        "main.cts",
        r#"import config from "./config.json";"#,
        ModuleKind::Node18,
        Some(false),
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 1192),
        "Did not expect TS1192 for CJS JSON default import, got: {diagnostics:?}"
    );
}

/// Regression case: type-only imports in CJS files
/// must emit TS2857 ("Import attributes cannot be used with type-only
/// imports or exports") rather than TS2856 ("Import attributes are not
/// allowed on statements that compile to CommonJS 'require' calls"),
/// because type-only imports are erased at compile time and never produce
/// `require()` calls. The type-only check must run before the CJS check.
#[test]
fn cjs_type_only_import_with_attributes_reports_ts2857_not_ts2856() {
    let diagnostics = check_resolution_mode(
        "main.cts",
        r#"import type value from "pkg" with { type: "json" };"#,
        1,
        ModuleKind::Node18,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2857),
        "Expected TS2857 for type-only import attributes in a CJS file, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2856),
        "Did not expect TS2856 for type-only import attributes in a CJS file (type-only imports never compile to require), got: {diagnostics:?}"
    );
}
