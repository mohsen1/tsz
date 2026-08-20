//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/generic_call/result.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ece21e9611aa7949b112d39bb86bc6360a76dab51861e0219ad535cf0ca7ea8e 72 result_wraps_call_result
    #[test]
    fn result_wraps_call_result() {
        let cr = CallResult::Success(TypeId::STRING);
        let result = GenericCallResult::new(cr);
        assert!(matches!(result.into_call_result(), CallResult::Success(t) if t == TypeId::STRING));
    }
// TSZ_INLINE_TEST_END ece21e9611aa7949b112d39bb86bc6360a76dab51861e0219ad535cf0ca7ea8e

// TSZ_INLINE_TEST_BEGIN ade66e2948ef3e278fe109171d0d14ff19f700de43d3a097e5ca9b01b2e950f1 79 result_into_call_result_consumes
    #[test]
    fn result_into_call_result_consumes() {
        let result = GenericCallResult::new(CallResult::Success(TypeId::NUMBER));
        let cr = result.into_call_result();
        assert!(matches!(cr, CallResult::Success(t) if t == TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END ade66e2948ef3e278fe109171d0d14ff19f700de43d3a097e5ca9b01b2e950f1

// TSZ_INLINE_TEST_BEGIN 7a79ab8da10dc1f207fb4dabe65cb35124c1f7626f4cc801280fc8dfa83c2dc9 86 result_starts_with_no_side_channel_data
    #[test]
    fn result_starts_with_no_side_channel_data() {
        let mut result = GenericCallResult::new(CallResult::Success(TypeId::VOID));
        assert!(result.take_instantiated_predicate().is_none());
        assert!(result.take_instantiated_params().is_none());
    }
// TSZ_INLINE_TEST_END 7a79ab8da10dc1f207fb4dabe65cb35124c1f7626f4cc801280fc8dfa83c2dc9

// TSZ_INLINE_TEST_BEGIN c72887f6597fd4f0f3c4ca1e7d992d953243efe33d4837ddf22b859c300374d6 93 result_take_instantiated_params_returns_and_clears
    #[test]
    fn result_take_instantiated_params_returns_and_clears() {
        let mut result = GenericCallResult::new(CallResult::Success(TypeId::VOID))
            .with_instantiated_params(Some(vec![]));
        let taken = result.take_instantiated_params();
        assert!(taken.is_some());
        assert!(result.take_instantiated_params().is_none());
    }
// TSZ_INLINE_TEST_END c72887f6597fd4f0f3c4ca1e7d992d953243efe33d4837ddf22b859c300374d6
