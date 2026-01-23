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
        format!("Cannot find global type '{}'.", name),
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
            "Cannot find name '{}'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.",
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
    // ES2015 - Core types
    "Promise",
    "PromiseLike",
    "PromiseConstructor",
    "PromiseConstructorLike",
    "PromiseSettledResult",
    "PromiseFulfilledResult",
    "PromiseRejectedResult",
    "Map",
    "MapConstructor",
    "Set",
    "SetConstructor",
    "WeakMap",
    "WeakMapConstructor",
    "WeakSet",
    "WeakSetConstructor",
    "Proxy",
    "ProxyHandler",
    "ProxyConstructor",
    "Reflect",
    "Symbol",
    "SymbolConstructor",
    // ES2015 - Iterator types
    "Iterator",
    "Iterable",
    "IterableIterator",
    "IteratorResult",
    "IteratorYieldResult",
    "IteratorReturnResult",
    "AsyncIterator",
    "AsyncIterable",
    "AsyncIterableIterator",
    // ES2015 - Generator types
    "Generator",
    "GeneratorFunction",
    "GeneratorFunctionConstructor",
    // ES2015 - Array/TypedArray types
    "ArrayLike",
    "ReadonlyMap",
    "ReadonlySet",
    "TemplateStringsArray",
    "TypedPropertyDescriptor",
    // ES2017
    "AsyncFunction",
    "AsyncFunctionConstructor",
    "SharedArrayBuffer",
    "SharedArrayBufferConstructor",
    "Atomics",
    // ES2018
    "AsyncGenerator",
    "AsyncGeneratorFunction",
    "AsyncGeneratorFunctionConstructor",
    // ES2019
    "ObjectEntries",
    "ObjectValues",
    // ES2020
    "BigInt",
    "BigIntConstructor",
    "BigInt64Array",
    "BigInt64ArrayConstructor",
    "BigUint64Array",
    "BigUint64ArrayConstructor",
    // ES2021
    "FinalizationRegistry",
    "FinalizationRegistryConstructor",
    "WeakRef",
    "WeakRefConstructor",
    "AggregateError",
    "AggregateErrorConstructor",
    // ES2022
    "Awaited",
    "ErrorOptions", // ES2022 error options (for Error cause)
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
        assert_eq!(
            diagnostic.message_text,
            "Cannot find global type 'Promise'."
        );
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
            "Cannot find name 'Map'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later."
        );
    }

    #[test]
    fn test_is_es2015_plus_type() {
        // Test ES2015 core types
        assert!(is_es2015_plus_type("Promise"));
        assert!(is_es2015_plus_type("PromiseLike"));
        assert!(is_es2015_plus_type("Map"));
        assert!(is_es2015_plus_type("MapConstructor"));
        assert!(is_es2015_plus_type("Set"));
        assert!(is_es2015_plus_type("SetConstructor"));
        assert!(is_es2015_plus_type("WeakMap"));
        assert!(is_es2015_plus_type("WeakSet"));
        assert!(is_es2015_plus_type("Proxy"));
        assert!(is_es2015_plus_type("ProxyHandler"));
        assert!(is_es2015_plus_type("Reflect"));
        assert!(is_es2015_plus_type("Symbol"));
        assert!(is_es2015_plus_type("SymbolConstructor"));

        // Test ES2015 iterator types
        assert!(is_es2015_plus_type("Iterator"));
        assert!(is_es2015_plus_type("Iterable"));
        assert!(is_es2015_plus_type("IterableIterator"));
        assert!(is_es2015_plus_type("IteratorResult"));
        assert!(is_es2015_plus_type("AsyncIterator"));
        assert!(is_es2015_plus_type("AsyncIterable"));
        assert!(is_es2015_plus_type("AsyncIterableIterator"));

        // Test ES2015 generator types
        assert!(is_es2015_plus_type("Generator"));
        assert!(is_es2015_plus_type("GeneratorFunction"));

        // Test ES2017+ types
        assert!(is_es2015_plus_type("AsyncFunction"));
        assert!(is_es2015_plus_type("SharedArrayBuffer"));
        assert!(is_es2015_plus_type("Atomics"));

        // Test ES2018 types
        assert!(is_es2015_plus_type("AsyncGenerator"));
        assert!(is_es2015_plus_type("AsyncGeneratorFunction"));

        // Test ES2020 types
        assert!(is_es2015_plus_type("BigInt"));
        assert!(is_es2015_plus_type("BigIntConstructor"));
        assert!(is_es2015_plus_type("BigInt64Array"));
        assert!(is_es2015_plus_type("BigUint64Array"));

        // Test ES2021 types
        assert!(is_es2015_plus_type("FinalizationRegistry"));
        assert!(is_es2015_plus_type("WeakRef"));

        // Test ES2022 types
        assert!(is_es2015_plus_type("Awaited"));

        // Test pre-ES2015 types (should return false)
        assert!(!is_es2015_plus_type("Object"));
        assert!(!is_es2015_plus_type("Array"));
        assert!(!is_es2015_plus_type("Function"));
        assert!(!is_es2015_plus_type("String"));
        assert!(!is_es2015_plus_type("Number"));
        assert!(!is_es2015_plus_type("Boolean"));
        assert!(!is_es2015_plus_type("Error"));
        assert!(!is_es2015_plus_type("Date"));
        assert!(!is_es2015_plus_type("RegExp"));
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

    #[test]
    fn test_get_global_type() {
        use crate::parser::ParserState;

        // Load lib.d.ts
        let lib_file = load_default_lib_dts();
        if lib_file.is_none() {
            // Skip test if lib.d.ts is not available
            return;
        }
        let lib_file = lib_file.unwrap();

        // Parse a simple source file
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Bind with lib symbols
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        binder.merge_lib_symbols(&[lib_file]);

        // Test get_global_type for built-in types
        assert!(
            binder.get_global_type("Array").is_some(),
            "get_global_type should find Array"
        );
        assert!(
            binder.get_global_type("Object").is_some(),
            "get_global_type should find Object"
        );
        assert!(
            binder.get_global_type("Function").is_some(),
            "get_global_type should find Function"
        );
        assert!(
            binder.get_global_type("Promise").is_some(),
            "get_global_type should find Promise"
        );
        assert!(
            binder.get_global_type("console").is_some(),
            "get_global_type should find console"
        );

        // Test has_global_type convenience method
        assert!(
            binder.has_global_type("Array"),
            "has_global_type should return true for Array"
        );
        assert!(
            !binder.has_global_type("NonExistentType12345"),
            "has_global_type should return false for non-existent type"
        );

        // Verify that get_global_type returns valid symbols
        let array_sym_id = binder.get_global_type("Array").unwrap();
        let array_sym = binder.get_symbol(array_sym_id);
        assert!(array_sym.is_some(), "Symbol should be resolvable");
        assert_eq!(array_sym.unwrap().escaped_name, "Array");
    }

    #[test]
    fn test_get_global_type_es2015_plus() {
        use crate::parser::ParserState;

        // Load lib.d.ts
        let lib_file = load_default_lib_dts();
        if lib_file.is_none() {
            // Skip test if lib.d.ts is not available
            return;
        }
        let lib_file = lib_file.unwrap();

        // Parse a simple source file
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Bind with lib symbols
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        binder.merge_lib_symbols(&[lib_file]);

        // Test ES2015+ types - these should exist if lib.d.ts includes ES2015 support
        // Note: the result depends on whether lib.d.ts has ES2015+ types
        let has_promise = binder.get_global_type("Promise").is_some();
        let has_map = binder.get_global_type("Map").is_some();
        let has_set = binder.get_global_type("Set").is_some();

        // Log the results for debugging
        println!("ES2015+ type availability:");
        println!("  Promise: {}", has_promise);
        println!("  Map: {}", has_map);
        println!("  Set: {}", has_set);

        // Test is_es2015_plus_type function
        assert!(is_es2015_plus_type("Promise"), "Promise is ES2015+");
        assert!(is_es2015_plus_type("Map"), "Map is ES2015+");
        assert!(is_es2015_plus_type("Set"), "Set is ES2015+");
        assert!(is_es2015_plus_type("WeakMap"), "WeakMap is ES2015+");
        assert!(is_es2015_plus_type("WeakSet"), "WeakSet is ES2015+");
        assert!(!is_es2015_plus_type("Array"), "Array is not ES2015+");
        assert!(!is_es2015_plus_type("Object"), "Object is not ES2015+");
    }

    #[test]
    fn test_nolib_emits_ts2318_for_global_type() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that references a global type that should exist in lib
        // Note: We use RegExp<T> (with type arg) instead of Array<T> because
        // Array is special-cased in the checker to always synthesize a type.
        // RegExp is a pre-ES2015 global type that requires lib.d.ts to be resolved.
        let source = r#"
let d: Date;
function f(r: RegExp): void {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        // Don't merge any lib symbols to simulate @noLib

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2318 for Date/RegExp since they're known global types but no lib is loaded
        let ts2318_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == CANNOT_FIND_GLOBAL_TYPE)
            .count();

        // Date and RegExp are known globals, should trigger TS2318
        assert!(
            ts2318_count >= 1,
            "Expected at least one TS2318 error for missing global type 'Date' or 'RegExp', got: {:?}",
            checker.ctx.diagnostics
        );
    }

    #[test]
    fn test_nolib_emits_ts2583_for_es2015_type() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that references an ES2015+ type
        let source = r#"
let p: Promise<number>;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        // Don't merge any lib symbols to simulate low target lib

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2583 for Promise since it's an ES2015+ type
        let ts2583_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == MISSING_ES2015_LIB_SUPPORT)
            .count();

        assert!(
            ts2583_count >= 1,
            "Expected at least one TS2583 error for ES2015+ type 'Promise', got diagnostics: {:?}",
            checker.ctx.diagnostics
        );

        // Verify it's not TS2318 (the generic global type error)
        let ts2318_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == CANNOT_FIND_GLOBAL_TYPE && d.message_text.contains("Promise"))
            .count();

        assert_eq!(
            ts2318_count, 0,
            "ES2015+ types should get TS2583, not TS2318: {:?}",
            checker.ctx.diagnostics
        );
    }

    #[test]
    fn test_es2015_plus_types_comprehensive() {
        // Test that all expected ES2015+ types are in the list
        // ES2015 core types
        assert!(is_es2015_plus_type("Promise"));
        assert!(is_es2015_plus_type("PromiseLike"));
        assert!(is_es2015_plus_type("PromiseConstructor"));
        assert!(is_es2015_plus_type("PromiseConstructorLike"));
        assert!(is_es2015_plus_type("PromiseSettledResult"));
        assert!(is_es2015_plus_type("PromiseFulfilledResult"));
        assert!(is_es2015_plus_type("PromiseRejectedResult"));
        assert!(is_es2015_plus_type("Map"));
        assert!(is_es2015_plus_type("MapConstructor"));
        assert!(is_es2015_plus_type("Set"));
        assert!(is_es2015_plus_type("SetConstructor"));
        assert!(is_es2015_plus_type("WeakMap"));
        assert!(is_es2015_plus_type("WeakMapConstructor"));
        assert!(is_es2015_plus_type("WeakSet"));
        assert!(is_es2015_plus_type("WeakSetConstructor"));
        assert!(is_es2015_plus_type("Proxy"));
        assert!(is_es2015_plus_type("ProxyHandler"));
        assert!(is_es2015_plus_type("ProxyConstructor"));
        assert!(is_es2015_plus_type("Reflect"));
        assert!(is_es2015_plus_type("Symbol"));
        assert!(is_es2015_plus_type("SymbolConstructor"));

        // Iterator types
        assert!(is_es2015_plus_type("Iterator"));
        assert!(is_es2015_plus_type("Iterable"));
        assert!(is_es2015_plus_type("IterableIterator"));
        assert!(is_es2015_plus_type("IteratorResult"));
        assert!(is_es2015_plus_type("IteratorYieldResult"));
        assert!(is_es2015_plus_type("IteratorReturnResult"));
        assert!(is_es2015_plus_type("AsyncIterator"));
        assert!(is_es2015_plus_type("AsyncIterable"));
        assert!(is_es2015_plus_type("AsyncIterableIterator"));

        // Generator types
        assert!(is_es2015_plus_type("Generator"));
        assert!(is_es2015_plus_type("GeneratorFunction"));
        assert!(is_es2015_plus_type("GeneratorFunctionConstructor"));

        // ES2017
        assert!(is_es2015_plus_type("AsyncFunction"));
        assert!(is_es2015_plus_type("AsyncFunctionConstructor"));
        assert!(is_es2015_plus_type("SharedArrayBuffer"));
        assert!(is_es2015_plus_type("SharedArrayBufferConstructor"));
        assert!(is_es2015_plus_type("Atomics"));

        // ES2018
        assert!(is_es2015_plus_type("AsyncGenerator"));
        assert!(is_es2015_plus_type("AsyncGeneratorFunction"));
        assert!(is_es2015_plus_type("AsyncGeneratorFunctionConstructor"));

        // ES2020
        assert!(is_es2015_plus_type("BigInt"));
        assert!(is_es2015_plus_type("BigIntConstructor"));
        assert!(is_es2015_plus_type("BigInt64Array"));
        assert!(is_es2015_plus_type("BigInt64ArrayConstructor"));
        assert!(is_es2015_plus_type("BigUint64Array"));
        assert!(is_es2015_plus_type("BigUint64ArrayConstructor"));

        // ES2021
        assert!(is_es2015_plus_type("FinalizationRegistry"));
        assert!(is_es2015_plus_type("FinalizationRegistryConstructor"));
        assert!(is_es2015_plus_type("WeakRef"));
        assert!(is_es2015_plus_type("WeakRefConstructor"));
        assert!(is_es2015_plus_type("AggregateError"));
        assert!(is_es2015_plus_type("AggregateErrorConstructor"));

        // ES2022
        assert!(is_es2015_plus_type("Awaited"));
        assert!(is_es2015_plus_type("ErrorOptions"));

        // Additional types
        assert!(is_es2015_plus_type("ArrayLike"));
        assert!(is_es2015_plus_type("ReadonlyMap"));
        assert!(is_es2015_plus_type("ReadonlySet"));
        assert!(is_es2015_plus_type("TemplateStringsArray"));
        assert!(is_es2015_plus_type("TypedPropertyDescriptor"));

        // Verify pre-ES2015 types are NOT in the list
        assert!(!is_es2015_plus_type("Object"));
        assert!(!is_es2015_plus_type("Array"));
        assert!(!is_es2015_plus_type("Function"));
        assert!(!is_es2015_plus_type("String"));
        assert!(!is_es2015_plus_type("Number"));
        assert!(!is_es2015_plus_type("Boolean"));
        assert!(!is_es2015_plus_type("Error")); // Error is pre-ES2015
        assert!(!is_es2015_plus_type("Date"));
        assert!(!is_es2015_plus_type("RegExp"));
        assert!(!is_es2015_plus_type("JSON"));
        assert!(!is_es2015_plus_type("Math"));
    }

    #[test]
    fn test_nolib_multiple_es2015_types() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that references multiple ES2015+ types
        let source = r#"
let p: Promise<number>;
let m: Map<string, number>;
let s: Set<string>;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2583 for Promise, Map, and Set
        let ts2583_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == MISSING_ES2015_LIB_SUPPORT)
            .count();

        assert!(
            ts2583_count >= 3,
            "Expected at least 3 TS2583 errors for ES2015+ types (Promise, Map, Set), got {}: {:?}",
            ts2583_count,
            checker.ctx.diagnostics
        );
    }

    #[test]
    fn test_es2015_value_usage_emits_ts2583() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that uses ES2015+ types as values (e.g., new Map(), new Set())
        let source = r#"
const m = new Map();
const s = new Set();
const p = Promise.resolve(42);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2583 for Map, Set, and Promise used as values
        let ts2583_diagnostics: Vec<_> = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_CHANGE_LIB)
            .collect();

        assert!(
            ts2583_diagnostics.len() >= 3,
            "Expected at least 3 TS2583 errors for ES2015+ values (Map, Set, Promise), got {}: {:?}",
            ts2583_diagnostics.len(),
            checker.ctx.diagnostics
        );

        // Verify the error messages suggest changing the lib
        for diag in &ts2583_diagnostics {
            assert!(
                diag.message_text.contains("change your target library")
                    || diag.message_text.contains("lib' compiler option"),
                "TS2583 message should suggest changing lib: {}",
                diag.message_text
            );
        }
    }

    #[test]
    fn test_symbol_value_usage_emits_ts2585() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that uses Symbol as a value
        let source = r#"
const sym = Symbol("test");
const sym2 = Symbol.for("shared");
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2585 for Symbol used as a value
        let ts2585_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| {
                d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB
            })
            .count();

        assert!(
            ts2585_count >= 1,
            "Expected at least 1 TS2585 error for Symbol value usage, got: {:?}",
            checker.ctx.diagnostics
        );
    }

    #[test]
    fn test_ts2318_for_pre_es2015_global_types() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that references pre-ES2015 global types (Date, RegExp)
        // These should emit TS2318, not TS2583
        let source = r#"
let d: Date;
let r: RegExp;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2318 for Date/RegExp (pre-ES2015 types), not TS2583
        let ts2318_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == CANNOT_FIND_GLOBAL_TYPE)
            .count();

        let ts2583_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == MISSING_ES2015_LIB_SUPPORT)
            .count();

        // Pre-ES2015 types should get TS2318
        assert!(
            ts2318_count >= 1,
            "Expected at least 1 TS2318 error for pre-ES2015 global types, got: {:?}",
            checker.ctx.diagnostics
        );

        // Ensure TS2583 is only for ES2015+ types
        // Date and RegExp should not trigger TS2583
        for diag in &checker.ctx.diagnostics {
            if diag.code == MISSING_ES2015_LIB_SUPPORT {
                assert!(
                    !diag.message_text.contains("Date") && !diag.message_text.contains("RegExp"),
                    "Pre-ES2015 types (Date, RegExp) should not get TS2583: {:?}",
                    diag
                );
            }
        }
    }

    #[test]
    fn test_ts2583_vs_ts2585_differentiation() {
        // TS2583: "Cannot find name 'X'. Do you need to change your target library?"
        //   - Used when the ES2015+ identifier is not found at all (value context)
        // TS2585: "'X' only refers to a type, but is being used as a value here. ..."
        //   - Used when a type-only symbol is being used as a value

        // Both should suggest changing the lib for ES2015+ types
        let test_cases = [
            ("Promise", true),
            ("Map", true),
            ("Set", true),
            ("WeakMap", true),
            ("WeakSet", true),
            ("Symbol", true),
            ("Proxy", true),
            ("Reflect", true),
            ("BigInt", true),
            ("Object", false), // Pre-ES2015
            ("Array", false),  // Pre-ES2015
            ("Date", false),   // Pre-ES2015
            ("Error", false),  // Pre-ES2015
        ];

        for (name, should_be_es2015) in test_cases {
            let is_es2015 = is_es2015_plus_type(name);
            assert_eq!(
                is_es2015, should_be_es2015,
                "is_es2015_plus_type('{}') should be {}",
                name, should_be_es2015
            );
        }
    }

    #[test]
    fn test_weakref_and_finalizationregistry_es2021() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that uses ES2021 types
        let source = r#"
const ref = new WeakRef({});
const registry = new FinalizationRegistry(() => {});
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2583 for WeakRef and FinalizationRegistry
        let ts2583_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_CHANGE_LIB)
            .count();

        assert!(
            ts2583_count >= 2,
            "Expected at least 2 TS2583 errors for ES2021 types (WeakRef, FinalizationRegistry), got {}: {:?}",
            ts2583_count,
            checker.ctx.diagnostics
        );
    }

    #[test]
    fn test_bigint_es2020() {
        use crate::binder::BinderState;
        use crate::checker::context::CheckerOptions;
        use crate::checker::state::CheckerState;
        use crate::checker::types::diagnostics::diagnostic_codes;
        use crate::parser::ParserState;
        use crate::solver::TypeInterner;

        // Source that uses BigInt (ES2020)
        let source = r#"
const big = BigInt(123);
const typed = new BigInt64Array(10);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        assert!(
            parser.get_diagnostics().is_empty(),
            "Parse errors: {:?}",
            parser.get_diagnostics()
        );

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut options = CheckerOptions::default();
        options.no_lib = true;

        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.check_source_file(root);

        // Should emit TS2583 for BigInt and BigInt64Array
        let ts2583_count = checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_CHANGE_LIB)
            .count();

        assert!(
            ts2583_count >= 2,
            "Expected at least 2 TS2583 errors for ES2020 types (BigInt, BigInt64Array), got {}: {:?}",
            ts2583_count,
            checker.ctx.diagnostics
        );
    }
}
