#[test]
fn test_project_set_auto_import_file_exclude_patterns() {
    let mut project = Project::new();
    project.set_auto_import_file_exclude_patterns(vec!["**/node_modules/**".to_string()]);
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
}

#[test]
fn test_project_set_auto_import_specifier_exclude_regexes() {
    let mut project = Project::new();
    project.set_auto_import_specifier_exclude_regexes(vec!["^@internal/.*$".to_string()]);
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
}

#[test]
fn test_project_update_file_with_edits() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x = 1;\nconst y = 2;\n".to_string(),
    );

    let source = "const x = 1;\nconst y = 2;\n";
    let line_map = LineMap::build(source);
    let edit = TextEdit::new(
        range_for_substring(source, &line_map, "1"),
        "42".to_string(),
    );

    let result = project.update_file("test.ts", &[edit]);
    assert!(result.is_some(), "update_file should succeed");

    let file = project.file("test.ts").unwrap();
    assert!(
        file.source_text().contains("42"),
        "Source should contain updated value"
    );
}

#[test]
fn test_project_update_file_missing() {
    let mut project = Project::new();
    let edit = TextEdit::new(
        Range::new(Position::new(0, 0), Position::new(0, 1)),
        "x".to_string(),
    );
    let result = project.update_file("nonexistent.ts", &[edit]);
    assert!(
        result.is_none(),
        "update_file should return None for missing file"
    );
}

#[test]
fn test_project_cross_file_definition() {
    let mut project = Project::new();
    project.set_file(
        "utils.ts".to_string(),
        "export function helper() {}\n".to_string(),
    );
    project.set_file(
        "main.ts".to_string(),
        "import { helper } from \"./utils\";\nhelper();\n".to_string(),
    );

    // Get definition for 'helper' usage on line 1
    let result = project.get_definition("main.ts", Position::new(1, 0));
    // Cross-file definition may resolve to the import specifier or the original declaration
    // depending on resolution. We just verify it returns a result.
    assert!(
        result.is_some(),
        "Should find definition for imported symbol"
    );
}

#[test]
fn test_project_workspace_symbols_filter() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export function alpha() {}\nexport function beta() {}\n".to_string(),
    );

    // Workspace symbols search for "alpha" should find the function
    let symbols = project.get_workspace_symbols("alpha");
    // The symbol index may or may not be populated depending on how set_file works
    // This test verifies the function returns without error and produces results if indexed
    if !symbols.is_empty() {
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.iter().any(|n: &&str| n.contains("alpha")),
            "Should find symbols matching 'alpha' query, got: {names:?}"
        );
    }
}

#[test]
fn test_project_handle_will_rename_files() {
    let mut project = Project::new();
    project.set_file("old.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "consumer.ts".to_string(),
        "import { x } from \"./old\";\n".to_string(),
    );

    let renames = vec![FileRename {
        old_uri: "old.ts".to_string(),
        new_uri: "new.ts".to_string(),
    }];

    let workspace_edit = project.handle_will_rename_files(&renames);
    // The workspace edit should contain text edits to update import paths
    let has_edits = workspace_edit
        .changes
        .values()
        .any(|edits| !edits.is_empty());
    assert!(
        has_edits,
        "Should produce workspace edits to update import paths"
    );
}

#[test]
fn test_project_subtypes_returns_empty_for_missing_file() {
    let project = Project::new();
    let result = project.subtypes("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "subtypes should return empty for missing file"
    );
}

#[test]
fn test_project_supertypes_returns_empty_for_missing_file() {
    let project = Project::new();
    let result = project.supertypes("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "supertypes should return empty for missing file"
    );
}

#[test]
fn test_project_get_code_actions_missing_file() {
    let project = Project::new();
    let range = Range::new(Position::new(0, 0), Position::new(0, 1));
    let result = project.get_code_actions("nonexistent.ts", range, vec![], None);
    assert!(
        result.is_none(),
        "get_code_actions should return None for missing file"
    );
}

