//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/properties.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 80819dbd94c95859a1f26fe9790418c59e052506468faae0f0c84d7d24441a26 1787 ts2339_suppressed_for_circular_typeof_constraint_direct_param
    /// TS2339 must be suppressed for property access on type parameters with
    /// circular `typeof` constraints (`T extends typeof a` where `a: T`).
    /// This applies to both direct parameters and destructured bindings.
    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_direct_param() {
        // Direct parameter: `a: T` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>(a: T) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for direct param with circular typeof constraint, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 80819dbd94c95859a1f26fe9790418c59e052506468faae0f0c84d7d24441a26

// TSZ_INLINE_TEST_BEGIN 9c7ab15030c8e3bf6e20ab9e81af16bcc33b0b5800e6ea54cc83cc22c5c5097d 1797 ts2339_suppressed_for_circular_typeof_constraint_destructured_param
    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_destructured_param() {
        // Destructured parameter: `{a}: {a:T}` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>({a}: {a:T}) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for destructured param with circular typeof constraint, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 9c7ab15030c8e3bf6e20ab9e81af16bcc33b0b5800e6ea54cc83cc22c5c5097d

// TSZ_INLINE_TEST_BEGIN 38dd5d5014e6686963dca7fcc7a088c9dd3f8f58eda97befcbdf57952f475fc0 1807 ts2339_suppressed_for_circular_typeof_constraint_array_destructured_param
    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_array_destructured_param() {
        // Array destructured parameter: `[a]: T[]` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>([a]: T[]) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for array-destructured param with circular typeof constraint, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 38dd5d5014e6686963dca7fcc7a088c9dd3f8f58eda97befcbdf57952f475fc0

// TSZ_INLINE_TEST_BEGIN 919a28c43445cc53166652ec7ac4c020ce4566280ca44e64b0305c7597e37711 1817 ts2339_not_suppressed_for_unconstrained_type_param
    #[test]
    fn ts2339_not_suppressed_for_unconstrained_type_param() {
        // Unconstrained type parameter should still emit TS2339
        let diags = diagnostics_for_source("function f<T>(a: T) { a.b; }");
        assert!(
            diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be emitted for unconstrained type param, got: {diags:?}"
        );
    }
// TSZ_INLINE_TEST_END 919a28c43445cc53166652ec7ac4c020ce4566280ca44e64b0305c7597e37711
