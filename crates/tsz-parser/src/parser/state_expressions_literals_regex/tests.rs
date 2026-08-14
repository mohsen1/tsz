//! Unit tests for `state_expressions_literals_regex` helpers.
//!
//! Relocated out of the parent module file to keep it under the
//! per-file line ceiling; pure move, no test-logic changes.

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
