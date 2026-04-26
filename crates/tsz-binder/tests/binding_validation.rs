//! Unit tests for `crates/tsz-binder/src/binding/validation.rs`.
//!
//! Mounted via `#[cfg(test)] #[path = "../../tests/binding_validation.rs"] mod tests;`
//! at the bottom of `binding/validation.rs`. Uses `super::*` to inherit the
//! enclosing crate scope.
//!
//! Tests cover:
//! - `validate_symbol_table` (clean state, `BrokenSymbolLink`, `OrphanedSymbol`,
//!   `InvalidValueDeclaration`)
//! - `is_symbol_table_valid` (true/false complement)
//! - `validate_global_symbols` (empty, fully merged, partial)
//! - `get_lib_symbol_report` (counts and lib-binder lines)
//! - `verify_lib_symbol_merge` (accessible, inaccessible, empty edge case)
//! - `get_resolution_stats` + `get_resolution_summary`

use std::sync::Arc;

use crate::lib_loader::LibFile;
use crate::state::{BinderState, ResolutionStats, ValidationError};
use crate::symbols::{Symbol, SymbolArena, SymbolId, SymbolTable, symbol_flags};
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;

// =============================================================================
// Helpers
// =============================================================================

/// Build a `BinderState` with explicit `node_symbols` so we can inject broken
/// links without going through a full parse-and-bind pipeline.
fn make_state(
    arena: SymbolArena,
    file_locals: SymbolTable,
    node_symbols: FxHashMap<u32, SymbolId>,
) -> BinderState {
    BinderState::from_bound_state(arena, file_locals, Arc::new(node_symbols))
}

/// Build a minimal symbol with one declaration so it is not orphaned.
fn alloc_with_declaration(arena: &mut SymbolArena, flags: u32, name: &str) -> SymbolId {
    let id = arena.alloc(flags, name.to_string());
    if let Some(sym) = arena.get_mut(id) {
        sym.add_declaration(NodeIndex(0), None);
    }
    id
}

// =============================================================================
// validate_symbol_table
// =============================================================================

mod validate_symbol_table {
    use super::*;

    #[test]
    fn empty_state_has_no_errors() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        assert!(state.validate_symbol_table().is_empty());
    }

    #[test]
    fn well_formed_state_has_no_errors() {
        let mut arena = SymbolArena::new();
        let id = alloc_with_declaration(&mut arena, symbol_flags::VALUE, "x");
        let mut node_symbols = FxHashMap::default();
        node_symbols.insert(0u32, id);
        let mut file_locals = SymbolTable::new();
        file_locals.set("x".to_string(), id);
        let state = make_state(arena, file_locals, node_symbols);
        assert!(state.validate_symbol_table().is_empty());
    }

    #[test]
    fn broken_symbol_link_is_reported() {
        // node_symbols points to a SymbolId that doesn't exist in the arena.
        let arena = SymbolArena::new();
        let mut node_symbols = FxHashMap::default();
        node_symbols.insert(7u32, SymbolId(999));
        let state = make_state(arena, SymbolTable::new(), node_symbols);
        let errors = state.validate_symbol_table();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ValidationError::BrokenSymbolLink {
                node_index,
                symbol_id,
            } => {
                assert_eq!(*node_index, 7);
                assert_eq!(*symbol_id, 999);
            }
            other => panic!("expected BrokenSymbolLink, got {other:?}"),
        }
    }

    #[test]
    fn orphaned_symbol_is_reported() {
        // alloc creates a symbol with empty `declarations`.
        let mut arena = SymbolArena::new();
        let _ = arena.alloc(symbol_flags::VALUE, "orphan".to_string());
        let state = make_state(arena, SymbolTable::new(), FxHashMap::default());
        let errors = state.validate_symbol_table();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ValidationError::OrphanedSymbol { name, .. } => assert_eq!(name, "orphan"),
            other => panic!("expected OrphanedSymbol, got {other:?}"),
        }
    }

    #[test]
    fn invalid_value_declaration_is_reported() {
        let mut arena = SymbolArena::new();
        let id = arena.alloc(symbol_flags::VALUE, "v".to_string());
        if let Some(sym) = arena.get_mut(id) {
            sym.add_declaration(NodeIndex(0), None);
            // value_declaration points to NodeIndex(42) which is NOT in node_symbols.
            sym.set_value_declaration(NodeIndex(42), None);
        }
        // node_symbols only has NodeIndex(0).
        let mut node_symbols = FxHashMap::default();
        node_symbols.insert(0u32, id);
        let state = make_state(arena, SymbolTable::new(), node_symbols);
        let errors = state.validate_symbol_table();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ValidationError::InvalidValueDeclaration { name, .. } => assert_eq!(name, "v"),
            other => panic!("expected InvalidValueDeclaration, got {other:?}"),
        }
    }

    #[test]
    fn value_declaration_none_does_not_error() {
        // Symbol with declarations but value_declaration left as NONE.
        let mut arena = SymbolArena::new();
        let _ = alloc_with_declaration(&mut arena, symbol_flags::VALUE, "ok");
        let state = make_state(arena, SymbolTable::new(), FxHashMap::default());
        // `value_declaration.is_some()` is false for NONE, so the
        // InvalidValueDeclaration branch should not fire.
        assert!(state.validate_symbol_table().is_empty());
    }

    #[test]
    fn multiple_errors_are_all_collected() {
        let mut arena = SymbolArena::new();
        // 1) Orphan symbol (no declarations).
        let _ = arena.alloc(symbol_flags::VALUE, "orphan_a".to_string());
        let _ = arena.alloc(symbol_flags::VALUE, "orphan_b".to_string());
        // 2) Broken link in node_symbols.
        let mut node_symbols = FxHashMap::default();
        node_symbols.insert(5u32, SymbolId(9999));
        node_symbols.insert(6u32, SymbolId(9998));

        let state = make_state(arena, SymbolTable::new(), node_symbols);
        let errors = state.validate_symbol_table();
        let orphan_count = errors
            .iter()
            .filter(|e| matches!(e, ValidationError::OrphanedSymbol { .. }))
            .count();
        let broken_count = errors
            .iter()
            .filter(|e| matches!(e, ValidationError::BrokenSymbolLink { .. }))
            .count();
        assert_eq!(orphan_count, 2);
        assert_eq!(broken_count, 2);
    }
}

// =============================================================================
// is_symbol_table_valid
// =============================================================================

mod is_symbol_table_valid {
    use super::*;

    #[test]
    fn empty_state_is_valid() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        assert!(state.is_symbol_table_valid());
    }

    #[test]
    fn broken_state_is_invalid() {
        let arena = SymbolArena::new();
        let mut node_symbols = FxHashMap::default();
        node_symbols.insert(0u32, SymbolId(0));
        let state = make_state(arena, SymbolTable::new(), node_symbols);
        assert!(!state.is_symbol_table_valid());
    }
}

// =============================================================================
// validate_global_symbols
// =============================================================================

mod validate_global_symbols {
    use super::*;

    /// Helper: build a lib `BinderState` with a given set of names registered
    /// in `file_locals`. Each name gets a fresh symbol id.
    fn lib_with_globals(names: &[&str]) -> Arc<BinderState> {
        let mut arena = SymbolArena::new();
        let mut file_locals = SymbolTable::new();
        for name in names {
            let id = arena.alloc(symbol_flags::VALUE, (*name).to_string());
            file_locals.set((*name).to_string(), id);
        }
        Arc::new(BinderState::from_bound_state(
            arena,
            file_locals,
            Arc::new(FxHashMap::default()),
        ))
    }

    #[test]
    fn empty_binder_reports_all_expected_missing() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let missing = state.validate_global_symbols();
        // Every entry in EXPECTED_GLOBAL_SYMBOLS should be missing. Spot-check
        // a few well-known names that the helper expects.
        assert!(missing.contains(&"Object".to_string()));
        assert!(missing.contains(&"Promise".to_string()));
        assert!(missing.contains(&"console".to_string()));
        // Sanity: at least 25 expected names.
        assert!(missing.len() >= 25);
    }

    #[test]
    fn file_locals_satisfies_expectation() {
        // A symbol present in `file_locals` should not appear as missing.
        let mut arena = SymbolArena::new();
        let id = arena.alloc(symbol_flags::VALUE, "Object".to_string());
        let mut file_locals = SymbolTable::new();
        file_locals.set("Object".to_string(), id);
        let state = make_state(arena, file_locals, FxHashMap::default());
        let missing = state.validate_global_symbols();
        assert!(!missing.contains(&"Object".to_string()));
    }

    #[test]
    fn lib_binders_satisfy_expectation() {
        // A symbol present in a lib_binder's file_locals should not appear as
        // missing.
        let lib = lib_with_globals(&["Object", "Promise", "console"]);
        let mut state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        state.lib_binders = Arc::new(vec![lib]);
        let missing = state.validate_global_symbols();
        assert!(!missing.contains(&"Object".to_string()));
        assert!(!missing.contains(&"Promise".to_string()));
        assert!(!missing.contains(&"console".to_string()));
        // Other expected names should still be missing.
        assert!(missing.contains(&"Map".to_string()));
    }

    #[test]
    fn symbol_present_in_either_source_satisfies() {
        // file_locals has Object, lib_binders has Promise: neither should be
        // reported missing.
        let lib = lib_with_globals(&["Promise"]);
        let mut arena = SymbolArena::new();
        let id = arena.alloc(symbol_flags::VALUE, "Object".to_string());
        let mut file_locals = SymbolTable::new();
        file_locals.set("Object".to_string(), id);
        let mut state = make_state(arena, file_locals, FxHashMap::default());
        state.lib_binders = Arc::new(vec![lib]);
        let missing = state.validate_global_symbols();
        assert!(!missing.contains(&"Object".to_string()));
        assert!(!missing.contains(&"Promise".to_string()));
    }
}

// =============================================================================
// get_lib_symbol_report
// =============================================================================

mod get_lib_symbol_report {
    use super::*;

    #[test]
    fn empty_state_reports_zero_counts() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let report = state.get_lib_symbol_report();
        assert!(report.starts_with("=== Lib Symbol Availability Report ===\n"));
        assert!(report.contains("File locals: 0 symbols"));
        assert!(report.contains("Lib binders: 0 symbols (0 binders)"));
        assert!(report.contains("Expected symbols present: 0/"));
    }

    #[test]
    fn missing_symbols_are_listed() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let report = state.get_lib_symbol_report();
        assert!(report.contains("Missing symbols:"));
        // Spot-check one expected missing entry.
        assert!(report.contains("- Object"));
    }

    #[test]
    fn lib_binder_contributions_section_appears_when_binders_present() {
        // Build a tiny lib_binder so the contributions section is emitted.
        let mut lib_arena = SymbolArena::new();
        let id = lib_arena.alloc(symbol_flags::VALUE, "Object".to_string());
        let mut lib_locals = SymbolTable::new();
        lib_locals.set("Object".to_string(), id);
        let lib = Arc::new(BinderState::from_bound_state(
            lib_arena,
            lib_locals,
            Arc::new(FxHashMap::default()),
        ));

        let mut state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        state.lib_binders = Arc::new(vec![lib]);
        let report = state.get_lib_symbol_report();
        assert!(report.contains("Lib binder contributions:"));
        assert!(report.contains("Lib binder 0: 1 symbols"));
        assert!(report.contains("Lib binders: 1 symbols (1 binders)"));
        // Object now satisfied -> not in the missing list.
        assert!(!report.contains("- Object\n"));
    }

    #[test]
    fn no_missing_section_when_no_missing() {
        // Construct a binder whose file_locals + lib_binders cover every
        // EXPECTED_GLOBAL_SYMBOL. We use the all-in-one lib_binder approach.
        // We don't know the exact list at compile time, so query and seed it.
        let dummy = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let expected_missing = dummy.validate_global_symbols();

        let mut lib_arena = SymbolArena::new();
        let mut lib_locals = SymbolTable::new();
        for name in &expected_missing {
            let id = lib_arena.alloc(symbol_flags::VALUE, name.clone());
            lib_locals.set(name.clone(), id);
        }
        let lib = Arc::new(BinderState::from_bound_state(
            lib_arena,
            lib_locals,
            Arc::new(FxHashMap::default()),
        ));

        let mut state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        state.lib_binders = Arc::new(vec![lib]);
        let report = state.get_lib_symbol_report();
        assert!(!report.contains("Missing symbols:"));
    }
}

// =============================================================================
// log_missing_lib_symbols
// =============================================================================

mod log_missing_lib_symbols {
    use super::*;

    #[test]
    fn returns_true_when_symbols_missing() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        assert!(state.log_missing_lib_symbols());
    }

    #[test]
    fn returns_false_when_all_present() {
        let dummy = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let expected_missing = dummy.validate_global_symbols();

        let mut arena = SymbolArena::new();
        let mut file_locals = SymbolTable::new();
        for name in &expected_missing {
            let id = arena.alloc(symbol_flags::VALUE, name.clone());
            file_locals.set(name.clone(), id);
        }
        let state = make_state(arena, file_locals, FxHashMap::default());
        assert!(!state.log_missing_lib_symbols());
    }
}

// =============================================================================
// verify_lib_symbol_merge
// =============================================================================

mod verify_lib_symbol_merge {
    use super::*;

    use tsz_parser::parser::node::NodeArena;

    /// Build a `LibFile` whose binder `file_locals` contain the given names.
    fn make_lib_file(file_name: &str, names: &[&str]) -> Arc<LibFile> {
        let mut arena = SymbolArena::new();
        let mut file_locals = SymbolTable::new();
        for name in names {
            let id = arena.alloc(symbol_flags::VALUE, (*name).to_string());
            file_locals.set((*name).to_string(), id);
        }
        let binder =
            BinderState::from_bound_state(arena, file_locals, Arc::new(FxHashMap::default()));
        Arc::new(LibFile::new(
            file_name.to_string(),
            Arc::new(NodeArena::new()),
            Arc::new(binder),
            NodeIndex(0),
        ))
    }

    #[test]
    fn empty_lib_files_returns_empty() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let inaccessible = state.verify_lib_symbol_merge(&[]);
        assert!(inaccessible.is_empty());
    }

    #[test]
    fn empty_binder_lib_is_skipped() {
        // Lib with no symbols never reports inaccessible because the function
        // short-circuits on `lib_file.binder.file_locals.is_empty()`.
        let lib = make_lib_file("empty.d.ts", &[]);
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let inaccessible = state.verify_lib_symbol_merge(&[lib]);
        assert!(inaccessible.is_empty());
    }

    #[test]
    fn accessible_via_file_locals() {
        let lib = make_lib_file("a.d.ts", &["Foo", "Bar"]);
        // State's file_locals contains "Foo".
        let mut user_arena = SymbolArena::new();
        let id = user_arena.alloc(symbol_flags::VALUE, "Foo".to_string());
        let mut user_locals = SymbolTable::new();
        user_locals.set("Foo".to_string(), id);
        let state = make_state(user_arena, user_locals, FxHashMap::default());
        let inaccessible = state.verify_lib_symbol_merge(&[lib]);
        assert!(inaccessible.is_empty());
    }

    #[test]
    fn accessible_via_lib_binder() {
        let lib = make_lib_file("a.d.ts", &["Foo"]);
        // Mirror the same name in another lib_binder.
        let mut other_arena = SymbolArena::new();
        let id = other_arena.alloc(symbol_flags::VALUE, "Foo".to_string());
        let mut other_locals = SymbolTable::new();
        other_locals.set("Foo".to_string(), id);
        let other_binder = Arc::new(BinderState::from_bound_state(
            other_arena,
            other_locals,
            Arc::new(FxHashMap::default()),
        ));

        let mut state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        state.lib_binders = Arc::new(vec![other_binder]);
        let inaccessible = state.verify_lib_symbol_merge(&[lib]);
        assert!(inaccessible.is_empty());
    }

    #[test]
    fn inaccessible_lib_is_reported() {
        let lib = make_lib_file("missing.d.ts", &["Quux"]);
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let inaccessible = state.verify_lib_symbol_merge(&[lib]);
        assert_eq!(inaccessible, vec!["missing.d.ts".to_string()]);
    }

    #[test]
    fn mixed_libs_only_inaccessible_reported() {
        let lib_ok = make_lib_file("ok.d.ts", &["Hit"]);
        let lib_miss = make_lib_file("miss.d.ts", &["NotThere"]);
        let mut user_arena = SymbolArena::new();
        let id = user_arena.alloc(symbol_flags::VALUE, "Hit".to_string());
        let mut user_locals = SymbolTable::new();
        user_locals.set("Hit".to_string(), id);
        let state = make_state(user_arena, user_locals, FxHashMap::default());
        let inaccessible = state.verify_lib_symbol_merge(&[lib_ok, lib_miss]);
        assert_eq!(inaccessible, vec!["miss.d.ts".to_string()]);
    }
}

// =============================================================================
// get_resolution_stats / get_resolution_summary
// =============================================================================

mod resolution_stats {
    use super::*;

    #[test]
    fn empty_state_reports_zero_counts() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let stats: ResolutionStats = state.get_resolution_stats();
        assert_eq!(stats.attempts, 0);
        assert_eq!(stats.scope_hits, 0);
        assert_eq!(stats.file_local_hits, 0);
        assert_eq!(stats.lib_binder_hits, 0);
        assert_eq!(stats.failures, 0);
    }

    #[test]
    fn file_local_count_matches_table_len() {
        let mut arena = SymbolArena::new();
        let a = arena.alloc(symbol_flags::VALUE, "a".to_string());
        let b = arena.alloc(symbol_flags::VALUE, "b".to_string());
        let mut file_locals = SymbolTable::new();
        file_locals.set("a".to_string(), a);
        file_locals.set("b".to_string(), b);
        let state = make_state(arena, file_locals, FxHashMap::default());
        let stats = state.get_resolution_stats();
        assert_eq!(stats.file_local_hits, 2);
    }

    #[test]
    fn lib_binder_count_sums_across_binders() {
        // Two lib binders with 2 + 3 symbols.
        let mk = |names: &[&str]| -> Arc<BinderState> {
            let mut arena = SymbolArena::new();
            let mut locals = SymbolTable::new();
            for name in names {
                let id = arena.alloc(symbol_flags::VALUE, (*name).to_string());
                locals.set((*name).to_string(), id);
            }
            Arc::new(BinderState::from_bound_state(
                arena,
                locals,
                Arc::new(FxHashMap::default()),
            ))
        };
        let lib1 = mk(&["x", "y"]);
        let lib2 = mk(&["a", "b", "c"]);

        let mut state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        state.lib_binders = Arc::new(vec![lib1, lib2]);
        let stats = state.get_resolution_stats();
        assert_eq!(stats.lib_binder_hits, 5);
    }

    #[test]
    fn summary_string_includes_known_lines() {
        let state = make_state(SymbolArena::new(), SymbolTable::new(), FxHashMap::default());
        let summary = state.get_resolution_summary();
        assert!(summary.starts_with("Symbol Resolution Summary:"));
        assert!(summary.contains("- Scope symbols: 0"));
        assert!(summary.contains("- File local symbols: 0"));
        assert!(summary.contains("- Lib binder symbols: 0 (from 0 binders)"));
        assert!(summary.contains("- Total accessible symbols: 0"));
    }

    #[test]
    fn summary_reflects_file_local_count() {
        let mut arena = SymbolArena::new();
        let id = arena.alloc(symbol_flags::VALUE, "thing".to_string());
        let mut file_locals = SymbolTable::new();
        file_locals.set("thing".to_string(), id);
        let state = make_state(arena, file_locals, FxHashMap::default());
        let summary = state.get_resolution_summary();
        assert!(summary.contains("- File local symbols: 1"));
        assert!(summary.contains("- Total accessible symbols: 1"));
    }
}

// =============================================================================
// Symbol shape sanity (lockstep precondition for the validators above)
// =============================================================================

#[test]
fn alloc_with_declaration_yields_non_orphan_symbol() {
    // Sanity check on the test helper: a symbol with one added declaration
    // should NOT be reported as orphaned by the validator.
    let mut arena = SymbolArena::new();
    let _ = alloc_with_declaration(&mut arena, symbol_flags::VALUE, "z");
    let state = make_state(arena, SymbolTable::new(), FxHashMap::default());
    assert!(state.validate_symbol_table().is_empty());
}

#[test]
fn fresh_symbol_value_declaration_is_none() {
    // Locks the precondition that `Symbol::new` leaves `value_declaration`
    // as NodeIndex::NONE so the InvalidValueDeclaration branch doesn't fire
    // for never-set value declarations.
    let s = Symbol::new(SymbolId(0), symbol_flags::VALUE, "f".to_string());
    assert!(!s.value_declaration.is_some());
    assert!(s.value_declaration.is_none());
}
