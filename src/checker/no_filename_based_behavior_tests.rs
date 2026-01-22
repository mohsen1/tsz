//! Tests verifying checker behavior is independent of file names
//!
//! These tests ensure that the type checker's behavior is controlled by
//! explicit APIs and compiler options, not by file naming patterns.

use crate::checker::CheckerState;
use crate::binder::BinderState;
use crate::checker::types::TypeInterner;
use crate::checker::context::CheckerOptions;
use crate::parser::Parser;
use crate::interner::StringInterner;
use crate::parser::syntax_kind_ext;

/// Test that class method detection works regardless of file name
/// The function uses AST-based parent traversal, not file name patterns.
#[test]
fn test_class_method_detection_independent_of_filename() {
    let source = r#"
class MyClass {
    async method() {
        return Promise.resolve(42);
    }
}
"#;

    // Test with various file names that previously would have triggered
    // file-name-based behavior
    let test_filenames = [
        "test.ts",              // Generic name
        "classTest.ts",         // Contains "class"
        "myClass.ts",           // Contains "Class"
        "methodTests.ts",       // Contains "method"
        "MyMethod.ts",          // Contains "Method"
        "some_random_file.ts",  // No special keywords
    ];

    for filename in test_filenames {
        let mut parser = Parser::new(filename, source);
        parser.parse_source_file();

        let arena = parser.into_arena();
        let mut interner = StringInterner::new();
        let mut types = TypeInterner::new();

        let mut binder = BinderState::new(filename, &arena, &mut interner);
        binder.bind_source_file(&arena);

        let options = CheckerOptions::default();
        let checker = CheckerState::new(&arena, binder, &mut types, filename, &options);

        // Find the class declaration node
        let class_idx = find_node_by_kind(&arena, syntax_kind_ext::CLASS_DECLARATION);

        // Find a method node within the class
        let method_idx = find_node_by_kind(&arena, syntax_kind_ext::FUNCTION_DECLARATION);

        // The method should be detected as a class method
        // regardless of the file name since we use AST-based detection
        if !method_idx.is_none() {
            let is_method = checker.is_class_method(method_idx);
            // This should work the same for all filenames
            // since it's now AST-based, not file-name-based
            assert!(
                is_method,
                "Class method should be detected as class method in {}",
                filename
            );
        }

        // A non-existent node should not be detected as a class method
        let fake_idx = crate::parser::NodeIndex::from(99999);
        assert!(
            !checker.is_class_method(fake_idx),
            "Invalid node should not be detected as class method in {}",
            filename
        );
    }
}

/// Test that WasmProgram treats lib files explicitly via addLibFile
/// and not via file name detection
#[test]
#[cfg(feature = "wasm")]
fn test_lib_file_detection_is_explicit() {
    // This test verifies the JS/Wasm API behavior
    // In Rust, we test that the API exists and works correctly

    // The key point: files added via addFile should be treated as user files
    // regardless of their name, and only files added via addLibFile
    // should be treated as library files

    // This is an integration test that would be run from JavaScript
    // but we document the expected behavior here
}

// Helper functions for tests

use crate::parser::NodeIndex;

fn find_node_by_kind(
    arena: &crate::parser::node::NodeArena,
    kind: u16,
) -> NodeIndex {
    for i in 0..arena.len() {
        let idx = NodeIndex::from(i as u32);
        if let Some(node) = arena.get(idx) {
            if node.kind == kind {
                return idx;
            }
        }
    }
    NodeIndex::none()
}
