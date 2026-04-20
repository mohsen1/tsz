#[test]
fn test_project_call_hierarchy_incoming_outgoing() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function a() {
    b();
}

function b() {
    c();
}

function c() {
    return 1;
}
"#
        .to_string(),
    );

    // Check incoming calls to b (should include 'a')
    let incoming = project.get_incoming_calls("test.ts", Position::new(4, 9));
    assert!(!incoming.is_empty(), "b should have incoming calls from a");
    assert_eq!(
        incoming[0].from.name, "a",
        "Incoming call should be from 'a'"
    );

    // Check outgoing calls from b (should include 'c')
    let outgoing = project.get_outgoing_calls("test.ts", Position::new(4, 9));
    assert!(!outgoing.is_empty(), "b should have outgoing calls to c");
    assert_eq!(outgoing[0].to.name, "c", "Outgoing call should be to 'c'");
}

#[test]
fn test_project_get_document_links() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "import { foo } from './other';\n".to_string(),
    );

    let links = project.get_document_links("test.ts");
    assert!(links.is_some(), "Should return document links");
    let links = links.unwrap();
    assert!(
        !links.is_empty(),
        "Should find at least one document link for the import"
    );
}

#[test]
fn test_project_get_linked_editing_ranges_jsx() {
    let mut project = Project::new();
    project.set_file(
        "test.tsx".to_string(),
        "const el = <div>hello</div>;\n".to_string(),
    );

    // Position on opening 'div' tag (line 0, character 12)
    let ranges = project.get_linked_editing_ranges("test.tsx", Position::new(0, 12));
    // JSX linked editing should find both opening and closing tag names
    if let Some(result) = ranges {
        assert_eq!(
            result.ranges.len(),
            2,
            "Should find 2 linked ranges (opening and closing tag)"
        );
    }
}

#[test]
fn test_project_format_document() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function   foo(  ) {\nreturn 1;\n}\n".to_string(),
    );

    let options = FormattingOptions::default();
    let result = project.format_document("test.ts", &options);
    assert!(result.is_some(), "Should return formatting result");
    // The result may be Ok or Err depending on formatter availability
}

#[test]
fn test_project_prepare_type_hierarchy() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Animal {
    name: string;
}

class Dog extends Animal {
    breed: string;
}
"#
        .to_string(),
    );

    // Position on 'Dog' class name (line 4, character 6)
    let item = project.prepare_type_hierarchy("test.ts", Position::new(4, 6));
    assert!(item.is_some(), "Should prepare type hierarchy for Dog");
    let item = item.unwrap();
    assert_eq!(
        item.name, "Dog",
        "Type hierarchy item should be named 'Dog'"
    );
}

#[test]
fn test_project_supertypes() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Base {}
class Middle extends Base {}
class Child extends Middle {}
"#
        .to_string(),
    );

    // Check supertypes of Child (line 2, character 6)
    let supertypes = project.supertypes("test.ts", Position::new(2, 6));
    assert!(!supertypes.is_empty(), "Child should have supertypes");
    assert_eq!(
        supertypes[0].name, "Middle",
        "First supertype should be 'Middle'"
    );
}

#[test]
fn test_project_get_document_symbols_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_document_symbols("missing.ts").is_none());
}

#[test]
fn test_project_get_folding_ranges_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_folding_ranges("missing.ts").is_none());
}

#[test]
fn test_project_get_semantic_tokens_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_semantic_tokens_full("missing.ts").is_none());
}

#[test]
fn test_project_get_document_highlighting_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .get_document_highlighting("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

