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

#[test]
fn test_folding_ranges_switch_statement() {
    let source = "\nswitch (x) {\n  case 1:\n    foo();\n    break;\n  case 2:\n    bar();\n    break;\n  default:\n    baz();\n}\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should find folding ranges for switch");
    let switch_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        switch_range.is_some(),
        "Should find switch body folding range"
    );
}

#[test]
fn test_folding_ranges_arrow_function() {
    let source = "\nconst fn = () => {\n  return 42;\n};\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding ranges for arrow function"
    );
    let arrow_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        arrow_range.is_some(),
        "Should find arrow function body folding range"
    );
}

#[test]
fn test_folding_ranges_constructor() {
    let source = "\nclass Foo {\n  constructor() {\n    this.x = 1;\n  }\n}\n";
    let ranges = get_ranges(source);
    // Should have class body + constructor body
    assert!(
        ranges.len() >= 2,
        "Should find folding ranges for class and constructor"
    );
}

#[test]
fn test_folding_ranges_getter_setter() {
    let source = "\nclass Foo {\n  get value() {\n    return this._v;\n  }\n  set value(v: number) {\n    this._v = v;\n  }\n}\n";
    let ranges = get_ranges(source);
    // class body + get body + set body
    assert!(
        ranges.len() >= 3,
        "Should find folding ranges for getter and setter, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_deeply_nested() {
    let source =
        "\nfunction a() {\n  function b() {\n    function c() {\n      return 1;\n    }\n  }\n}\n";
    let ranges = get_ranges(source);
    assert!(
        ranges.len() >= 3,
        "Should find folding ranges for nested functions, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_object_literal() {
    let source = "\nconst obj = {\n  a: 1,\n  b: 2,\n  c: 3\n};\n";
    let ranges = get_ranges(source);
    let obj_range = ranges.iter().find(|r| r.start_line <= 1 && r.end_line >= 4);
    assert!(
        obj_range.is_some(),
        "Should find folding range for multiline object literal"
    );
}

#[test]
fn test_folding_ranges_array_literal() {
    let source = "\nconst arr = [\n  1,\n  2,\n  3\n];\n";
    let ranges = get_ranges(source);
    let arr_range = ranges.iter().find(|r| r.start_line <= 1 && r.end_line >= 4);
    assert!(
        arr_range.is_some(),
        "Should find folding range for multiline array literal"
    );
}

#[test]
fn test_folding_ranges_try_catch() {
    let source = "\ntry {\n  foo();\n} catch (e) {\n  bar();\n} finally {\n  baz();\n}\n";
    let ranges = get_ranges(source);
    // Should have block ranges for try, catch, and finally
    assert!(
        !ranges.is_empty(),
        "Should find folding ranges for try/catch/finally"
    );
}

#[test]
fn test_folding_ranges_module_declaration() {
    let source = "\nnamespace MyModule {\n  export function hello() {\n    return 1;\n  }\n}\n";
    let ranges = get_ranges(source);
    assert!(
        ranges.len() >= 2,
        "Should find folding ranges for module and function inside, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_method_in_class() {
    let source = "\nclass Calculator {\n  add(a: number, b: number) {\n    return a + b;\n  }\n  multiply(a: number, b: number) {\n    return a * b;\n  }\n}\n";
    let ranges = get_ranges(source);
    // class body + 2 method bodies = at least 3
    assert!(
        ranges.len() >= 3,
        "Should have class body + 2 method body folds, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_function_expression() {
    let source = "\nconst handler = function() {\n  return true;\n};\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding range for function expression"
    );
}

#[test]
fn test_folding_ranges_multiline_type_alias() {
    let source =
        "\ntype Complex =\n  | { kind: 'a'; value: number }\n  | { kind: 'b'; value: string };\n";
    let ranges = get_ranges(source);
    let alias_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        alias_range.is_some(),
        "Should find folding range for multiline type alias"
    );
}

#[test]
fn test_folding_ranges_enum_with_computed_members() {
    let source = "\nenum Flags {\n  Read = 1 << 0,\n  Write = 1 << 1,\n  Execute = 1 << 2\n}\n";
    let ranges = get_ranges(source);
    let enum_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        enum_range.is_some(),
        "Should find folding range for enum with computed members"
    );
}

#[test]
fn test_folding_ranges_consecutive_three_comment_groups() {
    let source = "// group 1 line 1\n// group 1 line 2\nconst a = 1;\n// group 2 line 1\n// group 2 line 2\n// group 2 line 3\nconst b = 2;\n";
    let ranges = get_ranges(source);
    let comment_folds: Vec<&FoldingRange> = ranges
        .iter()
        .filter(|r| r.kind.as_deref() == Some("comment"))
        .collect();
    assert_eq!(
        comment_folds.len(),
        2,
        "Should find two separate comment groups, found {}",
        comment_folds.len()
    );
}

#[test]
fn test_folding_ranges_interface_with_many_properties() {
    let source = "\ninterface Config {\n  host: string;\n  port: number;\n  debug: boolean;\n  timeout: number;\n}\n";
    let ranges = get_ranges(source);
    let iface_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        iface_range.is_some(),
        "Should find folding range for interface with many properties"
    );
}

#[test]
fn test_folding_ranges_try_catch_finally() {
    let source =
        "try {\n  doSomething();\n} catch (e) {\n  handleError(e);\n} finally {\n  cleanup();\n}\n";
    let ranges = get_ranges(source);
    assert!(ranges.len() >= 1, "Should fold try/catch/finally blocks");
}

#[test]
fn test_folding_ranges_template_literal_multiline() {
    let source = "const html = `\n  <div>\n    <p>Hello</p>\n  </div>\n`;\n";
    let ranges = get_ranges(source);
    let _ = ranges;
}

#[test]
fn test_folding_ranges_class_with_decorators() {
    let source = "@Component({\n  selector: 'app'\n})\nclass AppComponent {\n  method() {\n    return true;\n  }\n}\n";
    let ranges = get_ranges(source);
    assert!(ranges.len() >= 1, "Should fold class with decorators");
}

#[test]
fn test_folding_ranges_single_line_no_fold() {
    let source = "const x = 1;";
    let ranges = get_ranges(source);
    assert!(ranges.is_empty(), "Single line should not produce folds");
}

#[test]
fn test_folding_ranges_arrow_function_body() {
    let source = "const process = (items: any[]) => {\n  return items\n    .filter(x => x)\n    .map(x => x * 2);\n};\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should fold arrow function body");
}

#[test]
fn test_folding_ranges_type_alias_object() {
    let source = "type Config = {\n  host: string;\n  port: number;\n  debug: boolean;\n};\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should fold type alias object literal");
}

#[test]
fn test_folding_ranges_mapped_type() {
    let source = "type Readonly<T> = {\n  readonly [K in keyof T]: T[K];\n};\n";
    let ranges = get_ranges(source);
    assert!(!ranges.is_empty(), "Should fold mapped type");
}

#[test]
fn test_folding_ranges_while_loop() {
    let source = "\nwhile (true) {\n  doWork();\n  if (done) break;\n}\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding range for while loop body"
    );
    let while_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        while_range.is_some(),
        "Should find while loop folding range at line 1"
    );
}

#[test]
fn test_folding_ranges_for_loop() {
    let source = "\nfor (let i = 0; i < 10; i++) {\n  console.log(i);\n}\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding range for for loop body"
    );
}

#[test]
fn test_folding_ranges_for_of_loop() {
    let source = "\nfor (const item of items) {\n  process(item);\n}\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding range for for-of loop"
    );
}

#[test]
fn test_folding_ranges_for_in_loop() {
    let source = "\nfor (const key in obj) {\n  console.log(key);\n}\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding range for for-in loop"
    );
}

#[test]
fn test_folding_ranges_do_while_loop() {
    let source = "\ndo {\n  attempt();\n} while (retry);\n";
    let ranges = get_ranges(source);
    assert!(
        !ranges.is_empty(),
        "Should find folding range for do-while loop"
    );
}

#[test]
fn test_folding_ranges_if_else() {
    let source = "\nif (a) {\n  doA();\n} else {\n  doB();\n}\n";
    let ranges = get_ranges(source);
    assert!(
        ranges.len() >= 2,
        "Should find folding ranges for both if and else blocks, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_multiple_imports_with_multiline_import() {
    let source =
        "import {\n  a,\n  b,\n  c\n} from \"mod\";\nimport { d } from \"other\";\nconst x = 1;\n";
    let ranges = get_ranges(source);
    // Should have at least an import group fold and the multiline import braces
    assert!(
        !ranges.is_empty(),
        "Should find folds for multiline import and import group"
    );
}

#[test]
fn test_folding_ranges_class_with_multiple_methods_and_properties() {
    let source = "\nclass Widget {\n  private name: string;\n  constructor(name: string) {\n    this.name = name;\n  }\n  render() {\n    return this.name;\n  }\n  dispose() {\n    this.name = '';\n  }\n}\n";
    let ranges = get_ranges(source);
    // class body + constructor + render + dispose = at least 4
    assert!(
        ranges.len() >= 4,
        "Should have class body + 3 method body folds, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_nested_object_literals() {
    let source = "\nconst config = {\n  server: {\n    host: 'localhost',\n    port: 3000\n  },\n  db: {\n    name: 'test'\n  }\n};\n";
    let ranges = get_ranges(source);
    // outer object + at least one inner object
    assert!(
        ranges.len() >= 2,
        "Should find folds for nested objects, got {}",
        ranges.len()
    );
}

#[test]
fn test_folding_ranges_whitespace_only_source() {
    let source = "   \n   \n   \n";
    let ranges = get_ranges(source);
    assert!(
        ranges.is_empty(),
        "Whitespace-only source should not produce folds"
    );
}

#[test]
fn test_folding_ranges_enum_const() {
    let source = "\nconst enum Direction {\n  Up,\n  Down,\n  Left,\n  Right\n}\n";
    let ranges = get_ranges(source);
    let enum_range = ranges.iter().find(|r| r.start_line == 1);
    assert!(
        enum_range.is_some(),
        "Should find folding range for const enum"
    );
}

#[test]
fn test_folding_ranges_interface_extending_multiple() {
    let source = "\ninterface A {\n  a: number;\n}\ninterface B {\n  b: string;\n}\ninterface C extends A, B {\n  c: boolean;\n}\n";
    let ranges = get_ranges(source);
    // 3 interface bodies
    assert!(
        ranges.len() >= 3,
        "Should find folds for all three interfaces, got {}",
        ranges.len()
    );
}
