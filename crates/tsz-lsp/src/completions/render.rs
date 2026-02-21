//! Completion rendering/sorting helpers.
//!
//! This module owns deterministic UI ordering utilities used by completion item
//! finalization. Data collection remains in `completions.rs`.

/// Compare two strings using a case-sensitive UI sort order that matches
/// TypeScript's `compareStringsCaseSensitiveUI` (Intl.Collator with
/// sensitivity: "variant", numeric: true). Uses multi-pass comparison like
/// the Unicode Collation Algorithm: primary pass resolves case-insensitive
/// differences (with numeric segments compared as numbers), then tertiary
/// pass resolves case (lowercase before uppercase).
pub(super) fn compare_case_sensitive_ui(a: &str, b: &str) -> std::cmp::Ordering {
    // Split strings into segments of digits/non-digits for numeric comparison
    fn split_numeric_segments(s: &str) -> Vec<&str> {
        let mut segments = Vec::new();
        let mut start = 0;
        let mut in_digit = false;

        for (i, ch) in s.char_indices() {
            let is_digit = ch.is_ascii_digit();
            if i == 0 {
                in_digit = is_digit;
            } else if is_digit != in_digit {
                segments.push(&s[start..i]);
                start = i;
                in_digit = is_digit;
            }
        }
        if start < s.len() {
            segments.push(&s[start..]);
        }
        segments
    }

    // Primary pass: case-insensitive + numeric
    let a_segments = split_numeric_segments(a);
    let b_segments = split_numeric_segments(b);

    for (a_seg, b_seg) in a_segments.iter().zip(b_segments.iter()) {
        let a_is_digit = a_seg.chars().next().is_some_and(|c| c.is_ascii_digit());
        let b_is_digit = b_seg.chars().next().is_some_and(|c| c.is_ascii_digit());

        let cmp = if a_is_digit && b_is_digit {
            // Numeric comparison
            let a_num = a_seg.parse::<u64>().unwrap_or(0);
            let b_num = b_seg.parse::<u64>().unwrap_or(0);
            a_num.cmp(&b_num)
        } else {
            // Case-insensitive lexical comparison
            a_seg.to_lowercase().cmp(&b_seg.to_lowercase())
        };

        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
    }

    // Compare segment count if all common segments equal
    let seg_cmp = a_segments.len().cmp(&b_segments.len());
    if seg_cmp != std::cmp::Ordering::Equal {
        return seg_cmp;
    }

    // Tertiary pass: case-sensitive (lowercase before uppercase)
    for (a_ch, b_ch) in a.chars().zip(b.chars()) {
        if a_ch == b_ch {
            continue;
        }

        let a_lower = a_ch.to_lowercase().next().unwrap_or(a_ch);
        let b_lower = b_ch.to_lowercase().next().unwrap_or(b_ch);

        if a_lower == b_lower {
            // Same letter, different case: lowercase comes first
            if a_ch.is_lowercase() && b_ch.is_uppercase() {
                return std::cmp::Ordering::Less;
            }
            if a_ch.is_uppercase() && b_ch.is_lowercase() {
                return std::cmp::Ordering::Greater;
            }
        }
    }

    // Fallback to direct comparison
    a.cmp(b)
}
