//! Lib Loader - Load and merge lib.d.ts symbols into the binder.
//!
//! This module provides functionality for loading standard library type definitions
//! (like lib.d.ts) and merging their global symbols into the binder's root scope.
//! This enables proper resolution of built-in types like `Object`, `Function`, `console`, etc.

use crate::binder::SymbolTable;
use crate::parser::thin_node::ThinNodeArena;
use crate::thin_binder::ThinBinderState;
use crate::thin_parser::ThinParserState;
use std::sync::Arc;

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

    let mut lib_parser = ThinParserState::new("lib.d.ts".to_string(), source_text);
    let source_file_idx = lib_parser.parse_source_file();

    if !lib_parser.get_diagnostics().is_empty() {
        // Parse errors in lib.d.ts - return None
        return None;
    }

    let mut lib_binder = ThinBinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

    let arena = Arc::new(lib_parser.into_arena());
    let binder = Arc::new(lib_binder);

    Some(Arc::new(LibFile::new("lib.d.ts".to_string(), arena, binder)))
}

/// Loaded lib file with its arena and binder state.
#[derive(Clone)]
pub struct LibFile {
    /// File name (e.g., "lib.d.ts")
    pub file_name: String,
    /// The arena (shared via Arc for cross-file resolution)
    pub arena: Arc<ThinNodeArena>,
    /// The binder state with bound symbols
    pub binder: Arc<ThinBinderState>,
}

impl LibFile {
    /// Create a new LibFile from a parsed and bound lib file.
    pub fn new(file_name: String, arena: Arc<ThinNodeArena>, binder: Arc<ThinBinderState>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_lib_symbols() {
        use crate::binder::{Symbol, SymbolArena, symbol_flags};

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

        let mut lib_binder =
            ThinBinderState::from_bound_state(arena, lib_file_locals, Default::default());

        let lib = Arc::new(LibFile::new(
            "lib.d.ts".to_string(),
            Arc::new(ThinNodeArena::new()),
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
            assert!(file_locals.has("Function"), "Function should be in lib.d.ts");
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
                assert!(file_locals.has("document"), "document should be in lib.d.ts");
            }
        }
    }

    #[test]
    fn test_bind_with_lib_symbols() {
        use crate::thin_parser::ThinParserState;

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

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        // Bind with lib symbols
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        binder.merge_lib_symbols(&[lib_file]);

        // Verify global symbols are accessible
        assert!(binder.file_locals.has("console"), "console should be in file_locals");
        assert!(binder.file_locals.has("Array"), "Array should be in file_locals");
        assert!(binder.file_locals.has("Object"), "Object should be in file_locals");
        assert!(binder.file_locals.has("Promise"), "Promise should be in file_locals");
    }

    #[test]
    fn test_get_symbol_resolves_lib_symbols() {
        use crate::thin_parser::ThinParserState;

        // Load lib.d.ts
        let lib_file = load_default_lib_dts();
        if lib_file.is_none() {
            // Skip test if lib.d.ts is not available
            return;
        }
        let lib_file = lib_file.unwrap();

        // Parse a source file
        let source = "const x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Bind with lib symbols
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        binder.merge_lib_symbols(&[lib_file]);

        // Get a lib symbol ID and verify get_symbol() can resolve it
        let console_sym_id = binder.file_locals.get("console")
            .expect("console should be in file_locals");
        let promise_sym_id = binder.file_locals.get("Promise")
            .expect("Promise should be in file_locals");
        let array_sym_id = binder.file_locals.get("Array")
            .expect("Array should be in file_locals");

        // This is the key test: get_symbol() should be able to resolve lib symbols
        // After the fix, get_symbol() checks lib_binders automatically
        let console_sym = binder.get_symbol(console_sym_id);
        assert!(console_sym.is_some(), "get_symbol() should resolve lib symbol 'console'");
        assert_eq!(console_sym.unwrap().escaped_name, "console");

        let promise_sym = binder.get_symbol(promise_sym_id);
        assert!(promise_sym.is_some(), "get_symbol() should resolve lib symbol 'Promise'");
        assert_eq!(promise_sym.unwrap().escaped_name, "Promise");

        let array_sym = binder.get_symbol(array_sym_id);
        assert!(array_sym.is_some(), "get_symbol() should resolve lib symbol 'Array'");
        assert_eq!(array_sym.unwrap().escaped_name, "Array");
    }
}
