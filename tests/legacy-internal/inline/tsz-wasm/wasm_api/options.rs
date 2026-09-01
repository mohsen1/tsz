//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-wasm/src/wasm_api/options.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 62b384a077d2d32349871463dc0b41408062fc228a4413d4d42cca9d2266f567 49 target_default_is_es5
    #[test]
    fn target_default_is_es5() {
        assert_eq!(target_kind_from_u8(None), ScriptTarget::ES5);
    }
// TSZ_INLINE_TEST_END 62b384a077d2d32349871463dc0b41408062fc228a4413d4d42cca9d2266f567

// TSZ_INLINE_TEST_BEGIN 346e37aaf940a891afcef27f0ea7ae8ce75ff9dc3eef739466b25fbe46b7e516 54 target_unknown_numeric_falls_back_to_es5
    #[test]
    fn target_unknown_numeric_falls_back_to_es5() {
        // 250 is outside the documented numeric range.
        assert_eq!(target_kind_from_u8(Some(250)), ScriptTarget::ES5);
    }
// TSZ_INLINE_TEST_END 346e37aaf940a891afcef27f0ea7ae8ce75ff9dc3eef739466b25fbe46b7e516

// TSZ_INLINE_TEST_BEGIN 42ec5b9f11d15ebf91dc75538e4ffc41af2e5e530aae2d89b801f0b1fa8de12c 60 target_known_numeric_round_trips
    #[test]
    fn target_known_numeric_round_trips() {
        // 0 → ES3, 1 → ES5, 2 → ES2015 are the canonical low values.
        assert_eq!(target_kind_from_u8(Some(0)), ScriptTarget::ES3);
        assert_eq!(target_kind_from_u8(Some(1)), ScriptTarget::ES5);
        assert_eq!(target_kind_from_u8(Some(2)), ScriptTarget::ES2015);
    }
// TSZ_INLINE_TEST_END 42ec5b9f11d15ebf91dc75538e4ffc41af2e5e530aae2d89b801f0b1fa8de12c

// TSZ_INLINE_TEST_BEGIN ad71593c76c468dda3d7272f4d47465e6796c496b906ad2825433fb8d1c02f5b 68 module_default_is_none
    #[test]
    fn module_default_is_none() {
        assert_eq!(module_kind_from_u8(None), ModuleKind::None);
    }
// TSZ_INLINE_TEST_END ad71593c76c468dda3d7272f4d47465e6796c496b906ad2825433fb8d1c02f5b

// TSZ_INLINE_TEST_BEGIN 3f88b78093cb701323bf2ff8d9092165d1937aba2ac6c2cd21d825d2177a9cd8 73 module_unknown_numeric_falls_back_to_none
    #[test]
    fn module_unknown_numeric_falls_back_to_none() {
        assert_eq!(module_kind_from_u8(Some(250)), ModuleKind::None);
    }
// TSZ_INLINE_TEST_END 3f88b78093cb701323bf2ff8d9092165d1937aba2ac6c2cd21d825d2177a9cd8

// TSZ_INLINE_TEST_BEGIN 45c4a82e635b0cfdea98840d6093f3f739153e700222d1daf46cb8bbb5a288a2 78 module_known_numeric_round_trips
    #[test]
    fn module_known_numeric_round_trips() {
        // 0 → None, 1 → CommonJS, 5 → ES2015 are tsc's canonical mappings.
        assert_eq!(module_kind_from_u8(Some(0)), ModuleKind::None);
        assert_eq!(module_kind_from_u8(Some(1)), ModuleKind::CommonJS);
        assert_eq!(module_kind_from_u8(Some(5)), ModuleKind::ES2015);
    }
// TSZ_INLINE_TEST_END 45c4a82e635b0cfdea98840d6093f3f739153e700222d1daf46cb8bbb5a288a2
