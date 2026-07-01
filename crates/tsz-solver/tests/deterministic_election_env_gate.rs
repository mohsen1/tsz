//! Behavioral witness for the `#15317` deterministic-election derivation:
//! setting a `#14344` campaign channel that is *not* one of the four
//! shared-store publication flags must still switch `DefinitionStore`
//! construction onto the deterministic home-decl election.
//!
//! The derivation latches a process-global `OnceLock` on first read, so the
//! env var must be set before the process's first store construction. The
//! test therefore re-executes itself: the parent spawns the same test binary
//! with `TSZ_TYPEPARAM_DECL_IDENTITY=1` in the child's environment (no
//! `unsafe` `set_var`, and the latch order is guaranteed in the fresh
//! process); the child performs the actual store-construction assertion.

use tsz_solver::construction::TypeInterner;
use tsz_solver::def::{DefId, DefinitionStore};

const CHILD_MARKER: &str = "TSZ_ELECTION_ENV_GATE_CHILD";

fn entry(name: &str, file_id: u32, span_start: u32) -> tsz_binder::SemanticDefEntry {
    tsz_binder::SemanticDefEntry {
        kind: tsz_binder::SemanticDefKind::Interface,
        name: name.to_string(),
        file_id,
        span_start,
        type_param_count: 0,
        type_param_names: Vec::new(),
        is_exported: false,
        enum_member_names: Vec::new(),
        is_const: false,
        is_abstract: false,
        extends_names: Vec::new(),
        implements_names: Vec::new(),
        parent_namespace: None,
        is_global_augmentation: false,
        is_declare: false,
    }
}

fn assert_provenance_ordered_election() {
    let interner = TypeInterner::new();
    let mut defs = rustc_hash::FxHashMap::default();
    defs.insert(tsz_binder::SymbolId(300), entry("Late", 7, 40));
    defs.insert(tsz_binder::SymbolId(100), entry("Early", 3, 10));
    defs.insert(tsz_binder::SymbolId(200), entry("Middle", 3, 20));

    let store = DefinitionStore::from_semantic_defs(&defs, |s| interner.intern_string(s));

    assert_eq!(
        [
            store.find_def_by_symbol(100),
            store.find_def_by_symbol(200),
            store.find_def_by_symbol(300),
        ],
        [Some(DefId(1)), Some(DefId(2)), Some(DefId(3))],
        "a campaign channel outside the four publication flags must still \
         elect DefIds by stable (file, span, symbol) provenance (#15317)"
    );
}

/// `TSZ_TYPEPARAM_DECL_IDENTITY` (the decl-identity keystone, gated outside
/// `def::core`) must enable the deterministic store election: `DefId`s follow
/// `(file_id, span_start, symbol_id)` provenance, not `FxHashMap` iteration
/// order.
#[test]
fn typeparam_decl_identity_channel_enables_deterministic_election() {
    if std::env::var(CHILD_MARKER).is_ok() {
        assert_provenance_ordered_election();
        return;
    }

    let exe = std::env::current_exe().expect("test binary path");
    let output = std::process::Command::new(exe)
        .args([
            "--exact",
            "typeparam_decl_identity_channel_enables_deterministic_election",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        .env("TSZ_TYPEPARAM_DECL_IDENTITY", "1")
        .output()
        .expect("spawn child test process");
    assert!(
        output.status.success(),
        "child assertion failed under TSZ_TYPEPARAM_DECL_IDENTITY=1:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
