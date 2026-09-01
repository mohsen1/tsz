//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/transforms/namespace_iife_binding_scan.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5cae9810267365cf88714697db6e8eae5e730670ca33da7f020faca19d6cb26d 353 detects_function_parameter_binding
    #[test]
    fn detects_function_parameter_binding() {
        // function f(schema) { ... } binds `schema`.
        assert!(text_has_non_namespace_binding_named(
            "{ function f(schema) { return 0; } }",
            "schema"
        ));
    }
// TSZ_INLINE_TEST_END 5cae9810267365cf88714697db6e8eae5e730670ca33da7f020faca19d6cb26d

// TSZ_INLINE_TEST_BEGIN f457aa585b7109833909cd52f77bbbaf5ea4024ed5a99f253cb5327403aa41a2 362 detects_function_parameter_binding_renamed_var
    #[test]
    fn detects_function_parameter_binding_renamed_var() {
        // Same shape, different chosen names — must still detect.
        assert!(text_has_non_namespace_binding_named(
            "{ function build(build) { return 0; } }",
            "build"
        ));
    }
// TSZ_INLINE_TEST_END f457aa585b7109833909cd52f77bbbaf5ea4024ed5a99f253cb5327403aa41a2

// TSZ_INLINE_TEST_BEGIN 3959c33a4c8c6f46fd437b81c557e9d87d541f8ec779481d0206aa3e23180ca7 371 detects_var_let_const_binding
    #[test]
    fn detects_var_let_const_binding() {
        assert!(text_has_non_namespace_binding_named("{ var n = 1; }", "n"));
        assert!(text_has_non_namespace_binding_named("{ let n = 1; }", "n"));
        assert!(text_has_non_namespace_binding_named(
            "{ const n = 1; }",
            "n"
        ));
    }
// TSZ_INLINE_TEST_END 3959c33a4c8c6f46fd437b81c557e9d87d541f8ec779481d0206aa3e23180ca7

// TSZ_INLINE_TEST_BEGIN 4fed37f4640287b21b9e417d448440c42f671671b48eaf5af68ff05482d0e1a3 381 ignores_qualified_member_reference
    #[test]
    fn ignores_qualified_member_reference() {
        // `schema.foo = ...` is a member reference, not a binding.
        assert!(!text_has_non_namespace_binding_named(
            "{ schema.createValidator = createValidator; }",
            "schema"
        ));
    }
// TSZ_INLINE_TEST_END 4fed37f4640287b21b9e417d448440c42f671671b48eaf5af68ff05482d0e1a3

// TSZ_INLINE_TEST_BEGIN 7a45929137bb1d690a06563f0a3f1ba094c0edec01d3b49279334d0cf1c4fee1 390 ignores_callee_reference
    #[test]
    fn ignores_callee_reference() {
        // `schema()` is a call, not a binding.
        assert!(!text_has_non_namespace_binding_named(
            "{ return schema(); }",
            "schema"
        ));
    }
// TSZ_INLINE_TEST_END 7a45929137bb1d690a06563f0a3f1ba094c0edec01d3b49279334d0cf1c4fee1

// TSZ_INLINE_TEST_BEGIN c94afe1ae9eb896e4c134f554e71308345f74aa068038821a53147a1c86de5a6 399 ignores_unrelated_names
    #[test]
    fn ignores_unrelated_names() {
        assert!(!text_has_non_namespace_binding_named(
            "{ function f(x) { return x; } }",
            "schema"
        ));
    }
// TSZ_INLINE_TEST_END c94afe1ae9eb896e4c134f554e71308345f74aa068038821a53147a1c86de5a6
