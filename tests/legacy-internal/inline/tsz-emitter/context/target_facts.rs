//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/context/target_facts.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e675bd177c18cfcb96534a818a1779be3b9d77b5bd14779cdaf31a3e99036be8 70 es2015_is_the_ts6_strategic_floor
    #[test]
    fn es2015_is_the_ts6_strategic_floor() {
        let es5 = EmitTargetFacts::from_target(ScriptTarget::ES5);
        assert!(es5.legacy_below_ts6_floor);
        assert!(es5.deprecated_in_ts6);
        assert!(es5.legacy_es5_or_lower);

        let es2015 = EmitTargetFacts::from_target(ScriptTarget::ES2015);
        assert!(es2015.is_ts6_strategic_target());
        assert!(!es2015.legacy_es5_or_lower);
    }
// TSZ_INLINE_TEST_END e675bd177c18cfcb96534a818a1779be3b9d77b5bd14779cdaf31a3e99036be8

// TSZ_INLINE_TEST_BEGIN 8054bc47388b3afb2e28dc532aedc4e08498cdca4b30aeddf35c10f3b2a08299 82 es3_is_removed_not_merely_deprecated
    #[test]
    fn es3_is_removed_not_merely_deprecated() {
        let facts = EmitTargetFacts::from_target(ScriptTarget::ES3);
        assert!(facts.removed_in_ts6);
        assert!(!facts.deprecated_in_ts6);
        assert!(facts.legacy_below_ts6_floor);
    }
// TSZ_INLINE_TEST_END 8054bc47388b3afb2e28dc532aedc4e08498cdca4b30aeddf35c10f3b2a08299

// TSZ_INLINE_TEST_BEGIN 561a02069a5fc2d455bdac4ab9bd89c3585483674786fcaf0cc845c7e6c60325 90 es2025_preserves_using_declarations
    #[test]
    fn es2025_preserves_using_declarations() {
        assert!(!EmitTargetFacts::from_target(ScriptTarget::ES2022).supports_using_declarations);
        assert!(EmitTargetFacts::from_target(ScriptTarget::ES2025).supports_using_declarations);
        assert!(EmitTargetFacts::from_target(ScriptTarget::ESNext).supports_using_declarations);
    }
// TSZ_INLINE_TEST_END 561a02069a5fc2d455bdac4ab9bd89c3585483674786fcaf0cc845c7e6c60325
