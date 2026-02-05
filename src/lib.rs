use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

// Initialize panic hook for WASM to prevent worker crashes
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    // Set panic hook to log errors to console instead of crashing worker
    console_error_panic_hook::set_once();
}

// Global cache for parsed lib files to avoid re-parsing lib.d.ts per test
// Key: (file_name, content_hash), Value: Arc<LibFile>
static LIB_FILE_CACHE: Lazy<Mutex<FxHashMap<(String, u64), Arc<lib_loader::LibFile>>>> =
    Lazy::new(|| Mutex::new(FxHashMap::default()));

/// Simple hash function for lib file content
fn hash_lib_content(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Get or create a cached lib file. This avoids re-parsing lib.d.ts for every test.
fn get_or_create_lib_file(file_name: String, source_text: String) -> Arc<lib_loader::LibFile> {
    let content_hash = hash_lib_content(&source_text);
    let cache_key = (file_name.clone(), content_hash);

    // Try to get from cache
    {
        let cache = LIB_FILE_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            return Arc::clone(cached);
        }
    }

    // Not in cache - parse and bind
    let mut lib_parser = ParserState::new(file_name.clone(), source_text);
    let source_file_idx = lib_parser.parse_source_file();

    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

    let arena = Arc::new(lib_parser.into_arena());
    let binder = Arc::new(lib_binder);

    let lib_file = Arc::new(lib_loader::LibFile::new(file_name, arena, binder));

    // Store in cache
    {
        let mut cache = LIB_FILE_CACHE.lock().unwrap();
        cache.insert(cache_key, Arc::clone(&lib_file));
    }

    lib_file
}

// Shared test fixtures for reduced allocation overhead
#[cfg(test)]
#[path = "tests/test_fixtures.rs"]
pub mod test_fixtures;

// String interning for identifier deduplication (Performance optimization)
pub mod interner;
pub use interner::{Atom, Interner, ShardedInterner};
#[cfg(test)]
#[path = "tests/interner_tests.rs"]
mod interner_tests;

// Common types - Shared constants to break circular dependencies
pub mod common;
pub use common::{ModuleKind, NewLineKind, ScriptTarget};

// Centralized limits and thresholds
pub mod limits;

// Scanner module - token definitions, scanning implementation, and character codes
pub mod scanner;
pub use scanner::char_codes;
pub use scanner::scanner_impl;
pub use scanner::scanner_impl::*;
pub use scanner::*;
#[cfg(test)]
#[path = "tests/scanner_impl_tests.rs"]
mod scanner_impl_tests;
#[cfg(test)]
#[path = "tests/scanner_tests.rs"]
mod scanner_tests;

// Parser AST types (Phase 3)
pub mod parser;

// Syntax utilities - Shared helpers for AST and transforms
pub mod syntax;

// Parser - Cache-optimized parser using NodeArena (Phase 0.1)
#[cfg(test)]
#[path = "tests/parser_state_tests.rs"]
mod parser_state_tests;

// Regex flag error detection tests
#[cfg(test)]
#[path = "tests/regex_flag_tests.rs"]
mod regex_flag_tests;

// Binder types and implementation (Phase 4)
pub mod binder;

// BinderState - Binder using NodeArena (Phase 0.1)
#[cfg(test)]
#[path = "tests/binder_state_tests.rs"]
mod binder_state_tests;

// Module Resolution Debugging - Logging for symbol table operations and scope lookups
pub mod module_resolution_debug;

// Lib Loader - Load and merge lib.d.ts symbols into the binder (BIND-10)
pub mod lib_loader;

// Embedded TypeScript Library Files - Only compiled when embedded_libs feature is enabled
// Not used at runtime - lib files are loaded from disk like tsgo
#[cfg(feature = "embedded_libs")]
pub mod embedded_libs;
#[cfg(feature = "embedded_libs")]
pub use embedded_libs::{EmbeddedLib, get_default_libs_for_target, get_lib, get_libs_for_target};

// Pre-parsed TypeScript Library Files - For faster startup (requires embedded_libs)
#[cfg(all(feature = "preparsed_libs", feature = "embedded_libs"))]
pub mod preparsed_libs;
#[cfg(all(feature = "preparsed_libs", feature = "embedded_libs"))]
pub use preparsed_libs::{
    PreParsedLib, PreParsedLibs, generate_and_write_cache, get_preparsed_libs, has_preparsed_libs,
    load_preparsed_libs_for_target,
};

// Checker types and implementation (Phase 5)
pub mod checker;

#[cfg(test)]
#[path = "tests/checker_state_tests.rs"]
mod checker_state_tests;
pub use checker::state::{CheckerState, MAX_CALL_DEPTH, MAX_INSTANTIATION_DEPTH};

// Emitter - Emitter using NodeArena (Phase 0.1)
pub mod emitter;
#[cfg(test)]
#[path = "tests/transform_api_tests.rs"]
mod transform_api_tests;

// Printer - Clean, safe AST-to-JavaScript printer
pub mod printer;
#[cfg(test)]
#[path = "tests/printer_tests.rs"]
mod printer_tests;

// Span - Source location tracking (byte offsets)
pub mod span;

// SourceFile - Owns source text and provides &str references
pub mod source_file;

// Diagnostics - Error collection, formatting, and reporting
pub mod diagnostics;

// Enums - Enum support including const enum inlining
pub mod enums;

// Parallel processing with Rayon (Phase 0.4)
pub mod parallel;

// Comment preservation (Phase 6.3)
pub mod comments;
#[cfg(test)]
#[path = "tests/comments_tests.rs"]
mod comments_tests;

// Source Map generation (Phase 6.2)
pub mod source_map;
#[cfg(test)]
#[path = "tests/source_map_test_utils.rs"]
mod source_map_test_utils;
#[cfg(test)]
#[path = "tests/source_map_tests_1.rs"]
mod source_map_tests_1;
#[cfg(test)]
#[path = "tests/source_map_tests_2.rs"]
mod source_map_tests_2;
#[cfg(test)]
#[path = "tests/source_map_tests_3.rs"]
mod source_map_tests_3;
#[cfg(test)]
#[path = "tests/source_map_tests_4.rs"]
mod source_map_tests_4;

// SourceWriter - Abstraction for emitter output with source map tracking
pub mod source_writer;

// EmitContext - Transform state management for the emitter
pub mod emit_context;

// TransformContext - Projection layer for AST transforms (Phase 6.1)
pub mod transform_context;

// LoweringPass - Phase 1 of Transform/Print architecture (Phase 6.1)
pub mod lowering_pass;

// Declaration file emitter (Phase 6.4)
pub mod declaration_emitter;

// JavaScript transforms (Phase 6.5+)
pub mod transforms;

// Query-based Structural Solver (Phase 7.5)
pub mod solver;

// LSP (Language Server Protocol) support
pub mod lsp;

// Test Harness - Infrastructure for unit and conformance tests
#[cfg(test)]
#[path = "tests/test_harness.rs"]
mod test_harness;

// Isolated Test Runner - Process-based test execution with resource limits
#[cfg(test)]
#[path = "tests/isolated_test_runner.rs"]
mod isolated_test_runner;

// Tracing configuration (text / tree / JSON output for debugging)
#[cfg(not(target_arch = "wasm32"))]
pub mod tracing_config;

// Native CLI (non-wasm targets only)
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

// WASM integration module - parallel type checking exports
pub mod wasm;
pub use wasm::{WasmParallelChecker, WasmParallelParser, WasmTypeInterner};

// TypeScript API compatibility layer - exposes TS-compatible APIs via WASM
pub mod wasm_api;
pub use wasm_api::{
    TsDiagnostic, TsProgram, TsSignature, TsSourceFile, TsSymbol, TsType, TsTypeChecker,
};

// Module Resolution Infrastructure (non-wasm targets only - requires file system access)
#[cfg(not(target_arch = "wasm32"))]
pub mod module_resolver;
#[cfg(not(target_arch = "wasm32"))]
pub use module_resolver::{ModuleExtension, ModuleResolver, ResolutionFailure, ResolvedModule};

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

use crate::binder::BinderState;
use crate::checker::context::LibContext;
use crate::emit_context::EmitContext;
use crate::emitter::{Printer, PrinterOptions};
use crate::lib_loader::LibFile;
use crate::lowering_pass::LoweringPass;
use crate::lsp::diagnostics::convert_diagnostic;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::resolver::ScopeCache;
use crate::lsp::{
    CodeActionContext, CodeActionProvider, Completions, DocumentSymbolProvider, FindReferences,
    GoToDefinition, HoverProvider, ImportCandidate, ImportCandidateKind, RenameProvider,
    SemanticTokensProvider, SignatureHelpProvider,
};
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::transform_context::TransformContext;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportCandidateInput {
    module_specifier: String,
    local_name: String,
    kind: String,
    export_name: Option<String>,
    #[serde(default)]
    is_type_only: bool,
}

/// Compiler options passed from JavaScript/WASM.
/// Maps to TypeScript compiler options.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields are deserialized but some not yet used
struct CompilerOptions {
    /// Enable all strict type checking options.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict: Option<bool>,

    /// Raise error on expressions and declarations with an implied 'any' type.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_any: Option<bool>,

    /// Enable strict null checks.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_null_checks: Option<bool>,

    /// Enable strict checking of function types.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_function_types: Option<bool>,

    /// Enable strict property initialization checks in classes.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    strict_property_initialization: Option<bool>,

    /// Report error when not all code paths in function return a value.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_returns: Option<bool>,

    /// Raise error on 'this' expressions with an implied 'any' type.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_implicit_this: Option<bool>,

    /// Specify ECMAScript target version (accepts string like "ES5" or numeric).
    #[serde(default, deserialize_with = "deserialize_target_or_module")]
    target: Option<u32>,

    /// Specify module code generation (accepts string like "CommonJS" or numeric).
    #[serde(default, deserialize_with = "deserialize_target_or_module")]
    module: Option<u32>,

    /// When true, do not include any library files.
    #[serde(default, deserialize_with = "deserialize_bool_option")]
    no_lib: Option<bool>,
}

/// Deserialize a boolean option that can be a boolean, string, or comma-separated string.
/// TypeScript test files often have boolean options like "true, false" for different test cases.
fn deserialize_bool_option<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct BoolOptionVisitor;

    impl<'de> Visitor<'de> for BoolOptionVisitor {
        type Value = Option<bool>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean, string, or comma-separated list of booleans")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Handle comma-separated values like "true, false" - take the first value
            let first_value = value.split(',').next().unwrap_or(value).trim();
            let result = match first_value.to_lowercase().as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            };
            Ok(result)
        }
    }

    deserializer.deserialize_any(BoolOptionVisitor)
}

/// Deserialize target/module values that can be either strings or numbers.
/// TypeScript test files often use strings like "ES5", "ES2015", "CommonJS", etc.
fn deserialize_target_or_module<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct TargetOrModuleVisitor;

    impl<'de> Visitor<'de> for TargetOrModuleVisitor {
        type Value = Option<u32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or integer representing target/module")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value as u32))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value as u32))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Parse string values to their TypeScript enum equivalents
            // Note: For shared values like ES2015/ES6, we use the ScriptTarget value
            // because both target and module use the same match arm
            let result = match value.to_uppercase().as_str() {
                // ScriptTarget values (0-10, 99)
                "ES3" => 0,
                "ES5" => 1,
                "ES2015" | "ES6" => 2,
                "ES2016" => 3,
                "ES2017" => 4,
                "ES2018" => 5,
                "ES2019" => 6,
                "ES2020" => 7,
                "ES2021" => 8,
                "ES2022" => 9,
                "ES2023" => 10,
                "ESNEXT" => 99,
                // ModuleKind-specific values
                "NONE" => 0,
                "COMMONJS" => 1,
                "AMD" => 2,
                "UMD" => 3,
                "SYSTEM" => 4,
                "NODE16" => 100,
                "NODENEXT" => 199,
                _ => return Ok(None), // Unknown value, treat as unset
            };
            Ok(Some(result))
        }
    }

    deserializer.deserialize_any(TargetOrModuleVisitor)
}

impl CompilerOptions {
    /// Resolve a boolean option with strict mode fallback.
    /// If the specific option is set, use it; otherwise, fall back to strict mode.
    fn resolve_bool(&self, specific: Option<bool>, strict_implies: bool) -> bool {
        if let Some(value) = specific {
            return value;
        }
        if strict_implies {
            return self.strict.unwrap_or(false);
        }
        false
    }

    /// Get the effective value for noImplicitAny.
    pub fn get_no_implicit_any(&self) -> bool {
        self.resolve_bool(self.no_implicit_any, true)
    }

    /// Get the effective value for strictNullChecks.
    pub fn get_strict_null_checks(&self) -> bool {
        self.resolve_bool(self.strict_null_checks, true)
    }

    /// Get the effective value for strictFunctionTypes.
    pub fn get_strict_function_types(&self) -> bool {
        self.resolve_bool(self.strict_function_types, true)
    }

    /// Get the effective value for strictPropertyInitialization.
    pub fn get_strict_property_initialization(&self) -> bool {
        self.resolve_bool(self.strict_property_initialization, true)
    }

    /// Get the effective value for noImplicitReturns.
    pub fn get_no_implicit_returns(&self) -> bool {
        self.resolve_bool(self.no_implicit_returns, false)
    }

    /// Get the effective value for noImplicitThis.
    pub fn get_no_implicit_this(&self) -> bool {
        self.resolve_bool(self.no_implicit_this, true)
    }

    fn resolve_target(&self) -> crate::checker::context::ScriptTarget {
        use crate::checker::context::ScriptTarget;

        match self.target {
            Some(0) => ScriptTarget::ES3,
            Some(1) => ScriptTarget::ES5,
            Some(2) => ScriptTarget::ES2015,
            Some(3) => ScriptTarget::ES2016,
            Some(4) => ScriptTarget::ES2017,
            Some(5) => ScriptTarget::ES2018,
            Some(6) => ScriptTarget::ES2019,
            Some(7) => ScriptTarget::ES2020,
            Some(8) => ScriptTarget::ESNext,
            Some(9) => ScriptTarget::ESNext,
            Some(99) => ScriptTarget::ESNext,
            Some(_) => ScriptTarget::ESNext,
            None => ScriptTarget::default(),
        }
    }

    /// Convert to CheckerOptions for type checking.
    pub fn to_checker_options(&self) -> crate::checker::context::CheckerOptions {
        let strict = self.strict.unwrap_or(false);
        let strict_null_checks = self.get_strict_null_checks();
        crate::checker::context::CheckerOptions {
            strict,
            no_implicit_any: self.get_no_implicit_any(),
            no_implicit_returns: self.get_no_implicit_returns(),
            strict_null_checks,
            strict_function_types: self.get_strict_function_types(),
            strict_property_initialization: self.get_strict_property_initialization(),
            no_implicit_this: self.get_no_implicit_this(),
            use_unknown_in_catch_variables: strict_null_checks,
            isolated_modules: false,
            no_unchecked_indexed_access: false,
            strict_bind_call_apply: false,
            exact_optional_property_types: false,
            no_lib: self.no_lib.unwrap_or(false),
            no_types_and_symbols: false,
            target: self.resolve_target(),
            module: crate::common::ModuleKind::None, // WASM API: use self.module if available
            es_module_interop: false,
            allow_synthetic_default_imports: false,
            allow_unreachable_code: false,
            no_property_access_from_index_signature: false,
            sound_mode: false,
            experimental_decorators: false,
            no_unused_locals: false,
            no_unused_parameters: false,
        }
    }
}

impl TryFrom<ImportCandidateInput> for ImportCandidate {
    type Error = JsValue;

    fn try_from(input: ImportCandidateInput) -> Result<Self, Self::Error> {
        let local_name = input.local_name;
        let kind = match input.kind.as_str() {
            "named" => {
                let export_name = input.export_name.unwrap_or_else(|| local_name.clone());
                ImportCandidateKind::Named { export_name }
            }
            "default" => ImportCandidateKind::Default,
            "namespace" => ImportCandidateKind::Namespace,
            other => {
                return Err(JsValue::from_str(&format!(
                    "Unsupported import candidate kind: {}",
                    other
                )));
            }
        };

        Ok(ImportCandidate {
            module_specifier: input.module_specifier,
            local_name,
            kind,
            is_type_only: input.is_type_only,
        })
    }
}

/// Opaque wrapper for transform directives across the wasm boundary.
#[wasm_bindgen]
pub struct WasmTransformContext {
    inner: TransformContext,
    target_es5: bool,
    module_kind: ModuleKind,
}

#[wasm_bindgen]
impl WasmTransformContext {
    /// Get the number of transform directives generated.
    #[wasm_bindgen(js_name = getCount)]
    pub fn get_count(&self) -> usize {
        self.inner.len()
    }
}

/// High-performance parser using Node architecture (16 bytes/node).
/// This is the optimized path for Phase 8 test suite evaluation.
#[wasm_bindgen]
pub struct Parser {
    parser: ParserState,
    source_file_idx: Option<parser::NodeIndex>,
    binder: Option<BinderState>,
    /// Local type interner for single-file checking.
    /// For multi-file compilation, use MergedProgram.type_interner instead.
    type_interner: TypeInterner,
    /// Line map for LSP position conversion (lazy initialized)
    line_map: Option<LineMap>,
    /// Persistent cache for type checking results across LSP queries.
    /// Invalidated when the file changes.
    type_cache: Option<checker::TypeCache>,
    /// Persistent cache for scope resolution across LSP queries.
    /// Invalidated when the file changes.
    scope_cache: ScopeCache,
    /// Pre-loaded lib files (parsed and bound) for global type resolution
    lib_files: Vec<Arc<LibFile>>,
    /// Compiler options for type checking
    compiler_options: CompilerOptions,
}

#[wasm_bindgen]
impl Parser {
    /// Create a new Parser for the given source file.
    #[wasm_bindgen(constructor)]
    pub fn new(file_name: String, source_text: String) -> Parser {
        Parser {
            parser: ParserState::new(file_name, source_text),
            source_file_idx: None,
            binder: None,
            type_interner: TypeInterner::new(),
            line_map: None,
            type_cache: None,
            scope_cache: ScopeCache::default(),
            lib_files: Vec::new(),
            compiler_options: CompilerOptions::default(),
        }
    }

    /// Set compiler options from JSON.
    ///
    /// # Arguments
    /// * `options_json` - JSON string containing compiler options
    ///
    /// # Example
    /// ```javascript
    /// const parser = new Parser("file.ts", "const x = 1;");
    /// parser.setCompilerOptions(JSON.stringify({
    ///   strict: true,
    ///   noImplicitAny: true,
    ///   strictNullChecks: true
    /// }));
    /// ```
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, options_json: &str) -> Result<(), JsValue> {
        match serde_json::from_str::<CompilerOptions>(options_json) {
            Ok(options) => {
                self.compiler_options = options;
                // Invalidate type cache when compiler options change
                self.type_cache = None;
                Ok(())
            }
            Err(e) => Err(JsValue::from_str(&format!(
                "Failed to parse compiler options: {}",
                e
            ))),
        }
    }

    /// Add a lib file (e.g., lib.es5.d.ts) for global type resolution.
    /// The lib file will be parsed and bound, and its global symbols will be
    /// available during binding and type checking.
    #[wasm_bindgen(js_name = addLibFile)]
    pub fn add_lib_file(&mut self, file_name: String, source_text: String) {
        let mut lib_parser = ParserState::new(file_name.clone(), source_text);
        let source_file_idx = lib_parser.parse_source_file();

        let mut lib_binder = BinderState::new();
        lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

        // Wrap in Arc for sharing with LibContext during type checking
        let arena = Arc::new(lib_parser.into_arena());
        let binder = Arc::new(lib_binder);

        // Create lib_loader::LibFile
        let lib_file = Arc::new(LibFile::new(file_name, arena, binder));

        self.lib_files.push(lib_file);

        // Invalidate binder since we have new global symbols
        self.binder = None;
        self.type_cache = None;
    }

    /// Parse the source file and return the root node index.
    #[wasm_bindgen(js_name = parseSourceFile)]
    pub fn parse_source_file(&mut self) -> u32 {
        let idx = self.parser.parse_source_file();
        self.source_file_idx = Some(idx);
        // Invalidate derived state on re-parse
        self.line_map = None;
        self.binder = None;
        self.type_cache = None; // Invalidate type cache when file changes
        self.scope_cache.clear();
        idx.0
    }

    /// Get the number of nodes in the AST.
    #[wasm_bindgen(js_name = getNodeCount)]
    pub fn get_node_count(&self) -> usize {
        self.parser.get_node_count()
    }

    /// Get parse diagnostics as JSON.
    #[wasm_bindgen(js_name = getDiagnosticsJson)]
    pub fn get_diagnostics_json(&self) -> String {
        let diags: Vec<_> = self
            .parser
            .get_diagnostics()
            .iter()
            .map(|d| {
                serde_json::json!({
                    "message": d.message,
                    "start": d.start,
                    "length": d.length,
                    "code": d.code,
                })
            })
            .collect();
        serde_json::to_string(&diags).unwrap_or_else(|_| "[]".to_string())
    }

    /// Bind the source file and return symbol count.
    #[wasm_bindgen(js_name = bindSourceFile)]
    pub fn bind_source_file(&mut self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let mut binder = BinderState::new();
            // Use bind_source_file_with_libs to merge lib symbols into the binder
            // This properly remaps SymbolIds to avoid collisions across lib files
            binder.bind_source_file_with_libs(self.parser.get_arena(), root_idx, &self.lib_files);

            // Collect symbol names for the result
            let symbols: std::collections::HashMap<String, u32> = binder
                .file_locals
                .iter()
                .map(|(name, id)| (name.clone(), id.0))
                .collect();

            let result = serde_json::json!({
                "symbols": symbols,
                "symbolCount": binder.symbols.len(),
            });

            self.binder = Some(binder);
            self.scope_cache.clear();
            serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
        } else {
            r#"{"error": "Source file not parsed"}"#.to_string()
        }
    }

    /// Type check the source file and return diagnostics.
    #[wasm_bindgen(js_name = checkSourceFile)]
    pub fn check_source_file(&mut self) -> String {
        if self.binder.is_none() {
            // Auto-bind if not done yet
            if self.source_file_idx.is_some() {
                self.bind_source_file();
            }
        }

        if let (Some(root_idx), Some(binder)) = (self.source_file_idx, &self.binder) {
            let file_name = self.parser.get_file_name().to_string();

            // Get compiler options
            let checker_options = self.compiler_options.to_checker_options();
            let mut checker = if let Some(cache) = self.type_cache.take() {
                CheckerState::with_cache_and_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    cache,
                    &checker_options,
                )
            } else {
                CheckerState::with_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    &checker_options,
                )
            };

            // Set up lib contexts for global type resolution (Object, Array, etc.)
            if !self.lib_files.is_empty() {
                let lib_contexts: Vec<LibContext> = self
                    .lib_files
                    .iter()
                    .map(|lib| LibContext {
                        arena: Arc::clone(&lib.arena),
                        binder: Arc::clone(&lib.binder),
                    })
                    .collect();
                checker.ctx.set_lib_contexts(lib_contexts);
            }

            // Full source file type checking - traverse all statements
            checker.check_source_file(root_idx);

            let diagnostics = checker
                .ctx
                .diagnostics
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "message_text": d.message_text.clone(),
                        "code": d.code,
                        "start": d.start,
                        "length": d.length,
                        "category": format!("{:?}", d.category),
                    })
                })
                .collect::<Vec<_>>();

            self.type_cache = Some(checker.extract_cache());

            let result = serde_json::json!({
                "typeCount": self.type_interner.len(),
                "diagnostics": diagnostics,
            });

            serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
        } else {
            r#"{"error": "Source file not parsed or bound"}"#.to_string()
        }
    }

    /// Get the type of a node as a string.
    #[wasm_bindgen(js_name = getTypeOfNode)]
    pub fn get_type_of_node(&mut self, node_idx: u32) -> String {
        if let (Some(_), Some(binder)) = (self.source_file_idx, &self.binder) {
            let file_name = self.parser.get_file_name().to_string();

            // Get compiler options
            let checker_options = self.compiler_options.to_checker_options();
            let mut checker = if let Some(cache) = self.type_cache.take() {
                CheckerState::with_cache_and_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    cache,
                    &checker_options,
                )
            } else {
                CheckerState::with_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    &checker_options,
                )
            };

            let type_id = checker.get_type_of_node(parser::NodeIndex(node_idx));
            // Use format_type for human-readable output
            let result = checker.format_type(type_id);
            self.type_cache = Some(checker.extract_cache());
            result
        } else {
            "unknown".to_string()
        }
    }

    /// Emit the source file as JavaScript (ES5 target, auto-detect CommonJS for modules).
    #[wasm_bindgen(js_name = emit)]
    pub fn emit(&self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let mut options = PrinterOptions::default();
            options.target = ScriptTarget::ES5;

            let mut ctx = EmitContext::with_options(options);
            ctx.auto_detect_module = true;

            self.emit_with_context(root_idx, ctx)
        } else {
            String::new()
        }
    }

    /// Emit the source file as JavaScript (ES6+ modern output).
    #[wasm_bindgen(js_name = emitModern)]
    pub fn emit_modern(&self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let mut options = PrinterOptions::default();
            options.target = ScriptTarget::ES2015;

            let ctx = EmitContext::with_options(options);

            self.emit_with_context(root_idx, ctx)
        } else {
            String::new()
        }
    }

    fn emit_with_context(&self, root_idx: parser::NodeIndex, ctx: EmitContext) -> String {
        let transforms = LoweringPass::new(self.parser.get_arena(), &ctx).run(root_idx);

        let mut printer = Printer::with_transforms_and_options(
            self.parser.get_arena(),
            transforms,
            ctx.options.clone(),
        );
        printer.set_target_es5(ctx.target_es5);
        printer.set_auto_detect_module(ctx.auto_detect_module);
        printer.set_source_text(self.parser.get_source_text());
        printer.emit(root_idx);
        printer.get_output().to_string()
    }

    /// Generate transform directives based on compiler options.
    #[wasm_bindgen(js_name = generateTransforms)]
    pub fn generate_transforms(&self, target: u32, module: u32) -> WasmTransformContext {
        let mut options = PrinterOptions::default();
        options.target = match target {
            0 => ScriptTarget::ES3,
            1 => ScriptTarget::ES5,
            2 => ScriptTarget::ES2015,
            3 => ScriptTarget::ES2016,
            4 => ScriptTarget::ES2017,
            5 => ScriptTarget::ES2018,
            6 => ScriptTarget::ES2019,
            7 => ScriptTarget::ES2020,
            8 => ScriptTarget::ES2021,
            9 => ScriptTarget::ES2022,
            _ => ScriptTarget::ESNext,
        };
        options.module = match module {
            0 => ModuleKind::None,
            1 => ModuleKind::CommonJS,
            2 => ModuleKind::AMD,
            3 => ModuleKind::UMD,
            4 => ModuleKind::System,
            5 => ModuleKind::ES2015,
            6 => ModuleKind::ES2020,
            7 => ModuleKind::ES2022,
            99 => ModuleKind::ESNext,
            100 => ModuleKind::Node16,
            199 => ModuleKind::NodeNext,
            _ => ModuleKind::None,
        };

        let ctx = EmitContext::with_options(options);
        let transforms = if let Some(root_idx) = self.source_file_idx {
            let lowering = LoweringPass::new(self.parser.get_arena(), &ctx);
            lowering.run(root_idx)
        } else {
            TransformContext::new()
        };

        WasmTransformContext {
            inner: transforms,
            target_es5: ctx.target_es5,
            module_kind: ctx.options.module,
        }
    }

    /// Emit the source file using pre-computed transforms.
    #[wasm_bindgen(js_name = emitWithTransforms)]
    pub fn emit_with_transforms(&self, context: &WasmTransformContext) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let mut printer =
                Printer::with_transforms(self.parser.get_arena(), context.inner.clone());
            printer.set_target_es5(context.target_es5);
            printer.set_module_kind(context.module_kind);
            printer.set_source_text(self.parser.get_source_text());
            printer.emit(root_idx);
            printer.get_output().to_string()
        } else {
            String::new()
        }
    }

    /// Get the AST as JSON (for debugging).
    #[wasm_bindgen(js_name = getAstJson)]
    pub fn get_ast_json(&self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let arena = self.parser.get_arena();
            format!(
                "{{\"nodeCount\": {}, \"rootIdx\": {}}}",
                arena.len(),
                root_idx.0
            )
        } else {
            "{}".to_string()
        }
    }

    /// Debug type lowering - trace what happens when lowering an interface type
    #[wasm_bindgen(js_name = debugTypeLowering)]
    pub fn debug_type_lowering(&self, interface_name: &str) -> String {
        use parser::syntax_kind_ext;
        use solver::{TypeKey, TypeLowering};

        let arena = self.parser.get_arena();
        let mut result = Vec::new();

        // Find the interface declaration
        let mut interface_decls = Vec::new();
        for i in 0..arena.len() {
            let idx = parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(idx)
                && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && let Some(interface) = arena.get_interface(node)
                && let Some(name_node) = arena.get(interface.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == interface_name
            {
                interface_decls.push(idx);
            }
        }

        if interface_decls.is_empty() {
            return format!("Interface '{}' not found", interface_name);
        }

        result.push(format!(
            "Found {} declaration(s) for '{}'",
            interface_decls.len(),
            interface_name
        ));

        // Lower the interface
        let lowering = TypeLowering::new(arena, &self.type_interner);
        let type_id = lowering.lower_interface_declarations(&interface_decls);

        result.push(format!("Lowered type ID: {:?}", type_id));

        // Inspect the result
        if let Some(key) = self.type_interner.lookup(type_id) {
            result.push(format!("Type key: {:?}", key));
            if let TypeKey::Object(shape_id) = key {
                let shape = self.type_interner.object_shape(shape_id);
                result.push(format!(
                    "Object shape properties: {}",
                    shape.properties.len()
                ));
                for prop in &shape.properties {
                    let name = self.type_interner.resolve_atom(prop.name);
                    result.push(format!(
                        "  Property '{}': type_id={:?}, optional={}",
                        name, prop.type_id, prop.optional
                    ));
                    // Try to show what the type_id resolves to
                    if let Some(prop_key) = self.type_interner.lookup(prop.type_id) {
                        result.push(format!("    -> {:?}", prop_key));
                    }
                }
            }
        }

        result.join("\n")
    }

    /// Debug interface parsing - dump interface members for diagnostics
    #[wasm_bindgen(js_name = debugInterfaceMembers)]
    pub fn debug_interface_members(&self, interface_name: &str) -> String {
        use parser::syntax_kind_ext;

        let arena = self.parser.get_arena();
        let mut result = Vec::new();

        for i in 0..arena.len() {
            let idx = parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(idx)
                && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && let Some(interface) = arena.get_interface(node)
                && let Some(name_node) = arena.get(interface.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == interface_name
            {
                result.push(format!(
                    "Interface '{}' found at node {}",
                    interface_name, i
                ));
                result.push(format!("  members list: {:?}", interface.members.nodes));

                for (mi, &member_idx) in interface.members.nodes.iter().enumerate() {
                    if let Some(member_node) = arena.get(member_idx) {
                        result.push(format!(
                            "  Member {} (idx {}): kind={}",
                            mi, member_idx.0, member_node.kind
                        ));
                        result.push(format!("    data_index: {}", member_node.data_index));
                        if let Some(sig) = arena.get_signature(member_node) {
                            result.push(format!("    name_idx: {:?}", sig.name));
                            result.push(format!(
                                "    type_annotation_idx: {:?}",
                                sig.type_annotation
                            ));

                            // Get name text
                            if let Some(name_n) = arena.get(sig.name) {
                                if let Some(name_id) = arena.get_identifier(name_n) {
                                    result
                                        .push(format!("    name_text: '{}'", name_id.escaped_text));
                                } else {
                                    result.push(format!("    name_node kind: {}", name_n.kind));
                                }
                            }

                            // Get type annotation text
                            if let Some(type_n) = arena.get(sig.type_annotation) {
                                if let Some(type_id) = arena.get_identifier(type_n) {
                                    result
                                        .push(format!("    type_text: '{}'", type_id.escaped_text));
                                } else {
                                    result.push(format!("    type_node kind: {}", type_n.kind));
                                }
                            }
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            format!("Interface '{}' not found", interface_name)
        } else {
            result.join("\n")
        }
    }

    /// Debug namespace scoping - dump scope info for all scopes
    #[wasm_bindgen(js_name = debugScopes)]
    pub fn debug_scopes(&self) -> String {
        let Some(binder) = &self.binder else {
            return "Binder not initialized. Call parseSourceFile and bindSourceFile first."
                .to_string();
        };

        let mut result = Vec::new();
        result.push(format!(
            "=== Persistent Scopes ({}) ===",
            binder.scopes.len()
        ));

        for (i, scope) in binder.scopes.iter().enumerate() {
            result.push(format!(
                "\nScope {} (parent: {:?}, kind: {:?}):",
                i, scope.parent, scope.kind
            ));
            result.push(format!("  table entries: {}", scope.table.len()));
            for (name, sym_id) in scope.table.iter() {
                if let Some(sym) = binder.symbols.get(*sym_id) {
                    result.push(format!(
                        "    '{}' -> SymbolId({}) [flags: 0x{:x}]",
                        name, sym_id.0, sym.flags
                    ));
                } else {
                    result.push(format!(
                        "    '{}' -> SymbolId({}) [MISSING SYMBOL]",
                        name, sym_id.0
                    ));
                }
            }
        }

        result.push(format!(
            "\n=== Node -> Scope Mappings ({}) ===",
            binder.node_scope_ids.len()
        ));
        for (&node_idx, &scope_id) in binder.node_scope_ids.iter() {
            result.push(format!(
                "  NodeIndex({}) -> ScopeId({})",
                node_idx, scope_id.0
            ));
        }

        result.push(format!(
            "\n=== File Locals ({}) ===",
            binder.file_locals.len()
        ));
        for (name, sym_id) in binder.file_locals.iter() {
            result.push(format!("  '{}' -> SymbolId({})", name, sym_id.0));
        }

        result.join("\n")
    }

    /// Trace the parent chain for a node at a given position
    #[wasm_bindgen(js_name = traceParentChain)]
    pub fn trace_parent_chain(&self, pos: u32) -> String {
        const IDENTIFIER_KIND: u16 = 80; // SyntaxKind::Identifier
        let arena = self.parser.get_arena();
        let binder = match &self.binder {
            Some(b) => b,
            None => return "Binder not initialized".to_string(),
        };

        let mut result = Vec::new();
        result.push(format!("=== Tracing parent chain for position {} ===", pos));

        // Find node at position
        let mut target_node = None;
        for i in 0..arena.len() {
            let idx = parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(idx)
                && node.pos <= pos
                && pos < node.end
                && node.kind == IDENTIFIER_KIND
            {
                target_node = Some(idx);
                // Don't break - prefer smaller range
            }
        }

        let start_idx = match target_node {
            Some(idx) => idx,
            None => return format!("No identifier node found at position {}", pos),
        };

        result.push(format!("Starting node: {:?}", start_idx));

        let mut current = start_idx;
        let mut depth = 0;
        while !current.is_none() && depth < 20 {
            if let Some(node) = arena.get(current) {
                let kind_name = format!("kind={}", node.kind);
                let scope_info = if let Some(&scope_id) = binder.node_scope_ids.get(&current.0) {
                    format!(" -> ScopeId({})", scope_id.0)
                } else {
                    String::new()
                };
                result.push(format!(
                    "  [{}] NodeIndex({}) {} [pos:{}-{}]{}",
                    depth, current.0, kind_name, node.pos, node.end, scope_info
                ));
            }

            if let Some(ext) = arena.get_extended(current) {
                if ext.parent.is_none() {
                    result.push(format!("  [{}] Parent is NodeIndex::NONE", depth + 1));
                    break;
                }
                current = ext.parent;
            } else {
                result.push(format!(
                    "  [{}] No extended info for NodeIndex({})",
                    depth + 1,
                    current.0
                ));
                break;
            }
            depth += 1;
        }

        result.join("\n")
    }

    /// Dump variable declaration info for debugging
    #[wasm_bindgen(js_name = dumpVarDecl)]
    pub fn dump_var_decl(&self, var_decl_idx: u32) -> String {
        let arena = self.parser.get_arena();
        let idx = parser::NodeIndex(var_decl_idx);

        let Some(node) = arena.get(idx) else {
            return format!("NodeIndex({}) not found", var_decl_idx);
        };

        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return format!(
                "NodeIndex({}) is not a VARIABLE_DECLARATION (kind={})",
                var_decl_idx, node.kind
            );
        };

        format!(
            "VariableDeclaration({}):\n  name: NodeIndex({})\n  type_annotation: NodeIndex({}) (is_none={})\n  initializer: NodeIndex({})",
            var_decl_idx,
            var_decl.name.0,
            var_decl.type_annotation.0,
            var_decl.type_annotation.is_none(),
            var_decl.initializer.0
        )
    }

    /// Dump all nodes for debugging
    #[wasm_bindgen(js_name = dumpAllNodes)]
    pub fn dump_all_nodes(&self, start: u32, count: u32) -> String {
        let arena = self.parser.get_arena();
        let mut result = Vec::new();

        for i in start..(start + count).min(arena.len() as u32) {
            let idx = parser::NodeIndex(i);
            if let Some(node) = arena.get(idx) {
                let parent_str = if let Some(ext) = arena.get_extended(idx) {
                    if ext.parent.is_none() {
                        "parent:NONE".to_string()
                    } else {
                        format!("parent:{}", ext.parent.0)
                    }
                } else {
                    "no-ext".to_string()
                };
                // Add identifier text if available
                let extra = if let Some(ident) = arena.get_identifier(node) {
                    format!(" \"{}\"", ident.escaped_text)
                } else {
                    String::new()
                };
                result.push(format!(
                    "  NodeIndex({}) kind={} [pos:{}-{}] {}{}",
                    i, node.kind, node.pos, node.end, parent_str, extra
                ));
            }
        }

        result.join("\n")
    }

    // =========================================================================
    // LSP Feature Methods
    // =========================================================================

    /// Ensure internal LineMap is built.
    fn ensure_line_map(&mut self) {
        if self.line_map.is_none() {
            self.line_map = Some(LineMap::build(self.parser.get_source_text()));
        }
    }

    /// Ensure source file is parsed and bound.
    fn ensure_bound(&mut self) -> Result<(), JsValue> {
        if self.source_file_idx.is_none() {
            return Err(JsValue::from_str("Source file not parsed"));
        }
        if self.binder.is_none() {
            self.bind_source_file();
        }
        Ok(())
    }

    /// Go to Definition: Returns array of Location objects.
    #[wasm_bindgen(js_name = getDefinitionAtPosition)]
    pub fn get_definition_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = GoToDefinition::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        let result =
            provider.get_definition_with_scope_cache(root, pos, &mut self.scope_cache, None);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Find References: Returns array of Location objects.
    #[wasm_bindgen(js_name = getReferencesAtPosition)]
    pub fn get_references_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = FindReferences::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        let result =
            provider.find_references_with_scope_cache(root, pos, &mut self.scope_cache, None);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Completions: Returns array of CompletionItem objects.
    #[wasm_bindgen(js_name = getCompletionsAtPosition)]
    pub fn get_completions_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();
        let file_name = self.parser.get_file_name().to_string();

        let provider = Completions::new_with_types(
            self.parser.get_arena(),
            binder,
            line_map,
            &self.type_interner,
            source_text,
            file_name,
        );
        let pos = Position::new(line, character);

        let result = provider.get_completions_with_caches(
            root,
            pos,
            &mut self.type_cache,
            &mut self.scope_cache,
            None,
        );
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Hover: Returns HoverInfo object.
    #[wasm_bindgen(js_name = getHoverAtPosition)]
    pub fn get_hover_at_position(&mut self, line: u32, character: u32) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();
        let file_name = self.parser.get_file_name().to_string();

        let provider = HoverProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            &self.type_interner,
            source_text,
            file_name,
        );
        let pos = Position::new(line, character);

        let result = provider.get_hover_with_scope_cache(
            root,
            pos,
            &mut self.type_cache,
            &mut self.scope_cache,
            None,
        );
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Signature Help: Returns SignatureHelp object.
    #[wasm_bindgen(js_name = getSignatureHelpAtPosition)]
    pub fn get_signature_help_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();
        let file_name = self.parser.get_file_name().to_string();

        let provider = SignatureHelpProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            &self.type_interner,
            source_text,
            file_name,
        );
        let pos = Position::new(line, character);

        let result = provider.get_signature_help_with_scope_cache(
            root,
            pos,
            &mut self.type_cache,
            &mut self.scope_cache,
            None,
        );
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Document Symbols: Returns array of DocumentSymbol objects.
    #[wasm_bindgen(js_name = getDocumentSymbols)]
    pub fn get_document_symbols(&mut self) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();

        let provider = DocumentSymbolProvider::new(self.parser.get_arena(), line_map, source_text);

        let result = provider.get_document_symbols(root);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Semantic Tokens: Returns flat array of u32 (delta encoded).
    #[wasm_bindgen(js_name = getSemanticTokens)]
    pub fn get_semantic_tokens(&mut self) -> Result<Vec<u32>, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();

        let mut provider =
            SemanticTokensProvider::new(self.parser.get_arena(), binder, line_map, source_text);

        Ok(provider.get_semantic_tokens(root))
    }

    /// Rename - Prepare: Check if rename is valid at position.
    #[wasm_bindgen(js_name = prepareRename)]
    pub fn prepare_rename(&mut self, line: u32, character: u32) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Internal error: binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Internal error: line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = RenameProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        let result = provider.prepare_rename(pos);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Rename - Edits: Get workspace edits for rename.
    #[wasm_bindgen(js_name = getRenameEdits)]
    pub fn get_rename_edits(
        &mut self,
        line: u32,
        character: u32,
        new_name: String,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = RenameProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        match provider.provide_rename_edits_with_scope_cache(
            root,
            pos,
            new_name,
            &mut self.scope_cache,
            None,
        ) {
            Ok(edit) => Ok(serde_wasm_bindgen::to_value(&edit)?),
            Err(e) => Err(JsValue::from_str(&e)),
        }
    }

    /// Code Actions: Get code actions for a range.
    #[wasm_bindgen(js_name = getCodeActions)]
    pub fn get_code_actions(
        &mut self,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = CodeActionProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );

        let range = Range::new(
            Position::new(start_line, start_char),
            Position::new(end_line, end_char),
        );

        let context = CodeActionContext {
            diagnostics: Vec::new(),
            only: None,
            import_candidates: Vec::new(),
        };

        let result = provider.provide_code_actions(root, range, context);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Code Actions: Get code actions for a range with diagnostics context.
    #[wasm_bindgen(js_name = getCodeActionsWithContext)]
    pub fn get_code_actions_with_context(
        &mut self,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
        diagnostics: JsValue,
        only: JsValue,
        import_candidates: JsValue,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let diagnostics = if diagnostics.is_null() || diagnostics.is_undefined() {
            Vec::new()
        } else {
            serde_wasm_bindgen::from_value(diagnostics)?
        };

        let only = if only.is_null() || only.is_undefined() {
            None
        } else {
            Some(serde_wasm_bindgen::from_value(only)?)
        };

        let import_candidates = if import_candidates.is_null() || import_candidates.is_undefined() {
            Vec::new()
        } else {
            let inputs: Vec<ImportCandidateInput> =
                serde_wasm_bindgen::from_value(import_candidates)?;
            inputs
                .into_iter()
                .map(ImportCandidate::try_from)
                .collect::<Result<Vec<_>, _>>()?
        };

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = CodeActionProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );

        let range = Range::new(
            Position::new(start_line, start_char),
            Position::new(end_line, end_char),
        );

        let context = CodeActionContext {
            diagnostics,
            only,
            import_candidates,
        };

        let result = provider.provide_code_actions(root, range, context);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Diagnostics: Get checker diagnostics in LSP format.
    #[wasm_bindgen(js_name = getLspDiagnostics)]
    pub fn get_lsp_diagnostics(&mut self) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        // Get compiler options
        let checker_options = self.compiler_options.to_checker_options();

        let mut checker = if let Some(cache) = self.type_cache.take() {
            CheckerState::with_cache_and_options(
                self.parser.get_arena(),
                binder,
                &self.type_interner,
                file_name.clone(),
                cache,
                &checker_options,
            )
        } else {
            CheckerState::with_options(
                self.parser.get_arena(),
                binder,
                &self.type_interner,
                file_name.clone(),
                &checker_options,
            )
        };

        checker.check_source_file(root);

        let lsp_diagnostics: Vec<_> = checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| convert_diagnostic(diag, line_map, source_text))
            .collect();

        self.type_cache = Some(checker.extract_cache());

        Ok(serde_wasm_bindgen::to_value(&lsp_diagnostics)?)
    }
}

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

/// Result of checking a single file in a multi-file program
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FileCheckResultJson {
    file_name: String,
    parse_diagnostics: Vec<ParseDiagnosticJson>,
    check_diagnostics: Vec<CheckDiagnosticJson>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ParseDiagnosticJson {
    message: String,
    start: u32,
    length: u32,
    code: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckDiagnosticJson {
    message_text: String,
    code: u32,
    start: u32,
    length: u32,
    category: String,
}

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

#[wasm_bindgen]
impl WasmProgram {
    /// Create a new empty program.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmProgram {
        WasmProgram {
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
    /// The file_name should be a relative path like "src/a.ts".
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
                "Failed to parse compiler options: {}",
                e
            ))),
        }
    }

    /// Get the number of files in the program.
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
        let mut file_codes: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
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
        (Some(a), Some(b)) => {
            if a == b {
                Comparison::EqualTo
            } else if a < b {
                Comparison::LessThan
            } else {
                Comparison::GreaterThan
            }
        }
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
        format!("{}/", path)
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

// ASI Conformance tests for verifying TS1005/TS1109 patterns
#[cfg(test)]
#[path = "tests/asi_conformance_tests.rs"]
mod asi_conformance_tests;

#[cfg(test)]
#[path = "tests/debug_asi.rs"]
mod debug_asi;

// P1 Error Recovery tests for synchronization point improvements
#[cfg(test)]
#[path = "tests/p1_error_recovery_tests.rs"]
mod p1_error_recovery_tests;

// Strict null checks manual tests for TS18050/TS2531/2532 error codes
#[cfg(test)]
#[path = "checker/tests/strict_null_manual.rs"]
mod strict_null_manual;

// Generic type inference manual tests
#[cfg(test)]
#[path = "checker/tests/generic_inference_manual.rs"]
mod generic_inference_manual;
