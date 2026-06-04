use super::state::ParserState;

use crate::parser::{NodeIndex, node::LiteralData};

use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

use tsz_scanner::SyntaxKind;

use tsz_scanner::scanner_impl::TokenFlags;

/// Map a UTF-8 `start` byte offset and a (possibly surrogate-pair) `char`
/// into the UTF-16 code-unit offsets used by regex range-order analysis.
///
/// Pathological inputs whose absolute offset does not fit in `u32` would
/// otherwise panic on the inner `u32::try_from`. We drop unrepresentable
/// offsets rather than panic — range-order analysis tolerates a shorter
/// offset vector and simply skips the affected atoms. See issue #4787.
fn split_non_unicode_atom_offsets(start: usize, ch: char) -> Vec<u32> {
    let utf16_len = ch.len_utf16();
    let utf8_len = ch.len_utf8();
    ch.encode_utf16(&mut [0; 2])
        .iter()
        .enumerate()
        .filter_map(|(i, _)| u32::try_from(start + (i * utf8_len) / utf16_len).ok())
        .collect()
}

include!("state_expressions_literals_regex_parts/part1.rs");
include!("state_expressions_literals_regex_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::*;

    // Regression for issue #4787: the regex range-order helper used
    // `u32::try_from(start).expect(...)` for absolute UTF-8 offsets, so a
    // sufficiently large absolute offset (in pathological / crafted input)
    // would panic the parser instead of degrading gracefully. After the
    // fix, oversized offsets simply produce an empty offset vector — the
    // range-order pass loses precision for those atoms but the parser
    // does not crash.
    #[test]
    fn split_non_unicode_atom_offsets_returns_empty_vec_when_offset_overflows_u32() {
        // start near usize::MAX guarantees `start + ...` cannot fit in u32
        // on 64-bit platforms.
        let offsets = split_non_unicode_atom_offsets(usize::MAX, 'a');
        assert!(
            offsets.is_empty(),
            "expected empty offset vec on u32 overflow, got {offsets:?}",
        );
    }

    #[test]
    fn split_non_unicode_atom_offsets_returns_offsets_for_bmp_chars() {
        // BMP char: one UTF-16 code unit, one UTF-8 byte. Offsets should
        // round-trip the start position unchanged.
        let offsets = split_non_unicode_atom_offsets(7, 'a');
        assert_eq!(offsets, vec![7]);
    }

    #[test]
    fn split_non_unicode_atom_offsets_returns_two_offsets_for_surrogate_pair() {
        // U+1F600 (😀) encodes to two UTF-16 code units and four UTF-8
        // bytes; the helper should yield two distinct offsets that both
        // fit in u32 for normal inputs.
        let offsets = split_non_unicode_atom_offsets(0, '\u{1F600}');
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], 0);
        // Second surrogate's offset = (1 * 4) / 2 = 2.
        assert_eq!(offsets[1], 2);
    }

    #[test]
    fn split_non_unicode_atom_offsets_drops_only_overflowing_entries() {
        // Pick a `start` such that the FIRST surrogate fits in u32 but the
        // SECOND does not. With a surrogate-pair char the second offset is
        // `start + 2`, so a start of `u32::MAX as usize - 1` makes the
        // first offset = u32::MAX - 1 (fits) and the second = u32::MAX + 1
        // (overflows). filter_map drops only the overflowing entry.
        let start = u32::MAX as usize - 1;
        let offsets = split_non_unicode_atom_offsets(start, '\u{1F600}');
        assert_eq!(
            offsets,
            vec![u32::MAX - 1],
            "first surrogate kept, second dropped",
        );
    }
}
