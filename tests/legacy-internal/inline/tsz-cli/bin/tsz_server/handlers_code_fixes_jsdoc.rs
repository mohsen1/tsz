//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes_jsdoc.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fc3ae1c974a4ad3bbcf0ec6004251855e5d45e43b673d1cbd7ea4cbfa479ef66 1570 normalize_jsdoc_function_type
    #[test]
    fn normalize_jsdoc_function_type() {
        assert_eq!(
            Server::normalize_jsdoc_type("function(*, ...number, ...boolean): void"),
            "(arg0: any, arg1: number[], ...rest: boolean[]) => void"
        );
        assert_eq!(
            Server::normalize_jsdoc_type("function(this:{ a: string}, string, number): boolean"),
            "(this: { a: string; }, arg1: string, arg2: number) => boolean"
        );
    }
// TSZ_INLINE_TEST_END fc3ae1c974a4ad3bbcf0ec6004251855e5d45e43b673d1cbd7ea4cbfa479ef66

// TSZ_INLINE_TEST_BEGIN d034f299b58f8dada00e7c624ef899b704db2022f1a46e4146ee335fd674dc89 1582 normalize_jsdoc_object_generic
    #[test]
    fn normalize_jsdoc_object_generic() {
        assert_eq!(
            Server::normalize_jsdoc_type("Object<string, boolean>"),
            "{ [s: string]: boolean; }"
        );
        assert_eq!(
            Server::normalize_jsdoc_type("Object<number, string>"),
            "{ [n: number]: string; }"
        );
    }
// TSZ_INLINE_TEST_END d034f299b58f8dada00e7c624ef899b704db2022f1a46e4146ee335fd674dc89

// TSZ_INLINE_TEST_BEGIN a54406567eaddb266c1cfbae4cc63c2a28a47b41d5326d9f78d4b2a440f57501 1594 normalize_jsdoc_promise_generic
    #[test]
    fn normalize_jsdoc_promise_generic() {
        assert_eq!(
            Server::normalize_jsdoc_type("promise<String>"),
            "Promise<string>"
        );
    }
// TSZ_INLINE_TEST_END a54406567eaddb266c1cfbae4cc63c2a28a47b41d5326d9f78d4b2a440f57501

// TSZ_INLINE_TEST_BEGIN bb3554c725a5288fc5bd747348c1c13494384106ecb5dc421c987911f429f592 1602 jsdoc_fallback_object_index_signatures
    #[test]
    fn jsdoc_fallback_object_index_signatures() {
        let src = "\n/** @param {Object<string, boolean>} sb\n  * @param {Object<number, string>} ns */\nfunction f(sb, ns) {\n    sb; ns;\n}\n";
        let expected = "\n/** @param {Object<string, boolean>} sb\n  * @param {Object<number, string>} ns */\nfunction f(sb: { [s: string]: boolean; }, ns: { [n: number]: string; }) {\n    sb; ns;\n}\n";
        let actual = Server::apply_simple_jsdoc_annotation_fallback(src)
            .expect("expected jsdoc fallback to apply");
        assert_eq!(actual, expected);
    }
// TSZ_INLINE_TEST_END bb3554c725a5288fc5bd747348c1c13494384106ecb5dc421c987911f429f592

// TSZ_INLINE_TEST_BEGIN 239d2055e5ed95e97a887d2d04daa3258ed8ba41b7a94e3687bcacac376ebe83 1611 jsdoc_fallback_template_function
    #[test]
    fn jsdoc_fallback_template_function() {
        let src = "/**\n * @template T\n * @param {number} a\n * @param {T} b\n */\nfunction f(a, b) {\n    return a || b;\n}\n";
        let expected = "/**\n * @template T\n * @param {number} a\n * @param {T} b\n */\nfunction f<T>(a: number, b: T) {\n    return a || b;\n}\n";
        let actual = Server::apply_simple_jsdoc_annotation_fallback(src)
            .expect("expected jsdoc fallback to apply");
        assert_eq!(actual, expected);
    }
// TSZ_INLINE_TEST_END 239d2055e5ed95e97a887d2d04daa3258ed8ba41b7a94e3687bcacac376ebe83
