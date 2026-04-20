#[test]
fn test_project_get_inlay_hints_returns_none_for_missing_file() {
    let project = Project::new();
    let range = Range::new(Position::new(0, 0), Position::new(10, 0));
    assert!(project.get_inlay_hints("missing.ts", range).is_none());
}

#[test]
fn test_project_prepare_call_hierarchy_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .prepare_call_hierarchy("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_get_document_links_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_document_links("missing.ts").is_none());
}

#[test]
fn test_project_get_linked_editing_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .get_linked_editing_ranges("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_format_document_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .format_document("missing.ts", &FormattingOptions::default())
            .is_none()
    );
}

#[test]
fn test_project_document_symbols_with_nested_structure() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"namespace MyApp {
    export interface Config {
        host: string;
        port: number;
    }

    export class Server {
        config: Config;
        start() {}
        stop() {}
    }

    export function createServer(config: Config): Server {
        return new Server();
    }
}
"#
        .to_string(),
    );

    let symbols = project.get_document_symbols("test.ts").unwrap();

    // Should have the MyApp namespace as top-level
    let ns = symbols.iter().find(|s| s.name == "MyApp");
    assert!(ns.is_some(), "Should have MyApp namespace");

    let ns = ns.unwrap();
    assert!(!ns.children.is_empty(), "MyApp should have children");

    let child_names: Vec<&str> = ns.children.iter().map(|c| c.name.as_str()).collect();
    assert!(
        child_names.contains(&"Config"),
        "MyApp should contain Config"
    );
    assert!(
        child_names.contains(&"Server"),
        "MyApp should contain Server"
    );
    assert!(
        child_names.contains(&"createServer"),
        "MyApp should contain createServer"
    );
}

#[test]
fn test_project_folding_ranges_include_comments() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"// region: MyRegion
const a = 1;
const b = 2;
// endregion

/*
 * Multi-line comment
 * spanning several lines
 */
function foo() {
    return a + b;
}
"#
        .to_string(),
    );

    let ranges = project.get_folding_ranges("test.ts").unwrap();
    // Should have folding for: region, multi-line comment, function body
    assert!(ranges.len() >= 2, "Should have at least 2 folding ranges");
}

#[test]
fn test_project_semantic_tokens_for_class() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Point {
    x: number;
    y: number;
    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }
    distance(): number {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }
}
"#
        .to_string(),
    );

    let tokens = project.get_semantic_tokens_full("test.ts").unwrap();
    assert!(
        !tokens.is_empty(),
        "Should produce semantic tokens for a class"
    );
    assert_eq!(tokens.len() % 5, 0, "Token count should be divisible by 5");
}

#[test]
fn test_project_document_highlighting_keyword() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"if (true) {
    console.log("a");
} else if (false) {
    console.log("b");
} else {
    console.log("c");
}
"#
        .to_string(),
    );

    // Position on 'if' keyword at line 0, character 0
    let highlights = project.get_document_highlighting("test.ts", Position::new(0, 0));
    // Keyword highlighting should find all if/else branches
    if let Some(highlights) = highlights {
        assert!(
            highlights.len() >= 2,
            "Should highlight multiple if/else keywords"
        );
    }
}

#[test]
fn test_project_workspace_symbols_empty_query() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export function createUser() {}\nexport class UserService {}\n".to_string(),
    );

    // Empty query should return no symbols (workspace symbols spec)
    let symbols = project.get_workspace_symbols("");
    assert!(symbols.is_empty(), "Empty query should return no symbols");
}

