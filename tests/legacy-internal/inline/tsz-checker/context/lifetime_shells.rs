//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/lifetime_shells.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e9012b5162821ab7e19a42f4b5f5a44688af9201de970ad1a9611fdb801c67a7 162 shells_implement_default
    /// The shells should be `Default` so future migrations can wire them up
    /// via `Default::default()` without bespoke constructor plumbing.
    #[test]
    fn shells_implement_default() {
        let _ = WorkerContext::default();
        let _ = FileSession::default();
        let _ = SpeculationScope::default();
        let _ = LspPersistentCache::default();
    }
// TSZ_INLINE_TEST_END e9012b5162821ab7e19a42f4b5f5a44688af9201de970ad1a9611fdb801c67a7

// TSZ_INLINE_TEST_BEGIN 38c9a03b01b0cc156badd65fe59a1dbc1e2be08a3f7653aae7a4ea156360f035 174 shells_can_be_constructed_const
    /// `const fn new()` returns the same logical shape as `Default::default()`
    /// — verifies that const-construction is wired up for compile-time
    /// initialization (the future migration may need this for static
    /// scratch).
    #[test]
    fn shells_can_be_constructed_const() {
        const _W: WorkerContext = WorkerContext::new();
        const _F: FileSession = FileSession::new();
        const _S: SpeculationScope = SpeculationScope::new();
        const _L: LspPersistentCache = LspPersistentCache::new();
    }
// TSZ_INLINE_TEST_END 38c9a03b01b0cc156badd65fe59a1dbc1e2be08a3f7653aae7a4ea156360f035
