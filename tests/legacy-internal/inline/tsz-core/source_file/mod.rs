//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/source_file/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3506ea3dd2133f9d792c0626740b5f1232bcad4d722719728d41fb013e63d191 465 test_source_file_basic
    #[test]
    fn test_source_file_basic() {
        let source = SourceFile::new("test.ts", "const x = 42;");
        assert_eq!(source.file_name(), "test.ts");
        assert_eq!(source.text(), "const x = 42;");
        assert_eq!(source.len(), 13);
        assert!(!source.is_empty());
    }
// TSZ_INLINE_TEST_END 3506ea3dd2133f9d792c0626740b5f1232bcad4d722719728d41fb013e63d191

// TSZ_INLINE_TEST_BEGIN bd5fe575655e6f55fe9ade54990ccc5e85e3a4c97ea78e24dd88895c6d2ea056 474 test_source_file_empty
    #[test]
    fn test_source_file_empty() {
        let source = SourceFile::new("empty.ts", "");
        assert!(source.is_empty());
        assert_eq!(source.len(), 0);
    }
// TSZ_INLINE_TEST_END bd5fe575655e6f55fe9ade54990ccc5e85e3a4c97ea78e24dd88895c6d2ea056

// TSZ_INLINE_TEST_BEGIN 224fb8c30554b17f419344b5732a62067296492ef7c22950f3a6671b93301efe 481 test_source_file_char_at
    #[test]
    fn test_source_file_char_at() {
        let source = SourceFile::new("test.ts", "hello");
        assert_eq!(source.char_at(0), Some('h'));
        assert_eq!(source.char_at(4), Some('o'));
        assert_eq!(source.char_at(5), None);
    }
// TSZ_INLINE_TEST_END 224fb8c30554b17f419344b5732a62067296492ef7c22950f3a6671b93301efe

// TSZ_INLINE_TEST_BEGIN 90c706d0307644c6242017865c1f13e5464730def7d84a4ada45118fce60648b 489 test_source_file_char_at_rejects_non_char_boundary
    #[test]
    fn test_source_file_char_at_rejects_non_char_boundary() {
        let source = SourceFile::new("unicode.ts", "🚀!");
        assert_eq!(source.char_at(1), None);
        assert_eq!(source.char_at(4), Some('!'));
    }
// TSZ_INLINE_TEST_END 90c706d0307644c6242017865c1f13e5464730def7d84a4ada45118fce60648b

// TSZ_INLINE_TEST_BEGIN 57036d8205a7eb3b25460b62975b7780063292a2e63334f529767f0f7040069f 496 test_source_file_byte_at
    #[test]
    fn test_source_file_byte_at() {
        let source = SourceFile::new("test.ts", "hello");
        assert_eq!(source.byte_at(0), Some(b'h'));
        assert_eq!(source.byte_at(4), Some(b'o'));
        assert_eq!(source.byte_at(5), None);
    }
// TSZ_INLINE_TEST_END 57036d8205a7eb3b25460b62975b7780063292a2e63334f529767f0f7040069f

// TSZ_INLINE_TEST_BEGIN 8bd7c9ed4fc2876b4b2812820e3e6cdb366651bd0607adce80153b13e7f2c893 504 test_source_file_slice
    #[test]
    fn test_source_file_slice() {
        let source = SourceFile::new("test.ts", "hello world");
        let span = Span::new(0, 5);
        assert_eq!(source.slice(span), "hello");

        let span2 = Span::new(6, 11);
        assert_eq!(source.slice(span2), "world");
    }
// TSZ_INLINE_TEST_END 8bd7c9ed4fc2876b4b2812820e3e6cdb366651bd0607adce80153b13e7f2c893

// TSZ_INLINE_TEST_BEGIN dd0854e9b6bacbce36bfbe4d54f5be41704cbb2674daec6abef5175f234448cb 514 test_source_file_slice_safe
    #[test]
    fn test_source_file_slice_safe() {
        let source = SourceFile::new("test.ts", "hello");
        let span = Span::new(0, 100); // Out of bounds
        assert_eq!(source.slice(span), "hello");
    }
// TSZ_INLINE_TEST_END dd0854e9b6bacbce36bfbe4d54f5be41704cbb2674daec6abef5175f234448cb

// TSZ_INLINE_TEST_BEGIN b65e94149467b77634b1b4dc18b0f085213b0177a6241c1153e6d3df60d7f82e 521 test_source_file_slice_range_from_and_to_handle_invalid_bounds
    #[test]
    fn test_source_file_slice_range_from_and_to_handle_invalid_bounds() {
        let source = SourceFile::new("test.ts", "hello");
        assert_eq!(source.slice_range(4, 2), "");
        assert_eq!(source.slice_from(99), "");
        assert_eq!(source.slice_to(99), "hello");
    }
// TSZ_INLINE_TEST_END b65e94149467b77634b1b4dc18b0f085213b0177a6241c1153e6d3df60d7f82e

// TSZ_INLINE_TEST_BEGIN e93fb26908621f21645468e0cf44a56033eff75dc4ea8b4a069e98e9c8bfb462 529 test_source_file_lines
    #[test]
    fn test_source_file_lines() {
        let mut source = SourceFile::new("test.ts", "line1\nline2\nline3");

        assert_eq!(source.line_count(), 3);
        assert_eq!(source.line_text(0), Some("line1"));
        assert_eq!(source.line_text(1), Some("line2"));
        assert_eq!(source.line_text(2), Some("line3"));
        assert_eq!(source.line_text(3), None);
    }
// TSZ_INLINE_TEST_END e93fb26908621f21645468e0cf44a56033eff75dc4ea8b4a069e98e9c8bfb462

// TSZ_INLINE_TEST_BEGIN 16d6ab325ace8bd3bfdb92185c033789f64b6fb9b2c63c0cc0c5fc6e1021f453 540 test_source_file_line_text_strips_crlf_and_cr
    #[test]
    fn test_source_file_line_text_strips_crlf_and_cr() {
        let mut source = SourceFile::new("test.ts", "line1\r\nline2\rline3");
        assert_eq!(source.line_count(), 3);
        assert_eq!(source.line_text(0), Some("line1"));
        assert_eq!(source.line_text(1), Some("line2"));
        assert_eq!(source.line_text(2), Some("line3"));
    }
// TSZ_INLINE_TEST_END 16d6ab325ace8bd3bfdb92185c033789f64b6fb9b2c63c0cc0c5fc6e1021f453

// TSZ_INLINE_TEST_BEGIN c9376b6d20b938ffa53447b7a235201a60dabf1b8c587805487e40d00765d636 549 test_source_file_position_conversion
    #[test]
    fn test_source_file_position_conversion() {
        let mut source = SourceFile::new("test.ts", "const x = 1;\nlet y = 2;");

        let pos = source.offset_to_position(0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        let pos = source.offset_to_position(13); // Start of second line
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // Roundtrip
        let offset = source.position_to_offset(Position::new(1, 4)).unwrap();
        assert_eq!(offset, 17); // "y" in "let y"
    }
// TSZ_INLINE_TEST_END c9376b6d20b938ffa53447b7a235201a60dabf1b8c587805487e40d00765d636

// TSZ_INLINE_TEST_BEGIN b5dd4521ca54b991746b8548b56035869204d64d9d3b28d34e7e83ad288e6c8b 566 test_source_file_span_to_range
    #[test]
    fn test_source_file_span_to_range() {
        let mut source = SourceFile::new("test.ts", "const x = 1;");
        let span = Span::new(6, 7); // "x"
        let range = source.span_to_range(span);

        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 7);
    }
// TSZ_INLINE_TEST_END b5dd4521ca54b991746b8548b56035869204d64d9d3b28d34e7e83ad288e6c8b

// TSZ_INLINE_TEST_BEGIN dd94e2aef657d270bbffc3d9c0c6542567163040b09156cc4b345928dab500bb 578 test_source_file_range_to_span_handles_invalid_and_clamped_positions
    #[test]
    fn test_source_file_range_to_span_handles_invalid_and_clamped_positions() {
        let mut source = SourceFile::new("test.ts", "abc\nxyz");

        assert_eq!(
            source.range_to_span(Range::new(Position::new(9, 0), Position::new(9, 1))),
            None
        );

        let span = source
            .range_to_span(Range::new(Position::new(0, 99), Position::new(1, 99)))
            .unwrap();
        assert_eq!(span, Span::new(3, 7));
    }
// TSZ_INLINE_TEST_END dd94e2aef657d270bbffc3d9c0c6542567163040b09156cc4b345928dab500bb

// TSZ_INLINE_TEST_BEGIN 1109cd6fbfb6be25c424d5144c01d1aea60a8624db7f0735a7b4293f614aea79 593 test_source_file_with_line_map
    #[test]
    fn test_source_file_with_line_map() {
        let source = SourceFile::with_line_map("test.ts", "a\nb\nc");
        assert!(source.line_map.is_built());
    }
// TSZ_INLINE_TEST_END 1109cd6fbfb6be25c424d5144c01d1aea60a8624db7f0735a7b4293f614aea79

// TSZ_INLINE_TEST_BEGIN 25c45ca401a1e37b0c220d64aeea27182c5c3340e23bccc018db536ed420e48e 599 test_source_file_new_does_not_pre_build_line_map
    #[test]
    fn test_source_file_new_does_not_pre_build_line_map() {
        let source = SourceFile::new("test.ts", "a\nb\nc");
        assert!(!source.line_map.is_built());
    }
// TSZ_INLINE_TEST_END 25c45ca401a1e37b0c220d64aeea27182c5c3340e23bccc018db536ed420e48e

// TSZ_INLINE_TEST_BEGIN d6d499cc1acf013dacf83f72f4fc5ee6f82ced9978bf187f5d39d5cc61269f86 605 test_source_file_line_map_access_builds_cache_once
    #[test]
    fn test_source_file_line_map_access_builds_cache_once() {
        let mut source = SourceFile::new("test.ts", "a\nb\nc");
        assert!(!source.line_map.is_built());
        let line_count_first = source.line_count();
        assert!(source.line_map.is_built());
        // Subsequent accesses reuse the built cache and return the same answer.
        let line_count_second = source.line_count();
        assert_eq!(line_count_first, line_count_second);
        assert!(source.line_map.is_built());
    }
// TSZ_INLINE_TEST_END d6d499cc1acf013dacf83f72f4fc5ee6f82ced9978bf187f5d39d5cc61269f86

// TSZ_INLINE_TEST_BEGIN 8be3059c8e12047e3cd0f2c159d8788dce293067ec2ce267f01c0cce12425c18 617 test_line_map_cache_default_starts_unbuilt
    #[test]
    fn test_line_map_cache_default_starts_unbuilt() {
        let cache = LineMapCache::default();
        assert!(!cache.is_built());
    }
// TSZ_INLINE_TEST_END 8be3059c8e12047e3cd0f2c159d8788dce293067ec2ce267f01c0cce12425c18

// TSZ_INLINE_TEST_BEGIN 48631aa14bf7e5ca77ccae564ecf8ed70603132f07efab64a2b7b45e422d7948 623 test_line_map_cache_built_constructor_is_built
    #[test]
    fn test_line_map_cache_built_constructor_is_built() {
        let cache = LineMapCache::built("a\nb");
        assert!(cache.is_built());
    }
// TSZ_INLINE_TEST_END 48631aa14bf7e5ca77ccae564ecf8ed70603132f07efab64a2b7b45e422d7948

// TSZ_INLINE_TEST_BEGIN ebbe06f84baf0ee2badb463ed82d1e9e4246352704777744ab78998c21b4b34f 629 test_line_map_cache_ensure_is_idempotent
    #[test]
    fn test_line_map_cache_ensure_is_idempotent() {
        let mut cache = LineMapCache::default();
        let line_count = cache.ensure("a\nb\nc").line_count();
        assert_eq!(line_count, 3);
        assert_eq!(cache.ensure("a\nb\nc").line_count(), line_count);
        assert!(cache.is_built());
    }
// TSZ_INLINE_TEST_END ebbe06f84baf0ee2badb463ed82d1e9e4246352704777744ab78998c21b4b34f

// TSZ_INLINE_TEST_BEGIN 809d629c6da945c02789f8188a1d61d13043aa8b5c971732b67ba3755cb1d955 638 test_line_map_cache_ensure_uses_first_text_when_called_again
    #[test]
    fn test_line_map_cache_ensure_uses_first_text_when_called_again() {
        // Once built, `ensure` returns the cached map regardless of the text
        // argument. The caller must pair the cache with a single text source.
        let mut cache = LineMapCache::default();
        let first = cache.ensure("a\nb").line_count();
        let second = cache
            .ensure("totally different text without newlines")
            .line_count();
        assert_eq!(first, second);
    }
// TSZ_INLINE_TEST_END 809d629c6da945c02789f8188a1d61d13043aa8b5c971732b67ba3755cb1d955

// TSZ_INLINE_TEST_BEGIN 157bc887acd4fdcc2ac68f6b107154747c449959ef6e49c0ff2053f532ea91a2 650 test_source_file_length_conversion_panics_for_overflow
    #[test]
    fn test_source_file_length_conversion_panics_for_overflow() {
        let overflow_len = u32::MAX as usize + 1;
        assert!(overflow_len > u32::MAX as usize);
        let panic = std::panic::catch_unwind(|| source_file_len_bytes_as_u32(overflow_len))
            .expect_err("oversized source file length must panic before truncating");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("panic payload should be a string");

        assert!(
            message.contains(SOURCE_FILE_LEN_OVERFLOW_MESSAGE),
            "panic message `{message}` should contain `{SOURCE_FILE_LEN_OVERFLOW_MESSAGE}`"
        );
    }
// TSZ_INLINE_TEST_END 157bc887acd4fdcc2ac68f6b107154747c449959ef6e49c0ff2053f532ea91a2

// TSZ_INLINE_TEST_BEGIN b3940fe8457493a252a8bfc830e8e437c41cc74e76e05164507ba6af11317170 668 test_source_file_length_conversion_accepts_u32_max
    #[test]
    fn test_source_file_length_conversion_accepts_u32_max() {
        assert_eq!(source_file_len_bytes_as_u32(u32::MAX as usize), u32::MAX);
    }
// TSZ_INLINE_TEST_END b3940fe8457493a252a8bfc830e8e437c41cc74e76e05164507ba6af11317170

// TSZ_INLINE_TEST_BEGIN 67bb23a4d9381aa2cbaca14df19ce9d07e79f45d5c08ad0c1ec823707b79b8a2 673 test_source_file_ref
    #[test]
    fn test_source_file_ref() {
        let source = SourceFile::new("test.ts", "hello world");
        let source_ref = SourceFileRef::from_source_file(&source);

        assert_eq!(source_ref.file_name, "test.ts");
        assert_eq!(source_ref.text, "hello world");
        assert_eq!(source_ref.len(), 11);
    }
// TSZ_INLINE_TEST_END 67bb23a4d9381aa2cbaca14df19ce9d07e79f45d5c08ad0c1ec823707b79b8a2

// TSZ_INLINE_TEST_BEGIN 484cbed3e905f9ccfb842eb843da092944e55da822b5cbad573951f6f2c85404 683 test_source_file_ref_new_slice_and_empty
    #[test]
    fn test_source_file_ref_new_slice_and_empty() {
        let source_ref = SourceFileRef::new("empty.ts", "");
        assert!(source_ref.is_empty());
        assert_eq!(source_ref.slice(Span::new(0, 1)), "");

        let populated = SourceFileRef::new("test.ts", "abcdef");
        assert_eq!(populated.slice(Span::new(1, 4)), "bcd");
    }
// TSZ_INLINE_TEST_END 484cbed3e905f9ccfb842eb843da092944e55da822b5cbad573951f6f2c85404

// TSZ_INLINE_TEST_BEGIN 03b7e0164097e90f7cea946c330824ca2aff2194f99e01561e2e6a59f65e5cd1 693 test_source_id
    #[test]
    fn test_source_id() {
        let id = SourceId::new(42);
        assert_eq!(id.0, 42);
        assert!(!id.is_unknown());

        assert!(SourceId::UNKNOWN.is_unknown());
    }
// TSZ_INLINE_TEST_END 03b7e0164097e90f7cea946c330824ca2aff2194f99e01561e2e6a59f65e5cd1

// TSZ_INLINE_TEST_BEGIN 534dbd4dee23002a9315948b3097365a92c72f684ec8dfffcf9e6aee29db6446 702 test_source_location
    #[test]
    fn test_source_location() {
        let mut source = SourceFile::new("test.ts", "const x = 42;");
        let span = Span::new(6, 7); // "x"
        let location = SourceLocation::from_span(&mut source, span);

        assert_eq!(location.file_name, "test.ts");
        assert_eq!(location.start_line, 0);
        assert_eq!(location.start_column, 6);
        assert_eq!(location.to_string_short(), "test.ts:1:7");
        assert_eq!(location.to_string_visual_studio(), "test.ts(1,7)");
    }
// TSZ_INLINE_TEST_END 534dbd4dee23002a9315948b3097365a92c72f684ec8dfffcf9e6aee29db6446

// TSZ_INLINE_TEST_BEGIN 31158093b21f2dae98259c82d29ca3554306f675f67887b99457b458df3550c7 715 test_source_location_display_uses_short_format
    #[test]
    fn test_source_location_display_uses_short_format() {
        let location = SourceLocation::new("test.ts".to_string(), Span::new(0, 1), 1, 2, 1, 3);
        assert_eq!(format!("{location}"), "test.ts:2:3");
    }
// TSZ_INLINE_TEST_END 31158093b21f2dae98259c82d29ca3554306f675f67887b99457b458df3550c7

// TSZ_INLINE_TEST_BEGIN 1e7dbf15ed5d97b60ca4a8052ec6f0538848b8a118d6bac0aba88cc72f3b3802 721 test_source_file_into_parts
    #[test]
    fn test_source_file_into_parts() {
        let source = SourceFile::new("test.ts", "content");
        let (name, text) = source.into_parts();
        assert_eq!(name, "test.ts");
        assert_eq!(text.as_ref(), "content");
    }
// TSZ_INLINE_TEST_END 1e7dbf15ed5d97b60ca4a8052ec6f0538848b8a118d6bac0aba88cc72f3b3802

// TSZ_INLINE_TEST_BEGIN b1f5c44d6bff6d04f77cf9369f754b1e9345cbd5e58cdcb0b97600bc05b6fb7d 729 test_source_file_deref
    #[test]
    fn test_source_file_deref() {
        let source = SourceFile::new("test.ts", "hello");
        // Can use &str methods directly via Deref
        assert!(source.starts_with("hel"));
        assert_eq!(&*source, "hello");
    }
// TSZ_INLINE_TEST_END b1f5c44d6bff6d04f77cf9369f754b1e9345cbd5e58cdcb0b97600bc05b6fb7d
