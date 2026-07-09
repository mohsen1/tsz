//! Discarded-diagnostics contexts must skip spelling-suggestion candidate
//! scans (issue #13250, Fast workstream B).
//!
//! When `CheckerContext::diagnostics_discarded` is set (transient cross-arena
//! delegation children, the driver's skipLibCheck declaration-preparation
//! pass), no diagnostic from the context ever surfaces, so the
//! full-symbol-universe Levenshtein scan behind "did you mean" suggestions is
//! pure presentation work. The scan gate must:
//!
//! - return no candidates for ANY node while the flag is set (previously only
//!   nodes inside built-in `lib.*.d.ts` files were gated), and
//! - keep producing candidates in retained-diagnostics contexts, where the
//!   suggestion decides the surfaced diagnostic (TS2552/TS2551 vs plain
//!   TS2304/TS2339).
//!
//! The retained-context lib behavior (e.g. `Arrray` -> `Array` TS2552) is
//! covered by `tests/lib_type_spelling_suggestions_tests.rs`.

use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::ParserState;

/// Run `f` against a checked source file, with `diagnostics_discarded` set
/// before the check when requested. `f` receives the checker and the
/// source-file root node, which sits in the top-level scope and therefore
/// sees every top-level binding as a suggestion candidate.
fn with_checked_state<R>(
    source: &str,
    discarded: bool,
    f: impl FnOnce(&CheckerState<'_>, tsz_parser::NodeIndex) -> R,
) -> R {
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "main.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.diagnostics_discarded = discarded;
    checker.check_source_file(source_file);
    f(&checker, source_file)
}

const TYPO_SOURCE: &str = "const whole = 1;\nlet x = wole;\n";

#[test]
fn retained_context_scans_candidates_for_user_node() {
    with_checked_state(TYPO_SOURCE, false, |checker, root| {
        let suggestions = checker.scan_similar_identifiers_for_meaning(
            "wole",
            root,
            tsz_binder::symbol_flags::VALUE,
        );
        assert_eq!(
            suggestions,
            vec!["whole".to_string()],
            "retained-diagnostics contexts must keep producing suggestions"
        );
    });
}

#[test]
fn discarded_context_skips_scan_for_user_node() {
    with_checked_state(TYPO_SOURCE, true, |checker, root| {
        let suggestions = checker.scan_similar_identifiers_for_meaning(
            "wole",
            root,
            tsz_binder::symbol_flags::VALUE,
        );
        assert!(
            suggestions.is_empty(),
            "discarded-diagnostics contexts must not run the candidate scan \
             even for nodes outside built-in lib files, got: {suggestions:?}"
        );
    });
}
