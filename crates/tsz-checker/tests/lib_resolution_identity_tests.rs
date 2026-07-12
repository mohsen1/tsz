//! Tests for lib-resolution stable identity path.
//!
//! These tests verify that lib type lowering uses the stable DefId identity
//! path (via `resolve_lib_node_in_arenas` + `get_lib_def_id`) instead of
//! on-demand DefId creation with local caching tricks. They cover:
//!
//! - Promise and generic lib references resolve correctly with lib loaded.
//! - Generic lib types (Array, Map, Set) retain type parameters via stable DefId.
//! - Import type lowering for lib types.
//! - Cross-lib interface heritage (e.g., Array extends ReadonlyArray) works.
//! - `resolve_scope_chain` and `resolve_name_to_lib_symbol` stable helpers.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{
    diagnostic_count, diagnostics_with_any_code, diagnostics_with_code,
    diagnostics_with_code_any_message, diagnostics_with_code_message, diagnostics_without_codes,
    has_any_diagnostic_code, has_diagnostic_code, has_diagnostic_code_message,
    load_compiled_lib_files, load_lib_files,
};
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    // `load_compiled_lib_files` parses only the named files; it does not follow
    // `/// <reference lib>` directives. Spell out the ES2015 aggregate closure
    // so value companions such as `Promise` and `Map` are present too.
    load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.decorators.d.ts",
        "lib.decorators.legacy.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ])
}

fn lib_files_available() -> bool {
    !load_lib_files_for_test().is_empty()
}

fn compile_with_lib(source: &str) -> Vec<(u32, String)> {
    compile_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

fn compile_with_lib_and_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// ---- Lib binder pre-population tests ----

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of lib_resolution_identity_tests tests.
#[path = "lib_resolution_identity_tests/part_00.rs"]
mod part_00;
#[path = "lib_resolution_identity_tests/part_01.rs"]
mod part_01;
#[path = "lib_resolution_identity_tests/part_02.rs"]
mod part_02;
