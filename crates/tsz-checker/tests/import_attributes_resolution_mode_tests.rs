use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ResolutionModeOverride, ResolutionRequestKind};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

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

/// Like `check_json_module_import`, but the resolved target is a plain `.ts`
/// module rather than `.json`. The JSON-module resolution path has its own,
/// pre-existing (and separately trackable) double-resolution quirk for type
/// references — unrelated to the file-wide TS2880 suppression fact this
/// helper's callers exercise — so tests that need a clean single-resolution
/// baseline use this instead of `check_json_module_import`.
fn check_ts_module_import(
    main_file_name: &str,
    source: &str,
    module: ModuleKind,
    file_is_esm: Option<bool>,
) -> Vec<Diagnostic> {
    let (arena_main, binder_main, root_main) = parse_and_bind(main_file_name, source);
    let (arena_target, binder_target, _) = parse_and_bind("mod.ts", "export interface Shape {}");

    let all_arenas = Arc::new(vec![Arc::clone(&arena_main), Arc::clone(&arena_target)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_main), Arc::clone(&binder_target)]);

    let mut resolved_module_paths = FxHashMap::default();
    resolved_module_paths.insert((0usize, "./mod".to_string()), 1usize);

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
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker
        .ctx
        .set_resolved_modules(FxHashSet::from_iter(["./mod".to_string()]));
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

/// Regression for Devin 🔴 on PR #2644: when `has_default_binding` was
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

/// A type-only import that *also* compiles to a CommonJS `require` reports the
/// CommonJS-incompatibility error, not the type-only error.
///
/// `tsc`'s `checkImportAttributes` checks the CommonJS condition *before* the
/// type-only condition, so for `import type value from "pkg" with { ... }` in a
/// `.cts` file it emits TS2856 ("Import attributes are not allowed on statements
/// that compile to CommonJS 'require' calls"), never TS2857. Verified against
/// `tsc` 6.0.2. (A prior revision asserted the opposite ordering, which did not
/// match `tsc`.)
#[test]
fn cjs_type_only_import_with_attributes_reports_ts2856_not_ts2857() {
    let diagnostics = check_resolution_mode(
        "main.cts",
        r#"import type value from "pkg" with { type: "json" };"#,
        1,
        ModuleKind::Node18,
        Some(false),
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2856),
        "Expected TS2856 for type-only import attributes in a CJS file (CommonJS check runs before the type-only check), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2857),
        "Did not expect TS2857: the CommonJS-incompatibility error takes precedence, got: {diagnostics:?}"
    );
}

/// Broad parity matrix for import-attribute grammar diagnostics.
///
/// Each row was verified against `tsc` 6.0.2 (`--strict --module <m>
/// --moduleResolution nodenext`); the `node16`/`node18` `assert` rows were
/// re-verified against the repo's currently pinned `typescript@7.0.2` for
/// #17203 and updated where the two versions disagree (7.0 hardened `assert`
/// into an unconditional, fully-suppressing TS2880 at every module kind —
/// 6.0.2's node18 carve-out no longer holds). The expectation is the exact
/// set of import-attribute grammar codes `tsc` emits for that combination of
/// module option, attribute keyword (`with`/`assert`), type-only-ness and
/// emit kind (`.mts` = ESM, `.cts` = CommonJS).
///
/// The ordering that this exercises (and that a prior implementation got
/// wrong): module-support (TS2823/TS2821) → assert deprecation (TS2880) →
/// CommonJS-incompatibility (TS2856/TS2836) → type-only (TS2857/TS2822), with
/// each step suppressing later ones. `assert` is a hard, fully-suppressing
/// error (TS2880 alone) at every module kind under 7.0.2 — there is no
/// longer a "non-fatal `assert` warning under `node18`" carve-out.
#[test]
fn import_attribute_grammar_matrix_matches_tsc() {
    // Import-attribute grammar codes we assert on; all other diagnostics
    // (module resolution, etc.) are ignored so the matrix stays focused.
    const ATTR_CODES: [u32; 7] = [2821, 2822, 2823, 2836, 2856, 2857, 2880];

    // (module, keyword, type_only, esm, expected_codes)
    type GrammarCase = (ModuleKind, &'static str, bool, bool, &'static [u32]);
    let cases: &[GrammarCase] = &[
        // node16: module does not support import attributes at all.
        (ModuleKind::Node16, "with", false, true, &[2823]),
        (ModuleKind::Node16, "with", true, false, &[2823]),
        // Oracle-confirmed (typescript@7.0.2, re-verified for #17203): the
        // removed `assert` keyword (TS2880) takes precedence over the
        // module-support gate here too, exactly as it does for `node18`/
        // `node20`/`nodenext` below — Node16 does not carve out an exception.
        // This previously pinned [2821], which tsc never emits for `assert`
        // regardless of module kind; that was a stale expectation, not a
        // regression.
        (ModuleKind::Node16, "assert", false, true, &[2880]),
        (ModuleKind::Node16, "assert", true, false, &[2880]),
        // node18: supported for `with`. Oracle-confirmed (typescript@7.0.2,
        // re-verified for #17203): `assert` is TS2880 ALONE at every
        // type-only/esm combination here too — it is a hard error, not a
        // non-fatal warning, and it suppresses the CommonJS (TS2836) and
        // type-only (TS2822) checks below just like it does for node20/
        // nodenext. The three rows below previously paired TS2880 with a
        // second code that tsc never emits alongside it; those were stale
        // expectations, not regressions.
        (ModuleKind::Node18, "with", false, true, &[]),
        (ModuleKind::Node18, "with", false, false, &[2856]),
        (ModuleKind::Node18, "with", true, true, &[2857]),
        (ModuleKind::Node18, "with", true, false, &[2856]),
        (ModuleKind::Node18, "assert", false, true, &[2880]),
        (ModuleKind::Node18, "assert", false, false, &[2880]),
        (ModuleKind::Node18, "assert", true, true, &[2880]),
        (ModuleKind::Node18, "assert", true, false, &[2880]),
        // node20 / nodenext: `assert` is a hard error (TS2880) that suppresses
        // the CommonJS and type-only checks.
        (ModuleKind::Node20, "with", false, true, &[]),
        (ModuleKind::Node20, "with", false, false, &[2856]),
        (ModuleKind::Node20, "with", true, true, &[2857]),
        (ModuleKind::Node20, "with", true, false, &[2856]),
        (ModuleKind::Node20, "assert", false, false, &[2880]),
        (ModuleKind::Node20, "assert", true, true, &[2880]),
        (ModuleKind::Node20, "assert", true, false, &[2880]),
        (ModuleKind::NodeNext, "with", true, false, &[2856]),
        (ModuleKind::NodeNext, "with", true, true, &[2857]),
        (ModuleKind::NodeNext, "assert", true, false, &[2880]),
        (ModuleKind::NodeNext, "assert", false, false, &[2880]),
    ];

    for &(module, keyword, type_only, esm, expected) in cases {
        let file_name = if esm { "main.mts" } else { "main.cts" };
        let type_prefix = if type_only { "type " } else { "" };
        let source =
            format!("import {type_prefix}value from \"pkg\" {keyword} {{ type: \"json\" }};");

        let diagnostics = check_resolution_mode(file_name, &source, 1, module, Some(esm));

        let mut got: Vec<u32> = diagnostics
            .iter()
            .map(|d| d.code)
            .filter(|c| ATTR_CODES.contains(c))
            .collect();
        got.sort_unstable();
        got.dedup();

        let mut want: Vec<u32> = expected.to_vec();
        want.sort_unstable();

        assert_eq!(
            got, want,
            "module={module:?} keyword={keyword} type_only={type_only} esm={esm}: \
             expected import-attribute codes {want:?}, got {got:?} (all: {diagnostics:?})"
        );
    }
}

/// TS2880 for an *import type expression* (`import("mod", { assert: ... })`,
/// as opposed to an import/export declaration's attributes clause) must
/// anchor on the `assert` property name token, matching tsc's
/// `grammarErrorOnFirstToken` on the attributes node — not on the property's
/// value object, which sits several tokens later.
#[test]
fn import_type_expression_deprecated_assert_anchors_at_property_name() {
    let source = r#"type T = import("./config.json", { assert: { type: "json" } });"#;
    let diagnostics = check_json_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    let ts2880 = diagnostics
        .iter()
        .find(|d| d.code == 2880)
        .unwrap_or_else(|| panic!("Expected TS2880, got: {diagnostics:?}"));

    let expected_start = source.find("assert").unwrap() as u32;
    assert_eq!(
        ts2880.start, expected_start,
        "Expected TS2880 anchored at the `assert` property name (not its value object), \
         got start={} in {source:?}",
        ts2880.start
    );
    assert_eq!(
        ts2880.length, 6,
        "Expected TS2880 to span exactly the `assert` keyword"
    );
}

/// Adjacent case: renaming the type alias binder and reordering unrelated
/// object-literal properties around `assert` must not move the anchor.
#[test]
fn import_type_expression_deprecated_assert_anchors_at_property_name_with_sibling_props() {
    let source =
        r#"type ImportedShape = import("./config.json", { with: {}, assert: { type: "json" } });"#;
    let diagnostics = check_json_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    let ts2880 = diagnostics
        .iter()
        .find(|d| d.code == 2880)
        .unwrap_or_else(|| panic!("Expected TS2880, got: {diagnostics:?}"));

    let expected_start = source.rfind("assert").unwrap() as u32;
    assert_eq!(
        ts2880.start, expected_start,
        "Expected TS2880 anchored at the `assert` property name even with sibling properties, \
         got start={} in {source:?}",
        ts2880.start
    );
}

/// Negative control: a bare import type expression that only uses `with`
/// (never `assert`) must not report TS2880 at all.
#[test]
fn import_type_expression_with_keyword_does_not_report_deprecated_assert() {
    let source = r#"type T = import("./config.json", { with: { type: "json" } });"#;
    let diagnostics = check_json_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    assert!(
        diagnostics.iter().all(|d| d.code != 2880),
        "Did not expect TS2880 for an import type expression using `with`, got: {diagnostics:?}"
    );
}

/// TS2880 file-wide dynamic-import suppression (#16220): when a source file
/// contains both a type-position `import(...)` (a type alias here) and a
/// value-position dynamic `import(...)` call, each independently eligible
/// for TS2880's deprecated-`assert` diagnostic, tsc reports it only for the
/// type-position occurrence and suppresses every dynamic-position one in
/// that file. Verified against the pinned `typescript@7.0.2` oracle.
#[test]
fn dynamic_import_assert_suppressed_when_type_position_sibling_exists() {
    let source = r#"
const a = import("./mod", { assert: { "resolution-mode": "import" } });
type T = import("./mod", { assert: { "resolution-mode": "import" } }).Shape;
"#;
    let diagnostics = check_ts_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    let dynamic_assert_start = source.find("assert").unwrap() as u32;
    let type_assert_start = source.rfind("assert").unwrap() as u32;
    let ts2880_starts: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 2880)
        .map(|d| d.start)
        .collect();
    assert!(
        !ts2880_starts.contains(&dynamic_assert_start),
        "Did not expect TS2880 at the dynamic-import `assert` position when a \
         type-position sibling exists in the file, got: {diagnostics:?}"
    );
    assert!(
        ts2880_starts.contains(&type_assert_start),
        "Expected TS2880 to still fire for the type-position `assert`, got: {diagnostics:?}"
    );
}

/// Adjacent case: dynamic import appears BEFORE its type-position sibling in
/// source order — tsc's suppression is order-independent within a file.
#[test]
fn dynamic_import_assert_suppressed_when_type_position_sibling_follows() {
    let source = r#"
type T = import("./mod", { assert: { "resolution-mode": "import" } }).Shape;
const a = import("./mod", { assert: { "resolution-mode": "import" } });
"#;
    let diagnostics = check_ts_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    let type_assert_start = source.find("assert").unwrap() as u32;
    let dynamic_assert_start = source.rfind("assert").unwrap() as u32;
    let ts2880_starts: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 2880)
        .map(|d| d.start)
        .collect();
    assert!(
        !ts2880_starts.contains(&dynamic_assert_start),
        "Did not expect TS2880 at the dynamic-import `assert` position regardless of \
         source order, got: {diagnostics:?}"
    );
    assert!(
        ts2880_starts.contains(&type_assert_start),
        "Expected TS2880 to still fire for the type-position `assert`, got: {diagnostics:?}"
    );
}

/// Negative control: a lone dynamic import with no type-position sibling in
/// the file must still report TS2880 — this is the pre-existing, already
/// correct behavior the suppression fact must not regress.
#[test]
fn dynamic_import_assert_reports_without_type_position_sibling() {
    let source = r#"const a = import("./mod", { assert: { "resolution-mode": "import" } });"#;
    let diagnostics = check_ts_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2880).count(),
        1,
        "Expected TS2880 for a dynamic import with no type-position sibling, got: {diagnostics:?}"
    );
}

/// Negative control: a type-position sibling that uses `with` (not `assert`)
/// is not itself TS2880-eligible, so it must not suppress a genuinely
/// eligible dynamic-import `assert` occurrence.
#[test]
fn dynamic_import_assert_reports_when_type_position_sibling_uses_with() {
    let source = r#"
type T = import("./mod", { with: { "resolution-mode": "import" } }).Shape;
const a = import("./mod", { assert: { "resolution-mode": "import" } });
"#;
    let diagnostics = check_ts_module_import("main.ts", source, ModuleKind::Node18, Some(true));

    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2880).count(),
        1,
        "Expected the dynamic import's TS2880 to survive since the type-position sibling \
         uses `with`, not `assert`, got: {diagnostics:?}"
    );
}
