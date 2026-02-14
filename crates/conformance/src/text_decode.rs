//! Helpers for decoding TypeScript conformance source files.
//!
//! TypeScript test corpora include files encoded as UTF-8, UTF-8 with BOM,
//! and UTF-16 (with BOM). The conformance runner should parse directives from
//! those files instead of skipping them as "non-UTF-8".

/// Decode source text from raw bytes, supporting common BOM-based encodings.
pub fn decode_source_text(bytes: &[u8]) -> Result<String, &'static str> {
    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(&bytes[3..])
            .map(|s| s.to_string())
            .map_err(|_| "invalid UTF-8");
    }

    // UTF-16 LE BOM
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_with_endianness(&bytes[2..], true);
    }

    // UTF-16 BE BOM
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_with_endianness(&bytes[2..], false);
    }

    // Plain UTF-8
    std::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .map_err(|_| "unsupported text encoding")
}

fn decode_utf16_with_endianness(bytes: &[u8], little_endian: bool) -> Result<String, &'static str> {
    if !bytes.len().is_multiple_of(2) {
        return Err("invalid UTF-16 byte length");
    }

    let words = bytes.chunks_exact(2).map(|chunk| {
        if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        }
    });

    std::char::decode_utf16(words)
        .collect::<Result<String, _>>()
        .map_err(|_| "invalid UTF-16")
}

#[cfg(test)]
mod tests {
    use super::decode_source_text;

    #[test]
    fn decodes_utf8_bom() {
        let bytes = [0xEF, 0xBB, 0xBF, b'a', b'=', b'1'];
        assert_eq!(decode_source_text(&bytes).unwrap(), "a=1");
    }

    #[test]
    fn decodes_utf16le_bom_with_unicode() {
        // "µs" in UTF-16LE with BOM
        let bytes = [0xFF, 0xFE, 0xB5, 0x00, 0x73, 0x00];
        assert_eq!(decode_source_text(&bytes).unwrap(), "µs");
    }

    #[test]
    fn decodes_utf16be_bom_with_unicode() {
        // "µs" in UTF-16BE with BOM
        let bytes = [0xFE, 0xFF, 0x00, 0xB5, 0x00, 0x73];
        assert_eq!(decode_source_text(&bytes).unwrap(), "µs");
    }
}
