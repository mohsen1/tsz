//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-binder/src/binding/stack_guard.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1bb2e7d2935245088209b6c17cef8a65ab423e7d11b009afda46b6fa618b0e2a 96 unmeasurable_headroom_never_trips
    #[test]
    fn unmeasurable_headroom_never_trips() {
        // `wasm32` reports `None`; that must not count as critically low, or the
        // binder aborts mid-file and drops later declarations (issue #13815).
        assert!(!measured_headroom_below(None, 1024 * 1024));
        assert!(!measured_headroom_below(None, usize::MAX));
    }
// TSZ_INLINE_TEST_END 1bb2e7d2935245088209b6c17cef8a65ab423e7d11b009afda46b6fa618b0e2a

// TSZ_INLINE_TEST_BEGIN d312f0f71b5cf556f1935f072b4be099ce393ed8be7326a263c74640fe4aef04 104 measured_headroom_compares_against_threshold
    #[test]
    fn measured_headroom_compares_against_threshold() {
        assert!(measured_headroom_below(Some(512 * 1024), 1024 * 1024));
        assert!(!measured_headroom_below(Some(2 * 1024 * 1024), 1024 * 1024));
        assert!(!measured_headroom_below(Some(1024 * 1024), 1024 * 1024));
    }
// TSZ_INLINE_TEST_END d312f0f71b5cf556f1935f072b4be099ce393ed8be7326a263c74640fe4aef04
