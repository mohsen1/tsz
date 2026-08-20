//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/lib_capabilities.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5d6e296634425fc3edd0142b703d0d93508664b59535ae5cceeb83dc09c5684d 258 baseline_validation_excludes_dom_globals
    #[test]
    fn baseline_validation_excludes_dom_globals() {
        let baseline: Vec<_> = baseline_global_symbols().collect();
        assert!(baseline.contains(&"Object"));
        assert!(baseline.contains(&"Promise"));
        assert!(!baseline.contains(&"console"));

        let dom: Vec<_> = dom_global_symbols().collect();
        assert_eq!(dom, vec!["console"]);
    }
// TSZ_INLINE_TEST_END 5d6e296634425fc3edd0142b703d0d93508664b59535ae5cceeb83dc09c5684d

// TSZ_INLINE_TEST_BEGIN 3823459772f4ae9847d4c0ab8d00d4f25223c4844a9945a01c9d62dba131a12a 269 type_and_value_queries_share_the_same_table
    #[test]
    fn type_and_value_queries_share_the_same_table() {
        assert!(is_known_es_type("Promise"));
        assert!(is_known_es_type("AsyncGenerator"));
        assert!(!is_known_es_type("PromiseLike"));

        assert!(is_known_value_lib_suggestion("Promise"));
        assert!(is_known_value_lib_suggestion("Reflect"));
        assert!(!is_known_value_lib_suggestion("Proxy"));
    }
// TSZ_INLINE_TEST_END 3823459772f4ae9847d4c0ab8d00d4f25223c4844a9945a01c9d62dba131a12a

// TSZ_INLINE_TEST_BEGIN cf5344e3469d99bdde1739898fe739603fc28bd64ed3266f2ecaeb0270e04872 280 suggested_libs_come_from_capabilities
    #[test]
    fn suggested_libs_come_from_capabilities() {
        assert_eq!(
            suggested_lib_for_type("Promise").map(RequiredLib::as_str),
            Some("es2015")
        );
        assert_eq!(
            suggested_lib_for_type("SharedArrayBuffer").map(RequiredLib::as_str),
            Some("es2017")
        );
        assert_eq!(
            suggested_lib_for_type("AsyncGenerator").map(RequiredLib::as_str),
            Some("es2018")
        );
        assert_eq!(
            suggested_lib_for_type("BigInt").map(RequiredLib::as_str),
            Some("es2020")
        );
        assert_eq!(
            suggested_lib_for_type("WeakRef").map(RequiredLib::as_str),
            Some("es2021")
        );
        assert_eq!(
            suggested_lib_for_type("Disposable").map(RequiredLib::as_str),
            Some("esnext")
        );
        assert_eq!(suggested_lib_for_type("UnknownType"), None);
    }
// TSZ_INLINE_TEST_END cf5344e3469d99bdde1739898fe739603fc28bd64ed3266f2ecaeb0270e04872
