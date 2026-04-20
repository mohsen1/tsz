#[test]
fn test_project_definition_missing_file() {
    let mut project = Project::new();
    let result = project.get_definition("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_hover_missing_file() {
    let mut project = Project::new();
    let result = project.get_hover("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_completions_missing_file() {
    let mut project = Project::new();
    let result = project.get_completions("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_references_missing_file() {
    let mut project = Project::new();
    let result = project.find_references("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_rename_missing_file() {
    let mut project = Project::new();
    let result =
        project.get_rename_edits("nonexistent.ts", Position::new(0, 0), "newName".to_string());
    assert!(result.is_err(), "Should return Err for missing file");
}

#[test]
fn test_project_signature_help_missing_file() {
    let mut project = Project::new();
    let result = project.get_signature_help("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_implementations_missing_file() {
    let mut project = Project::new();
    let result = project.get_implementations("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_type_definition_missing_file() {
    let project = Project::new();
    let result = project.get_type_definition("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_set_import_module_specifier_ending() {
    let mut project = Project::new();
    project.set_import_module_specifier_ending(Some(".js".to_string()));
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    // Just verify it doesn't crash
    assert_eq!(project.file_count(), 1);
}

#[test]
fn test_project_set_import_module_specifier_preference() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("relative".to_string()));
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
}

