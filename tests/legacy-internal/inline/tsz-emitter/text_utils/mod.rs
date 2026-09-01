//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/text_utils/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 365f8edbf9ad3fe2e49c95c93f553aa84da2407cfb95ca3e4e9293524f789399 25 delegates_to_shared_owner
    /// Behavior tests live beside the owner in `tsz_common::numeric`; this
    /// smoke test only pins that the emitter delegate wires through.
    #[test]
    fn delegates_to_shared_owner() {
        assert_eq!(format_js_number(1e21), "1e+21");
        assert_eq!(format_js_number(-0.0), "0");
    }
// TSZ_INLINE_TEST_END 365f8edbf9ad3fe2e49c95c93f553aa84da2407cfb95ca3e4e9293524f789399
