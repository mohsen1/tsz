use super::*;

#[test]
fn test_vlq_encode() {
    assert_eq!(vlq::encode(0), "A");
    assert_eq!(vlq::encode(1), "C");
    assert_eq!(vlq::encode(-1), "D");
    assert_eq!(vlq::encode(15), "e");
    assert_eq!(vlq::encode(16), "gB");
    assert_eq!(vlq::encode(-16), "hB");
}

#[test]
fn test_simple_source_map() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let _ = generator.add_source("input.ts".to_string());

    // Add some mappings
    generator.add_simple_mapping(0, 0, 0, 0, 0); // Line 1, col 1
    generator.add_simple_mapping(0, 4, 0, 0, 4); // "var " -> same
    generator.add_simple_mapping(1, 0, 0, 1, 0); // Line 2

    let map = generator.generate();

    assert_eq!(map.version, 3);
    assert_eq!(map.file, "output.js");
    assert_eq!(map.sources, vec!["input.ts"]);
    assert!(!map.mappings.is_empty());
}

#[test]
fn test_inline_source_map() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let _ = generator.add_source("input.ts".to_string());
    generator.add_simple_mapping(0, 0, 0, 0, 0);

    let inline = generator.generate_inline();

    assert!(inline.starts_with("//# sourceMappingURL=data:application/json;base64,"));
}

#[test]
fn test_with_names() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let _ = generator.add_source("input.ts".to_string());

    let name_idx = generator.add_name("myFunction".to_string());
    generator.add_mapping(0, 0, 0, 0, 0, Some(name_idx));

    let map = generator.generate();

    assert_eq!(map.names, vec!["myFunction"]);
}

#[test]
fn test_with_source_content() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let _ = generator.add_source_with_content("input.ts".to_string(), "const x = 1;".to_string());

    let map = generator.generate();

    assert!(map.sources_content.is_some());
    assert_eq!(map.sources_content.unwrap()[0], "const x = 1;");
}

#[test]
fn test_source_root_and_mixed_sources_content() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    generator.set_source_root("../src".to_string());
    let _ = generator.add_source_with_content("input.ts".to_string(), "const x = 1;".to_string());
    let _ = generator.add_source("helper.ts".to_string());
    generator.add_simple_mapping(0, 0, 0, 0, 0);

    let map = generator.generate();

    assert_eq!(map.source_root, "../src");
    assert_eq!(map.sources, vec!["input.ts", "helper.ts"]);
    assert_eq!(
        map.sources_content,
        Some(vec!["const x = 1;".to_string(), String::new()])
    );
}

// =============================================================================
// VLQ encode/decode roundtrip
// =============================================================================

#[test]
fn vlq_decode_basic() {
    assert_eq!(vlq::decode("A"), Some((0, 1)));
    assert_eq!(vlq::decode("C"), Some((1, 1)));
    assert_eq!(vlq::decode("D"), Some((-1, 1)));
    assert_eq!(vlq::decode("gB"), Some((16, 2)));
    assert_eq!(vlq::decode("hB"), Some((-16, 2)));
}

#[test]
fn vlq_roundtrip_values() {
    let test_values = [0, 1, -1, 15, 16, -16, 100, -100, 1000, -1000, 10000, -10000];
    for val in test_values {
        let encoded = vlq::encode(val);
        let (decoded, consumed) =
            vlq::decode(&encoded).unwrap_or_else(|| panic!("decode failed for {val}"));
        assert_eq!(
            decoded, val,
            "roundtrip failed for {val}, encoded={encoded}"
        );
        assert_eq!(consumed, encoded.len(), "consumed mismatch for {val}");
    }
}

#[test]
fn vlq_decode_invalid() {
    assert_eq!(vlq::decode(""), None);
}

#[test]
fn vlq_encode_to_buffer() {
    let mut buf = String::new();
    vlq::encode_to(0, &mut buf);
    assert_eq!(buf, "A");

    buf.clear();
    vlq::encode_to(1, &mut buf);
    assert_eq!(buf, "C");

    buf.clear();
    vlq::encode_to(-1, &mut buf);
    assert_eq!(buf, "D");
}

// =============================================================================
// base64_encode
// =============================================================================

#[test]
fn base64_encode_empty() {
    assert_eq!(base64_encode(b""), "");
}

#[test]
fn base64_encode_hello() {
    assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
}

#[test]
fn base64_encode_padding() {
    // 1 byte -> 2 base64 chars + 2 padding
    assert_eq!(base64_encode(b"M"), "TQ==");
    // 2 bytes -> 3 base64 chars + 1 padding
    assert_eq!(base64_encode(b"Ma"), "TWE=");
    // 3 bytes -> 4 base64 chars, no padding
    assert_eq!(base64_encode(b"Man"), "TWFu");
}

// =============================================================================
// escape_json
// =============================================================================

#[test]
fn escape_json_no_special_chars() {
    let input = "hello world 123";
    assert_eq!(escape_json(input), input);
}

#[test]
fn escape_json_quotes() {
    assert_eq!(escape_json(r#"say "hello""#), r#"say \"hello\""#);
}

#[test]
fn escape_json_backslashes() {
    assert_eq!(escape_json(r#"path\to\file"#), r#"path\\to\\file"#);
}

#[test]
fn escape_json_newlines() {
    assert_eq!(escape_json("line1\nline2"), r#"line1\nline2"#);
    assert_eq!(escape_json("line1\rline2"), r#"line1\rline2"#);
    assert_eq!(escape_json("col1\tcol2"), r#"col1\tcol2"#);
}

#[test]
fn escape_json_mixed_special_chars() {
    // When other escape-triggering chars are present, control chars get escaped too
    let input = "hello\n\x01world";
    let output = escape_json(input);
    assert!(output.contains("\\n"));
    assert!(output.contains("\\u0001"));
}

// =============================================================================
// escape_js_string
// =============================================================================

#[test]
fn escape_js_string_no_special() {
    let input = "hello world";
    assert_eq!(escape_js_string(input, '\''), input);
    assert_eq!(escape_js_string(input, '"'), input);
}

#[test]
fn escape_js_string_single_quotes() {
    assert_eq!(escape_js_string("it's", '\''), r"it\'s");
}

#[test]
fn escape_js_string_double_quotes() {
    assert_eq!(escape_js_string(r#"say "hi""#, '"'), r#"say \"hi\""#);
}

#[test]
fn escape_js_string_backslash() {
    assert_eq!(escape_js_string(r"back\slash", '\''), r"back\\slash");
}

#[test]
fn escape_js_string_newlines() {
    assert_eq!(escape_js_string("a\nb", '\''), r"a\nb");
    assert_eq!(escape_js_string("a\rb", '\''), r"a\rb");
}

// =============================================================================
// SourceMapGenerator edge cases
// =============================================================================

#[test]
fn source_map_multiple_lines() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    // Add mappings across 3 lines
    smg.add_simple_mapping(0, 0, 0, 0, 0);
    smg.add_simple_mapping(1, 0, 0, 1, 0);
    smg.add_simple_mapping(2, 0, 0, 2, 0);

    let map = smg.generate();
    // Should have semicolons separating lines
    assert!(map.mappings.contains(';'));
}

#[test]
fn source_map_multiple_segments_same_line() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    smg.add_simple_mapping(0, 0, 0, 0, 0);
    smg.add_simple_mapping(0, 5, 0, 0, 5);
    smg.add_simple_mapping(0, 10, 0, 0, 10);

    let map = smg.generate();
    // Same line segments separated by commas, no semicolons needed
    assert!(map.mappings.contains(','));
    assert!(!map.mappings.contains(';'));
}

#[test]
fn source_map_json_output() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());
    smg.add_simple_mapping(0, 0, 0, 0, 0);

    let json = smg.generate_json();
    assert!(json.contains("\"version\":3"));
    assert!(json.contains("\"file\":\"out.js\""));
    assert!(json.contains("\"sources\":[\"in.ts\"]"));
}

#[test]
fn source_map_duplicate_name() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let idx1 = smg.add_name("foo".to_string());
    let idx2 = smg.add_name("foo".to_string());
    assert_eq!(idx1, idx2); // Should deduplicate
}

#[test]
fn add_source_assigns_sequential_zero_based_indices() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    assert_eq!(smg.add_source("a.ts".to_string()), 0);
    assert_eq!(smg.add_source("b.ts".to_string()), 1);
    assert_eq!(
        smg.add_source_with_content("c.ts".to_string(), "/* c */".to_string()),
        2
    );
    assert_eq!(smg.add_source("d.ts".to_string()), 3);
}

#[test]
fn add_name_assigns_sequential_indices_and_dedupes_existing() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    assert_eq!(smg.add_name("alpha".to_string()), 0);
    assert_eq!(smg.add_name("beta".to_string()), 1);
    assert_eq!(smg.add_name("gamma".to_string()), 2);
    // Dedupe must return the original index, not the next slot.
    assert_eq!(smg.add_name("beta".to_string()), 1);
    // Adding a fresh name continues from the real length, not from the deduped slot.
    assert_eq!(smg.add_name("delta".to_string()), 3);
}

// =============================================================================
// VLQ encoding overflow (issue #4780)
// =============================================================================
//
// Source Map v3 segments encode each mapping field as a 32-bit signed VLQ
// delta. Mapping fields are stored as u32 in `Mapping`, but values greater
// than i32::MAX cannot be represented faithfully and previously fell back to
// i32::MAX silently — emitting a syntactically valid but wrong mapping. The
// generator now panics on that overflow so corruption is loud rather than
// silent.

#[test]
#[should_panic(expected = "source map generated_column overflowed i32")]
fn source_map_panics_on_generated_column_overflow() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    let too_large: u32 = i32::MAX.unsigned_abs() + 1;
    smg.add_simple_mapping(0, too_large, 0, 0, 0);

    let _ = smg.generate();
}

#[test]
#[should_panic(expected = "source map source_index overflowed i32")]
fn source_map_panics_on_source_index_overflow() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    let too_large: u32 = i32::MAX.unsigned_abs() + 1;
    smg.add_simple_mapping(0, 0, too_large, 0, 0);

    let _ = smg.generate();
}

#[test]
#[should_panic(expected = "source map original_line overflowed i32")]
fn source_map_panics_on_original_line_overflow() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    let too_large: u32 = i32::MAX.unsigned_abs() + 1;
    smg.add_simple_mapping(0, 0, 0, too_large, 0);

    let _ = smg.generate();
}

#[test]
#[should_panic(expected = "source map original_column overflowed i32")]
fn source_map_panics_on_original_column_overflow() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    let too_large: u32 = i32::MAX.unsigned_abs() + 1;
    smg.add_simple_mapping(0, 0, 0, 0, too_large);

    let _ = smg.generate();
}

#[test]
#[should_panic(expected = "source map name_index overflowed i32")]
fn source_map_panics_on_name_index_overflow() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());

    let too_large: u32 = i32::MAX.unsigned_abs() + 1;
    smg.add_mapping(0, 0, 0, 0, 0, Some(too_large));

    let _ = smg.generate();
}

#[test]
fn source_map_shift_generated_lines() {
    let mut smg = SourceMapGenerator::new("out.js".to_string());
    let _ = smg.add_source("in.ts".to_string());
    smg.add_simple_mapping(0, 0, 0, 0, 0);
    smg.add_simple_mapping(1, 0, 0, 1, 0);
    smg.add_simple_mapping(2, 0, 0, 2, 0);

    // Shift lines >= 1 by 2
    smg.shift_generated_lines(1, 2);

    let map = smg.generate();
    // Line 0 mapping stays, lines 1+2 shifted to 3+4
    // So we should see more semicolons (empty lines)
    let semicolons = map.mappings.chars().filter(|&c| c == ';').count();
    assert!(
        semicolons >= 3,
        "expected at least 3 semicolons for shifted lines, got {semicolons}"
    );
}
