#[test]
fn test_project_resolve_code_lens_missing_file() {
    let mut project = Project::new();
    let lens = CodeLens {
        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
        command: None,
        data: None,
    };
    let result = project.resolve_code_lens("nonexistent.ts", &lens);
    assert!(
        result.is_none(),
        "resolve_code_lens should return None for missing file"
    );
}

#[test]
fn test_project_get_incoming_calls_missing_file() {
    let project = Project::new();
    let result = project.get_incoming_calls("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "get_incoming_calls should return empty for missing file"
    );
}

#[test]
fn test_project_get_outgoing_calls_missing_file() {
    let project = Project::new();
    let result = project.get_outgoing_calls("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "get_outgoing_calls should return empty for missing file"
    );
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_project_multiple_file_diagnostics() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const x: string = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: number = 'hello';\n".to_string(),
    );

    let diags_a = project
        .get_diagnostics("a.ts")
        .expect("Expected diagnostics for a.ts");
    let diags_b = project
        .get_diagnostics("b.ts")
        .expect("Expected diagnostics for b.ts");

    assert!(!diags_a.is_empty(), "a.ts should have type errors");
    assert!(!diags_b.is_empty(), "b.ts should have type errors");
}

#[test]
fn test_project_diagnostics_update_after_edit() {
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x: string = 42;\n".to_string());

    let diags_before = project
        .get_diagnostics("test.ts")
        .expect("Expected diagnostics");
    assert!(
        !diags_before.is_empty(),
        "Should have type error before fix"
    );

    // Fix the error
    let edit = {
        let file = project.file("test.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "42");
        TextEdit::new(range, "\"hello\"".to_string())
    };
    project
        .update_file("test.ts", &[edit])
        .expect("Expected update to succeed");

    let diags_after = project
        .get_diagnostics("test.ts")
        .expect("Expected diagnostics after fix");
    assert!(
        diags_after.len() < diags_before.len(),
        "Should have fewer diagnostics after fix, before: {}, after: {}",
        diags_before.len(),
        diags_after.len()
    );
}

#[test]
fn test_project_folding_ranges_nested_functions() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function outer() {
    function inner() {
        return 1;
    }
    return inner();
}
"#
        .to_string(),
    );

    let ranges = project.get_folding_ranges("test.ts");
    assert!(ranges.is_some(), "Should return folding ranges");
    let ranges = ranges.unwrap();
    assert!(
        ranges.len() >= 2,
        "Should have folding ranges for outer and inner functions, got: {}",
        ranges.len()
    );
}

#[test]
fn test_project_selection_ranges_multiple_positions() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x = 1;\nconst y = 2;\n".to_string(),
    );

    let positions = vec![Position::new(0, 6), Position::new(1, 6)];
    let ranges = project.get_selection_ranges("test.ts", &positions);
    assert!(ranges.is_some(), "Should return selection ranges");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 2, "Should have one result per position");
    assert!(ranges[0].is_some(), "First position should have a range");
    assert!(ranges[1].is_some(), "Second position should have a range");
}

#[test]
fn test_project_selection_ranges_returns_none_for_missing_file() {
    let project = Project::new();
    let result = project.get_selection_ranges("missing.ts", &[Position::new(0, 0)]);
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_inlay_hints_function_call() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function greet(name: string, age: number) { return name; }\ngreet(\"Alice\", 30);\n"
            .to_string(),
    );

    let range = Range::new(Position::new(0, 0), Position::new(2, 0));
    let hints = project.get_inlay_hints("test.ts", range);
    assert!(hints.is_some(), "Should return inlay hints result");
}

#[test]
fn test_project_code_lens_for_functions() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function foo() {
    return 1;
}

function bar() {
    return foo();
}
"#
        .to_string(),
    );

    let lenses = project.get_code_lenses("test.ts");
    assert!(lenses.is_some(), "Should return code lenses");
    let lenses = lenses.unwrap();
    // There should be at least some code lenses for functions
    assert!(
        !lenses.is_empty(),
        "Should produce code lenses for functions"
    );
}

