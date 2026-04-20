#[test]
fn test_project_scope_cache_reuse_hover_to_references_after_edit_across_files() {
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
    assert!(project.find_references("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected references to reuse cached scope after edit across files"
    );
}

#[test]
#[ignore = "TODO: LSP scope cache reuse after edit across files"]
fn test_project_scope_cache_reuse_hover_to_rename_after_edit_across_files() {
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
    let _ = project
        .get_rename_edits("b.ts", position, "next".to_string())
        .expect("Expected rename edits");

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected rename to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_signature_help_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const other = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let hover_position = {
        let file = project.file("b.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "foo(1").start
    };
    let signature_position = {
        let file = project.file("b.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "1").start
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", hover_position).is_some());
    assert!(
        project
            .get_signature_help("b.ts", signature_position)
            .is_some()
    );

    let timing = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected signature help to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_completions_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const other = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
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
    let items = project
        .get_completions("b.ts", position)
        .expect("Expected completions");
    assert!(items.iter().any(|item| item.label == "value"));

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected completions to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_completions_after_edit() {
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
    let items = project
        .get_completions("a.ts", position)
        .expect("Expected completions");
    assert!(items.iter().any(|item| item.label == "value"));

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected completions to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_signature_help_after_edit() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let hover_position = Position::new(1, 0);
    // Position must be inside the call args (after the opening paren)
    let signature_position = Position::new(1, 4);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "\"x\"");
        TextEdit::new(range, "\"y\"".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("a.ts", hover_position).is_some());
    assert!(
        project
            .get_signature_help("a.ts", signature_position)
            .is_some()
    );

    let timing = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected signature help to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_references_after_edit() {
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
    assert!(project.find_references("a.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected references to reuse cached scope after edit"
    );
}

#[test]
#[ignore = "TODO: LSP scope cache reuse after edit"]
fn test_project_scope_cache_reuse_hover_to_rename_after_edit() {
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
    let _ = project
        .get_rename_edits("a.ts", position, "next".to_string())
        .expect("Expected rename edits");

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected rename to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_across_requests() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.get_definition("a.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_completions() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    let items = project
        .get_completions("a.ts", position)
        .expect("Expected completions");
    assert!(items.iter().any(|item| item.label == "value"));

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected completions to reuse cached scope"
    );
}

