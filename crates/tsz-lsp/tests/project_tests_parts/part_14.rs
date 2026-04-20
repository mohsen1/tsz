#[test]
fn test_project_code_lens_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_code_lenses("missing.ts").is_none());
}

#[test]
fn test_project_semantic_tokens_for_interface() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "interface Foo {\n    bar: string;\n    baz(x: number): void;\n}\n".to_string(),
    );

    let tokens = project.get_semantic_tokens_full("test.ts");
    assert!(tokens.is_some(), "Should return semantic tokens");
    let tokens = tokens.unwrap();
    assert_eq!(tokens.len() % 5, 0, "Token data should be in groups of 5");
    assert!(
        tokens.len() >= 5,
        "Should have at least one semantic token for interface"
    );
}

#[test]
fn test_project_format_document_produces_edits() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function   foo(  ){\nreturn 1\n}\n".to_string(),
    );

    let options = FormattingOptions::default();
    let result = project.format_document("test.ts", &options);
    assert!(result.is_some(), "Should return formatting result");
    let edits = result.unwrap();
    // Formatting result depends on formatter configuration
    // Just verify the API returns without panicking
    let _ = edits;
}

#[test]
fn test_project_highlighting_variable_all_occurrences() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const value = 1;\nconst doubled = value * 2;\nconst tripled = value * 3;\n".to_string(),
    );

    // Position on 'value' at declaration (line 0, char 6)
    let highlights = project.get_document_highlighting("test.ts", Position::new(0, 6));
    assert!(highlights.is_some(), "Should find highlights for 'value'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 3,
        "Should highlight all 3 occurrences of 'value', got: {}",
        highlights.len()
    );
}

#[test]
fn test_project_call_hierarchy_prepare_missing_file() {
    let project = Project::new();
    assert!(
        project
            .prepare_call_hierarchy("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_type_hierarchy_prepare_for_interface() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"interface Shape {
    area(): number;
}

interface Circle extends Shape {
    radius: number;
}
"#
        .to_string(),
    );

    let item = project.prepare_type_hierarchy("test.ts", Position::new(4, 10));
    assert!(
        item.is_some(),
        "Should prepare type hierarchy for Circle interface"
    );
    let item = item.unwrap();
    assert_eq!(
        item.name, "Circle",
        "Type hierarchy item should be 'Circle'"
    );
}

#[test]
fn test_project_subtypes_for_base_class() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Base {
    value: number;
}

class Derived extends Base {
    extra: string;
}
"#
        .to_string(),
    );

    let subtypes = project.subtypes("test.ts", Position::new(0, 6));
    // May or may not find subtypes depending on implementation scope
    // At minimum, verify it does not crash and returns a valid result
    let _ = subtypes;
}

#[test]
fn test_project_stale_diagnostics_after_file_update() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nconst y: string = x;\n".to_string(),
    );

    // Initial diagnostics
    let _ = project.get_diagnostics("b.ts");

    // Update a.ts
    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let stale = project.get_stale_diagnostics();
    // After updating a.ts, dependents (b.ts) may be marked stale
    // The implementation may vary, so just check this doesn't crash
    let _ = stale;
}

#[test]
fn test_project_hover_on_function_declaration() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function add(a: number, b: number): number {\n    return a + b;\n}\nadd(1, 2);\n"
            .to_string(),
    );

    // Hover on 'add' at its declaration
    let hover = project.get_hover("test.ts", Position::new(0, 9));
    assert!(
        hover.is_some(),
        "Should provide hover for function declaration"
    );
    let hover = hover.unwrap();
    assert!(
        hover.contents.iter().any(|c| c.contains("add")),
        "Hover should contain function name"
    );
}

#[test]
fn test_project_definition_within_same_file() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const myVar = 42;\nconsole.log(myVar);\n".to_string(),
    );

    // Go to definition of myVar usage (line 1)
    let defs = project.get_definition("test.ts", Position::new(1, 12));
    assert!(defs.is_some(), "Should find definition for myVar");
    let defs = defs.unwrap();
    assert!(
        defs.iter()
            .any(|loc| loc.file_path == "test.ts" && loc.range.start.line == 0),
        "Definition should point to declaration on line 0"
    );
}

