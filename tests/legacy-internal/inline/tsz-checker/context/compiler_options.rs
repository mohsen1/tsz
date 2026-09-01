//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/compiler_options.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 458ca0292e700a45914a2ff5f327529044dd53e5b546a172eb7a6f3ce494a11a 334 d_tsx_is_source_file_not_declaration_file
    #[test]
    fn d_tsx_is_source_file_not_declaration_file() {
        assert!(!is_declaration_file_name("index.d.tsx"));
        assert!(is_declaration_file_name("index.d.ts"));
        assert!(is_declaration_file_name("index.d.mts"));
        assert!(is_declaration_file_name("index.d.cts"));
    }
// TSZ_INLINE_TEST_END 458ca0292e700a45914a2ff5f327529044dd53e5b546a172eb7a6f3ce494a11a

// TSZ_INLINE_TEST_BEGIN 8d03cffae14ed40809a9eaf4f229acbdfb83822190d0deef5a902a7ffafda883 342 d_tsx_does_not_emit_declaration_file_ambient_diagnostics
    #[test]
    fn d_tsx_does_not_emit_declaration_file_ambient_diagnostics() {
        let diagnostics = check_source(
            "let x: number;\nx = \"bad\";\n",
            "index.d.tsx",
            CheckerOptions {
                strict: true,
                ..CheckerOptions::default()
            },
        );
        let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();

        assert!(codes.contains(&2322), "{diagnostics:?}");
        assert!(!codes.contains(&1036), "{diagnostics:?}");
        assert!(!codes.contains(&1046), "{diagnostics:?}");
    }
// TSZ_INLINE_TEST_END 8d03cffae14ed40809a9eaf4f229acbdfb83822190d0deef5a902a7ffafda883
