use wasm_bindgen::prelude::wasm_bindgen;

mod api;

/// Shared WASM compiler-options DTO and its option -> `CheckerOptions`
/// resolution. Every WASM program surface (website `WasmProgram`, npm
/// wrapper `TsProgram` in `tsz-wasm`) must derive `CheckerOptions` through
/// this single owner so strict-family implication semantics cannot drift
/// per surface (#13117).
pub use api::wasm::compiler_options::CompilerOptions as WasmCompilerOptions;

// Shared test fixtures for reduced allocation overhead
#[cfg(test)]
#[path = "../tests/test_fixtures.rs"]
pub mod test_fixtures;

// Re-export foundation types from tsz-common workspace crate
pub use tsz_common::interner;
pub use tsz_common::interner::{AstAtom, Atom, Interner, ShardedInterner};
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
#[path = "../tests/variadic_indexed_constraint_inference_tests.rs"]
mod variadic_indexed_constraint_inference_tests;

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

// Diagnostics - re-exported so `tsz::diagnostics` resolves for downstream consumers
pub use tsz_common::diagnostics;

// SourceFile - Owns source text and provides &str references
pub mod source_file;

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

#[cfg(feature = "dts")]
pub use tsz_emitter::declaration_emitter;

// JavaScript transforms - re-exported from tsz-emitter workspace crate
pub use tsz_emitter::transforms;

// Query-based Structural Solver (Phase 7.5)
pub use tsz_solver::{self, self as solver_cache};

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
mod resolution;
#[cfg(not(target_arch = "wasm32"))]
pub use module_resolver::{
    ModuleExtension, ModuleLookupError, ModuleLookupOutcome, ModuleLookupRequest,
    ModuleLookupResult, ModuleResolver, ResolutionFailure, ResolvedModule,
};
pub(crate) use resolution::helpers as module_resolver_helpers;

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

pub use crate::api::wasm::transforms::WasmTransformContext;

pub use crate::api::wasm::parser::Parser;

/// Create a new Parser for the given source text.
/// This is the recommended parser for production use.
#[wasm_bindgen(js_name = createParser)]
pub fn create_parser(file_name: String, source_text: String) -> Parser {
    Parser::new(file_name, source_text)
}

pub use crate::api::wasm::program::WasmProgram;

/// Create a new multi-file program.
#[wasm_bindgen(js_name = createProgram)]
pub fn create_program() -> WasmProgram {
    WasmProgram::new()
}

pub use crate::api::wasm::core_utils::{
    ALT_DIRECTORY_SEPARATOR, Comparison, DIRECTORY_SEPARATOR, compare_strings_case_insensitive,
    compare_strings_case_insensitive_eslint_compatible, compare_strings_case_sensitive,
    ensure_trailing_directory_separator, equate_strings_case_insensitive,
    equate_strings_case_sensitive, file_extension_is, get_base_file_name, has_extension,
    has_trailing_directory_separator, is_any_directory_separator, is_ascii_letter, is_digit,
    is_hex_digit, is_line_break, is_octal_digit, is_white_space_like, is_white_space_single_line,
    is_word_character, normalize_slashes, path_is_relative, remove_trailing_directory_separator,
    to_file_name_lower_case,
};

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

// Void return exception tests
#[cfg(test)]
#[path = "../../tsz-checker/tests/void_return_exception.rs"]
mod void_return_exception;

// `function_bivariance`, `global_type_tests`, and `ts2304_tests` (#13633), plus
// `any_propagation`, `any_propagation_tests`, `const_assertion_tests`, and
// `freshness_stripping_tests` (#13109) are owned by `tsz-checker`: they depend
// only on `tsz_checker::*` (via `tsz_checker::test_utils::check_source_with_libs`
// with no lib files), not on `tsz-core`'s `test_fixtures`. They run in
// `tsz-checker`'s own test runner via `[[test]]` entries; do not re-mount here.
//
// `constructor_accessibility`, `symbol_resolution_tests`, `ts2305_tests`,
// `ts2306_tests`, `ts2498_export_star_export_equals_tests`, and
// `widening_integration_tests` relocated to `tsz-checker` (own `src/lib.rs`
// `#[path]` mounts) on top of that crate's `test_utils` helpers, replacing
// their prior dependency on `tsz-core`'s `test_fixtures`
// (`TestContext`/`setup_lib_contexts`/`merge_shared_lib_symbols`); do not
// re-mount them here.
