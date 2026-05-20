//! Helpers for decoding TypeScript conformance source files.
//!
//! TypeScript test corpora include files encoded as UTF-8, UTF-8 with BOM,
//! and UTF-16 (with BOM). The conformance runner should parse directives from
//! those files instead of skipping them as "non-UTF-8".

/// Decode source text from raw bytes, supporting common BOM-based encodings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedSourceText {
    Text(String),
    /// Text decoded from a non-UTF-8 encoding (e.g. UTF-16 with BOM).
    /// The decoded text is available for directive parsing, but the original
    /// bytes should be written to disk so the compiler can detect the encoding.
    TextWithOriginalBytes(String, Vec<u8>),
    Binary(Vec<u8>),
}

/// Decode source text from raw bytes.
///
/// - UTF-16 with BOM decodes to text.
/// - UTF-8 (with or without BOM) decodes to text.
/// - Invalid UTF-8 without BOM is treated as binary and passed through unchanged.
pub fn decode_source_text(bytes: &[u8]) -> DecodedSourceText {
    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return match std::str::from_utf8(&bytes[3..]) {
            Ok(s) => DecodedSourceText::Text(s.to_string()),
            Err(_) => DecodedSourceText::Binary(bytes.to_vec()),
        };
    }

    // UTF-16 LE BOM
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_with_endianness(&bytes[2..], true).map_or_else(
            |_| DecodedSourceText::Binary(bytes.to_vec()),
            |text| DecodedSourceText::TextWithOriginalBytes(text, bytes.to_vec()),
        );
    }

    // UTF-16 BE BOM
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_with_endianness(&bytes[2..], false).map_or_else(
            |_| DecodedSourceText::Binary(bytes.to_vec()),
            |text| DecodedSourceText::TextWithOriginalBytes(text, bytes.to_vec()),
        );
    }

    // Plain UTF-8
    match std::str::from_utf8(bytes) {
        Ok(s) => DecodedSourceText::Text(s.to_string()),
        Err(_) => DecodedSourceText::Binary(bytes.to_vec()),
    }
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
#[path = "../tests/text_decode_tests.rs"]
mod tests;
