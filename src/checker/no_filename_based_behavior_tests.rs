//! Tests verifying checker behavior is independent of file names
//!
//! These tests ensure that the type checker's behavior is controlled by
//! explicit APIs and compiler options, not by file naming patterns.

use crate::Parser;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;

/// Test that WasmProgram treats lib files explicitly via addLibFile
/// and not via file name detection
///
/// This documents the expected behavior:
/// - Files added via addFile are always treated as user files,
///   regardless of their name
/// - Only files added via addLibFile are treated as library files
/// - No automatic file name pattern detection occurs
#[test]
fn test_lib_file_detection_is_explicit() {
    // This test documents the API contract
    // The actual behavior is tested via JavaScript integration tests
    // in the conformance suite

    // Key principles:
    // 1. addFile("lib.d.ts", content) -> treated as USER file
    // 2. addLibFile("lib.d.ts", content) -> treated as LIBRARY file
    // 3. File names don't affect behavior, only the API used

    // This separation makes the API explicit and predictable,
    // removing any magic behavior based on file naming patterns.
}

/// Test that parsing works consistently regardless of file name
#[test]
fn test_parsing_is_independent_of_filename() {
    let source = r#"
class MyClass {
    async method() {
        return Promise.resolve(42);
    }
}
"#;

    // Test with various file names that previously might have triggered
    // special behavior in other compilers
    let test_filenames = [
        "test.ts",             // Generic name
        "classTest.ts",        // Contains "class"
        "myClass.ts",          // Contains "Class"
        "methodTests.ts",      // Contains "method"
        "MyMethod.ts",         // Contains "Method"
        "some_random_file.ts", // No special keywords
    ];

    for filename in test_filenames {
        let mut parser = Parser::new(filename.to_string(), source.to_string());
        let _ = parser.parse_source_file();

        let arena = parser.parser.into_arena();

        // Find the class declaration node
        let class_count = count_nodes_by_kind(&arena, syntax_kind_ext::CLASS_DECLARATION);

        // We should find exactly 1 class in all cases
        // regardless of filename
        assert_eq!(
            class_count, 1,
            "Should find exactly 1 class in {}, got {}",
            filename, class_count
        );

        // Find method nodes
        let method_count = count_nodes_by_kind(&arena, syntax_kind_ext::FUNCTION_DECLARATION);

        // We should find exactly 1 method in all cases
        assert_eq!(
            method_count, 1,
            "Should find exactly 1 method in {}, got {}",
            filename, method_count
        );
    }
}

/// Test that keywords in filenames don't affect parsing
#[test]
fn test_filename_keywords_dont_affect_ast() {
    let source = r#"
// Standalone function (not in a class)
function standaloneFunction() {
    return 42;
}

// Class with method
class Example {
    method() {
        return 123;
    }
}
"#;

    // These filenames contain keywords that were previously checked
    let filenames_with_keywords = [
        "classStandalone.ts", // Contains "class"
        "MethodTest.ts",      // Contains "Method"
        "classAndMethod.ts",  // Contains both
    ];

    for filename in filenames_with_keywords {
        let mut parser = Parser::new(filename.to_string(), source.to_string());
        let _ = parser.parse_source_file();

        let arena = parser.parser.into_arena();

        // Should find exactly 1 class regardless of filename
        let class_count = count_nodes_by_kind(&arena, syntax_kind_ext::CLASS_DECLARATION);
        assert_eq!(
            class_count, 1,
            "Should find exactly 1 class in {}, got {}",
            filename, class_count
        );

        // Should find exactly 2 functions (standalone + method)
        let func_count = count_nodes_by_kind(&arena, syntax_kind_ext::FUNCTION_DECLARATION);
        assert_eq!(
            func_count, 2,
            "Should find exactly 2 functions in {}, got {}",
            filename, func_count
        );
    }
}

// Helper functions for tests

fn count_nodes_by_kind(arena: &NodeArena, kind: u16) -> usize {
    let mut count = 0;
    for i in 0..arena.len() {
        let idx = crate::parser::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx) {
            if node.kind == kind {
                count += 1;
            }
        }
    }
    count
}
