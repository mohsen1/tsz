use rustc_hash::FxHashMap;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

mod api;
mod project;

// Shared test fixtures for reduced allocation overhead
#[cfg(test)]
#[path = "../tests/test_fixtures.rs"]
pub mod test_fixtures;

// Re-export foundation types from tsz-common workspace crate
pub use tsz_common::interner;
pub use tsz_common::interner::{Atom, Interner, ShardedInterner};
#[cfg(test)]
#[path = "../tests/interner_tests.rs"]
mod interner_tests;

pub use tsz_common::common;
pub use tsz_common::common::{ModuleKind, NewLineKind, ScriptTarget};

pub use tsz_common::limits;

// Scanner module - re-exported from tsz-scanner workspace crate
pub use tsz_scanner as scanner;
pub use tsz_scanner::char_codes;
pub use tsz_scanner::scanner_impl;
pub use tsz_scanner::scanner_impl::{ScannerState, TokenFlags};
pub use tsz_scanner::*;
#[cfg(test)]
#[path = "../tests/scanner_impl_tests.rs"]
mod scanner_impl_tests;
#[cfg(test)]
#[path = "../tests/scanner_tests.rs"]
mod scanner_tests;

// Parser AST types - re-exported from tsz-parser workspace crate
pub use tsz_parser::parser;

// Syntax utilities - re-exported from tsz-parser workspace crate
pub use tsz_parser::syntax;

// Parser - Cache-optimized parser using NodeArena (Phase 0.1)
#[cfg(test)]
#[path = "../tests/parser_state_tests.rs"]
mod parser_state_tests;

// TS1038 - declare modifier in ambient context tests
#[cfg(test)]
#[path = "../tests/parser_ts1038_tests.rs"]
mod parser_ts1038_tests;

// Control flow validation tests (TS1104, TS1105)
#[cfg(test)]
#[path = "../tests/control_flow_validation_tests.rs"]
mod control_flow_validation_tests;

// Regex flag error detection tests
#[cfg(test)]
#[path = "../tests/regex_flag_tests.rs"]
mod regex_flag_tests;

#[cfg(test)]
#[path = "../tests/strict_bind_call_apply_tests.rs"]
mod strict_bind_call_apply_tests;

// Binder types and implementation - re-exported from tsz-binder workspace crate
pub use tsz_binder as binder;

// BinderState - Binder using NodeArena (Phase 0.1)
#[cfg(test)]
#[path = "../tests/binder_state_tests.rs"]
mod binder_state_tests;

// Lib Loader - re-exported from tsz-binder
pub use tsz_binder::lib_loader;

// Checker types and implementation (Phase 5) - re-exported from tsz-checker workspace crate
pub use tsz_checker as checker;

#[cfg(test)]
#[path = "../tests/checker_state_tests.rs"]
mod checker_state_tests;

#[cfg(test)]
#[path = "../tests/variable_redeclaration_tests.rs"]
mod variable_redeclaration_tests;

#[cfg(test)]
#[path = "../tests/strict_mode_and_module_tests.rs"]
mod strict_mode_and_module_tests;

#[cfg(test)]
#[path = "../tests/overload_compatibility_tests.rs"]
mod overload_compatibility_tests;

#[cfg(test)]
#[path = "../tests/core_public_helpers_tests.rs"]
mod core_public_helpers_tests;

// Cross-file module resolution tests
#[cfg(test)]
#[path = "../tests/module_resolution_tests.rs"]
mod module_resolution_tests;

pub use checker::state::{CheckerState, MAX_CALL_DEPTH, MAX_INSTANTIATION_DEPTH};

// Emitter - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::emitter;
#[cfg(test)]
#[path = "../tests/transform_api_tests.rs"]
mod transform_api_tests;

// Printer - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::output::printer;

// Safe string slice utilities - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::safe_slice;
#[cfg(test)]
#[path = "../tests/printer_tests.rs"]
mod printer_tests;

// Span - Source location tracking (byte offsets)
pub use tsz_common::span;

// SourceFile - Owns source text and provides &str references
pub mod source_file;

// Diagnostics - Error collection, formatting, and reporting
pub mod diagnostics;

// Enums - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::enums;

// Parallel processing with Rayon (Phase 0.4)
pub mod parallel;

// Embedded lib.d.ts files for zero-I/O startup
pub mod embedded_libs;

// Comment preservation (Phase 6.3)
pub use tsz_common::comments;
#[cfg(test)]
#[path = "../tests/comments_tests.rs"]
mod comments_tests;

// Source Map generation (Phase 6.2)
pub use tsz_common::source_map;
#[cfg(test)]
#[path = "../tests/source_map_test_utils.rs"]
mod source_map_test_utils;
#[cfg(test)]
#[path = "../tests/source_map_tests_1.rs"]
mod source_map_tests_1;
#[cfg(test)]
#[path = "../tests/source_map_tests_2.rs"]
mod source_map_tests_2;
#[cfg(test)]
#[path = "../tests/source_map_tests_3.rs"]
mod source_map_tests_3;
#[cfg(test)]
#[path = "../tests/source_map_tests_4.rs"]
mod source_map_tests_4;

// SourceWriter - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::output::source_writer;
#[cfg(test)]
#[path = "../tests/source_writer_tests.rs"]
mod source_writer_tests;

// Context (EmitContext, TransformContext) - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::context;

// LoweringPass - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::lowering;

// Declaration file emitter - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::declaration_emitter;

// JavaScript transforms - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::transforms;

// Query-based Structural Solver (Phase 7.5)
pub use tsz_solver;

// LSP (Language Server Protocol) support - re-exported from tsz-lsp workspace crate
pub use tsz_lsp as lsp;

// Test Harness - Infrastructure for unit and conformance tests
#[cfg(test)]
#[path = "../tests/test_harness.rs"]
mod test_harness;

// Isolated Test Runner - Process-based test execution with resource limits
#[cfg(test)]
#[path = "../tests/isolated_test_runner.rs"]
mod isolated_test_runner;

// Compiler configuration types (shared between core and CLI)
pub mod config;

// Re-exports from tsz-cli crate (when available as a dependency)
// CLI code has been moved to crates/tsz-cli/

// Re-exports from tsz-wasm crate (when available as a dependency)
// WASM integration code has been moved to crates/tsz-wasm/

// Module Resolution Infrastructure (non-wasm targets only - requires file system access)
#[cfg(not(target_arch = "wasm32"))]
pub mod module_resolver;
#[cfg(not(target_arch = "wasm32"))]
mod module_resolver_helpers;
#[cfg(not(target_arch = "wasm32"))]
pub use module_resolver::{
    ModuleExtension, ModuleLookupError, ModuleLookupOutcome, ModuleLookupRequest,
    ModuleLookupResult, ModuleResolver, ResolutionFailure, ResolvedModule,
};

// Import/Export Tracking
pub mod imports;
pub use imports::{ImportDeclaration, ImportKind, ImportTracker, ImportedBinding};

pub mod exports;
pub use exports::{ExportDeclaration, ExportKind, ExportTracker, ExportedBinding};

// Module Dependency Graph (non-wasm targets only - requires module_resolver)
#[cfg(not(target_arch = "wasm32"))]
pub mod module_graph;
#[cfg(not(target_arch = "wasm32"))]
pub use module_graph::{CircularDependency, ModuleGraph, ModuleId, ModuleInfo};

// =============================================================================
// Scanner Factory Function
// =============================================================================

/// Create a new scanner for the given source text.
/// This is the wasm-bindgen entry point for creating scanners from JavaScript.
#[wasm_bindgen(js_name = createScanner)]
pub fn create_scanner(text: String, skip_trivia: bool) -> ScannerState {
    ScannerState::new(text, skip_trivia)
}

// =============================================================================
// Parser WASM Interface (High-Performance Parser)
// =============================================================================

use crate::api::wasm::compiler_options::CompilerOptions;
use crate::api::wasm::program_results::{
    CheckDiagnosticJson, FileCheckResultJson, ParseDiagnosticJson,
};
use crate::context::transform::TransformContext;
use crate::project::lib_cache::get_or_create_lib_file;
use std::sync::Arc;

/// Opaque wrapper for transform directives across the wasm boundary.
#[wasm_bindgen]
pub struct WasmTransformContext {
    pub(crate) inner: TransformContext,
    pub(crate) target_es5: bool,
    pub(crate) module_kind: ModuleKind,
}

#[wasm_bindgen]
impl WasmTransformContext {
    /// Get the number of transform directives generated.
    #[wasm_bindgen(js_name = getCount)]
    pub fn get_count(&self) -> usize {
        self.inner.len()
    }
}

pub use crate::api::wasm::parser::Parser;

/// Create a new Parser for the given source text.
/// This is the recommended parser for production use.
#[wasm_bindgen(js_name = createParser)]
pub fn create_parser(file_name: String, source_text: String) -> Parser {
    Parser::new(file_name, source_text)
}

// =============================================================================
// WasmProgram - Multi-file TypeScript Program Support
// =============================================================================

use crate::parallel::{
    BindResult, MergedProgram, check_files_parallel, merge_bind_results, parse_and_bind_parallel,
};

/// Multi-file TypeScript program for cross-file type checking.
///
/// This struct provides an API for compiling multiple TypeScript files together,
/// enabling proper module resolution and cross-file type checking.
///
/// # Example (JavaScript)
/// ```javascript
/// const program = new WasmProgram();
/// program.addFile("a.ts", "export const x = 1;");
/// program.addFile("b.ts", "import { x } from './a'; const y = x + 1;");
/// const result = program.checkAll();
/// console.log(result);
/// ```
#[wasm_bindgen]
pub struct WasmProgram {
    /// Accumulated files before compilation
    files: Vec<(String, String)>,
    /// Merged program state after compilation (lazy)
    merged: Option<MergedProgram>,
    /// Bind results (kept for diagnostics access)
    bind_results: Option<Vec<BindResult>>,
    /// Lib files (lib.d.ts, lib.dom.d.ts, etc.) for global symbol resolution
    lib_files: Vec<(String, String)>,
    /// Compiler options for type checking
    compiler_options: CompilerOptions,
}

impl Default for WasmProgram {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmProgram {
    /// Create a new empty program.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            lib_files: Vec::new(),
            merged: None,
            bind_results: None,
            compiler_options: CompilerOptions::default(),
        }
    }

    /// Add a file to the program.
    ///
    /// Files are accumulated and compiled together when `checkAll` is called.
    /// The `file_name` should be a relative path like "src/a.ts".
    ///
    /// For TypeScript library files (lib.d.ts, lib.dom.d.ts, etc.), use `addLibFile` instead.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, file_name: String, source_text: String) {
        // Invalidate any previous compilation
        self.merged = None;
        self.bind_results = None;

        // Skip package.json files - they're used for module resolution but not parsed
        if file_name.ends_with("package.json") {
            return;
        }

        self.files.push((file_name, source_text));
    }

    /// Add a TypeScript library file (lib.d.ts, lib.dom.d.ts, etc.) to the program.
    ///
    /// Lib files are used for global symbol resolution and are merged into
    /// the symbol table before user files are processed.
    ///
    /// Use this method explicitly instead of relying on automatic file name detection.
    /// This makes the API behavior predictable and explicit.
    ///
    /// # Example (JavaScript)
    /// ```javascript
    /// const program = new WasmProgram();
    /// program.addLibFile("lib.d.ts", libContent);
    /// program.addFile("src/a.ts", userCode);
    /// ```
    #[wasm_bindgen(js_name = addLibFile)]
    pub fn add_lib_file(&mut self, file_name: String, source_text: String) {
        // Invalidate any previous compilation
        self.merged = None;
        self.bind_results = None;

        self.lib_files.push((file_name, source_text));
    }

    /// Set compiler options from JSON.
    ///
    /// # Arguments
    /// * `options_json` - JSON string containing compiler options
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, options_json: &str) -> Result<(), JsValue> {
        match serde_json::from_str::<CompilerOptions>(options_json) {
            Ok(options) => {
                self.compiler_options = options;
                // Invalidate any previous compilation since options affect typing
                self.merged = None;
                self.bind_results = None;
                Ok(())
            }
            Err(e) => Err(JsValue::from_str(&format!(
                "Failed to parse compiler options: {e}"
            ))),
        }
    }

    /// Get the number of files in the program.
    #[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
    #[wasm_bindgen(js_name = getFileCount)]
    pub fn get_file_count(&self) -> usize {
        self.files.len()
    }

    /// Clear all files and reset the program state.
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.files.clear();
        self.lib_files.clear();
        self.merged = None;
        self.bind_results = None;
    }

    /// Compile all files and return diagnostics as JSON.
    ///
    /// This performs:
    /// 1. Load lib files for global symbol resolution
    /// 2. Parallel parsing of all files
    /// 3. Parallel binding of all files with lib symbols merged
    /// 4. Symbol merging (sequential)
    /// 5. Parallel type checking
    ///
    /// Returns a JSON object with diagnostics per file.
    #[wasm_bindgen(js_name = checkAll)]
    pub fn check_all(&mut self) -> String {
        if self.files.is_empty() && self.lib_files.is_empty() {
            return r#"{"files":[],"stats":{"totalFiles":0,"totalDiagnostics":0}}"#.to_string();
        }

        // Load lib files for binding
        // Use cache to avoid re-parsing lib.d.ts for every test
        let lib_file_objects: Vec<Arc<lib_loader::LibFile>> = self
            .lib_files
            .iter()
            .map(|(file_name, source_text)| {
                get_or_create_lib_file(file_name.clone(), source_text.clone())
            })
            .collect();

        // Parse and bind all files in parallel with lib symbols
        let bind_results = if !lib_file_objects.is_empty() {
            // Use lib-aware binding
            use crate::parallel;
            parallel::parse_and_bind_parallel_with_libs(self.files.clone(), &lib_file_objects)
        } else {
            // No lib files - use regular binding
            parse_and_bind_parallel(self.files.clone())
        };

        // Collect parse diagnostics before merging
        let parse_diags: Vec<Vec<_>> = bind_results
            .iter()
            .map(|r| r.parse_diagnostics.clone())
            .collect();
        let file_names: Vec<String> = bind_results.iter().map(|r| r.file_name.clone()).collect();

        // Merge bind results into unified program
        let merged = merge_bind_results(bind_results);

        // Type check all files in parallel
        let checker_options = self.compiler_options.to_checker_options();
        let check_result = check_files_parallel(&merged, &checker_options, &lib_file_objects);

        // Build JSON result
        let mut file_results: Vec<FileCheckResultJson> = Vec::new();
        let mut total_diagnostics = 0;

        for (i, file_name) in file_names.iter().enumerate() {
            let parse_diagnostics: Vec<ParseDiagnosticJson> = parse_diags[i]
                .iter()
                .map(|d| ParseDiagnosticJson {
                    message: d.message.clone(),
                    start: d.start,
                    length: d.length,
                    code: d.code,
                })
                .collect();

            // Find check diagnostics for this file
            let check_diagnostics: Vec<CheckDiagnosticJson> = check_result
                .file_results
                .iter()
                .find(|r| &r.file_name == file_name)
                .map(|r| {
                    r.diagnostics
                        .iter()
                        .map(|d| CheckDiagnosticJson {
                            message_text: d.message_text.clone(),
                            code: d.code,
                            start: d.start,
                            length: d.length,
                            category: format!("{:?}", d.category),
                        })
                        .collect()
                })
                .unwrap_or_default();

            total_diagnostics += parse_diagnostics.len() + check_diagnostics.len();

            file_results.push(FileCheckResultJson {
                file_name: file_name.clone(),
                parse_diagnostics,
                check_diagnostics,
            });
        }

        // Store merged program for potential future queries
        self.merged = Some(merged);

        let result = serde_json::json!({
            "files": file_results,
            "stats": {
                "totalFiles": file_names.len(),
                "totalDiagnostics": total_diagnostics,
            }
        });

        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
    }

    /// Get diagnostic codes for all files (for conformance testing).
    ///
    /// Returns a JSON object mapping file names to arrays of error codes.
    #[wasm_bindgen(js_name = getDiagnosticCodes)]
    pub fn get_diagnostic_codes(&mut self) -> String {
        if self.files.is_empty() && self.lib_files.is_empty() {
            return "{}".to_string();
        }

        // Load lib files for binding (enables global symbol resolution: console, Array, etc.)
        // Use cache to avoid re-parsing lib.d.ts for every test
        let lib_file_objects: Vec<Arc<lib_loader::LibFile>> = self
            .lib_files
            .iter()
            .map(|(file_name, source_text)| {
                get_or_create_lib_file(file_name.clone(), source_text.clone())
            })
            .collect();

        // Parse and bind all files in parallel with lib symbols
        let bind_results = if !lib_file_objects.is_empty() {
            use crate::parallel;
            parallel::parse_and_bind_parallel_with_libs(self.files.clone(), &lib_file_objects)
        } else {
            parse_and_bind_parallel(self.files.clone())
        };

        // Collect parse diagnostic codes
        let mut file_codes: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        for result in &bind_results {
            let codes: Vec<u32> = result.parse_diagnostics.iter().map(|d| d.code).collect();
            file_codes.insert(result.file_name.clone(), codes);
        }

        // Merge and check
        let merged = merge_bind_results(bind_results);
        let checker_options = self.compiler_options.to_checker_options();
        let check_result = check_files_parallel(&merged, &checker_options, &lib_file_objects);

        // Add check diagnostic codes
        for file_result in &check_result.file_results {
            let entry = file_codes.entry(file_result.file_name.clone()).or_default();
            for diag in &file_result.diagnostics {
                entry.push(diag.code);
            }
        }

        // Store merged program
        self.merged = Some(merged);

        serde_json::to_string(&file_codes).unwrap_or_else(|_| "{}".to_string())
    }

    /// Get all diagnostic codes as a flat array (for simple conformance comparison).
    ///
    /// This combines all parse and check diagnostics from all files into a single
    /// array of error codes, which can be compared against tsc output.
    #[wasm_bindgen(js_name = getAllDiagnosticCodes)]
    pub fn get_all_diagnostic_codes(&mut self) -> Vec<u32> {
        if self.files.is_empty() && self.lib_files.is_empty() {
            return Vec::new();
        }

        // Load lib files for binding (enables global symbol resolution: console, Array, etc.)
        // Use cache to avoid re-parsing lib.d.ts for every test
        let lib_file_objects: Vec<Arc<lib_loader::LibFile>> = self
            .lib_files
            .iter()
            .map(|(file_name, source_text)| {
                get_or_create_lib_file(file_name.clone(), source_text.clone())
            })
            .collect();

        // Parse and bind all files in parallel with lib symbols
        let bind_results = if !lib_file_objects.is_empty() {
            use crate::parallel;
            parallel::parse_and_bind_parallel_with_libs(self.files.clone(), &lib_file_objects)
        } else {
            parse_and_bind_parallel(self.files.clone())
        };

        // Collect all parse diagnostic codes
        let mut all_codes: Vec<u32> = Vec::new();
        for result in &bind_results {
            for diag in &result.parse_diagnostics {
                all_codes.push(diag.code);
            }
        }

        // Merge and check
        let merged = merge_bind_results(bind_results);
        let checker_options = self.compiler_options.to_checker_options();
        let check_result = check_files_parallel(&merged, &checker_options, &lib_file_objects);

        // Add all check diagnostic codes
        for file_result in &check_result.file_results {
            for diag in &file_result.diagnostics {
                all_codes.push(diag.code);
            }
        }

        // Store merged program
        self.merged = Some(merged);

        all_codes
    }
}

/// Create a new multi-file program.
#[wasm_bindgen(js_name = createProgram)]
pub fn create_program() -> WasmProgram {
    WasmProgram::new()
}

// =============================================================================
// Comparison enum - matches TypeScript's Comparison const enum
// =============================================================================

/// Comparison result for ordering operations.
/// Matches TypeScript's `Comparison` const enum in src/compiler/corePublic.ts
#[wasm_bindgen]
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    LessThan = -1,
    EqualTo = 0,
    GreaterThan = 1,
}

// =============================================================================
// String Comparison Utilities (Phase 1.1)
// =============================================================================

/// Compare two strings using a case-sensitive ordinal comparison.
///
/// Ordinal comparisons are based on the difference between the unicode code points
/// of both strings. Characters with multiple unicode representations are considered
/// unequal. Ordinal comparisons provide predictable ordering, but place "a" after "B".
#[wasm_bindgen(js_name = compareStringsCaseSensitive)]
pub fn compare_strings_case_sensitive(a: Option<String>, b: Option<String>) -> Comparison {
    match (a, b) {
        (None, None) => Comparison::EqualTo,
        (None, Some(_)) => Comparison::LessThan,
        (Some(_), None) => Comparison::GreaterThan,
        (Some(a), Some(b)) => match a.cmp(&b) {
            std::cmp::Ordering::Equal => Comparison::EqualTo,
            std::cmp::Ordering::Less => Comparison::LessThan,
            std::cmp::Ordering::Greater => Comparison::GreaterThan,
        },
    }
}

/// Compare two strings using a case-insensitive ordinal comparison.
///
/// Case-insensitive comparisons compare both strings one code-point at a time using
/// the integer value of each code-point after applying `to_uppercase` to each string.
/// We always map both strings to their upper-case form as some unicode characters do
/// not properly round-trip to lowercase (such as `ẞ` German sharp capital s).
#[wasm_bindgen(js_name = compareStringsCaseInsensitive)]
pub fn compare_strings_case_insensitive(a: Option<String>, b: Option<String>) -> Comparison {
    match (a, b) {
        (None, None) => Comparison::EqualTo,
        (None, Some(_)) => Comparison::LessThan,
        (Some(_), None) => Comparison::GreaterThan,
        (Some(a), Some(b)) => {
            if a == b {
                return Comparison::EqualTo;
            }
            // Use iterator-based comparison to avoid allocating new strings
            compare_strings_case_insensitive_iter(&a, &b)
        }
    }
}

/// Iterator-based case-insensitive comparison (no allocation).
/// Maps characters to uppercase on-the-fly without creating new strings.
#[inline]
fn compare_strings_case_insensitive_iter(a: &str, b: &str) -> Comparison {
    use std::cmp::Ordering;

    let mut a_chars = a.chars().flat_map(char::to_uppercase);
    let mut b_chars = b.chars().flat_map(char::to_uppercase);

    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return Comparison::EqualTo,
            (None, Some(_)) => return Comparison::LessThan,
            (Some(_), None) => return Comparison::GreaterThan,
            (Some(a_char), Some(b_char)) => match a_char.cmp(&b_char) {
                Ordering::Less => return Comparison::LessThan,
                Ordering::Greater => return Comparison::GreaterThan,
                Ordering::Equal => continue,
            },
        }
    }
}

/// Compare two strings using a case-insensitive ordinal comparison (eslint-compatible).
///
/// This uses `to_lowercase` instead of `to_uppercase` to match eslint's `sort-imports`
/// rule behavior. The difference affects the relative order of letters and ASCII
/// characters 91-96, of which `_` is a valid identifier character.
#[wasm_bindgen(js_name = compareStringsCaseInsensitiveEslintCompatible)]
pub fn compare_strings_case_insensitive_eslint_compatible(
    a: Option<String>,
    b: Option<String>,
) -> Comparison {
    match (a, b) {
        (None, None) => Comparison::EqualTo,
        (None, Some(_)) => Comparison::LessThan,
        (Some(_), None) => Comparison::GreaterThan,
        (Some(a), Some(b)) => {
            if a == b {
                return Comparison::EqualTo;
            }
            // Use iterator-based comparison to avoid allocating new strings
            compare_strings_case_insensitive_lower_iter(&a, &b)
        }
    }
}

/// Iterator-based case-insensitive comparison using lowercase (no allocation).
/// Used for eslint compatibility.
#[inline]
fn compare_strings_case_insensitive_lower_iter(a: &str, b: &str) -> Comparison {
    use std::cmp::Ordering;

    let mut a_chars = a.chars().flat_map(char::to_lowercase);
    let mut b_chars = b.chars().flat_map(char::to_lowercase);

    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return Comparison::EqualTo,
            (None, Some(_)) => return Comparison::LessThan,
            (Some(_), None) => return Comparison::GreaterThan,
            (Some(a_char), Some(b_char)) => match a_char.cmp(&b_char) {
                Ordering::Less => return Comparison::LessThan,
                Ordering::Greater => return Comparison::GreaterThan,
                Ordering::Equal => continue,
            },
        }
    }
}

/// Check if two strings are equal (case-sensitive).
#[wasm_bindgen(js_name = equateStringsCaseSensitive)]
pub fn equate_strings_case_sensitive(a: &str, b: &str) -> bool {
    a == b
}

/// Check if two strings are equal (case-insensitive).
/// Uses iterator-based comparison to avoid allocating new strings.
#[wasm_bindgen(js_name = equateStringsCaseInsensitive)]
pub fn equate_strings_case_insensitive(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        // Quick length check - but note that uppercase/lowercase might change length
        // for some unicode characters, so we need the full comparison
    }
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

// =============================================================================
// Path Utilities (Phase 1.2)
// =============================================================================

/// Directory separator used internally (forward slash).
pub const DIRECTORY_SEPARATOR: char = '/';

/// Alternative directory separator (backslash, used on Windows).
pub const ALT_DIRECTORY_SEPARATOR: char = '\\';

/// Determines whether a charCode corresponds to `/` or `\`.
#[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
#[wasm_bindgen(js_name = isAnyDirectorySeparator)]
pub fn is_any_directory_separator(char_code: u32) -> bool {
    char_code == DIRECTORY_SEPARATOR as u32 || char_code == ALT_DIRECTORY_SEPARATOR as u32
}

/// Normalize path separators, converting `\` into `/`.
#[wasm_bindgen(js_name = normalizeSlashes)]
pub fn normalize_slashes(path: &str) -> String {
    if path.contains('\\') {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}

/// Determines whether a path has a trailing separator (`/` or `\\`).
#[wasm_bindgen(js_name = hasTrailingDirectorySeparator)]
pub fn has_trailing_directory_separator(path: &str) -> bool {
    let last_char = match path.chars().last() {
        Some(c) => c,
        None => return false,
    };
    last_char == DIRECTORY_SEPARATOR || last_char == ALT_DIRECTORY_SEPARATOR
}

/// Determines whether a path starts with a relative path component (i.e. `.` or `..`).
#[wasm_bindgen(js_name = pathIsRelative)]
pub fn path_is_relative(path: &str) -> bool {
    // Matches /^\.\.?(?:$|[\\/])/
    if path.starts_with("./") || path.starts_with(".\\") || path == "." {
        return true;
    }
    if path.starts_with("../") || path.starts_with("..\\") || path == ".." {
        return true;
    }
    false
}

/// Removes a trailing directory separator from a path, if it has one.
/// Uses char-based operations for UTF-8 safety.
#[wasm_bindgen(js_name = removeTrailingDirectorySeparator)]
pub fn remove_trailing_directory_separator(path: &str) -> String {
    if !has_trailing_directory_separator(path) || path.len() <= 1 {
        return path.to_string();
    }
    // Use strip_suffix for UTF-8 safe character removal
    path.strip_suffix(DIRECTORY_SEPARATOR)
        .or_else(|| path.strip_suffix(ALT_DIRECTORY_SEPARATOR))
        .unwrap_or(path)
        .to_string()
}

/// Ensures a path has a trailing directory separator.
#[wasm_bindgen(js_name = ensureTrailingDirectorySeparator)]
pub fn ensure_trailing_directory_separator(path: &str) -> String {
    if has_trailing_directory_separator(path) {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

/// Determines whether a path has an extension.
#[wasm_bindgen(js_name = hasExtension)]
pub fn has_extension(file_name: &str) -> bool {
    get_base_file_name(file_name).contains('.')
}

/// Returns the path except for its containing directory name (basename).
/// Uses char-based operations for UTF-8 safety.
#[wasm_bindgen(js_name = getBaseFileName)]
pub fn get_base_file_name(path: &str) -> String {
    let path = normalize_slashes(path);
    // Remove trailing separator using UTF-8 safe operations
    let path = if has_trailing_directory_separator(&path) && path.len() > 1 {
        path.strip_suffix(DIRECTORY_SEPARATOR)
            .or_else(|| path.strip_suffix(ALT_DIRECTORY_SEPARATOR))
            .unwrap_or(&path)
    } else {
        &path
    };
    // Find last separator - safe because '/' is ASCII and rfind returns valid char boundary
    match path.rfind('/') {
        Some(idx) => path[idx + 1..].to_string(),
        None => path.to_string(),
    }
}

/// Check if path ends with a specific extension.
#[wasm_bindgen(js_name = fileExtensionIs)]
pub fn file_extension_is(path: &str, extension: &str) -> bool {
    path.len() > extension.len() && path.ends_with(extension)
}

/// Convert file name to lowercase for case-insensitive file systems.
///
/// This function handles special Unicode characters that need to remain
/// case-sensitive for proper cross-platform file name handling:
/// - \u{0130} (İ - Latin capital I with dot above)
/// - \u{0131} (ı - Latin small letter dotless i)
/// - \u{00DF} (ß - Latin small letter sharp s)
///
/// These characters are excluded from lowercase conversion to maintain
/// compatibility with case-insensitive file systems that have special
/// handling for these characters (notably Turkish locale on Windows).
///
/// Matches TypeScript's `toFileNameLowerCase` in src/compiler/core.ts
#[wasm_bindgen(js_name = toFileNameLowerCase)]
pub fn to_file_name_lower_case(x: &str) -> String {
    // First, check if we need to do any work (optimization - avoid allocation)
    // The "safe" set of characters that don't need lowercasing:
    // - \u{0130} (İ), \u{0131} (ı), \u{00DF} (ß) - special Turkish chars
    // - a-z (lowercase ASCII letters)
    // - 0-9 (digits)
    // - \ / : - _ . (path separators and common filename chars)
    // - space

    let needs_conversion = x.chars().any(|c| {
        !matches!(c,
            '\u{0130}' | '\u{0131}' | '\u{00DF}' |  // Special Unicode chars
            'a'..='z' | '0'..='9' |  // ASCII lowercase and digits
            '\\' | '/' | ':' | '-' | '_' | '.' | ' '  // Path chars and space
        )
    });

    if !needs_conversion {
        return x.to_string();
    }

    // Convert to lowercase, preserving the special characters
    x.to_lowercase()
}

// =============================================================================
// Character Classification (Phase 1.3 - Scanner Prep)
// =============================================================================

use crate::char_codes::CharacterCodes;

/// Check if character is a line break (LF, CR, LS, PS).
#[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
#[wasm_bindgen(js_name = isLineBreak)]
pub fn is_line_break(ch: u32) -> bool {
    ch == CharacterCodes::LINE_FEED
        || ch == CharacterCodes::CARRIAGE_RETURN
        || ch == CharacterCodes::LINE_SEPARATOR
        || ch == CharacterCodes::PARAGRAPH_SEPARATOR
}

/// Check if character is a single-line whitespace (not including line breaks).
#[wasm_bindgen(js_name = isWhiteSpaceSingleLine)]
pub fn is_white_space_single_line(ch: u32) -> bool {
    ch == CharacterCodes::SPACE
        || ch == CharacterCodes::TAB
        || ch == CharacterCodes::VERTICAL_TAB
        || ch == CharacterCodes::FORM_FEED
        || ch == CharacterCodes::NON_BREAKING_SPACE
        || ch == CharacterCodes::NEXT_LINE
        || ch == CharacterCodes::OGHAM
        || (CharacterCodes::EN_QUAD..=CharacterCodes::ZERO_WIDTH_SPACE).contains(&ch)
        || ch == CharacterCodes::NARROW_NO_BREAK_SPACE
        || ch == CharacterCodes::MATHEMATICAL_SPACE
        || ch == CharacterCodes::IDEOGRAPHIC_SPACE
        || ch == CharacterCodes::BYTE_ORDER_MARK
}

/// Check if character is any whitespace (including line breaks).
#[wasm_bindgen(js_name = isWhiteSpaceLike)]
pub fn is_white_space_like(ch: u32) -> bool {
    is_white_space_single_line(ch) || is_line_break(ch)
}

/// Check if character is a decimal digit (0-9).
#[wasm_bindgen(js_name = isDigit)]
pub fn is_digit(ch: u32) -> bool {
    (CharacterCodes::_0..=CharacterCodes::_9).contains(&ch)
}

/// Check if character is an octal digit (0-7).
#[wasm_bindgen(js_name = isOctalDigit)]
pub fn is_octal_digit(ch: u32) -> bool {
    (CharacterCodes::_0..=CharacterCodes::_7).contains(&ch)
}

/// Check if character is a hexadecimal digit (0-9, A-F, a-f).
#[wasm_bindgen(js_name = isHexDigit)]
pub fn is_hex_digit(ch: u32) -> bool {
    is_digit(ch)
        || (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_F).contains(&ch)
        || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_F).contains(&ch)
}

/// Check if character is an ASCII letter (A-Z, a-z).
#[wasm_bindgen(js_name = isASCIILetter)]
pub fn is_ascii_letter(ch: u32) -> bool {
    (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_Z).contains(&ch)
        || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_Z).contains(&ch)
}

/// Check if character is a word character (A-Z, a-z, 0-9, _).
#[wasm_bindgen(js_name = isWordCharacter)]
pub fn is_word_character(ch: u32) -> bool {
    is_ascii_letter(ch) || is_digit(ch) || ch == CharacterCodes::UNDERSCORE
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod playground_bridge_option_tests {
    use crate::api::wasm::compiler_options::CompilerOptions;

    #[test]
    fn test_compiler_options_parse_sound_mode_from_playground_json() {
        let options: CompilerOptions =
            serde_json::from_str(r#"{"strict":true,"soundMode":true,"module":99}"#)
                .expect("compiler options JSON should parse");
        let checker_options = options.to_checker_options();

        assert!(checker_options.strict);
        assert!(checker_options.sound_mode);
        assert_eq!(checker_options.module, crate::common::ModuleKind::ESNext);
    }
}

// ASI Conformance tests for verifying TS1005/TS1109 patterns
#[cfg(test)]
#[path = "../tests/asi_conformance_tests.rs"]
mod asi_conformance_tests;

#[cfg(test)]
#[path = "../tests/debug_asi.rs"]
mod debug_asi;

// P1 Error Recovery tests for synchronization point improvements
#[cfg(test)]
#[path = "../tests/p1_error_recovery_tests.rs"]
mod p1_error_recovery_tests;

// Tests moved to checker crate: strict_null_manual, generic_inference_manual,
// enum_nominality_tests, private_brands

// Constructor accessibility tests
#[cfg(test)]
#[path = "../../tsz-checker/tests/constructor_accessibility.rs"]
mod constructor_accessibility;

// Void return exception tests
#[cfg(test)]
#[path = "../../tsz-checker/tests/void_return_exception.rs"]
mod void_return_exception;

// Any-propagation tests
#[cfg(test)]
#[path = "../../tsz-checker/tests/any_propagation.rs"]
mod any_propagation;

// Tests that depend on test_fixtures (require root crate context)
#[cfg(test)]
#[path = "../../tsz-checker/tests/any_propagation_tests.rs"]
mod any_propagation_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/const_assertion_tests.rs"]
mod const_assertion_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/freshness_stripping_tests.rs"]
mod freshness_stripping_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/function_bivariance.rs"]
mod function_bivariance;
#[cfg(test)]
#[path = "../../tsz-checker/tests/global_type_tests.rs"]
mod global_type_tests;
// symbol_resolution_tests: disabled (rustfmt parsing error with Rust 2024 edition via #[path] include)
#[cfg(test)]
#[path = "../../tsz-checker/tests/ts2304_tests.rs"]
mod ts2304_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/ts2305_tests.rs"]
mod ts2305_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/ts2306_tests.rs"]
mod ts2306_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/ts2498_export_star_export_equals_tests.rs"]
mod ts2498_export_star_export_equals_tests;
#[cfg(test)]
#[path = "../../tsz-checker/tests/widening_integration_tests.rs"]
mod widening_integration_tests;
