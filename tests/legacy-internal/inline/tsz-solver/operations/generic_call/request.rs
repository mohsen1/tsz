//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/generic_call/request.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8ebd13e9455028fefb0a0a21aa369aa65bd531f2e77d7abfe428772b9af4ec3a 44 request_exposes_func_and_arg_types
    #[test]
    fn request_exposes_func_and_arg_types() {
        let func = empty_func();
        let args = [TypeId::STRING, TypeId::NUMBER];
        let req = GenericCallRequest::new(&func, &args);
        assert_eq!(req.arg_types(), &[TypeId::STRING, TypeId::NUMBER]);
        assert_eq!(req.func().params.len(), 0);
    }
// TSZ_INLINE_TEST_END 8ebd13e9455028fefb0a0a21aa369aa65bd531f2e77d7abfe428772b9af4ec3a

// TSZ_INLINE_TEST_BEGIN abbd18b941b5a2fae9a9be2819b065f085e454736201db82af61bc42ba9bb723 53 request_accepts_empty_arg_types
    #[test]
    fn request_accepts_empty_arg_types() {
        let func = empty_func();
        let req = GenericCallRequest::new(&func, &[]);
        assert!(req.arg_types().is_empty());
    }
// TSZ_INLINE_TEST_END abbd18b941b5a2fae9a9be2819b065f085e454736201db82af61bc42ba9bb723

// TSZ_INLINE_TEST_BEGIN 501de4aa6b6aa618b93fb2a4e885b48a74445580b095e9c9889b01f96a9b6458 60 request_preserves_arg_type_order
    #[test]
    fn request_preserves_arg_type_order() {
        let func = empty_func();
        let args = [TypeId::BOOLEAN, TypeId::STRING, TypeId::NUMBER];
        let req = GenericCallRequest::new(&func, &args);
        assert_eq!(req.arg_types()[0], TypeId::BOOLEAN);
        assert_eq!(req.arg_types()[1], TypeId::STRING);
        assert_eq!(req.arg_types()[2], TypeId::NUMBER);
    }
// TSZ_INLINE_TEST_END 501de4aa6b6aa618b93fb2a4e885b48a74445580b095e9c9889b01f96a9b6458
