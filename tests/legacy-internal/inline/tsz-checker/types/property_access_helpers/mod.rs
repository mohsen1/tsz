//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/property_access_helpers/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2cc163e49e2352012537b2f0770bb515c54735db8ebda0f7b638d787d966dd89 33 explicit_property_in_intersection_suppresses_ts4111
    #[test]
    fn explicit_property_in_intersection_suppresses_ts4111() {
        let diagnostics = get_diagnostics(
            r#"
type Bag = { foo: string } & { [k: string]: string };
declare const bag: Bag;
bag.foo;
"#,
        );

        let ts4111 = diagnostics
            .iter()
            .filter(|(code, _)| *code == 4111)
            .collect::<Vec<_>>();
        assert!(
            ts4111.is_empty(),
            "Explicit properties in intersections should not be treated as pure index-signature access. Got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 2cc163e49e2352012537b2f0770bb515c54735db8ebda0f7b638d787d966dd89
