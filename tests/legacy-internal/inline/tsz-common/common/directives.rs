//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/common/directives.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a89c2679d223738a1e1374389ace6e6fb46ddfdae938b765c23c55a358bab2fe 60 plain_double_quoted_is_directive
    #[test]
    fn plain_double_quoted_is_directive() {
        assert!(is_use_strict_directive_raw_text("\"use strict\""));
    }
// TSZ_INLINE_TEST_END a89c2679d223738a1e1374389ace6e6fb46ddfdae938b765c23c55a358bab2fe

// TSZ_INLINE_TEST_BEGIN b1526e374e8e1605243ab0870a8e247567ee7859b07af3d2044a60efa4b137a0 65 plain_single_quoted_is_directive
    #[test]
    fn plain_single_quoted_is_directive() {
        assert!(is_use_strict_directive_raw_text("'use strict'"));
    }
// TSZ_INLINE_TEST_END b1526e374e8e1605243ab0870a8e247567ee7859b07af3d2044a60efa4b137a0

// TSZ_INLINE_TEST_BEGIN 938d2f5a87bf4a4bfbb62affea237a36bb2e8322a448aa2289abffeef062b492 70 escaped_form_is_not_directive
    #[test]
    fn escaped_form_is_not_directive() {
        // Cooked value is `use strict`, but the escape disqualifies it.
        assert!(!is_use_strict_directive_raw_text("\"use\\u0020strict\""));
        assert!(!is_use_strict_directive_raw_text("'use\\x20strict'"));
        assert!(!is_use_strict_directive_raw_text("\"\\u0075se strict\""));
    }
// TSZ_INLINE_TEST_END 938d2f5a87bf4a4bfbb62affea237a36bb2e8322a448aa2289abffeef062b492

// TSZ_INLINE_TEST_BEGIN 7e554dca6bd31a4d4c2dc32a85930e0719738d0c794f785003d82c5c92811bda 78 alternate_spacing_or_content_is_not_directive
    #[test]
    fn alternate_spacing_or_content_is_not_directive() {
        assert!(!is_use_strict_directive_raw_text("\"use strict \""));
        assert!(!is_use_strict_directive_raw_text("\" use strict\""));
        assert!(!is_use_strict_directive_raw_text("\"use  strict\""));
        assert!(!is_use_strict_directive_raw_text("`use strict`"));
        assert!(!is_use_strict_directive_raw_text("use strict"));
    }
// TSZ_INLINE_TEST_END 7e554dca6bd31a4d4c2dc32a85930e0719738d0c794f785003d82c5c92811bda

// TSZ_INLINE_TEST_BEGIN a369ca648fa84c6efea1cfe3f09a7b414eef1bfb5524a62fdfc9b2da81879dd3 87 cooked_fallback_only_applies_without_raw_text
    #[test]
    fn cooked_fallback_only_applies_without_raw_text() {
        // With raw text present, the escaped form is rejected even though the
        // cooked value matches.
        assert!(!is_use_strict_directive(
            Some("\"use\\u0020strict\""),
            "use strict"
        ));
        // Without raw text, the cooked value is the only available signal.
        assert!(is_use_strict_directive(None, "use strict"));
        assert!(!is_use_strict_directive(None, "use client"));
        // With raw text present, plain forms are accepted.
        assert!(is_use_strict_directive(Some("'use strict'"), "use strict"));
    }
// TSZ_INLINE_TEST_END a369ca648fa84c6efea1cfe3f09a7b414eef1bfb5524a62fdfc9b2da81879dd3
