use super::{Server, TsServerRequest};
use crate::{CheckOptions, LogConfig, LogLevel, ServerMode};
use rustc_hash::FxHashMap;
use std::path::PathBuf;

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

/// Verify tsz generates TS2304 for `useMemo` even when its declaration file
/// is in `open_files`. The `.d.ts` is a module (has top-level exports), so
/// `useMemo` is NOT in global scope — `app.tsx` still needs an import.
#[test]
fn semantic_diagnostics_ts2304_for_usememo_with_dts_in_open_files() {
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

    let diagnostics = server.get_semantic_diagnostics_full(app_tsx, app_content);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "expected TS2304 for 'useMemo', got diagnostic codes: {codes:?}"
    );
}

/// Sanity check: without the `.d.ts` file, TS2304 IS generated for `useMemo`.
#[test]
fn semantic_diagnostics_ts2304_for_usememo_without_dts() {
    let mut server = make_server();

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

/// Regression test for `importFixesWithPackageJsonInSideAnotherPackage`:
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
