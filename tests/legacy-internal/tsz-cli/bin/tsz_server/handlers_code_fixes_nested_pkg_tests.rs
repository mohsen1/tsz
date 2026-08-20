use super::{Server, TsServerRequest};
use crate::{CheckOptions, LogConfig, LogLevel, ServerMode};
use rustc_hash::FxHashMap;
use std::path::PathBuf;

/// Returns a server with the real TypeScript lib files wired up, or `None` if the
/// lib directory cannot be discovered in this environment (e.g. no TypeScript install).
/// Tests that depend on accurate checker output should call this and skip via `let
/// Some(server) = make_server_with_real_libs() else { return; }`.
fn make_server_with_real_libs() -> Option<Server> {
    let lib_dir = Server::find_lib_dir().ok()?;
    let tests_lib_dir = Server::find_tests_lib_dir(&lib_dir);
    let mut server = make_server();
    server.lib_dir = lib_dir;
    server.tests_lib_dir = tests_lib_dir;
    Some(server)
}

fn make_server() -> Server {
    Server {
        completion_import_module_specifier_ending: None,
        import_module_specifier_preference: None,
        organize_imports_type_order: None,
        organize_imports_ignore_case: false,
        auto_import_file_exclude_patterns: Vec::new(),
        lib_dir: PathBuf::from("/nonexistent"),
        tests_lib_dir: PathBuf::from("/nonexistent"),
        lib_cache: FxHashMap::default(),
        unified_lib_cache: None,
        checks_completed: 0,
        response_seq: 0,
        open_files: FxHashMap::default(),
        external_project_files: FxHashMap::default(),
        completion_project_cache: None,
        project_cache: std::cell::RefCell::new(None),
        parse_bind_cache: std::cell::RefCell::new(crate::ParseBindCache::default()),
        _server_mode: ServerMode::Semantic,
        _log_config: LogConfig {
            level: LogLevel::Off,
            file: None,
            trace_to_console: false,
        },
        enable_telemetry: false,
        allow_importing_ts_extensions: false,
        inferred_check_options: CheckOptions::default(),
        inferred_projectinfo_options: None,
        auto_imports_allowed_for_inferred_projects: true,
        inferred_module_is_none_for_projects: false,
        auto_import_specifier_exclude_regexes: Vec::new(),
        include_completions_with_class_member_snippets: false,
        include_inlay_parameter_name_hints: None,
        generate_return_in_doc_template: None,
        new_line_character: None,
        plugin_configs: FxHashMap::default(),
        native_ts_worker: None,
        pending_events: Vec::new(),
    }
}

/// Register `app` plus one sibling `.d.ts` in a real-libs server and return the
/// semantic diagnostic codes reported for `app`. Returns `None` when the
/// TypeScript lib directory is not discoverable (caller should skip).
fn dts_sibling_codes(app: &str, app_src: &str, dts: &str, dts_src: &str) -> Option<Vec<u32>> {
    let mut server = make_server_with_real_libs()?;
    server
        .open_files
        .insert(app.to_string(), app_src.to_string());
    server
        .open_files
        .insert(dts.to_string(), dts_src.to_string());
    Some(
        server
            .get_semantic_diagnostics_full(app, app_src)
            .iter()
            .map(|d| d.code)
            .collect(),
    )
}

/// Verify tsz generates TS2304 for `useMemo` even when its declaration file
/// is in `open_files`. The `.d.ts` is a module (has top-level exports), so
/// `useMemo` is NOT in global scope — `app.tsx` still needs an import.
///
/// A module's top-level exports stay file-scoped: they are reachable from a
/// sibling file only through an explicit `import`. The shared
/// `Symbol::is_cross_file_global` predicate (issue #12372) governs both the
/// merged `program.globals` seeding and the checker's cross-file
/// `global_file_locals_index`, so an installed-but-unimported package export
/// never leaks as a global. See the `module_dts_*` / `script_dts_*` matrix
/// below for the full module-vs-script contract.
#[test]
fn semantic_diagnostics_ts2304_for_usememo_with_dts_in_open_files() {
    let Some(mut server) = make_server_with_real_libs() else {
        return; // skip when TypeScript lib files are not discoverable
    };

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useEffect(effect: () => void): void;\nexport declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
    );

    let diagnostics = server.get_semantic_diagnostics_full(app_tsx, app_content);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "expected TS2304 for 'useMemo', got diagnostic codes: {codes:?}"
    );
}

/// Module `.d.ts` contract (name-agnostic): a sibling declaration file that is
/// an external module — here because it carries a top-level `export declare` —
/// keeps its exports file-scoped. An unqualified reference from another file is
/// `TS2304`; the export is reachable only through an explicit `import`.
/// Mirrors `tsc`: `export declare function widgetInit()` in a module `.d.ts`
/// does not seed the global scope.
#[test]
fn module_dts_export_declare_does_not_leak_into_global_scope() {
    // Sibling module `.d.ts` (module by virtue of the top-level `export`).
    let Some(codes) = dts_sibling_codes(
        "/project/consumer.ts",
        "widgetInit();",
        "/project/widget.d.ts",
        "export declare function widgetInit(): void;\n",
    ) else {
        return; // skip when TypeScript lib files are not discoverable
    };
    assert!(
        codes.contains(&2304),
        "module `.d.ts` export must stay file-scoped (expected TS2304), got: {codes:?}"
    );
}

/// Module determination drives global-scope contribution for *every* top-level
/// declaration, not just the exported one: a bare `export {}` marker turns the
/// file into a module, so even a non-exported ambient `declare function` in it
/// is file-scoped and unreachable as a global. Matches `tsc` (`TS2304`).
#[test]
fn module_dts_via_export_marker_keeps_ambient_declare_file_scoped() {
    // `export {}` makes this `.d.ts` a module even though `moduleOnlyFn` itself
    // is not exported — so it must not land in the ambient global scope.
    let Some(codes) = dts_sibling_codes(
        "/project/site.ts",
        "moduleOnlyFn();",
        "/project/ambient.d.ts",
        "declare function moduleOnlyFn(): void;\nexport {};\n",
    ) else {
        return; // skip when TypeScript lib files are not discoverable
    };
    assert!(
        codes.contains(&2304),
        "ambient declare in a module `.d.ts` must stay file-scoped (expected TS2304), got: {codes:?}"
    );
}

/// Positive control (do not over-correct): a *script* `.d.ts` — no top-level
/// `import`/`export`, so not an external module — contributes its top-level
/// ambient declarations to the global scope. A sibling reference resolves with
/// no import and no `TS2304`, exactly as `tsc` accepts it.
#[test]
fn script_dts_ambient_declare_is_globally_visible() {
    // No top-level import/export: this `.d.ts` is a script, so its ambient
    // declarations seed the global scope.
    let Some(codes) = dts_sibling_codes(
        "/project/page.ts",
        "scriptGlobalFn();",
        "/project/globals.d.ts",
        "declare function scriptGlobalFn(): void;\n",
    ) else {
        return; // skip when TypeScript lib files are not discoverable
    };
    assert!(
        !codes.contains(&2304),
        "script `.d.ts` ambient declarations must be globally visible (expected no TS2304), got: {codes:?}"
    );
}

/// Sanity check: without the `.d.ts` file, TS2304 IS generated for `useMemo`.
#[test]
fn semantic_diagnostics_ts2304_for_usememo_without_dts() {
    let Some(mut server) = make_server_with_real_libs() else {
        return; // skip when TypeScript lib files are not discoverable
    };

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());

    let diagnostics = server.get_semantic_diagnostics_full(app_tsx, app_content);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "expected TS2304 for 'useMemo', got diagnostic codes: {codes:?}"
    );
}

/// Full-fixture replica of the fourslash `importFixesWithPackageJsonInSideAnotherPackage`
/// test: includes the parent `preact/package.json`, `tsconfig.json`, and
/// `component.tsx` (which already imports from `preact/hooks`).
/// This is the scenario the fourslash runner presents to tsz-server.
#[test]
fn import_fix_with_package_json_in_nested_subpackage_full_fixture() {
    let mut server = make_server();

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());
    // tsconfig.json with jsx settings (mirrors the fourslash fixture)
    server.open_files.insert(
        "/project/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "jsx": "react", "jsxFactory": "h" } }"#.to_string(),
    );
    // component.tsx already imports from preact/hooks
    server.open_files.insert(
        "/project/component.tsx".to_string(),
        r#"import { useEffect } from "preact/hooks";"#.to_string(),
    );
    // parent preact package.json (key difference from the simpler unit test)
    server.open_files.insert(
        "/project/node_modules/preact/package.json".to_string(),
        r#"{ "name": "preact", "version": "10.3.4", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useEffect(effect: () => void): void;\nexport declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": app_tsx,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 1,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");

    let descriptions: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("description").and_then(serde_json::Value::as_str))
        .collect();

    let has_preact_import = descriptions
        .iter()
        .any(|d| d.contains("preact/hooks") && d.contains("useMemo"));

    assert!(
        has_preact_import,
        "expected import fix for 'useMemo' from 'preact/hooks', got: {descriptions:?}"
    );
}

/// Regression test: empty `errorCodes` at position (1,1) should also trigger the
/// `CANNOT_FIND_NAME` fallback and produce an import fix. This mirrors the fourslash
/// harness calling `getCodeFixes` at the start of a file with no specific code
/// (because the marker at line 1, col 0 has no overlapping diagnostic).
#[test]
fn import_fix_with_empty_error_codes_at_start_of_file() {
    let mut server = make_server();

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useEffect(effect: () => void): void;\nexport declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
    );

    // Empty errorCodes — fourslash calls this way when no marker diagnostic overlaps
    // the cursor (position 0). The server should fall back to all CANNOT_FIND_NAME
    // diagnostics in the file and still return the import fix.
    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": app_tsx,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 1,
            "errorCodes": []
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");

    let descriptions: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("description").and_then(serde_json::Value::as_str))
        .collect();

    let has_preact_import = descriptions
        .iter()
        .any(|d| d.contains("preact/hooks") && d.contains("useMemo"));

    assert!(
        has_preact_import,
        "expected import fix for 'useMemo' from 'preact/hooks' with empty errorCodes, got: {descriptions:?}"
    );
}

/// Regression test for `importFixesWithPackageJsonInSideAnotherPackage`:
/// Regression: project has a `package.json` that does NOT list `preact` as a
/// dependency, but `preact/hooks` exists as a directly installed subpackage
/// (with its own `package.json`). The import fix must still suggest `preact/hooks`
/// because the subpackage is directly addressable via its full specifier.
#[test]
fn import_fix_with_subpackage_allowed_despite_absent_parent_in_project_deps() {
    let mut server = make_server();

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());
    // Project package.json lists only "typescript" — NOT "preact".
    server.open_files.insert(
        "/project/package.json".to_string(),
        r#"{ "name": "my-app", "dependencies": { "typescript": "^5.0.0" } }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": app_tsx,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 1,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");

    let descriptions: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("description").and_then(serde_json::Value::as_str))
        .collect();

    let has_preact_import = descriptions
        .iter()
        .any(|d| d.contains("preact/hooks") && d.contains("useMemo"));

    assert!(
        has_preact_import,
        "expected import fix for 'useMemo' from 'preact/hooks' even though project deps omit 'preact', got: {descriptions:?}"
    );
}

/// When a package has a nested subpath `package.json` (e.g. `preact/hooks`) but
/// no parent `package.json` in `open_files`, the import fix should still find
/// the correct module specifier `preact/hooks` for a missing identifier.
#[test]
fn import_fix_with_package_json_in_nested_subpackage() {
    let mut server = make_server();

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useEffect(effect: () => void): void;\nexport declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
    );

    // Request at position (0,0) - start of file - matching the fourslash test's "line 1, col 0" marker
    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": app_tsx,
            "startLine": 1,
            "startOffset": 1,
            "endLine": 1,
            "endOffset": 1,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");

    let descriptions: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("description").and_then(serde_json::Value::as_str))
        .collect();

    let has_preact_import = descriptions
        .iter()
        .any(|d| d.contains("preact/hooks") && d.contains("useMemo"));

    assert!(
        has_preact_import,
        "expected import fix for 'useMemo' from 'preact/hooks', got: {descriptions:?}"
    );
}

/// Mirrors the exact `importFixesWithPackageJsonInSideAnotherPackage` fourslash
/// call pattern: `verify.importFixAtPosition` calls `getCodeFixes` at each
/// diagnostic's own span, not at the file start. `useMemo` starts at byte offset
/// 14 (0-indexed) in `"const state = useMemo(...)"`, so the 1-indexed request is
/// `startOffset: 15, endOffset: 22`.
#[test]
fn import_fix_at_diagnostic_span_not_file_start() {
    let mut server = make_server();

    let app_tsx = "/project/app.tsx";
    let app_content = "const state = useMemo(() => 'Hello', []);";

    server
        .open_files
        .insert(app_tsx.to_string(), app_content.to_string());
    server.open_files.insert(
        "/project/tsconfig.json".to_string(),
        r#"{ "compilerOptions": { "jsx": "react", "jsxFactory": "h" } }"#.to_string(),
    );
    server.open_files.insert(
        "/project/component.tsx".to_string(),
        r#"import { useEffect } from "preact/hooks";"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/package.json".to_string(),
        r#"{ "name": "preact", "version": "10.3.4", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/package.json".to_string(),
        r#"{ "name": "hooks", "version": "0.1.0", "types": "src/index.d.ts" }"#.to_string(),
    );
    server.open_files.insert(
        "/project/node_modules/preact/hooks/src/index.d.ts".to_string(),
        "export declare function useEffect(effect: () => void): void;\nexport declare function useMemo<T>(factory: () => T, inputs: ReadonlyArray<unknown> | undefined): T;\n".to_string(),
    );

    let req = TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: "getCodeFixes".to_string(),
        arguments: serde_json::json!({
            "file": app_tsx,
            "startLine": 1,
            "startOffset": 15,
            "endLine": 1,
            "endOffset": 22,
            "errorCodes": [2304]
        }),
    };

    let resp = server.handle_get_code_fixes(1, &req);
    assert!(resp.success, "expected getCodeFixes to succeed");
    let actions = resp
        .body
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .expect("expected getCodeFixes actions array");

    let descriptions: Vec<&str> = actions
        .iter()
        .filter_map(|a| a.get("description").and_then(serde_json::Value::as_str))
        .collect();

    let has_preact_import = descriptions
        .iter()
        .any(|d| d.contains("preact/hooks") && d.contains("useMemo"));

    assert!(
        has_preact_import,
        "expected import fix for 'useMemo' from 'preact/hooks' at diagnostic span (1,15)-(1,22), got: {descriptions:?}"
    );
}
