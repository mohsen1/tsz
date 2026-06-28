//! Cross-arena declaration-identity resolution for generic-call inference
//! (issue #14344).
//!
//! # The bug
//!
//! The generic-call inference resolver is a [`crate::caches::QueryCache`]
//! (`InferenceContext::with_query_db`). Historically the `QueryCache` was built
//! with ONLY a `&TypeInterner` and NO `DefinitionStore`, so every `DefId`-keyed
//! [`crate::relations::subtype::TypeResolver`] method on it
//! (`def_to_symbol_id`, `get_def_kind`, `get_def_name`, `canonical_def_id`,
//! `defs_are_equivalent`) silently returned the trait default. Under
//! whole-program multi-arena load the SAME interface (e.g. fp-ts
//! `Magma`/`Eq`/`Semigroup`/`Kind`) is lowered into DISTINCT per-arena `DefId`s
//! with DISTINCT binder-local `SymbolId`s. When inference compares two
//! `Application` bases that are this same interface from different arenas,
//! `shared_application_base_def_id` calls `defs_are_equivalent`; without the
//! shared store, the resolver cannot observe either `(file, declaration-node)`
//! identity or store-owned `SymbolId`s, so the bases are treated as unrelated,
//! per-argument inference is SKIPPED, and the callee's type parameter resolves
//! to `unknown`
//! (`infer_resolve.rs`), surfacing as a false `TS2322`/`TS2345`
//! (`HKT<F, unknown>` vs `HKT<F, readonly [K, A]>`).
//!
//! # The fix
//!
//! Thread the shared `DefinitionStore` into the inference `QueryCache` (see the
//! CLI driver and `tsz-core` checking paths) so the `DefId`-keyed resolver
//! methods resolve, re-enabling the intended `defs_are_equivalent` cross-arena
//! base unification. The equivalence is deliberately declaration-site based;
//! it does not rewrite `DefId`s or chase broad alias canonicalization. Those
//! `QueryCache` resolver methods are gated behind
//! [`xarena_base_decl_enabled`] (`TSZ_XARENA_BASE_DECL`, default-OFF) so flag-OFF
//! stays byte-parity with the historical store-less behavior until the change is
//! proven on full conformance.

use std::sync::OnceLock;

/// `TSZ_XARENA_BASE_DECL=1` activates store-backed `DefId` resolution on the
/// inference `QueryCache` (cross-arena declaration identity). Default-OFF, so
/// flag-OFF is byte-parity with the historical store-less inference resolver.
pub(crate) fn xarena_base_decl_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_XARENA_BASE_DECL").is_ok_and(|v| v == "1"))
}

/// `TSZ_XARENA_BASE_DECL_DUMP=1` makes the CLI driver print a one-line marker
/// after compile, confirming which flag state the run used.
pub fn xarena_dump_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_XARENA_BASE_DECL_DUMP").is_ok_and(|v| v == "1"))
}

/// One-line summary for the harness/CLI: the active flag state.
pub fn xarena_base_decl_dump_line() -> String {
    format!("XARENA_BASE_DECL enabled={}", xarena_base_decl_enabled())
}
