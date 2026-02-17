use super::decode_source_text;
use super::DecodedSourceText;

#[test]
fn decodes_utf8_bom() {
    let bytes = [0xEF, 0xBB, 0xBF, b'a', b'=', b'1'];
    assert_eq!(
        decode_source_text(&bytes),
        DecodedSourceText::Text("a=1".to_string())
    );
}

#[test]
fn decodes_utf16le_bom_with_unicode() {
    // "µs" in UTF-16LE with BOM
    let bytes = [0xFF, 0xFE, 0xB5, 0x00, 0x73, 0x00];
    assert_eq!(
        decode_source_text(&bytes),
        DecodedSourceText::TextWithOriginalBytes("µs".to_string(), bytes.to_vec())
    );
}

#[test]
fn decodes_utf16be_bom_with_unicode() {
    // "µs" in UTF-16BE with BOM
    let bytes = [0xFE, 0xFF, 0x00, 0xB5, 0x00, 0x73];
    assert_eq!(
        decode_source_text(&bytes),
        DecodedSourceText::TextWithOriginalBytes("µs".to_string(), bytes.to_vec())
    );
}

#[test]
fn non_utf8_bytes_become_binary() {
    let bytes = [0x47, 0x40, 0x04, 0x92];
    assert!(matches!(
        decode_source_text(&bytes),
        DecodedSourceText::Binary(_)
    ));
}

#[test]
fn corrupted_bytes_become_binary() {
    let bytes = [0xC6, 0x1F, 0xBC, 0x03, 0x08, 0x19, 0x1F, 0x00];
    assert!(matches!(
        decode_source_text(&bytes),
        DecodedSourceText::Binary(_)
    ));
}
