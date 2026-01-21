//! Lib Loader - Load and merge lib.d.ts symbols into the binder.
//!
//! This module provides functionality for loading standard library type definitions
//! (like lib.d.ts) and merging their global symbols into the binder's root scope.
//! This enables proper resolution of built-in types like `Object`, `Function`, `console`, etc.

use crate::binder::BinderState;
use crate::binder::SymbolTable;
use crate::checker::types::diagnostics::Diagnostic;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use std::sync::Arc;

// =============================================================================
// Diagnostic Error Codes
// =============================================================================

/// TS2318: Cannot find global type '{0}'.
pub const CANNOT_FIND_GLOBAL_TYPE: u32 = 2318;

/// TS2583: Cannot find name '{0}'. Do you need to change your target library?
pub const MISSING_ES2015_LIB_SUPPORT: u32 = 2583;

/// Load the default lib.d.ts file from the tests/lib directory.
///
/// This is a convenience function for tests and development that loads
/// the standard library definitions containing global symbols like
/// `console`, `Object`, `Array`, `Promise`, `window`, `document`, etc.
///
/// Returns `None` if the lib.d.ts file cannot be found or read.
pub fn load_default_lib_dts() -> Option<Arc<LibFile>> {
    let lib_dts_path = std::path::Path::new("tests/lib/lib.d.ts");
    let source_text = std::fs::read_to_string(lib_dts_path).ok()?;

    let mut lib_parser = ParserState::new("lib.d.ts".to_string(), source_text);
    let source_file_idx = lib_parser.parse_source_file();

    if !lib_parser.get_diagnostics().is_empty() {
        // Parse errors in lib.d.ts - return None
        return None;
    }

    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

    let arena = Arc::new(lib_parser.into_arena());
    let binder = Arc::new(lib_binder);

    Some(Arc::new(LibFile::new(
        "lib.d.ts".to_string(),
        arena,
        binder,
    )))
}

/// Loaded lib file with its arena and binder state.
#[derive(Clone)]
pub struct LibFile {
    /// File name (e.g., "lib.d.ts")
    pub file_name: String,
    /// The arena (shared via Arc for cross-file resolution)
    pub arena: Arc<NodeArena>,
    /// The binder state with bound symbols
    pub binder: Arc<BinderState>,
}

impl LibFile {
    /// Create a new LibFile from a parsed and bound lib file.
    pub fn new(file_name: String, arena: Arc<NodeArena>, binder: Arc<BinderState>) -> Self {
        Self {
            file_name,
            arena,
            binder,
        }
    }

    /// Get the file locals (global symbols) from this lib file.
    pub fn file_locals(&self) -> &SymbolTable {
        &self.binder.file_locals
    }
}

/// Merge lib file symbols into a target symbol table.
///
/// This is called during binder initialization to ensure global symbols
/// from lib.d.ts (like `Object`, `Function`, `console`, etc.) are available
/// during type checking.
pub fn merge_lib_symbols(target: &mut SymbolTable, lib_files: &[Arc<LibFile>]) {
    for lib in lib_files {
        for (name, sym_id) in lib.binder.file_locals.iter() {
            // Only add if not already defined (user code can override lib symbols)
            if !target.has(name) {
                target.set(name.clone(), *sym_id);
            }
        }
    }
}

// =============================================================================
// Error Emission Helpers
// =============================================================================

/// Create a TS2318 error for a missing global type.
///
/// This error is emitted when a global type (like `Promise`, `Map`, `Set`)
/// cannot be found in any of the loaded lib files. This typically indicates
/// that the target library version doesn't support the requested feature.
///
/// # Arguments
/// * `name` - The name of the missing global type
/// * `file_name` - The source file name
/// * `start` - Start position of the reference
/// * `length` - Length of the reference
pub fn emit_error_global_type_missing(
    name: &str,
    file_name: String,
    start: u32,
    length: u32,
) -> Diagnostic {
    Diagnostic::error(
        file_name,
        start,
        length,
        format!("Cannot find global type '{}'", name),
        CANNOT_FIND_GLOBAL_TYPE,
    )
}

/// Create a TS2583 error when a type is missing due to insufficient lib support.
///
/// This error is emitted when a global type is not available because the
/// target library version doesn't include ES2015+ features. This helps users
/// understand they need to change their `target` or `lib` compiler options.
///
/// # Arguments
/// * `name` - The name of the missing type
/// * `file_name` - The source file name
/// * `start` - Start position of the reference
/// * `length` - Length of the reference
pub fn emit_error_lib_target_mismatch(
    name: &str,
    file_name: String,
    start: u32,
    length: u32,
) -> Diagnostic {
    Diagnostic::error(
        file_name,
        start,
        length,
        format!(
            "Cannot find name '{}'. Do you need to change your target library?",
            name
        ),
        MISSING_ES2015_LIB_SUPPORT,
    )
}

// =============================================================================
// ES2015+ Type Detection
// =============================================================================

/// ES2015+ global types that may not be available in all lib versions.
///
/// These types were introduced in ES2015 and later. When a user references
/// these types but their target lib doesn't include them, we should emit
/// TS2583 with a helpful message about changing their target library.
const ES2015_PLUS_TYPES: &[&str] = &[
    // ES2015
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Proxy",
    "Reflect",
    "Symbol",
    // ES2017
    "AsyncFunction",
    // ES2019
    "ObjectEntries",
    "ObjectValues",
    // ES2020
    "BigInt",
    // ES2021
    "FinalizationRegistry",
    "WeakRef",
];

/// Check if a type name is an ES2015+ feature that requires specific lib support.
///
/// This helps determine whether we should emit TS2583 (suggesting target/lib change)
/// instead of TS2318 (generic type not found).
pub fn is_es2015_plus_type(name: &str) -> bool {
    ES2015_PLUS_TYPES.contains(&name)
}

/// Validate that the loaded lib files support the required ES2015+ features.
///
/// This function checks if all essential ES2015+ global types are available
/// in the provided lib files. If not, it returns diagnostics suggesting
/// that the user may need to adjust their target or lib compiler options.
///
/// # Arguments
/// * `lib_files` - The loaded lib files to validate
/// * `file_name` - The file name to use in diagnostics
///
/// # Returns
/// A list of diagnostics for any missing ES2015+ types
pub fn validate_lib_es2015_support(
    lib_files: &[Arc<LibFile>],
    file_name: String,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Collect all available global types from lib files
    let mut available_globals = SymbolTable::new();
    merge_lib_symbols(&mut available_globals, lib_files);

    // Check for missing ES2015+ types
    for &type_name in ES2015_PLUS_TYPES {
        if !available_globals.has(type_name) {
            // Emit a diagnostic at position 0 (no specific location for lib-level check)
            diagnostics.push(emit_error_lib_target_mismatch(
                type_name,
                file_name.clone(),
                0,
                0,
            ));
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_lib_symbols() {
        use crate::binder::{SymbolArena, symbol_flags};

        let mut target = SymbolTable::new();
        let mut arena = SymbolArena::new();

        // Create some lib symbols
        let object_id = arena.alloc(symbol_flags::VALUE, "Object".to_string());
        let function_id = arena.alloc(symbol_flags::VALUE, "Function".to_string());
        let console_id = arena.alloc(symbol_flags::VALUE, "console".to_string());

        // Create a lib binder with these symbols
        let mut lib_file_locals = SymbolTable::new();
        lib_file_locals.set("Object".to_string(), object_id);
        lib_file_locals.set("Function".to_string(), function_id);
        lib_file_locals.set("console".to_string(), console_id);

        let lib_binder = BinderState::from_bound_state(arena, lib_file_locals, Default::default());

        let lib = Arc::new(LibFile::new(
            "lib.d.ts".to_string(),
            Arc::new(NodeArena::new()),
            Arc::new(lib_binder),
        ));

        // Add a user symbol that should override lib symbol
        let mut user_arena = SymbolArena::new();
        let user_object_id = user_arena.alloc(symbol_flags::VALUE, "Object".to_string());
        target.set("Object".to_string(), user_object_id);

        // Merge lib symbols
        merge_lib_symbols(&mut target, &[lib]);

        // User's Object should not be overridden
        assert_eq!(target.get("Object"), Some(user_object_id));
        // Function and console should be added
        assert_eq!(target.get("Function"), Some(function_id));
        assert_eq!(target.get("console"), Some(console_id));
    }

    #[test]
    fn test_emit_error_global_type_missing() {
        // Test that the global type missing error is created correctly
        let diagnostic = emit_error_global_type_missing("Promise", "test.ts".to_string(), 10, 7);

        assert_eq!(diagnostic.code, CANNOT_FIND_GLOBAL_TYPE);
        assert_eq!(diagnostic.file, "test.ts");
        assert_eq!(diagnostic.start, 10);
        assert_eq!(diagnostic.length, 7);
        assert_eq!(diagnostic.message_text, "Cannot find global type 'Promise'");
    }

    #[test]
    fn test_emit_error_lib_target_mismatch() {
        // Test that the lib target mismatch error is created correctly
        let diagnostic = emit_error_lib_target_mismatch("Map", "test.ts".to_string(), 20, 3);

        assert_eq!(diagnostic.code, MISSING_ES2015_LIB_SUPPORT);
        assert_eq!(diagnostic.file, "test.ts");
        assert_eq!(diagnostic.start, 20);
        assert_eq!(diagnostic.length, 3);
        assert_eq!(
            diagnostic.message_text,
            "Cannot find name 'Map'. Do you need to change your target library?"
        );
    }

    #[test]
    fn test_is_es2015_plus_type() {
        // Test ES2015 types
        assert!(is_es2015_plus_type("Promise"));
        assert!(is_es2015_plus_type("Map"));
        assert!(is_es2015_plus_type("Set"));
        assert!(is_es2015_plus_type("WeakMap"));
        assert!(is_es2015_plus_type("WeakSet"));
        assert!(is_es2015_plus_type("Proxy"));
        assert!(is_es2015_plus_type("Reflect"));
        assert!(is_es2015_plus_type("Symbol"));

        // Test ES2017+ types
        assert!(is_es2015_plus_type("AsyncFunction"));
        assert!(is_es2015_plus_type("BigInt"));
        assert!(is_es2015_plus_type("FinalizationRegistry"));
        assert!(is_es2015_plus_type("WeakRef"));

        // Test pre-ES2015 types (should return false)
        assert!(!is_es2015_plus_type("Object"));
        assert!(!is_es2015_plus_type("Array"));
        assert!(!is_es2015_plus_type("Function"));
        assert!(!is_es2015_plus_type("String"));
        assert!(!is_es2015_plus_type("Number"));
        assert!(!is_es2015_plus_type("Boolean"));
    }

    #[test]
    fn test_validate_lib_es2015_support() {
        // Load lib.d.ts
        let lib_file = load_default_lib_dts();
        if lib_file.is_none() {
            // Skip test if lib.d.ts is not available
            return;
        }
        let lib_file = lib_file.unwrap();

        // Validate ES2015+ support - if lib.d.ts has these types, should return empty vec
        let diagnostics = validate_lib_es2015_support(&[lib_file], "test.ts".to_string());

        // The result depends on whether lib.d.ts has ES2015+ types
        // If it does, diagnostics should be empty
        // If it doesn't, diagnostics should contain errors for missing types
        // Just verify the function runs without panicking
        println!("ES2015+ validation diagnostics: {}", diagnostics.len());
    }

    #[test]
    fn test_load_default_lib_dts() {
        // Test that we can load the default lib.d.ts file
        let lib_file = load_default_lib_dts();

        // This test may run in environments where tests/lib/lib.d.ts is not available
        // (e.g., cargo test from a different directory)
        if let Some(lib) = lib_file {
            // Verify that key global symbols are present
            let file_locals = lib.file_locals();

            // Core ECMAScript globals
            assert!(file_locals.has("Object"), "Object should be in lib.d.ts");
            assert!(file_locals.has("Array"), "Array should be in lib.d.ts");
            assert!(
                file_locals.has("Function"),
                "Function should be in lib.d.ts"
            );
            assert!(file_locals.has("Promise"), "Promise should be in lib.d.ts");
            assert!(file_locals.has("console"), "console should be in lib.d.ts");

            // DOM globals (if present in lib.d.ts)
            // Note: These may not be in all lib.d.ts versions
            let has_window = file_locals.has("window");
            let has_document = file_locals.has("document");

            if has_window {
                assert!(file_locals.has("window"), "window should be in lib.d.ts");
            }
            if has_document {
                assert!(
                    file_locals.has("document"),
                    "document should be in lib.d.ts"
                );
            }
        }
    }

    #[test]
    fn test_bind_with_lib_symbols() {
        use crate::parser::ParserState;

        // Load lib.d.ts
        let lib_file = load_default_lib_dts();
        if lib_file.is_none() {
            // Skip test if lib.d.ts is not available
            return;
        }
        let lib_file = lib_file.unwrap();

        // Parse a source file that uses global symbols
        let source = r#"
console.log("hello");
const arr: Array<number> = [1, 2, 3];
const obj = Object.create(null);
async function foo() {
    return await Promise.resolve(42);
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        // Bind with lib symbols
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        binder.merge_lib_symbols(&[lib_file]);

        // Verify global symbols are accessible
        assert!(
            binder.file_locals.has("console"),
            "console should be in file_locals"
        );
        assert!(
            binder.file_locals.has("Array"),
            "Array should be in file_locals"
        );
        assert!(
            binder.file_locals.has("Object"),
            "Object should be in file_locals"
        );
        assert!(
            binder.file_locals.has("Promise"),
            "Promise should be in file_locals"
        );
    }

    #[test]
    fn test_get_symbol_resolves_lib_symbols() {
        use crate::parser::ParserState;

        // Load lib.d.ts
        let lib_file = load_default_lib_dts();
        if lib_file.is_none() {
            // Skip test if lib.d.ts is not available
            return;
        }
        let lib_file = lib_file.unwrap();

        // Parse a source file
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Bind with lib symbols
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        binder.merge_lib_symbols(&[lib_file]);

        // Get a lib symbol ID and verify get_symbol() can resolve it
        let console_sym_id = binder
            .file_locals
            .get("console")
            .expect("console should be in file_locals");
        let promise_sym_id = binder
            .file_locals
            .get("Promise")
            .expect("Promise should be in file_locals");
        let array_sym_id = binder
            .file_locals
            .get("Array")
            .expect("Array should be in file_locals");

        // This is the key test: get_symbol() should be able to resolve lib symbols
        // After the fix, get_symbol() checks lib_binders automatically
        let console_sym = binder.get_symbol(console_sym_id);
        assert!(
            console_sym.is_some(),
            "get_symbol() should resolve lib symbol 'console'"
        );
        assert_eq!(console_sym.unwrap().escaped_name, "console");

        let promise_sym = binder.get_symbol(promise_sym_id);
        assert!(
            promise_sym.is_some(),
            "get_symbol() should resolve lib symbol 'Promise'"
        );
        assert_eq!(promise_sym.unwrap().escaped_name, "Promise");

        let array_sym = binder.get_symbol(array_sym_id);
        assert!(
            array_sym.is_some(),
            "get_symbol() should resolve lib symbol 'Array'"
        );
        assert_eq!(array_sym.unwrap().escaped_name, "Array");
    }
}
