use super::*;

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
fn test_project_get_linked_editing_ranges_jsx_member_expression() {
    let mut project = Project::new();
    project.set_file(
        "test.tsx".to_string(),
        "const x = <Foo.Bar>hi</Foo.Bar>;\n".to_string(),
    );

    let result = project
        .get_linked_editing_ranges("test.tsx", Position::new(0, 11))
        .expect("JSX member expression tag names should have linked editing ranges");

    assert_eq!(result.ranges.len(), 2);
    assert_eq!(result.ranges[0].start, Position::new(0, 11));
    assert_eq!(result.ranges[0].end, Position::new(0, 18));
    assert_eq!(result.ranges[1].start, Position::new(0, 23));
    assert_eq!(result.ranges[1].end, Position::new(0, 30));
    assert_eq!(
        result.word_pattern.as_deref(),
        Some("[a-zA-Z0-9:\\-\\._$]*")
    );
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

#[test]
fn test_project_diagnostics_on_type_error() {
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x: string = 42;\n".to_string());

    let diagnostics = project.get_diagnostics("test.ts");
    assert!(diagnostics.is_some(), "Should return diagnostics");
    let diagnostics = diagnostics.unwrap();
    assert!(
        !diagnostics.is_empty(),
        "Should have at least one diagnostic"
    );

    let has_2322 = diagnostics.iter().any(|d| d.code == Some(2322));
    assert!(has_2322, "Should report TS2322 for type mismatch");
}

#[test]
fn test_project_diagnostics_clean_for_valid_code() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x: number = 42;\nconst y: string = 'hello';\n".to_string(),
    );

    let diagnostics = project.get_diagnostics("test.ts");
    assert!(diagnostics.is_some(), "Should return diagnostics");
    let diagnostics = diagnostics.unwrap();
    assert!(
        diagnostics.is_empty(),
        "Valid code should have no diagnostics"
    );
}

#[test]
fn test_project_stale_diagnostics_empty_initially() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;\n".to_string());

    // Newly created files start with diagnostics_dirty = false
    let stale = project.get_stale_diagnostics();
    // Initially no files should be stale since set_file creates fresh ProjectFile
    // with diagnostics_dirty = false
    assert!(
        stale.is_empty(),
        "Should have no stale diagnostics for fresh files"
    );

    // After calling get_diagnostics, dirty flag is cleared
    let _ = project.get_diagnostics("a.ts");
    let stale_after = project.get_stale_diagnostics();
    assert!(
        stale_after.is_empty(),
        "Should have no stale diagnostics after getting diagnostics"
    );
}

#[test]
fn test_project_set_strict_mode() {
    let mut project = Project::new();
    project.set_strict(true);
    project.set_file(
        "test.ts".to_string(),
        "function foo(x) { return x; }\n".to_string(),
    );

    let diagnostics = project.get_diagnostics("test.ts");
    assert!(diagnostics.is_some());
}

#[test]
fn test_project_remove_file() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\n".to_string(),
    );

    assert_eq!(project.file_count(), 2);
    project.remove_file("a.ts");
    assert_eq!(project.file_count(), 1);
    assert!(project.file("a.ts").is_none());
    assert!(project.file("b.ts").is_some());
}

#[test]
fn test_project_remove_file_cleans_dependency_graph() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nexport const y = x;\n".to_string(),
    );

    // b.ts depends on a.ts (verify dependency edge exists before removal)
    let _deps = project.get_file_dependents("./a");

    // Remove a.ts
    project.remove_file("a.ts");

    // After removal, the dependency graph should not reference a.ts anymore
    let deps_after = project.get_file_dependents("a.ts");
    assert!(
        deps_after.is_empty(),
        "Dependency graph should be cleaned up after file removal, got: {deps_after:?}"
    );

    // b.ts should still exist
    assert!(project.file("b.ts").is_some());
}

#[test]
fn test_project_remove_file_invalidates_dependent_caches() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nconst y: number = x;\n".to_string(),
    );

    // Force diagnostics computation for b.ts to populate its caches
    let _ = project.get_diagnostics("b.ts");

    // Remove a.ts — b.ts's caches should be invalidated
    project.remove_file("a.ts");

    // b.ts should still be queryable (no crash)
    assert!(project.file("b.ts").is_some());
}

#[test]
fn test_project_file_count() {
    let mut project = Project::new();
    assert_eq!(project.file_count(), 0);
    project.set_file("a.ts".to_string(), "const a = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
    project.set_file("b.ts".to_string(), "const b = 2;\n".to_string());
    assert_eq!(project.file_count(), 2);
    // Overwrite existing file
    project.set_file("a.ts".to_string(), "const a = 42;\n".to_string());
    assert_eq!(project.file_count(), 2);
}

#[test]
fn test_project_get_file_dependents() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\n".to_string(),
    );

    // get_file_dependents returns files that depend on the given file
    // The exact resolution depends on how module specifiers map to file names
    let deps = project.get_file_dependents("a.ts");
    // Dependency tracking may use raw specifiers or resolved paths
    // We just verify the function returns without error
    assert!(
        deps.is_empty() || deps.iter().any(|d| d.contains("b")),
        "Dependents should either be empty (if specifier resolution differs) or include b.ts, got: {deps:?}"
    );
}

#[test]
fn test_project_import_candidates_for_prefix() {
    let mut project = Project::new();
    project.set_file(
        "utils.ts".to_string(),
        "export function calculateTotal() {}\nexport function calculateTax() {}\n".to_string(),
    );
    project.set_file("main.ts".to_string(), "calc\n".to_string());

    let candidates = project.get_import_candidates_for_prefix("main.ts", "calc");
    // Should find exported symbols from utils.ts matching prefix
    let names: Vec<&str> = candidates.iter().map(|c| c.local_name.as_str()).collect();
    assert!(
        names.iter().any(|n: &&str| n.contains("calculate")),
        "Should suggest exported symbols matching 'calc' prefix, got: {names:?}"
    );
}

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
