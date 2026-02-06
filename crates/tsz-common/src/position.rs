//! Position and location utilities for LSP.
//!
//! LSP uses line/column positions, while our AST uses byte offsets.
//! This module provides conversion utilities.

/// A position in a source file (0-indexed line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Position {
    /// 0-indexed line number
    pub line: u32,
    /// 0-indexed column (UTF-16 code units for LSP compatibility)
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Position { line, character }
    }
}

/// A range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Range { start, end }
    }
}

/// A location in a source file (file path + range).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Location {
    #[serde(rename = "uri")]
    pub file_path: String,
    pub range: Range,
}

impl Location {
    pub fn new(file_path: String, range: Range) -> Self {
        Location { file_path, range }
    }
}

/// Source location with both offset and line/column info.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceLocation {
    /// Byte offset from start of file
    pub offset: u32,
    /// 0-indexed line number
    pub line: u32,
    /// 0-indexed column
    pub character: u32,
}

impl SourceLocation {
    pub fn new(offset: u32, line: u32, character: u32) -> Self {
        SourceLocation {
            offset,
            line,
            character,
        }
    }
}

/// Line map for efficient offset <-> position conversion.
/// Stores the starting offset of each line.
#[derive(Debug, Clone)]
pub struct LineMap {
    /// Starting offset of each line (line_starts[0] is always 0)
    line_starts: Vec<u32>,
}

impl LineMap {
    /// Build a line map from source text.
    pub fn build(source: &str) -> Self {
        let mut line_starts = vec![0u32];

        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                // Next line starts after the newline
                line_starts.push((i + 1) as u32);
            } else if ch == '\r' {
                // Handle \r\n (Windows) and \r (old Mac)
                let next_idx = i + 1;
                if source.as_bytes().get(next_idx) != Some(&b'\n') {
                    // \r not followed by \n - treat as line ending
                    line_starts.push((next_idx) as u32);
                }
                // \r followed by \n - the \n will create the line start
            }
        }

        LineMap { line_starts }
    }

    /// Convert a byte offset to a Position (line, character).
    /// Character is counted in UTF-16 code units for LSP compatibility.
    pub fn offset_to_position(&self, offset: u32, source: &str) -> Position {
        // Binary search for the line containing this offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(insert_point) => insert_point.saturating_sub(1),
        };

        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let clamped_end = (offset as usize).min(source.len());
        let start = (line_start as usize).min(clamped_end);
        let slice = source.get(start..clamped_end).unwrap_or("");
        let character = slice.chars().map(|ch| ch.len_utf16() as u32).sum();

        Position {
            line: line as u32,
            character,
        }
    }

    /// Convert a Position (line, character) to a byte offset.
    pub fn position_to_offset(&self, position: Position, source: &str) -> Option<u32> {
        let line_idx = position.line as usize;
        let line_start = *self.line_starts.get(line_idx)?;
        let line_limit = if line_idx + 1 < self.line_starts.len() {
            self.line_starts[line_idx + 1]
        } else {
            source.len() as u32
        };
        let slice = source
            .get(line_start as usize..line_limit as usize)
            .unwrap_or("");
        let mut utf16_count = 0u32;
        let mut byte_count = 0u32;

        for ch in slice.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            let ch_utf16 = ch.len_utf16() as u32;
            if utf16_count + ch_utf16 > position.character {
                break;
            }
            utf16_count += ch_utf16;
            byte_count += ch.len_utf8() as u32;
            if utf16_count == position.character {
                break;
            }
        }

        Some(line_start + byte_count)
    }

    /// Get the number of lines.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Get the starting offset of a line.
    pub fn line_start(&self, line: usize) -> Option<u32> {
        self.line_starts.get(line).copied()
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;

    #[test]
    fn test_line_map_simple() {
        let source = "line1\nline2\nline3";
        let map = LineMap::build(source);

        assert_eq!(map.line_count(), 3);

        // First character of first line
        assert_eq!(map.offset_to_position(0, source), Position::new(0, 0));
        // Last character of first line
        assert_eq!(map.offset_to_position(4, source), Position::new(0, 4));
        // First character of second line
        assert_eq!(map.offset_to_position(6, source), Position::new(1, 0));
        // First character of third line
        assert_eq!(map.offset_to_position(12, source), Position::new(2, 0));
    }

    #[test]
    fn test_line_map_windows_line_endings() {
        let source = "line1\r\nline2\r\nline3";
        let map = LineMap::build(source);

        assert_eq!(map.line_count(), 3);

        // First character of second line (after \r\n)
        assert_eq!(map.offset_to_position(7, source), Position::new(1, 0));
    }

    #[test]
    fn test_position_to_offset_roundtrip() {
        let source = "const x = 1;\nlet y = 2;\nvar z = 3;";
        let map = LineMap::build(source);

        for offset in 0..source.len() as u32 {
            let pos = map.offset_to_position(offset, source);
            let back = map.position_to_offset(pos, source).unwrap();
            assert_eq!(offset, back, "roundtrip failed for offset {}", offset);
        }
    }

    #[test]
    fn test_utf16_columns() {
        let source = "A 🚀 B";
        let map = LineMap::build(source);

        let pos_rocket = map.offset_to_position(2, source);
        assert_eq!(pos_rocket.character, 2);

        let pos_b = map.offset_to_position(7, source);
        assert_eq!(pos_b.character, 5);

        let offset = map.position_to_offset(Position::new(0, 5), source).unwrap();
        assert_eq!(offset, 7);
    }
}
