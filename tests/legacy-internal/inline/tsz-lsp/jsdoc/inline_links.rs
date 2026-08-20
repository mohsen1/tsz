//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/jsdoc/inline_links.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d9e6a9f380d6f83caf8d48caa5fa082d0366cdf472cf023204cd87773d485e84 367 parse_simple_link
    #[test]
    fn parse_simple_link() {
        let spans = parse_link_spans("{@link Foo}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].variant, LinkVariant::Link);
        assert_eq!(spans[0].target, "Foo");
        assert!(spans[0].display.is_none());
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 11);
    }
// TSZ_INLINE_TEST_END d9e6a9f380d6f83caf8d48caa5fa082d0366cdf472cf023204cd87773d485e84

// TSZ_INLINE_TEST_BEGIN adcfb9491ea5fcf036fbeaa198b1468f6ae44ac15994452be7a035713a5b6cae 378 parse_link_with_display_text
    #[test]
    fn parse_link_with_display_text() {
        let spans = parse_link_spans("{@link Foo the display}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "Foo");
        assert_eq!(spans[0].display.as_deref(), Some("the display"));
    }
// TSZ_INLINE_TEST_END adcfb9491ea5fcf036fbeaa198b1468f6ae44ac15994452be7a035713a5b6cae

// TSZ_INLINE_TEST_BEGIN ed974c6748333e50c06233d79801aa439e0381bfaf338dafd59b23bb773a2eaf 386 parse_linkcode_variant
    #[test]
    fn parse_linkcode_variant() {
        let spans = parse_link_spans("{@linkcode myFunc}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].variant, LinkVariant::Linkcode);
        assert_eq!(spans[0].target, "myFunc");
    }
// TSZ_INLINE_TEST_END ed974c6748333e50c06233d79801aa439e0381bfaf338dafd59b23bb773a2eaf

// TSZ_INLINE_TEST_BEGIN 94a6f9da81dbaa4fe40bc356040f78505818dd0e72b9694d637267b112968573 394 parse_linkplain_variant
    #[test]
    fn parse_linkplain_variant() {
        let spans = parse_link_spans("{@linkplain SomeType}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].variant, LinkVariant::Linkplain);
        assert_eq!(spans[0].target, "SomeType");
    }
// TSZ_INLINE_TEST_END 94a6f9da81dbaa4fe40bc356040f78505818dd0e72b9694d637267b112968573

// TSZ_INLINE_TEST_BEGIN dc330fbc1c38ad0ac506ee3d63a2acfc07bd8514c160f3a1011008a418217f53 402 parse_url_target
    #[test]
    fn parse_url_target() {
        let spans = parse_link_spans("{@link https://example.com}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "https://example.com");
        assert!(spans[0].is_url());
    }
// TSZ_INLINE_TEST_END dc330fbc1c38ad0ac506ee3d63a2acfc07bd8514c160f3a1011008a418217f53

// TSZ_INLINE_TEST_BEGIN 62b4de079a402c40ca8ef3c9fc1b408f4c471bd7910b3d6ced7e1f6fbac8a60a 410 parse_dotted_symbol_name
    #[test]
    fn parse_dotted_symbol_name() {
        let spans = parse_link_spans("{@link NS.R}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].target, "NS.R");
    }
// TSZ_INLINE_TEST_END 62b4de079a402c40ca8ef3c9fc1b408f4c471bd7910b3d6ced7e1f6fbac8a60a

// TSZ_INLINE_TEST_BEGIN 3bdab8f4e2011a17f85a175465feb54e434f7b913455ccc9429723798cabe6ee 417 parse_multiple_links
    #[test]
    fn parse_multiple_links() {
        let text = "See {@link A} and {@link B Display}.";
        let spans = parse_link_spans(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].target, "A");
        assert_eq!(spans[1].target, "B");
        assert_eq!(spans[1].display.as_deref(), Some("Display"));
    }
// TSZ_INLINE_TEST_END 3bdab8f4e2011a17f85a175465feb54e434f7b913455ccc9429723798cabe6ee

// TSZ_INLINE_TEST_BEGIN abe55ec402e30973262d6a4419b102259a6af1a46747aaa42855a5161c00e27b 427 parse_empty_link_is_skipped
    #[test]
    fn parse_empty_link_is_skipped() {
        let spans = parse_link_spans("{@link}");
        assert!(spans.is_empty(), "empty link should be skipped");
    }
// TSZ_INLINE_TEST_END abe55ec402e30973262d6a4419b102259a6af1a46747aaa42855a5161c00e27b

// TSZ_INLINE_TEST_BEGIN 1e135030bf789d6c638d110fca60ae5fa3062caf97361d954095bbcd3b569a8b 433 parse_unclosed_link_is_skipped
    #[test]
    fn parse_unclosed_link_is_skipped() {
        let spans = parse_link_spans("{@link Foo");
        assert!(spans.is_empty(), "unclosed link should be skipped");
    }
// TSZ_INLINE_TEST_END 1e135030bf789d6c638d110fca60ae5fa3062caf97361d954095bbcd3b569a8b

// TSZ_INLINE_TEST_BEGIN 6327f7c37e30361a8674ada64123038a64b2b0f2a265f0af539fedcbd9808bd7 439 parse_no_links_in_plain_text
    #[test]
    fn parse_no_links_in_plain_text() {
        let spans = parse_link_spans("nothing special here");
        assert!(spans.is_empty());
    }
// TSZ_INLINE_TEST_END 6327f7c37e30361a8674ada64123038a64b2b0f2a265f0af539fedcbd9808bd7

// TSZ_INLINE_TEST_BEGIN ea5968aed4549506dbab3c3a50d6bef8212dfb62a5f8433fe2a168f0d073264e 446 parse_any_identifier_name_works
    // Renamed variable: same structural position, different identifier name.
    #[test]
    fn parse_any_identifier_name_works() {
        for name in &["K", "MyClass", "X", "some_fn", "NS.Method"] {
            let text = format!("{{@link {name}}}");
            let spans = parse_link_spans(&text);
            assert_eq!(spans.len(), 1, "should parse link for identifier {name}");
            assert_eq!(spans[0].target, *name);
        }
    }
// TSZ_INLINE_TEST_END ea5968aed4549506dbab3c3a50d6bef8212dfb62a5f8433fe2a168f0d073264e

// TSZ_INLINE_TEST_BEGIN 232c9d5103d97b9e78ebcbf23e34963ed0d013324080b3b94c12e172da6c9dd8 458 plain_text_simple_link
    #[test]
    fn plain_text_simple_link() {
        assert_eq!(expand_to_plain_text("{@link Foo}"), "Foo");
    }
// TSZ_INLINE_TEST_END 232c9d5103d97b9e78ebcbf23e34963ed0d013324080b3b94c12e172da6c9dd8

// TSZ_INLINE_TEST_BEGIN 5f81b659a9baeb0e5346cb28911c1c90a42c71e63c0d2cb32793eb621118124b 463 plain_text_link_with_display
    #[test]
    fn plain_text_link_with_display() {
        assert_eq!(
            expand_to_plain_text("{@link Foo the display}"),
            "the display"
        );
    }
// TSZ_INLINE_TEST_END 5f81b659a9baeb0e5346cb28911c1c90a42c71e63c0d2cb32793eb621118124b

// TSZ_INLINE_TEST_BEGIN 306dd034f89464e3a3ccc9aa4f68952841f856cf9341ea67446c8a36e0ccc2bf 471 plain_text_in_sentence
    #[test]
    fn plain_text_in_sentence() {
        assert_eq!(
            expand_to_plain_text("Use {@link SomeClass} for details."),
            "Use SomeClass for details."
        );
    }
// TSZ_INLINE_TEST_END 306dd034f89464e3a3ccc9aa4f68952841f856cf9341ea67446c8a36e0ccc2bf

// TSZ_INLINE_TEST_BEGIN 0a17c3dd1a4a56f52301cf1d22d5e9a39a2c6c1d1d4c75720f1147dd61c83cf6 479 plain_text_linkcode
    #[test]
    fn plain_text_linkcode() {
        assert_eq!(expand_to_plain_text("{@linkcode myFunc}"), "myFunc");
    }
// TSZ_INLINE_TEST_END 0a17c3dd1a4a56f52301cf1d22d5e9a39a2c6c1d1d4c75720f1147dd61c83cf6

// TSZ_INLINE_TEST_BEGIN 662bf1ffc6a95eabe8e05eec071aa75570a16d9368f35bca5f0d960495564ab2 484 plain_text_linkplain
    #[test]
    fn plain_text_linkplain() {
        assert_eq!(expand_to_plain_text("{@linkplain SomeType}"), "SomeType");
    }
// TSZ_INLINE_TEST_END 662bf1ffc6a95eabe8e05eec071aa75570a16d9368f35bca5f0d960495564ab2

// TSZ_INLINE_TEST_BEGIN b4fadbd5bc0e6d83e4de26c58f8af00e7efc1d24324c9f2f7f9851d9d6ffb232 489 plain_text_multiple_links
    #[test]
    fn plain_text_multiple_links() {
        assert_eq!(
            expand_to_plain_text("See {@link A} and {@link B DisplayB}."),
            "See A and DisplayB."
        );
    }
// TSZ_INLINE_TEST_END b4fadbd5bc0e6d83e4de26c58f8af00e7efc1d24324c9f2f7f9851d9d6ffb232

// TSZ_INLINE_TEST_BEGIN 332db4982ca58ba3ca59f5a01dd37d930164169ca31b4c9a86019b21cb1ed1f5 497 plain_text_no_links_unchanged
    #[test]
    fn plain_text_no_links_unchanged() {
        let input = "No inline tags here.";
        assert_eq!(expand_to_plain_text(input), input);
    }
// TSZ_INLINE_TEST_END 332db4982ca58ba3ca59f5a01dd37d930164169ca31b4c9a86019b21cb1ed1f5

// TSZ_INLINE_TEST_BEGIN ee69910ea6bf479161954e4ea1f9e39f338497f91ffb2501cd28d8e3c04e8979 503 plain_text_url_link
    #[test]
    fn plain_text_url_link() {
        assert_eq!(
            expand_to_plain_text("{@link https://example.com}"),
            "https://example.com"
        );
    }
// TSZ_INLINE_TEST_END ee69910ea6bf479161954e4ea1f9e39f338497f91ffb2501cd28d8e3c04e8979

// TSZ_INLINE_TEST_BEGIN 0c56d7c7c4b51245346f4bb8f9dd2738e6748c748a1b0a980ff20f48a361f14e 513 markdown_simple_link_becomes_inline_code
    #[test]
    fn markdown_simple_link_becomes_inline_code() {
        let out = expand_to_markdown_escaped("{@link Foo}");
        assert_eq!(out, "`Foo`");
    }
// TSZ_INLINE_TEST_END 0c56d7c7c4b51245346f4bb8f9dd2738e6748c748a1b0a980ff20f48a361f14e

// TSZ_INLINE_TEST_BEGIN b9fc68efe0bb087008c2b49f8bb11807a97cbdca4ccbceb51472a5dfc53efedc 519 markdown_link_in_sentence_escapes_prose
    #[test]
    fn markdown_link_in_sentence_escapes_prose() {
        let out = expand_to_markdown_escaped("Use {@link SomeClass} for details.");
        assert_eq!(out, "Use `SomeClass` for details.");
    }
// TSZ_INLINE_TEST_END b9fc68efe0bb087008c2b49f8bb11807a97cbdca4ccbceb51472a5dfc53efedc

// TSZ_INLINE_TEST_BEGIN eaf7f88cc4c77994ae61daf5c053dbb3af5526777a08d1189eb5abd08e4ac9e4 525 markdown_link_display_text_used
    #[test]
    fn markdown_link_display_text_used() {
        let out = expand_to_markdown_escaped("{@link Foo the label}");
        assert_eq!(out, "`the label`");
    }
// TSZ_INLINE_TEST_END eaf7f88cc4c77994ae61daf5c053dbb3af5526777a08d1189eb5abd08e4ac9e4

// TSZ_INLINE_TEST_BEGIN 991709f99af04325f62277cb02c948842eeaa7eb2beeeaca7a9ae196bb4cfdcc 531 markdown_linkcode_becomes_inline_code
    #[test]
    fn markdown_linkcode_becomes_inline_code() {
        let out = expand_to_markdown_escaped("{@linkcode myFunc}");
        assert_eq!(out, "`myFunc`");
    }
// TSZ_INLINE_TEST_END 991709f99af04325f62277cb02c948842eeaa7eb2beeeaca7a9ae196bb4cfdcc

// TSZ_INLINE_TEST_BEGIN 2ad1b97884389a5a93b803e624648c96dd5568dfe2687299b900d77636ccc0b1 537 markdown_linkplain_becomes_plain
    #[test]
    fn markdown_linkplain_becomes_plain() {
        let out = expand_to_markdown_escaped("{@linkplain SomeType}");
        assert_eq!(out, "SomeType");
    }
// TSZ_INLINE_TEST_END 2ad1b97884389a5a93b803e624648c96dd5568dfe2687299b900d77636ccc0b1

// TSZ_INLINE_TEST_BEGIN 0cd6be733896c914da35b38902af331499dc55533a89b993cb7684b729947f19 543 markdown_url_becomes_hyperlink
    #[test]
    fn markdown_url_becomes_hyperlink() {
        let out = expand_to_markdown_escaped("{@link https://example.com}");
        assert_eq!(out, "[https://example.com](https://example.com)");
    }
// TSZ_INLINE_TEST_END 0cd6be733896c914da35b38902af331499dc55533a89b993cb7684b729947f19

// TSZ_INLINE_TEST_BEGIN 16f246d7c336f63a4d37151259b2e08d49bec9d9915979efd408f63653195564 549 markdown_url_with_display_text
    #[test]
    fn markdown_url_with_display_text() {
        let out = expand_to_markdown_escaped("{@link https://example.com Click here}");
        assert_eq!(out, "[Click here](https://example.com)");
    }
// TSZ_INLINE_TEST_END 16f246d7c336f63a4d37151259b2e08d49bec9d9915979efd408f63653195564

// TSZ_INLINE_TEST_BEGIN 0e89796b7cc45a813df25ab7a475fbf7559b24e7307d75ae34b1ca7d0f349362 555 markdown_prose_with_special_chars_is_escaped
    #[test]
    fn markdown_prose_with_special_chars_is_escaped() {
        let out = expand_to_markdown_escaped("[brackets] before {@link Foo}.");
        assert!(out.contains("\\[brackets\\]"), "got: {out}");
        assert!(out.contains("`Foo`"), "got: {out}");
    }
// TSZ_INLINE_TEST_END 0e89796b7cc45a813df25ab7a475fbf7559b24e7307d75ae34b1ca7d0f349362

// TSZ_INLINE_TEST_BEGIN ef32c07faab497b85a6b7fd1215f3779041e04e688a644c0d3b021f0167cd9e0 562 markdown_no_links_still_escapes
    #[test]
    fn markdown_no_links_still_escapes() {
        let out = expand_to_markdown_escaped("see [here](there)");
        assert!(out.contains("\\[here\\]"), "got: {out}");
    }
// TSZ_INLINE_TEST_END ef32c07faab497b85a6b7fd1215f3779041e04e688a644c0d3b021f0167cd9e0

// TSZ_INLINE_TEST_BEGIN 6b1130917589db034ef6c59f08f0cd620cd886fee50e14e0f33cbbcb5f624688 568 markdown_with_resolver
    #[test]
    fn markdown_with_resolver() {
        let out = expand_to_markdown_with_resolver("{@link Foo}", |name| {
            if name == "Foo" {
                Some("file:///test.ts#L1".to_string())
            } else {
                None
            }
        });
        assert_eq!(out, "[Foo](file:///test.ts#L1)");
    }
// TSZ_INLINE_TEST_END 6b1130917589db034ef6c59f08f0cd620cd886fee50e14e0f33cbbcb5f624688

// TSZ_INLINE_TEST_BEGIN 2a0691f9250a41662986671fe74998d6ef09bd840efbfb0d994ab0c9f7c64701 580 markdown_linkcode_with_resolver_preserves_code_voice
    #[test]
    fn markdown_linkcode_with_resolver_preserves_code_voice() {
        let out = expand_to_markdown_with_resolver("{@linkcode Foo}", |name| {
            (name == "Foo").then(|| "file:///test.ts#L1".to_string())
        });
        assert_eq!(out, "[`Foo`](file:///test.ts#L1)");
    }
// TSZ_INLINE_TEST_END 2a0691f9250a41662986671fe74998d6ef09bd840efbfb0d994ab0c9f7c64701

// TSZ_INLINE_TEST_BEGIN bce08f900b6db026834b6fb601bec8e90e58343473d724661f0486834d98455f 588 markdown_linkplain_with_resolver_links_plain_label
    #[test]
    fn markdown_linkplain_with_resolver_links_plain_label() {
        let out = expand_to_markdown_with_resolver("{@linkplain Foo}", |name| {
            (name == "Foo").then(|| "file:///test.ts#L1".to_string())
        });
        assert_eq!(out, "[Foo](file:///test.ts#L1)");
    }
// TSZ_INLINE_TEST_END bce08f900b6db026834b6fb601bec8e90e58343473d724661f0486834d98455f

// TSZ_INLINE_TEST_BEGIN 5f75f1e30f589a851316fcf88f646fd29f9afb9a0c1976102619e9dd62febc16 598 display_parts_simple_link
    #[test]
    fn display_parts_simple_link() {
        // {open-link, linkName(Foo), close-link} = 3 parts
        let parts = build_doc_display_parts("{@link Foo}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@link ");
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "Foo");
        assert_eq!(arr[2]["kind"], KIND_LINK);
        assert_eq!(arr[2]["text"], "}");
    }
// TSZ_INLINE_TEST_END 5f75f1e30f589a851316fcf88f646fd29f9afb9a0c1976102619e9dd62febc16

// TSZ_INLINE_TEST_BEGIN c35baccd801fa125899eb0ea53119f1da22c83fed9507e3f20700c04a20bef8f 612 display_parts_in_sentence
    #[test]
    fn display_parts_in_sentence() {
        // text("Use "), link("{@link "), linkName("Foo"), link("}"), text(" for details.")
        let parts = build_doc_display_parts("Use {@link Foo} for details.");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 5);
        assert_eq!(arr[0]["kind"], KIND_TEXT);
        assert_eq!(arr[0]["text"], "Use ");
        assert_eq!(arr[1]["kind"], KIND_LINK);
        assert_eq!(arr[1]["text"], "{@link ");
        assert_eq!(arr[2]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[2]["text"], "Foo");
        assert_eq!(arr[3]["kind"], KIND_LINK);
        assert_eq!(arr[3]["text"], "}");
        assert_eq!(arr[4]["kind"], KIND_TEXT);
        assert_eq!(arr[4]["text"], " for details.");
    }
// TSZ_INLINE_TEST_END c35baccd801fa125899eb0ea53119f1da22c83fed9507e3f20700c04a20bef8f

// TSZ_INLINE_TEST_BEGIN 705a22170e3ea4c049ba577220c47414b8473ee119560cff8bf0c6e87146ada3 630 display_parts_no_links_single_text_part
    #[test]
    fn display_parts_no_links_single_text_part() {
        let parts = build_doc_display_parts("plain text");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"], KIND_TEXT);
        assert_eq!(arr[0]["text"], "plain text");
    }
// TSZ_INLINE_TEST_END 705a22170e3ea4c049ba577220c47414b8473ee119560cff8bf0c6e87146ada3

// TSZ_INLINE_TEST_BEGIN 31a33c8a89c7284c9c85553cdb4a852db6e85536be7a789328a96b138115d8b0 639 display_parts_linkcode_tag_name
    #[test]
    fn display_parts_linkcode_tag_name() {
        // Opening link part contains the full tag prefix.
        let parts = build_doc_display_parts("{@linkcode myFunc}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@linkcode ");
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "myFunc");
    }
// TSZ_INLINE_TEST_END 31a33c8a89c7284c9c85553cdb4a852db6e85536be7a789328a96b138115d8b0

// TSZ_INLINE_TEST_BEGIN 559664b589b1fac52e41f5f6e0809f417aff58075ce0585ff0a73e63451798fc 650 display_parts_link_with_display_text
    #[test]
    fn display_parts_link_with_display_text() {
        // {open-link, linkName(Foo), linkText(the label), close-link} = 4 parts
        let parts = build_doc_display_parts("{@link Foo the label}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@link ");
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "Foo");
        assert_eq!(arr[2]["kind"], KIND_LINK_TEXT);
        assert_eq!(arr[2]["text"], "the label");
        assert_eq!(arr[3]["kind"], KIND_LINK);
        assert_eq!(arr[3]["text"], "}");
    }
// TSZ_INLINE_TEST_END 559664b589b1fac52e41f5f6e0809f417aff58075ce0585ff0a73e63451798fc

// TSZ_INLINE_TEST_BEGIN fcf6b2f4d0aee7ed4276d512105d13448cbc65f161d0cebd1c4ff998060646b7 666 display_parts_url_with_display_text
    #[test]
    fn display_parts_url_with_display_text() {
        // URL: full "https://... Click here" as a single linkText part.
        let parts = build_doc_display_parts("{@link https://example.com Click here}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["kind"], KIND_LINK);
        assert_eq!(arr[0]["text"], "{@link ");
        assert_eq!(arr[1]["kind"], KIND_LINK_TEXT);
        assert_eq!(arr[1]["text"], "https://example.com Click here");
        assert_eq!(arr[2]["kind"], KIND_LINK);
        assert_eq!(arr[2]["text"], "}");
    }
// TSZ_INLINE_TEST_END fcf6b2f4d0aee7ed4276d512105d13448cbc65f161d0cebd1c4ff998060646b7

// TSZ_INLINE_TEST_BEGIN 36e7301730ef119da70b3b91743cd11e15ec0055c77a01bef9525b3c9f3a3bb6 680 display_parts_url_no_display
    #[test]
    fn display_parts_url_no_display() {
        let parts = build_doc_display_parts("{@link https://example.com}");
        let arr = parts.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[1]["kind"], KIND_LINK_TEXT);
        assert_eq!(arr[1]["text"], "https://example.com");
    }
// TSZ_INLINE_TEST_END 36e7301730ef119da70b3b91743cd11e15ec0055c77a01bef9525b3c9f3a3bb6

// TSZ_INLINE_TEST_BEGIN 7343517e31e889d4f36e3af760afaacdf408b02f297b6705fb339997479a20ac 689 display_parts_resolver_sets_target_on_link_name
    #[test]
    fn display_parts_resolver_sets_target_on_link_name() {
        let parts = build_doc_display_parts_with_resolver("{@link Foo}", |name| {
            if name == "Foo" {
                Some(serde_json::json!({"fileName": "test.ts", "textSpan": {"start": 0}}))
            } else {
                None
            }
        });
        let arr = parts.as_array().unwrap();
        // 3 parts: open-link, linkName(Foo)+target, close-link
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[1]["kind"], KIND_LINK_NAME);
        assert_eq!(arr[1]["text"], "Foo");
        assert!(
            arr[1].get("target").is_some(),
            "linkName should have target field"
        );
    }
// TSZ_INLINE_TEST_END 7343517e31e889d4f36e3af760afaacdf408b02f297b6705fb339997479a20ac
