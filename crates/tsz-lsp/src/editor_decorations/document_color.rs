//! Document Color provider for LSP.
//!
//! Finds color literals in source code (hex colors in string literals like
//! `"#ff0000"`, `"#rgb"`, `"#rrggbb"`, `"#rrggbbaa"`) and returns them as
//! LSP `ColorInformation` entries so editors can display inline color swatches.

use tsz_common::position::{LineMap, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;

/// A color value in RGBA format (0.0..1.0 per channel).
#[derive(Clone, Debug)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

/// A color found in the document with its source range.
#[derive(Clone, Debug)]
pub struct ColorInformation {
    pub range: Range,
    pub color: Color,
}

/// Provider for document colors.
pub struct DocumentColorProvider<'a> {
    arena: &'a NodeArena,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> DocumentColorProvider<'a> {
    pub const fn new(arena: &'a NodeArena, line_map: &'a LineMap, source_text: &'a str) -> Self {
        Self {
            arena,
            line_map,
            source_text,
        }
    }

    /// Find all color literals in the document.
    pub fn provide_document_colors(&self, _root: NodeIndex) -> Vec<ColorInformation> {
        let mut colors = Vec::new();

        for (i, node) in self.arena.nodes.iter().enumerate() {
            let _node_idx = NodeIndex(i as u32);

            // Only look at string literals
            if node.kind != SyntaxKind::StringLiteral as u16
                && node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                continue;
            }

            let start = node.pos as usize;
            let end = node.end as usize;
            if end <= start || end > self.source_text.len() {
                continue;
            }

            let text = &self.source_text[start..end];

            // Extract string content (strip quotes)
            let content = if text.len() >= 2 {
                &text[1..text.len() - 1]
            } else {
                continue;
            };

            // Find hex color patterns within the string content
            self.find_hex_colors(content, node.pos + 1, &mut colors);
        }

        colors
    }

    /// Scan text for hex color patterns and add them to the results.
    fn find_hex_colors(&self, text: &str, base_offset: u32, colors: &mut Vec<ColorInformation>) {
        let bytes = text.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'#' {
                // Try to parse a hex color starting at this position
                let remaining = &text[i..];
                if let Some((color, len)) = Self::parse_hex_color(remaining) {
                    let color_start = base_offset + i as u32;
                    let color_end = color_start + len as u32;

                    let range = Range::new(
                        self.line_map
                            .offset_to_position(color_start, self.source_text),
                        self.line_map
                            .offset_to_position(color_end, self.source_text),
                    );

                    colors.push(ColorInformation { range, color });
                    i += len;
                    continue;
                }
            }
            i += 1;
        }
    }

    /// Parse a hex color string. Returns (Color, length) if valid.
    fn parse_hex_color(s: &str) -> Option<(Color, usize)> {
        if !s.starts_with('#') {
            return None;
        }

        let hex = &s[1..];
        let hex_chars: Vec<u8> = hex.bytes().take_while(|b| b.is_ascii_hexdigit()).collect();

        match hex_chars.len() {
            // #rgb
            3 => {
                let r = Self::hex_val(hex_chars[0])? * 17;
                let g = Self::hex_val(hex_chars[1])? * 17;
                let b = Self::hex_val(hex_chars[2])? * 17;
                Some((
                    Color {
                        red: r as f64 / 255.0,
                        green: g as f64 / 255.0,
                        blue: b as f64 / 255.0,
                        alpha: 1.0,
                    },
                    4,
                ))
            }
            // #rgba
            4 => {
                let r = Self::hex_val(hex_chars[0])? * 17;
                let g = Self::hex_val(hex_chars[1])? * 17;
                let b = Self::hex_val(hex_chars[2])? * 17;
                let a = Self::hex_val(hex_chars[3])? * 17;
                Some((
                    Color {
                        red: r as f64 / 255.0,
                        green: g as f64 / 255.0,
                        blue: b as f64 / 255.0,
                        alpha: a as f64 / 255.0,
                    },
                    5,
                ))
            }
            // #rrggbb
            6 => {
                let r = Self::hex_byte(hex_chars[0], hex_chars[1])?;
                let g = Self::hex_byte(hex_chars[2], hex_chars[3])?;
                let b = Self::hex_byte(hex_chars[4], hex_chars[5])?;
                Some((
                    Color {
                        red: r as f64 / 255.0,
                        green: g as f64 / 255.0,
                        blue: b as f64 / 255.0,
                        alpha: 1.0,
                    },
                    7,
                ))
            }
            // #rrggbbaa
            8 => {
                let r = Self::hex_byte(hex_chars[0], hex_chars[1])?;
                let g = Self::hex_byte(hex_chars[2], hex_chars[3])?;
                let b = Self::hex_byte(hex_chars[4], hex_chars[5])?;
                let a = Self::hex_byte(hex_chars[6], hex_chars[7])?;
                Some((
                    Color {
                        red: r as f64 / 255.0,
                        green: g as f64 / 255.0,
                        blue: b as f64 / 255.0,
                        alpha: a as f64 / 255.0,
                    },
                    9,
                ))
            }
            _ => None,
        }
    }

    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
        Some(Self::hex_val(hi)? * 16 + Self::hex_val(lo)?)
    }
}
