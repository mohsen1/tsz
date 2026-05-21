use super::*;

pub(super) fn make_server() -> Server {
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

pub(super) fn make_server_with_real_libs() -> Server {
    let mut server = make_server();
    server.lib_dir = Server::find_lib_dir().expect("lib dir should be discoverable in tests");
    server.tests_lib_dir = Server::find_tests_lib_dir(&server.lib_dir);
    server
}

pub(super) fn make_request(command: &str, arguments: serde_json::Value) -> TsServerRequest {
    TsServerRequest {
        seq: 1,
        _msg_type: "request".to_string(),
        command: command.to_string(),
        arguments,
    }
}

pub(super) fn apply_tsserver_text_edits(mut source: String, edits: &[serde_json::Value]) -> String {
    let mut spans: Vec<(usize, usize, String)> = edits
        .iter()
        .filter_map(|edit| {
            let start = edit.get("start")?;
            let end = edit.get("end")?;
            let start_line = start.get("line")?.as_u64()? as u32;
            let start_offset = start.get("offset")?.as_u64()? as u32;
            let end_line = end.get("line")?.as_u64()? as u32;
            let end_offset = end.get("offset")?.as_u64()? as u32;
            let new_text = edit.get("newText")?.as_str()?.to_string();
            let start_byte = Server::line_offset_to_byte(&source, start_line, start_offset);
            let end_byte = Server::line_offset_to_byte(&source, end_line, end_offset);
            Some((start_byte, end_byte, new_text))
        })
        .collect();

    spans.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (start, end, new_text) in spans {
        if start <= end && end <= source.len() {
            source.replace_range(start..end, &new_text);
        }
    }
    source
}

pub(super) fn assert_full_comment_text_changes(body: &serde_json::Value) {
    let changes = body
        .as_array()
        .expect("comment edit body should be an array");
    assert!(
        !changes.is_empty(),
        "expected at least one edit, got {body:#}"
    );
    for change in changes {
        assert!(
            change.get("start").is_none(),
            "full comment edit should not use simplified start/end shape: {change:#}"
        );
        assert!(
            change.get("end").is_none(),
            "full comment edit should not use simplified start/end shape: {change:#}"
        );
        assert!(
            change
                .get("newText")
                .and_then(serde_json::Value::as_str)
                .is_some(),
            "full comment edit should include newText: {change:#}"
        );
        let span = change
            .get("span")
            .expect("full comment edit should include span");
        assert!(
            span.get("start")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "full comment edit span should include numeric start: {change:#}"
        );
        assert!(
            span.get("length")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "full comment edit span should include numeric length: {change:#}"
        );
    }
}
