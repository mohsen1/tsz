use super::*;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

fn get_ranges(source: &str) -> Vec<FoldingRange> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);
    let provider = FoldingRangeProvider::new(arena, &line_map, source);
    provider.get_folding_ranges(root)
}

#[test]
fn test_folding_ranges_simple_function() {
    let source = "\nfunction foo() {\n    return 1;\n}\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should find at least one folding range");
    let function_range = ranges.iter().find(|r| r.start_line == 1 && r.end_line == 3);
    assert!(
        function_range.is_some(),
        "Should find function body folding range"
    );
}

#[test]
fn test_folding_ranges_nested_functions() {
    let source = "\nfunction outer() {\n    function inner() {\n        return 1;\n    }\n    return inner();\n}\n";
    let ranges = get_ranges(source);
    assert!(ranges.len() >= 2, "Should find at least 2 folding ranges");
}

#[test]
fn test_folding_ranges_class() {
    let source = "\nclass MyClass {\n    method1() {\n        return 1;\n    }\n\n    method2() {\n        return 2;\n    }\n}\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should find folding ranges for class");
    let class_range = ranges
        .iter()
        .find(|r| r.kind.is_none() && r.start_line == 1);
    assert!(
        class_range.is_some(),
        "Should find class body folding range"
    );
}

#[test]
fn test_folding_ranges_block_statement() {
    let source = "\nif (true) {\n    console.log(\"yes\");\n}\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find block statement folding range"
    );
}

#[test]
fn test_folding_ranges_interface() {
    let source = "\ninterface Point {\n    x: number;\n    y: number;\n}\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should find interface folding range");
}

#[test]
fn test_folding_ranges_enum() {
    let source = "\nenum Color {\n    Red,\n    Green,\n    Blue\n}\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should find enum folding range");
}

#[test]
fn test_folding_ranges_namespace() {
    let source = "\nnamespace MyNamespace {\n    function foo() {}\n    const bar = 1;\n}\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should find namespace folding range");
}

#[test]
fn test_folding_ranges_no_single_line() {
    let source = "function foo() { return 1; }";
    let ranges = get_ranges(source);
    assert!(
        ranges.is_empty(),
        "Should not find folding ranges for single-line code"
    );
}

#[test]
fn test_folding_ranges_empty_source() {
    let source = "";
    let ranges = get_ranges(source);
    assert!(
        ranges.is_empty(),
        "Should not find folding ranges in empty source"
    );
}

// --- #region/#endregion tests ---

#[test]
fn test_region_basic() {
    let source = "// #region My Region\nconst a = 1;\nconst b = 2;\n// #endregion\n";
    let ranges = get_ranges(source);
    let region = ranges
        .iter()
        .find(|r| r.kind.as_deref() == Some("region") && r.start_line == 0);
    assert!(region.is_some(), "Should find #region folding range");
    let region = region.unwrap();
    assert_eq!(region.start_line, 0);
    assert_eq!(region.end_line, 3);
}

#[test]
fn test_region_no_space_before_hash() {
    let source = "//#region NoSpace\nconst x = 1;\n//#endregion\n";
    let ranges = get_ranges(source);
    let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
    assert!(
        region.is_some(),
        "Should find #region without space after //"
    );
    let region = region.unwrap();
    assert_eq!(region.start_line, 0);
    assert_eq!(region.end_line, 2);
}

#[test]
fn test_region_nested() {
    let source = "// #region Outer\nconst a = 1;\n// #region Inner\nconst b = 2;\n// #endregion\nconst c = 3;\n// #endregion\n";
    let ranges = get_ranges(source);
    let regions: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("region"))
        .collect();
    assert!(
        regions.len() >= 2,
        "Should find at least 2 nested regions, found {}",
        regions.len()
    );
    let inner = regions
        .iter()
        .find(|r| r.start_line == 2 && r.end_line == 4);
    assert!(inner.is_some(), "Should find inner region (lines 2-4)");
    let outer = regions
        .iter()
        .find(|r| r.start_line == 0 && r.end_line == 6);
    assert!(outer.is_some(), "Should find outer region (lines 0-6)");
}

#[test]
fn test_region_inside_block_comment_ignored() {
    let source =
        "/*\n// #region Should Be Ignored\nsome comment text\n// #endregion\n*/\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let regions: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("region"))
        .collect();
    assert!(
        regions.is_empty(),
        "Should NOT find region markers inside block comments"
    );
}

#[test]
fn test_region_unclosed_is_ignored() {
    let source = "// #region Unclosed\nconst a = 1;\nconst b = 2;\n";
    let ranges = get_ranges(source);
    let regions: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("region"))
        .collect();
    assert!(
        regions.is_empty(),
        "Unclosed #region should not produce a folding range"
    );
}

#[test]
fn test_region_with_label() {
    let source = "// #region Database Setup\nconst db = connect();\n// #endregion\n";
    let ranges = get_ranges(source);
    let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
    assert!(region.is_some(), "Should find region with label");
}

#[test]
fn test_region_without_label() {
    let source = "// #region\nconst db = connect();\n// #endregion\n";
    let ranges = get_ranges(source);
    let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
    assert!(region.is_some(), "Should find region without label");
}

// --- Consecutive single-line comment tests ---

#[test]
fn test_consecutive_single_line_comments() {
    let source =
        "// This is a\n// multi-line description\n// using single-line comments\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let comment = ranges
        .iter()
        .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 0);
    assert!(
        comment.is_some(),
        "Should find consecutive single-line comment fold"
    );
    let comment = comment.unwrap();
    assert_eq!(comment.start_line, 0);
    assert_eq!(comment.end_line, 2);
}

#[test]
fn test_single_comment_no_fold() {
    let source = "// Just one comment\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let comment = ranges
        .iter()
        .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 0);
    assert!(
        comment.is_none(),
        "A single // comment should not produce a fold"
    );
}

#[test]
fn test_two_single_line_comments_fold() {
    let source = "// Comment line 1\n// Comment line 2\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let comment = ranges
        .iter()
        .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 0);
    assert!(comment.is_some(), "Two consecutive // comments should fold");
    let comment = comment.unwrap();
    assert_eq!(comment.start_line, 0);
    assert_eq!(comment.end_line, 1);
}

#[test]
fn test_region_comments_excluded_from_single_line_group() {
    let source = "// #region Test\n// normal comment 1\n// normal comment 2\n// #endregion\n";
    let ranges = get_ranges(source);
    let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
    assert!(region.is_some(), "Should find region fold");
    let comment = ranges
        .iter()
        .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 1);
    assert!(
        comment.is_some(),
        "Should find comment fold for normal comments inside region"
    );
    if let Some(c) = comment {
        assert_eq!(c.end_line, 2);
    }
}

// --- Block comment tests ---

#[test]
fn test_multiline_block_comment() {
    let source = "/*\n * This is a block comment\n * spanning multiple lines\n */\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let comment = ranges.iter().find(|r| r.kind.as_deref() == Some("comment"));
    assert!(
        comment.is_some(),
        "Should find multi-line block comment fold"
    );
    let comment = comment.unwrap();
    assert_eq!(comment.start_line, 0);
    assert_eq!(comment.end_line, 3);
}

#[test]
fn test_jsdoc_comment() {
    let source =
        "/**\n * JSDoc comment\n * @param x - the value\n */\nfunction foo(x: number) {}\n";
    let ranges = get_ranges(source);
    let comment = ranges.iter().find(|r| r.kind.as_deref() == Some("comment"));
    assert!(comment.is_some(), "Should find JSDoc comment fold");
    let comment = comment.unwrap();
    assert_eq!(comment.start_line, 0);
    assert_eq!(comment.end_line, 3);
}

#[test]
fn test_adjacent_block_comments_separate() {
    let source = "/*\n * First block\n */\n/*\n * Second block\n */\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let comments: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("comment"))
        .collect();
    assert_eq!(
        comments.len(),
        2,
        "Two adjacent block comments should produce two separate folds, got {}",
        comments.len()
    );
}

// --- Import group tests ---

#[test]
fn test_import_group_fold() {
    let source = "import { a } from \"a\";\nimport { b } from \"b\";\nimport { c } from \"c\";\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let imports = ranges.iter().find(|r| r.kind.as_deref() == Some("imports"));
    assert!(imports.is_some(), "Should find import group fold");
    let imports = imports.unwrap();
    assert_eq!(imports.start_line, 0, "Import group should start at line 0");
    // The end line depends on parser node boundaries; just verify it covers multiple lines
    assert!(
        imports.end_line >= 2,
        "Import group should span at least 3 lines"
    );
}

#[test]
fn test_single_import_no_fold() {
    let source = "import { a } from \"a\";\n\nconst x = 1;\n";
    let ranges = get_ranges(source);
    let imports = ranges.iter().find(|r| r.kind.as_deref() == Some("imports"));
    assert!(
        imports.is_none(),
        "Single import should not produce imports fold"
    );
}

#[test]
fn test_two_import_groups_separated() {
    let source = "import { a } from \"a\";\nimport { b } from \"b\";\n\nconst mid = 1;\n\nimport { c } from \"c\";\nimport { d } from \"d\";\n";
    let ranges = get_ranges(source);
    let imports: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("imports"))
        .collect();
    assert_eq!(
        imports.len(),
        2,
        "Should find two import groups, found {}",
        imports.len()
    );
}

// --- parse_region_delimiter unit tests ---

#[test]
fn test_parse_region_delimiter_basic() {
    let result = parse_region_delimiter("// #region My Region");
    assert!(result.is_some());
    let d = result.unwrap();
    assert!(d.is_start);
}

#[test]
fn test_parse_region_delimiter_no_space() {
    let result = parse_region_delimiter("//#region NoSpace");
    assert!(result.is_some());
    let d = result.unwrap();
    assert!(d.is_start);
}

#[test]
fn test_parse_region_delimiter_endregion() {
    let result = parse_region_delimiter("// #endregion");
    assert!(result.is_some());
    let d = result.unwrap();
    assert!(!d.is_start);
}

#[test]
fn test_parse_region_delimiter_no_label() {
    let result = parse_region_delimiter("// #region");
    assert!(result.is_some());
    let d = result.unwrap();
    assert!(d.is_start);
}

#[test]
fn test_parse_region_delimiter_not_a_region() {
    assert!(parse_region_delimiter("// just a comment").is_none());
    assert!(parse_region_delimiter("const x = 1;").is_none());
    assert!(parse_region_delimiter("/* #region */").is_none());
}

#[test]
fn test_parse_region_delimiter_with_preceding_text() {
    assert!(parse_region_delimiter("test // #region").is_none());
}

// --- Combined test ---

#[test]
fn test_combined_imports_comments_regions() {
    let source = "// #region Imports\nimport { a } from \"a\";\nimport { b } from \"b\";\n// #endregion\n\n// Header comment\n// describing the module\nconst x = 1;\n\n/*\n * Block comment\n */\nfunction foo() {\n    return x;\n}\n";
    let ranges = get_ranges(source);
    let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
    assert!(region.is_some(), "Should find region fold");
    let imports = ranges.iter().find(|r| r.kind.as_deref() == Some("imports"));
    assert!(imports.is_some(), "Should find imports fold");
    let comments: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("comment"))
        .collect();
    assert!(
        comments.len() >= 2,
        "Should find at least 2 comment folds, found {}",
        comments.len()
    );
}
