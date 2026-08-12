//! Pins the `CheckerState`/`CheckerContext` constructor matrix (#13077).
//!
//! Every public constructor routes through one private build path
//! (`CheckerContext::from_parts`) with an explicit options policy. These
//! tests pin, per constructor:
//!
//! - whether `apply_strict_defaults` expansion happens here (the legacy
//!   clobber-the-opt-outs behavior of the cache/parent paths is preserved
//!   bit-for-bit and pinned, not silently changed), and
//! - whether `no_unchecked_indexed_access` / `exact_optional_property_types`
//!   are pushed into the `QueryDatabase`, and
//! - which `DefinitionStore` the context ends up with.
//!
//! If a future change alters any cell of this matrix, a test here fails
//! loudly instead of the behavior drifting silently.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::TypeCache;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefinitionStore;

const FILE_NAME: &str = "matrix_fixture.ts";
const SOURCE: &str = "interface Box { value: number }\nclass Item {}\nlet count = 1;\n";

struct Fixture {
    parser: ParserState,
    binder: BinderState,
}

fn fixture() -> Fixture {
    let mut parser = ParserState::new(FILE_NAME.to_string(), SOURCE.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    Fixture { parser, binder }
}

/// Fixed input that distinguishes every policy in the matrix:
///
/// - `strict: true` with two explicit sub-flag opt-outs: expansion paths
///   clobber the opt-outs back to `true`; pre-resolved paths preserve them;
/// - both index flags enabled: push paths copy them into the interner
///   (which defaults to `false` for both).
fn pinned_input_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        no_implicit_any: false,
        strict_property_initialization: false,
        no_unchecked_indexed_access: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    }
}

/// Assert one row of the constructor matrix.
fn assert_matrix_row(
    constructor: &str,
    checker: &CheckerState<'_>,
    types: &TypeInterner,
    strict_expansion_applied: bool,
    index_flags_pushed: bool,
) {
    let opts = &checker.ctx.compiler_options;
    assert_eq!(
        opts.no_implicit_any,
        strict_expansion_applied,
        "{constructor}: no_implicit_any opt-out should {} be clobbered by strict expansion",
        if strict_expansion_applied { "" } else { "not" },
    );
    assert_eq!(
        opts.strict_property_initialization,
        strict_expansion_applied,
        "{constructor}: strict_property_initialization opt-out should {} be clobbered",
        if strict_expansion_applied { "" } else { "not" },
    );
    // Untouched by either policy: the input values pass through.
    assert!(opts.strict, "{constructor}: strict flag must pass through");
    assert!(
        opts.no_unchecked_indexed_access,
        "{constructor}: no_unchecked_indexed_access option must pass through"
    );
    assert!(
        opts.exact_optional_property_types,
        "{constructor}: exact_optional_property_types option must pass through"
    );
    assert_eq!(
        types.no_unchecked_indexed_access(),
        index_flags_pushed,
        "{constructor}: no_unchecked_indexed_access {} be pushed into the QueryDatabase",
        if index_flags_pushed {
            "should"
        } else {
            "should not"
        },
    );
    assert_eq!(
        types.exact_optional_property_types(),
        index_flags_pushed,
        "{constructor}: exact_optional_property_types {} be pushed into the QueryDatabase",
        if index_flags_pushed {
            "should"
        } else {
            "should not"
        },
    );
}

// =========================================================================
// Pre-resolved family: no strict expansion (driver/config layer already
// expanded with individual overrides honored); index flags pushed.
// =========================================================================

#[test]
fn matrix_new_preserves_opt_outs_and_pushes_index_flags() {
    let f = fixture();
    let types = TypeInterner::new();
    let checker = CheckerState::new(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        pinned_input_options(),
    );
    assert_matrix_row("new", &checker, &types, false, true);
    assert!(
        !checker.ctx.definition_store.is_empty(),
        "new: per-file DefinitionStore should be built from the binder's semantic defs"
    );
}

#[test]
fn matrix_new_with_shared_def_store_preserves_opt_outs_and_pushes_index_flags() {
    let f = fixture();
    let types = TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());
    let checker = CheckerState::new_with_shared_def_store(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        pinned_input_options(),
        Arc::clone(&store),
    );
    assert_matrix_row("new_with_shared_def_store", &checker, &types, false, true);
    assert!(
        Arc::ptr_eq(&checker.ctx.definition_store, &store),
        "new_with_shared_def_store: must install the provided shared store"
    );
}

#[test]
fn matrix_with_options_preserves_opt_outs_and_pushes_index_flags() {
    let f = fixture();
    let types = TypeInterner::new();
    let checker = CheckerState::with_options(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        &pinned_input_options(),
    );
    assert_matrix_row("with_options", &checker, &types, false, true);
    assert!(
        !checker.ctx.definition_store.is_empty(),
        "with_options: per-file DefinitionStore should be built from the binder's semantic defs"
    );
}

#[test]
fn matrix_with_options_deferred_def_store_leaves_store_empty() {
    let f = fixture();
    let types = TypeInterner::new();
    let checker = CheckerState::with_options_deferred_def_store(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        &pinned_input_options(),
    );
    assert_matrix_row(
        "with_options_deferred_def_store",
        &checker,
        &types,
        false,
        true,
    );
    assert!(
        checker.ctx.definition_store.is_empty(),
        "with_options_deferred_def_store: store must stay empty until \
         ProgramContext::apply_to installs the shared one"
    );
}

/// `with_parent_cache`'s sole production callers (`CheckerState::delegate_for_arena`
/// and every direct `with_parent_cache_attributed` call site) always pass
/// `parent.ctx.compiler_options.clone()` — already fully resolved by the
/// driver/config layer — never a raw `strict` umbrella. Re-expanding here
/// (the pre-#17110 `EXPAND_STRICT_LOCALLY` policy) silently clobbered an
/// explicit per-file sub-flag opt-out (e.g. `strictNullChecks: false` with
/// no bare `--strict`) back to the umbrella's value, and because `types` is
/// one `QueryDatabase` shared with the parent and every other file in the
/// compilation, the clobber leaked past the one child. `with_parent_cache`
/// moved to `PRE_RESOLVED` for this reason; `with_cache_pre_resolved` is its
/// non-delegation sibling for the same pre-resolved-options family. See
/// `CheckerContext::with_parent_cache`'s doc comment.
#[test]
fn matrix_with_parent_cache_preserves_opt_outs_and_pushes_index_flags() {
    let f = fixture();
    let parent_types = TypeInterner::new();
    let child_types = TypeInterner::new();
    let parent = CheckerState::new(
        f.parser.get_arena(),
        &f.binder,
        &parent_types,
        FILE_NAME.to_string(),
        pinned_input_options(),
    );
    let child = CheckerState::with_parent_cache(
        f.parser.get_arena(),
        &f.binder,
        &child_types,
        FILE_NAME.to_string(),
        pinned_input_options(),
        &parent,
    );
    // A dedicated child interner proves the parent path DOES push index
    // flags into the QueryDatabase under PRE_RESOLVED (redundant with the
    // parent's own push in production, where child and parent share one
    // QueryDatabase instance, but still the policy's actual behavior).
    assert_matrix_row("with_parent_cache", &child, &child_types, false, true);
    assert!(
        Arc::ptr_eq(&child.ctx.definition_store, &parent.ctx.definition_store),
        "with_parent_cache: child must share the parent's DefinitionStore"
    );
}

#[test]
fn matrix_with_parent_cache_attributed_matches_with_parent_cache() {
    let f = fixture();
    let parent_types = TypeInterner::new();
    let child_types = TypeInterner::new();
    let parent = CheckerState::new(
        f.parser.get_arena(),
        &f.binder,
        &parent_types,
        FILE_NAME.to_string(),
        pinned_input_options(),
    );
    let child = CheckerState::with_parent_cache_attributed(
        f.parser.get_arena(),
        &f.binder,
        &child_types,
        FILE_NAME.to_string(),
        pinned_input_options(),
        &parent,
        tsz_common::perf_counters::CheckerCreationReason::Other,
    );
    assert_matrix_row(
        "with_parent_cache_attributed",
        &child,
        &child_types,
        false,
        true,
    );
    assert!(
        Arc::ptr_eq(&child.ctx.definition_store, &parent.ctx.definition_store),
        "with_parent_cache_attributed: child must share the parent's DefinitionStore"
    );
}

// =========================================================================
// Local-expansion family: strict expansion applied here (preserved legacy
// behavior — clobbers explicit sub-flag opt-outs); index flags NOT pushed.
// =========================================================================

#[test]
fn matrix_with_cache_expands_strict_and_skips_index_flag_push() {
    let f = fixture();
    let types = TypeInterner::new();
    let checker = CheckerState::with_cache(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        TypeCache::default(),
        pinned_input_options(),
    );
    assert_matrix_row("with_cache", &checker, &types, true, false);
}

#[test]
fn matrix_with_cache_and_options_expands_strict_and_skips_index_flag_push() {
    let f = fixture();
    let types = TypeInterner::new();
    let checker = CheckerState::with_cache_and_options(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        TypeCache::default(),
        &pinned_input_options(),
    );
    assert_matrix_row("with_cache_and_options", &checker, &types, true, false);
}

#[test]
fn matrix_with_cache_and_shared_def_store_expands_strict_and_skips_index_flag_push() {
    let f = fixture();
    let types = TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());
    let checker = CheckerState::with_cache_and_shared_def_store(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        TypeCache::default(),
        pinned_input_options(),
        Arc::clone(&store),
    );
    assert_matrix_row(
        "with_cache_and_shared_def_store",
        &checker,
        &types,
        true,
        false,
    );
    assert!(
        Arc::ptr_eq(&checker.ctx.definition_store, &store),
        "with_cache_and_shared_def_store: must install the provided shared store"
    );
}

// =========================================================================
// Expand-and-push: the one path that historically applied strict expansion
// at the CheckerState layer AND pushed index flags (parallel checking).
// =========================================================================

#[test]
fn matrix_with_options_and_shared_def_store_expands_strict_and_pushes_index_flags() {
    let f = fixture();
    let types = TypeInterner::new();
    let store = Arc::new(DefinitionStore::new());
    let checker = CheckerState::with_options_and_shared_def_store(
        f.parser.get_arena(),
        &f.binder,
        &types,
        FILE_NAME.to_string(),
        &pinned_input_options(),
        Arc::clone(&store),
    );
    assert_matrix_row(
        "with_options_and_shared_def_store",
        &checker,
        &types,
        true,
        true,
    );
    assert!(
        Arc::ptr_eq(&checker.ctx.definition_store, &store),
        "with_options_and_shared_def_store: must install the provided shared store"
    );
}
