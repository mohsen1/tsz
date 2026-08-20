//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/context/plan.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6b31c38fca52da50db855d8737d159807e7996a19942805fc9584f2ab37efc90 122 plan_carries_target_facts_from_options
    #[test]
    fn plan_carries_target_facts_from_options() {
        let options = PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };

        let plan = EmitPlan::empty(&options);

        assert_eq!(plan.target_facts.target, ScriptTarget::ES5);
        assert!(plan.is_legacy_target_lane());
    }
// TSZ_INLINE_TEST_END 6b31c38fca52da50db855d8737d159807e7996a19942805fc9584f2ab37efc90

// TSZ_INLINE_TEST_BEGIN fbb3a4b412f0f75ba99ad74dbf2aadd3c72f5dd1f2fff245efb884dfc2821a4e 135 plan_snapshots_lowering_helpers
    #[test]
    fn plan_snapshots_lowering_helpers() {
        let options = PrinterOptions::default();
        let mut transforms = TransformContext::new();
        transforms.helpers_mut().awaiter = true;

        let plan = EmitPlan::from_transforms(&options, transforms);

        assert!(plan.helpers.awaiter);
    }
// TSZ_INLINE_TEST_END fbb3a4b412f0f75ba99ad74dbf2aadd3c72f5dd1f2fff245efb884dfc2821a4e
