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
    #[must_use]
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    #[must_use]
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
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
    #[must_use]
    pub fn new(file_path: String, range: Range) -> Self {
        Self { file_path, range }
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
    #[must_use]
    pub fn new(offset: u32, line: u32, character: u32) -> Self {
        Self {
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
    /// Starting offset of each line (`line_starts`[0] is always 0)
    line_starts: Vec<u32>,
}

impl LineMap {
    /// Build a line map from source text.
    #[must_use]
    pub fn build(source: &str) -> Self {
        let mut line_starts = vec![0u32];

        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                // Next line starts after the newline
                line_starts.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
            } else if ch == '\r' {
                // Handle \r\n (Windows) and \r (old Mac)
                let next_idx = i + 1;
                if source.as_bytes().get(next_idx) != Some(&b'\n') {
                    // \r not followed by \n - treat as line ending
                    line_starts.push(u32::try_from(next_idx).unwrap_or(u32::MAX));
                }
                // \r followed by \n - the \n will create the line start
            }
        }

        Self { line_starts }
    }

    /// Convert a byte offset to a Position (line, character).
    /// Character is counted in UTF-16 code units for LSP compatibility.
    #[must_use]
    pub fn offset_to_position(&self, offset: u32, source: &str) -> Position {
        // Binary search for the line containing this offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(insert_point) => insert_point.saturating_sub(1),
        };

        let line_start = usize::try_from(self.line_starts.get(line).copied().unwrap_or(0))
            .unwrap_or(usize::MAX)
            .min(source.len());
        let clamped_end = usize::try_from(offset)
            .unwrap_or(source.len())
            .min(source.len());
        let start = line_start.min(clamped_end);
        let slice = source.get(start..clamped_end).unwrap_or("");
        let character = slice
            .chars()
            .map(|ch| u32::try_from(ch.len_utf16()).unwrap_or(u32::MAX))
            .sum();

        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character,
        }
    }

    /// Convert a Position (line, character) to a byte offset.
    #[must_use]
    pub fn position_to_offset(&self, position: Position, source: &str) -> Option<u32> {
        let line_idx = usize::try_from(position.line).ok()?;
        let line_start = *self.line_starts.get(line_idx)?;
        let line_start = usize::try_from(line_start).ok()?;
        let line_limit = if line_idx + 1 < self.line_starts.len() {
            usize::try_from(self.line_starts[line_idx + 1]).ok()?
        } else {
            source.len()
        };
        let slice = source.get(line_start..line_limit).unwrap_or("");
        let mut utf16_count = 0u32;
        let mut byte_count = 0usize;

        for ch in slice.chars() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            let ch_utf16 = u32::try_from(ch.len_utf16()).ok()?;
            if utf16_count + ch_utf16 > position.character {
                break;
            }
            utf16_count += ch_utf16;
            byte_count += ch.len_utf8();
            if utf16_count == position.character {
                break;
            }
        }

        u32::try_from(line_start + byte_count).ok()
    }

    /// Get the number of lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Get the starting offset of a line.
    #[must_use]
    pub fn line_start(&self, line: usize) -> Option<u32> {
        self.line_starts.get(line).copied()
    }
}

#[cfg(test)]
#[path = "../tests/position_tests.rs"]
mod tests;
