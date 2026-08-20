//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/statements/for_recovery.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4adfd35ff7771e331225be231587cee228daaf3a245651837c78272ec7a765c9 307 invalid_let_of_array_for_recovery_accepts_trailing_header_trivia
    #[test]
    fn invalid_let_of_array_for_recovery_accepts_trailing_header_trivia() {
        for source in [
            "for (let of [1, 2, 3] ) ;",
            "for (let of [1, 2, 3] /* keep */) ;",
            "for (let of [1, 2, 3] // keep\n) ;",
        ] {
            let output = emit_es5(source);

            assert!(
                output.contains("for (let of, []; 1, 2, 3; )"),
                "Invalid `let of` recovery should ignore trailing header trivia.\nSource:\n{source}\nOutput:\n{output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 4adfd35ff7771e331225be231587cee228daaf3a245651837c78272ec7a765c9

// TSZ_INLINE_TEST_BEGIN 37f28a5e6462ca6e6565021915bedbb4a9649562e95249532515e2bf01f76136 323 typed_for_body_call_recovery_preserves_tsc_header_shape
    #[test]
    fn typed_for_body_call_recovery_preserves_tsc_header_shape() {
        let output = emit_es5("for (let x: y) { z(x); }");

        assert!(
            output.contains("for (let x, { z }; (x); )\n    ;"),
            "Typed recovered `for` should preserve tsc's recovered call header.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 37f28a5e6462ca6e6565021915bedbb4a9649562e95249532515e2bf01f76136

// TSZ_INLINE_TEST_BEGIN 396da45436b9834360b56c083873594399f3f92aad0c78b249888d0f290cf401 333 typed_for_body_call_recovery_uses_source_identifiers
    #[test]
    fn typed_for_body_call_recovery_uses_source_identifiers() {
        let output = emit_es5("for (let item: Type) { consume(item); }");

        assert!(
            output.contains("for (let item, { consume }; (item); )\n    ;"),
            "Typed recovered `for` should be source-backed, not fixture-name-specific.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 396da45436b9834360b56c083873594399f3f92aad0c78b249888d0f290cf401

// TSZ_INLINE_TEST_BEGIN 7be2ad78be961ad077b455a3425aa17e9506bade97afa7877be853ee14345846 343 typed_for_body_call_recovery_leaves_valid_for_loops_alone
    #[test]
    fn typed_for_body_call_recovery_leaves_valid_for_loops_alone() {
        let output = emit_es5("for (let x; ; ) { z(x); }");

        assert!(
            output.contains("for (var x;;) {"),
            "Valid `for` loops should continue through the normal printer path.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 7be2ad78be961ad077b455a3425aa17e9506bade97afa7877be853ee14345846

// TSZ_INLINE_TEST_BEGIN 6c6b02ac782add8ed3bb0619198d7dff6d1758aff64d2bbaa02a66c2b0d72368 353 typed_for_body_call_recovery_accepts_unicode_identifiers
    #[test]
    fn typed_for_body_call_recovery_accepts_unicode_identifiers() {
        // \u{e9} and \u{65e5} are valid ECMAScript identifier-start chars.
        let output =
            emit_es5("for (let r\u{e9}sum\u{e9}: Type) { donn\u{e9}es(r\u{e9}sum\u{e9}); }");
        assert!(
            output.contains("r\u{e9}sum\u{e9}"),
            "Unicode binding identifier should be preserved in recovery.\nOutput:\n{output}"
        );

        let output2 = emit_es5(
            "for (let \u{65e5}\u{672c}\u{8a9e}: T) { \u{51e6}\u{7406}(\u{65e5}\u{672c}\u{8a9e}); }",
        );
        assert!(
            output2.contains("\u{65e5}\u{672c}\u{8a9e}"),
            "CJK binding identifier should be preserved in recovery.\nOutput:\n{output2}"
        );
    }
// TSZ_INLINE_TEST_END 6c6b02ac782add8ed3bb0619198d7dff6d1758aff64d2bbaa02a66c2b0d72368
