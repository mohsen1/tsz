//! Canonical registry of the `#14344`/`#14345` identity / materialize-once
//! campaign env channels.
//!
//! The campaign accumulated a family of default-OFF env flags (issue #15317).
//! Several of them make the shared-store `DefId` allocation / canonical election
//! observable, which is the surface the historical run-to-run election flap
//! manifests under. Rather than have
//! [`deterministic_store_election_enabled`](super::semantic_construction::deterministic_store_election_enabled)
//! re-derive its own hand-picked subset (the pre-#15317 code keyed off only 4 of
//! the publication channels, leaving a determinism hole for compositions such as
//! `DECLID + XARENA_BASE + HERITAGE`), the whole substrate set is enumerated here
//! once and consumed from a single place.
//!
//! Each entry is the *env variable name* of a campaign substrate channel. The
//! flags are activated in real gauges only through these process-global env vars
//! (their per-flag gates are `OnceLock`s over the same names), so reading the env
//! here is exactly the signal that decides whether a composed gauge is running
//! any identity-affecting channel. Enabling deterministic election is a no-op
//! superset — it only pins `DefId` allocation to the sound home-decl order — so
//! listing a channel that turns out not to perturb election is safe; omitting one
//! that does is the bug this module exists to prevent.
//!
//! Measurement-only channels (`TSZ_DEF_PUBLICATION_CENSUS`,
//! `TSZ_TYPEPARAM_DIVERGENCE_PROBE`, `TSZ_XARENA_BASE_DECL_DUMP`) and the
//! counter-only depth knob (`TSZ_REREDUCE_DEPTH_CAP`) are deliberately excluded:
//! they emit diagnostics or tune a budget without changing which type is elected.
//!
//! The graduation ledger (`docs/plan/campaign-flag-ledger.md`) is the
//! human-readable companion; this array is the machine-checked one. Keep them in
//! sync — the ledger's channel table and this list must name the same flags.

/// Env variable names of the campaign substrate channels that can make the
/// shared-store `DefId` election observable. Single source of truth; keep sorted
/// by landing order for readability but treat membership as the contract.
pub(crate) const CAMPAIGN_STORE_CHANNELS: &[&str] = &[
    "TSZ_INST_RESOLVER_REREDUCE",
    "TSZ_OPTIONB_STORE_RESOLVER",
    "TSZ_INFER_HKT_REDUCE",
    "TSZ_TYPEPARAM_DECL_IDENTITY",
    "TSZ_XARENA_BASE_DECL",
    "TSZ_XARENA_HERITAGE_TYPEARG",
    "TSZ_TYPEOF_URI_SELFLOOP",
    "TSZ_AUGMENTED_BODY_SYMBOL_REDIRECT",
    "TSZ_MODULE_AUG_SYMBOL_EDGE",
    "TSZ_MODULE_AUG_BODY_PUBLISH",
    "TSZ_ALPHA_NAME_PAIR",
    "TSZ_LAZY_REF_RELATION",
];

/// Whether any campaign substrate channel is active via its env var.
///
/// Returns `false` when the process sets none of [`CAMPAIGN_STORE_CHANNELS`] to
/// `1`, so the all-flags-OFF default pipeline is unaffected (byte-identical to
/// `main`). Reading the env directly — rather than calling each per-flag
/// accessor — keeps this crate-boundary agnostic: two of the flap-driving
/// channels (`TSZ_TYPEPARAM_DECL_IDENTITY`, `TSZ_XARENA_HERITAGE_TYPEARG`) are
/// read in `tsz-checker`, which the solver cannot call into, yet their env vars
/// are visible here.
pub(crate) fn any_campaign_store_channel_enabled() -> bool {
    CAMPAIGN_STORE_CHANNELS
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|v| v == "1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_list_is_deduplicated() {
        let mut seen = std::collections::HashSet::new();
        for name in CAMPAIGN_STORE_CHANNELS {
            assert!(seen.insert(*name), "duplicate campaign channel: {name}");
        }
    }

    #[test]
    fn channel_names_are_tsz_prefixed() {
        for name in CAMPAIGN_STORE_CHANNELS {
            assert!(
                name.starts_with("TSZ_"),
                "campaign channel env var must be TSZ_-prefixed: {name}"
            );
        }
    }

    #[test]
    fn channel_count_is_pinned() {
        // Tripwire, not a second copy of the list: the audited substrate set has
        // 12 behavior channels (#15317). Changing the set is deliberate — when
        // this fires, update the ledger table (docs/plan/campaign-flag-ledger.md)
        // to match.
        assert_eq!(
            CAMPAIGN_STORE_CHANNELS.len(),
            12,
            "campaign channel set changed; sync the ledger"
        );
    }

    /// The gauge script (`scripts/bench/campaign-gauge/run.sh`) hand-lists the
    /// same stack in a bash `CAMPAIGN_FLAGS=( ... )` array. That copy is the one
    /// that actually composes the substrate the determinism check measures, so
    /// bind it to the const here — this is what makes `CAMPAIGN_STORE_CHANNELS`
    /// the machine-checked single source of truth its docs claim, rather than a
    /// prose "keep in sync". The gauge array is the const plus the explicit
    /// `TSZ_DETERMINISTIC_STORE_ELECTION` override (see the doc comment above).
    #[test]
    fn gauge_stack_matches_registry() {
        const RUN_SH: &str = include_str!("../../../../../scripts/bench/campaign-gauge/run.sh");
        let open = RUN_SH
            .find("CAMPAIGN_FLAGS=(")
            .expect("run.sh must define CAMPAIGN_FLAGS=(");
        let body = &RUN_SH[open..];
        let close = body.find(')').expect("CAMPAIGN_FLAGS=( ... ) must close");
        let gauge: Vec<&str> = body[..close]
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("TSZ_"))
            .collect();

        let mut expected: Vec<&str> = CAMPAIGN_STORE_CHANNELS.to_vec();
        expected.push("TSZ_DETERMINISTIC_STORE_ELECTION");
        expected.sort_unstable();
        let mut gauge_sorted = gauge.clone();
        gauge_sorted.sort_unstable();

        assert_eq!(
            gauge_sorted, expected,
            "gauge CAMPAIGN_FLAGS must equal CAMPAIGN_STORE_CHANNELS + \
             TSZ_DETERMINISTIC_STORE_ELECTION; update scripts/bench/campaign-gauge/run.sh"
        );
    }
}
