#[test]
fn test_export_addition_invalidates_dependents() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo();\n".to_string(),
    );

    // Manually wire the dependency (extract_imports uses raw specifiers like "./a")
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Clean b.ts diagnostics
    let _ = project.get_diagnostics("b.ts");
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should be clean after getting diagnostics"
    );

    // Add a new export to a.ts — this changes the export signature
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let end_range = Range::new(
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
    );
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: end_range,
            new_text: "\nexport function bar() {}".to_string(),
        }],
    );

    // b.ts SHOULD be marked dirty — the export signature changed
    assert!(
        project.files["b.ts"].diagnostics_dirty,
        "b.ts SHOULD be invalidated when a.ts adds a new export"
    );
}

#[test]
fn test_comment_edit_does_not_invalidate_dependents() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "// version 1\nexport const x = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nconsole.log(x);\n".to_string(),
    );

    // Manually wire the dependency (extract_imports uses raw specifiers like "./a")
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Clean b.ts
    let _ = project.get_diagnostics("b.ts");

    // Edit only the comment in a.ts
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let edit_range = range_for_substring(&a_source, &a_line_map, "version 1");
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: edit_range,
            new_text: "version 2".to_string(),
        }],
    );

    // b.ts should NOT be dirty
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should NOT be invalidated by a comment-only edit in a.ts"
    );
}

#[test]
fn test_private_addition_does_not_invalidate_dependents() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export function foo() {}\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo();\n".to_string(),
    );

    // Manually wire the dependency (extract_imports uses raw specifiers like "./a")
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Clean b.ts
    let _ = project.get_diagnostics("b.ts");

    // Add a private (non-exported) symbol to a.ts
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let end_range = Range::new(
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
    );
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: end_range,
            new_text: "const helper = 42;\n".to_string(),
        }],
    );

    // b.ts should NOT be dirty — private additions don't change exports
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should NOT be invalidated when a.ts adds a private symbol"
    );
}

// =============================================================================
// Project-level feature tests for new wrappers
// =============================================================================

#[test]
fn test_project_get_document_symbols() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"
interface Greeter {
    greet(): string;
}

class Hello implements Greeter {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
    greet() { return `Hello ${this.name}`; }
}

function createGreeter(name: string): Greeter {
    return new Hello(name);
}

const DEFAULT_NAME = "World";
"#
        .to_string(),
    );

    let symbols = project.get_document_symbols("test.ts");
    assert!(symbols.is_some(), "Should return document symbols");
    let symbols = symbols.unwrap();
    assert!(
        symbols.len() >= 3,
        "Should have at least 3 top-level symbols (Greeter, Hello, createGreeter, DEFAULT_NAME)"
    );

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Greeter"),
        "Should contain Greeter interface"
    );
    assert!(names.contains(&"Hello"), "Should contain Hello class");
    assert!(
        names.contains(&"createGreeter"),
        "Should contain createGreeter function"
    );
    assert!(
        names.contains(&"DEFAULT_NAME"),
        "Should contain DEFAULT_NAME constant"
    );

    // Check that Hello has children (members)
    let hello = symbols.iter().find(|s| s.name == "Hello").unwrap();
    assert!(
        !hello.children.is_empty(),
        "Hello class should have children (members)"
    );
}

#[test]
fn test_project_get_folding_ranges() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function foo() {
    if (true) {
        console.log("hi");
    }
}

class Bar {
    method() {
        return 42;
    }
}
"#
        .to_string(),
    );

    let ranges = project.get_folding_ranges("test.ts");
    assert!(ranges.is_some(), "Should return folding ranges");
    let ranges = ranges.unwrap();
    assert!(
        ranges.len() >= 3,
        "Should have at least 3 folding ranges (function, if, class, method)"
    );
}

#[test]
fn test_project_get_selection_ranges() {
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x = 42;\n".to_string());

    let positions = vec![Position::new(0, 6)]; // on 'x'
    let ranges = project.get_selection_ranges("test.ts", &positions);
    assert!(ranges.is_some(), "Should return selection ranges");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 1, "Should have one result per position");
    assert!(ranges[0].is_some(), "Selection range at 'x' should exist");
}

#[test]
fn test_project_get_semantic_tokens() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x: number = 42;\nfunction foo(a: string) { return a; }\n".to_string(),
    );

    let tokens = project.get_semantic_tokens_full("test.ts");
    assert!(tokens.is_some(), "Should return semantic tokens");
    let tokens = tokens.unwrap();
    // Tokens are encoded as groups of 5 integers (deltaLine, deltaStartChar, length, tokenType, tokenModifiers)
    assert_eq!(tokens.len() % 5, 0, "Token data should be in groups of 5");
    assert!(tokens.len() >= 5, "Should have at least 1 token");
}

#[test]
fn test_project_get_document_highlighting() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x = 1;\nconst y = x + x;\n".to_string(),
    );

    // Position on 'x' at line 1, character 10
    let highlights = project.get_document_highlighting("test.ts", Position::new(1, 10));
    assert!(highlights.is_some(), "Should find highlights for 'x'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight at least 2 occurrences of 'x'"
    );
}

#[test]
fn test_project_get_inlay_hints() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function add(a: number, b: number) { return a + b; }\nconst sum = add(1, 2);\n"
            .to_string(),
    );

    let range = Range::new(Position::new(0, 0), Position::new(2, 0));
    let hints = project.get_inlay_hints("test.ts", range);
    assert!(hints.is_some(), "Should return inlay hints");
    // Whether hints are non-empty depends on the provider configuration
}

#[test]
fn test_project_prepare_call_hierarchy() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function caller() {
    callee();
}

function callee() {
    return 42;
}
"#
        .to_string(),
    );

    // Position on 'callee' declaration (line 4, character 9)
    let item = project.prepare_call_hierarchy("test.ts", Position::new(4, 9));
    assert!(item.is_some(), "Should prepare call hierarchy for callee");
    let item = item.unwrap();
    assert_eq!(
        item.name, "callee",
        "Call hierarchy item should be named 'callee'"
    );
}

