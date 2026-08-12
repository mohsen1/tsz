//! Union construction-mode campaign flag (#15809).
//!
//! `TSZ_UNION_LITERAL_DEFAULT` gates the move toward tsc's
//! `UnionReduction.Literal` vs `.Subtype` discipline: tsc builds most unions
//! (instantiation / mapping / type-node resolution / indexed access) with
//! `UnionReduction.Literal` (literal→primitive absorption only, no pairwise
//! subtype removal) and reserves `.Subtype` for a small set of
//! expression-derived construction sites. tsz historically pairwise
//! subtype-reduces on every evaluated union (the evaluate-layer
//! `simplify_union_members` full-relation reduce) and every interned union
//! (`reduce_union_subtypes`).
//!
//! The flag is **default-OFF and byte-parity when OFF** (campaign-flag-ledger
//! convention): with it unset the pipeline reduces exactly as historical
//! `main`. When ON it (a) drops the evaluate-layer blanket reduction (Stage 2),
//! (b) makes the interner constructor (`normalize_union`) literal-mode by
//! skipping its unconditional construction-time pairwise subtype sweep — the
//! structural root #15809 names, and the discipline the Stage 2 gate already
//! assumes when it re-interns evaluated unions without its own reduce — and
//! (c) routes the evaluate-reachable `.Subtype` construction sites through the
//! derived `subtype_reduced` query to recover pairwise removal where tsc does.

use std::sync::OnceLock;

/// Whether the union literal-default construction mode is enabled
/// (`TSZ_UNION_LITERAL_DEFAULT=1`).
///
/// Default-OFF; read once through an `OnceLock` so the interner/evaluator hot
/// paths pay a single relaxed load. Flag-off must be byte-identical to the
/// pre-campaign pipeline by construction — every read site guards a behavior
/// change so that the OFF branch is the historical path.
pub(crate) fn union_literal_default_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_UNION_LITERAL_DEFAULT").is_ok_and(|v| v == "1"))
}
