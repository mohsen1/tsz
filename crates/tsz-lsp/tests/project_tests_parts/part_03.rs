#[test]
fn test_project_diagnostics_cached() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value: string = 1;\n".to_string());

    let diagnostics = project
        .get_diagnostics("a.ts")
        .expect("Expected diagnostics");
    assert!(!diagnostics.is_empty(), "Should report diagnostics");
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::Error));

    let diagnostics_again = project
        .get_diagnostics("a.ts")
        .expect("Expected diagnostics on cached run");
    assert_eq!(diagnostics_again.len(), diagnostics.len());
}

#[test]
fn test_project_performance_scope_cache_hits_definition() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let _ = project.get_definition("a.ts", position);
    let first = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    let _ = project.get_definition("a.ts", position);
    let second = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_hover() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::Hover)
        .expect("Expected timing data for hover");

    assert!(project.get_hover("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::Hover)
        .expect("Expected timing data for hover");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_completions() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\n".to_string());
    let position = Position::new(1, 0);

    let first_items = project
        .get_completions("a.ts", position)
        .expect("Expected completions on first request");
    assert!(first_items.iter().any(|item| item.label == "value"));

    let first = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    let second_items = project
        .get_completions("a.ts", position)
        .expect("Expected completions on second request");
    assert!(second_items.iter().any(|item| item.label == "value"));

    let second = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_signature_help() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let position = Position::new(1, 4);

    assert!(project.get_signature_help("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(project.get_signature_help("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_references() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.find_references("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(project.find_references("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
#[ignore = "TODO: LSP scope cache performance test"]
fn test_project_performance_scope_cache_hits_rename() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let _ = project
        .get_rename_edits("a.ts", position, "next".to_string())
        .expect("Expected rename edits");
    let first = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    let _ = project
        .get_rename_edits("a.ts", position, "next2".to_string())
        .expect("Expected rename edits");
    let second = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_scope_cache_cleared_after_update() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "const value = 1;\nvalue;\nconst later = 2;\n".to_string(),
    );
    let position = Position::new(1, 0);

    assert!(project.get_definition("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(project.get_definition("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "2");
        TextEdit::new(range, "3".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_definition("a.ts", position).is_some());
    let third = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(third.scope_misses > 0, "Expected cache misses after edit");
    assert_eq!(
        third.scope_hits, 0,
        "Expected cache hits cleared after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_definition_after_edit() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.get_definition("a.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_definition_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", position).is_some());
    assert!(project.get_definition("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope after edit across files"
    );
}

