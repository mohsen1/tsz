//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/check_file.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4a20922e379f6d962e90d9c2c8e5789cb96369a8e118817fe2e5f27b664cbf3f 927 assignment_is_deterministic
    #[test]
    fn assignment_is_deterministic() {
        let costs = [5, 1, 4, 1, 3, 9, 2, 6];
        assert_eq!(
            lpt_bin_assignment(&costs, 3),
            lpt_bin_assignment(&costs, 3),
            "same input must yield the same assignment"
        );
    }
// TSZ_INLINE_TEST_END 4a20922e379f6d962e90d9c2c8e5789cb96369a8e118817fe2e5f27b664cbf3f

// TSZ_INLINE_TEST_BEGIN 48190dce30a77487fd441580e82cbcd496efea210d4bd10ea8f6d068093e70c5 937 every_item_lands_in_a_valid_bin
    #[test]
    fn every_item_lands_in_a_valid_bin() {
        let costs = [7, 7, 7, 1, 1, 1, 1];
        let pool = 4;
        let bins = lpt_bin_assignment(&costs, pool);
        assert_eq!(bins.len(), costs.len());
        assert!(bins.iter().all(|&b| b < pool));
    }
// TSZ_INLINE_TEST_END 48190dce30a77487fd441580e82cbcd496efea210d4bd10ea8f6d068093e70c5

// TSZ_INLINE_TEST_BEGIN 298c14db1137ed48c26f9eaa06a1b792edeeb53785201b350698d64de8a29e5f 946 edge_cases
    #[test]
    fn edge_cases() {
        // Empty input.
        assert!(lpt_bin_assignment(&[], 4).is_empty());
        // Single bin: everything in bin 0.
        assert_eq!(lpt_bin_assignment(&[3, 1, 2], 1), vec![0, 0, 0]);
        // Degenerate pool_size 0 is clamped to 1.
        assert_eq!(lpt_bin_assignment(&[3, 1], 0), vec![0, 0]);
        // Fewer items than bins: heaviest-first, one per bin.
        assert_eq!(lpt_bin_assignment(&[1, 3, 2], 8), vec![2, 0, 1]);
    }
// TSZ_INLINE_TEST_END 298c14db1137ed48c26f9eaa06a1b792edeeb53785201b350698d64de8a29e5f

// TSZ_INLINE_TEST_BEGIN c893fcdd5c331b95d6203b1aa361b388a362f47df28c7397f86aae90bd31ae77 962 lpt_beats_round_robin_under_aligned_skew
    /// The headline property: when heavy files happen to align to the pool
    /// width, cost-blind round-robin piles them all into one straggler bin,
    /// while LPT spreads them and pads with the light files — reaching the
    /// theoretical `total / pool_size` makespan lower bound.
    #[test]
    fn lpt_beats_round_robin_under_aligned_skew() {
        let pool = 8;
        let n = 64;
        // Eight heavy files at positions 0, 8, 16, ... — all `≡ 0 (mod 8)`, so
        // round-robin sends every one of them to bin 0.
        let mut costs = vec![1u64; n];
        for k in 0..8 {
            costs[k * 8] = 100;
        }
        let total: u64 = costs.iter().sum();
        let lower_bound = total.div_ceil(pool as u64);

        let rr_max = max_bin_load(&costs, &round_robin(n, pool), pool);
        let lpt_max = max_bin_load(&costs, &lpt_bin_assignment(&costs, pool), pool);

        // Round-robin strands all eight heavies in one bin.
        assert_eq!(rr_max, 8 * 100, "round-robin clusters the aligned heavies");
        // LPT is at or near the optimal makespan, far below round-robin.
        assert!(
            lpt_max < rr_max / 4,
            "lpt makespan {lpt_max} should be far below round-robin {rr_max}"
        );
        // LPT never exceeds the lower bound plus one max-cost item (its 4/3
        // approximation guarantee comfortably implies this looser bound).
        let max_cost = *costs.iter().max().unwrap();
        assert!(
            lpt_max <= lower_bound + max_cost,
            "lpt makespan {lpt_max} exceeds lower_bound {lower_bound} + max_cost {max_cost}"
        );
    }
// TSZ_INLINE_TEST_END c893fcdd5c331b95d6203b1aa361b388a362f47df28c7397f86aae90bd31ae77

// TSZ_INLINE_TEST_BEGIN 16d5ef718e43b9d97aeb862f0cbeae0c0ee4daf3a059d8582f19cf689a816461 1001 lpt_respects_greedy_bound_and_wins_in_aggregate
    /// Across a spread of continuous (power-law-ish) cost distributions and
    /// pool widths, every LPT assignment satisfies the always-true least-loaded
    /// greedy bound `makespan <= ceil(total / pool) + max_cost`, and LPT beats
    /// cost-blind round-robin in aggregate — the robustness claim that
    /// motivates replacing the static split. (Per-trial `lpt <= rr` is not
    /// asserted: LPT is a 4/3-approximation, so an unlucky round-robin can
    /// occasionally tie or edge it on a single shape; the aggregate cannot.)
    #[test]
    fn lpt_respects_greedy_bound_and_wins_in_aggregate() {
        let mut state: u64 = 0x9E37_79B9;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (mut lpt_total, mut rr_total) = (0u64, 0u64);
        for _ in 0..200 {
            let n = 1 + (next() % 300) as usize;
            let pool = 1 + (next() % 16) as usize;
            let costs: Vec<u64> = (0..n)
                .map(|_| {
                    if next() % 16 == 0 {
                        50 + next() % 200
                    } else {
                        1
                    }
                })
                .collect();
            let total: u64 = costs.iter().sum();
            let max_cost = *costs.iter().max().unwrap();
            let lpt_max = max_bin_load(&costs, &lpt_bin_assignment(&costs, pool), pool);
            assert!(
                lpt_max <= total.div_ceil(pool as u64) + max_cost,
                "lpt {lpt_max} violated greedy bound for n={n} pool={pool}"
            );
            lpt_total += lpt_max;
            rr_total += max_bin_load(&costs, &round_robin(n, pool), pool);
        }
        assert!(
            lpt_total < rr_total,
            "lpt aggregate makespan {lpt_total} should beat round-robin {rr_total}"
        );
    }
// TSZ_INLINE_TEST_END 16d5ef718e43b9d97aeb862f0cbeae0c0ee4daf3a059d8582f19cf689a816461
