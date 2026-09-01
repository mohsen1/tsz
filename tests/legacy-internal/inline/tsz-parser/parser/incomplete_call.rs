//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/incomplete_call.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 52f9a225ca30db6074005810f4848faacaae2c671d89d00618bed8524ce02031 335 paren_basic_call
    #[test]
    fn paren_basic_call() {
        let src = "foo(a, b";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.callee_end_offset, 3); // end of "foo"
        assert_eq!(ctx.delimiter, CallDelimiter::Paren);
        assert_eq!(ctx.active_parameter, 1);
        assert!(!ctx.is_new_expression);
    }
// TSZ_INLINE_TEST_END 52f9a225ca30db6074005810f4848faacaae2c671d89d00618bed8524ce02031

// TSZ_INLINE_TEST_BEGIN ca81a95448b53d4a4ffcc60563e132c6f7c123630a13ad8eabaa21b9a4b2059e 346 paren_callee_end_is_identifier_end_not_delimiter
    #[test]
    fn paren_callee_end_is_identifier_end_not_delimiter() {
        // "foo  (a" — callee_end_offset must point after "foo", not at "("
        let src = "foo  (a";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.callee_end_offset, 3); // end of "foo", not 5 (the `(`)
    }
// TSZ_INLINE_TEST_END ca81a95448b53d4a4ffcc60563e132c6f7c123630a13ad8eabaa21b9a4b2059e

// TSZ_INLINE_TEST_BEGIN 3981267099c7e65c6f514cd3362fda28343e358d2f9274f2da72bb80bd5e56e6 355 paren_renamed_callee
    #[test]
    fn paren_renamed_callee() {
        let src = "myFunction(x, y, z";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "myFunction");
        assert_eq!(ctx.active_parameter, 2);
    }
// TSZ_INLINE_TEST_END 3981267099c7e65c6f514cd3362fda28343e358d2f9274f2da72bb80bd5e56e6

// TSZ_INLINE_TEST_BEGIN a6a6091390b0d98a6ab492b3e4b00953f17b784a103003ace11d4f14883b6ebf 363 paren_new_expression
    #[test]
    fn paren_new_expression() {
        let src = "new Foo(a";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "Foo");
        assert!(ctx.is_new_expression);
    }
// TSZ_INLINE_TEST_END a6a6091390b0d98a6ab492b3e4b00953f17b784a103003ace11d4f14883b6ebf

// TSZ_INLINE_TEST_BEGIN 2ffcb8ec8733a8c9ce6be1b157516c6da80dab9a3741600bf0fd2abe7587edff 371 paren_new_expression_different_name
    #[test]
    fn paren_new_expression_different_name() {
        let src = "new MyClass(x, y";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "MyClass");
        assert!(ctx.is_new_expression);
        assert_eq!(ctx.active_parameter, 1);
    }
// TSZ_INLINE_TEST_END 2ffcb8ec8733a8c9ce6be1b157516c6da80dab9a3741600bf0fd2abe7587edff

// TSZ_INLINE_TEST_BEGIN 39dbdab45796fabea86f8ae6fe4d0797d254fa71d13bbd3bdcc145d83ab9caf1 380 paren_nested_call_cursor_in_outer
    #[test]
    fn paren_nested_call_cursor_in_outer() {
        let src = "outer(inner(), x";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "outer");
        assert_eq!(ctx.active_parameter, 1);
    }
// TSZ_INLINE_TEST_END 39dbdab45796fabea86f8ae6fe4d0797d254fa71d13bbd3bdcc145d83ab9caf1

// TSZ_INLINE_TEST_BEGIN 300f2aed4c6b9036677c93c083517da633cac012ff7743d066b3c50a2ccf3ceb 388 paren_stops_at_semicolon
    #[test]
    fn paren_stops_at_semicolon() {
        let src = "let x = 1; foo(a";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.active_parameter, 0);
    }
// TSZ_INLINE_TEST_END 300f2aed4c6b9036677c93c083517da633cac012ff7743d066b3c50a2ccf3ceb

// TSZ_INLINE_TEST_BEGIN 3c5bdc13e8bf5a821b65e7bf34a0be9663a38a2e7b59bfa3b48887386bbe24c9 396 paren_declaration_keyword_excluded
    #[test]
    fn paren_declaration_keyword_excluded() {
        let src = "function foo(a";
        assert!(find_incomplete_paren_call(src, src.len()).is_none());
    }
// TSZ_INLINE_TEST_END 3c5bdc13e8bf5a821b65e7bf34a0be9663a38a2e7b59bfa3b48887386bbe24c9

// TSZ_INLINE_TEST_BEGIN 4dffac1e4e7984f133a94e5cc2a3ffb62ba441bf9e967e82b0482fc2296ce7f7 402 paren_no_trigger
    #[test]
    fn paren_no_trigger() {
        assert!(find_incomplete_paren_call("no parens here", 14).is_none());
    }
// TSZ_INLINE_TEST_END 4dffac1e4e7984f133a94e5cc2a3ffb62ba441bf9e967e82b0482fc2296ce7f7

// TSZ_INLINE_TEST_BEGIN 7bc954499ce6fa4aad0bc0dedec85a66fe138bebeb5ecb4e9746caa5a626d835 409 angle_basic_generic
    #[test]
    fn angle_basic_generic() {
        let src = "foo<A, B";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.callee_end_offset, 3); // end of "foo"
        assert_eq!(ctx.delimiter, CallDelimiter::AngleBracket);
        assert_eq!(ctx.active_parameter, 1);
    }
// TSZ_INLINE_TEST_END 7bc954499ce6fa4aad0bc0dedec85a66fe138bebeb5ecb4e9746caa5a626d835

// TSZ_INLINE_TEST_BEGIN 4ae4452d1a0cb6c7b783450be5e7363990edec7f6d01c4eebfe92cfd4e417cdd 419 angle_different_callee
    #[test]
    fn angle_different_callee() {
        let src = "myGeneric<T, U, V";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "myGeneric");
        assert_eq!(ctx.active_parameter, 2);
    }
// TSZ_INLINE_TEST_END 4ae4452d1a0cb6c7b783450be5e7363990edec7f6d01c4eebfe92cfd4e417cdd

// TSZ_INLINE_TEST_BEGIN 688936829965d77ca7bdcc8b407723ede52876527ea445a709fff21189ebf2a2 427 angle_nested_angle_brackets
    #[test]
    fn angle_nested_angle_brackets() {
        let src = "foo<Array<number>, ";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.active_parameter, 1);
    }
// TSZ_INLINE_TEST_END 688936829965d77ca7bdcc8b407723ede52876527ea445a709fff21189ebf2a2

// TSZ_INLINE_TEST_BEGIN 0e1100f8d805cc3ff2950ea17c3c138ad2df6dec3a53c35cf61e7717cc7a19bb 435 angle_no_trigger
    #[test]
    fn angle_no_trigger() {
        assert!(find_incomplete_angle_call("no angle brackets", 17).is_none());
    }
// TSZ_INLINE_TEST_END 0e1100f8d805cc3ff2950ea17c3c138ad2df6dec3a53c35cf61e7717cc7a19bb

// TSZ_INLINE_TEST_BEGIN 1211d4d150ff08a073a2a3c64718b51b39eafbd3ae0b680667e980a5edaf59e3 440 angle_less_equal_comparison_is_not_generic_trigger
    #[test]
    fn angle_less_equal_comparison_is_not_generic_trigger() {
        let src = "foo <= value";
        assert!(find_incomplete_angle_call(src, src.len()).is_none());
    }
// TSZ_INLINE_TEST_END 1211d4d150ff08a073a2a3c64718b51b39eafbd3ae0b680667e980a5edaf59e3

// TSZ_INLINE_TEST_BEGIN 0573ce06f6805d60b4fee86f8cf53d882c427ce97b5a0d970528700bc06135f9 446 angle_nested_less_equal_comparison_is_not_generic_trigger
    #[test]
    fn angle_nested_less_equal_comparison_is_not_generic_trigger() {
        let src = "outer(foo <= value";
        assert!(find_incomplete_angle_call(src, src.len()).is_none());
    }
// TSZ_INLINE_TEST_END 0573ce06f6805d60b4fee86f8cf53d882c427ce97b5a0d970528700bc06135f9

// TSZ_INLINE_TEST_BEGIN 9331c83cd360cb3412ce36f486db0103721cb5aae98e1c17e8345dcb69dc1cee 452 angle_spaced_less_than_comparison_is_not_generic_trigger
    #[test]
    fn angle_spaced_less_than_comparison_is_not_generic_trigger() {
        let src = "foo < value";
        assert!(find_incomplete_angle_call(src, src.len()).is_none());
    }
// TSZ_INLINE_TEST_END 9331c83cd360cb3412ce36f486db0103721cb5aae98e1c17e8345dcb69dc1cee

// TSZ_INLINE_TEST_BEGIN 224700ea03f509d8f0c6786f44cbb70f83461d58e2e5c5c1a69377e49972a272 458 angle_nested_spaced_less_than_comparison_is_not_generic_trigger
    #[test]
    fn angle_nested_spaced_less_than_comparison_is_not_generic_trigger() {
        let src = "outer(foo < value";
        assert!(find_incomplete_angle_call(src, src.len()).is_none());
    }
// TSZ_INLINE_TEST_END 224700ea03f509d8f0c6786f44cbb70f83461d58e2e5c5c1a69377e49972a272

// TSZ_INLINE_TEST_BEGIN 896295125d1bdca86f4753558f7c17c0b202d3aed1c1f908f1976c5a8cfd9001 464 angle_less_than_generic_trigger_still_works
    #[test]
    fn angle_less_than_generic_trigger_still_works() {
        let src = "foo<T";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.active_parameter, 0);
    }
// TSZ_INLINE_TEST_END 896295125d1bdca86f4753558f7c17c0b202d3aed1c1f908f1976c5a8cfd9001

// TSZ_INLINE_TEST_BEGIN c70e82ae9b39b761d416faa50cace3587f1c4b79822b0f987d17806b31f8c1be 474 commas_basic
    #[test]
    fn commas_basic() {
        assert_eq!(count_top_level_commas("a, b, c", 0, 7), 2);
    }
// TSZ_INLINE_TEST_END c70e82ae9b39b761d416faa50cace3587f1c4b79822b0f987d17806b31f8c1be

// TSZ_INLINE_TEST_BEGIN 1b268583648c34d28d4663064d628f62fb10ddeed873680f1f2fbcf624fb9bf4 479 commas_nested_parens_ignored
    #[test]
    fn commas_nested_parens_ignored() {
        assert_eq!(count_top_level_commas("a, foo(x, y), b", 0, 15), 2);
    }
// TSZ_INLINE_TEST_END 1b268583648c34d28d4663064d628f62fb10ddeed873680f1f2fbcf624fb9bf4

// TSZ_INLINE_TEST_BEGIN a9c1c42e670f1f88489ca6277f9f2da2a4a56ac3429213ad068c42ea04ab4345 484 commas_in_line_comment_skipped
    #[test]
    fn commas_in_line_comment_skipped() {
        let src = "a, // , not counted\nb";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }
// TSZ_INLINE_TEST_END a9c1c42e670f1f88489ca6277f9f2da2a4a56ac3429213ad068c42ea04ab4345

// TSZ_INLINE_TEST_BEGIN ab23e29f739532e8255f849591466451bdf705e6a966de636b3f18be8ce6f7cb 490 commas_in_block_comment_skipped
    #[test]
    fn commas_in_block_comment_skipped() {
        let src = "a, /* , not counted */ b";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }
// TSZ_INLINE_TEST_END ab23e29f739532e8255f849591466451bdf705e6a966de636b3f18be8ce6f7cb

// TSZ_INLINE_TEST_BEGIN 4f3fd3d9910d10ab823af3eda07296c2e18c90bd961844ed093ea3175383f822 496 commas_nested_brackets
    #[test]
    fn commas_nested_brackets() {
        assert_eq!(count_top_level_commas("a, [x, y], b", 0, 12), 2);
    }
// TSZ_INLINE_TEST_END 4f3fd3d9910d10ab823af3eda07296c2e18c90bd961844ed093ea3175383f822

// TSZ_INLINE_TEST_BEGIN 0473658b33e9713a8934bc33c1520bedb2ac5082c038bc3dc7bdbb03bf73d683 501 commas_after_less_equal_comparison_are_top_level
    #[test]
    fn commas_after_less_equal_comparison_are_top_level() {
        let src = "a <= b, c";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }
// TSZ_INLINE_TEST_END 0473658b33e9713a8934bc33c1520bedb2ac5082c038bc3dc7bdbb03bf73d683

// TSZ_INLINE_TEST_BEGIN 183cdc7203387cd20d934bd8bf23c5afc83ce5e677f52222e105b3bdb39be329 507 commas_after_spaced_less_than_comparison_are_top_level
    #[test]
    fn commas_after_spaced_less_than_comparison_are_top_level() {
        let src = "a < b, c";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }
// TSZ_INLINE_TEST_END 183cdc7203387cd20d934bd8bf23c5afc83ce5e677f52222e105b3bdb39be329

// TSZ_INLINE_TEST_BEGIN 585ba2d0e1eea5f532fa1441fd6af424b1c5f578e85e5e9b0f74d692cb7e5f22 515 has_comma_yes
    #[test]
    fn has_comma_yes() {
        assert!(has_comma_between_offsets("a, b", 0, 4));
    }
// TSZ_INLINE_TEST_END 585ba2d0e1eea5f532fa1441fd6af424b1c5f578e85e5e9b0f74d692cb7e5f22

// TSZ_INLINE_TEST_BEGIN a887f7550d41f283f1031cec949504153d54f7ad36a05354396f84d4a85f2f6e 520 has_comma_no
    #[test]
    fn has_comma_no() {
        assert!(!has_comma_between_offsets("ab", 0, 2));
    }
// TSZ_INLINE_TEST_END a887f7550d41f283f1031cec949504153d54f7ad36a05354396f84d4a85f2f6e

// TSZ_INLINE_TEST_BEGIN 4c65def2d48400d3c7b545a50c67d63a419aa4a1a30c776fa4146f1ccfe5819d 525 has_comma_in_line_comment_skipped
    #[test]
    fn has_comma_in_line_comment_skipped() {
        let src = "// , ignored\na";
        assert!(!has_comma_between_offsets(src, 0, src.len()));
    }
// TSZ_INLINE_TEST_END 4c65def2d48400d3c7b545a50c67d63a419aa4a1a30c776fa4146f1ccfe5819d

// TSZ_INLINE_TEST_BEGIN ca6ea5187590652a571b59b3e36fe8f6eb0bff3fb5a8d22f23f8197eb2af30a4 531 has_comma_in_block_comment_skipped
    #[test]
    fn has_comma_in_block_comment_skipped() {
        let src = "/* , ignored */ a";
        assert!(!has_comma_between_offsets(src, 0, src.len()));
    }
// TSZ_INLINE_TEST_END ca6ea5187590652a571b59b3e36fe8f6eb0bff3fb5a8d22f23f8197eb2af30a4

// TSZ_INLINE_TEST_BEGIN 5b07fa0d83ba801144f2d67366f0db17666a930166d96dd1ba1b8477dc78c490 537 has_comma_ignores_nesting_by_design
    #[test]
    fn has_comma_ignores_nesting_by_design() {
        // Unlike count_top_level_commas, a comma inside nested brackets is detected.
        // This reflects the coarse semantics: any comma in the range, not just top-level.
        assert!(has_comma_between_offsets("(x, y)", 0, 6));
    }
// TSZ_INLINE_TEST_END 5b07fa0d83ba801144f2d67366f0db17666a930166d96dd1ba1b8477dc78c490

// TSZ_INLINE_TEST_BEGIN 341b30a6157e51235341b2fcdee3d8cf7a4ebc2cb835fdff57934d820c7aa5b0 546 commas_with_division_operator
    #[test]
    fn commas_with_division_operator() {
        // A bare `/` (not `//` or `/*`) must not cause an infinite loop.
        assert_eq!(count_top_level_commas("a / b, c", 0, 8), 1);
    }
// TSZ_INLINE_TEST_END 341b30a6157e51235341b2fcdee3d8cf7a4ebc2cb835fdff57934d820c7aa5b0

// TSZ_INLINE_TEST_BEGIN 3e08cc00ad588f21cef9607f620c0fd6dcb163cb87d716b35c1d68f991f4dcfd 552 has_comma_with_division_operator
    #[test]
    fn has_comma_with_division_operator() {
        assert!(has_comma_between_offsets("a / b, c", 0, 8));
    }
// TSZ_INLINE_TEST_END 3e08cc00ad588f21cef9607f620c0fd6dcb163cb87d716b35c1d68f991f4dcfd
