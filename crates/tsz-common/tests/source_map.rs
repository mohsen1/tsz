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
