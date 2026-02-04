//! Shared test utilities for source map tests
//!
//! This module contains helper functions used across multiple source map test files.

use crate::source_map::vlq;

#[derive(Debug)]
pub struct DecodedMapping {
    pub generated_line: u32,
    pub generated_column: u32,
    pub source_index: u32,
    pub original_line: u32,
    pub original_column: u32,
    #[allow(dead_code)]
    pub name_index: Option<u32>,
}

pub fn decode_mappings(mappings: &str) -> Vec<DecodedMapping> {
    let mut decoded = Vec::new();
    let mut generated_line = 0u32;
    let mut prev_generated_column = 0i32;
    let mut prev_source_index = 0i32;
    let mut prev_original_line = 0i32;
    let mut prev_original_column = 0i32;
    let mut prev_name_index = 0i32;

    for line in mappings.split(';') {
        if line.is_empty() {
            generated_line += 1;
            prev_generated_column = 0;
            continue;
        }

        for segment in line.split(',') {
            if segment.is_empty() {
                continue;
            }

            let mut rest = segment;
            let (gen_col_delta, consumed) = vlq::decode(rest).expect("decode generated column");
            rest = &rest[consumed..];

            let gen_col = prev_generated_column + gen_col_delta;
            prev_generated_column = gen_col;

            if rest.is_empty() {
                continue;
            }

            let (src_delta, consumed) = vlq::decode(rest).expect("decode source index");
            rest = &rest[consumed..];
            let (orig_line_delta, consumed) = vlq::decode(rest).expect("decode original line");
            rest = &rest[consumed..];
            let (orig_col_delta, consumed) = vlq::decode(rest).expect("decode original column");
            rest = &rest[consumed..];

            let source_index = prev_source_index + src_delta;
            let original_line = prev_original_line + orig_line_delta;
            let original_column = prev_original_column + orig_col_delta;

            prev_source_index = source_index;
            prev_original_line = original_line;
            prev_original_column = original_column;

            let name_index = if !rest.is_empty() {
                let (name_delta, consumed) = vlq::decode(rest).expect("decode name index");
                rest = &rest[consumed..];
                let name_index = prev_name_index + name_delta;
                prev_name_index = name_index;
                Some(name_index as u32)
            } else {
                None
            };

            assert!(
                rest.is_empty(),
                "unexpected trailing data in mappings segment: {segment}"
            );

            decoded.push(DecodedMapping {
                generated_line,
                generated_column: gen_col as u32,
                source_index: source_index as u32,
                original_line: original_line as u32,
                original_column: original_column as u32,
                name_index,
            });
        }

        generated_line += 1;
        prev_generated_column = 0;
    }

    decoded
}

pub fn find_line_col(text: &str, needle: &str) -> (u32, u32) {
    let idx = text
        .find(needle)
        .unwrap_or_else(|| panic!("expected to find {needle} in {text}"));

    let mut line = 0u32;
    let mut col = 0u32;
    for &b in text.as_bytes().iter().take(idx) {
        if b == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

pub fn has_mapping_for_prefixes(
    decoded: &[DecodedMapping],
    output: &str,
    source: &str,
    needle: &str,
    prefixes: &[&str],
) -> bool {
    let (target_line, target_col) = find_line_col(source, needle);
    let needle_len = needle.len() as u32;
    let lower_bound = target_col.saturating_sub(6);
    let upper_bound = target_col + needle_len;

    for entry in decoded.iter() {
        if entry.source_index != 0 {
            continue;
        }
        if entry.original_line != target_line {
            continue;
        }
        if entry.original_column < lower_bound || entry.original_column > upper_bound {
            continue;
        }

        let output_line_text = match output.lines().nth(entry.generated_line as usize) {
            Some(line) => line,
            None => continue,
        };
        let output_slice = match output_line_text.get(entry.generated_column as usize..) {
            Some(slice) => slice,
            None => continue,
        };
        if prefixes
            .iter()
            .any(|prefix| output_slice.starts_with(prefix))
        {
            return true;
        }
    }

    false
}
