use super::Server;
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

#[test]
fn implement_interface_plan_separates_analysis_from_rendering() {
    let server = make_server();
    let content = "interface I { value: string; }\nclass C implements I {}\n";

    let plan = server
        .implement_interface_plan("/same_file.ts", content)
        .expect("expected implement-interface analysis plan");

    assert_eq!(plan.interface_name, "I");
    assert_eq!(plan.interface_file_path, "/same_file.ts");
    assert_eq!(plan.missing_members.len(), 1);
    assert_eq!(plan.missing_members[0].name(), "value");

    let updated = Server::render_implement_interface_content(content, &plan, &[])
        .expect("expected rendered implement-interface edit");
    assert_eq!(
        updated,
        "interface I { value: string; }\nclass C implements I {\n    value: string;\n}\n"
    );
}
