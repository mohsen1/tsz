//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/apparent_type.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fb8ac5a688f784fd3283455235ed8fea6265af7e5605a09103c2c56dbf7a87a1 149 property_access_decision_site_stays_shrink_only
    #[test]
    fn property_access_decision_site_stays_shrink_only() {
        let count = raw_entry_call_count(RESOLVE_SRC);
        // Exact match, not `<=`: `BASELINE` is `0` (the site is fully behind the
        // gateway), so a `count <= BASELINE` would be an always-true comparison
        // against `usize::MIN` (`clippy::absurd_extreme_comparisons`). Any raw
        // evaluate_* call added to the decision site makes `count > BASELINE` and
        // fails; migrating one out (only possible if `BASELINE` is later raised)
        // makes `count < BASELINE` and must be paired with lowering `BASELINE`.
        assert_eq!(
            count, BASELINE,
            "property-access decision site changed its raw evaluation-entry call \
             count ({count} vs baseline {BASELINE}); route new receiver reductions \
             through query_boundaries::apparent_type instead of calling a raw \
             evaluate_* entry directly. If you migrated a call *out*, lower BASELINE \
             to {count}.",
        );
    }
// TSZ_INLINE_TEST_END fb8ac5a688f784fd3283455235ed8fea6265af7e5605a09103c2c56dbf7a87a1

// TSZ_INLINE_TEST_BEGIN 0de76d1a40ffeecdd55a898874ef9f54c7a060e23ef7b88560d4cc9fe8968914 168 receiver_reduction_uses_the_gateway
    #[test]
    fn receiver_reduction_uses_the_gateway() {
        assert!(
            RESOLVE_SRC.contains("apparent_type_of_receiver_env(")
                && RESOLVE_SRC.contains("apparent_type_of_receiver_light("),
            "property-access receiver reduction should route through the \
             apparent_type gateway",
        );
    }
// TSZ_INLINE_TEST_END 0de76d1a40ffeecdd55a898874ef9f54c7a060e23ef7b88560d4cc9fe8968914
