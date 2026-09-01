//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/def/core/campaign_channels.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f634c056ed7705abdc894ad756892a15ddcdbada7a82a0f6603083a97d32584d 69 channel_list_is_deduplicated
    #[test]
    fn channel_list_is_deduplicated() {
        let mut seen = std::collections::HashSet::new();
        for name in CAMPAIGN_STORE_CHANNELS {
            assert!(seen.insert(*name), "duplicate campaign channel: {name}");
        }
    }
// TSZ_INLINE_TEST_END f634c056ed7705abdc894ad756892a15ddcdbada7a82a0f6603083a97d32584d

// TSZ_INLINE_TEST_BEGIN e7c682d06d6fac1f5aceb9a6b3e6f8f02b63e215629086e5681d11c9c96edfb5 77 channel_names_are_tsz_prefixed
    #[test]
    fn channel_names_are_tsz_prefixed() {
        for name in CAMPAIGN_STORE_CHANNELS {
            assert!(
                name.starts_with("TSZ_"),
                "campaign channel env var must be TSZ_-prefixed: {name}"
            );
        }
    }
// TSZ_INLINE_TEST_END e7c682d06d6fac1f5aceb9a6b3e6f8f02b63e215629086e5681d11c9c96edfb5

// TSZ_INLINE_TEST_BEGIN 76011acd1eeddb60a3342721449fa1bc5c809780effa51c57b23095d7d907706 87 channel_count_is_pinned
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
// TSZ_INLINE_TEST_END 76011acd1eeddb60a3342721449fa1bc5c809780effa51c57b23095d7d907706

// TSZ_INLINE_TEST_BEGIN 69371c1cc5af2158b02675122aa469180578c1f90baa7cbdf79d8599ffaa4bcd 107 gauge_stack_matches_registry
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
// TSZ_INLINE_TEST_END 69371c1cc5af2158b02675122aa469180578c1f90baa7cbdf79d8599ffaa4bcd
